//! Version detection against real servers: fx v0.0.16 asks `server/discover`
//! before anything else (`vercel-labs/fx@481104fd src/mcp/core/era.zig`), on
//! stdio and over HTTP, and these cases hold xfx to what fx does with each
//! kind of answer -- the frames it sends, the version it ends on, the words
//! it fails with -- and to its bounds: a hostile answer neither crashes nor
//! hangs a start, and nothing of it reaches the screen.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{bounded_run, notices, settled, Home, WAIT};
use crate::mcp::config::{self, ServerConfig};
use crate::mcp::runtime::{Job, Retry, ServerView, Status};

fn discover_fixture() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mcp_discover_fixture.sh")
}

/// A stdio server of `mode` (`tests/support/mcp_discover_fixture.sh`), its
/// frames logged to `log`.
fn discover_server(mode: &str, log: &std::path::Path) -> ServerConfig {
    let mut server = ServerConfig::local(
        "probe",
        "/bin/sh",
        &[discover_fixture().to_string_lossy().into_owned()],
    );
    server.env = vec![
        ("MCP_DISCOVER_MODE".into(), mode.into()),
        ("MCP_FIXTURE_LOG".into(), log.to_string_lossy().into_owned()),
    ];
    server
}

fn wire(log: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("a frame"))
        .collect()
}

fn methods(frames: &[Value]) -> Vec<String> {
    frames
        .iter()
        .map(|frame| frame["method"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The 2026 `_meta` of request `id`, as fx writes it with xfx as the client
/// (`meta`, `era.zig:459-474`).
fn modern_meta(id: u64) -> Value {
    json!({
        "progressToken": id,
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "xfx", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// One try only, so a server that cannot come up is `Failed` at once.
const ONCE: Retry = Retry {
    attempts: 1,
    first: Duration::from_millis(1),
    cap: Duration::from_millis(1),
};

#[test]
fn a_stdio_server_is_asked_server_discover_first_and_every_request_leads_with_its_progress_token() {
    // `p5-mcp-wire-new`'s frames, fx v0.0.16's against a server that does not
    // know `server/discover`: the probe, then the legacy handshake, ids from
    // 1, `_meta.progressToken` on every request, `"params":{}` on
    // `notifications/initialized` (`writeDiscover`, `writeInitialize`,
    // `writeInitialized`, `era.zig:476-496`).
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = super::fixture_server("fixture");
    server.env = vec![("MCP_FIXTURE_LOG".into(), log.to_string_lossy().into_owned())];
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "fixture");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    runtime
        .call_tool("mcp_fixture_echo", json!({"text": "hi"}), None)
        .expect("called");
    let frames = wire(&log);
    let version = env!("CARGO_PKG_VERSION");
    let expected = [
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"server/discover\",\"params\":{{\"_meta\":{}}}}}",
            modern_meta(1)
        ),
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"initialize\",\"params\":{{\"_meta\":{{\"progressToken\":2}},\
             \"protocolVersion\":\"2025-11-25\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"xfx\",\"version\":\"{version}\"}}}}}}"
        ),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"_meta":{"progressToken":3}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"_meta":{"progressToken":4},"name":"echo","arguments":{"text":"hi"}}}"#.to_string(),
    ];
    let got: Vec<String> = frames.iter().map(Value::to_string).collect();
    assert_eq!(got, expected);
    runtime.shutdown();
}

#[test]
fn a_2026_stdio_server_is_served_in_its_era_with_no_initialize() {
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    config::add_profile_server(&home.profile(), discover_server("modern", &log)).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "probe");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    assert_eq!(view.tool_count, Some(1));
    let (_, _, result) = runtime
        .call_tool("mcp_probe_echo", json!({"text": "hi"}), None)
        .expect("called");
    assert_eq!(result["content"][0]["text"], "DISCOVER_modern:hi");
    // The fixture asked `ping` before it answered the call: a 2026 server
    // sends no requests, and xfx answers none (`src/mcp/server.zig:1152-1153`).
    // An answer would be written as the ping is read, before the call's
    // answer is: the fixture would log it ahead of the next request.
    runtime
        .call_tool("mcp_probe_echo", json!({"text": "again"}), None)
        .expect("called again");
    let frames = wire(&log);
    assert_eq!(
        methods(&frames),
        ["server/discover", "tools/list", "tools/call", "tools/call"]
    );
    assert_eq!(frames[1]["params"], json!({"_meta": modern_meta(2)}));
    assert_eq!(
        frames[2]["params"],
        json!({"_meta": modern_meta(3), "name": "echo", "arguments": {"text": "hi"}})
    );
    runtime.shutdown();
}

