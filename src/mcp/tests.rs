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
use super::runtime::{Connection, Event, Job, McpRuntime, Paths, Retry, Status};
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

    /// A runtime over this home, whose servers are tried as fx tries them --
    /// eight times -- but with waits of milliseconds ([`FAST`]).
    fn runtime(&self) -> Arc<McpRuntime> {
        let runtime = McpRuntime::new(Paths {
            profile: Some(self.profile()),
            settings: Some(self.settings()),
            workspace: self.workspace(),
        });
        runtime.set_retry(FAST);
        runtime
    }
}

/// fx's eight tries, with waits a case can sit through.
const FAST: Retry = Retry {
    attempts: 8,
    first: Duration::from_millis(1),
    cap: Duration::from_millis(5),
};

/// Waits until `name` is done coming up: neither connecting nor retrying.
fn settled(runtime: &McpRuntime, name: &str) -> super::runtime::ServerView {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(view) = runtime.views().into_iter().find(|view| view.name == name) {
            if !matches!(
                view.connection,
                Connection::Connecting | Connection::Retrying
            ) {
                return view;
            }
        }
        assert!(Instant::now() < deadline, "{name} never settled");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `runtime.run(job)`, failing the case loudly if it has not returned within
/// a minute rather than hanging the whole run.
fn bounded_run(runtime: &Arc<McpRuntime>, job: Job) {
    let (done, finished) = std::sync::mpsc::channel();
    let runtime = Arc::clone(runtime);
    std::thread::spawn(move || {
        runtime.run(job);
        let _ = done.send(());
    });
    finished
        .recv_timeout(WAIT)
        .expect("an MCP job did not finish within the case's deadline");
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
fn a_server_that_cannot_start_is_retrying_between_its_tries_and_failed_after_the_last() {
    // fx v0.0.16 tries a server that fails to start eight times in a row,
    // `retrying` while it waits between them, and then `failed`, with how its
    // process ended and its last stderr lines as the reason
    // (`fail-server-1`; `p5-mcp-failed`). The startup notice is gone: the
    // status line says it instead.
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    let mut server = ServerConfig::local(
        "broken",
        "/bin/sh",
        &[
            "-c".into(),
            "echo started >> \"$1\"; echo 'npm error code E401' >&2; echo 'npm error Incorrect or missing password.' >&2; exit 7".into(),
            "sh".into(),
            attempts.to_string_lossy().into_owned(),
        ],
    );
    server.startup_timeout_ms = 20_000;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    // Waits long enough to be seen.
    runtime.set_retry(Retry {
        attempts: 3,
        first: Duration::from_millis(400),
        cap: Duration::from_millis(400),
    });
    runtime.start();
    let words = "its process exited with code 7; its last stderr lines: \"npm error code E401 / npm error Incorrect or missing password.\"";
    let deadline = Instant::now() + WAIT;
    let retrying = loop {
        let view = runtime
            .views()
            .into_iter()
            .find(|view| view.name == "broken")
            .expect("broken");
        if view.connection == Connection::Retrying {
            break view;
        }
        assert_ne!(view.connection, Connection::Failed, "{view:?}");
        assert!(Instant::now() < deadline, "broken never retried");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(retrying.status(), Status::Retrying);
    assert_eq!(retrying.failure.as_deref(), Some(words));
    let view = settled(&runtime, "broken");
    assert_eq!(view.status(), Status::Failed);
    assert_eq!(view.failure.as_deref(), Some(words));
    assert_eq!(
        std::fs::read_to_string(&attempts).unwrap().lines().count(),
        3
    );
    assert!(notices(&runtime).is_empty());
}

#[test]
fn fx_tries_a_server_eight_times_waiting_one_two_four_eight_sixteen_thirty_and_thirty_seconds() {
    let waits: Vec<u64> = (1..Retry::FX.attempts)
        .map(|failed| Retry::FX.backoff(failed).as_secs())
        .collect();
    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30]);
    assert_eq!(waits.iter().sum::<u64>(), 91);
    // And with the default schedule, eight starts before `Failed`.
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "broken",
            "/bin/sh",
            &[
                "-c".into(),
                "echo started >> \"$1\"; exit 3".into(),
                "sh".into(),
                attempts.to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "broken");
    assert_eq!(view.status(), Status::Failed);
    assert_eq!(
        view.failure.as_deref(),
        Some("its process exited with code 3")
    );
    assert_eq!(
        std::fs::read_to_string(&attempts).unwrap().lines().count(),
        8
    );
}

#[test]
fn a_project_server_waits_for_approval_and_the_choice_is_kept_in_settings() {
    let home = Home::new();
    let entry = json!({"command": "/bin/sh", "args": [fixture()]});
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": entry}}).to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    // Nothing asks: fx v0.0.16 has no launch prompt (`ba0ea4a9`); the
    // server waits for `/mcp`'s menu or `/mcp approve`, and nothing is said.
    assert!(
        !runtime
            .take_events()
            .iter()
            .any(|event| matches!(event, Event::Notice { .. })),
        "something was said at launch"
    );
    let digest = config::entry_digest(&entry);
    let view = settled(&runtime, "proj");
    assert_eq!(view.state_label(), "Pending trust");
    assert_eq!(
        runtime.list_resources("proj", false, None),
        Err(super::McpError::Named("McpWorkspaceApprovalRequired"))
    );
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::Approve("proj".into()),
            body: "Approving project MCP server 'proj'.".into(),
        },
    );
    // The reload says nothing of its own (fx v0.0.16, A13).
    assert_eq!(notices(&runtime), ["Approving project MCP server 'proj'."]);
    assert_eq!(settled(&runtime, "proj").status(), Status::Ready);
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(home.settings()).unwrap()).unwrap();
    let entry = &document["workspaces"][home.workspace().to_str().unwrap()];
    assert_eq!(entry["enabledMcpjsonServers"], json!(["proj"]));
    assert_eq!(
        entry["enabledMcpjsonServerDigests"],
        json!({"proj": digest})
    );
    runtime.shutdown();
}

#[test]
fn an_approval_an_older_xfx_saved_by_name_alone_asks_once_more() {
    let home = Home::new();
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(home.settings().parent().unwrap()).unwrap();
    std::fs::write(
        home.settings(),
        json!({"workspaces": {home.workspace().to_str().unwrap(): {
            "enabledMcpjsonServers": ["proj"],
            "enableAllProjectMcpServers": true,
        }}})
        .to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start();
    assert_eq!(settled(&runtime, "proj").state_label(), "Pending trust");
    assert!(settled(&runtime, "proj")
        .entry
        .is_some_and(|entry| entry.digest
            == config::entry_digest(&json!({"command": "/bin/sh", "args": [fixture()]}))));
    runtime.shutdown();
}

#[test]
fn a_pending_entry_is_spelled_once_per_read_and_shared_by_every_view() {
    // Views are taken every frame, every turn and every key; the entry is
    // spelled when the read that found it builds the server, not per view.
    let home = Home::new();
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "proj");
    // The startup reload builds the servers again once: wait for it, so the
    // two views below are of one read.
    let deadline = Instant::now() + WAIT;
    while runtime.generation() < 1 {
        assert!(Instant::now() < deadline, "the startup reload never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
    let entry = |runtime: &McpRuntime| {
        runtime
            .views()
            .into_iter()
            .find(|view| view.name == "proj")
            .and_then(|view| view.entry)
            .expect("a pending entry is shown")
    };
    assert!(Arc::ptr_eq(&entry(&runtime), &entry(&runtime)));
    runtime.shutdown();
}

#[test]
fn approve_all_approves_the_servers_shown_and_not_an_entry_shown_only_as_an_error() {
    // `"type": "${T:-stdio}"` does not parse as written, so the person sees a
    // configuration error, not a server; after expansion it is a stdio
    // server. Approving all is approving what was shown, so it stays out.
    let home = Home::new();
    let ran = home.dir.path().join("templated-ran");
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {
            "shown": {"command": "/bin/sh", "args": [fixture()]},
            "templated": {
                "type": "${T:-stdio}",
                "command": "/bin/sh",
                "args": ["-c", ": > \"$0\"; exec /bin/sh \"$1\"", ran, fixture()],
            },
        }})
        .to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "shown").state_label(), "Pending trust");
    assert_eq!(
        runtime.issues(),
        [".mcp.json server 'templated' was skipped: its \"type\" isn't valid."]
    );
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::ApproveAll,
            body: "Approving all project MCP servers for this workspace.".into(),
        },
    );
    assert_eq!(settled(&runtime, "shown").status(), Status::Ready);
    assert!(
        runtime.views().iter().all(|view| view.name != "templated"),
        "{:?}",
        runtime.views()
    );
    assert!(!ran.exists(), "approve-all ran an entry nobody was shown");
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(home.settings()).unwrap()).unwrap();
    let entry = &document["workspaces"][home.workspace().to_str().unwrap()];
    assert_eq!(
        entry["enabledMcpjsonServers"],
        json!(["shown"]),
        "{document:#}"
    );
    runtime.shutdown();
}

#[test]
fn an_approved_project_server_rewritten_by_the_models_write_asks_again_before_it_runs() {
    // The approval is for the server the person was shown. `.mcp.json` is an
    // ordinary workspace file -- `auto`, the default mode, writes it without
    // asking, as upstream's does (`sensitiveAutoWriteTarget`,
    // `tool_admission.zig:679-693`) -- so a write that swaps an approved
    // entry's command must not inherit the approval on the next reload.
    let home = Home::new();
    let ran = home.dir.path().join("replacement-ran");
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    settled(&runtime, "proj");
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::Approve("proj".into()),
            body: "Approving project MCP server 'proj'.".into(),
        },
    );
    assert_eq!(settled(&runtime, "proj").status(), Status::Ready);
    let _ = runtime.take_events();

    let replaced = json!({"mcpServers": {"proj": {
        "command": "/bin/sh",
        "args": ["-c", ": > \"$0\"; exec /bin/sh \"$1\"", ran, fixture()],
    }}})
    .to_string();
    // As the model would: read the file, then replace it.
    let write = |mode, rules| {
        let context = context(&home, &runtime, mode, rules);
        let read = call(&context, "read_file", json!({"path": ".mcp.json"}));
        assert!(read.ok, "{read:?}");
        call(
            &context,
            "write_file",
            json!({"path": ".mcp.json", "content": replaced}),
        )
    };
    // The first defence: `ask` with nobody to ask refuses (in the TUI the
    // person sees the diff), and `auto` does not change the file on its own,
    // because it decides what runs (SEC-C5).
    let asked = write(PermissionMode::Ask, RuleSet::default());
    assert_eq!(asked.outcome, crate::tools::Outcome::Denied, "{asked:?}");
    let held = write(PermissionMode::Auto, RuleSet::default());
    assert_eq!(held.outcome, crate::tools::Outcome::Denied, "{held:?}");
    assert!(
        held.output
            .contains("auto mode does not change `.mcp.json` on its own"),
        "{}",
        held.output
    );
    // The second, this one: the write gets through the way a person lets it,
    // with their own rule that allows file edits (a configured allow still
    // admits what auto holds), and permission to edit a file is still no
    // approval of what the edited entry starts.
    let rules = RuleSet::parse(&json!({"edit": "allow"})).expect("rules");
    let wrote = write(PermissionMode::Auto, rules);
    assert!(wrote.ok, "{wrote:?}");

    runtime.reload(super::runtime::Origin::Transcript);
    let view = settled(&runtime, "proj");
    assert!(
        !ran.exists(),
        "the rewritten entry ran without being approved again"
    );
    assert_eq!(
        view.state_label(),
        "Pending trust",
        "the rewritten entry kept the approval: {view:?}"
    );
    // The entry shown for approval is what would run, not only the name.
    let entry = view.entry.expect("a pending entry is shown");
    assert_eq!(
        entry.rows,
        [format!(
            "command: /bin/sh -c ': > \"$0\"; exec /bin/sh \"$1\"' {} {}",
            ran.display(),
            fixture().display()
        )]
    );
    let digest = &entry.digest;
    // An approval of a screen that showed the old entry approves the old
    // entry, which is no longer there.
    let first = config::entry_digest(&json!({"command": "/bin/sh", "args": [fixture()]}));
    assert_ne!(*digest, first);
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::ApproveShown {
                name: "proj".into(),
                digest: first,
            },
            body: "Approving project MCP server 'proj'.".into(),
        },
    );
    assert_eq!(settled(&runtime, "proj").state_label(), "Pending trust");
    assert!(!ran.exists());
    // The person approving the entry they were shown runs it.
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::ApproveShown {
                name: "proj".into(),
                digest: digest.clone(),
            },
            body: "Approving project MCP server 'proj'.".into(),
        },
    );
    assert_eq!(settled(&runtime, "proj").status(), Status::Ready);
    assert!(ran.exists());
    runtime.shutdown();
}

