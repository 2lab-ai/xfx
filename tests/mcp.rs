//! `/mcp` and the MCP client, end to end: the real binary on a real
//! pseudoterminal, a stdio MCP server written for the test
//! (`support/mcp_fixture.sh`), and the scripted loopback Gateway. No network.
//!
//! The claims are the ones a person and a model can check: the menu names the
//! configured server and its state, a turn's request carries upstream's servers
//! block and meta-tools and -- once a search selects it, or the model calls it
//! by name -- the server's own tool, and the tool's answer is what reaches the
//! next request.

#![cfg(unix)]

mod support;

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use support::fake_gateway::{content_only, finish, sse_body, tool_call, FakeGateway, Reply};
use support::pty::{Pty, Session};
use support::sandbox::Sandbox;

/// What every frame the TUI paints starts with once it is up.
const READY: &str = "\u{1b}[?2004h";

/// The sandbox's model as the hint row names it.
const MODEL: &str = "glm-5.2";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mcp_fixture.sh")
}

/// The profile, owner-only as xfx requires, with `mcp` and `settings`.
fn profile(sandbox: &Sandbox, mcp: &Value, settings: &Value) {
    use std::os::unix::fs::PermissionsExt;
    let dir = sandbox.home.join(".xfx");
    std::fs::create_dir_all(&dir).expect("profile dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("0700");
    for (name, value) in [("mcp.json", mcp), ("settings.json", settings)] {
        let path = dir.join(name);
        std::fs::write(&path, value.to_string()).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("0600");
    }
}

fn tui(sandbox: &Sandbox, gateway: &FakeGateway) -> std::process::Command {
    let mut command = sandbox.command_with(gateway);
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    command
}

fn system_texts(body: &Value) -> Vec<String> {
    body["prompt"]
        .as_array()
        .expect("a prompt")
        .iter()
        .filter(|message| message["role"] == "system")
        .filter_map(|message| message["content"].as_str().map(str::to_string))
        .collect()
}

/// Opens `/mcp`, waits for the fixture's row to say it is ready, and closes
/// the menu again, so a turn typed next finds the server up.
fn wait_until_ready_in_the_menu(session: &mut Session) {
    session.type_bytes(b"/mcp\r");
    session.wait_for("MCP servers");
    session.wait_for("ready, 1 tool");
    close_the_menu(session);
}

/// Esc until the menu is closed: each is a lone Esc once it has 50 ms of
/// quiet (a letter typed before that would make it Alt+letter). The hint row
/// painted back -- the model's name on it, which the open menu's key help
/// replaced -- is what says it closed.
fn close_the_menu(session: &mut Session) {
    let shown = session.text().matches(MODEL).count();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while session.text().matches(MODEL).count() <= shown {
        assert!(
            std::time::Instant::now() < deadline,
            "the menu never closed"
        );
        session.type_bytes(b"\x1b");
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// A key the menu takes, and a moment for the frame it asks for. An Enter
/// waits past the 500 ms in which a second Enter reads as the first one held
/// down and answers no question (`MCP_ENTER_HELD`).
fn press(session: &Session, key: &[u8]) {
    session.type_bytes(key);
    let settle = if key == ENTER { 700 } else { 300 };
    std::thread::sleep(std::time::Duration::from_millis(settle));
}

const DOWN: &[u8] = b"\x1b[B";
const ENTER: &[u8] = b"\r";

fn tool_names(body: &Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn a_profile_server_is_ready_in_the_menu_and_its_tool_runs_once_a_search_selects_it() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call(
                "s1",
                "capability_search",
                json!({"query": "echo text back"}),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            tool_call("e1", "mcp_fixture_echo", json!({"text": "hi"})),
            finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["MARKER-MCP-DONE"])),
    ]);
    let sandbox = Sandbox::new();
    profile(
        &sandbox,
        &json!({"mcp": {"fixture": {"type": "local", "command": ["/bin/sh", fixture()]}}}),
        // A rule naming the tool admits its call in `auto`, which otherwise
        // holds a server's tool (there is no reviewer to clear it).
        &json!({"permission": {"mcp_fixture_echo": "allow"}}),
    );
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    session.wait_for(READY);
    wait_until_ready_in_the_menu(&mut session);

    session.type_bytes(b"use the echo tool\r");
    session.wait_for("MARKER-MCP-DONE");
    session.wait_for("Ran ");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));

    let requests = gateway.requests();
    assert_eq!(requests.len(), 3, "one request per step");
    let first = requests[0].json();
    // No `tools="1"` yet: fx v0.0.16 names a count only once its catalog has
    // read the server's list (`p5-mcp-turn`'s bodies).
    assert!(
        system_texts(&first).iter().any(|text| text.ends_with(
            "<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" />\n</mcp_servers>\n"
        )),
        "{first:#}"
    );
    let names = tool_names(&first);
    for meta in ["capability_search", "mcp_select_tool"] {
        assert!(names.iter().any(|name| name == meta), "{names:?}");
    }
    assert!(
        !names.iter().any(|name| name == "mcp_features"),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "mcp_fixture_echo"),
        "advertised before it was selected"
    );

    let second = requests[1].json();
    let echo = second["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mcp_fixture_echo")
        .expect("the selected tool is advertised from the next step on");
    assert_eq!(
        echo["inputSchema"],
        json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})
    );

    let third = requests[2].json();
    let wire = third.to_string();
    assert!(
        wire.contains(r#"{\"server\":\"fixture\",\"tool\":\"mcp_fixture_echo\",\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"FIXTURE_ECHO:hi\"}],\"isError\":false}}"#),
        "{third:#}"
    );
}

