//! How one JSON-RPC request reaches a server and its answer comes back.
//!
//! Two transports behind one synchronous call, [`Transport::request`], because
//! every caller is synchronous: a tool executor runs on the turn's thread, and
//! the `/mcp` commands run on the runtime's own worker thread. A call waits for
//! its answer up to a deadline and never longer, and a turn's cancellation ends
//! the wait at once.
//!
//! - **stdio** (`vercel-labs/fx@c1db9191 src/core/mcp/stdio_dispatcher.zig`):
//!   a child in a process group of its own, newline-delimited JSON on its
//!   standard streams, one reader thread routing each answer to the caller
//!   waiting on its id. A request the server sends back is answered -- `ping`
//!   with an empty result, everything else with `Method not found` -- so a
//!   server that asks never hangs on xfx. Stopped by its own group, never by
//!   name: the end of standard input first, then `SIGTERM`, then `SIGKILL`.
//! - **streamable HTTP** without OAuth (`legacy_streamable_http.zig`): one
//!   `POST` per message, answered as JSON or as an event stream, the session
//!   id the server hands out sent back on every later request. It runs on a
//!   thread of its own with a runtime of its own, because the caller may be
//!   inside the TUI worker's runtime, where blocking on another future would
//!   panic.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::McpError;
use crate::gateway::CancelToken;

/// The largest frame one answer may be (`mcp_discovery_response_frame_cap_bytes`).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// How much of a server's standard error is kept, for the failure it explains.
const STDERR_TAIL_BYTES: usize = 4 * 1024;

/// How long a gone server's standard error is waited for, for that tail.
const STDERR_SETTLE: Duration = Duration::from_millis(500);

/// How often a waiting call looks at the turn's cancellation.
const CANCEL_POLL: Duration = Duration::from_millis(20);

/// How long a stopping server is given after `SIGTERM` before `SIGKILL`.
const STOP_GRACE: Duration = Duration::from_millis(500);

type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<Result<Value, McpError>>>>>;

/// A notification the server sent, for whoever is watching it.
pub type NotificationSink = Arc<dyn Fn(&str) + Send + Sync>;

/// One connection to one server.
pub trait Transport: Send + Sync {
    /// Sends `method` with `params` and waits for the answer's `result`.
    fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError>;
    /// Sends a notification, which has no answer.
    fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpError>;
    /// Whether the connection can still carry a request.
    fn alive(&self) -> bool;
    /// What the server said on its own way out, when it has gone.
    fn diagnostic(&self) -> Option<String>;
    /// Ends the connection and everything it started.
    fn close(&self);
}

/// One JSON-RPC request, in upstream's key order: `jsonrpc`, `id`, `method`,
/// `params`.
pub fn request_frame(id: u64, method: &str, params: Value) -> Value {
    let mut frame = Map::new();
    frame.insert("jsonrpc".into(), Value::from("2.0"));
    frame.insert("id".into(), Value::from(id));
    frame.insert("method".into(), Value::from(method));
    frame.insert("params".into(), params);
    Value::Object(frame)
}

fn notification_frame(method: &str, params: Option<Value>) -> Value {
    let mut frame = Map::new();
    frame.insert("jsonrpc".into(), Value::from("2.0"));
    frame.insert("method".into(), Value::from(method));
    if let Some(params) = params {
        frame.insert("params".into(), params);
    }
    Value::Object(frame)
}

/// The `result` of an answer, or the server's error.
pub fn answer(frame: Value) -> Result<Value, McpError> {
    let Value::Object(mut frame) = frame else {
        return Err(McpError::Named("McpInvalidResult"));
    };
    if let Some(error) = frame.remove("error") {
        return Err(McpError::Protocol {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            data: error.get("data").cloned(),
        });
    }
    frame.remove("result").ok_or(McpError::Named("McpNoResult"))
}