#[test]
fn every_feature_command_c1db9191_had_says_where_it_went() {
    // fx v0.0.16 dropped `/mcp resource` and `/mcp prompt` with MCP v1, and
    // moved `auth` (`src/core/mcp_host/verbs.zig:64-73@481104fd`): with a
    // server up that would have answered each, each line now says only that,
    // and nothing is asked of the server.
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    let _ = runtime.take_events();
    let resources = "/mcp resource is gone: xfx doesn't support MCP resources and prompts yet.";
    let prompts = "/mcp prompt is gone: xfx doesn't support MCP resources and prompts yet.";
    for (line, expected) in [
        ("resource list fixture", resources),
        ("resource read fixture fixture://alpha", resources),
        ("prompt list fixture", prompts),
        ("prompt get fixture review {\"tone\":\"brief\"}", prompts),
        ("prompt complete fixture review tone b", prompts),
        ("auth fixture", "/mcp auth is now /mcp login NAME."),
        (
            "logout fixture",
            "fixture runs a command, so it has no login.",
        ),
    ] {
        runtime.run(Job::Command(line.into()));
        assert_eq!(notices(&runtime), [expected], "{line}");
    }
    runtime.shutdown();
}

#[test]
fn a_hostile_name_is_spelled_out_and_cut_at_upstreams_256_bytes() {
    use super::terminal_safe;
    // `encodeTerminalSafe(.., 256)` (`text_utils.zig:343-380@c1db9191`): a
    // name that would retitle the window, clear the screen and reverse what
    // follows is shown as the characters it is made of.
    assert_eq!(
        terminal_safe("ok\u{1b}]0;owned\u{7}\u{1b}[2J\u{202e}x\u{2066}\u{200b}\u{feff}\u{9b}\n"),
        r"ok\x1b]0;owned\x07\x1b[2J\u{202e}x\u{2066}\u{200b}\u{feff}\u{009b}\x0a"
    );
    assert_eq!(
        terminal_safe("plain-name_1 \u{65e5}"),
        "plain-name_1 \u{65e5}"
    );
    // And the default-ignorable code points SEC-A4 measured a terminal
    // drawing as nothing, a tail hidden behind them.
    assert_eq!(
        terminal_safe("a\u{ad}b\u{3164}c\u{ffa0}d"),
        r"a\u{00ad}b\u{3164}c\u{ffa0}d"
    );
    // Spelled text is spelled the same again.
    let once = terminal_safe("x\u{202e}\u{7}");
    assert_eq!(terminal_safe(&once), once);
    // At most 256 bytes, cut at a token boundary and marked as upstream's is.
    assert_eq!(terminal_safe(&"a".repeat(256)), "a".repeat(256));
    let cut = terminal_safe(&"a".repeat(300));
    assert_eq!(cut, format!("{}...", "a".repeat(253)));
    assert_eq!(cut.len(), super::TERMINAL_SAFE_MAX_BYTES);
    // Never half an escape: the override that would not fit goes whole.
    assert_eq!(
        terminal_safe(&format!("{}{}", "a".repeat(250), "\u{202e}".repeat(10))),
        format!("{}...", "a".repeat(250))
    );
}

#[test]
fn a_hostile_command_is_named_in_its_entry_as_the_approval_view_names_it() {
    let home = Home::new();
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": {
            "command": "/bin/sh",
            "args": ["-c", "true \u{202e}hs.live cexe\u{200b}"],
        }}})
        .to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start();
    let entry = settled(&runtime, "proj")
        .entry
        .expect("a pending entry is shown");
    assert_eq!(
        entry.rows,
        ["command: /bin/sh -c 'true \\u{202e}hs.live cexe\\u{200b}'"]
    );
    runtime.shutdown();
}

/// Every entry of the workspace file in `root` approved as it is written --
/// what pressing `2` at its prompt saves.
fn approving_every_entry(root: &Path) -> config::Choices {
    config::Choices {
        approved: config::load_workspace(root, &config::Choices::default(), &|_| None).digests,
        ..config::Choices::default()
    }
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
        r#"{"server":"fixture","tool":"mcp_fixture_echo","result":{"content":[{"type":"text","text":"FIXTURE_ECHO:hi"}],"isError":false}}"#
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
    // fx v0.0.16 has no `mcp_features`, and neither has xfx: the model
    // cannot reach a resource or a prompt.
    assert!(crate::tools::Registry::builtin()
        .spec("mcp_features")
        .is_none());
    runtime.shutdown();
}

/// Answers every question it is put with one answer, and counts them.
struct Answering {
    answer: crate::permission::ApprovalAnswer,
    asked: Arc<Mutex<usize>>,
}

impl crate::permission::ApprovalPrompter for Answering {
    fn request(
        &mut self,
        _: &crate::permission::ApprovalRequest,
    ) -> std::io::Result<crate::permission::ApprovalAnswer> {
        *self.asked.lock().unwrap() += 1;
        Ok(self.answer)
    }
}

/// How many `tools/call` frames the fixture logged in `log`.
fn calls_logged(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(r#""method":"tools/call""#))
        .count()
}

/// What one call came to, as the model and the server see it.
#[derive(Debug, PartialEq)]
struct Came {
    /// Whether the machine took the name for a tool at all
    /// (`select_unselected`); a name it does not ends the turn as
    /// `ToolCallUnsupported` and nothing is judged or sent.
    a_tool: bool,
    /// Each call's result: `ok`, its outcome, and the model's text.
    results: Vec<(bool, crate::tools::Outcome, String)>,
    /// Questions put to the person.
    asked: usize,
    /// `tools/call` frames the server received.
    sent: usize,
}

#[test]
fn an_unselected_call_meets_every_admission_rule_a_selected_one_does() {
    // fx v0.0.16 runs a live MCP tool the model calls without loading it
    // (`advertiseUnselectedMcpCalls`, `orchestrator.zig:2497-2538@481104fd`)
    // and sends it "through normal validation and MCP permission". So here:
    // every mode, every kind of rule (a name, a glob, an ask, a deny, a
    // deny of everything) and every answer a person can give, once on a
    // tool selected first and once on the same call unselected, through the
    // machine's own two steps -- `select_unselected`, then `call`. The two
    // must be judged alike: the same questions, the same result, and the
    // server reached exactly as often. A deny the selected tool met after it
    // was selected (a rule written mid-turn) leaves the unselected one no
    // tool at all, so nothing is sent either way.
    use crate::permission::ApprovalAnswer;
    let home = Home::new();
    let log = home.dir.path().join("wire.log");
    let mut server = fixture_server("fixture");
    server.env = vec![("MCP_FIXTURE_LOG".into(), log.to_string_lossy().into_owned())];
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    let (owner, tool) = runtime
        .tool_by_alias("mcp_fixture_echo")
        .expect("the fixture's tool");
    let chosen =
        crate::mcp::catalog::select(&tool, owner.instructions().as_deref()).expect("within budget");

    let modes = [
        PermissionMode::Ask,
        PermissionMode::Auto,
        PermissionMode::Yolo,
    ];
    // What each table is, for the expected outcome below.
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        NoRule,
        Allows,
        Asks,
        Denies,
    }
    let rules = [
        (json!({}), Kind::NoRule),
        (json!({"mcp_fixture_echo": "allow"}), Kind::Allows),
        (json!({"mcp_fixture_*": "allow"}), Kind::Allows),
        (json!({"mcp_fixture_echo": "ask"}), Kind::Asks),
        (json!({"mcp_fixture_echo": "deny"}), Kind::Denies),
        (json!({"mcp_*": "deny"}), Kind::Denies),
        (json!({"*": "deny"}), Kind::Denies),
        // The rule after it, naming the tool, decides.
        (
            json!({"*": "deny", "mcp_fixture_echo": "allow"}),
            Kind::Allows,
        ),
        // A deny on the two meta-tools is about them, not the server's tools:
        // a call by name is judged as if no rule spoke (fx hands the lookup
        // the table and the tool's own name, `tool_runtime.zig:1042-1053`).
        (
            json!({"capability_search": "deny", "mcp_select_tool": "deny"}),
            Kind::NoRule,
        ),
    ];
    // The outcome every case must have, unselected, by the rules alone
    // (`tool_admission.zig:161-165`, `:1116-1157`, `:1426-1442`): whether the
    // name is a tool at all, the questions put, and the frames sent for the
    // two calls. A deny the table holds leaves no tool; an allow sends both
    // unasked; full access sends both whatever else (an ask rule included);
    // `auto` holds a call no rule decides; a question answered once asks
    // each time, "always" once, "deny" refuses each time, nobody to ask
    // refuses.
    let expected = |mode: PermissionMode, kind: Kind, answer: Option<ApprovalAnswer>| {
        let asked = |answer: Option<ApprovalAnswer>| match answer {
            None => (0, 0),
            Some(ApprovalAnswer::Once) => (2, 2),
            Some(ApprovalAnswer::Always) => (1, 2),
            Some(ApprovalAnswer::Deny) => (2, 0),
        };
        match (kind, mode) {
            (Kind::Denies, _) => (false, 0, 0),
            (Kind::Allows, _) | (_, PermissionMode::Yolo) => (true, 0, 2),
            (Kind::NoRule, PermissionMode::Auto) => (true, 0, 0),
            (Kind::NoRule | Kind::Asks, _) => {
                let (questions, sent) = asked(answer);
                (true, questions, sent)
            }
        }
    };
    let answers = [
        None,
        Some(ApprovalAnswer::Once),
        Some(ApprovalAnswer::Always),
        Some(ApprovalAnswer::Deny),
    ];
    let mut cases = 0;
    let mut sent_somewhere = false;
    let mut refused_somewhere = false;
    for mode in modes {
        for (table, kind) in &rules {
            for answer in answers {
                let came = |selected_first: bool| -> Came {
                    let asked = Arc::new(Mutex::new(0));
                    let mut session = PermissionSession::new(mode)
                        .with_rules(RuleSet::parse(table).expect("rules"));
                    if let Some(answer) = answer {
                        session = session.with_prompter(Box::new(Answering {
                            answer,
                            asked: Arc::clone(&asked),
                        }));
                    }
                    let scope = AccessScope::primary_only(home.workspace()).expect("scope");
                    let context = ToolContext::new(scope)
                        .with_permissions(session)
                        .with_mcp(Arc::clone(&runtime));
                    if selected_first {
                        context.select_mcp(vec![chosen.clone()]);
                    }
                    let before = calls_logged(&log);
                    let a_tool = crate::tools::mcp::select_unselected(&context, "mcp_fixture_echo");
                    let mut results = Vec::new();
                    if a_tool {
                        // Twice, so an "always" answered on the first is
                        // seen deciding the second.
                        for text in ["one", "two"] {
                            let result = call(&context, "mcp_fixture_echo", json!({"text": text}));
                            results.push((result.ok, result.outcome, result.output));
                        }
                        // Selected from now on: advertised in the next request.
                        assert!(crate::tools::mcp::is_selected(&context, "mcp_fixture_echo"));
                    } else {
                        assert!(context.mcp_selected().is_empty());
                    }
                    let asked = *asked.lock().unwrap();
                    Came {
                        a_tool,
                        results,
                        asked,
                        sent: calls_logged(&log) - before,
                    }
                };
                let selected = came(true);
                let unselected = came(false);
                let case = format!("{mode:?} {table} {answer:?}");
                assert_eq!(
                    (unselected.a_tool, unselected.asked, unselected.sent),
                    expected(mode, *kind, answer),
                    "{case}: {unselected:?}"
                );
                assert!(selected.a_tool, "{case}");
                assert_eq!(
                    unselected.sent, selected.sent,
                    "{case}: {selected:?} {unselected:?}"
                );
                assert_eq!(
                    unselected.asked, selected.asked,
                    "{case}: {selected:?} {unselected:?}"
                );
                if unselected.a_tool {
                    assert_eq!(unselected.results, selected.results, "{case}");
                } else {
                    // Refused outright by the table: the selected call is an
                    // unknown tool's, and neither reached the server.
                    assert_eq!(selected.sent, 0, "{case}");
                    assert!(
                        selected.results.iter().all(|(ok, ..)| !ok),
                        "{case}: {selected:?}"
                    );
                }
                sent_somewhere |= selected.sent > 0;
                refused_somewhere |= selected.results.iter().any(|(ok, ..)| !ok);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 3 * 9 * 4);
    assert!(sent_somewhere && refused_somewhere);

    // The rules that matter, spelled out: `auto` holds an unselected call
    // with no rule, as it holds a selected one; full access runs it.
    let held = {
        let context = context(&home, &runtime, PermissionMode::Auto, RuleSet::default());
        assert!(crate::tools::mcp::select_unselected(
            &context,
            "mcp_fixture_echo"
        ));
        call(&context, "mcp_fixture_echo", json!({"text": "hi"}))
    };
    assert_eq!(held.outcome, crate::tools::Outcome::Denied, "{held:?}");
    assert!(
        held.output.contains("not admitted in auto mode"),
        "{}",
        held.output
    );
    let ran = {
        let context = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());
        assert!(crate::tools::mcp::select_unselected(
            &context,
            "mcp_fixture_echo"
        ));
        call(&context, "mcp_fixture_echo", json!({"text": "hi"}))
    };
    assert!(ran.ok, "{ran:?}");
    // A name no live server has is no tool.
    let none = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());
    assert!(!crate::tools::mcp::select_unselected(
        &none,
        "mcp_fixture_nope"
    ));
    assert!(!crate::tools::mcp::select_unselected(
        &none,
        "mcp_other_echo"
    ));
    runtime.shutdown();
}

