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
//!   server that asks never hangs on xfx. No write waits past its deadline,
//!   and none holds a lock while it waits, so a server that stops reading
//!   its input holds up neither a call nor its own stop. Stopped by its own
//!   group, never by name: the end of standard input first, then `SIGTERM`,
//!   then `SIGKILL`.
//! - **streamable HTTP** without OAuth (`legacy_streamable_http.zig`): one
//!   `POST` per message, answered as JSON or as an event stream, the session
//!   id the server hands out sent back on every later request. It runs on a
//!   thread of its own with a runtime of its own, because the caller may be
//!   inside the TUI worker's runtime, where blocking on another future would
//!   panic.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
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

/// How long a frame no answer comes back for -- a notification, or the answer
/// to a request the server sent -- may wait for the server to take it: fx's
/// bound for such an answer (`server_request_write_timeout_ms`,
/// `stdio_dispatcher.zig:19`, `:1651`). fx gives `notifications/initialized`
/// its startup timeout instead (`server_transport.zig:900-906`); a frame that
/// small, to a server that has just read `initialize`, does not wait.
const FRAME_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// How long a call's cancellation may wait for the server to take it
/// (`cancellation_write_timeout_ms`, `stdio_dispatcher.zig:18`).
const CANCEL_WRITE_TIMEOUT: Duration = Duration::from_millis(100);

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
    /// How a server that is a process ended, once it has: asked beside
    /// [`Self::diagnostic`], before [`Self::close`] ends it itself.
    fn exit(&self) -> Option<Exit> {
        None
    }
    /// Ends the connection and everything it started.
    fn close(&self);
}

/// How a server's process ended: its exit code, or the signal that ended it
/// (`Exit`, `vercel-labs/fx@481104fd src/mcp/io/process.zig:165`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
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
    wait_until(receiver, Instant::now() + timeout, cancel)
}

/// Waits for an answer on `receiver` until `deadline`, or until `cancel`.
fn wait_until(
    receiver: &mpsc::Receiver<Result<Value, McpError>>,
    deadline: Instant,
    cancel: Option<&CancelToken>,
) -> Result<Value, McpError> {
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
    stdin: Arc<Input>,
    /// Uncollected while it is here: only [`Transport::close`] takes it out,
    /// to collect it. So its id is still its group's.
    child: Arc<Mutex<Option<Child>>>,
    pending: Pending,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    stderr: Arc<Mutex<String>>,
    /// Set once the server's standard error reaches its end.
    stderr_closed: Arc<AtomicBool>,
    docker: Mutex<Option<super::docker::Cleanup>>,
}

/// The variables of xfx's own environment a stdio server is given, as the MCP
/// SDKs give them (`inherited_names`, `vercel-labs/fx@481104fd
/// src/core/mcp_host/config.zig:200-202`, since v0.0.15).
const INHERITED_NAMES: [&str; 6] = ["HOME", "LOGNAME", "PATH", "SHELL", "TERM", "USER"];

/// The whole environment a stdio server starts with: those of
/// [`INHERITED_NAMES`] that `inherited` holds, then the server's own `env` --
/// what the person configured, so it wins, and so a credential it names is a
/// grant (`transportFor`, `config.zig:219-225`). Nothing else of xfx's: not
/// its credential, not a token or proxy setting a person exported for
/// something else. A value that is an exported shell function (`() {...`) is
/// not passed on, as upstream's is not.
pub fn server_environment(
    inherited: impl Fn(&str) -> Option<OsString>,
    env: &[(String, String)],
) -> BTreeMap<OsString, OsString> {
    let mut environment = BTreeMap::new();
    for name in INHERITED_NAMES {
        if let Some(value) = inherited(name) {
            if !value.as_encoded_bytes().starts_with(b"()") {
                environment.insert(OsString::from(name), value);
            }
        }
    }
    for (name, value) in env {
        environment.insert(OsString::from(name), OsString::from(value));
    }
    environment
}

impl StdioTransport {
    /// Starts `command` with `args`, its environment [`server_environment`]'s
    /// and nothing inherited, in a process group of its own (`pgid = 0`,
    /// `vercel-labs/fx@c1db9191 src/core/mcp/server_transport.zig:945`). A
    /// `docker run` gets a `--cidfile` so its container can be removed with
    /// it, by a `docker` given the same environment ([`super::docker`]).
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
        notifications: Option<NotificationSink>,
    ) -> Result<Self, McpError> {
        let mut argv: Vec<String> = Vec::with_capacity(args.len() + 1);
        argv.push(command.to_string());
        argv.extend(args.iter().cloned());
        let environment = server_environment(|name| std::env::var_os(name), env);
        let prepared = super::docker::prepare(&argv, &environment);
        let mut builder = crate::process::command(&prepared.argv[0]);
        builder
            .args(&prepared.argv[1..])
            .env_clear()
            .envs(&environment)
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
            stdin: Arc::new(Input::new(stdin)),
            child: Arc::new(Mutex::new(Some(child))),
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
                child: Arc::clone(&transport.child),
                notifications,
            };
            std::thread::Builder::new()
                .name("xfx-mcp-stdio".into())
                .spawn(move || reader.run(stdout))
                .map_err(|_| McpError::Named("McpSpawnFailed"))?;
        }
        Ok(transport)
    }

    /// Writes `frame` by `deadline`, ending the connection when the server
    /// would not take it or failed it.
    fn write(
        &self,
        frame: &Value,
        deadline: Instant,
        cancel: Option<&CancelToken>,
    ) -> Result<(), McpError> {
        self.stdin
            .send(frame, deadline, cancel)
            .map_err(|unsent| match unsent {
                Unsent::Held(err) => err,
                Unsent::Cut(err) => {
                    abandon(&self.child, &self.alive, &self.pending);
                    err
                }
                Unsent::Broken => {
                    abandon(&self.child, &self.alive, &self.pending);
                    McpError::Named("McpConnectionClosed")
                }
                Unsent::Closed => McpError::Named("McpConnectionClosed"),
            })
    }
}

/// The server's standard input, one frame at a time, and no lock held while
/// one is written: a writer takes the pipe out, writes to it without blocking
/// -- waiting for room in slices of [`CANCEL_POLL`], each time looking at its
/// deadline, its cancellation and [`Input::close`] -- and puts it back. A
/// server that stops reading holds up the frame being written until its
/// deadline, and the frames behind it until theirs; never the close. fx
/// races the write against the deadline and the cancellation instead, and its
/// close kills the child when a writer has the pipe (`writeBodyBounded`,
/// `closeStdin`, `stdio_dispatcher.zig:754-879`, `:1702-1717`).
struct Input {
    slot: Mutex<Slot>,
    /// Told whenever the pipe is put back, and at the close.
    free: Condvar,
    closed: AtomicBool,
}

