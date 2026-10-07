//! The runtime against servers written for these tests: the stdio fixture in
//! `tests/support/mcp_fixture.sh` and a loopback HTTP server in this file.
//! Nothing here reaches the network.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::config::{self, ServerConfig, TrustAction};
use super::runtime::{Connection, Event, Job, McpRuntime, Paths, Status};
use crate::gateway::protocol::ToolCall;
use crate::permission::{PermissionMode, PermissionSession, RuleSet};
use crate::tools::ToolContext;
use crate::workspace::AccessScope;

const WAIT: Duration = Duration::from_secs(60);

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mcp_fixture.sh")
}

fn fixture_server(name: &str) -> ServerConfig {
    ServerConfig::local(name, "/bin/sh", &[fixture().to_string_lossy().into_owned()])
}

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("home");
        std::fs::create_dir_all(dir.path().join("work")).expect("workspace");
        Self { dir }
    }

    fn profile(&self) -> PathBuf {
        self.dir.path().join(".xfx").join("mcp.json")
    }

    fn settings(&self) -> PathBuf {
        self.dir.path().join(".xfx").join("settings.json")
    }

    fn workspace(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    fn runtime(&self) -> Arc<McpRuntime> {
        McpRuntime::new(Paths {
            profile: Some(self.profile()),
            settings: Some(self.settings()),
            workspace: self.workspace(),
        })
    }
}

/// Waits until `name` has left `Connecting`.
fn settled(runtime: &McpRuntime, name: &str) -> super::runtime::ServerView {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(view) = runtime.views().into_iter().find(|view| view.name == name) {
            if view.connection != Connection::Connecting {
                return view;
            }
        }
        assert!(Instant::now() < deadline, "{name} never settled");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn notices(runtime: &McpRuntime) -> Vec<String> {
    runtime
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            Event::Notice { body, .. } => Some(body),
            _ => None,
        })
        .collect()
}

#[test]
fn a_profile_server_is_started_initialized_listed_and_called() {
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = fixture_server("fixture");
    server.env = vec![("MCP_FIXTURE_LOG".into(), log.to_string_lossy().into_owned())];
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "fixture");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    assert_eq!(view.tool_count, Some(1));
    assert_eq!(view.protocol_version.as_deref(), Some("2025-06-18"));
    let (server, tool, result) = runtime
        .call_tool("mcp_fixture_echo", json!({"text": "hi"}), None)
        .expect("called");
    assert_eq!(server.config.name, "fixture");
    assert_eq!(tool.name, "echo");
    assert_eq!(
        result,
        json!({"content": [{"type": "text", "text": "FIXTURE_ECHO:hi"}]})
    );
    // The wire, frame by frame, as upstream writes it
    // (`cap/fx-mcp-wire.ndjson`): xfx asks for no client capability it does
    // not answer.
    let wire: Vec<Value> = std::fs::read_to_string(&log)
        .expect("the fixture's log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a frame"))
        .collect();
    assert_eq!(
        wire[0],
        json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "xfx", "version": env!("CARGO_PKG_VERSION")},
        }})
    );
    assert_eq!(
        wire[1],
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    );
    assert_eq!(
        wire[2],
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
    );
    assert_eq!(
        wire[3].to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":{"progressToken":2},"name":"echo","arguments":{"text":"hi"}}}"#
    );
    runtime.shutdown();
}

#[test]
fn a_server_that_cannot_start_is_tried_once_more_and_then_failed() {
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    let mut server = ServerConfig::local(
        "broken",
        "/bin/sh",
        &[
            "-c".into(),
            "echo started >> \"$1\"; echo 'npm error code E401' >&2; exit 7".into(),
            "sh".into(),
            attempts.to_string_lossy().into_owned(),
        ],
    );
    server.startup_timeout_ms = 20_000;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "broken");
    assert_eq!(view.status(), Status::Failed);
    assert_eq!(view.failure.as_deref(), Some("McpInitFailed"));
    // `restart_limit: 1`: the first start and one restart.
    assert_eq!(
        std::fs::read_to_string(&attempts).unwrap().lines().count(),
        2
    );
    // A startup that failed says so once discovery settles (`health.zig:438-480`).
    let deadline = Instant::now() + WAIT;
    let said = loop {
        let said = notices(&runtime);
        if !said.is_empty() || Instant::now() > deadline {
            break said;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        said,
        ["MCP startup: 1 server needs attention, 1 failed. Use /mcp list for details."]
    );
}