/// A stdio server with one tool, `echo`, whose `tools/call` answer is the
/// `result` or `error` member `answer`, written verbatim into the frame --
/// or, when `answer` is `LONG=<n>`, an error whose message is `n` bytes,
/// made by the server when it answers (bounded by `head -c`), so the
/// environment it starts with stays small.
fn answering_server(home: &Home, name: &str, answer: &str) -> ServerConfig {
    let script = home.dir.path().join(format!("{name}.sh"));
    let body = r#"while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/^{"jsonrpc":"2.0","id":\([0-9][0-9]*\),.*/\1/p')
  case "$line" in
    *'"method":"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"t","version":"1"}}}\n' "$id" ;;
    *'"method":"tools/list"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo","inputSchema":{"type":"object"}}]}}\n' "$id" ;;
    *'"method":"tools/call"'*)
      if [ -n "${LONG:-}" ]; then
        message=$(/usr/bin/head -c "$LONG" /dev/zero | /usr/bin/tr '\0' x)
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"%s"}}\n' "$id" "$message"
      else
        printf '{"jsonrpc":"2.0","id":%s,%s}\n' "$id" "$ANSWER"
      fi ;;
    *) if [ -n "$id" ]; then printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"; fi ;;
  esac
done
"#;
    std::fs::write(&script, body).expect("script");
    let mut server = ServerConfig::local(name, "/bin/sh", &[script.to_string_lossy().into_owned()]);
    server.env = match answer.strip_prefix("LONG=") {
        Some(bytes) => vec![("LONG".into(), bytes.into())],
        None => vec![("ANSWER".into(), answer.into())],
    };
    server
}

#[test]
fn a_call_the_server_answers_with_no_result_fails_as_fx_says_it_did() {
    // At `call()`, what the model is told and that it is a failure, never a
    // success: an `isError` result (`classifyCall`, `protocol/tools.zig:
    // 255-273@481104fd`), an answer that is no tool result (`copyCall`,
    // `runtime.zig:2363-2378`, `ended`, `model.zig:855-878`), and a JSON-RPC
    // error (`write_protocol_diagnostic`, `tool_result.zig:199-214`) -- its
    // data left out, as `render` leaves it out (`model.zig:841`).
    let home = Home::new();
    let cases = [
        (
            "iserror",
            r#""result":{"content":[{"type":"text","text":"no such issue"}],"isError":true}"#,
            r#"{"server":"iserror","tool":"mcp_iserror_echo","result":{"content":[{"type":"text","text":"no such issue"}],"isError":true}}"#,
        ),
        (
            "malformed",
            r#""result":{"content":"no such issue"}"#,
            "MCP server 'malformed' ended the call: the server's answer was malformed. The tool may have run.",
        ),
        (
            "refusing",
            r#""error":{"code":-32603,"message":"fixture refuses <now> & \"always\"","data":{"why":"x"}}"#,
            r#"MCP protocol error -32603: fixture refuses <now> & "always""#,
        ),
        (
            "nul",
            r#""error":{"code":-32000,"message":"a\u0000b"}"#,
            "MCP protocol error -32000: binary or non-utf8 tool output omitted (3 bytes)",
        ),
        // The error's data, whatever it holds, is not told
        // (`runtime.zig:117`, `:2371`; `model.zig:841`; `p5-mcp-errdata`).
        (
            "spaceddata",
            r#""error":{"code":-32603,"message":"fixture refuses","data": { "b" : [ 2 , 1 ] ,  "a":1.0 } }"#,
            "MCP protocol error -32603: fixture refuses",
        ),
        (
            "nuldata",
            r#""error":{"code":-32603,"message":"fixture refuses","data":{"k":"a\u0000b"}}"#,
            "MCP protocol error -32603: fixture refuses",
        ),
    ];
    // A message over 64 KiB is no error fx reads
    // (`parseProtocolError`, `features/tools.zig:305-307`).
    let long = format!("LONG={}", 64 * 1024 + 1);
    let cases: Vec<(&str, &str, &str)> = cases
        .iter()
        .copied()
        .chain([(
            "long",
            long.as_str(),
            "MCP protocol failure: InvalidEnvelope",
        )])
        .collect();
    for (name, answer, _) in &cases {
        config::add_profile_server(&home.profile(), answering_server(&home, name, answer))
            .expect("saved");
    }
    let runtime = home.runtime();
    runtime.start();
    let full = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());
    for (name, _, told) in &cases {
        assert_eq!(settled(&runtime, name).status(), Status::Ready, "{name}");
        let alias = format!("mcp_{name}_echo");
        assert!(
            crate::tools::mcp::select_unselected(&full, &alias),
            "{name}"
        );
        let result = call(&full, &alias, json!({}));
        assert!(!result.ok, "{name}: {result:?}");
        assert_eq!(result.outcome, crate::tools::Outcome::Finished, "{name}");
        assert_eq!(result.output, *told, "{name}");
    }
    runtime.shutdown();
}

#[test]
fn the_servers_block_counts_a_servers_tools_only_once_the_model_has_read_them() {
    // fx v0.0.16's block names a server's tool count only once its catalog
    // has read the list (`modelSnapshot`, `model.zig:472-500@481104fd`): not
    // at the first request (`p5-mcp-turn`'s bodies), and from a search, a
    // selection or a call on, for as long as a reload keeps the server.
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    let unread = "<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" />\n</mcp_servers>\n";
    let read = "<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" tools=\"1\" />\n</mcp_servers>\n";
    assert!(crate::mcp::catalog::servers_block(&runtime).ends_with(unread));
    // The runtime knows the count all along; only the model's block waits.
    assert_eq!(settled(&runtime, "fixture").tool_count, Some(1));

    // A search that finds nothing still read the list.
    let full = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());
    let found = call(
        &full,
        "capability_search",
        json!({"query": "weather forecast"}),
    );
    assert!(
        found.output.contains(r#""state":"no_match""#),
        "{}",
        found.output
    );
    assert!(crate::mcp::catalog::servers_block(&runtime).ends_with(read));

    // A reload that keeps the server keeps its list.
    runtime.reload(super::runtime::Origin::Transcript);
    settled(&runtime, "fixture");
    assert!(crate::mcp::catalog::servers_block(&runtime).ends_with(read));

    // One that starts it anew does not; then a direct call reads it again.
    config::remove_profile_server(&home.profile(), "fixture").expect("removed");
    let mut changed = fixture_server("fixture");
    changed.env = vec![("XFX_TEST_CHANGED".into(), "1".into())];
    config::add_profile_server(&home.profile(), changed).expect("saved");
    runtime.reload(super::runtime::Origin::Transcript);
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    assert!(crate::mcp::catalog::servers_block(&runtime).ends_with(unread));
    // A call by name a rule refuses is no tool, and fx read the list
    // before it asked the table (`toolSchema`, `model.zig:296-318`).
    let refused = context(
        &home,
        &runtime,
        PermissionMode::Yolo,
        RuleSet::parse(&json!({"mcp_fixture_echo": "deny"})).expect("rules"),
    );
    assert!(!crate::tools::mcp::select_unselected(
        &refused,
        "mcp_fixture_echo"
    ));
    assert!(crate::mcp::catalog::servers_block(&runtime).ends_with(read));
    runtime.shutdown();
}

#[test]
fn an_open_search_that_finds_nothing_names_a_server_it_could_not_look_in() {
    // fx v0.0.16 (`47154a8f`; `withUnavailableLocked`, `model.zig:282-294`;
    // `renderCombined`, `capability_search.zig:301-304@481104fd`).
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "bad",
            "/bin/sh",
            &["-c".into(), "echo boom >&2; exit 3".into()],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    let bad = settled(&runtime, "bad");
    assert_eq!(bad.status(), Status::Failed, "{bad:?}");
    let failure = bad.failure.clone().expect("why it failed");
    let full = context(&home, &runtime, PermissionMode::Yolo, RuleSet::default());

    let nothing = call(
        &full,
        "capability_search",
        json!({"query": "weather forecast"}),
    );
    assert_eq!(
        nothing.output,
        format!(
            r#"{{"skills":[],"mcp_tools":[],"counts":{{"skills":0,"mcp_tools":0}},"total_matches":{{"skills":0,"mcp_tools":0}},"state":"no_match","mcp_unavailable":[{}]}}"#,
            Value::from(crate::mcp::catalog::encode_scalar(&format!(
                "MCP server 'bad' is unavailable: {failure}"
            )))
        )
    );
    // A search that found something, or one that named a server, says
    // nothing of the rest.
    let found = call(
        &full,
        "capability_search",
        json!({"query": "echo text back"}),
    );
    assert!(
        !found.output.contains("mcp_unavailable"),
        "{}",
        found.output
    );
    let named = call(
        &full,
        "capability_search",
        json!({"query": "weather forecast", "server": "fixture"}),
    );
    assert!(
        !named.output.contains("mcp_unavailable"),
        "{}",
        named.output
    );
    runtime.shutdown();
}

#[test]
fn a_deny_written_mid_turn_takes_a_selected_tool_out_of_the_next_request_in_every_mode() {
    // Upstream re-reads each selected definition before every request, with
    // the engine's rules whatever the mode (`snapshotDynamicTools`,
    // `gateway_step.zig`; `snapshotToolDefinition`, `mcp_runtime.zig:2093`;
    // `src/main.zig:1957-1959`): a deny written mid-turn makes the tool
    // `unavailable`, the next request leaves it out, and a call to it is to a
    // tool the step did not advertise (`unknown tool: …`,
    // `tool_dispatch.zig:845`).
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    for mode in [PermissionMode::Yolo, PermissionMode::Ask] {
        let live = crate::permission::LiveRules::new(RuleSet::default());
        let scope = AccessScope::primary_only(home.workspace()).expect("scope");
        let context = ToolContext::new(scope)
            .with_permissions(PermissionSession::new(mode).with_live_rules(live.clone()))
            .with_mcp(Arc::clone(&runtime));
        let selected = call(
            &context,
            "mcp_select_tool",
            json!({"name": "mcp_fixture_echo"}),
        );
        assert!(selected.ok, "{mode:?}: {selected:?}");
        let names = |context: &ToolContext| -> Vec<String> {
            crate::tools::Registry::builtin()
                .advertisement_for(context)
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        };
        assert!(
            names(&context).contains(&"mcp_fixture_echo".to_string()),
            "{mode:?}"
        );

        live.replace(RuleSet::parse(&json!({"mcp_fixture_*": "deny"})).expect("rules"));
        assert!(
            !names(&context).contains(&"mcp_fixture_echo".to_string()),
            "{mode:?}"
        );
        let refused = call(&context, "mcp_fixture_echo", json!({"text": "hi"}));
        assert!(!refused.ok, "{mode:?}");
        assert_eq!(refused.output, "unknown tool: mcp_fixture_echo", "{mode:?}");

        // Taken back: offered again from the next step on.
        live.replace(RuleSet::default());
        assert!(
            names(&context).contains(&"mcp_fixture_echo".to_string()),
            "{mode:?}"
        );
    }
    runtime.shutdown();
}

#[test]
fn a_rule_that_asks_about_an_mcp_tool_asks_in_auto_before_auto_holds_it() {
    // An ask rule comes before the mode (`configured_ask`,
    // `tool_admission.zig:1426-1442`), for a server's tool as for a read:
    // with nobody to ask the refusal names the rule, not auto's hold.
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    let rules = RuleSet::parse(&json!({"mcp_fixture_echo": "ask"})).expect("rules");
    let auto = context(&home, &runtime, PermissionMode::Auto, rules);
    let selected = call(
        &auto,
        "mcp_select_tool",
        json!({"name": "mcp_fixture_echo"}),
    );
    assert!(selected.ok, "{selected:?}");
    let asked = call(&auto, "mcp_fixture_echo", json!({"text": "hi"}));
    assert_eq!(asked.outcome, crate::tools::Outcome::Denied, "{asked:?}");
    assert!(
        asked
            .output
            .contains("a configured permission rule asks before `mcp` on `mcp_fixture_echo`"),
        "{}",
        asked.output
    );
    runtime.shutdown();
}