/// Waits for an answer on `receiver` until `timeout`, or until `cancel`.
fn wait(
    receiver: &mpsc::Receiver<Result<Value, McpError>>,
    timeout: Duration,
    cancel: Option<&CancelToken>,
) -> Result<Value, McpError> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Err(McpError::Named("Cancelled"));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(McpError::Named("McpRequestTimedOut"));
        }
        match receiver.recv_timeout(remaining.min(CANCEL_POLL)) {
            Ok(answer) => return answer,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(McpError::Named("McpConnectionClosed"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// stdio
// ---------------------------------------------------------------------------

/// A server that is a child process.
pub struct StdioTransport {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Mutex<Option<Child>>,
    pending: Pending,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    stderr: Arc<Mutex<String>>,
    /// Set once the server's standard error reaches its end.
    stderr_closed: Arc<AtomicBool>,
    docker: Mutex<Option<super::docker::Cleanup>>,
}

impl StdioTransport {
    /// Starts `command` with `args`, its environment the inherited one with
    /// `env` added (`server_transport.zig:107-124`), in a process group of
    /// its own (`pgid = 0`, `:946`). A `docker run` gets a `--cidfile` so its
    /// container can be removed with it ([`super::docker`]).
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
        notifications: Option<NotificationSink>,
    ) -> Result<Self, McpError> {
        let mut argv: Vec<String> = Vec::with_capacity(args.len() + 1);
        argv.push(command.to_string());
        argv.extend(args.iter().cloned());
        let prepared = super::docker::prepare(&argv);
        let mut builder = crate::process::command(&prepared.argv[0]);
        builder
            .args(&prepared.argv[1..])
            .envs(env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            builder.process_group(0);
        }
        let mut child = builder
            .spawn()
            .map_err(|_| McpError::Named("McpSpawnFailed"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let transport = Self {
            stdin: Arc::new(Mutex::new(stdin)),
            child: Mutex::new(Some(child)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(0),
            alive: Arc::new(AtomicBool::new(true)),
            stderr: Arc::new(Mutex::new(String::new())),
            stderr_closed: Arc::new(AtomicBool::new(false)),
            docker: Mutex::new(prepared.cleanup),
        };
        if let Some(stderr_pipe) = stderr_pipe {
            let tail = Arc::clone(&transport.stderr);
            let closed = Arc::clone(&transport.stderr_closed);
            let _ = std::thread::Builder::new()
                .name("xfx-mcp-stderr".into())
                .spawn(move || {
                    keep_tail(stderr_pipe, &tail);
                    closed.store(true, Ordering::Release);
                });
        }
        if let Some(stdout) = stdout {
            let reader = Reader {
                pending: Arc::clone(&transport.pending),
                alive: Arc::clone(&transport.alive),
                stdin: Arc::clone(&transport.stdin),
                notifications,
            };
            std::thread::Builder::new()
                .name("xfx-mcp-stdio".into())
                .spawn(move || reader.run(stdout))
                .map_err(|_| McpError::Named("McpSpawnFailed"))?;
        }
        Ok(transport)
    }

    fn write(&self, frame: &Value) -> Result<(), McpError> {
        write_line(&self.stdin, frame)
    }
}

fn write_line(stdin: &Mutex<Option<ChildStdin>>, frame: &Value) -> Result<(), McpError> {
    let mut line = frame.to_string();
    line.push('\n');
    let mut guard = stdin.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(stdin) = guard.as_mut() else {
        return Err(McpError::Named("McpConnectionClosed"));
    };
    stdin
        .write_all(line.as_bytes())
        .and_then(|()| stdin.flush())
        .map_err(|_| McpError::Named("McpConnectionClosed"))
}

/// Keeps the last [`STDERR_TAIL_BYTES`] of what the server wrote to its
/// standard error.
fn keep_tail(mut pipe: impl Read, tail: &Mutex<String>) {
    let mut buffer = [0u8; 4096];
    while let Ok(read) = pipe.read(&mut buffer) {
        if read == 0 {
            break;
        }
        let mut kept = tail.lock().unwrap_or_else(PoisonError::into_inner);
        kept.push_str(&String::from_utf8_lossy(&buffer[..read]));
        if kept.len() > STDERR_TAIL_BYTES {
            let mut cut = kept.len() - STDERR_TAIL_BYTES;
            while !kept.is_char_boundary(cut) {
                cut += 1;
            }
            kept.drain(..cut);
        }
    }
}

/// The stdio reader: every line the server writes, routed.
struct Reader {
    pending: Pending,
    alive: Arc<AtomicBool>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    notifications: Option<NotificationSink>,
}

impl Reader {
    fn run(self, stdout: impl Read) {
        let mut lines = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match lines
                .by_ref()
                .take(MAX_FRAME_BYTES as u64 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line.len() > MAX_FRAME_BYTES {
                break;
            }
            let text = String::from_utf8_lossy(&line);
            let text = text.trim_end_matches(['\r', '\n']);
            if text.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(text) {
                Ok(frame) => self.route(frame),
                // A line that is not JSON is the server breaking the protocol
                // (fx's `startup_garbage`): every caller waiting is told.
                Err(_) => self.fail_all("McpInvalidJson"),
            }
        }
        self.alive.store(false, Ordering::Release);
        self.fail_all("McpConnectionClosed");
    }

    fn route(&self, frame: Value) {
        let method = frame
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        match (method, frame.get("id").cloned()) {
            (Some(method), Some(id)) => {
                // A request from the server: answered, never left hanging.
                let reply = if method == "ping" {
                    json!({"jsonrpc": "2.0", "id": id, "result": {}})
                } else {
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})
                };
                let _ = write_line(&self.stdin, &reply);
            }
            (Some(method), None) => {
                if let Some(sink) = &self.notifications {
                    sink(&method);
                }
            }
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else {
                    return;
                };
                let waiter = self
                    .pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
                if let Some(waiter) = waiter {
                    let _ = waiter.send(answer(frame));
                }
            }
            (None, None) => {}
        }
    }

    fn fail_all(&self, name: &'static str) {
        let waiters: Vec<_> = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .collect();
        for (_, waiter) in waiters {
            let _ = waiter.send(Err(McpError::Named(name)));
        }
    }
}

impl Transport for StdioTransport {
    fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError> {
        if !self.alive() {
            return Err(McpError::Named("McpConnectionClosed"));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let params = with_progress_token(method, params, id);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, sender);
        if let Err(err) = self.write(&request_frame(id, method, params)) {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            return Err(err);
        }
        let result = wait(&receiver, timeout, cancel);
        if result.is_err() {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            // A call the caller stopped waiting for is cancelled on the wire
            // too, so the server can stop the work.
            if matches!(
                result,
                Err(McpError::Named("Cancelled" | "McpRequestTimedOut"))
            ) {
                let _ = self.notify(
                    "notifications/cancelled",
                    Some(json!({"requestId": id, "reason": "cancelled"})),
                );
            }
        }
        result
    }

    fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpError> {
        self.write(&notification_frame(method, params))
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn diagnostic(&self) -> Option<String> {
        // A server that has gone may still have its last words in the pipe:
        // its output ends before its standard error is read to the end, so
        // the tail is waited for, briefly.
        if !self.alive() {
            let deadline = Instant::now() + STDERR_SETTLE;
            while !self.stderr_closed.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let tail = self.stderr.lock().unwrap_or_else(PoisonError::into_inner);
        let tail = tail.trim();
        (!tail.is_empty()).then(|| tail.to_string())
    }

    fn close(&self) {
        // End of input first: a well-behaved server leaves on it.
        self.stdin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let child = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(mut child) = child {
            stop_child(&mut child);
        }
        if let Some(cleanup) = self
            .docker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            cleanup.run();
        }
        self.alive.store(false, Ordering::Release);
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        self.close();
    }
}

/// Stops `child` and its process group: `SIGTERM`, a short grace, `SIGKILL`,
/// and the child reaped. Only this child's own group is signalled.
fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    let group = rustix::process::Pid::from_raw(child.id() as i32);
    let started = Instant::now();
    if let Ok(Some(_)) = child.try_wait() {
        #[cfg(unix)]
        if let Some(group) = group {
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
        return;
    }
    #[cfg(unix)]
    if let Some(group) = group {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
    }
    while started.elapsed() < STOP_GRACE {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    #[cfg(unix)]
    if let Some(group) = group {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `tools/call` carries a progress token, the request's own id, first in its
/// params, as upstream's does (`{"_meta":{"progressToken":2},"name":...}`;
/// the legacy fixture refuses a call without one).
fn with_progress_token(method: &str, params: Value, id: u64) -> Value {
    if method != "tools/call" {
        return params;
    }
    let Value::Object(params) = params else {
        return params;
    };
    let mut ordered = Map::new();
    ordered.insert("_meta".into(), json!({"progressToken": id}));
    for (key, value) in params {
        if key != "_meta" {
            ordered.insert(key, value);
        }
    }
    Value::Object(ordered)
}

// ---------------------------------------------------------------------------
// streamable HTTP
// ---------------------------------------------------------------------------

/// One message for the HTTP thread to send, and where its answer goes.
struct HttpJob {
    body: Value,
    /// The id the answer is for, or `None` for a notification.
    id: Option<u64>,
    reply: mpsc::Sender<Result<Value, McpError>>,
}

/// A server reached over streamable HTTP.
pub struct HttpTransport {
    jobs: Mutex<Option<mpsc::Sender<HttpJob>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    failure: Arc<Mutex<Option<String>>>,
}

/// What every request carries besides its body.
struct HttpTarget {
    url: String,
    headers: Vec<(String, String)>,
    protocol_version: Arc<Mutex<Option<String>>>,
    session_id: Arc<Mutex<Option<String>>>,
}

impl HttpTransport {
    /// A connection to `url`. Nothing is sent until the first request.
    pub fn open(url: &str, headers: &[(String, String)]) -> Result<Self, McpError> {
        let (jobs, receiver) = mpsc::channel::<HttpJob>();
        let alive = Arc::new(AtomicBool::new(true));
        let failure = Arc::new(Mutex::new(None));
        let target = HttpTarget {
            url: url.to_string(),
            headers: headers.to_vec(),
            protocol_version: Arc::new(Mutex::new(None)),
            session_id: Arc::new(Mutex::new(None)),
        };
        let thread_alive = Arc::clone(&alive);
        std::thread::Builder::new()
            .name("xfx-mcp-http".into())
            .spawn(move || http_loop(target, receiver, thread_alive))
            .map_err(|_| McpError::Named("McpSpawnFailed"))?;
        Ok(Self {
            jobs: Mutex::new(Some(jobs)),
            next_id: AtomicU64::new(0),
            alive,
            failure,
        })
    }

    fn send(&self, job: HttpJob) -> Result<(), McpError> {
        let guard = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(jobs) = guard.as_ref() else {
            return Err(McpError::Named("McpConnectionClosed"));
        };
        jobs.send(job)
            .map_err(|_| McpError::Named("McpConnectionClosed"))
    }
}

impl Transport for HttpTransport {
    fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: Option<&CancelToken>,
    ) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let params = with_progress_token(method, params, id);
        let (reply, receiver) = mpsc::channel();
        self.send(HttpJob {
            body: request_frame(id, method, params),
            id: Some(id),
            reply,
        })?;
        let result = wait(&receiver, timeout, cancel);
        if let Err(McpError::Named(name)) = &result {
            if matches!(*name, "McpAuthenticationRequired" | "McpSessionExpired") {
                *self.failure.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some((*name).to_string());
            }
        }
        result
    }

    fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpError> {
        let (reply, receiver) = mpsc::channel();
        self.send(HttpJob {
            body: notification_frame(method, params.or_else(|| Some(json!({})))),
            id: None,
            reply,
        })?;
        wait(&receiver, Duration::from_secs(10), None).map(|_| ())
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn diagnostic(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn close(&self) {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        self.alive.store(false, Ordering::Release);
    }
}

impl Drop for HttpTransport {
    fn drop(&mut self) {
        self.close();
    }
}

/// The HTTP thread: one runtime, one client, one message at a time.
fn http_loop(target: HttpTarget, jobs: mpsc::Receiver<HttpJob>, alive: Arc<AtomicBool>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        alive.store(false, Ordering::Release);
        return;
    };
    // No redirects: an endpoint that moves is a different server than the
    // one the person configured (`RedirectNotAllowed`).
    let Ok(client) = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        alive.store(false, Ordering::Release);
        return;
    };
    while let Ok(job) = jobs.recv() {
        let answer = runtime.block_on(post(&client, &target, &job));
        let _ = job.reply.send(answer);
    }
    // Upstream ends a session it opened with a `DELETE` (`deleteCore`); a
    // server that does not know it answers 405, which is fine.
    let session = target
        .session_id
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(session) = session {
        let mut request = client
            .delete(&target.url)
            .header("MCP-Session-Id", session)
            .timeout(Duration::from_secs(2));
        for (name, value) in &target.headers {
            request = request.header(name, value);
        }
        // Built inside the runtime: the request's deadline is a timer, and a
        // timer made outside one has no reactor to fire it.
        let _ = runtime.block_on(async move { request.send().await });
    }
    alive.store(false, Ordering::Release);
}

/// One message, sent and answered (`runBounded`, `legacy_streamable_http.zig:
/// 640-760`, and `appendHeaders`, `:870-905`).
async fn post(
    client: &reqwest::Client,
    target: &HttpTarget,
    job: &HttpJob,
) -> Result<Value, McpError> {
    let mut request = client
        .post(&target.url)
        .header("Content-Type", "application/json")
        .header(
            "Accept",
            if job.id.is_some() {
                "application/json, text/event-stream"
            } else {
                "text/event-stream"
            },
        );
    let version = target
        .protocol_version
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    // `2025-03-26` sends no version header (`sendsProtocolHeader`, `:27-29`).
    if let Some(version) = version.filter(|version| version != "2025-03-26") {
        request = request.header("MCP-Protocol-Version", version);
    }
    let session = target
        .session_id
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(session) = &session {
        request = request.header("MCP-Session-Id", session);
    }
    for (name, value) in &target.headers {
        request = request.header(name, value);
    }
    let response = request
        .body(job.body.to_string())
        .send()
        .await
        .map_err(|_| McpError::Named("McpConnectionFailed"))?;
    let status = response.status();
    if status.is_redirection() {
        return Err(McpError::Named("RedirectNotAllowed"));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED
        || (status == reqwest::StatusCode::FORBIDDEN
            && response
                .headers()
                .contains_key(reqwest::header::WWW_AUTHENTICATE))
    {
        return Err(McpError::Named("McpAuthenticationRequired"));
    }
    let Some(id) = job.id else {
        return if status == reqwest::StatusCode::ACCEPTED || status.is_success() {
            Ok(Value::Null)
        } else {
            Err(McpError::Named("UnexpectedHttpStatus"))
        };
    };
    if status == reqwest::StatusCode::NOT_FOUND && session.is_some() {
        return Err(McpError::Named("McpSessionExpired"));
    }
    if status != reqwest::StatusCode::OK {
        return Err(McpError::Named("UnexpectedHttpStatus"));
    }
    if let Some(value) = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        if !value.is_empty()
            && value.len() <= 1024
            && value.bytes().all(|b| (0x21..=0x7e).contains(&b))
        {
            *target
                .session_id
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(value.to_string());
        } else {
            return Err(McpError::Named("InvalidMcpSessionId"));
        }
    }
    let media = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        })
        .ok_or(McpError::Named("MissingContentType"))?;
    let body = read_bounded(response).await?;
    let frame = match media.as_str() {
        "application/json" => {
            serde_json::from_slice::<Value>(&body).map_err(|_| McpError::Named("McpInvalidJson"))?
        }
        "text/event-stream" => sse_answer(&body, id)?,
        _ => return Err(McpError::Named("UnsupportedContentType")),
    };
    if frame.get("id").and_then(Value::as_u64) != Some(id) {
        return Err(McpError::Named("McpResponseIdMismatch"));
    }
    // The negotiated version, once the initialize answer names it, goes on
    // every later request.
    if job.body.get("method").and_then(Value::as_str) == Some("initialize") {
        if let Some(version) = frame
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str)
        {
            *target
                .protocol_version
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(version.to_string());
        }
    }
    answer(frame)
}

async fn read_bounded(response: reqwest::Response) -> Result<Vec<u8>, McpError> {
    use futures_util::StreamExt;
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| McpError::Named("McpConnectionClosed"))?;
        if body.len() + chunk.len() > MAX_FRAME_BYTES {
            return Err(McpError::Named("ResponseTooLarge"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The answer to request `id` among the events of a stream: each event's
/// `data` lines joined, and the first one that is a JSON-RPC answer for that
/// id taken. A notification or a server request on the stream is passed over.
fn sse_answer(body: &[u8], id: u64) -> Result<Value, McpError> {
    let text = String::from_utf8_lossy(body);
    let mut data = String::new();
    let take = |data: &mut String| -> Option<Value> {
        if data.is_empty() {
            return None;
        }
        let frame = serde_json::from_str::<Value>(data).ok();
        data.clear();
        let frame = frame?;
        let is_answer =
            frame.get("method").is_none() && frame.get("id").and_then(Value::as_u64) == Some(id);
        is_answer.then_some(frame)
    };
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            if let Some(frame) = take(&mut data) {
                return Ok(frame);
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    take(&mut data).ok_or(McpError::Named("McpNoResult"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_frame_is_in_upstreams_key_order() {
        assert_eq!(
            request_frame(1, "tools/list", json!({})).to_string(),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#
        );
        assert_eq!(
            notification_frame("notifications/initialized", None).to_string(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        );
    }

    #[test]
    fn a_tool_call_leads_with_its_progress_token() {
        let params = with_progress_token(
            "tools/call",
            json!({"name": "echo", "arguments": {"text": "hi"}}),
            2,
        );
        assert_eq!(
            params.to_string(),
            r#"{"_meta":{"progressToken":2},"name":"echo","arguments":{"text":"hi"}}"#
        );
        assert_eq!(with_progress_token("tools/list", json!({}), 2), json!({}));
    }

    #[test]
    fn an_answer_is_its_result_or_the_servers_error() {
        assert_eq!(
            answer(json!({"id": 1, "result": {"a": 1}})),
            Ok(json!({"a": 1}))
        );
        assert_eq!(
            answer(json!({"id": 1, "error": {"code": -32601, "message": "Method not found"}})),
            Err(McpError::Protocol {
                code: -32601,
                message: "Method not found".into(),
                data: None
            })
        );
        assert_eq!(
            answer(json!({"id": 1})),
            Err(McpError::Named("McpNoResult"))
        );
    }

    #[test]
    fn a_stream_answer_skips_notifications_and_other_ids() {
        let body = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{}}\n\ndata: {\"jsonrpc\":\"2.0\",\n\
data: \"id\":4,\"result\":{\"ok\":true}}\n\n";
        assert_eq!(
            sse_answer(body, 4).unwrap(),
            json!({"jsonrpc": "2.0", "id": 4, "result": {"ok": true}})
        );
        assert_eq!(
            sse_answer(b"data: {}\n\n", 4),
            Err(McpError::Named("McpNoResult"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_stdio_server_is_answered_and_stopped_by_its_own_group() {
        // A server written for the test: it answers one request with an id it
        // reads back, and leaves a grandchild in its group, which stopping the
        // connection has to take too.
        let script = r#"sleep 30 & echo $! > "$1"; while IFS= read -r line; do id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p'); printf '{"jsonrpc":"2.0","id":%s,"result":{"echo":true}}\n' "$id"; done"#;
        let dir = tempfile::tempdir().expect("dir");
        let pid_file = dir.path().join("grandchild");
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                script.into(),
                "sh".into(),
                pid_file.to_string_lossy().into_owned(),
            ],
            &[],
            None,
        )
        .expect("spawned");
        let result = transport
            .request("ping", json!({}), Duration::from_secs(60), None)
            .expect("answered");
        assert_eq!(result, json!({"echo": true}));
        let deadline = Instant::now() + Duration::from_secs(60);
        let grandchild: i32 = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "the grandchild never started");
            std::thread::sleep(Duration::from_millis(20));
        };
        transport.close();
        assert!(!transport.alive());
        let pid = rustix::process::Pid::from_raw(grandchild).expect("pid");
        let deadline = Instant::now() + Duration::from_secs(60);
        while rustix::process::test_kill_process(pid).is_ok() {
            assert!(
                Instant::now() < deadline,
                "the grandchild outlived its server"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            transport.request("ping", json!({}), Duration::from_secs(1), None),
            Err(McpError::Named("McpConnectionClosed"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_request_the_server_never_answers_times_out_and_a_cancelled_one_stops_at_once() {
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &["-c".into(), "cat > /dev/null".into()],
            &[],
            None,
        )
        .expect("spawned");
        assert_eq!(
            transport.request("x", json!({}), Duration::from_millis(100), None),
            Err(McpError::Named("McpRequestTimedOut"))
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            transport.request("x", json!({}), Duration::from_secs(60), Some(&cancel)),
            Err(McpError::Named("Cancelled"))
        );
    }
}