#[test]
fn a_stdio_server_that_speaks_only_2024_11_05_is_refused_in_fxs_words() {
    // `unsupported_version` (`era.zig:244-251`): "fx speaks 2025-03-26 and
    // newer" (`src/core/mcp_host/runtime.zig:2296`).
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    config::add_profile_server(&home.profile(), discover_server("old2024", &log)).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(Retry {
        attempts: 2,
        ..ONCE
    });
    runtime.start();
    let view = settled(&runtime, "probe");
    assert_eq!(view.status(), Status::Failed, "{view:?}");
    assert_eq!(
        view.failure.as_deref(),
        Some(
            "it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)"
        )
    );
    assert_eq!(view.protocol_version, None);
    // Two tries, two processes, and the ids run on: fx's request table is the
    // server's (`p5-mcp-discover-stdio-old2024`'s wire).
    let frames = wire(&log);
    assert_eq!(
        methods(&frames),
        [
            "server/discover",
            "initialize",
            "server/discover",
            "initialize"
        ]
    );
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame["id"].as_u64())
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), Some(4)]
    );
    runtime.shutdown();
}

#[test]
fn a_stdio_server_that_never_answers_the_probe_is_initialized_once_the_probe_times_out() {
    // fx's probe waits 10 s (`probe_timeout_ms`, `era.zig:25`), then falls
    // back to `initialize` (`probe_timer_fired`, `:239-243`).
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    config::add_profile_server(&home.profile(), discover_server("silent", &log)).expect("saved");
    let runtime = home.runtime();
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "probe");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    // Not before the probe's 10 s; how long after is the start's to bound
    // (`probe_wait_is_its_timer_cut_to_what_is_left_of_the_start`).
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(9_900), "{took:?}");
    // The abandoned probe is still in flight, not yet cancelled: fx cancels
    // it only at its request's idle timeout, 60 s here
    // (`a_stdio_probe_given_up_on_is_cancelled_at_its_request_timeout`).
    assert_eq!(runtime.probes_open("probe"), 1);
    assert_eq!(
        methods(&wire(&log)),
        [
            "server/discover",
            "initialize",
            "notifications/initialized",
            "tools/list"
        ]
    );
    runtime.shutdown();
}

#[test]
fn a_stdio_probe_given_up_on_is_cancelled_at_its_request_timeout() {
    // fx leaves a probe whose timer fired in its request table, and cancels
    // it when the request times out -- after the server's
    // `operation_timeout_ms` (`idle_ms`, `src/mcp/core/request.zig:64-66`;
    // `timerFired`, `:296-304`; `send_cancel`, `src/mcp/server.zig:1983-1992`)
    // -- with the frame fx writes (`writeCancel`).
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = discover_server("silent", &log);
    server.operation_timeout_ms = 2_000;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.set_probe_timeout(Duration::from_millis(300));
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "probe");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    // fx's frame, byte for byte, as the fixture read it.
    let cancel = r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#;
    let deadline = Instant::now() + WAIT;
    let frames = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.lines().any(|line| line == cancel) {
            break wire(&log);
        }
        assert!(Instant::now() < deadline, "never cancelled: {text}");
        std::thread::sleep(Duration::from_millis(20));
    };
    // Not at the probe's timer: at the request's.
    assert!(started.elapsed() >= Duration::from_millis(2_000));
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["method"] == "notifications/cancelled")
            .count(),
        1,
        "{frames:?}"
    );
    let open = Instant::now() + WAIT;
    while runtime.probes_open("probe") != 0 {
        assert!(Instant::now() < open, "the probe stayed open");
        std::thread::sleep(Duration::from_millis(20));
    }
    runtime.shutdown();
}

#[test]
fn the_probe_and_initialize_share_the_start_so_a_start_never_outlasts_its_timeout() {
    // The probe's 10 s are cut to what is left of the start (`startLeft`,
    // `src/mcp/server.zig:966-971`), and initialize gets the rest: a silent
    // probe on a 1.5 s start leaves initialize nothing, and the start fails
    // within it.
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = discover_server("silent", &log);
    server.startup_timeout_ms = 1_500;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONCE);
    // A probe timer far past the start: uncut, it would hold the start for
    // ten minutes, past `settled`'s minute.
    runtime.set_probe_timeout(Duration::from_secs(600));
    runtime.start();
    let view = settled(&runtime, "probe");
    assert_eq!(view.status(), Status::Failed, "{view:?}");
    assert_eq!(
        view.failure.as_deref(),
        Some("it refused or never answered xfx's initialize request")
    );
    runtime.shutdown();
}

/// What may reach a terminal of what a server said: nothing that is a
/// control character.
fn assert_inert(what: &str, text: &str) {
    assert!(
        !text.chars().any(|c| c.is_control() && c != '\n'),
        "{what} holds a control character: {text:?}"
    );
    assert!(
        !text.contains("pwned"),
        "{what} holds the server's words: {text:?}"
    );
}

fn assert_view_inert(view: &ServerView) {
    assert_inert("the failure", view.failure.as_deref().unwrap_or_default());
    assert_inert(
        "the version",
        view.protocol_version.as_deref().unwrap_or_default(),
    );
    if let Some(version) = &view.protocol_version {
        assert!(
            ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"].contains(&version.as_str()),
            "a version not the client's own: {version:?}"
        );
    }
}

