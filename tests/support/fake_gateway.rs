//! A local stand-in for the Vercel AI Gateway.
//!
//! It speaks enough HTTP/1.1 to be indistinguishable from the real endpoint for
//! the paths xfx exercises: it reads a `Content-Length` request, records the
//! exact method, path, headers, and body, and replays a scripted response as a
//! chunked `text/event-stream`.
//!
//! It is written directly on `std::net::TcpListener` rather than on a framework
//! so that a test can do things a framework hides: split one SSE event across
//! several TCP writes, and close a connection in the middle of a response body
//! without a terminating chunk. Both are protocol facts xfx must survive, and
//! both are how a real stream fails.
//!
//! Modeled on upstream's fake Gateway, which serves scripted SSE and captures
//! `{body, headers}` per request
//! (`vercel-labs/fx@580a0c5d tests/e2e/tmux-helpers.ts:125-207`, `:324-398`).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};

/// How long the accept loop waits between shutdown checks.
const ACCEPT_POLL: Duration = Duration::from_millis(5);

/// How long a finished connection waits for the client to close its half.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// The longest a deliberately unfinished response is held open.
const HANG_TIMEOUT: Duration = Duration::from_secs(30);

/// One request the fake Gateway received, captured verbatim.
#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    /// Header names are lowercased; values are kept exactly as sent.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl CapturedRequest {
    /// The first value of `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(key, _)| *key == wanted)
            .map(|(_, value)| value.as_str())
    }

    /// The request body parsed as JSON. Panics with the raw body when it is not
    /// JSON, because a malformed request body is the failure under test.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|err| panic!("request body is not JSON ({err}): {:?}", self.body))
    }
}

/// `requests` as `METHOD path bearer=yes|no`: what a case about the key going
/// nowhere prints when it went somewhere.
pub fn sent(requests: &[CapturedRequest]) -> Vec<String> {
    requests
        .iter()
        .map(|request| {
            let bearer = if request.header("authorization").is_some() {
                "yes"
            } else {
                "no"
            };
            format!("{} {} bearer={bearer}", request.method, request.path)
        })
        .collect()
}

/// One scripted response.
#[derive(Debug, Clone)]
pub enum Reply {
    /// A complete `text/event-stream` body, written as a single chunk.
    Sse(String),
    /// A `text/event-stream` body written as the given pieces, in order, each
    /// as its own chunk with a flush. Used to prove the decoder does not depend
    /// on event boundaries lining up with transport boundaries.
    SsePieces(Vec<String>),
    /// Write these bytes as chunks and then drop the connection without the
    /// terminating chunk, so the client sees a truncated body mid-delivery.
    SseThenAbort(Vec<String>),
    /// Write these bytes as chunks and then keep the response open until the
    /// client hangs up. This is the shape of a real stream that has started
    /// answering and has not finished, which is the only state a user can
    /// actually interrupt.
    SseThenHang(Vec<String>),
    /// A non-2xx response with a plain body.
    Status(u16, String),
    /// A redirect to `location`, which a client that follows redirects will
    /// replay the request body to.
    Redirect { status: u16, location: String },
    /// A non-2xx response carrying extra headers, so a test can drive the
    /// client's reaction to `Retry-After`.
    StatusWithHeaders {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
    /// The inner reply, written only once the flag is set: a provider that has
    /// the request and has not answered it yet. Made by [`Reply::held`].
    ///
    /// A hold nobody releases -- the fixture dropped first, or
    /// [`HANG_TIMEOUT`] gone by -- is answered with a 500 rather than with the
    /// reply, so a test that forgot its release fails instead of passing late.
    ///
    /// What a test does with that interval is look at the screen while the
    /// turn is still in hand. Without it, a turn that is answered at once can
    /// run from its start to its next decision while the UI thread is not
    /// scheduled, and the UI then takes all of it in one batch and paints
    /// only where it ended.
    Held(Arc<AtomicBool>, Box<Reply>),
}

impl Reply {
    /// A retryable status that asks the client to wait `seconds`.
    pub fn retry_after(status: u16, seconds: u64, body: &str) -> Self {
        Self::StatusWithHeaders {
            status,
            headers: vec![("retry-after".to_string(), seconds.to_string())],
            body: body.to_string(),
        }
    }