#[test]
fn a_project_server_waits_for_approval_and_the_choice_is_kept_in_settings() {
    let home = Home::new();
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start();
    let events = runtime.take_events();
    assert!(
        events.contains(&Event::Raw(
            "Project MCP server 'proj' is defined in .mcp.json.\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n".into()
        )),
        "{events:?}"
    );
    assert!(events.contains(&Event::TrustPrompt {
        name: "proj".into()
    }));
    let view = settled(&runtime, "proj");
    assert_eq!(view.state_label(), "Pending trust");
    assert_eq!(
        runtime.list_resources("proj", false, None),
        Err(super::McpError::Named("McpWorkspaceApprovalRequired"))
    );
    runtime.run(Job::Trust {
        action: TrustAction::Approve("proj".into()),
        body: "Approving project MCP server 'proj'.".into(),
    });
    assert_eq!(
        notices(&runtime),
        [
            "Approving project MCP server 'proj'.",
            "MCP configuration reloaded successfully."
        ]
    );
    assert_eq!(settled(&runtime, "proj").status(), Status::Ready);
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(home.settings()).unwrap()).unwrap();
    assert_eq!(
        document["workspaces"][home.workspace().to_str().unwrap()]["enabledMcpjsonServers"],
        json!(["proj"])
    );
    runtime.shutdown();
}

#[test]
fn every_feature_command_answers_in_upstreams_shape() {
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    for (line, expected) in [
        (
            "resource list fixture",
            "MCP resources from fixture (1):\n  fixture :: fixture://alpha — alpha\n",
        ),
        (
            "resource read fixture fixture://alpha",
            "[untrusted MCP resource content] fixture :: fixture://alpha\n\nfixture://alpha (text/plain)\nFIXTURE_RESOURCE_TEXT\n",
        ),
        (
            "prompt list fixture",
            "MCP prompts from fixture (1):\n  fixture :: review — Review prompt [tone*]\n",
        ),
        (
            "prompt get fixture review {\"tone\":\"brief\"}",
            "[untrusted MCP prompt content] fixture :: review\nReview prompt\n\nuser (text):\n{\"type\":\"text\",\"text\":\"FIXTURE_PROMPT_TEXT\"}\n",
        ),
        (
            "prompt complete fixture review tone b",
            "MCP completions from fixture (2 of 2):\n  brief\n  thorough\n",
        ),
        (
            "auth fixture",
            "MCP authentication for 'fixture' failed: McpAuthenticationNotRemote.",
        ),
        (
            "logout fixture",
            "MCP logout for 'fixture' failed: McpAuthenticationNotRemote.",
        ),
        (
            "prompt get fixture review [1]",
            "MCP prompt invocation failed: McpInvalidPromptArguments.",
        ),
    ] {
        runtime.run(Job::Command(line.into()));
        assert_eq!(notices(&runtime), [expected], "{line}");
    }
    runtime.shutdown();
}

/// A context over `runtime` in `mode`, with `rules`.
fn context(
    home: &Home,
    runtime: &Arc<McpRuntime>,
    mode: PermissionMode,
    rules: RuleSet,
) -> ToolContext {
    let scope = AccessScope::primary_only(home.workspace()).expect("scope");
    ToolContext::new(scope)
        .with_permissions(PermissionSession::new(mode).with_rules(rules))
        .with_mcp(Arc::clone(runtime))
}