#[test]
fn a_global_deny_hides_the_mcp_meta_tools_and_refuses_a_call_to_one_anyway() {
    // The meta-tools are builtins upstream projects and admits like any
    // other (`tool_projection.zig:563-566`; `toolHasPermissionContract`,
    // `tool_admission.zig:101-103`): under `*: deny` none is offered, and a
    // search called anyway selects nothing.
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    settled(&runtime, "fixture");
    let rules = RuleSet::parse(&json!({"*": "deny"})).expect("rules");
    let denied = context(&home, &runtime, PermissionMode::Auto, rules);
    assert!(crate::tools::Registry::builtin()
        .advertisement_for(&denied)
        .is_empty());
    let searched = call(
        &denied,
        "capability_search",
        json!({"query": "echo text back"}),
    );
    assert_eq!(
        searched.outcome,
        crate::tools::Outcome::Denied,
        "{searched:?}"
    );
    assert!(denied.mcp_selected().is_empty());
    // Full access offers both, and the search runs.
    let full = context(
        &home,
        &runtime,
        PermissionMode::Yolo,
        RuleSet::parse(&json!({"capability_search": "deny"})).expect("rules"),
    );
    let names: Vec<String> = crate::tools::Registry::builtin()
        .advertisement_for(&full)
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect();
    for name in crate::tools::mcp::META_TOOLS {
        assert!(names.contains(&name.to_string()), "{name}: {names:?}");
    }
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
    runtime.run(Job::Command("login remote".into()));
    assert_eq!(notices(&runtime), [super::command::AUTH_UNAVAILABLE]);
    runtime.shutdown();
}

#[test]
fn removing_a_server_stops_its_process_and_the_reload_says_nothing_of_its_own() {
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
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    let pid: i32 = std::fs::read_to_string(&log)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    runtime.run(Job::Command("remove fixture".into()));
    // Only the command's own words: the reload says nothing (fx v0.0.16,
    // `333d4764`), and stops the server before it returns.
    assert_eq!(notices(&runtime), ["Removed fixture from ~/.xfx/mcp.json."]);
    let pid = rustix::process::Pid::from_raw(pid).expect("pid");
    let deadline = Instant::now() + WAIT;
    while rustix::process::test_kill_process(pid).is_ok() {
        assert!(
            Instant::now() < deadline,
            "the removed server is still running"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // Edited in place, as fx v0.0.16 edits it: the old profile keeps its
    // `mcp` key, written two-space (`src/core/mcp_host/config_file.zig@481104fd`).
    assert_eq!(
        std::fs::read_to_string(home.profile()).unwrap(),
        "{\n  \"mcp\": {}\n}\n"
    );
}

#[test]
fn an_http_server_is_sent_its_headers_then_its_environment_headers_then_its_bearer() {
    let mut server = ServerConfig::http("remote", "https://mcp.example.test/mcp");
    server.headers = vec![("X-Static".into(), "one".into())];
    server.header_env = vec![("X-Team".into(), "FIXTURE_TEAM".into())];
    server.bearer_token_env = Some("FIXTURE_TOKEN".into());
    let set = |name: &str| match name {
        "FIXTURE_TEAM" => Some("blue".to_string()),
        "FIXTURE_TOKEN" => Some("not-a-real-token".to_string()),
        _ => None,
    };
    assert_eq!(
        super::runtime::http_headers(&server, &set).expect("resolved"),
        [
            ("X-Static".to_string(), "one".to_string()),
            ("X-Team".to_string(), "blue".to_string()),
            (
                "Authorization".to_string(),
                "Bearer not-a-real-token".to_string()
            ),
        ]
    );
    // A variable that is not set is upstream's failure, not a missing header.
    let unset = |_: &str| None;
    assert_eq!(
        super::runtime::http_headers(&server, &unset),
        Err(super::McpError::Named("McpHeaderEnvironmentMissing"))
    );
    server.header_env.clear();
    assert_eq!(
        super::runtime::http_headers(&server, &unset),
        Err(super::McpError::Named("McpBearerEnvironmentMissing"))
    );
}

/// A repository's `.mcp.json` names xfx's own credentials every way it can.
/// An approval is one answer to one prompt, and an HTTP server runs no code
/// of the repository's, so nothing it is sent may carry the gateway
/// credential: `${VAR}` never
/// resolves one, and `header_env` / `bearer_token_env` naming one is refused
/// where the person can see it. A profile entry, which the person wrote, may.
#[test]
fn a_project_server_never_receives_xfx_credentials_and_a_profile_server_still_may() {
    let withheld = |server: &str, field: &str, variable: &str| {
        format!(
            ".mcp.json server '{server}' field {field} uses '{variable}', which xfx withholds \
             from project servers; a server in ~/.xfx/mcp.json may use it."
        )
    };
    const KEY: &str = "sentinel-gateway-key-not-real";
    const TOKEN: &str = "sentinel-oidc-token-not-real";
    let dir = tempfile::tempdir().expect("workspace");
    let url = "https://mcp.example.test/mcp";
    std::fs::write(
        dir.path().join(config::WORKSPACE_FILE),
        json!({"mcpServers": {
            "env": {"command": "/bin/sh", "env": {"K": "${AI_GATEWAY_API_KEY}"}},
            "args": {"command": "/bin/sh", "args": ["--token=${VERCEL_OIDC_TOKEN:-fallback}"]},
            "url": {"type": "http", "url": "https://mcp.example.test/${AI_GATEWAY_API_KEY}"},
            "header": {"type": "http", "url": url, "headers": {"X-Key": "${AI_GATEWAY_API_KEY}"}},
            "header-env": {"type": "http", "url": url, "header_env": {"X-Key": "AI_GATEWAY_API_KEY"}},
            "bearer": {"type": "http", "url": url, "bearer_token_env": "VERCEL_OIDC_TOKEN"},
            "indirect": {"type": "http", "url": url, "bearer_token_env": "${POINTER}"},
            "fine": {"type": "http", "url": url, "bearer_token_env": "FIXTURE_TOKEN",
                     "headers": {"X-Team": "${FIXTURE_TEAM}"}},
        }})
        .to_string(),
    )
    .unwrap();
    let environment = |name: &str| match name {
        "AI_GATEWAY_API_KEY" => Some(KEY.to_string()),
        "VERCEL_OIDC_TOKEN" => Some(TOKEN.to_string()),
        "POINTER" => Some("AI_GATEWAY_API_KEY".to_string()),
        "FIXTURE_TOKEN" => Some("not-a-real-token".to_string()),
        "FIXTURE_TEAM" => Some("blue".to_string()),
        _ => None,
    };
    let approved = approving_every_entry(dir.path());
    let loaded = config::load_workspace(dir.path(), &approved, &environment);
    for server in &loaded.servers {
        let mut sent: Vec<String> = vec![server.command.clone(), server.url.clone()];
        sent.extend(server.args.iter().cloned());
        sent.extend(server.env.iter().map(|(_, value)| value.clone()));
        sent.extend(server.headers.iter().map(|(_, value)| value.clone()));
        if let Ok(headers) = super::runtime::http_headers(server, &environment) {
            sent.extend(headers.into_iter().map(|(_, value)| value));
        }
        for value in &sent {
            assert!(
                !value.contains(KEY) && !value.contains(TOKEN),
                "project server `{}` resolved xfx's credential: {value:?}",
                server.name
            );
        }
    }
    let mut names: Vec<&str> = loaded.servers.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["args", "fine"], "{loaded:?}");
    let server = |name: &str| loaded.servers.iter().find(|s| s.name == name).unwrap();
    // Withheld is unset: a default stands in for it.
    assert_eq!(server("args").args, ["--token=fallback"]);
    assert_eq!(
        super::runtime::http_headers(server("fine"), &environment).expect("resolved"),
        [
            ("X-Team".to_string(), "blue".to_string()),
            (
                "Authorization".to_string(),
                "Bearer not-a-real-token".to_string()
            ),
        ]
    );
    let sorted = |mut issues: Vec<String>| {
        issues.sort();
        issues
    };
    // Every refusal says the variable is withheld on purpose, and where it
    // may be used instead.
    assert_eq!(
        sorted(loaded.issues.clone()),
        [
            withheld("bearer", "bearer_token_env", "VERCEL_OIDC_TOKEN"),
            withheld("env", "environment", "AI_GATEWAY_API_KEY"),
            withheld("header", "http_header", "AI_GATEWAY_API_KEY"),
            withheld("header-env", "header_env", "AI_GATEWAY_API_KEY"),
            withheld("indirect", "bearer_token_env", "AI_GATEWAY_API_KEY"),
            withheld("url", "url", "AI_GATEWAY_API_KEY"),
        ]
    );
    // Refused before the person is asked: an entry naming one is not offered
    // for an approval it could never use. (`indirect` is not expanded while
    // pending, so it fails upstream's plain-name rule instead.)
    let pending = config::load_workspace(dir.path(), &config::Choices::default(), &environment);
    assert_eq!(
        sorted(pending.issues),
        [
            withheld("bearer", "bearer_token_env", "VERCEL_OIDC_TOKEN"),
            withheld("header-env", "header_env", "AI_GATEWAY_API_KEY"),
            ".mcp.json server 'indirect' was skipped: its \"bearer_token_env\" isn't valid."
                .to_string(),
        ]
    );
    // The profile is the person's own file: the same fields still resolve.
    let profile = config::parse_profile(
        &json!({"mcp": {"own": {"type": "http", "url": url,
            "bearer_token_env": "AI_GATEWAY_API_KEY",
            "header_env": {"X-Oidc": "VERCEL_OIDC_TOKEN"}}}})
        .to_string(),
    )
    .expect("a profile entry may name them");
    assert_eq!(
        super::runtime::http_headers(&profile.servers[0], &environment).expect("resolved"),
        [
            ("X-Oidc".to_string(), TOKEN.to_string()),
            ("Authorization".to_string(), format!("Bearer {KEY}")),
        ]
    );
}

/// An approved project server whose `${VAR}` has no value and no default
/// does not start, and the menu says which field needs which variable, in
/// upstream's words (`renderWorkspaceDiagnostic`, `vercel-labs/fx@c1db9191
/// src/core/mcp/project_config.zig:156-176`, the server dropped at
/// `:526-545`). Every field `${}` is expanded in takes that path.
#[test]
fn an_unset_variable_keeps_a_project_server_from_starting_and_says_so_as_upstream_does() {
    let dir = tempfile::tempdir().expect("workspace");
    let url = "https://mcp.example.test/mcp";
    std::fs::write(
        dir.path().join(config::WORKSPACE_FILE),
        json!({"mcpServers": {
            "cmd": {"command": "${UNSET_CMD}"},
            "arg": {"command": "/bin/sh", "args": ["-c", "x ${UNSET_ARG} y"]},
            "array": {"command": ["/bin/sh", "${UNSET_ARRAY}"]},
            "env": {"command": "/bin/sh", "env": {"K": "${UNSET_ENV}"}},
            "header": {"type": "http", "url": url, "headers": {"X-A": "${UNSET_HEADER}"}},
            "url": {"type": "http", "url": "https://${UNSET_HOST}/mcp"},
            "empty": {"command": "/bin/sh", "env": {"K": "${SET_EMPTY}"}},
            "defaulted": {"command": "/bin/sh", "args": ["${UNSET_ARG:-d}"]},
        }})
        .to_string(),
    )
    .unwrap();
    let environment = |name: &str| (name == "SET_EMPTY").then(String::new);
    let approved = approving_every_entry(dir.path());
    let loaded = config::load_workspace(dir.path(), &approved, &environment);
    let mut names: Vec<&str> = loaded.servers.iter().map(|s| s.name.as_str()).collect();
    names.sort_unstable();
    // Set, even to nothing, is set; a default stands in for an unset one.
    assert_eq!(names, ["defaulted", "empty"], "{loaded:?}");
    let missing = |server: &str, field: &str, variable: &str| {
        format!(
            ".mcp.json server '{server}' field {field} requires environment variable \
             '{variable}'; set it or use ${{{variable}:-default}}."
        )
    };
    let mut issues = loaded.issues.clone();
    issues.sort();
    assert_eq!(
        issues,
        [
            missing("arg", "argument", "UNSET_ARG"),
            missing("array", "argument", "UNSET_ARRAY"),
            missing("cmd", "command", "UNSET_CMD"),
            missing("env", "environment", "UNSET_ENV"),
            missing("header", "http_header", "UNSET_HEADER"),
            missing("url", "url", "UNSET_HOST"),
        ]
    );
    // Not approved, not expanded: nothing is missing yet.
    let pending = config::load_workspace(dir.path(), &config::Choices::default(), &environment);
    assert_eq!(pending.servers.len(), 8, "{pending:?}");
    assert!(pending.issues.is_empty(), "{:?}", pending.issues);
}

/// Only a plain variable name is ever looked up. The lookup is `getenv`,
/// and macOS's `getenv("HOME=")` answers `HOME`'s value: a name that
/// carried `=` would pass the exact-name credential guard and still read the
/// credential. Upstream refuses such an entry before any lookup
/// (`isValidEnvName`, `vercel-labs/fx@c1db9191
/// src/core/mcp/project_config.zig:610-612`, `.invalid`), and an unclosed
/// `${` the same way (`:603-604`); both say upstream's sentence (`:178-182`).
#[test]
fn a_name_that_is_not_a_plain_variable_name_is_never_looked_up() {
    let dir = tempfile::tempdir().expect("workspace");
    std::fs::write(
        dir.path().join(config::WORKSPACE_FILE),
        json!({"mcpServers": {
            "equals": {"command": "/bin/sh", "env": {"K": "${AI_GATEWAY_API_KEY=}"}},
            "equals-default": {"command": "/bin/sh", "args": ["${AI_GATEWAY_API_KEY=:-x}"]},
            "space": {"command": "/bin/sh", "args": ["${AI_GATEWAY_API_KEY }"]},
            "nul": {"command": "/bin/sh", "args": ["${AI_GATEWAY_API_KEY\u{0}}"]},
            "empty": {"command": "/bin/sh", "args": ["${:-x}"]},
            "path": {"command": "/bin/sh", "env": {"K": "${PATH=}"}},
            "unclosed": {"command": "/bin/sh", "args": ["${PLAIN"]},
            "plain": {"command": "/bin/sh", "args": ["${PLAIN}"]},
        }})
        .to_string(),
    )
    .unwrap();
    let asked = std::cell::RefCell::new(Vec::<String>::new());
    let environment = |name: &str| {
        asked.borrow_mut().push(name.to_string());
        Some("looked-up".to_string())
    };
    let approved = approving_every_entry(dir.path());
    let loaded = config::load_workspace(dir.path(), &approved, &environment);
    assert_eq!(asked.into_inner(), ["PLAIN"]);
    let names: Vec<&str> = loaded.servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["plain"], "{loaded:?}");
    let mut issues = loaded.issues.clone();
    issues.sort();
    let skipped = |server: &str| {
        format!(".mcp.json server '{server}' was skipped: a ${{...}} in it isn't valid.")
    };
    assert_eq!(
        issues,
        [
            skipped("empty"),
            skipped("equals"),
            skipped("equals-default"),
            skipped("nul"),
            skipped("path"),
            skipped("space"),
            skipped("unclosed"),
        ]
    );
    // Through the process's own `getenv`: `PATH` is set in every test run,
    // and `${PATH=}` must not read it.
    let process = |name: &str| std::env::var(name).ok();
    let loaded = config::load_workspace(dir.path(), &approved, &process);
    assert!(
        !loaded.servers.iter().any(|server| server.name == "path"),
        "`${{PATH=}}` was looked up"
    );
}

