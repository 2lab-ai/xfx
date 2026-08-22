//! A local stand-in for a llmux daemon, and the Anthropic frames it sends.
//!
//! Two things live here. The frame builders below render the exact
//! `event: <name>\ndata: <json>\n\n` shape a real daemon writes, so a decoder
//! test reads like the wire it is about. The server is a sibling of
//! `fake_gateway`: it binds an ephemeral loopback port, records every request
//! verbatim, and answers by *path* rather than in script order, because
//! `xfx setup llmux` makes two probe requests before it makes any other kind.
//!
//! Nothing here holds or expects a credential. That is the point of the backend
//! under test: a keyless loopback request is what llmux accepts, so a fake that
//! demanded a key would be testing something xfx must never send.

use std::collections::VecDeque;
use std::io::BufReader;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};

use super::fake_gateway::{
    close_cleanly, read_request, write_reply, write_status, CapturedRequest, Reply,
};

/// How long the accept loop waits between shutdown checks.
const ACCEPT_POLL: Duration = Duration::from_millis(5);

/// The path the Anthropic Messages data plane lives at.
const MESSAGES_PATH: &str = "/v1/messages";

/// What a real daemon answers `GET /` with, byte for byte.
///
/// It is the probe's whole identification of llmux: any HTTP server on loopback
/// can answer 200, and only this one answers this word.
pub const ROOT_BODY: &str = "llmux";

/// How the fake answers the two probe endpoints.
struct Probes {
    root: (u16, String),
    catalog: (u16, String),
    /// When set, `GET /models` answers this redirect instead of the catalog.
    catalog_redirect: Option<(u16, String)>,
}

struct State {
    requests: Mutex<Vec<CapturedRequest>>,
    script: Mutex<VecDeque<Reply>>,
    probes: Mutex<Probes>,
}

/// A scripted llmux daemon listening on a loopback port.
///
/// Unlike the fake Gateway it answers by **path**, because `xfx setup llmux`
/// probes `GET /` and `GET /models` before anything else happens, and a script
/// keyed by arrival order would make the probe's own requests consume the
/// replies meant for a turn. Only `POST /v1/messages` reads the script.
///
/// Dropping it stops the listener and joins its thread, so a test cannot leak a
/// server into the next test.
pub struct FakeLlmux {
    addr: SocketAddr,
    state: Arc<State>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeLlmux {
    /// A healthy daemon that answers `script` on the data plane, in order.
    pub fn start(script: Vec<Reply>) -> Self {
        Self::with_probes(
            script,
            Probes {
                root: (200, ROOT_BODY.to_string()),
                catalog: (
                    200,
                    catalog(&[("claude-fable-5[1m]", &["fable"])]).to_string(),
                ),
                catalog_redirect: None,
            },
        )
    }

    fn with_probes(script: Vec<Reply>, probes: Probes) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
        let addr = listener.local_addr().expect("resolve local address");
        listener
            .set_nonblocking(true)
            .expect("set the listener non-blocking");

        let state = Arc::new(State {
            requests: Mutex::new(Vec::new()),
            script: Mutex::new(script.into()),
            probes: Mutex::new(probes),
        });
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_state = Arc::clone(&state);
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => serve(&thread_state, &thread_shutdown, stream),
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

    /// Answers `GET /` with something other than the daemon's own word.
    pub fn with_root_body(self, status: u16, body: &str) -> Self {
        self.state.probes.lock().expect("probes lock").root = (status, body.to_string());
        self
    }

    /// Answers `GET /models` with this exact document.
    pub fn with_catalog(self, catalog: Value) -> Self {
        self.state.probes.lock().expect("probes lock").catalog = (200, catalog.to_string());
        self
    }

    /// Answers `GET /models` with a status and a raw body.
    pub fn with_catalog_response(self, status: u16, body: &str) -> Self {
        self.state.probes.lock().expect("probes lock").catalog = (status, body.to_string());
        self
    }

    /// Answers `GET /models` with a redirect, which a following client replays to.
    pub fn with_catalog_redirect(self, status: u16, location: &str) -> Self {
        self.state
            .probes
            .lock()
            .expect("probes lock")
            .catalog_redirect = Some((status, location.to_string()));
        self
    }

    /// The daemon's base URL, which is what `llmux_url` holds.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.addr.port())
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Every request received so far, in arrival order.
    pub fn requests(&self) -> Vec<CapturedRequest> {
        self.state.requests.lock().expect("requests lock").clone()
    }

    /// The paths requested, in order: what a probe did before a turn ran.
    pub fn paths(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .map(|request| request.path)
            .collect()
    }

    /// Every data-plane request, ignoring the probes.
    pub fn message_requests(&self) -> Vec<CapturedRequest> {
        self.requests()
            .into_iter()
            .filter(|request| request.path == MESSAGES_PATH)
            .collect()
    }

    /// The single data-plane request. Panics when the count is not exactly one.
    pub fn only_message_request(&self) -> CapturedRequest {
        let requests = self.message_requests();
        assert_eq!(
            requests.len(),
            1,
            "expected exactly one `{MESSAGES_PATH}` request, got {}",
            requests.len()
        );
        requests.into_iter().next().expect("one request")
    }
}

impl Drop for FakeLlmux {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Reads one request, records it, and answers it by path.
fn serve(state: &Arc<State>, shutdown: &Arc<AtomicBool>, stream: TcpStream) {
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

    let mut writer = stream;
    match request.path.as_str() {
        MESSAGES_PATH => {
            let reply = state.script.lock().expect("script lock").pop_front();
            write_reply(&mut writer, reply, shutdown);
        }
        "/" => {
            let (status, body) = state.probes.lock().expect("probes lock").root.clone();
            write_status(&mut writer, status, &[], &body);
        }
        "/models" => {
            let probes = state.probes.lock().expect("probes lock");
            match probes.catalog_redirect.clone() {
                Some((status, location)) => {
                    drop(probes);
                    write_status(
                        &mut writer,
                        status,
                        &[("location".to_string(), location)],
                        "",
                    );
                }
                None => {
                    let (status, body) = probes.catalog.clone();
                    drop(probes);
                    write_status(&mut writer, status, &[], &body);
                }
            }
        }
        _ => write_status(&mut writer, 404, &[], "fake llmux: no such path"),
    }
    close_cleanly(&mut writer);
}

/// A `GET /models` document in the daemon's own shape.
pub fn catalog(entries: &[(&str, &[&str])]) -> Value {
    let models: Vec<Value> = entries
        .iter()
        .map(|(id, aliases)| {
            json!({
                "id": id,
                "name": id,
                "aliases": aliases,
                "group": "anthropic",
                "efforts": [],
                "max_context": 200000,
            })
        })
        .collect();
    json!({ "models": models })
}

// ---------------------------------------------------------------------------
// Anthropic SSE body construction
// ---------------------------------------------------------------------------

/// One named SSE frame, in the shape the live daemon writes it.
pub fn anthropic_event(name: &str, payload: Value) -> String {
    format!("event: {name}\ndata: {payload}\n\n")
}

/// The `message_start` frame, which is where the input token count arrives.
pub fn anthropic_start(model: &str, input_tokens: u64) -> String {
    anthropic_event(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_fake",
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "usage": { "input_tokens": input_tokens, "output_tokens": 1 },
            },
        }),
    )
}