/// Starts a stdio discover server of `mode` on a 10 s start with a probe
/// timer of `probe`, one try only, and returns how it settled and how long
/// that took; what `/mcp show` then says is held to [`assert_inert`].
fn start_once(mode: &str, probe: Duration) -> (ServerView, Duration, Vec<Value>) {
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = discover_server(mode, &log);
    server.startup_timeout_ms = 10_000;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONCE);
    runtime.set_probe_timeout(probe);
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "probe");
    let took = started.elapsed();
    assert_view_inert(&view);
    bounded_run(&runtime, Job::Command("show probe".into()));
    for notice in notices(&runtime) {
        assert_inert(mode, &notice);
    }
    runtime.shutdown();
    (view, took, wire(&log))
}

#[test]
fn a_hostile_discover_answer_neither_crashes_nor_hangs_a_start_nor_reaches_the_screen() {
    // Each answer, its end state, and what it says: fx's detection
    // (`classifyDiscover`, `era.zig:386-405`; `onRefusedLine`,
    // `src/mcp/server.zig:769-790`; an error without an id, `:1159-1160`).
    //
    // Answers the probe reads at once: under a probe timer of ten minutes,
    // cut to the 10 s start, one that were waited on would eat the start and
    // end in other words -- or past `settled`'s minute.
    let at_once: [(&str, Status, Option<&str>); 10] = [
        ("malformed", Status::Ready, None),
        ("hostile", Status::Failed, Some("it supports no MCP version xfx speaks")),
        ("idless", Status::Ready, None),
        // 2584 levels in all, as deep as fx's decoder reads (`scan_stack_bytes`,
        // `src/mcp/protocol/wire.zig:100-101`): a version list with none.
        ("deep:2582", Status::Failed, Some("it supports no MCP version xfx speaks")),
        ("rejected", Status::Failed, Some("it refused xfx's MCP version")),
        ("legacyonly", Status::Ready, None),
        ("old2024", Status::Failed, Some("it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)")),
        ("asks", Status::Ready, None),
        // Versions are compared as written: one with an escape is another
        // version (`plainString`, `src/mcp/core/era.zig:370-377`, `:425`).
        ("escaped", Status::Failed, Some("it supports no MCP version xfx speaks")),
        // A field named twice says nothing (`objectFields`,
        // `src/mcp/protocol/wire.zig:142-146`): no version list, a legacy answer.
        ("doubled", Status::Ready, None),
    ];
    for (mode, status, words) in at_once {
        let (view, _, _) = start_once(mode, Duration::from_secs(600));
        assert_eq!(view.status(), status, "{mode}: {view:?}");
        assert_eq!(view.failure.as_deref(), words, "{mode}");
        if status == Status::Ready {
            assert_eq!(
                view.protocol_version.as_deref(),
                Some("2025-06-18"),
                "{mode}"
            );
        }
    }
    // Answers that are none: a line that is not JSON, or nested deeper than
    // fx's decoder reads -- kept aside, as fx keeps them (`keepStray`) -- and
    // a modern server's silence on its second probe. Each waits out one
    // probe timer, and not less.
    let waited: [(&str, Status, Option<&str>); 4] = [
        ("garbage", Status::Ready, None),
        ("deep:2583", Status::Ready, None),
        ("deep", Status::Ready, None),
        (
            "quiet",
            Status::Failed,
            Some("it stopped answering while starting"),
        ),
    ];
    let probe = Duration::from_secs(3);
    for (mode, status, words) in waited {
        let (view, took, _) = start_once(mode, probe);
        assert!(took >= probe, "{mode}: {took:?}");
        assert_eq!(view.status(), status, "{mode}: {view:?}");
        assert_eq!(view.failure.as_deref(), words, "{mode}");
        if status == Status::Ready {
            assert_eq!(
                view.protocol_version.as_deref(),
                Some("2025-06-18"),
                "{mode}"
            );
        }
    }
}

#[test]
fn a_2026_server_that_writes_a_banner_before_its_answer_is_still_read() {
    // A line that is not JSON-RPC and names no request is kept aside
    // (`onRefusedLine`, `keepStray`, `src/mcp/server.zig:772-776`): the probe
    // still waits for its answer, which follows. Under a ten-minute probe
    // timer a lost answer would eat the start.
    let (view, _, frames) = start_once("banner", Duration::from_secs(600));
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    assert_eq!(methods(&frames), ["server/discover", "tools/list"]);
}