/// A loopback port nothing listens on, and what keeps it so while it is held.
///
/// Linux gives a bind to port 0 any free port, at random, so a port bound and
/// released can be the next one a test beside this one listens on: the
/// connection is then taken and cut, `McpConnectionFailed`, not refused. So
/// the port stays bound, by a socket that never listens: Linux refuses a
/// connection to it, and binds nothing else to it.
#[cfg(target_os = "linux")]
fn a_port_nothing_listens_on() -> (u16, Option<tokio::net::TcpSocket>) {
    let socket = tokio::net::TcpSocket::new_v4().expect("a socket");
    socket
        .bind((std::net::Ipv4Addr::LOCALHOST, 0).into())
        .expect("bind loopback");
    let port = socket.local_addr().expect("its address").port();
    assert_eq!(
        TcpListener::bind(("127.0.0.1", port))
            .err()
            .map(|err| err.kind()),
        Some(std::io::ErrorKind::AddrInUse),
        "a listener was given the held port"
    );
    (port, Some(socket))
}

/// macOS drops a connection to a socket that is bound and not listening
/// rather than refusing it -- a timeout -- but hands ports out in turn, from
/// 49152 to 65535: one bound and released is not handed out again until the
/// range has gone round.
#[cfg(not(target_os = "linux"))]
fn a_port_nothing_listens_on() -> (u16, Option<tokio::net::TcpSocket>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    (listener.local_addr().unwrap().port(), None)
}

#[test]
fn an_http_server_nothing_listens_on_fails_in_fxs_words_with_upstreams_counts() {
    let (port, _held) = a_port_nothing_listens_on();
    let home = Home::new();
    config::add_profile_server(
        &home.profile(),
        ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp")),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::Failed, "{view:?}");
    // fx v0.0.16's `show remote` for a server at a port nothing listens on
    // (`verbs-end.full`: `error  it refused or never answered fx's
    // initialize request`).
    assert_eq!(
        view.failure.as_deref(),
        Some("it refused or never answered xfx's initialize request")
    );
    // `vercel-labs/fx@c1db9191`'s details for the same server
    // (`receipts/m1/fx-mcp-menu-screens-40x120-details-1`).
    assert_eq!(
        view.capability_summary(),
        "tools=0 resources=0 templates=0 prompts=0"
    );
    runtime.shutdown();
}

#[test]
fn a_ready_servers_lists_are_counted_once_read() {
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let summary = |runtime: &McpRuntime| {
        runtime
            .views()
            .into_iter()
            .find(|view| view.name == "fixture")
            .expect("a view")
            .capability_summary()
    };
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    // The fixture advertises resources and prompts: unknown until listed.
    assert_eq!(
        summary(&runtime),
        "tools=1 resources=? templates=? prompts=?"
    );
    runtime
        .list_resources("fixture", false, None)
        .expect("resources");
    runtime
        .list_resources("fixture", true, None)
        .expect("templates");
    runtime.list_prompts("fixture", None).expect("prompts");
    assert_eq!(
        summary(&runtime),
        "tools=1 resources=1 templates=0 prompts=1"
    );
    runtime.shutdown();
}

/// A stdio server that writes its process id to `log` and is then the test
/// fixture: its process can be told apart from the one a restart starts.
fn logged_server(name: &str, log: &Path, mark: &str) -> ServerConfig {
    let mut server = ServerConfig::local(
        name,
        "/bin/sh",
        &[
            "-c".into(),
            "echo $$ > \"$1\"; exec /bin/sh \"$2\"".into(),
            mark.into(),
            log.to_string_lossy().into_owned(),
            fixture().to_string_lossy().into_owned(),
        ],
    );
    server.restart_limit = 0;
    server
}

fn pid_in(log: &Path) -> i32 {
    std::fs::read_to_string(log)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn running(pid: i32) -> bool {
    rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid).expect("pid")).is_ok()
}

#[test]
fn a_reload_restarts_only_the_server_whose_config_changed_and_says_nothing() {
    // fx v0.0.16 takes a changed config in place: a server whose config is
    // the same keeps running with its connection, and only a changed one
    // starts fresh (`Runtime.reload`, `vercel-labs/fx@481104fd
    // src/core/mcp_host/runtime.zig:631-641`; `333d4764`).
    let home = Home::new();
    let (same_log, changed_log) = (
        home.dir.path().join("same"),
        home.dir.path().join("changed"),
    );
    config::add_profile_server(&home.profile(), logged_server("same", &same_log, "one"))
        .expect("saved");
    config::add_profile_server(
        &home.profile(),
        logged_server("changed", &changed_log, "one"),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "same").status(), Status::Ready);
    assert_eq!(settled(&runtime, "changed").status(), Status::Ready);
    let (same_pid, changed_pid) = (pid_in(&same_log), pid_in(&changed_log));
    let same_before = runtime.server("same").expect("same");
    let started = runtime.started_at().expect("started");
    runtime.take_events();

    config::add_profile_server(
        &home.profile(),
        logged_server("changed", &changed_log, "two"),
    )
    .expect("saved");
    runtime.reload(super::runtime::Origin::Transcript);

    // The same server is the same server, on the same process, still ready.
    assert!(Arc::ptr_eq(
        &runtime.server("same").expect("same"),
        &same_before
    ));
    assert_eq!(settled(&runtime, "same").status(), Status::Ready);
    assert!(running(same_pid));
    assert_eq!(pid_in(&same_log), same_pid);
    // The changed one was stopped and started again.
    assert!(
        !running(changed_pid),
        "the changed server's old process still runs"
    );
    assert_eq!(settled(&runtime, "changed").status(), Status::Ready);
    assert_ne!(pid_in(&changed_log), changed_pid);
    // Nothing said, and the status line's wait starts over.
    assert!(notices(&runtime).is_empty());
    assert!(runtime.started_at().expect("started") > started);
    runtime.shutdown();
}

#[test]
fn a_failed_server_whose_config_is_the_same_stays_failed_across_a_reload() {
    // fx's engine keeps the slot of a server whose config is the same,
    // whatever its phase (`Engine.reload`, `src/mcp/engine.zig:167-186@481104fd`):
    // a reload is not a retry.
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "broken",
            "/bin/sh",
            &[
                "-c".into(),
                "echo started >> \"$1\"; exit 3".into(),
                "sh".into(),
                attempts.to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "broken").status(), Status::Failed);
    let tries = std::fs::read_to_string(&attempts).unwrap().lines().count();
    runtime.reload(super::runtime::Origin::Transcript);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(settled(&runtime, "broken").status(), Status::Failed);
    assert_eq!(
        std::fs::read_to_string(&attempts).unwrap().lines().count(),
        tries
    );
}

