//! Version detection against real servers: fx v0.0.16 asks `server/discover`
//! before anything else (`vercel-labs/fx@481104fd src/mcp/core/era.zig`), on
//! stdio and over HTTP, and these cases hold xfx to what fx does with each
//! kind of answer -- the frames it sends, the version it ends on, the words
//! it fails with -- and to its bounds: a hostile answer neither crashes nor
//! hangs a start, and nothing of it reaches the screen.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
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
    // An answer would be the fixture's next line, logged once it reads it.
    std::thread::sleep(Duration::from_millis(500));
    let frames = wire(&log);
    assert_eq!(
        methods(&frames),
        ["server/discover", "tools/list", "tools/call"]
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
    std::thread::sleep(Duration::from_secs(2));
    let early = runtime
        .views()
        .into_iter()
        .find(|view| view.name == "probe")
        .expect("listed");
    assert_eq!(early.status(), Status::Connecting, "{early:?}");
    assert_eq!(methods(&wire(&log)), ["server/discover"]);
    let view = settled(&runtime, "probe");
    let took = started.elapsed();
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert!(took >= Duration::from_millis(9_900), "{took:?}");
    assert!(took < Duration::from_secs(20), "{took:?}");
    // The abandoned probe is not cancelled on the wire.
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
fn the_probe_and_initialize_share_the_start_so_a_start_never_outlasts_its_timeout() {
    // The probe's 10 s are cut to what is left of the start (`startLeft`,
    // `src/mcp/server.zig:913-916`), and initialize gets the rest: a silent
    // probe on a 1.5 s start leaves initialize nothing, and the start fails
    // within it.
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = discover_server("silent", &log);
    server.startup_timeout_ms = 1_500;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONCE);
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "probe");
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "{:?}",
        started.elapsed()
    );
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

#[test]
fn a_hostile_discover_answer_neither_crashes_nor_hangs_a_start_nor_reaches_the_screen() {
    // Each answer, its end state, and what it says: fx's detection
    // (`classifyDiscover`, `era.zig:386-405`; `onRefusedLine`,
    // `src/mcp/server.zig:769-789`; an error without an id, `:1159-1160`).
    // The probes each case waits out: none for an answer the probe reads at
    // once, one for silence or a line that is not JSON -- after a modern
    // answer, on its second probe.
    let cases: [(&str, Status, Option<&str>, u32); 9] = [
        ("malformed", Status::Ready, None, 0),
        ("hostile", Status::Failed, Some("it supports no MCP version xfx speaks"), 0),
        ("garbage", Status::Ready, None, 1),
        ("idless", Status::Ready, None, 0),
        ("deep", Status::Ready, None, 1),
        ("rejected", Status::Failed, Some("it refused xfx's MCP version"), 0),
        ("legacyonly", Status::Ready, None, 0),
        ("quiet", Status::Failed, Some("it stopped answering while starting"), 1),
        ("old2024", Status::Failed, Some("it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)"), 0),
    ];
    let probe = Duration::from_secs(3);
    for (mode, status, words, waits) in cases {
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
        // Bounded: the probes it waits out and a quick rest -- never the 10 s
        // start, never a hang -- and an answer read at once is not waited on.
        let took = started.elapsed();
        assert!(took >= probe * waits, "{mode}: {took:?}");
        assert!(
            took < probe * waits + Duration::from_millis(2_500),
            "{mode}: {took:?}"
        );
        assert_eq!(view.status(), status, "{mode}: {view:?}");
        assert_eq!(view.failure.as_deref(), words, "{mode}");
        if status == Status::Ready {
            assert_eq!(
                view.protocol_version.as_deref(),
                Some("2025-06-18"),
                "{mode}"
            );
        }
        assert_view_inert(&view);
        // What `/mcp show` says of it holds nothing of the server's either.
        bounded_run(&runtime, Job::Command("show probe".into()));
        for notice in notices(&runtime) {
            assert_inert(mode, &notice);
        }
        runtime.shutdown();
    }
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
/// - `hang`: discover held open, never answered, then `legacy`.
fn discover_http(mode: &'static str) -> (u16, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let log = Arc::clone(&log);
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
                        let _ = stream.write_all(
                            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\n\r\n",
                        );
                        continue;
                    }
                    let frame: Value = serde_json::from_slice(&body).expect("json");
                    let method = frame["method"].as_str().unwrap_or_default().to_string();
                    entry.method = method.clone();
                    let modern_ok = entry.version.as_deref() == Some("2026-07-28")
                        && frame["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"]
                            == "2026-07-28";
                    log.lock().unwrap().push(entry);
                    let id = frame["id"].clone();
                    let modern = matches!(mode, "modern" | "notimpl");
                    let answer = |status: &str, extra: &str, payload: String| {
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{payload}",
                            payload.len()
                        )
                    };
                    let response = match method.as_str() {
                        "server/discover" => match mode {
                            "modern" => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "result": {"resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}}}}).to_string()),
                            "notimpl" => answer("404 Not Found", "", json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}).to_string()),
                            "malformed" => answer("200 OK", "", json!({"jsonrpc": "2.0", "id": id, "result": "\u{1b}]0;pwned\u{7}"}).to_string()),
                            "slam" => return,
                            "hang" => {
                                // Held until the client lets go of it.
                                let mut rest = [0u8; 1];
                                let _ = reader.read(&mut rest);
                                return;
                            }
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
    (port, seen)
}