#[test]
fn a_servers_request_is_answered_only_once_initialize_has_begun_a_2025_session() {
    // fx answers a server's request only in a 2025 session (`.request => if
    // (r.legacyEra())`, `src/mcp/server.zig:1154`; `legacyEra`, `:888-891`):
    // the `ping` before the probe's answer is dropped, the one before
    // initialize's is answered. Each answer is written as its ping is read,
    // before the answer after it is: the fixture logs it ahead of the next
    // request.
    let (view, _, frames) = start_once("asks", Duration::from_secs(600));
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    let got: Vec<String> = frames.iter().map(Value::to_string).collect();
    assert_eq!(got.len(), 5, "{got:?}");
    assert_eq!(
        methods(&frames),
        [
            "server/discover",
            "initialize",
            "",
            "notifications/initialized",
            "tools/list"
        ]
    );
    assert_eq!(got[2], r#"{"jsonrpc":"2.0","id":"q2","result":{}}"#);
}

#[test]
fn probe_wait_is_its_timer_cut_to_what_is_left_of_the_start() {
    // `startLeft` (`src/mcp/server.zig:966-971`): the probe's 10 s on a start
    // with more left, what is left on one with less.
    use crate::mcp::runtime::probe_wait;
    let ten = Duration::from_secs(10);
    assert_eq!(probe_wait(ten, Duration::from_secs(29)), ten);
    assert_eq!(
        probe_wait(ten, Duration::from_millis(1_500)),
        Duration::from_millis(1_500)
    );
    assert_eq!(probe_wait(ten, Duration::ZERO), Duration::ZERO);
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// One request as a discover HTTP fixture saw it: the JSON-RPC method, the
/// session id, and the `MCP-Protocol-Version`, `Mcp-Method` and `Mcp-Name`
/// headers.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    method: String,
    session: Option<String>,
    version: Option<String>,
    mcp_method: Option<String>,
    mcp_name: Option<String>,
}

fn seen(
    method: &str,
    session: Option<&str>,
    version: Option<&str>,
    mcp_method: Option<&str>,
) -> Seen {
    Seen {
        method: method.into(),
        session: session.map(str::to_string),
        version: version.map(str::to_string),
        mcp_method: mcp_method.map(str::to_string),
        mcp_name: None,
    }
}

/// A discover HTTP fixture: its port, what it was asked, and its
/// connections -- how many the client opened, and the last JSON-RPC method
/// of each that has ended, in the order they ended.
struct Http {
    port: u16,
    log: Arc<Mutex<Vec<Seen>>>,
    opened: Arc<AtomicUsize>,
    ended: Arc<Mutex<Vec<String>>>,
}

impl Http {
    fn seen(&self) -> Vec<Seen> {
        self.log.lock().unwrap().clone()
    }

    fn methods(&self) -> Vec<String> {
        self.seen().into_iter().map(|s| s.method).collect()
    }

    fn ended(&self) -> Vec<String> {
        self.ended.lock().unwrap().clone()
    }

    /// Waits, at most [`WAIT`], until `done` holds of the connections that
    /// have ended.
    fn wait_ended(&self, what: &str, done: impl Fn(&[String]) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            let ended = self.ended();
            if done(&ended) {
                return;
            }
            assert!(Instant::now() < deadline, "{what}: {ended:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Waits until every connection the client opened has ended: after a
    /// close, the point past which nothing more comes -- a session's
    /// `DELETE` included, which goes out before the connection's client is
    /// let go of.
    fn wait_all_ended(&self) {
        self.wait_ended("connections left open", |ended| {
            ended.len() == self.opened.load(Ordering::Acquire)
        });
    }
}

/// Counts a connection's end, with the last method it carried.
struct Ends {
    ended: Arc<Mutex<Vec<String>>>,
    last: String,
}

impl Drop for Ends {
    fn drop(&mut self) {
        self.ended
            .lock()
            .unwrap()
            .push(std::mem::take(&mut self.last));
    }
}

/// A loopback HTTP server of `mode`, answering as each kind of server meets
/// the probe:
/// - `legacy`: discover 404 with no body (fx's legacy remote fixture), then a
///   2025 server with a session;
/// - `modern`: discover lists 2026-07-28; no session; every request must carry
///   the 2026 `_meta` and version header (else 400 -32602);
/// - `notimpl`: `modern`, but discover is 404 with -32601;
/// - `old2024`: `legacy`, initialize answered with 2024-11-05;
/// - `malformed`: discover 200 with JSON that is no valid JSON-RPC, then
///   `legacy`;
/// - `slam`: discover's connection closed with no answer, then `legacy`;
/// - `request`: discover answered with a request of the server's own, which
///   answers nothing, then `legacy`;
/// - `hang`: discover held open, never answered, then `legacy`;
/// - `ssehold`: `modern`, discover answered by one event on a stream held
///   open after it;
/// - `sseidless`: `legacy`, discover answered on a stream by an error with no
///   id;
/// - `ssestrid`: `modern`, discover's answer on a stream naming its id as a
///   string of digits;
/// - `ssefail`: `modern`, discover's answer on a stream under a 500;
/// - `redirect`: `legacy`, discover answered with a 307 to `location`.
fn discover_http(mode: &'static str) -> Http {
    discover_http_to(mode, String::new())
}

fn discover_http_to(mode: &'static str, location: String) -> Http {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let http = Http {
        port,
        log: Arc::new(Mutex::new(Vec::new())),
        opened: Arc::new(AtomicUsize::new(0)),
        ended: Arc::new(Mutex::new(Vec::new())),
    };
    let (log, opened, ended) = (
        Arc::clone(&http.log),
        Arc::clone(&http.opened),
        Arc::clone(&http.ended),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            opened.fetch_add(1, Ordering::AcqRel);
            let log = Arc::clone(&log);
            let mut ends = Ends {
                ended: Arc::clone(&ended),
                last: String::new(),
            };
            let location = location.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                loop {
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    let mut entry = Seen {
                        method: String::new(),
                        session: None,
                        version: None,
                        mcp_method: None,
                        mcp_name: None,
                    };
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0 {
                            return;
                        }
                        let header = header.trim_end();
                        if header.is_empty() {
                            break;
                        }
                        let (name, value) = header.split_once(':').expect("a header");
                        let value = value.trim().to_string();
                        match name.to_ascii_lowercase().as_str() {
                            "content-length" => length = value.parse().unwrap(),
                            "mcp-session-id" => entry.session = Some(value),
                            "mcp-protocol-version" => entry.version = Some(value),
                            "mcp-method" => entry.mcp_method = Some(value),
                            "mcp-name" => entry.mcp_name = Some(value),
                            _ => {}
                        }
                    }
                    let mut body = vec![0u8; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    if !request_line.starts_with("POST") {
                        // A session's end (`DELETE`) or a listen (`GET`),
                        // logged by its HTTP method.
                        entry.method = request_line
                            .split(' ')
                            .next()
                            .unwrap_or_default()
                            .to_string();
                        ends.last.clone_from(&entry.method);
                        log.lock().unwrap().push(entry);
                        let _ = stream.write_all(
                            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n",
                        );
                        continue;
                    }
                    let frame: Value = serde_json::from_slice(&body).expect("json");
                    let method = frame["method"].as_str().unwrap_or_default().to_string();
                    entry.method = method.clone();
                    ends.last.clone_from(&method);
                    let modern_ok = entry.version.as_deref() == Some("2026-07-28")
                        && frame["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"]
                            == "2026-07-28";
                    log.lock().unwrap().push(entry);
                    let id = frame["id"].clone();
                    let modern = matches!(
                        mode,
                        "modern" | "notimpl" | "ssehold" | "ssestrid" | "ssefail"
                    );
                    let answer = |status: &str, extra: &str, payload: String| {
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{payload}",
                            payload.len()
                        )
                    };
                    let discovered = |id: Value| json!({"jsonrpc": "2.0", "id": id, "result": {"resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}}});
                    // An event stream with no length: its body ends with the
                    // connection.
                    let stream_of = |status: &str, event: Value| {
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {event}\n\n"
                        )
                    };
                    let response = match method.as_str() {
                        "server/discover" => match mode {
                            "modern" => answer("200 OK", "", discovered(id).to_string()),
                            "notimpl" => answer("404 Not Found", "", json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}).to_string()),
                            "malformed" => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "result": "\u{1b}]0;pwned\u{7}"}).to_string()),
                            "slam" => return,
                            "request" => answer(
                                "200 OK",
                                "",
                                json!({"jsonrpc": "2.0", "id": 9, "method": "ping"}).to_string(),
                            ),
                            "hang" | "ssehold" => {
                                if mode == "ssehold" {
                                    let _ = stream.write_all(stream_of("200 OK", discovered(id)).as_bytes());
                                    let _ = stream.flush();
                                }
                                // Held until the client lets go of it.
                                let mut rest = [0u8; 1];
                                let _ = reader.read(&mut rest);
                                return;
                            }
                            "sseidless" | "ssestrid" | "ssefail" => {
                                let (status, event) = match mode {
                                    "sseidless" => ("200 OK", json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32601, "message": "Method not found"}})),
                                    "ssestrid" => ("200 OK", discovered(Value::from(id.to_string()))),
                                    _ => ("500 Internal Server Error", discovered(id)),
                                };
                                let _ = stream.write_all(stream_of(status, event).as_bytes());
                                return;
                            }
                            "redirect" => format!(
                                "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\n\r\n"
                            ),
                            _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string(),
                        },
                        _ if id.is_null() => "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".to_string(),
                        "initialize" if modern => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}).to_string()),
                        "initialize" => answer(
                            "200 OK",
                            "Mcp-Session-Id: s-1\r\n",
                            json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": if mode == "old2024" { "2024-11-05" } else { "2025-06-18" }, "capabilities": {"tools": {}}, "serverInfo": {"name": "discover-http", "version": "1"}}}).to_string(),
                        ),
                        _ if modern && !modern_ok => answer("400 Bad Request", "", json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "Missing modern request metadata"}}).to_string()),
                        "tools/list" => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "result": {"resultType": "complete", "tools": [{"name": "echo", "description": "Echo over HTTP", "inputSchema": {"type": "object"}}]}}).to_string()),
                        "tools/call" => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "result": {"resultType": "complete", "content": [{"type": "text", "text": "HTTP_ECHO"}]}}).to_string()),
                        _ => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}).to_string()),
                    };
                    if stream.write_all(response.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    http
}