fn call(context: &ToolContext, name: &str, input: Value) -> crate::tools::ToolResult {
    let call = ToolCall {
        id: "c1".into(),
        name: name.into(),
        input,
    };
    match crate::tools::Registry::builtin().spec(name) {
        Some(_) => crate::tools::Registry::builtin()
            .execute(&call, context)
            .expect("advertised"),
        None => crate::tools::mcp::call(&call, context),
    }
}

#[test]
fn a_search_selects_the_tool_and_a_rule_or_full_access_admits_its_call() {
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");

    let auto = context(&home, &runtime, PermissionMode::Auto, RuleSet::default());
    let found = call(
        &auto,
        "capability_search",
        json!({"query": "echo text back"}),
    );
    assert!(found.ok, "{found:?}");
    assert!(
        found
            .output
            .contains(r#""name":"mcp_fixture_echo","server":"fixture""#),
        "{}",
        found.output
    );
    assert!(crate::tools::mcp::is_selected(&auto, "mcp_fixture_echo"));
    let advertised = crate::tools::Registry::builtin().advertisement_for(&auto);
    assert_eq!(
        advertised.last().unwrap().to_string(),
        r#"{"type":"function","name":"mcp_fixture_echo","description":"Echo text back through the test fixture","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}"#
    );

    // `auto` holds it: there is no reviewer to clear it.
    let held = call(&auto, "mcp_fixture_echo", json!({"text": "hi"}));
    assert!(!held.ok);
    assert_eq!(held.outcome, crate::tools::Outcome::Denied);
    assert!(
        held.output.contains("not admitted in auto mode"),
        "{}",
        held.output
    );

    // A rule naming the tool admits it, in upstream's rule language.
    let rules = RuleSet::parse(&json!({"mcp_fixture_*": "allow"})).expect("rules");
    let allowed = context(&home, &runtime, PermissionMode::Auto, rules);
    allowed.select_mcp(auto.mcp_selected());
    let ran = call(&allowed, "mcp_fixture_echo", json!({"text": "hi"}));
    assert!(ran.ok, "{ran:?}");
    assert_eq!(
        ran.output,
        r#"{"server":"fixture","tool":"mcp_fixture_echo","result":{"content":[{"type":"text","text":"FIXTURE_ECHO:hi"}]}}"#
    );

    // A deny keeps it out of every search.
    let rules = RuleSet::parse(&json!({"mcp_fixture_echo": "deny"})).expect("rules");
    let denied = context(&home, &runtime, PermissionMode::Yolo, rules);
    let none = call(
        &denied,
        "capability_search",
        json!({"query": "echo text back"}),
    );
    assert!(
        !crate::tools::mcp::is_selected(&denied, "mcp_fixture_echo"),
        "{}",
        none.output
    );

    let full = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());
    let selected = call(
        &full,
        "mcp_select_tool",
        json!({"name": "mcp_fixture_echo"}),
    );
    assert_eq!(
        selected.output,
        "Selected dynamic MCP tool `mcp_fixture_echo`. Its executable schema will be available on the next model step; call `mcp_fixture_echo` with arguments matching the selected schema."
    );
    let features = call(
        &full,
        "mcp_features",
        json!({"action": "resource_read", "server": "fixture", "uri": "fixture://alpha"}),
    );
    assert_eq!(
        features.output,
        r#"{"trust":"untrusted_external","authority":"none","action":"resource_read","server":"fixture","identity":"fixture://alpha","contents":[{"uri":"fixture://alpha","mimeType":"text/plain","type":"text","text":"FIXTURE_RESOURCE_TEXT"}]}"#
    );
    runtime.shutdown();
}