    /// `reply`, held until the returned flag is set ([`Reply::Held`]).
    ///
    /// **The caller must set it within [`HANG_TIMEOUT`]** of the request
    /// arriving; a hold still standing then is answered with a 500.
    pub fn held(reply: Reply) -> (Self, Arc<AtomicBool>) {
        let release = Arc::new(AtomicBool::new(false));
        (Self::Held(Arc::clone(&release), Box::new(reply)), release)
    }
}

#[derive(Default)]
struct State {
    requests: Mutex<Vec<CapturedRequest>>,
    script: Mutex<VecDeque<Reply>>,
    /// What `GET /coding-agent/v1/models` answers: a status and a body, `404`
    /// until a test names a catalog ([`FakeGateway::with_catalog`]).
    catalog: Mutex<Option<(u16, String)>>,
    /// Every catalog request, kept apart from [`Self::requests`].
    catalog_requests: Mutex<Vec<CapturedRequest>>,
    /// What a session-title request is answered with: the title's text, or
    /// `None` for a stream with no text -- an answer that leaves the title the
    /// prompt gave ([`FakeGateway::with_title`]).
    title: Mutex<Option<String>>,
    /// Every session-title request, kept apart from [`Self::requests`].
    title_requests: Mutex<Vec<CapturedRequest>>,
    /// While set and not yet released, a title request is answered only once
    /// it is ([`FakeGateway::with_title_held`]).
    title_hold: Mutex<Option<Arc<AtomicBool>>>,
}

/// What every session-title request xfx sends begins its instructions with
/// (upstream's `instructions_text`, `vercel-labs/fx@c1db9191
/// src/core/session/session_title_generation.zig:23`).
pub const TITLE_INSTRUCTIONS: &str = "Generate a short title for a conversation";

/// Where the real Gateway publishes its model catalog
/// (`vercel-labs/fx@c1db9191 src/builtins/gateway.zig:48`).
pub const MODELS_PATH: &str = "/coding-agent/v1/models";

/// A scripted Gateway listening on a loopback port.
///
/// Dropping it stops the listener and joins its thread, so a test cannot leak a
/// server into the next test.
pub struct FakeGateway {
    addr: SocketAddr,
    state: Arc<State>,
    shutdown: Arc<AtomicBool>,
    /// Accepts connections and starts a reader for each.
    thread: Option<JoinHandle<()>>,
    /// Answers the scripted requests, one at a time.
    server: Option<JoinHandle<()>>,
}

impl FakeGateway {
    /// Starts a Gateway that answers requests with `script`, in order.
    ///
    /// A request past the end of the script is answered `500`, so an unexpected
    /// extra request fails the test rather than hanging it.
    pub fn start(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
        let addr = listener.local_addr().expect("resolve local address");
        listener
            .set_nonblocking(true)
            .expect("set the listener non-blocking");

        let state = Arc::new(State {
            requests: Mutex::new(Vec::new()),
            script: Mutex::new(script.into()),
            ..State::default()
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        let (scripted, queue) = mpsc::channel::<(CapturedRequest, TcpStream)>();

        // Every connection is read on a thread of its own, so a client that
        // has connected and not yet written -- or a side call the fixture is
        // holding -- is in front of nobody. A session-title request and a
        // catalog request are answered right there, apart from the script;
        // everything else goes to the one thread that answers the script.
        let thread_state = Arc::clone(&state);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&thread_state);
                        let shutdown = Arc::clone(&thread_shutdown);
                        let scripted = scripted.clone();
                        thread::spawn(move || read_one(&state, &shutdown, stream, &scripted));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL);
                    }
                    Err(_) => break,
                }
            }
        });
        // The script is answered one request at a time, in the order the
        // requests arrived whole, on purpose: the request order a test
        // asserts on is then the real order, and a reply that hangs holds the
        // next scripted request back as a real stream holds its connection.
        let server_state = Arc::clone(&state);
        let server_shutdown = Arc::clone(&shutdown);
        let server = thread::spawn(move || loop {
            match queue.recv_timeout(ACCEPT_POLL) {
                Ok((request, stream)) => {
                    answer_scripted(&server_state, &server_shutdown, request, stream)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if server_shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        });

        Self {
            addr,
            state,
            shutdown,
            thread: Some(thread),
            server: Some(server),
        }
    }

    /// The completion endpoint, in the same path shape as the real Gateway
    /// (`vercel-labs/fx@580a0c5d src/builtins/gateway.zig:41`).
    pub fn chat_url(&self) -> String {
        format!("http://127.0.0.1:{}/v3/ai/language-model", self.addr.port())
    }

    /// The same endpoint addressed by name rather than by literal address.
    pub fn localhost_chat_url(&self) -> String {
        format!("http://localhost:{}/v3/ai/language-model", self.addr.port())
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<CapturedRequest> {
        self.state.requests.lock().expect("requests lock").clone()
    }

    /// Answers `GET /coding-agent/v1/models` with this document.
    ///
    /// A catalog request is **not** a turn: it is answered from here, never
    /// from the script, and recorded apart from [`Self::requests`] -- so a
    /// case that counts the requests a turn made counts the same whether or
    /// not `/model` asked for the catalog first.
    pub fn with_catalog(self, catalog: Value) -> Self {
        *self.state.catalog.lock().expect("catalog lock") = Some((200, catalog.to_string()));
        self
    }

    /// Answers a session-title request with `title`.
    ///
    /// A title request is **not** a turn either: it is answered from here,
    /// never from the script, and recorded apart from [`Self::requests`] --
    /// so a case that counts a turn's requests counts the same whether or
    /// not its first prompt asked for a title. Without this, a title request
    /// is answered with a stream that has no text in it, and the session
    /// keeps the title its first prompt gives it.
    pub fn with_title(self, title: &str) -> Self {
        *self.state.title.lock().expect("title lock") = Some(title.to_string());
        self
    }

    /// [`Self::with_title`], answered only once the returned flag is set (or
    /// the fixture is dropped, or [`HANG_TIMEOUT`] passes): a Gateway that is
    /// slow to name a session.
    pub fn with_title_held(self, title: &str) -> (Self, Arc<AtomicBool>) {
        let release = Arc::new(AtomicBool::new(false));
        *self.state.title_hold.lock().expect("title lock") = Some(Arc::clone(&release));
        (self.with_title(title), release)
    }

    /// Every session-title request received so far, in arrival order.
    pub fn title_requests(&self) -> Vec<CapturedRequest> {
        self.state
            .title_requests
            .lock()
            .expect("title lock")
            .clone()
    }

    /// Every catalog request received so far, in arrival order.
    pub fn catalog_requests(&self) -> Vec<CapturedRequest> {
        self.state
            .catalog_requests
            .lock()
            .expect("catalog lock")
            .clone()
    }

    pub fn request_count(&self) -> usize {
        self.state.requests.lock().expect("requests lock").len()
    }

    /// The single request received. Panics when the count is not exactly one,
    /// which is the assertion most turn tests actually want.
    pub fn only_request(&self) -> CapturedRequest {
        let requests = self.requests();
        assert_eq!(
            requests.len(),
            1,
            "expected exactly one Gateway request, got {}",
            requests.len()
        );
        requests.into_iter().next().expect("one request")
    }
}

impl Drop for FakeGateway {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        for thread in [self.thread.take(), self.server.take()]
            .into_iter()
            .flatten()
        {
            let _ = thread.join();
        }
    }
}