#[test]
fn a_config_that_cannot_be_read_again_is_one_warning_and_the_servers_stay() {
    // `collectMcpHostFacts`, `src/core/app/app_commands.zig:340-345@481104fd`.
    let home = Home::new();
    config::add_profile_server(&home.profile(), fixture_server("fixture")).expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    runtime.take_events();
    std::fs::write(home.profile(), "{ not json").expect("broken profile");
    runtime.reload(super::runtime::Origin::Transcript);
    assert_eq!(
        runtime
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                Event::Notice { warning, body } => Some((warning, body)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [(
            true,
            "The MCP config changed, but xfx couldn't load it again. Your current servers are still active.".to_string()
        )]
    );
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    runtime.shutdown();
}

#[test]
fn a_server_waiting_out_its_backoff_is_stopped_at_once_by_a_reload_or_the_end() {
    // A 30-second wait is never waited out by `/quit` or by the reload that
    // removes the server.
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "broken",
            "/bin/sh",
            &[
                "-c".into(),
                "echo started >> \"$1\"; exit 3".into(),
                "sh".into(),
                attempts.to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(Retry::FX);
    runtime.start_and_wait(WAIT);
    let deadline = Instant::now() + WAIT;
    let server = loop {
        let server = runtime.server("broken").expect("broken");
        if runtime.views()[0].connection == Connection::Retrying {
            break server;
        }
        assert!(Instant::now() < deadline, "never retried");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(config::remove_profile_server(&home.profile(), "broken").expect("removed"));
    let asked = Instant::now();
    runtime.reload(super::runtime::Origin::Transcript);
    assert!(runtime.views().is_empty());
    // Its thread sees the stop within a slice of its wait and starts nothing.
    std::thread::sleep(Duration::from_millis(1_500));
    assert!(asked.elapsed() < Duration::from_secs(5));
    assert_eq!(
        std::fs::read_to_string(&attempts).unwrap().lines().count(),
        1
    );
    drop(server);
    runtime.shutdown();
}

// ---------------------------------------------------------------------------
// MCP-Sb: a child mid-handshake, `Retrying` waited on, the pins
// ---------------------------------------------------------------------------

/// A stdio server that writes its process id to `pid` and then never answers
/// `initialize`: it reads nothing and is gone by itself after 20 s, so a case
/// that fails leaves nothing running for long.
fn holding_server(name: &str, pid: &Path) -> ServerConfig {
    let mut server = ServerConfig::local(
        name,
        "/bin/sh",
        &[
            "-c".into(),
            "echo $$ > \"$1\"; exec sleep 20".into(),
            "sh".into(),
            pid.to_string_lossy().into_owned(),
        ],
    );
    server.startup_timeout_ms = 20_000;
    server
}

/// Waits until `path` holds a process id, and returns it.
fn pid_written(path: &Path) -> i32 {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "the server never started");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Whether anything is left of `pid`'s process group (each server leads its
/// own, `process_group(0)`).
fn group_left(pid: i32) -> bool {
    rustix::process::test_kill_process_group(rustix::process::Pid::from_raw(pid).expect("pid"))
        .is_ok()
}

#[test]
fn a_server_held_mid_handshake_is_gone_before_its_replacement_starts() {
    // fx never runs a server twice: what would start one waits while the
    // process a reload dropped under that name is still stopping
    // (`Engine.stopping`, `vercel-labs/fx@481104fd src/mcp/engine.zig:194-199`;
    // `src/core/mcp_host/runtime.zig:832-840`). The replacement says what it
    // saw of the old one when it started.
    let home = Home::new();
    let (old_pid, verdict) = (home.dir.path().join("old"), home.dir.path().join("verdict"));
    config::add_profile_server(&home.profile(), holding_server("slow", &old_pid)).expect("saved");
    let runtime = home.runtime();
    // The startup reload over first: a reload asked for while it runs is
    // folded into it and returns at once, which would read as a stop that
    // did not wait.
    runtime.start_and_wait(WAIT);
    let pid = pid_written(&old_pid);
    assert_eq!(runtime.views()[0].connection, Connection::Connecting);
    let mut replacement = ServerConfig::local(
        "slow",
        "/bin/sh",
        &[
            "-c".into(),
            "if kill -0 \"$(cat \"$1\")\" 2>/dev/null; then echo alive; else echo gone; fi > \"$2\"; exec /bin/sh \"$3\"".into(),
            "sh".into(),
            old_pid.to_string_lossy().into_owned(),
            verdict.to_string_lossy().into_owned(),
            fixture().to_string_lossy().into_owned(),
        ],
    );
    replacement.restart_limit = 0;
    config::add_profile_server(&home.profile(), replacement).expect("saved");
    runtime.reload(super::runtime::Origin::Transcript);
    assert!(
        !running(pid),
        "the old server outlived the reload that dropped it"
    );
    assert!(!group_left(pid), "its process group outlived the reload");
    assert_eq!(settled(&runtime, "slow").status(), Status::Ready);
    assert_eq!(std::fs::read_to_string(&verdict).unwrap().trim(), "gone");
    runtime.shutdown();
}

#[test]
fn a_server_held_mid_handshake_is_gone_when_a_reload_removes_it_or_the_session_ends() {
    let home = Home::new();
    let (removed, ended) = (
        home.dir.path().join("removed"),
        home.dir.path().join("ended"),
    );
    config::add_profile_server(&home.profile(), holding_server("removed", &removed))
        .expect("saved");
    config::add_profile_server(&home.profile(), holding_server("ended", &ended)).expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    let (removed_pid, ended_pid) = (pid_written(&removed), pid_written(&ended));
    assert!(config::remove_profile_server(&home.profile(), "removed").expect("removed"));
    runtime.reload(super::runtime::Origin::Transcript);
    assert!(
        !running(removed_pid) && !group_left(removed_pid),
        "removed, still running"
    );
    assert!(running(ended_pid));
    runtime.shutdown();
    assert!(
        !running(ended_pid) && !group_left(ended_pid),
        "the session ended, it did not"
    );
}

#[test]
fn a_feature_request_during_a_startup_backoff_waits_for_the_next_try() {
    // `Retrying` is still coming up: a resource or prompt read waits through
    // it as through `Connecting`, rather than answering from the capabilities
    // of a try that never got as far as `initialize`.
    let home = Home::new();
    let marker = home.dir.path().join("tried");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "flaky",
            "/bin/sh",
            &[
                "-c".into(),
                "if [ -e \"$1\" ]; then exec /bin/sh \"$2\"; else : > \"$1\"; exit 3; fi".into(),
                "sh".into(),
                marker.to_string_lossy().into_owned(),
                fixture().to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(Retry {
        attempts: 8,
        first: Duration::from_millis(800),
        cap: Duration::from_millis(800),
    });
    runtime.start();
    let deadline = Instant::now() + WAIT;
    while runtime.views()[0].connection != Connection::Retrying {
        assert!(Instant::now() < deadline, "flaky never retried");
        std::thread::sleep(Duration::from_millis(2));
    }
    // Both asked during the backoff, each on its own: the prompt read does
    // not ride on the resource read's wait.
    let prompts = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || runtime.list_prompts("flaky", None))
    };
    let resources = runtime
        .list_resources("flaky", false, None)
        .expect("resources");
    assert_eq!(resources.len(), 1, "{resources:?}");
    assert_eq!(
        prompts
            .recv_timeout(WAIT)
            .expect("the prompt read ended")
            .expect("prompts")
            .len(),
        1
    );
    runtime.shutdown();
}

#[test]
fn a_config_reload_does_not_start_a_failed_server_again_and_nothing_else_does() {
    // fx v0.0.16 has no reload of its own -- its `/mcp reload` is gone, and
    // so is c1db9191's menu `r` that reconnected -- and its reloads keep a
    // server by its configuration alone (`src/core/mcp_host/verbs.zig:70`,
    // `src/mcp/engine.zig:167-186@481104fd`).
    let home = Home::new();
    let attempts = home.dir.path().join("attempts");
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "broken",
            "/bin/sh",
            &[
                "-c".into(),
                "echo started >> \"$1\"; exit 3".into(),
                "sh".into(),
                attempts.to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "broken").status(), Status::Failed);
    let tries = || std::fs::read_to_string(&attempts).unwrap().lines().count();
    assert_eq!(tries(), 8);
    runtime.reload(super::runtime::Origin::Transcript);
    assert_eq!(settled(&runtime, "broken").status(), Status::Failed);
    assert_eq!(tries(), 8, "a config reload is not a retry");
    // `/mcp reload` went with c1db9191's `/mcp`: it says where, and starts
    // nothing.
    runtime.run(Job::Command("reload".into()));
    assert_eq!(
        notices(&runtime),
        ["/mcp reload is gone: xfx reloads after add, remove, approve, and reject."]
    );
    assert_eq!(tries(), 8, "a verb that is gone started a server");
    assert_eq!(settled(&runtime, "broken").status(), Status::Failed);
    runtime.shutdown();
}

#[test]
fn a_server_that_answers_401_is_asked_once_and_needs_a_login() {
    // A sign-in is not something another try brings: one request, then
    // `NeedsAuth` (`worth_retrying`).
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let asked = Arc::new(Mutex::new(0usize));
    {
        let asked = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let asked = Arc::clone(&asked);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let mut length = 0usize;
                        loop {
                            let mut header = String::new();
                            if reader.read_line(&mut header).unwrap_or(0) == 0 {
                                return;
                            }
                            let header = header.trim_end();
                            if header.is_empty() {
                                break;
                            }
                            if let Some((name, value)) = header.split_once(':') {
                                if name.eq_ignore_ascii_case("content-length") {
                                    length = value.trim().parse().unwrap_or(0);
                                }
                            }
                        }
                        let mut body = vec![0u8; length];
                        if reader.read_exact(&mut body).is_err() {
                            return;
                        }
                        *asked.lock().unwrap() += 1;
                        let _ = stream
                            .write_all(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n");
                    }
                });
            }
        });
    }
    let home = Home::new();
    config::add_profile_server(
        &home.profile(),
        ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp")),
    )
    .expect("saved");
    let runtime = home.runtime();
    runtime.start();
    let view = settled(&runtime, "remote");
    assert_eq!(view.status(), Status::NeedsAuth, "{view:?}");
    assert_eq!(view.failure.as_deref(), Some("it needs a login"));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(*asked.lock().unwrap(), 1, "it was tried again");
    runtime.shutdown();
}

#[test]
fn a_servers_hostile_stderr_reaches_capability_search_spelled_and_bounded() {
    // fx tells the model the same failure (`MCP server '…' is unavailable:
    // …`, `src/core/mcp_host/model.zig:711@481104fd`, fed by `lastError`,
    // `runtime.zig:2020-2022`): a configured child's stderr tail, here as the
    // spelled text it is on the screen.
    let home = Home::new();
    let mut server = ServerConfig::local(
        "bad",
        "/bin/sh",
        &[
            "-c".into(),
            "printf 'boom\\033[2J\\033]52;c;aGk=\\007\\342\\200\\256gnp\\n' >&2; head -c 150 /dev/zero | tr '\\0' z >&2; exit 3".into(),
        ],
    );
    server.restart_limit = 0;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start();
    assert_eq!(settled(&runtime, "bad").status(), Status::Failed);
    let search = super::catalog::search(&runtime, "anything", Some("bad"));
    let output: Value = serde_json::from_str(&search.output).expect("json");
    let error = output["mcp_error"].as_str().expect("an error");
    for obeyed in ['\u{1b}', '\u{7}', '\u{202e}'] {
        assert!(!error.contains(obeyed), "{error:?}");
    }
    // As the model is sent it: JSON, its quotes as entities (`encode_scalar`).
    let (head, tail) = error
        .split_once("its last stderr lines: &quot;")
        .expect("a tail");
    assert_eq!(
        head,
        "MCP server 'bad' is unavailable: its process exited with code 3; "
    );
    let tail = tail.strip_suffix("&quot;").expect("closed");
    assert!(
        tail.starts_with("boom\\x1b[2J\\x1b]52;c;aGk=\\x07"),
        "{tail:?}"
    );
    assert!(
        tail.ends_with(&format!(" / {}", "z".repeat(150))),
        "{tail:?}"
    );
    assert!(tail.len() <= 240, "{}", tail.len());
    runtime.shutdown();
}

// ---------------------------------------------------------------------------
// MCP-Sc: a failed handshake's own teardown is the stop's to wait for
// ---------------------------------------------------------------------------

/// A server that writes its pid, ignores `SIGTERM` (an ignored signal stays
/// ignored across `exec`) and never answers: its try fails at a 200 ms
/// startup timeout and then reads its stderr, waits for its exit and closes
/// it -- a close that has to wait out the grace and kill it. Gone by itself
/// after 20 s.
fn stubborn_server(name: &str, pid: &Path) -> ServerConfig {
    let mut server = ServerConfig::local(
        name,
        "/bin/sh",
        &[
            "-c".into(),
            "trap '' TERM; echo $$ > \"$1\"; exec sleep 20".into(),
            "sh".into(),
            pid.to_string_lossy().into_owned(),
        ],
    );
    server.startup_timeout_ms = 200;
    server
}

/// One try only, so the failed one is not followed by another.
const ONE_TRY: Retry = Retry {
    attempts: 1,
    first: Duration::from_millis(1),
    cap: Duration::from_millis(1),
};

#[test]
fn a_reload_during_a_failed_handshakes_teardown_returns_only_once_the_child_is_gone() {
    let home = Home::new();
    let pid_file = home.dir.path().join("pid");
    config::add_profile_server(&home.profile(), stubborn_server("slow", &pid_file)).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONE_TRY);
    runtime.start_and_wait(WAIT);
    let pid = pid_written(&pid_file);
    // Past the 200 ms timeout, inside the teardown that follows it.
    std::thread::sleep(Duration::from_millis(350));
    assert!(config::remove_profile_server(&home.profile(), "slow").expect("removed"));
    runtime.reload(super::runtime::Origin::Transcript);
    assert!(
        !running(pid),
        "the reload returned with the old child alive"
    );
    assert!(!group_left(pid), "its group outlived the reload");
    runtime.shutdown();
}

#[test]
fn the_end_of_a_session_during_a_failed_handshakes_teardown_waits_for_the_child() {
    let home = Home::new();
    let pid_file = home.dir.path().join("pid");
    config::add_profile_server(&home.profile(), stubborn_server("slow", &pid_file)).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONE_TRY);
    runtime.start_and_wait(WAIT);
    let pid = pid_written(&pid_file);
    std::thread::sleep(Duration::from_millis(350));
    runtime.shutdown();
    assert!(!running(pid), "the session ended with the child alive");
    assert!(!group_left(pid), "its group outlived the session");
}

/// A hook that holds every try at `point` until released, and says when one
/// got there.
#[cfg(feature = "fault-injection")]
#[allow(clippy::type_complexity)]
fn holding_at(
    point: super::runtime::Hold,
) -> (
    impl Fn(super::runtime::Hold) + Send + Sync + 'static,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Sender<()>,
) {
    let (reached, arrived) = std::sync::mpsc::channel::<()>();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (reached, released) = (Mutex::new(reached), Mutex::new(released));
    let hook = move |at: super::runtime::Hold| {
        if at == point {
            let _ = reached.lock().unwrap().send(());
            let _ = released.lock().unwrap().recv_timeout(WAIT);
        }
    };
    (hook, arrived, release)
}