struct Slot {
    pipe: Option<ChildStdin>,
    /// Whether a writer has the pipe out.
    taken: bool,
}

/// Why a frame was not written.
enum Unsent {
    /// Its deadline or its cancellation came before any of it was written:
    /// none of it went, and the connection is as it was.
    Held(McpError),
    /// Its deadline or its cancellation came while it was being written --
    /// after part of it went, or after the server refused it: what the server
    /// would read next may be half a frame. fx ends the connection here too
    /// (`taint_connection`, `stdio_dispatcher.zig:668`, `:743-748`).
    Cut(McpError),
    /// The input was closed by xfx -- by [`Input::close`] or by a frame cut
    /// before -- and whatever ends the connection is already under way.
    Closed,
    /// The server failed the write: it closed its input, or it has gone.
    /// Nothing more can be said to it, so the connection ends and its group
    /// is killed, as fx kills a child it can no longer write to
    /// (`stdio_dispatcher.zig:585-594`, `:743-751`).
    Broken,
}

impl Input {
    fn new(pipe: Option<ChildStdin>) -> Self {
        // A pipe that cannot be written without blocking is not written at
        // all: a write to it could wait past every deadline.
        let pipe = pipe.filter(|pipe| nonblocking(pipe).is_ok());
        Self {
            slot: Mutex::new(Slot { pipe, taken: false }),
            free: Condvar::new(),
            closed: AtomicBool::new(false),
        }
    }

    fn send(
        &self,
        frame: &Value,
        deadline: Instant,
        cancel: Option<&CancelToken>,
    ) -> Result<(), Unsent> {
        let mut line = frame.to_string();
        line.push('\n');
        let mut pipe = self.take(deadline, cancel)?;
        let sent = self.push(&mut pipe, line.as_bytes(), deadline, cancel, Instant::now);
        let intact = matches!(sent, Ok(()) | Err(Unsent::Held(_)));
        self.put_back(intact.then_some(pipe));
        sent
    }

    /// The pipe, once no other writer has it -- and never for a frame whose
    /// deadline has passed or whose call was cancelled, even when it is free.
    fn take(&self, deadline: Instant, cancel: Option<&CancelToken>) -> Result<ChildStdin, Unsent> {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err(Unsent::Closed);
            }
            if let Some(err) = stopped(Instant::now(), deadline, cancel) {
                return Err(Unsent::Held(err));
            }
            if !slot.taken {
                let pipe = slot.pipe.take().ok_or(Unsent::Closed)?;
                slot.taken = true;
                return Ok(pipe);
            }
            let slice = deadline
                .saturating_duration_since(Instant::now())
                .min(CANCEL_POLL);
            slot = self
                .free
                .wait_timeout(slot, slice)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Writes `line`, looking at the close, the cancellation and the deadline
    /// before every attempt: a frame stopped before any of it went, and before
    /// the server refused any of it, leaves the connection as it was; one
    /// stopped after is [`Unsent::Cut`]. fx looks at the cancellation before a
    /// request's first byte (`stdio_dispatcher.zig:549-551`) and at both once
    /// its write has ended (`:814-826`), racing the deadline in between.
    ///
    /// `now` is the clock the deadline is read against: `Instant::now`, or a
    /// test's, which can pass the deadline exactly between two pieces.
    fn push(
        &self,
        pipe: &mut impl Sink,
        line: &[u8],
        deadline: Instant,
        cancel: Option<&CancelToken>,
        now: impl Fn() -> Instant,
    ) -> Result<(), Unsent> {
        let mut written = 0;
        // Whether the server has refused any of it: the write is under way,
        // fx's `.writing` (`:668`), even with nothing sent.
        let mut refused = false;
        while written < line.len() {
            if self.closed.load(Ordering::Acquire) {
                return Err(Unsent::Closed);
            }
            if let Some(err) = stopped(now(), deadline, cancel) {
                return Err(if written == 0 && !refused {
                    Unsent::Held(err)
                } else {
                    Unsent::Cut(err)
                });
            }
            match pipe.write(&line[written..]) {
                Ok(0) => return Err(Unsent::Broken),
                Ok(count) => written += count,
                Err(err) if err.kind() == ErrorKind::Interrupted => {}
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    refused = true;
                    pipe.wait_writable(
                        deadline
                            .saturating_duration_since(Instant::now())
                            .min(CANCEL_POLL),
                    );
                }
                // The server closed its input, or has gone.
                Err(_) => return Err(Unsent::Broken),
            }
        }
        Ok(())
    }

    /// Gives the pipe back for the next writer, or, when `pipe` is `None` or
    /// the input has been closed, lets it go.
    fn put_back(&self, pipe: Option<ChildStdin>) {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        slot.taken = false;
        if !self.closed.load(Ordering::Acquire) {
            slot.pipe = pipe;
        }
        drop(slot);
        self.free.notify_all();
    }

    /// The end of the server's input. A writer that has the pipe out is not
    /// waited for: it sees the close at its next look and lets the pipe go.
    fn close(&self) {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        self.closed.store(true, Ordering::Release);
        let pipe = slot.pipe.take();
        drop(slot);
        self.free.notify_all();
        drop(pipe);
    }
}

/// Where [`Input::push`] writes: the server's input, or a test's stand-in for
/// it.
trait Sink: Write {
    /// Waits up to `wait` for room, or for the reader to go.
    fn wait_writable(&self, wait: Duration);
}

impl Sink for ChildStdin {
    fn wait_writable(&self, wait: Duration) {
        writable(self, wait);
    }
}

/// Why a write stops waiting for room at `now`: its cancellation, then its
/// deadline.
fn stopped(now: Instant, deadline: Instant, cancel: Option<&CancelToken>) -> Option<McpError> {
    if cancel.is_some_and(CancelToken::is_cancelled) {
        Some(McpError::Named("Cancelled"))
    } else if now >= deadline {
        Some(McpError::Named("McpRequestTimedOut"))
    } else {
        None
    }
}

/// Makes writes to `pipe` answer at once, with what fitted. Only xfx's end:
/// the server's end of the pipe is a description of its own.
#[cfg(unix)]
fn nonblocking(pipe: &ChildStdin) -> std::io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(pipe)?;
    rustix::fs::fcntl_setfl(pipe, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}

/// Elsewhere a write blocks, and a full pipe holds the writer until the
/// server reads or is stopped. The release targets are all Unix.
#[cfg(not(unix))]
fn nonblocking(_: &ChildStdin) -> std::io::Result<()> {
    Ok(())
}

/// Waits up to `wait` for room in `pipe`, or for its reader to go.
#[cfg(unix)]
fn writable(pipe: &ChildStdin, wait: Duration) {
    let mut ready = [rustix::event::PollFd::new(
        pipe,
        rustix::event::PollFlags::OUT,
    )];
    // At most one slice, which always fits.
    let wait = rustix::event::Timespec::try_from(wait.min(CANCEL_POLL)).unwrap_or_default();
    let _ = rustix::event::poll(&mut ready, Some(&wait));
}