/// Reads one connection's request and answers it, or hands it to the thread
/// that answers the script ([`answer_scripted`]).
///
/// A reader whose client never writes gives up after [`HANG_TIMEOUT`], so no
/// reader outlives a test by more than that.
fn read_one(
    state: &Arc<State>,
    shutdown: &Arc<AtomicBool>,
    stream: TcpStream,
    scripted: &mpsc::Sender<(CapturedRequest, TcpStream)>,
) {
    // On the BSD socket API an accepted socket inherits the listener's
    // O_NONBLOCK flag, so this connection would return `WouldBlock` for a read
    // that simply has not arrived yet. The listener polls; the connection does
    // not.
    stream
        .set_nonblocking(false)
        .expect("serve each connection in blocking mode");
    stream
        .set_nodelay(true)
        .expect("disable Nagle so a piece is its own packet");
    let _ = stream.set_read_timeout(Some(HANG_TIMEOUT));
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));

    let Some(request) = read_request(&mut reader) else {
        return;
    };
    if request.method == "GET" && request.path == MODELS_PATH {
        state
            .catalog_requests
            .lock()
            .expect("catalog lock")
            .push(request);
        let answer = state.catalog.lock().expect("catalog lock").clone();
        let (status, body) = answer.unwrap_or((404, "not found".to_string()));
        let mut writer = stream;
        write_status(&mut writer, status, &[], &body);
        close_cleanly(&mut writer);
        return;
    }
    if request.method == "POST" && request.body.contains(TITLE_INSTRUCTIONS) {
        state
            .title_requests
            .lock()
            .expect("title lock")
            .push(request);
        let hold = state.title_hold.lock().expect("title lock").clone();
        if let Some(release) = hold {
            wait_for_release(&release, shutdown);
        }
        let title = state.title.lock().expect("title lock").clone();
        let mut events: Vec<Value> = title
            .iter()
            .map(|title| text_delta("title_1", title))
            .collect();
        events.push(finish("stop"));
        let mut writer = stream;
        write_reply(&mut writer, Some(Reply::Sse(sse_body(&events))), shutdown);
        close_cleanly(&mut writer);
        return;
    }
    let _ = scripted.send((request, stream));
}