fn http_runtime(home: &Home, port: u16) -> Arc<crate::mcp::McpRuntime> {
    config::add_profile_server(
        &home.profile(),
        ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp")),
    )
    .expect("saved");
    home.runtime()
}

const LEGACY_START: [&str; 4] = [
    "server/discover",
    "initialize",
    "notifications/initialized",
    "tools/list",
];

#[test]
fn a_2026_http_server_is_probed_with_its_version_headers_and_served_without_a_session() {
    // `requestHeaders`: `MCP-Protocol-Version`, `Mcp-Method` and, for a call,
    // `Mcp-Name` (`src/mcp/protocol/http_headers.zig:289-307`).
    let http = discover_http("modern");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    let (_, _, result) = runtime
        .call_tool("mcp_remote_echo", json!({}), None)
        .expect("called");
    assert_eq!(result["content"][0]["text"], "HTTP_ECHO");
    let mut call = seen("tools/call", None, Some("2026-07-28"), Some("tools/call"));
    call.mcp_name = Some("echo".into());
    assert_eq!(
        http.seen(),
        [
            seen(
                "server/discover",
                None,
                Some("2026-07-28"),
                Some("server/discover")
            ),
            seen("tools/list", None, Some("2026-07-28"), Some("tools/list")),
            call,
        ]
    );
    runtime.shutdown();
}