/// The `message_delta` frame that names the stop reason and the output tokens.
///
/// This is the frame that decides whether a stream was an answer: `message_stop`
/// says the stream ended, and only this one says why the model stopped.
pub fn anthropic_stop(reason: &str, input_tokens: u64, output_tokens: u64) -> String {
    anthropic_event(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": reason, "stop_sequence": null },
            "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens },
        }),
    )
}

/// One text block: start, one delta per fragment, stop.
pub fn anthropic_text_block(index: u64, fragments: &[&str]) -> String {
    let mut out = anthropic_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "text", "text": "" },
        }),
    );
    for fragment in fragments {
        out.push_str(&anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "text_delta", "text": fragment },
            }),
        ));
    }
    out.push_str(&anthropic_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": index }),
    ));
    out
}

/// One `tool_use` block whose input arrives as JSON fragments.
pub fn anthropic_tool_block(index: u64, id: &str, name: &str, fragments: &[&str]) -> String {
    let mut out = anthropic_event(
        "content_block_start",
        json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "tool_use", "id": id, "name": name },
        }),
    );
    for fragment in fragments {
        out.push_str(&anthropic_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": index,
                "delta": { "type": "input_json_delta", "partial_json": fragment },
            }),
        ));
    }
    out.push_str(&anthropic_event(
        "content_block_stop",
        json!({ "type": "content_block_stop", "index": index }),
    ));
    out
}

/// A complete content-only answer, from `message_start` to `message_stop`.
pub fn anthropic_answer(fragments: &[&str]) -> String {
    let mut out = anthropic_start("claude-fake", 3);
    out.push_str(&anthropic_text_block(0, fragments));
    out.push_str(&anthropic_stop("end_turn", 3, 5));
    out.push_str(&anthropic_event(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));
    out
}

/// A complete answer whose only content is one tool call.
pub fn anthropic_tool_answer(id: &str, name: &str, input: &str) -> String {
    let mut out = anthropic_start("claude-fake", 3);
    out.push_str(&anthropic_tool_block(0, id, name, &[input]));
    out.push_str(&anthropic_stop("tool_use", 3, 5));
    out.push_str(&anthropic_event(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));
    out
}

/// The `error` frame a daemon sends inside a 200 response.
pub fn anthropic_error(message: &str) -> String {
    anthropic_event(
        "error",
        json!({
            "type": "error",
            "error": { "type": "api_error", "message": message },
        }),
    )
}
