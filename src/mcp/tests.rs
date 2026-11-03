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
    let entry = json!({"command": "/bin/sh", "args": [fixture()]});
    std::fs::write(
        home.workspace().join(".mcp.json"),
        json!({"mcpServers": {"proj": entry}}).to_string(),
    )
    .unwrap();
    let runtime = home.runtime();
    runtime.start();
    let events = runtime.take_events();
    assert!(
        events.contains(&Event::Raw(format!(
            "Project MCP server 'proj' is defined in .mcp.json.\n  command: /bin/sh {}\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
            fixture().display()
        ))),
        "{events:?}"
    );
    let digest = config::entry_digest(&entry);
    assert!(events.contains(&Event::TrustPrompt {
        name: "proj".into(),
        digest: digest.clone(),
        listed: vec![("proj".into(), digest.clone())],
        whole: true,
    }));
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
    assert!(runtime
        .take_events()
        .iter()
        .any(|event| matches!(event, Event::TrustPrompt { name, .. } if name == "proj")));
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
    runtime.start();
    assert_eq!(settled(&runtime, "shown").state_label(), "Pending trust");
    assert_eq!(runtime.issues(), ["templated: McpConfigInvalidType"]);
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
    runtime.start();
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

    bounded_run(&runtime, Job::Command("reload".into()));
    let view = settled(&runtime, "proj");
    assert!(
        !ran.exists(),
        "the rewritten entry ran without a new prompt"
    );
    assert_eq!(
        view.state_label(),
        "Pending trust",
        "the rewritten entry kept the approval: {view:?}"
    );
    // The prompt names what would run, not only the name.
    let events = runtime.take_events();
    let shown = format!(
        "Project MCP server 'proj' is defined in .mcp.json.\n  command: /bin/sh -c ': > \"$0\"; exec /bin/sh \"$1\"' {} {}\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
        ran.display(),
        fixture().display()
    );
    assert!(events.contains(&Event::Raw(shown)), "{events:?}");
    let Some(Event::TrustPrompt { digest, .. }) = events
        .iter()
        .find(|event| matches!(event, Event::TrustPrompt { .. }))
    else {
        panic!("no prompt: {events:?}")
    };
    // An answer to a prompt that showed the old entry approves the old entry,
    // which is no longer there.
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
fn a_hostile_command_is_named_in_the_trust_prompt_as_the_approval_view_names_it() {
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
    let events = runtime.take_events();
    let prompt = events
        .iter()
        .find_map(|event| match event {
            Event::Raw(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no prompt: {events:?}"));
    assert_eq!(
        prompt,
        "Project MCP server 'proj' is defined in .mcp.json.\n  command: /bin/sh -c 'true \\u{202e}hs.live cexe\\u{200b}'\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
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
    // Full access offers all three, and the search runs.
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
            "indirect: McpConfigInvalidBearerEnvironment".to_string(),
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
    let skipped = |server: &str| format!(".mcp.json server '{server}' was skipped: invalid_entry.");
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

#[test]
fn an_http_server_nothing_listens_on_fails_as_connection_refused_with_upstreams_counts() {
    // A port bound and released: nothing listens on it now.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.local_addr().unwrap().port()
    };
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
    assert_eq!(view.failure.as_deref(), Some("ConnectionRefused"));
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