#[test]
fn a_404_with_method_not_found_is_a_2026_server_without_discovery() {
    // `discover_not_implemented` (`classifyDiscoverHttp`, `era.zig:411-415`):
    // served at the probe's version.
    let http = discover_http("notimpl");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    assert_eq!(view.tool_count, Some(1));
    assert_eq!(http.methods(), ["server/discover", "tools/list"]);
    runtime.shutdown();
}

#[test]
fn a_bare_404_is_a_2025_server_whose_initialize_carries_no_version_header() {
    // A 2025 session: initialize with `Accept` alone, then the agreed version
    // and the session id on every POST (`sessionHeaders`,
    // `http_headers.zig:257-273`; `sendInitialize`, `src/mcp/server.zig:
    // 1472-1486`).
    let http = discover_http("legacy");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert_eq!(
        http.seen(),
        [
            seen(
                "server/discover",
                None,
                Some("2026-07-28"),
                Some("server/discover")
            ),
            seen("initialize", None, None, None),
            seen(
                "notifications/initialized",
                Some("s-1"),
                Some("2025-06-18"),
                None
            ),
            seen("tools/list", Some("s-1"), Some("2025-06-18"), None),
        ]
    );
    runtime.shutdown();
    // The session it took is ended at the close, before the connections
    // are.
    http.wait_all_ended();
    let last = http.seen().last().cloned().expect("a request");
    assert_eq!(
        (last.method.as_str(), last.session.as_deref()),
        ("DELETE", Some("s-1"))
    );
}

#[test]
fn an_http_server_that_speaks_only_2024_11_05_is_refused_in_fxs_words() {
    let http = discover_http("old2024");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.set_retry(ONCE);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Failed, "{view:?}");
    assert_eq!(
        view.failure.as_deref(),
        Some(
            "it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)"
        )
    );
    runtime.shutdown();
    // The session initialize offered was never taken, so none is ended: no
    // `DELETE` (`onInitializeAnswer` keeps the incoming id only for an
    // accepted answer, `src/mcp/server.zig:1574-1593`).
    http.wait_all_ended();
    assert_eq!(http.methods(), ["server/discover", "initialize"]);
}

#[test]
fn an_http_body_that_is_no_json_rpc_is_a_legacy_answer_at_once() {
    // fx's client reads JSON that is no valid message as a status without
    // one (`run`, `src/mcp/io/http.zig:469-485`), a legacy answer: initialize
    // follows at once. Under a ten-minute probe timer, one waited out would
    // eat the start.
    let http = discover_http("malformed");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.set_retry(ONCE);
    runtime.set_probe_timeout(Duration::from_secs(600));
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert_view_inert(&view);
    assert_eq!(http.methods(), LEGACY_START);
    runtime.shutdown();
}

#[test]
fn a_probe_whose_connection_breaks_is_sent_once_more_then_waits_out_its_timer() {
    // A broken stream is re-issued once (`max_attempts = 2`,
    // `src/mcp/core/http_modern.zig:21-24`), and then only the probe's timer
    // answers (`probe_timer_fired`): initialize after it. A body whose one
    // message is a request answers nothing either: its stream ends without
    // the answer (`src/mcp/server.zig:1152-1158`).
    for mode in ["slam", "request"] {
        let http = discover_http(mode);
        let home = Home::new();
        let runtime = http_runtime(&home, http.port);
        runtime.set_probe_timeout(Duration::from_millis(600));
        let started = Instant::now();
        runtime.start();
        let view = settled(&runtime, "remote");
        let took = started.elapsed();
        assert_eq!(view.status(), Status::Ready, "{view:?}");
        assert!(took >= Duration::from_millis(550), "{took:?}");
        assert_eq!(
            http.methods(),
            [
                "server/discover",
                "server/discover",
                "initialize",
                "notifications/initialized",
                "tools/list"
            ],
            "{mode}"
        );
        runtime.shutdown();
    }
}