/// Records one scripted request and writes the next scripted reply.
fn answer_scripted(
    state: &Arc<State>,
    shutdown: &Arc<AtomicBool>,
    request: CapturedRequest,
    stream: TcpStream,
) {
    state.requests.lock().expect("requests lock").push(request);

    let reply = state.script.lock().expect("script lock").pop_front();
    let mut writer = stream;
    write_reply(&mut writer, reply, shutdown);
    close_cleanly(&mut writer);
}

/// Writes one scripted reply, or a 500 when the script has run out.
///
/// Shared with `fake_llmux`: an unscripted request answering 500 is what makes
/// an unexpected extra request fail a test rather than hang it, and the chunk
/// splitting, abort, and hang shapes are transport facts that do not depend on
/// which wire is being spoken over them.
pub fn write_reply(writer: &mut TcpStream, reply: Option<Reply>, shutdown: &Arc<AtomicBool>) {
    match reply {
        Some(Reply::Sse(body)) => write_sse(writer, &[body], true),
        Some(Reply::SsePieces(pieces)) => write_sse(writer, &pieces, true),
        Some(Reply::SseThenAbort(pieces)) => write_sse(writer, &pieces, false),
        Some(Reply::SseThenHang(pieces)) => {
            write_sse(writer, &pieces, false);
            hang_until_hangup(writer, shutdown);
        }
        Some(Reply::Redirect { status, location }) => {
            write_status(writer, status, &[("location".to_string(), location)], "")
        }
        Some(Reply::Status(status, body)) => write_status(writer, status, &[], &body),
        Some(Reply::StatusWithHeaders {
            status,
            headers,
            body,
        }) => write_status(writer, status, &headers, &body),
        Some(Reply::Held(release, reply)) => {
            if wait_for_release(&release, shutdown) {
                write_reply(writer, Some(*reply), shutdown);
            } else {
                write_status(
                    writer,
                    500,
                    &[],
                    "fake server: a held reply was never released",
                );
            }
        }
        None => write_status(writer, 500, &[], "fake server: unscripted request"),
    }
}

/// Waits until a held answer is released, the fixture shuts down, or
/// [`HANG_TIMEOUT`] passes -- polled, so that dropping the server can still
/// join its threads and a test that fails before it releases fails rather
/// than hangs. Whether it was released.
fn wait_for_release(release: &AtomicBool, shutdown: &AtomicBool) -> bool {
    let deadline = std::time::Instant::now() + HANG_TIMEOUT;
    while !release.load(Ordering::SeqCst)
        && !shutdown.load(Ordering::SeqCst)
        && std::time::Instant::now() < deadline
    {
        thread::sleep(ACCEPT_POLL);
    }
    release.load(Ordering::SeqCst)
}

/// Holds a started response open until the client closes its end.
///
/// The loop polls rather than blocking forever so that dropping the server can
/// still join its thread: a test that fails before it interrupts anything must
/// fail, not hang. Bounded for the same reason.
fn hang_until_hangup(stream: &mut TcpStream, shutdown: &Arc<AtomicBool>) {
    let deadline = std::time::Instant::now() + HANG_TIMEOUT;
    let _ = stream.set_read_timeout(Some(ACCEPT_POLL));
    let mut discard = [0u8; 1024];
    while std::time::Instant::now() < deadline && !shutdown.load(Ordering::SeqCst) {
        match stream.read(&mut discard) {
            // The client closed, which is what a cancelled turn does.
            Ok(0) => return,
            Ok(_) => continue,
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => return,
        }
    }
}