fn http_runtime(home: &Home, port: u16) -> Arc<crate::mcp::McpRuntime> {
    config::add_profile_server(
        &home.profile(),
        ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp")),
    )
    .expect("saved");
    home.runtime()
}

#[test]
fn a_2026_http_server_is_probed_with_its_version_headers_and_served_without_a_session() {
    // `requestHeaders`: `MCP-Protocol-Version`, `Mcp-Method` and, for a call,
    // `Mcp-Name` (`src/mcp/protocol/http_headers.zig:289-307`).
    let (port, log) = discover_http("modern");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
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
        log.lock().unwrap().clone(),
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
    let (port, log) = discover_http("notimpl");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2026-07-28"));
    assert_eq!(view.tool_count, Some(1));
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .map(|s| s.method.clone())
            .collect::<Vec<_>>(),
        ["server/discover", "tools/list"]
    );
    runtime.shutdown();
}

#[test]
fn a_bare_404_is_a_2025_server_whose_initialize_carries_no_version_header() {
    // A 2025 session: initialize with `Accept` alone, then the agreed version
    // and the session id on every POST (`sessionHeaders`,
    // `http_headers.zig:257-273`; `sendInitialize`, `src/mcp/server.zig:
    // 1472-1486`).
    let (port, log) = discover_http("legacy");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert_eq!(
        log.lock().unwrap().clone(),
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
}

#[test]
fn an_http_server_that_speaks_only_2024_11_05_is_refused_in_fxs_words() {
    let (port, _) = discover_http("old2024");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
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
}

#[test]
fn an_http_body_that_is_no_json_rpc_is_a_legacy_answer_at_once() {
    // fx's client reads JSON that is no valid message as a status without
    // one (`run`, `src/mcp/io/http.zig:469-485`), a legacy answer: initialize
    // follows at once, not after the probe's 10 s.
    let (port, log) = discover_http("malformed");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "remote");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert_view_inert(&view);
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .map(|s| s.method.clone())
            .collect::<Vec<_>>(),
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
fn a_probe_whose_connection_breaks_is_sent_once_more_then_waits_out_its_timer() {
    // A broken stream is re-issued once (`max_attempts = 2`,
    // `src/mcp/core/http_modern.zig:21-24`), and then only the probe's timer
    // answers (`probe_timer_fired`): initialize after it.
    let (port, log) = discover_http("slam");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
    runtime.set_probe_timeout(Duration::from_millis(600));
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "remote");
    let took = started.elapsed();
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert!(took >= Duration::from_millis(550), "{took:?}");
    assert!(took < WAIT, "{took:?}");
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .map(|s| s.method.clone())
            .collect::<Vec<_>>(),
        [
            "server/discover",
            "server/discover",
            "initialize",
            "notifications/initialized",
            "tools/list"
        ]
    );
    runtime.shutdown();
}

#[test]
fn an_http_probe_the_server_holds_open_does_not_hold_up_initialize() {
    // Silence over HTTP is a legacy answer once the probe's timer fires
    // (`probe_timer_fired`, `era.zig:239-243`), and fx's `initialize` goes
    // out beside the probe it gave up on: a POST held open must not keep it.
    let (port, log) = discover_http("hang");
    let home = Home::new();
    let runtime = http_runtime(&home, port);
    runtime.set_probe_timeout(Duration::from_millis(400));
    let started = Instant::now();
    runtime.start();
    let view = settled(&runtime, "remote");
    let took = started.elapsed();
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    assert!(took >= Duration::from_millis(350), "{took:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .map(|s| s.method.clone())
            .collect::<Vec<_>>(),
        [
            "server/discover",
            "initialize",
            "notifications/initialized",
            "tools/list"
        ]
    );
    runtime.shutdown();
}