#[test]
fn an_http_probe_the_server_holds_open_stays_open_beside_the_requests_after_it() {
    // Silence over HTTP is a legacy answer once the probe's timer fires
    // (`probe_timer_fired`, `era.zig:239-243`), and fx's `initialize` goes
    // out beside the probe it gave up on, which it leaves open: its request
    // timeout's cancel goes to the 2025 session, which does not know it
    // (`server.zig:1997-2000`; `http_legacy.zig:288-289`), so it ends when
    // the server or the connection's close ends it.
    let http = discover_http("hang");
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.set_probe_timeout(Duration::from_millis(400));
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "remote");
    let took = started.elapsed();
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert!(took >= Duration::from_millis(350), "{took:?}");
    assert_eq!(runtime.probes_open("remote"), 1);
    // The next request goes and comes back while the probe is open.
    let (_, _, result) = runtime
        .call_tool("mcp_remote_echo", json!({}), None)
        .expect("called beside the open probe");
    assert_eq!(result["content"][0]["text"], "HTTP_ECHO");
    assert_eq!(runtime.probes_open("remote"), 1);
    let mut methods = LEGACY_START.to_vec();
    methods.push("tools/call");
    assert_eq!(http.methods(), methods);
    runtime.shutdown();
    // The connection's close ends it.
    http.wait_ended("the probe was never let go of", |ended| {
        ended.iter().any(|method| method == "server/discover")
    });
}

