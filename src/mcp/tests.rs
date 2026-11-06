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
    runtime.start_and_wait(WAIT);
    let events = runtime.take_events();
    let digest = config::entry_digest(&entry);
    assert!(
        events.contains(&Event::TrustPrompt {
            name: "proj".into(),
            digest: digest.clone(),
            listed: vec![("proj".into(), digest.clone())],
            whole: true,
            text: format!(
                "Project MCP server 'proj' is defined in .mcp.json.\n  command: /bin/sh {}\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
                fixture().display()
            ),
        }),
        "{events:?}"
    );
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
fn a_change_to_a_non_first_pending_server_sends_the_prompt_again_unasked() {
    // The prompt is about the first pending server and its `2` about all of
    // them: a reload that changes only the second sends it again, with no
    // key pressed; a reload that changes nothing sends nothing.
    let home = Home::new();
    let quiet = |mark: &str| json!({"command": "/bin/sh", "args": ["-c", "exit 0", mark]});
    let write = |b: &str| {
        std::fs::write(
            home.workspace().join(".mcp.json"),
            json!({"mcpServers": {"a": quiet("a1"), "b": quiet(b)}}).to_string(),
        )
        .unwrap()
    };
    write("b1");
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    settled(&runtime, "a");
    let prompts = |events: Vec<Event>| -> Vec<Vec<(String, String)>> {
        events
            .into_iter()
            .filter_map(|event| match event {
                Event::TrustPrompt { listed, .. } => Some(listed),
                _ => None,
            })
            .collect()
    };
    let _ = runtime.take_events();
    bounded_run(&runtime, Job::Command("reload".into()));
    assert_eq!(
        prompts(runtime.take_events()),
        Vec::<Vec<(String, String)>>::new()
    );
    write("b2");
    bounded_run(&runtime, Job::Command("reload".into()));
    assert_eq!(
        prompts(runtime.take_events()),
        [vec![
            ("a".to_string(), config::entry_digest(&quiet("a1"))),
            ("b".to_string(), config::entry_digest(&quiet("b2"))),
        ]]
    );
    runtime.shutdown();
}

#[test]
fn approving_a_listed_set_saves_only_the_entries_still_pending_as_listed() {
    // What persistence does with a list some of which no longer stands: of
    // what was listed one entry is unchanged, one rewritten, one removed, one
    // rejected and one approved in the meantime, and one entry was added --
    // only the unchanged one is approved, and each other is named with what
    // became of it.
    let home = Home::new();
    let quiet = |mark: &str| json!({"command": "/bin/sh", "args": ["-c", "exit 0", mark]});
    let write = |servers: Value| {
        std::fs::write(
            home.workspace().join(".mcp.json"),
            json!({"mcpServers": servers}).to_string(),
        )
        .unwrap()
    };
    write(json!({
        "same": quiet("s"), "rewritten": quiet("r1"), "gone": quiet("g"),
        "refused": quiet("f"), "taken": quiet("t"),
    }));
    let runtime = home.runtime();
    runtime.start_and_wait(WAIT);
    settled(&runtime, "same");
    let listed: Vec<(String, String)> = [
        ("same", "s"),
        ("rewritten", "r1"),
        ("gone", "g"),
        ("refused", "f"),
        ("taken", "t"),
    ]
    .into_iter()
    .map(|(name, mark)| (name.to_string(), config::entry_digest(&quiet(mark))))
    .collect();
    write(json!({
        "same": quiet("s"), "rewritten": quiet("r2"), "refused": quiet("f"),
        "taken": quiet("t"), "added": quiet("a"),
    }));
    bounded_run(&runtime, Job::Command("reload".into()));
    let deadline = Instant::now() + WAIT;
    while !runtime.views().iter().any(|view| view.name == "added") {
        assert!(
            Instant::now() < deadline,
            "the reload never read the new file"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    for action in [
        TrustAction::Reject("refused".into()),
        TrustAction::Approve("taken".into()),
    ] {
        bounded_run(
            &runtime,
            Job::Trust {
                action,
                body: "In the meantime.".into(),
            },
        );
    }
    let _ = runtime.take_events();
    bounded_run(
        &runtime,
        Job::Trust {
            action: TrustAction::ApproveListed(listed),
            body: "Approving all project MCP servers for this workspace.".into(),
        },
    );
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(home.settings()).unwrap()).unwrap();
    assert_eq!(
        document["workspaces"][home.workspace().to_str().unwrap()]["enabledMcpjsonServerDigests"],
        json!({
            "taken": config::entry_digest(&quiet("t")),
            "same": config::entry_digest(&quiet("s")),
        })
    );
    let said = notices(&runtime);
    for expected in [
        "Approving all project MCP servers for this workspace.",
        "Project MCP server 'rewritten' changed since it was listed; it is still waiting for approval.",
        "Project MCP server 'gone' is no longer a server in .mcp.json; there was nothing to approve.",
        "Project MCP server 'refused' was rejected in the meantime; it was not approved.",
        "Project MCP server 'taken' was approved in the meantime.",
        "Project MCP server 'added' was added since the list was drawn; it is still waiting for approval.",
    ] {
        assert!(said.iter().any(|body| body == expected), "{expected}: {said:?}");
    }
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
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::TrustPrompt { text, .. } if *text == shown)),
        "{events:?}"
    );
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
            Event::TrustPrompt { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no prompt: {events:?}"));
    assert_eq!(
        prompt,
        "Project MCP server 'proj' is defined in .mcp.json.\n  command: /bin/sh -c 'true \\u{202e}hs.live cexe\\u{200b}'\n  [1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts\n",
    );
    runtime.shutdown();
}

#[test]
fn an_entry_that_would_push_its_command_row_off_the_screen_is_not_shown_whole() {
    // Each variable is a row of its own: none is too long, but enough of them
    // push the command row above an 80x24 screen before the choices are
    // drawn. `1` approves only an entry shown whole, so such a prompt sends
    // the person to the details and `v`.
    let whole = |count: usize| -> bool {
        let home = Home::new();
        let env: serde_json::Map<String, Value> = (0..count)
            .map(|at| (format!("V{at}"), json!("x")))
            .collect();
        std::fs::write(
            home.workspace().join(".mcp.json"),
            json!({"mcpServers": {"proj": {"command": "node", "env": env}}}).to_string(),
        )
        .unwrap();
        let runtime = home.runtime();
        runtime.start();
        let events = runtime.take_events();
        runtime.shutdown();
        events
            .iter()
            .find_map(|event| match event {
                Event::TrustPrompt { whole, .. } => Some(*whole),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no prompt: {events:?}"))
    };
    // The name row, the command row, 18 variables and the choices: the 21
    // rows the screen's 24 leave over the band's three.
    assert!(whole(18));
    assert!(!whole(19));
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
    runtime.start_and_wait(WAIT);
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
fn an_http_server_nothing_listens_on_fails_as_connection_refused_with_upstreams_counts() {
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
