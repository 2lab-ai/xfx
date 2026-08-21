//! A local stand-in for the Vercel AI Gateway.
//!
//! It speaks enough HTTP/1.1 to be indistinguishable from the real endpoint for
//! the paths fxr exercises: it reads a `Content-Length` request, records the
//! exact method, path, headers, and body, and replays a scripted response as a
//! chunked `text/event-stream`.
//!
//! It is written directly on `std::net::TcpListener` rather than on a framework
//! so that a test can do things a framework hides: split one SSE event across
//! several TCP writes, and close a connection in the middle of a response body
//! without a terminating chunk. Both are protocol facts fxr must survive, and
//! both are how a real stream fails.
//!
//! Modeled on upstream's fake Gateway, which serves scripted SSE and captures
//! `{body, headers}` per request
//! (`vercel-labs/fx@580a0c5d tests/e2e/tmux-helpers.ts:125-207`, `:324-398`).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};

/// How long the accept loop waits between shutdown checks.
const ACCEPT_POLL: Duration = Duration::from_millis(5);

/// How long a finished connection waits for the client to close its half.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

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
    /// A non-2xx response with a plain body.
    Status(u16, String),
}

#[derive(Default)]
struct State {
    requests: Mutex<Vec<CapturedRequest>>,
    script: Mutex<VecDeque<Reply>>,
}

/// A scripted Gateway listening on a loopback port.
///
/// Dropping it stops the listener and joins its thread, so a test cannot leak a
/// server into the next test.
pub struct FakeGateway {
    addr: SocketAddr,
    state: Arc<State>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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
        });
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_state = Arc::clone(&state);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        // Connections are served one at a time on purpose: the
                        // request order a test asserts on is then the real order.
                        serve(&thread_state, stream);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(ACCEPT_POLL);
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr,
            state,
            shutdown,
            thread: Some(thread),
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
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads one request, records it, and writes the next scripted reply.
fn serve(state: &Arc<State>, stream: TcpStream) {
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
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));

    let Some(request) = read_request(&mut reader) else {
        return;
    };
    state
        .requests
        .lock()
        .expect("requests lock")
        .push(request.clone());

    let reply = state.script.lock().expect("script lock").pop_front();
    let mut writer = stream;
    match reply {
        Some(Reply::Sse(body)) => write_sse(&mut writer, &[body], true),
        Some(Reply::SsePieces(pieces)) => write_sse(&mut writer, &pieces, true),
        Some(Reply::SseThenAbort(pieces)) => write_sse(&mut writer, &pieces, false),
        Some(Reply::Status(status, body)) => write_status(&mut writer, status, &body),
        None => write_status(&mut writer, 500, "fake gateway: unscripted request"),
    }
    close_cleanly(&mut writer);
}

/// Ends a connection without losing what was already written.
///
/// Dropping a `TcpStream` outright can send an RST, and an RST discards data the
/// peer has received but not yet read. That turns a complete response into a
/// truncated one at random, which is a flaky test rather than a real protocol
/// fact. So: send FIN, then read until the client closes too.
fn close_cleanly(stream: &mut TcpStream) {
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
fn read_request(reader: &mut BufReader<TcpStream>) -> Option<CapturedRequest> {
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

fn write_status(stream: &mut TcpStream, status: u16, body: &str) {
    let response = format!(
        "HTTP/1.1 {status} {}\r\n\
         content-type: application/json\r\n\
         content-length: {}\r\n\
         connection: close\r\n\
         \r\n{body}",
        reason_phrase(status),
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
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