/// `runtime.shutdown()` on a thread of its own, and a way to know it ended.
#[cfg(feature = "fault-injection")]
fn shutting_down(runtime: &Arc<McpRuntime>) -> std::sync::mpsc::Receiver<()> {
    let (done, ended) = std::sync::mpsc::channel();
    let runtime = Arc::clone(runtime);
    std::thread::spawn(move || {
        runtime.shutdown();
        let _ = done.send(());
    });
    ended
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_stop_waits_for_a_try_held_before_its_hand_over_which_then_closes_its_own_process() {
    // The window between starting a process and handing it to the server: a
    // stop there finds nothing to close, waits for the try (`Live::opening`),
    // and the try, finding the server stopped, closes its own
    // (`Server::hand_over`).
    let home = Home::new();
    let pid_file = home.dir.path().join("pid");
    config::add_profile_server(&home.profile(), holding_server("slow", &pid_file)).expect("saved");
    let runtime = home.runtime();
    let (hook, arrived, release) = holding_at(super::runtime::Hold::BeforeHandOver);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    arrived
        .recv_timeout(WAIT)
        .expect("the try reached its hand-over");
    let pid = pid_written(&pid_file);
    let ended = shutting_down(&runtime);
    assert!(
        ended.recv_timeout(Duration::from_millis(400)).is_err(),
        "the stop did not wait for the try"
    );
    release.send(()).expect("released");
    // At once: the try closes its own process rather than going on to a
    // handshake only its 20-second timeout would end.
    ended
        .recv_timeout(Duration::from_secs(5))
        .expect("the stop ended with the try's own close");
    assert!(
        !running(pid) && !group_left(pid),
        "the try's process outlived the stop"
    );
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_stop_waits_for_a_failed_try_closing_its_own_process() {
    // A failed try has taken its process back and is about to close it: a
    // stop then finds nothing to close and waits for that close.
    let home = Home::new();
    let pid_file = home.dir.path().join("pid");
    let mut server = holding_server("slow", &pid_file);
    server.startup_timeout_ms = 200;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONE_TRY);
    let (hook, arrived, release) = holding_at(super::runtime::Hold::BeforeOwnClose);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    let pid = pid_written(&pid_file);
    arrived
        .recv_timeout(WAIT)
        .expect("the try failed and took its process back");
    let ended = shutting_down(&runtime);
    assert!(
        ended.recv_timeout(Duration::from_millis(400)).is_err(),
        "the stop did not wait for the try's own close"
    );
    assert!(running(pid), "closed before the try was released");
    release.send(()).expect("released");
    ended.recv_timeout(WAIT).expect("the stop ended");
    assert!(
        !running(pid) && !group_left(pid),
        "the try's process outlived the stop"
    );
}

// ---------------------------------------------------------------------------
// MCP-Sd: a stop ends a handshake that has no child to wait for
// ---------------------------------------------------------------------------

/// A loopback HTTP endpoint that takes every request and never answers it;
/// how many it took.
fn silent_http() -> (u16, Arc<Mutex<usize>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let taken = Arc::new(Mutex::new(0usize));
    let counted = Arc::clone(&taken);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            *counted.lock().unwrap() += 1;
            // Held open, unanswered, until the case's process ends.
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut sink = [0u8; 4096];
                while matches!(stream.read(&mut sink), Ok(read) if read > 0) {}
            });
        }
    });
    (port, taken)
}

fn waiting_on(taken: &Mutex<usize>) {
    let deadline = Instant::now() + WAIT;
    while *taken.lock().unwrap() == 0 {
        assert!(Instant::now() < deadline, "the endpoint was never asked");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn silent_http_runtime(home: &Home, port: u16) -> Arc<McpRuntime> {
    let mut server = ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp"));
    server.startup_timeout_ms = 6_000;
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    runtime
}

#[test]
fn a_reload_ends_an_http_handshake_against_a_silent_endpoint_at_once() {
    let (port, taken) = silent_http();
    let home = Home::new();
    let runtime = silent_http_runtime(&home, port);
    waiting_on(&taken);
    assert!(config::remove_profile_server(&home.profile(), "remote").expect("removed"));
    let asked = Instant::now();
    runtime.reload(super::runtime::Origin::Transcript);
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
    runtime.shutdown();
}

#[test]
fn the_end_of_a_session_ends_an_http_handshake_against_a_silent_endpoint_at_once() {
    let (port, taken) = silent_http();
    let home = Home::new();
    let runtime = silent_http_runtime(&home, port);
    waiting_on(&taken);
    let asked = Instant::now();
    runtime.shutdown();
    assert!(
        asked.elapsed() < Duration::from_secs(2),
        "{:?}",
        asked.elapsed()
    );
}

/// A stdio server that never answers, whose standard output a grandchild in
/// a session of its own also holds: ending the server's group leaves the
/// pipe open, so no end of its output says the server is gone. Its pid and
/// the grandchild's are written to `pid` and `stray`; the grandchild sleeps
/// 30 s, and the case kills it by its pid.
fn stray_holding_server(pid: &Path, stray: &Path) -> ServerConfig {
    let mut server = ServerConfig::local(
        "held",
        "/bin/sh",
        &[
            "-c".into(),
            "/usr/bin/perl -e 'use POSIX; setsid(); open my $f, \">\", $ARGV[0] or die; print $f $$; close $f; exec \"sleep\", \"30\"' \"$2\" & echo $$ > \"$1\"; exec sleep 30".into(),
            "sh".into(),
            pid.to_string_lossy().into_owned(),
            stray.to_string_lossy().into_owned(),
        ],
    );
    server.startup_timeout_ms = 6_000;
    server
}

/// Kills the grandchild by its pid, so a case leaves nothing behind.
struct Stray(PathBuf);

impl Drop for Stray {
    fn drop(&mut self) {
        if let Some(pid) = std::fs::read_to_string(&self.0)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .and_then(rustix::process::Pid::from_raw)
        {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
    }
}

#[test]
fn a_stop_ends_a_stdio_handshake_whose_output_a_stray_grandchild_holds() {
    for end in ["reload", "shutdown"] {
        let home = Home::new();
        let (pid_file, stray_file) = (home.dir.path().join("pid"), home.dir.path().join("stray"));
        let _stray = Stray(stray_file.clone());
        config::add_profile_server(
            &home.profile(),
            stray_holding_server(&pid_file, &stray_file),
        )
        .expect("saved");
        let runtime = home.runtime();
        runtime.start_and_wait(WAIT);
        let pid = pid_written(&pid_file);
        pid_written(&stray_file);
        let asked = Instant::now();
        if end == "reload" {
            assert!(config::remove_profile_server(&home.profile(), "held").expect("removed"));
            runtime.reload(super::runtime::Origin::Transcript);
        } else {
            runtime.shutdown();
        }
        assert!(
            asked.elapsed() < Duration::from_secs(2),
            "{end}: {:?}",
            asked.elapsed()
        );
        assert!(
            !running(pid) && !group_left(pid),
            "{end}: the server outlived the stop"
        );
        runtime.shutdown();
    }
}

#[test]
fn a_reload_after_the_end_of_a_session_starts_nothing() {
    let home = Home::new();
    let marker = home.dir.path().join("started");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    runtime.shutdown();
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "late",
            "/bin/sh",
            &[
                "-c".into(),
                ": > \"$1\"; exec /bin/sh \"$2\"".into(),
                "sh".into(),
                marker.to_string_lossy().into_owned(),
                fixture().to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    runtime.reload(super::runtime::Origin::Transcript);
    runtime.run(Job::Command("reload".into()));
    std::thread::sleep(Duration::from_millis(300));
    assert!(runtime.views().is_empty());
    assert!(!marker.exists(), "a server started after the session ended");
}

/// `work` on a thread of its own, and its result on a channel: a case waits
/// for it with a bound, so a deadlock fails the case rather than hanging it.
fn in_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::sync::mpsc::Receiver<T> {
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    ended
}

/// A hook that tells of every arrival at a point in `observe`, and holds the
/// first arrival at `block` until released.
#[cfg(feature = "fault-injection")]
#[allow(clippy::type_complexity)]
fn hooked(
    observe: &[super::runtime::Hold],
    block: super::runtime::Hold,
) -> (
    impl Fn(super::runtime::Hold) + Send + Sync + 'static,
    std::sync::mpsc::Receiver<super::runtime::Hold>,
    std::sync::mpsc::Sender<()>,
) {
    let observe = observe.to_vec();
    let (reached, arrived) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (reached, released) = (Mutex::new(reached), Mutex::new(released));
    let blocked = std::sync::atomic::AtomicBool::new(false);
    let hook = move |at: super::runtime::Hold| {
        if observe.contains(&at) || at == block {
            let _ = reached.lock().unwrap().send(at);
        }
        if at == block && !blocked.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let _ = released.lock().unwrap().recv_timeout(WAIT);
        }
    };
    (hook, arrived, release)
}

/// Waits, at most 10 s, for an arrival at `at`.
#[cfg(feature = "fault-injection")]
fn arrives(arrived: &std::sync::mpsc::Receiver<super::runtime::Hold>, at: super::runtime::Hold) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match arrived.recv_timeout(left) {
            Ok(got) if got == at => return,
            Ok(_) => {}
            Err(_) => panic!("never reached {at:?}"),
        }
    }
}

/// `count` servers that ignore SIGTERM and never answer, so a stop waits out
/// the group's grace, put in the profile; the files their pids go to.
#[cfg(feature = "fault-injection")]
fn stubborn_set(home: &Home, count: usize) -> Vec<PathBuf> {
    let files: Vec<PathBuf> = (0..count)
        .map(|i| home.dir.path().join(format!("p{i}")))
        .collect();
    for (index, file) in files.iter().enumerate() {
        let mut server = stubborn_server(&format!("s{index}"), file);
        server.startup_timeout_ms = 20_000;
        config::add_profile_server(&home.profile(), server).expect("saved");
    }
    files
}

#[cfg(feature = "fault-injection")]
fn pids_written(files: &[PathBuf]) -> Vec<i32> {
    files.iter().map(|file| pid_written(file)).collect()
}

#[cfg(feature = "fault-injection")]
fn remove_all(home: &Home, count: usize) {
    for index in 0..count {
        assert!(
            config::remove_profile_server(&home.profile(), &format!("s{index}")).expect("removed")
        );
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn the_end_of_a_session_stops_what_a_reload_it_outwaited_was_still_retiring() {
    // A reload drops three servers and is held at its first close; the end
    // of the session stops waiting for it at once and must still stop all
    // three: they are on the list it takes.
    use super::runtime::Hold;
    let home = Home::new();
    let files = stubborn_set(&home, 3);
    let runtime = home.runtime();
    let (hook, arrived, release) = hooked(&[Hold::StopWaiting], Hold::StopClosing);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    let pids = pids_written(&files);
    remove_all(&home, 3);
    let reloaded = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || runtime.reload(super::runtime::Origin::Transcript))
    };
    arrives(&arrived, Hold::StopClosing);
    runtime.set_reload_wait(Duration::ZERO);
    let ended = shutting_down(&runtime);
    arrives(&arrived, Hold::StopWaiting);
    release.send(()).expect("released");
    ended.recv_timeout(WAIT).expect("the session ended");
    for pid in &pids {
        assert!(
            !running(*pid) && !group_left(*pid),
            "{pid} outlived the session"
        );
    }
    reloaded.recv_timeout(WAIT).expect("the reload ended");
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_second_stop_of_a_server_waits_for_the_first_ones_close() {
    // One server, a reload's stop holding its connection just before
    // closing it: the session's stop of the same server finds nothing to
    // take and must wait for that close (`Live::closing`).
    use super::runtime::Hold;
    let home = Home::new();
    let files = stubborn_set(&home, 1);
    let runtime = home.runtime();
    let (hook, arrived, release) = hooked(&[Hold::StopWaiting], Hold::StopClosing);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    let pid = pids_written(&files)[0];
    remove_all(&home, 1);
    let reloaded = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || runtime.reload(super::runtime::Origin::Transcript))
    };
    arrives(&arrived, Hold::StopClosing);
    runtime.set_reload_wait(Duration::ZERO);
    let ended = shutting_down(&runtime);
    arrives(&arrived, Hold::StopWaiting);
    release.send(()).expect("released");
    ended.recv_timeout(WAIT).expect("the session ended");
    assert!(
        !running(pid) && !group_left(pid),
        "the session ended before the close did"
    );
    reloaded.recv_timeout(WAIT).expect("the reload ended");
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_reload_marks_every_server_it_drops_before_it_closes_any() {
    // Held at its first close, the reload has already cancelled the second
    // server's handshake: that try ends and closes its own process.
    use super::runtime::Hold;
    let home = Home::new();
    let files = stubborn_set(&home, 2);
    let runtime = home.runtime();
    let (hook, arrived, release) = hooked(&[], Hold::StopClosing);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    let pids = pids_written(&files);
    remove_all(&home, 2);
    let reloaded = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || runtime.reload(super::runtime::Origin::Transcript))
    };
    arrives(&arrived, Hold::StopClosing);
    let deadline = Instant::now() + Duration::from_secs(5);
    while running(pids[1]) {
        assert!(
            Instant::now() < deadline,
            "the second server was not marked before the first closed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // Not asserted: the first server is still alive. Its own try, cancelled
    // by the same marking, closes the connection the held stop took, so it
    // may already be gone too.
    release.send(()).expect("released");
    reloaded.recv_timeout(WAIT).expect("the reload ended");
    assert!(!running(pids[0]) && !group_left(pids[0]));
    runtime.shutdown();
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_reload_queued_behind_one_the_end_of_a_session_outwaited_starts_nothing() {
    // A reload is still stopping a server (its try held in its own close)
    // when a second is asked for, which would start `late`; the session
    // ends, its wait for the reload runs out, and once the hold is released
    // neither pass starts anything (`closed`).
    use super::runtime::Hold;
    let home = Home::new();
    let (pid_file, marker) = (home.dir.path().join("pid"), home.dir.path().join("started"));
    let mut slow = holding_server("slow", &pid_file);
    slow.startup_timeout_ms = 200;
    config::add_profile_server(&home.profile(), slow).expect("saved");
    let runtime = home.runtime();
    runtime.set_retry(ONE_TRY);
    let (hook, arrived, release) = hooked(&[Hold::StopWaiting], Hold::BeforeOwnClose);
    runtime.set_hold(hook);
    runtime.start_and_wait(WAIT);
    let pid = pid_written(&pid_file);
    arrives(&arrived, Hold::BeforeOwnClose);
    assert!(config::remove_profile_server(&home.profile(), "slow").expect("removed"));
    let first = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || runtime.reload(super::runtime::Origin::Transcript))
    };
    arrives(&arrived, Hold::StopWaiting);
    config::add_profile_server(
        &home.profile(),
        ServerConfig::local(
            "late",
            "/bin/sh",
            &[
                "-c".into(),
                ": > \"$1\"; exec /bin/sh \"$2\"".into(),
                "sh".into(),
                marker.to_string_lossy().into_owned(),
                fixture().to_string_lossy().into_owned(),
            ],
        ),
    )
    .expect("saved");
    // Folded into the reload under way, as its next pass.
    runtime.reload(super::runtime::Origin::Transcript);
    runtime.set_reload_wait(Duration::ZERO);
    let ended = shutting_down(&runtime);
    arrives(&arrived, Hold::StopWaiting);
    release.send(()).expect("released");
    ended.recv_timeout(WAIT).expect("the session ended");
    first.recv_timeout(WAIT).expect("the reload ended");
    // Nothing to wait for but an absence: a start would have been asked for
    // before the reload returned, and its process writes at once.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "a server started after the session ended");
    assert!(!running(pid) && !group_left(pid));
}