#[test]
fn a_tool_the_model_calls_by_name_runs_without_a_search_and_its_count_is_known_next_turn() {
    // fx v0.0.16 (`0fb5cb8e`, `p5-mcp-direct`): a live tool called with no
    // search or selection is selected then, judged as any MCP call -- here a
    // rule admits it in `auto` -- and run; its schema is in the next request,
    // and the next turn's servers block counts the server's tools.
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("e1", "mcp_fixture_echo", json!({"text": "hi"})),
            finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["MARKER-MCP-DIRECT"])),
        Reply::Sse(content_only(&["MARKER-MCP-NEXT"])),
    ]);
    let sandbox = Sandbox::new();
    profile(
        &sandbox,
        &json!({"mcp": {"fixture": {"type": "local", "command": ["/bin/sh", fixture()]}}}),
        &json!({"permission": {"mcp_fixture_echo": "allow"}}),
    );
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    session.wait_for(READY);
    wait_until_ready_in_the_menu(&mut session);

    session.type_bytes(b"call the echo tool\r");
    session.wait_for("MARKER-MCP-DIRECT");
    session.type_bytes(b"and again\r");
    session.wait_for("MARKER-MCP-NEXT");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));

    let requests = gateway.requests();
    assert_eq!(requests.len(), 3, "two steps, then one");
    let first = requests[0].json();
    assert!(!tool_names(&first)
        .iter()
        .any(|name| name == "mcp_fixture_echo"));
    let unread = "<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" />\n</mcp_servers>\n";
    assert!(
        system_texts(&first)
            .iter()
            .any(|text| text.ends_with(unread)),
        "{first:#}"
    );

    let second = requests[1].json();
    assert!(
        tool_names(&second)
            .iter()
            .any(|name| name == "mcp_fixture_echo"),
        "the tool called by name is advertised from the next step on"
    );
    assert!(
        second.to_string().contains(r#"{\"server\":\"fixture\",\"tool\":\"mcp_fixture_echo\",\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"FIXTURE_ECHO:hi\"}],\"isError\":false}}"#),
        "{second:#}"
    );
    // The block is the turn's, so the second step still has no count.
    assert!(
        system_texts(&second)
            .iter()
            .any(|text| text.ends_with(unread)),
        "{second:#}"
    );

    let next = requests[2].json();
    assert!(
        system_texts(&next).iter().any(|text| text.ends_with(
            "<mcp_servers>\n  <server name=\"fixture\" state=\"ready\" tools=\"1\" />\n</mcp_servers>\n"
        )),
        "{next:#}"
    );
    // A new turn starts with nothing selected.
    assert!(!tool_names(&next)
        .iter()
        .any(|name| name == "mcp_fixture_echo"));
}