#[test]
fn an_abandoned_http_probe_ends_at_its_request_timeout_only_outside_a_session() {
    // fx's request timeout for a probe given up on (`operation_timeout_ms`,
    // `idle_ms`, `src/mcp/core/request.zig:64-66`): with no 2025 session
    // begun, the probe's stream is closed (`stepExchange(l, .cancel)`,
    // `src/mcp/server.zig:1997-2000`); in one, the cancel goes to the
    // session and the stream stays open until the connection closes.
    use crate::mcp::era::Meta;
    use crate::mcp::transport::{HttpTransport, Transport, WireEra};
    let idle = Duration::from_millis(300);
    for session in [false, true] {
        let http = discover_http("hang");
        let url = format!("http://127.0.0.1:{}/mcp", http.port);
        let transport = HttpTransport::open(&url, &[])
            .expect("open")
            .with_idle(idle);
        if session {
            transport.set_era(WireEra::Initializing);
        }
        let started = Instant::now();
        let probe = transport.detect(
            "server/discover",
            json!({}),
            &Meta::Modern("2026-07-28"),
            Duration::from_millis(100),
            None,
        );
        assert_eq!(
            probe,
            Err(crate::mcp::McpError::Named("McpRequestTimedOut"))
        );
        if !session {
            http.wait_ended("the probe outlived its request timeout", |ended| {
                ended.iter().any(|method| method == "server/discover")
            });
            assert!(started.elapsed() >= idle, "{:?}", started.elapsed());
            let deadline = Instant::now() + WAIT;
            while transport.probes_open() != 0 {
                assert!(Instant::now() < deadline, "the probe stayed counted");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(transport.alive());
            transport.close();
            continue;
        }
        // Well past its timeout, still open: a request sent after that
        // timeout comes back while the probe's connection has not ended.
        while started.elapsed() < idle * 3 {
            std::thread::sleep(Duration::from_millis(20));
        }
        transport
            .request("tools/list", json!({}), WAIT, None)
            .expect("answered beside the probe");
        assert_eq!(transport.probes_open(), 1);
        assert_eq!(http.methods(), ["server/discover", "tools/list"]);
        assert!(!http
            .ended()
            .iter()
            .any(|method| method == "server/discover"));
        transport.close();
        http.wait_ended("the close left the probe open", |ended| {
            ended.iter().any(|method| method == "server/discover")
        });
    }
}

#[test]
fn a_request_whose_caller_stops_waiting_has_its_post_closed() {
    // Outside a 2025 session fx closes a request's stream when its timeout
    // fires (`stepExchange(l, .cancel)`, `closeKey`, `src/mcp/server.zig:
    // 1997-2003`, `:1440-1447`). The fixture holds any `server/discover`
    // POST; sent here as an operation, it is one the caller waits 200 ms for.
    use crate::mcp::transport::{HttpTransport, Transport};
    let http = discover_http("hang");
    let url = format!("http://127.0.0.1:{}/mcp", http.port);
    let transport = HttpTransport::open(&url, &[]).expect("open");
    assert_eq!(
        transport.request(
            "server/discover",
            json!({}),
            Duration::from_millis(200),
            None
        ),
        Err(crate::mcp::McpError::Named("McpRequestTimedOut"))
    );
    http.wait_ended("the POST outlived its caller", |ended| {
        ended.iter().any(|method| method == "server/discover")
    });
    assert_eq!(transport.probes_open(), 0);
    transport.close();
}

#[test]
fn a_probe_answered_on_an_event_stream_is_read_as_its_events_come() {
    // fx reads each event as it comes and takes the first that answers the
    // POST (`readEvents`, `src/mcp/io/http.zig:643-692`): an answer on a
    // stream held open (`ssehold`), one naming its id as a string of digits
    // (`ownId`, `wire.zig:305-313`), one under an error status (an event
    // stream's answers are read whatever the status), and an error with no
    // id, which answers the POST it came on (`src/mcp/server.zig:
    // 1155-1158`) -- a legacy answer. Under a ten-minute probe timer, an
    // answer missed would eat the start.
    for (mode, version, methods) in [
        (
            "ssehold",
            "2026-07-28",
            &["server/discover", "tools/list"][..],
        ),
        (
            "ssestrid",
            "2026-07-28",
            &["server/discover", "tools/list"][..],
        ),
        (
            "ssefail",
            "2026-07-28",
            &["server/discover", "tools/list"][..],
        ),
        ("sseidless", "2025-06-18", &LEGACY_START[..]),
    ] {
        let http = discover_http(mode);
        let home = Home::new();
        let runtime = http_runtime(&home, http.port);
        runtime.set_retry(ONCE);
        runtime.set_probe_timeout(Duration::from_secs(600));
        runtime.start();
        let view = settled(&runtime, "remote");
        assert_eq!(view.status(), Status::Ready, "{mode}: {view:?}");
        assert_eq!(view.protocol_version.as_deref(), Some(version), "{mode}");
        assert_eq!(http.methods(), methods, "{mode}");
        assert_eq!(runtime.probes_open("remote"), 0, "{mode}");
        runtime.shutdown();
    }
}

#[test]
fn a_probe_redirected_elsewhere_is_not_followed() {
    // Redirects are never followed (`RedirectNotAllowed`; fx's client reads
    // a 3xx as a status: `run`, `src/mcp/io/http.zig:417-487`): a 307 with no
    // answer is a legacy answer, and the place it points to is never asked.
    let elsewhere = TcpListener::bind("127.0.0.1:0").expect("bind");
    elsewhere.set_nonblocking(true).expect("nonblocking");
    let location = format!(
        "http://127.0.0.1:{}/mcp",
        elsewhere.local_addr().expect("addr").port()
    );
    let http = discover_http_to("redirect", location);
    let home = Home::new();
    let runtime = http_runtime(&home, http.port);
    runtime.set_retry(ONCE);
    runtime.set_probe_timeout(Duration::from_secs(600));
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert_eq!(http.methods(), LEGACY_START);
    runtime.shutdown();
    http.wait_all_ended();
    // A connection made to it would wait in its backlog.
    assert_eq!(
        elsewhere.accept().map(|_| ()).map_err(|err| err.kind()),
        Err(std::io::ErrorKind::WouldBlock)
    );
}

#[test]
fn past_32_exchanges_in_flight_a_message_waits_and_past_32_waiting_it_is_refused() {
    // fx's client runs at most 32 requests at once, lets 32 more wait for a
    // slot, oldest first, and refuses the next (`max_in_flight`,
    // `waiting_room`, `claim`, `src/mcp/io/http.zig:26`, `:106-108`,
    // `:218-226`). Here 32 probes the server holds open fill the slots.
    use crate::mcp::era::Meta;
    use crate::mcp::transport::{HttpTransport, Transport};
    use crate::mcp::McpError;
    let http = discover_http("hang");
    let url = format!("http://127.0.0.1:{}/mcp", http.port);
    let transport = HttpTransport::open(&url, &[]).expect("open");
    for _ in 0..32 {
        let probe = transport.detect(
            "server/discover",
            json!({}),
            &Meta::Modern("2026-07-28"),
            Duration::from_millis(1),
            None,
        );
        assert_eq!(probe, Err(McpError::Named("McpRequestTimedOut")));
    }
    let short = Duration::from_millis(1);
    for n in 33..=64 {
        assert_eq!(
            transport.request("tools/list", json!({}), short, None),
            Err(McpError::Named("McpRequestTimedOut")),
            "message {n} waits"
        );
    }
    assert_eq!(
        transport.request("tools/list", json!({}), Duration::from_secs(60), None),
        Err(McpError::Named("McpTooManyInFlight"))
    );
    let deadline = Instant::now() + WAIT;
    while transport.probes_open() != 32 {
        assert!(
            Instant::now() < deadline,
            "{} probes open",
            transport.probes_open()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    transport.close();
}