/// A fixture server that counts its starts in `starts` and writes its pid,
/// with `restart_limit` restarts.
#[cfg(feature = "fault-injection")]
fn counted_server(home: &Home, restart_limit: u8) -> (PathBuf, PathBuf) {
    let (starts, pid_file) = (home.dir.path().join("starts"), home.dir.path().join("pid"));
    let mut server = ServerConfig::local(
        "fixture",
        "/bin/sh",
        &[
            "-c".into(),
            "echo started >> \"$1\"; echo $$ > \"$2\"; exec /bin/sh \"$3\"".into(),
            "sh".into(),
            starts.to_string_lossy().into_owned(),
            pid_file.to_string_lossy().into_owned(),
            fixture().to_string_lossy().into_owned(),
        ],
    );
    server.restart_limit = restart_limit;
    config::add_profile_server(&home.profile(), server).expect("saved");
    (starts, pid_file)
}

/// Kills the server's process and waits until every surface calls it failed.
#[cfg(feature = "fault-injection")]
fn kill_and_see(runtime: &McpRuntime, pid_file: &Path) {
    rustix::process::kill_process(
        rustix::process::Pid::from_raw(pid_in(pid_file)).expect("pid"),
        rustix::process::Signal::KILL,
    )
    .expect("killed");
    let deadline = Instant::now() + WAIT;
    while settled(runtime, "fixture").status() != Status::Failed {
        assert!(Instant::now() < deadline, "its death was never seen");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(feature = "fault-injection")]
fn calling(
    runtime: &Arc<McpRuntime>,
    cancel: Option<crate::gateway::CancelToken>,
) -> std::sync::mpsc::Receiver<Result<Value, super::McpError>> {
    let runtime = Arc::clone(runtime);
    in_thread(move || {
        runtime
            .call_tool("mcp_fixture_echo", json!({"text": "hi"}), cancel.as_ref())
            .map(|(_, _, result)| result)
    })
}

#[cfg(feature = "fault-injection")]
#[test]
fn two_calls_that_find_a_server_gone_spend_one_restart_and_start_one_process() {
    // The first call's restart is held before its process is handed over;
    // the second, finding that try under way, waits for it and takes what
    // it opened, rather than spending a restart of its own and getting the
    // dead connection back.
    use super::runtime::Hold;
    let home = Home::new();
    let (starts, pid_file) = counted_server(&home, 2);
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    let started = || std::fs::read_to_string(&starts).unwrap().lines().count();
    kill_and_see(&runtime, &pid_file);
    let (hook, arrived, release) = hooked(&[Hold::WaitingForTry], Hold::BeforeHandOver);
    runtime.set_hold(hook);
    let a = calling(&runtime, None);
    arrives(&arrived, Hold::BeforeHandOver);
    let b = calling(&runtime, None);
    arrives(&arrived, Hold::WaitingForTry);
    // The held try's process writes its start once its shell runs.
    let deadline = Instant::now() + WAIT;
    while started() < 2 {
        assert!(
            Instant::now() < deadline,
            "the held try's process never ran"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(started(), 2, "the second call started a process of its own");
    release.send(()).expect("released");
    for (name, call) in [("first", a), ("second", b)] {
        let result = call.recv_timeout(WAIT).expect("the call ended");
        assert!(result.is_ok(), "{name}: {result:?}");
    }
    assert_eq!(started(), 2);
    // One restart spent: one is left, and then none.
    runtime.set_hold(|_| {});
    kill_and_see(&runtime, &pid_file);
    assert!(runtime
        .call_tool("mcp_fixture_echo", json!({"text": "hi"}), None)
        .is_ok());
    assert_eq!(started(), 3);
    kill_and_see(&runtime, &pid_file);
    assert!(runtime
        .call_tool("mcp_fixture_echo", json!({"text": "hi"}), None)
        .is_err());
    assert_eq!(started(), 3);
    runtime.shutdown();
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_call_waiting_on_another_calls_restart_ends_when_its_own_turn_is_cancelled() {
    // Esc on the second call ends its wait at once, and only its: the first
    // call's restart goes on and serves the first call.
    use super::runtime::Hold;
    let home = Home::new();
    let (starts, pid_file) = counted_server(&home, 1);
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    assert_eq!(settled(&runtime, "fixture").status(), Status::Ready);
    kill_and_see(&runtime, &pid_file);
    let (hook, arrived, release) = hooked(&[Hold::WaitingForTry], Hold::BeforeHandOver);
    runtime.set_hold(hook);
    let a = calling(&runtime, None);
    arrives(&arrived, Hold::BeforeHandOver);
    let cancel = crate::gateway::CancelToken::new();
    let b = calling(&runtime, Some(cancel.clone()));
    arrives(&arrived, Hold::WaitingForTry);
    cancel.cancel();
    let cancelled = Instant::now();
    let result = b
        .recv_timeout(Duration::from_secs(1))
        .expect("the cancelled call kept waiting");
    assert!(
        cancelled.elapsed() < Duration::from_millis(500),
        "{:?}",
        cancelled.elapsed()
    );
    assert_eq!(result, Err(super::McpError::Named("Cancelled")));
    release.send(()).expect("released");
    assert!(a.recv_timeout(WAIT).expect("the first call ended").is_ok());
    assert_eq!(std::fs::read_to_string(&starts).unwrap().lines().count(), 2);
    runtime.shutdown();
}

#[test]
fn a_restart_closes_the_dead_connection_it_replaces_outside_the_servers_lock() {
    // The first start answers the handshake, then shuts its output and lives
    // on ignoring SIGTERM: its connection is dead, its process is not, and
    // closing it waits out the group's grace. The restart that replaces it
    // must not hold the server's lock for that: the status line reads every
    // server under it four times a second.
    let home = Home::new();
    let tried = home.dir.path().join("tried");
    let server = ServerConfig::local(
        "fixture",
        "/bin/sh",
        &[
            "-c".into(),
            concat!(
                "if [ -e \"$1\" ]; then exec /bin/sh \"$2\"; fi; : > \"$1\"; trap '' TERM; ",
                "id() { printf '%s' \"$1\" | sed -n 's/^{\"jsonrpc\":\"2.0\",\"id\":\\([0-9][0-9]*\\),.*/\\1/p'; }; ",
                "read -r line; printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{\"tools\":{}},\"serverInfo\":{\"name\":\"once\",\"version\":\"1\"}}}\\n' \"$(id \"$line\")\"; ",
                "read -r line; read -r line; ",
                "printf '{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"tools\":[{\"name\":\"echo\",\"inputSchema\":{\"type\":\"object\"}}]}}\\n' \"$(id \"$line\")\"; ",
                "exec >&-; exec sleep 20"
            )
            .into(),
            "sh".into(),
            tried.to_string_lossy().into_owned(),
            fixture().to_string_lossy().into_owned(),
        ],
    );
    config::add_profile_server(&home.profile(), server).expect("saved");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    let deadline = Instant::now() + WAIT;
    while runtime.views()[0].connection != Connection::Failed {
        assert!(Instant::now() < deadline, "its output never ended");
        std::thread::sleep(Duration::from_millis(5));
    }
    let call = {
        let runtime = Arc::clone(&runtime);
        in_thread(move || {
            runtime
                .call_tool("mcp_fixture_echo", json!({"text": "hi"}), None)
                .map(|_| ())
        })
    };
    let mut slowest = Duration::ZERO;
    let result = loop {
        let asked = Instant::now();
        let _ = runtime.views();
        slowest = slowest.max(asked.elapsed());
        match call.try_recv() {
            Ok(result) => break result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(err) => panic!("{err}"),
        }
        assert!(Instant::now() < deadline, "the restart never ended");
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(result.is_ok(), "{result:?}");
    assert!(
        slowest < Duration::from_millis(250),
        "a read of the servers waited {slowest:?}"
    );
    runtime.shutdown();
}

/// A loopback HTTP endpoint that answers `initialize` (an accepted version,
/// tools offered, a session id), takes `notifications/initialized` with 202
/// when `swallow` is 3, and never answers its `swallow`th request; how many
/// requests it took.
fn swallowing_http(swallow: usize) -> (u16, Arc<Mutex<usize>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let taken = Arc::new(Mutex::new(0usize));
    let counted = Arc::clone(&taken);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let counted = Arc::clone(&counted);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut length = 0usize;
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap_or(0) == 0 {
                            return;
                        }
                        let header = header.trim_end();
                        if header.is_empty() {
                            break;
                        }
                        if let Some((name, value)) = header.split_once(':') {
                            if name.eq_ignore_ascii_case("content-length") {
                                length = value.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                    let mut body = vec![0u8; length];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    let index = {
                        let mut taken = counted.lock().unwrap();
                        *taken += 1;
                        *taken
                    };
                    if index >= swallow {
                        // Held: never answered.
                        let mut sink = [0u8; 4096];
                        while matches!(reader.get_mut().read(&mut sink), Ok(read) if read > 0) {}
                        return;
                    }
                    let frame: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let response = if frame.get("id").is_some() {
                        let payload = json!({"jsonrpc": "2.0", "id": frame["id"], "result": {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "swallowing", "version": "1"}}}).to_string();
                        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: s-1\r\nContent-Length: {}\r\n\r\n{payload}", payload.len())
                    } else {
                        "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n".to_string()
                    };
                    let _ = stream.write_all(response.as_bytes());
                }
            });
        }
    });
    (port, taken)
}

#[test]
fn a_stop_ends_an_http_handshake_held_at_its_initialized_notification_or_its_tool_list() {
    // The notification and `tools/list` wait on the stop's cancel as
    // `initialize` does.
    for (step, swallow) in [("notifications/initialized", 2), ("tools/list", 3)] {
        let (port, taken) = swallowing_http(swallow);
        let home = Home::new();
        let mut server = ServerConfig::http("remote", &format!("http://127.0.0.1:{port}/mcp"));
        server.startup_timeout_ms = 20_000;
        config::add_profile_server(&home.profile(), server).expect("saved");
        let runtime = home.runtime();
        runtime.start_and_wait(WAIT);
        let deadline = Instant::now() + WAIT;
        while *taken.lock().unwrap() < swallow {
            assert!(Instant::now() < deadline, "{step}: never asked");
            std::thread::sleep(Duration::from_millis(5));
        }
        let asked = Instant::now();
        runtime.shutdown();
        assert!(
            asked.elapsed() < Duration::from_secs(2),
            "{step}: {:?}",
            asked.elapsed()
        );
    }
}