#[test]
fn a_project_server_waits_unasked_and_the_menu_rejects_it_into_settings() {
    // fx v0.0.16 has no launch prompt (`ba0ea4a9`): the server waits, the
    // status line says so, and `/mcp`'s menu is where it is approved or
    // rejected -- Reject asking on its row first.
    let gateway = FakeGateway::start(Vec::new());
    let sandbox = Sandbox::new();
    profile(&sandbox, &json!({"mcp": {}}), &json!({}));
    std::fs::write(
        sandbox.workspace.join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .expect("project config");
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    session.wait_for(READY);
    // The status line says so on its right, as fx v0.0.16's does
    // (`dismiss-settled`): no server is still starting, so at once.
    session.wait_for("proj is waiting for approval \u{b7} /mcp");
    assert!(
        !session.text().contains("Project MCP server"),
        "a launch prompt went up"
    );
    session.type_bytes(b"/mcp\r");
    session.wait_for("Needs attention");
    press(&session, ENTER);
    session.wait_for("/mcp approve proj");
    press(&session, DOWN);
    press(&session, ENTER);
    session.wait_for("xfx won't start it in this project.");
    // A letter at the question is a no, and reaches nothing else.
    press(&session, b"x");
    press(&session, ENTER);
    session.wait_for_count("start it in this project.", 2);
    press(&session, ENTER);
    let saved = || -> Value {
        std::fs::read_to_string(sandbox.home.join(".xfx/settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    };
    let key = sandbox.workspace.to_str().unwrap().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while saved()["workspaces"][&key]["disabledMcpjsonServers"] != json!(["proj"]) {
        assert!(
            std::time::Instant::now() < deadline,
            "never saved: {}",
            saved()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // A choice made in the menu says nothing in the transcript.
    assert!(!session.text().contains("Rejected proj"));
    close_the_menu(&mut session);
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_eq!(
        gateway.request_count(),
        0,
        "a trust answer is never a prompt"
    );
}

#[test]
fn a_project_server_the_model_rewrites_after_approval_waits_to_be_approved_again() {
    // The menu's Approve approves the entry its screen showed. The model then
    // replaces that entry's command with a plain file write -- which `auto`
    // would hold (SEC-C5), so the person's own rule allows file edits -- and
    // the reload that follows holds the new entry back, waiting, instead of
    // running it; the menu shows the new command.
    let sandbox = Sandbox::new();
    let ran = sandbox.home.join("replacement-ran");
    let replaced = json!({"mcpServers": {"proj": {
        "command": "/bin/sh",
        "args": ["-c", ": > \"$0\"; exec /bin/sh \"$1\"", ran, fixture()],
    }}})
    .to_string();
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("r1", "read_file", json!({"path": ".mcp.json"})),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[
            tool_call(
                "w1",
                "write_file",
                json!({"path": ".mcp.json", "content": replaced}),
            ),
            finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["MARKER-MCP-REWRITTEN"])),
    ]);
    profile(
        &sandbox,
        &json!({"mcp": {}}),
        &json!({"permission": {"edit": "allow"}}),
    );
    std::fs::write(
        sandbox.workspace.join(".mcp.json"),
        json!({"mcpServers": {"proj": {"command": "/bin/sh", "args": [fixture()]}}}).to_string(),
    )
    .expect("project config");
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    session.wait_for(READY);
    session.wait_for("proj is waiting for approval \u{b7} /mcp");
    session.type_bytes(b"/mcp\r");
    session.wait_for("Needs attention");
    press(&session, ENTER);
    press(&session, ENTER);
    session.wait_for("Approve proj for this project? It runs:");
    press(&session, ENTER);
    // Approved and up: its tools load on its screen.
    session.wait_for("Tools 1");
    close_the_menu(&mut session);

    session.type_bytes(b"tidy the MCP config\r");
    session.wait_for("MARKER-MCP-REWRITTEN");
    assert!(
        std::fs::read_to_string(sandbox.workspace.join(".mcp.json"))
            .expect("the file")
            .contains("exec /bin/sh"),
        "the model's write did not land"
    );
    // fx v0.0.16 reloads after add, remove, approve and reject, and has no
    // `/mcp reload`: an add is the reload that reads the model's rewrite.
    session.type_bytes(b"/mcp add web http://127.0.0.1:9/mcp\r");
    session.wait_for("Added web to ~/.xfx/mcp.json.");
    session.type_bytes(b"/mcp\r");
    session.wait_for_count("Needs attention", 2);
    session.wait_until("proj waiting again", |text| {
        unescaped(text).contains("proj                    waiting for approval    project")
    });
    // proj is the attention group's only row; ↓ past web to it.
    let shown = session.text().len();
    press(&session, DOWN);
    press(&session, ENTER);
    press(&session, ENTER);
    session.wait_until("the approval of the rewritten entry", |text| {
        text[shown..].contains("Approve proj for this project? It runs:")
    });
    // Drawn whole again (Ctrl-L): the band is otherwise only the cells that
    // changed.
    let shown = session.text().len();
    session.type_bytes(b"\x0c");
    session.wait_until("the rewritten entry's command", |text| {
        unescaped(&text[shown..]).contains("exec /bin/sh")
    });
    // Not approved: Esc goes back, and Reject rejects it.
    press(&session, b"\x1b");
    press(&session, DOWN);
    press(&session, ENTER);
    press(&session, ENTER);
    let saved = || -> Value {
        std::fs::read_to_string(sandbox.home.join(".xfx/settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    };
    let key = sandbox.workspace.to_str().unwrap().to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while saved()["workspaces"][&key]["disabledMcpjsonServers"] != json!(["proj"]) {
        assert!(
            std::time::Instant::now() < deadline,
            "never saved: {}",
            saved()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    close_the_menu(&mut session);
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert!(
        !ran.exists(),
        "the rewritten entry ran without being approved"
    );
    let entry = &saved()["workspaces"][&key];
    assert_eq!(entry.get("enabledMcpjsonServers"), None, "{entry:#}");
    assert_eq!(gateway.requests().len(), 3, "one request per step");
}

/// `text` with the terminal's control sequences taken out -- CSI (`ESC [ … final`)
/// and OSC (`ESC ] … BEL`) -- so rows a long line was wrapped onto read as the
/// one line they were cut from.
fn unescaped(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                for next in chars.by_ref() {
                    if next == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn a_long_command_or_address_is_shown_whole_on_the_approval_screen_of_a_narrow_terminal() {
    // The approval's screen is where the person sees what they approve, so a
    // 300-character command line and a long address are never cut on an
    // 80-column terminal: each is wrapped at spaces -- an address with none,
    // at the width -- onto as many rows as it needs, all of it above the
    // hint row Enter answers.
    let gateway = FakeGateway::start(Vec::new());
    let sandbox = Sandbox::new();
    profile(&sandbox, &json!({"mcp": {}}), &json!({}));
    let words: Vec<String> = (1..=58).map(|i| format!("a{i:02}")).collect();
    let script = format!("echo {} TAILCMD", words.join(" "));
    let segments: Vec<String> = (1..=40).map(|i| format!("p{i:02}")).collect();
    let url = format!("https://example.test/{}/TAILURL", segments.join("/"));
    std::fs::write(
        sandbox.workspace.join(".mcp.json"),
        json!({"mcpServers": {
            "long": {"command": "/bin/sh", "args": ["-c", script]},
            "remote": {"type": "http", "url": url},
        }})
        .to_string(),
    )
    .expect("project config");
    let pty = Pty::open();
    pty.resize(40, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    session.wait_for(READY);
    session.type_bytes(b"/mcp\r");
    session.wait_for("Needs attention");
    let hint = "enter confirm     esc cancel";
    // The band is drawn as the cells that changed since the last frame, so
    // once the approval is up Ctrl-L asks for the whole screen again: every
    // row then written whole.
    let approval = |session: &Session, from: usize, question: &str| -> String {
        let up = |from: usize| {
            move |text: &str| {
                text[from..]
                    .find(question)
                    .is_some_and(|at| text[from + at..].contains(hint))
            }
        };
        session.wait_until("the approval's screen and its hint", up(from));
        let from = session.text().len();
        session.type_bytes(b"\x0c");
        let text = session.wait_until("the approval redrawn whole", up(from));
        let text = &text[from..];
        let at = text.find(question).expect("the question");
        let end = at + text[at..].find(hint).expect("the hint");
        unescaped(&text[at..end])
    };
    let from = session.text().len();
    press(&session, ENTER);
    press(&session, ENTER);
    let shown = approval(&session, from, "Approve long for this project? It runs:");
    let mut rest = shown.as_str();
    for word in ["/bin/sh", "-c", "'echo"]
        .into_iter()
        .chain(words.iter().map(String::as_str))
        .chain(["TAILCMD'"])
    {
        let at = rest
            .find(word)
            .unwrap_or_else(|| panic!("{word:?} is not on the screen in order:\n{shown}"));
        rest = &rest[at + word.len()..];
    }
    assert!(!shown.contains('\u{2026}'), "cut: {shown}");
    press(&session, b"\x1b");
    press(&session, b"\x1b");
    let from = session.text().len();
    press(&session, DOWN);
    press(&session, ENTER);
    press(&session, ENTER);
    let shown = approval(
        &session,
        from,
        "Approve remote for this project? It connects to:",
    );
    // Every piece after the first is led by `↳ `, so no piece of an address
    // reads as a row of its own; without the leads it is the address whole.
    assert!(
        shown.contains("\u{21b3} /p"),
        "a piece is not led:\n{shown}"
    );
    let joined: String = shown
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{21b3}')
        .collect();
    assert!(joined.contains(&url), "the address is not whole:\n{shown}");
    close_the_menu(&mut session);
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
}