#[cfg(not(unix))]
fn writable(_: &ChildStdin, wait: Duration) {
    std::thread::sleep(wait);
}

/// Ends a connection whose server would not take a frame, or failed one:
/// every caller waiting on it is told, and the server is killed with its
/// group rather than left to read half a message or to run on unreachable
/// (`failConnection`, `terminateChild`, `stdio_dispatcher.zig:687-690`). It
/// is collected by [`Transport::close`], which every connection ends with;
/// until then its id is still its group's.
fn abandon(child: &Mutex<Option<Child>>, alive: &AtomicBool, pending: &Pending) {
    alive.store(false, Ordering::Release);
    fail_all(pending, "McpConnectionClosed");
    let mut child = child.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(child) = child.as_mut() {
        #[cfg(unix)]
        if let Some(group) = rustix::process::Pid::from_raw(child.id() as i32) {
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
        }
        let _ = child.kill();
    }
}

/// Tells every caller waiting on `pending` that its answer will not come.
fn fail_all(pending: &Pending, name: &'static str) {
    let waiters: Vec<_> = pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .drain()
        .collect();
    for (_, waiter) in waiters {
        let _ = waiter.send(Err(McpError::Named(name)));
    }
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
    stdin: Arc<Input>,
    child: Arc<Mutex<Option<Child>>>,
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
                // Bounded, so a server that asks and does not read cannot
                // stop this thread for longer than that.
                let deadline = Instant::now() + FRAME_WRITE_TIMEOUT;
                if let Err(Unsent::Cut(_) | Unsent::Broken) =
                    self.stdin.send(&reply, deadline, None)
                {
                    abandon(&self.child, &self.alive, &self.pending);
                }
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
        fail_all(&self.pending, name);
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
        // The deadline covers the write as well as the wait for the answer
        // (`commit_deadline`, `stdio_dispatcher.zig:553-572`): a server that
        // stops reading cannot hold a call past it.
        let deadline = Instant::now() + timeout;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let params = with_progress_token(method, params, id);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, sender);
        if let Err(err) = self.write(&request_frame(id, method, params), deadline, cancel) {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            return Err(err);
        }
        let result = wait_until(&receiver, deadline, cancel);
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
                let _ = self.write(
                    &notification_frame(
                        "notifications/cancelled",
                        Some(json!({"requestId": id, "reason": "cancelled"})),
                    ),
                    Instant::now() + CANCEL_WRITE_TIMEOUT,
                    None,
                );
            }
        }
        result
    }

    fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpError> {
        self.write(
            &notification_frame(method, params),
            Instant::now() + FRAME_WRITE_TIMEOUT,
            None,
        )
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    fn diagnostic(&self) -> Option<String> {
        // Asked only once a handshake failed. A server that has gone may
        // still have its last words in the pipe -- the failure is seen at a
        // write to its closed input or at the end of its output, either one
        // before its standard error is read to the end -- so the tail is
        // waited for, briefly.
        let deadline = Instant::now() + STDERR_SETTLE;
        while !self.stderr_closed.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let tail = self.stderr.lock().unwrap_or_else(PoisonError::into_inner);
        let tail = tail.trim();
        (!tail.is_empty()).then(|| tail.to_string())
    }

    fn exit(&self) -> Option<Exit> {
        // A server whose standard error has ended is on its way out: its exit
        // is waited for as briefly as its last words are. One still writing
        // there is looked at once.
        let deadline = Instant::now() + STDERR_SETTLE;
        loop {
            let found = self
                .child
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_mut()
                .and_then(exit_of);
            if found.is_some()
                || !self.stderr_closed.load(Ordering::Acquire)
                || Instant::now() >= deadline
            {
                return found;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn close(&self) {
        // End of input first: a well-behaved server leaves on it. A write
        // under way is not waited for (`Input::close`), so a server that
        // stopped reading is stopped all the same.
        self.stdin.close();
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

/// How `child` ended, if it has, leaving it uncollected: its id stays its
/// group's until [`stop_child`] collects it (`NOWAIT`, as [`Stopping::exited`]
/// reads it).
#[cfg(unix)]
fn exit_of(child: &mut Child) -> Option<Exit> {
    use rustix::process::{waitid, WaitId, WaitIdOptions};
    let leader = rustix::process::Pid::from_raw(child.id() as i32)?;
    let status = waitid(
        WaitId::Pid(leader),
        WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
    )
    .ok()??;
    status
        .exit_status()
        .map(Exit::Code)
        .or_else(|| status.terminating_signal().map(Exit::Signal))
}

/// No group is signalled here, so collecting early costs nothing.
#[cfg(not(unix))]
fn exit_of(child: &mut Child) -> Option<Exit> {
    child
        .try_wait()
        .ok()?
        .and_then(|status| status.code())
        .map(Exit::Code)
}

/// Stops `child` and its process group: `SIGTERM`, a short grace, `SIGKILL`,
/// and the child reaped. Only this child's own group is signalled, and only
/// while its leader is uncollected: the group's id is the leader's, and it is
/// not handed out again until the leader is collected -- `terminal.rs`'s rule
/// (`exited_uncollected`), and fx v0.0.16's (`src/mcp/io/process.zig:9-11`,
/// `:557-559`). c1db9191 means the same when it signals only a child its
/// reader has not reaped (`childMayBeRunning`, `stdio_dispatcher.zig:1178-1182`).
fn stop_child(child: &mut Child) {
    stop(&mut Server(child), STOP_GRACE);
}

/// A signal stopping a server sends its group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Term,
    Kill,
}

/// The steps of stopping a server, apart from their order so that the order
/// can be tested: a group id is only free of reuse while its leader is not yet
/// collected, which no test of a real process can catch going wrong.
trait Stopping {
    /// Signals the server's group.
    fn signal(&mut self, signal: Stop);
    /// Whether the leader has exited, without collecting it.
    fn exited(&mut self) -> bool;
    /// Collects the leader.
    fn collect(&mut self);
}

/// `SIGTERM`, up to `grace` for the leader to exit, `SIGKILL`, and only then
/// the leader collected.
fn stop(server: &mut impl Stopping, grace: Duration) {
    let started = Instant::now();
    server.signal(Stop::Term);
    while started.elapsed() < grace && !server.exited() {
        std::thread::sleep(Duration::from_millis(10));
    }
    server.signal(Stop::Kill);
    server.collect();
}

/// A real server, by its leader.
struct Server<'a>(&'a mut Child);

impl Stopping for Server<'_> {
    fn signal(&mut self, signal: Stop) {
        #[cfg(unix)]
        if let Some(group) = rustix::process::Pid::from_raw(self.0.id() as i32) {
            let signal = match signal {
                Stop::Term => rustix::process::Signal::TERM,
                Stop::Kill => rustix::process::Signal::KILL,
            };
            let _ = rustix::process::kill_process_group(group, signal);
        }
        #[cfg(not(unix))]
        if signal == Stop::Kill {
            let _ = self.0.kill();
        }
    }

    #[cfg(unix)]
    fn exited(&mut self) -> bool {
        use rustix::process::{waitid, WaitId, WaitIdOptions};
        let Some(leader) = rustix::process::Pid::from_raw(self.0.id() as i32) else {
            return true;
        };
        // `NOWAIT` reports the exit and leaves the leader to be collected.
        !matches!(
            waitid(
                WaitId::Pid(leader),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
            ),
            Ok(None) | Err(rustix::io::Errno::INTR)
        )
    }

    /// No group is signalled here, so collecting early costs nothing.
    #[cfg(not(unix))]
    fn exited(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(Some(_)) | Err(_))
    }

    fn collect(&mut self) {
        // The leader alone, should it have left its group; still uncollected.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
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
    // one the person configured (`RedirectNotAllowed`). And no proxy from the
    // environment unless the URL is https: the request carries the server's
    // `headers`, `header_env` and `bearer_token_env` values, and over loopback
    // http a proxy would read them as sent. Upstream loads no proxy at all.
    let Ok(client) = crate::gateway::bearer_rules(reqwest::Client::builder(), &target.url).build()
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
        .map_err(|err| McpError::Named(send_failure(&err)))?;
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

/// Why a request never got an answer, by the name upstream's HTTP client
/// gives the reason where it is one a person can act on: `ConnectionRefused`
/// for an endpoint nothing listens on (what fx's server details show), and
/// `McpConnectionFailed` for the rest.
fn send_failure(err: &reqwest::Error) -> &'static str {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::ConnectionRefused {
                return "ConnectionRefused";
            }
        }
        source = cause.source();
    }
    "McpConnectionFailed"
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
    fn an_http_server_on_loopback_is_asked_with_no_proxy_between() {
        // Cleartext to loopback with the configured `Authorization`: a proxy
        // from the environment would read it as sent
        // (`crate::gateway::bearer_rules`). Upstream loads no proxy.
        if let Some(origin) = crate::test_net::server_in_this_copy() {
            let transport = HttpTransport::open(
                &format!("{origin}/mcp"),
                &[(
                    "Authorization".to_string(),
                    "Bearer fake-mcp-token-not-real".to_string(),
                )],
            )
            .expect("a transport");
            assert_eq!(
                transport.request("tools/list", json!({}), Duration::from_secs(10), None),
                Ok(json!({"tools": []}))
            );
            return;
        }
        crate::test_net::assert_no_proxy_is_used(
            "mcp::transport::tests::an_http_server_on_loopback_is_asked_with_no_proxy_between",
            crate::test_net::json_answer(r#"{"jsonrpc":"2.0","id":0,"result":{"tools":[]}}"#),
        );
    }

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

    /// Upstream's own case (`src/core/mcp_host/config.zig:389-411@481104fd`),
    /// by name: six of xfx's variables and the server's `env` over them, a
    /// shell function dropped, the credential and the rest never passed.
    #[test]
    fn a_server_is_given_six_of_xfxs_variables_then_its_own_env() {
        let inherited = |name: &str| {
            [
                ("HOME", "/home/me"),
                ("LOGNAME", "me"),
                ("PATH", "/usr/bin"),
                ("SHELL", "() { :; }"),
                ("TERM", "xterm"),
                ("USER", "me"),
                ("AI_GATEWAY_API_KEY", "xfx-sentinel-gateway-key"),
                ("VERCEL_OIDC_TOKEN", "xfx-sentinel-oidc-token"),
                ("HTTPS_PROXY", "http://127.0.0.1:9"),
            ]
            .into_iter()
            .find(|(held, _)| *held == name)
            .map(|(_, value)| OsString::from(value))
        };
        let env = [
            ("GITHUB_TOKEN".to_string(), "g".to_string()),
            ("TERM".to_string(), "dumb".to_string()),
        ];
        let got = server_environment(inherited, &env);
        let want: BTreeMap<OsString, OsString> = [
            ("GITHUB_TOKEN", "g"),
            ("HOME", "/home/me"),
            ("LOGNAME", "me"),
            ("PATH", "/usr/bin"),
            ("TERM", "dumb"),
            ("USER", "me"),
        ]
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect();
        assert_eq!(got, want);
        // A credential the server's own `env` names is a grant.
        let granted = server_environment(
            |_| None,
            &[("AI_GATEWAY_API_KEY".into(), "xfx-sentinel-granted".into())],
        );
        assert_eq!(
            granted.get(std::ffi::OsStr::new("AI_GATEWAY_API_KEY")),
            Some(&OsString::from("xfx-sentinel-granted"))
        );
        // Exactly these six: asked about every name, it passes on no other,
        // so a seventh added to the list fails here rather than passing.
        let everything = server_environment(|name| Some(OsString::from(name)), &[]);
        let names: Vec<&std::ffi::OsStr> = everything.keys().map(OsString::as_os_str).collect();
        assert_eq!(names, ["HOME", "LOGNAME", "PATH", "SHELL", "TERM", "USER"]);
    }

    /// Set only in the runner the case below starts, so the cases it runs
    /// refuse to run anywhere else.
    #[cfg(unix)]
    const SENTINEL_RUNNER: &str = "XFX_TEST_SENTINEL_RUNNER";

    /// The cases below need xfx's own environment to hold credentials, and a
    /// test may not set a variable in its own process. So they run in a copy
    /// of this test binary started with nothing of this one's environment but
    /// `PATH`: fake credentials, an unrelated fake secret and a proxy, and the
    /// six names a server is given. A child that prints its environment there
    /// can print only these, whatever the machine running the suite holds.
    #[cfg(unix)]
    #[test]
    fn a_server_and_its_docker_cleanup_see_only_what_they_are_given() {
        const CASES: [&str; 2] = [
            "mcp::transport::tests::sentinel_runner_a_stdio_server_sees_six_names_and_its_own_env",
            "mcp::transport::tests::sentinel_runner_a_docker_cleanup_runs_with_the_servers_environment",
        ];
        let dir = tempfile::tempdir().expect("dir");
        let mut runner =
            crate::process::command(std::env::current_exe().expect("this test binary"));
        runner
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            )
            .env("TMPDIR", dir.path())
            .env("HOME", dir.path())
            .env("LOGNAME", "xfx-sentinel-logname")
            .env("SHELL", "/bin/sh")
            .env("TERM", "xfx-sentinel-term")
            .env("USER", "xfx-sentinel-user")
            .env("AI_GATEWAY_API_KEY", "xfx-sentinel-gateway-key")
            .env("VERCEL_OIDC_TOKEN", "xfx-sentinel-oidc-token")
            .env("GITHUB_TOKEN", "xfx-sentinel-github-token")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env(SENTINEL_RUNNER, "1")
            .args(["--ignored", "--exact", "--test-threads=1"])
            .args(CASES);
        let output = runner.output().expect("a runner");
        let report = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // Counted, not only exited-zero: a filter that matched nothing passes.
        assert!(
            output.status.success() && report.contains("test result: ok. 2 passed"),
            "{report}"
        );
    }

    /// What `/usr/bin/env` wrote to `path`, by name, once it is all there.
    #[cfg(unix)]
    fn printed_environment(path: &std::path::Path) -> std::collections::BTreeMap<String, String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let text = loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                break text;
            }
            assert!(
                Instant::now() < deadline,
                "{} never appeared",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        text.lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// A child's environment held none of xfx's that it was not given --
    /// named, never valued, in the failure.
    #[cfg(unix)]
    fn assert_given_only(
        environment: &std::collections::BTreeMap<String, String>,
        granted: &[&str],
        who: &str,
    ) {
        let names: Vec<&String> = environment.keys().collect();
        for name in [
            "AI_GATEWAY_API_KEY",
            "VERCEL_OIDC_TOKEN",
            "GITHUB_TOKEN",
            "HTTPS_PROXY",
            SENTINEL_RUNNER,
        ] {
            assert!(
                granted.contains(&name) || !environment.contains_key(name),
                "{who} was handed {name}: {names:?}"
            );
        }
        for (name, value) in [
            ("LOGNAME", "xfx-sentinel-logname"),
            ("SHELL", "/bin/sh"),
            ("USER", "xfx-sentinel-user"),
        ] {
            assert_eq!(
                environment.get(name).map(String::as_str),
                Some(value),
                "{who} lost {name}: {names:?}"
            );
        }
        for name in ["HOME", "PATH"] {
            assert!(
                environment.contains_key(name),
                "{who} lost {name}: {names:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "run by a_server_and_its_docker_cleanup_see_only_what_they_are_given"]
    fn sentinel_runner_a_stdio_server_sees_six_names_and_its_own_env() {
        assert!(
            std::env::var_os(SENTINEL_RUNNER).is_some(),
            "runs only in a_server_and_its_docker_cleanup_see_only_what_they_are_given's runner"
        );
        let dir = tempfile::tempdir().expect("dir");
        let out = dir.path().join("server.env");
        let out = out.to_string_lossy().into_owned();
        // The server's own `env` is a grant: a credential it names on purpose
        // is passed, and a name it shares with xfx's environment is its own.
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                r#"/usr/bin/env > "$XFX_TEST_OUT.part" && /bin/mv "$XFX_TEST_OUT.part" "$XFX_TEST_OUT"; exec /bin/cat > /dev/null"#.into(),
            ],
            &[
                ("XFX_TEST_OUT".into(), out.clone()),
                ("VERCEL_OIDC_TOKEN".into(), "xfx-sentinel-granted".into()),
                ("TERM".into(), "dumb".into()),
            ],
            None,
        )
        .expect("spawned");
        let environment = printed_environment(std::path::Path::new(&out));
        transport.close();
        assert_given_only(&environment, &["VERCEL_OIDC_TOKEN"], "the server");
        let names: Vec<&String> = environment.keys().collect();
        assert_eq!(
            environment.get("TERM").map(String::as_str),
            Some("dumb"),
            "{names:?}"
        );
        assert_eq!(
            environment.get("VERCEL_OIDC_TOKEN").map(String::as_str),
            Some("xfx-sentinel-granted"),
            "{names:?}"
        );
        assert_eq!(environment.get("XFX_TEST_OUT"), Some(&out), "{names:?}");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "run by a_server_and_its_docker_cleanup_see_only_what_they_are_given"]
    fn sentinel_runner_a_docker_cleanup_runs_with_the_servers_environment() {
        use std::os::unix::fs::PermissionsExt;

        assert!(
            std::env::var_os(SENTINEL_RUNNER).is_some(),
            "runs only in a_server_and_its_docker_cleanup_see_only_what_they_are_given's runner"
        );
        // A `docker` written for the test: `run` names a container in the
        // cidfile xfx inserted and waits for the end of its input, `rm` only
        // records how it was started.
        let dir = tempfile::tempdir().expect("dir");
        let docker = dir.path().join("docker");
        std::fs::write(
            &docker,
            "#!/bin/sh\n\
             dir=${0%/*}\n\
             case \"$1\" in\n\
             run) printf 0123456789abcdef > \"$3\"; /usr/bin/env > \"$dir/run.part\"; /bin/mv \"$dir/run.part\" \"$dir/run.env\"; exec /bin/cat > /dev/null ;;\n\
             rm) /usr/bin/env > \"$dir/rm.part\"; /bin/mv \"$dir/rm.part\" \"$dir/rm.env\" ;;\n\
             esac\n",
        )
        .expect("a docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("0755");
        let transport = StdioTransport::spawn(
            &docker.to_string_lossy(),
            &["run".into(), "-i".into(), "xfx-fixture".into()],
            &[("DOCKER_HOST".into(), "unix:///xfx-sentinel.sock".into())],
            None,
        )
        .expect("spawned");
        let run = printed_environment(&dir.path().join("run.env"));
        transport.close();
        let rm = printed_environment(&dir.path().join("rm.env"));
        // The cleanup reaches the daemon the server did, or the container
        // stays.
        for (environment, who) in [(&run, "docker run"), (&rm, "docker rm")] {
            assert_eq!(
                environment.get("DOCKER_HOST").map(String::as_str),
                Some("unix:///xfx-sentinel.sock"),
                "{who} lost the server's DOCKER_HOST: {:?}",
                environment.keys().collect::<Vec<_>>()
            );
        }
        assert_given_only(&run, &[], "docker run");
        assert_given_only(&rm, &[], "docker rm");
        for name in ["HOME", "PATH", "TERM"] {
            assert_eq!(run.get(name), rm.get(name), "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_request_the_server_never_answers_times_out_and_a_cancelled_one_stops_at_once() {
        let transport = StdioTransport::spawn(
            "/bin/sh",
            // `; :` so that no shell `exec`s `cat` and closes the output.
            &["-c".into(), "cat > /dev/null; :".into()],
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

    /// A server that stays up and never reads its input: its leader and a
    /// grandchild in its group, their ids as `(leader, grandchild)`.
    #[cfg(unix)]
    fn deaf_server(script: &str) -> (Arc<StdioTransport>, (i32, i32), tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("dir");
        let pids = dir.path().join("pids");
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                script.into(),
                "sh".into(),
                pids.to_string_lossy().into_owned(),
            ],
            &[],
            None,
        )
        .expect("spawned");
        let deadline = Instant::now() + Duration::from_secs(60);
        let ids = loop {
            let text = std::fs::read_to_string(&pids).unwrap_or_default();
            let ids: Vec<i32> = text
                .split_whitespace()
                .filter_map(|id| id.parse().ok())
                .collect();
            if text.ends_with('\n') && ids.len() == 2 {
                break (ids[0], ids[1]);
            }
            assert!(Instant::now() < deadline, "the server never started");
            std::thread::sleep(Duration::from_millis(20));
        };
        (Arc::new(transport), ids, dir)
    }

    /// The script of a server that never reads: a grandchild, both ids, and
    /// then a leader that holds its input open and asleep -- both for longer
    /// than [`assert_gone`] waits, so only a signal ends them in time.
    #[cfg(unix)]
    const DEAF: &str = r#"sleep 120 & echo "$$ $!" > "$1"; exec sleep 120"#;

    /// More than any pipe buffer holds (64 KiB on Linux and macOS).
    #[cfg(unix)]
    fn oversized_call() -> Value {
        json!({"name": "echo", "arguments": {"text": "x".repeat(2 * 1024 * 1024)}})
    }

    /// `f` on a thread of its own, and what it returned and how long it took,
    /// or `None` if it had not returned after `bound`.
    #[cfg(unix)]
    fn within<T: Send + 'static>(
        bound: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Option<(T, Duration)> {
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let started = Instant::now();
            let value = f();
            let _ = done.send((value, started.elapsed()));
        });
        finished.recv_timeout(bound).ok()
    }

    #[cfg(unix)]
    fn assert_gone(pid: i32, what: &str) {
        let pid = rustix::process::Pid::from_raw(pid).expect("pid");
        let deadline = Instant::now() + Duration::from_secs(30);
        while rustix::process::test_kill_process(pid).is_ok() {
            assert!(Instant::now() < deadline, "{what} outlived its stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_request_to_a_server_that_never_reads_ends_at_its_deadline_and_close_stops_it() {
        let (transport, (leader, grandchild), _dir) = deaf_server(DEAF);
        // The deadline is set before the frame is serialized -- 2 MiB, some
        // 40 ms in a debug build on a fast machine and more on a loaded one
        // -- and a deadline that passes before any of the frame went holds it
        // back whole: the connection is as it was, and the server is left
        // alone (`a_cancelled_or_expired_call_puts_nothing_on_the_wire`). So
        // a request whose deadline came that early is made again, with a
        // longer one, until its deadline comes with the frame under way.
        let mut timeout = Duration::from_millis(300);
        loop {
            let caller = Arc::clone(&transport);
            let (result, took) = within(timeout + Duration::from_secs(20), move || {
                caller.request("tools/call", oversized_call(), timeout, None)
            })
            .expect(
                "the request outlived its deadline: its write to the full pipe is not under it",
            );
            assert_eq!(result, Err(McpError::Named("McpRequestTimedOut")));
            assert!(took < timeout + Duration::from_secs(5), "took {took:?}");
            if !transport.alive() {
                break;
            }
            assert!(
                rustix::process::test_kill_process(
                    rustix::process::Pid::from_raw(grandchild).expect("pid")
                )
                .is_ok(),
                "a frame held back whole stopped the server's group"
            );
            assert!(
                matches!(
                    rustix::process::waitid(
                        rustix::process::WaitId::Pid(
                            rustix::process::Pid::from_raw(leader).expect("pid")
                        ),
                        rustix::process::WaitIdOptions::EXITED
                            | rustix::process::WaitIdOptions::NOWAIT
                            | rustix::process::WaitIdOptions::NOHANG,
                    ),
                    Ok(None)
                ),
                "a frame held back whole stopped the server"
            );
            timeout *= 4;
            assert!(
                timeout <= Duration::from_secs(20),
                "the frame was never under way by its deadline"
            );
        }
        // The server would not take the frame, and what it would read next
        // is half of one: the connection is over before the request returns
        // -- the next one is refused, never written after the half frame --
        // and its whole group is killed there and then, before any close,
        // while its leader is left for the close to collect.
        assert_eq!(
            transport.request("ping", json!({}), Duration::from_secs(1), None),
            Err(McpError::Named("McpConnectionClosed"))
        );
        assert_gone(grandchild, "the server's grandchild, at the cut,");
        assert!(
            matches!(
                rustix::process::waitid(
                    rustix::process::WaitId::Pid(
                        rustix::process::Pid::from_raw(leader).expect("pid")
                    ),
                    rustix::process::WaitIdOptions::EXITED
                        | rustix::process::WaitIdOptions::NOWAIT
                        | rustix::process::WaitIdOptions::NOHANG,
                ),
                Ok(Some(_))
            ),
            "the leader was not killed, or was collected, before the close"
        );
        let closer = Arc::clone(&transport);
        let ((), took) =
            within(Duration::from_secs(20), move || closer.close()).expect("close never returned");
        assert!(took < STOP_GRACE + Duration::from_secs(5), "took {took:?}");
        assert_gone(leader, "the server");
        assert_gone(grandchild, "the server's grandchild");
    }

    #[cfg(unix)]
    #[test]
    fn a_request_to_a_server_that_never_reads_stops_at_its_cancellation() {
        let (transport, (leader, grandchild), _dir) = deaf_server(DEAF);
        let cancel = CancelToken::new();
        let caller = Arc::clone(&transport);
        let token = cancel.clone();
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let result = caller.request(
                "tools/call",
                oversized_call(),
                Duration::from_secs(120),
                Some(&token),
            );
            let _ = done.send(result);
        });
        std::thread::sleep(Duration::from_millis(300));
        cancel.cancel();
        let result = finished.recv_timeout(Duration::from_secs(20)).expect(
            "the request outlived its cancellation: its write to the full pipe does not look",
        );
        assert_eq!(result, Err(McpError::Named("Cancelled")));
        // Part of the frame went: the connection is over.
        assert!(!transport.alive());
        assert_eq!(
            transport.request("ping", json!({}), Duration::from_secs(1), None),
            Err(McpError::Named("McpConnectionClosed"))
        );
        transport.close();
        assert_gone(leader, "the server");
        assert_gone(grandchild, "the server's grandchild");
    }

    #[cfg(unix)]
    #[test]
    fn close_stops_a_server_that_never_reads_while_a_write_to_it_is_blocked() {
        let (transport, (leader, grandchild), _dir) = deaf_server(DEAF);
        let caller = Arc::clone(&transport);
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let result = caller.request(
                "tools/call",
                oversized_call(),
                Duration::from_secs(120),
                None,
            );
            let _ = done.send(result);
        });
        // The request is registered, and then a moment for its write to fill
        // the pipe and block.
        let deadline = Instant::now() + Duration::from_secs(60);
        while transport
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
        {
            assert!(Instant::now() < deadline, "the request never started");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300));
        let closer = Arc::clone(&transport);
        let ((), took) = within(Duration::from_secs(20), move || closer.close()).expect(
            "close never returned: it waits for the input a blocked write holds, and the server runs on",
        );
        assert!(took < STOP_GRACE + Duration::from_secs(5), "took {took:?}");
        assert_gone(leader, "the server");
        assert_gone(grandchild, "the server's grandchild");
        let result = finished
            .recv_timeout(Duration::from_secs(20))
            .expect("the request outlived close");
        assert_eq!(result, Err(McpError::Named("McpConnectionClosed")));
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_asks_and_never_reads_the_answers_cannot_hold_close() {
        // Thousands of `ping`s, whose answers are more than the pipe holds,
        // and the input never read.
        let script = r#"sleep 120 & echo "$$ $!" > "$1"; i=0; while [ "$i" -lt 4000 ]; do printf '{"jsonrpc":"2.0","id":%d,"method":"ping"}\n' "$i"; i=$((i+1)); done; exec sleep 120"#;
        let (transport, (leader, grandchild), _dir) = deaf_server(script);
        // An answer it does not take within its bound ends the connection,
        // rather than stopping the reader for a second per question.
        let deadline = Instant::now() + Duration::from_secs(20);
        while transport.alive() {
            assert!(
                Instant::now() < deadline,
                "a server that asks and never reads kept its connection"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let closer = Arc::clone(&transport);
        let ((), took) = within(Duration::from_secs(20), move || closer.close())
            .expect("close never returned: it waits for the input an answer to the server holds");
        assert!(took < STOP_GRACE + Duration::from_secs(5), "took {took:?}");
        assert_gone(leader, "the server");
        assert_gone(grandchild, "the server's grandchild");
    }

    /// A server whose leader exits at its `exits_at`-th look, and every step
    /// taken on it, in order.
    struct Scripted {
        exits_at: usize,
        looks: usize,
        exited: bool,
        steps: Vec<&'static str>,
    }

    impl Stopping for Scripted {
        fn signal(&mut self, signal: Stop) {
            self.steps.push(match signal {
                Stop::Term => "TERM",
                Stop::Kill => "KILL",
            });
        }
        fn exited(&mut self) -> bool {
            self.looks += 1;
            self.exited |= self.looks >= self.exits_at;
            self.exited
        }
        fn collect(&mut self) {
            self.steps.push("collect");
        }
    }

    #[test]
    fn every_signal_reaches_the_group_before_its_leader_is_collected() {
        // Gone before the stop, gone during the grace, and never gone.
        for (exits_at, grace) in [
            (1, Duration::from_secs(60)),
            (3, Duration::from_secs(60)),
            (usize::MAX, Duration::from_millis(50)),
        ] {
            let mut server = Scripted {
                exits_at,
                looks: 0,
                exited: false,
                steps: Vec::new(),
            };
            stop(&mut server, grace);
            let collected = server
                .steps
                .iter()
                .position(|step| *step == "collect")
                .expect("the leader was never collected");
            assert_eq!(
                &server.steps[collected..],
                ["collect"],
                "a signal after the leader was collected, whose id is then free \
                 for another group (leader exits at look {exits_at}): {:?}",
                server.steps
            );
            assert_eq!(
                server.steps[..collected].last(),
                Some(&"KILL"),
                "{:?}",
                server.steps
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_server_whose_leader_has_gone_has_the_rest_of_its_group_stopped() {
        let (transport, (leader, grandchild), _dir) =
            deaf_server(r#"sleep 120 & echo "$$ $!" > "$1"; exit 0"#);
        let pid = rustix::process::Pid::from_raw(leader).expect("pid");
        let deadline = Instant::now() + Duration::from_secs(30);
        // Exited, and not collected until the close.
        while rustix::process::waitid(
            rustix::process::WaitId::Pid(pid),
            rustix::process::WaitIdOptions::EXITED
                | rustix::process::WaitIdOptions::NOWAIT
                | rustix::process::WaitIdOptions::NOHANG,
        )
        .ok()
        .flatten()
        .is_none()
        {
            assert!(Instant::now() < deadline, "the leader never exited");
            std::thread::sleep(Duration::from_millis(20));
        }
        transport.close();
        assert_gone(leader, "the leader");
        assert_gone(grandchild, "the leader's grandchild");
    }

    /// A server's input that takes `piece` bytes per write, and does `then`
    /// at each attempt, before it answers.
    struct Pieces<F: FnMut(usize) -> Option<ErrorKind>> {
        piece: usize,
        attempts: usize,
        received: Vec<u8>,
        then: F,
    }

    impl<F: FnMut(usize) -> Option<ErrorKind>> Write for Pieces<F> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.attempts += 1;
            if let Some(kind) = (self.then)(self.attempts) {
                return Err(kind.into());
            }
            let count = bytes.len().min(self.piece);
            self.received.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<F: FnMut(usize) -> Option<ErrorKind>> Sink for Pieces<F> {
        fn wait_writable(&self, _: Duration) {}
    }

    fn pieces<F: FnMut(usize) -> Option<ErrorKind>>(then: F) -> Pieces<F> {
        Pieces {
            piece: 4,
            attempts: 0,
            received: Vec::new(),
            then,
        }
    }

    #[test]
    fn a_cancellation_between_two_pieces_of_a_frame_ends_it_there() {
        let input = Input::new(None);
        let cancel = CancelToken::new();
        let token = cancel.clone();
        // The cancellation comes while the first piece is being taken.
        let mut sink = pieces(move |attempt| {
            if attempt == 1 {
                token.cancel();
            }
            None
        });
        let sent = input.push(
            &mut sink,
            b"0123456789abcdef\n",
            Instant::now() + Duration::from_secs(60),
            Some(&cancel),
            Instant::now,
        );
        assert!(
            matches!(sent, Err(Unsent::Cut(McpError::Named("Cancelled")))),
            "a cancelled frame was written on to its end: {:?}",
            String::from_utf8_lossy(&sink.received)
        );
        assert_eq!(sink.received, b"0123");
    }

    #[test]
    fn a_cancellation_during_an_interrupted_write_sends_nothing() {
        let input = Input::new(None);
        let cancel = CancelToken::new();
        let token = cancel.clone();
        let mut sink = pieces(move |attempt| {
            (attempt == 1).then(|| {
                token.cancel();
                ErrorKind::Interrupted
            })
        });
        let sent = input.push(
            &mut sink,
            b"0123456789abcdef\n",
            Instant::now() + Duration::from_secs(60),
            Some(&cancel),
            Instant::now,
        );
        assert!(
            matches!(sent, Err(Unsent::Held(McpError::Named("Cancelled")))),
            "a write retried after an interruption ignored its cancellation: {:?}",
            String::from_utf8_lossy(&sink.received)
        );
        assert!(sink.received.is_empty());
    }

    #[test]
    fn a_deadline_that_passes_between_two_pieces_ends_the_frame_there() {
        let input = Input::new(None);
        // A clock held before the deadline until the first piece has been
        // taken, and past it from then on: no scheduler can move the deadline
        // to before the first attempt.
        let deadline = Instant::now() + Duration::from_secs(3600);
        let passed = Arc::new(AtomicBool::new(false));
        let mark = Arc::clone(&passed);
        let mut sink = pieces(move |attempt| {
            if attempt == 1 {
                mark.store(true, Ordering::SeqCst);
            }
            None
        });
        let clock = move || {
            if passed.load(Ordering::SeqCst) {
                deadline + Duration::from_millis(1)
            } else {
                deadline - Duration::from_secs(1)
            }
        };
        let sent = input.push(&mut sink, b"0123456789abcdef\n", deadline, None, clock);
        assert!(
            matches!(
                sent,
                Err(Unsent::Cut(McpError::Named("McpRequestTimedOut")))
            ),
            "a frame was written on past its deadline: {:?}",
            String::from_utf8_lossy(&sink.received)
        );
        assert_eq!(sink.received, b"0123");
    }

    #[cfg(unix)]
    #[test]
    fn a_cancelled_or_expired_call_puts_nothing_on_the_wire() {
        // A server that records every byte it is sent. The `:` after it keeps
        // a shell that would `exec` its last command (dash) from doing so: the
        // shell holds the server's output open, as a live server does.
        let dir = tempfile::tempdir().expect("dir");
        let wire = dir.path().join("wire");
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                r#"cat > "$1"; :"#.into(),
                "sh".into(),
                wire.to_string_lossy().into_owned(),
            ],
            &[],
            None,
        )
        .expect("spawned");
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            transport.request(
                "tools/call",
                json!({"name": "rm", "arguments": {}}),
                Duration::from_secs(60),
                Some(&cancel)
            ),
            Err(McpError::Named("Cancelled"))
        );
        assert_eq!(
            transport.request("tools/list", json!({}), Duration::ZERO, None),
            Err(McpError::Named("McpRequestTimedOut"))
        );
        // Whatever went before this reaches the server before it.
        transport
            .notify("notifications/sentinel", None)
            .expect("the sentinel");
        let sentinel = "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/sentinel\"}\n";
        let deadline = Instant::now() + Duration::from_secs(30);
        let received = loop {
            let received = std::fs::read_to_string(&wire).unwrap_or_default();
            if received.ends_with(sentinel) {
                break received;
            }
            assert!(Instant::now() < deadline, "the sentinel never arrived");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            received, sentinel,
            "a cancelled or expired call reached the server"
        );
        assert!(transport.alive(), "nothing was sent, so nothing was cut");
        transport.close();
    }

    #[cfg(unix)]
    #[test]
    fn looking_for_a_leader_s_exit_leaves_it_to_be_collected() {
        // `true` from the path: `/bin/true` is `/usr/bin/true` on macOS.
        let mut child = crate::process::command("true").spawn().expect("spawned");
        let leader = rustix::process::Pid::from_raw(child.id() as i32).expect("pid");
        let deadline = Instant::now() + Duration::from_secs(30);
        while !Server(&mut child).exited() {
            assert!(Instant::now() < deadline, "the leader never exited");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Still there to be collected: its id is still its group's.
        assert!(
            matches!(
                rustix::process::waitid(
                    rustix::process::WaitId::Pid(leader),
                    rustix::process::WaitIdOptions::EXITED
                        | rustix::process::WaitIdOptions::NOWAIT
                        | rustix::process::WaitIdOptions::NOHANG,
                ),
                Ok(Some(_))
            ),
            "looking for the exit collected the leader"
        );
        assert!(child.wait().expect("collected").success());
    }

    #[cfg(unix)]
    #[test]
    fn a_server_that_shut_its_input_is_diagnosed_by_what_it_says_on_its_way_out() {
        // The request fails at the write, with the server's output still
        // open: the connection is not known to be gone until the write says
        // so. fx ends a child it can no longer write to, and its last words
        // still arrive (`stdio_dispatcher.zig:2897-2928`).
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                "exec 0<&-; echo 'npm error code E401' >&2; exec sleep 30".into(),
            ],
            &[],
            None,
        )
        .expect("spawned");
        let deadline = Instant::now() + Duration::from_secs(30);
        while !transport
            .stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains("E401")
        {
            assert!(Instant::now() < deadline, "the server never spoke");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            transport.request("initialize", json!({}), Duration::from_secs(60), None),
            Err(McpError::Named("McpConnectionClosed"))
        );
        assert!(
            !transport.alive(),
            "a server xfx can no longer write to kept its connection, so it is never started again"
        );
        assert_eq!(
            transport.diagnostic().as_deref(),
            Some("npm error code E401")
        );
        transport.close();
    }

    #[cfg(unix)]
    #[test]
    fn a_servers_exit_is_read_without_collecting_it_and_close_collects_it_and_its_group() {
        // `Transport::exit` reads how the process ended with `NOWAIT`: as
        // often as asked, the same answer, and the leader still uncollected,
        // so its id is still its group's until `close` collects it.
        let transport = StdioTransport::spawn(
            "/bin/sh",
            &["-c".into(), "exec 0<&-; exit 3".into()],
            &[],
            None,
        )
        .expect("spawned");
        let leader = rustix::process::Pid::from_raw(
            transport
                .child
                .lock()
                .unwrap()
                .as_ref()
                .expect("a child")
                .id() as i32,
        )
        .expect("pid");
        let deadline = Instant::now() + Duration::from_secs(30);
        let first = loop {
            if let Some(exit) = transport.exit() {
                break exit;
            }
            assert!(Instant::now() < deadline, "the server never exited");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(first, Exit::Code(3));
        for _ in 0..3 {
            assert_eq!(transport.exit(), Some(Exit::Code(3)));
        }
        let uncollected = || {
            matches!(
                rustix::process::waitid(
                    rustix::process::WaitId::Pid(leader),
                    rustix::process::WaitIdOptions::EXITED
                        | rustix::process::WaitIdOptions::NOWAIT
                        | rustix::process::WaitIdOptions::NOHANG,
                ),
                Ok(Some(_))
            )
        };
        assert!(uncollected(), "reading the exit collected the leader");
        transport.close();
        assert!(!uncollected(), "close left the leader uncollected");
        assert!(
            rustix::process::test_kill_process_group(leader).is_err(),
            "its group outlived the close"
        );
        assert_eq!(transport.exit(), None, "closed: nothing left to read");
    }
}