/// Ends a connection without losing what was already written.
///
/// Dropping a `TcpStream` outright can send an RST, and an RST discards data the
/// peer has received but not yet read. That turns a complete response into a
/// truncated one at random, which is a flaky test rather than a real protocol
/// fact. So: send FIN, then read until the client closes too.
pub fn close_cleanly(stream: &mut TcpStream) {
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(DRAIN_TIMEOUT));
    let mut discard = [0u8; 1024];
    loop {
        match stream.read(&mut discard) {
            Ok(0) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

/// Parses request line, headers, and a `Content-Length` body.
pub fn read_request(reader: &mut BufReader<TcpStream>) -> Option<CapturedRequest> {
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if name == "content-length" {
            content_length = value.parse().unwrap_or(0);
        }
        headers.push((name, value));
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }

    Some(CapturedRequest {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// Writes an SSE response, one HTTP chunk per piece.
///
/// `terminate` decides whether the terminating zero-length chunk is written. A
/// test that wants a truncated body passes `false`.
fn write_sse(stream: &mut TcpStream, pieces: &[String], terminate: bool) {
    let head = "HTTP/1.1 200 OK\r\n\
         content-type: text/event-stream\r\n\
         cache-control: no-cache\r\n\
         transfer-encoding: chunked\r\n\
         connection: close\r\n\
         \r\n";
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    for piece in pieces {
        if piece.is_empty() {
            continue;
        }
        let chunk = format!("{:x}\r\n{piece}\r\n", piece.len());
        if stream.write_all(chunk.as_bytes()).is_err() || stream.flush().is_err() {
            return;
        }
    }
    if terminate {
        let _ = stream.write_all(b"0\r\n\r\n");
        let _ = stream.flush();
    }
    // Dropping without the terminating chunk leaves the client with a truncated
    // body, which is exactly the "delivery already started" failure under test.
}

pub fn write_status(stream: &mut TcpStream, status: u16, extra: &[(String, String)], body: &str) {
    let mut response = format!(
        "HTTP/1.1 {status} {}\r\n\
         content-type: application/json\r\n\
         content-length: {}\r\n\
         connection: close\r\n",
        reason_phrase(status),
        body.len()
    );
    for (name, value) in extra {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    response.push_str(body);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

// ---------------------------------------------------------------------------
// SSE body construction
// ---------------------------------------------------------------------------

/// Renders events as `data: <json>\n\n` frames followed by `data: [DONE]`,
/// matching upstream's fake Gateway body
/// (`vercel-labs/fx@580a0c5d tests/e2e/tmux-helpers.ts:125-130`).
pub fn sse_body(events: &[Value]) -> String {
    let mut out = String::new();
    for event in events {
        out.push_str(&format!("data: {event}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}

/// The same frames without the trailing `[DONE]`.
pub fn sse_body_without_done(events: &[Value]) -> String {
    let mut out = String::new();
    for event in events {
        out.push_str(&format!("data: {event}\n\n"));
    }
    out
}

pub fn text_delta(id: &str, text: &str) -> Value {
    json!({ "type": "text-delta", "id": id, "delta": text })
}

pub fn finish(reason: &str) -> Value {
    json!({ "type": "finish", "finishReason": { "unified": reason, "raw": reason } })
}

pub fn finish_with_usage(reason: &str, input_tokens: u64, output_tokens: u64) -> Value {
    json!({
        "type": "finish",
        "finishReason": { "unified": reason, "raw": reason },
        "usage": {
            "inputTokens": { "total": input_tokens },
            "outputTokens": { "total": output_tokens },
        },
    })
}

pub fn tool_call(id: &str, name: &str, input: Value) -> Value {
    json!({ "type": "tool-call", "toolCallId": id, "toolName": name, "input": input })
}

/// A complete content-only answer: the deltas, then a `stop` finish with usage.
pub fn content_only(deltas: &[&str]) -> String {
    let mut events: Vec<Value> = deltas
        .iter()
        .enumerate()
        .map(|(index, text)| text_delta(&format!("answer_{index}"), text))
        .collect();
    events.push(finish_with_usage("stop", 3, 5));
    sse_body(&events)
}