/// What the server saw: each method, and the session id it came with.
type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// A loopback streamable HTTP server: it answers each POST with one JSON-RPC
/// answer -- as JSON, or as an event stream for `tools/call` -- hands out a
/// session id on `initialize`, and records the headers it was sent.
fn http_server(seen: Seen) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            let seen = Arc::clone(&seen);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                loop {
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    let mut session = None;
                    loop {
                        let mut header = String::new();
                        reader.read_line(&mut header).expect("header");
                        let header = header.trim_end();
                        if header.is_empty() {
                            break;
                        }
                        let (name, value) = header.split_once(':').expect("a header");
                        match name.to_ascii_lowercase().as_str() {
                            "content-length" => length = value.trim().parse().unwrap(),
                            "mcp-session-id" => session = Some(value.trim().to_string()),
                            _ => {}
                        }
                    }
                    let mut body = vec![0u8; length];
                    reader.read_exact(&mut body).expect("body");
                    if request_line.starts_with("DELETE") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
                        continue;
                    }
                    let frame: Value = serde_json::from_slice(&body).expect("json");
                    let method = frame["method"].as_str().unwrap_or_default().to_string();
                    seen.lock().unwrap().push((method.clone(), session));
                    let id = frame["id"].clone();
                    let (content_type, extra, payload) = match method.as_str() {
                        "initialize" => (
                            "application/json",
                            "Mcp-Session-Id: s-123\r\n",
                            json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "http-fixture", "version": "1"}}}).to_string(),
                        ),
                        "tools/list" => (
                            "application/json",
                            "",
                            json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [{"name": "ping", "description": "Ping over HTTP", "inputSchema": {"type": "object"}}]}}).to_string(),
                        ),
                        "tools/call" => (
                            "text/event-stream",
                            "",
                            format!(
                                "data: {}\n\ndata: {}\n\n",
                                json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": {}}),
                                json!({"jsonrpc": "2.0", "id": id, "result": {"content": [{"type": "text", "text": "PONG"}]}})
                            ),
                        ),
                        _ => {
                            let _ = stream.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n");
                            continue;
                        }
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\n\r\n{payload}",
                        payload.len()
                    );
                    if stream.write_all(response.as_bytes()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    port
}

#[test]
fn a_loopback_http_server_is_initialized_and_called_with_its_session_id() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let port = http_server(Arc::clone(&seen));
    let home = Home::new();
    config::add_profile_server(
        &home.profile(),
        ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp")),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Ready, "{view:?}");
    let (_, _, result) = runtime
        .call_tool("mcp_remote_ping", json!({}), None)
        .expect("called");
    assert_eq!(
        result,
        json!({"content": [{"type": "text", "text": "PONG"}]})
    );
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        [
            ("initialize".to_string(), None),
            (
                "notifications/initialized".to_string(),
                Some("s-123".to_string())
            ),
            ("tools/list".to_string(), Some("s-123".to_string())),
            ("tools/call".to_string(), Some("s-123".to_string())),
        ]
    );
    // A remote server's sign-in is the gated leaf: checked, then refused.
    runtime.run(Job::Command("auth remote --open".into()));
    assert_eq!(notices(&runtime), [super::command::AUTH_UNAVAILABLE]);
    runtime.shutdown();
}

#[test]
fn removing_a_server_stops_its_process_and_says_how_the_reload_went() {
    let home = Home::new();
    let log = home.dir.path().join("pid");
    let mut server = ServerConfig::local(
        "fixture",
        "/bin/sh",
        &[
            "-c".into(),
            "echo $$ > \"$1\"; exec /bin/sh \"$2\"".into(),
            "sh".into(),
            log.to_string_lossy().into_owned(),
            fixture().to_string_lossy().into_owned(),
        ],
    );
    server.restart_limit = 0;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    let pid: i32 = std::fs::read_to_string(&log)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    runtime.run(Job::Command("remove fixture".into()));
    assert_eq!(
        notices(&runtime),
        [
            format!(
                "Removed MCP server 'fixture'.\n{}",
                super::command::RELOAD_STARTED
            ),
            "MCP configuration reloaded. No servers are configured.".to_string()
        ]
    );
    let pid = rustix::process::Pid::from_raw(pid).expect("pid");
    let deadline = Instant::now() + WAIT;
    while rustix::process::test_kill_process(pid).is_ok() {
        assert!(
            Instant::now() < deadline,
            "the removed server is still running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        std::fs::read_to_string(home.profile()).unwrap(),
        r#"{"mcp":{}}"#
    );
}
