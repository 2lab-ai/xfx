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
    session.wait_for("[Servers]");
    // The row may first be painted `Reloading` while the server starts; the
    // frame that follows rewrites only the cells that change, `ady    `.
    session.wait_until("the server row to say Ready", |text| {
        text.contains("Ready") || (text.contains("Reloading") && text.contains("ady    "))
    });
    session.wait_for("stdio \u{b7} Profile");
    // Esc closes the menu once it is known to be a lone Esc (50 ms of
    // quiet); a letter typed before that would make it Alt+letter. The hint
    // row painted back -- the model's name on it, which the open menu's key
    // help replaced -- is what says it settled.
    let shown = session.text().matches(MODEL).count();
    session.type_bytes(b"\x1b");
    session.wait_for_count(MODEL, shown + 1);
}

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
fn a_project_server_asks_first_and_three_rejects_it_into_settings() {
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
    session.wait_for("Project MCP server 'proj' is defined in .mcp.json.");
    session.wait_for("[1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts");
    // A letter typed at the prompt is not a draft: the prompt owns the keys.
    session.type_bytes(b"x3");
    session.wait_for("Rejecting project MCP server 'proj'.");
    session.wait_for("MCP configuration reloaded successfully.");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.home.join(".xfx/settings.json")).expect("settings"),
    )
    .expect("json");
    assert_eq!(
        settings["workspaces"][sandbox.workspace.to_str().unwrap()]["disabledMcpjsonServers"],
        json!(["proj"])
    );
    assert_eq!(
        gateway.request_count(),
        0,
        "a trust answer is never a prompt"
    );
}

#[test]
fn a_project_server_the_model_rewrites_after_approval_is_shown_and_asked_about_again() {
    // `1` approves the entry the prompt showed. The model then replaces that
    // entry's command with a plain file write -- which `auto` would hold
    // (SEC-C5), so the person's own rule allows file edits -- and
    // the reload that follows asks again, showing the new command, instead
    // of running it.
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
    session.wait_for("Project MCP server 'proj' is defined in .mcp.json.");
    session.wait_for("command: /bin/sh /");
    session.wait_for("[1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts");
    session.type_bytes(b"1");
    session.wait_for("Approving project MCP server 'proj'.");
    session.wait_for("MCP configuration reloaded successfully.");

    session.type_bytes(b"tidy the MCP config\r");
    session.wait_for("MARKER-MCP-REWRITTEN");
    assert!(
        std::fs::read_to_string(sandbox.workspace.join(".mcp.json"))
            .expect("the file")
            .contains("exec /bin/sh"),
        "the model's write did not land"
    );
    session.type_bytes(b"/mcp reload\r");
    session.wait_for("command: /bin/sh -c");
    session.type_bytes(b"3");
    session.wait_for("Rejecting project MCP server 'proj'.");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
    assert!(
        !ran.exists(),
        "the rewritten entry ran without being asked about"
    );
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(sandbox.home.join(".xfx/settings.json")).expect("settings"),
    )
    .expect("json");
    let entry = &settings["workspaces"][sandbox.workspace.to_str().unwrap()];
    assert_eq!(
        entry["disabledMcpjsonServers"],
        json!(["proj"]),
        "{settings:#}"
    );
    assert_eq!(entry.get("enabledMcpjsonServers"), None, "{settings:#}");
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
fn a_long_command_or_address_reaches_a_narrow_screen_whole_before_the_choices() {
    // The prompt is the only place the person sees what they are approving,
    // so a 300-character command line and a long address must not be clipped
    // on an 80-column terminal: each is wrapped onto as many rows as it needs,
    // every row after the first led by `↳ ` so none reads as a row of the
    // entry's own, and all of it is written before the row of choices that
    // `1` answers.
    let gateway = FakeGateway::start(Vec::new());
    let sandbox = Sandbox::new();
    profile(&sandbox, &json!({"mcp": {}}), &json!({}));
    let words: Vec<String> = (1..=58).map(|i| format!("a{i:02}")).collect();
    let script = format!("echo {} TAILCMD", words.join(" "));
    let command = format!("command: /bin/sh -c '{script}'");
    assert!(command.len() >= 260, "{}", command.len());
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
    let choices = "[1] approve  [2] approve all  [3] reject  [esc] dismiss remaining prompts";
    let pty = Pty::open();
    pty.resize(40, 80);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, tui(&sandbox, &gateway));
    let whole_before_choices = |text: &str, value: &str| {
        let text = unescaped(text);
        assert!(
            text.contains("\u{21b3} "),
            "no row of {value:?} is led as a continuation:\n{text}"
        );
        // The value as written, the leads it wrapped behind taken out.
        let text = text.replace("\u{21b3} ", "");
        let at = text
            .find(value)
            .unwrap_or_else(|| panic!("{value:?} is not on the screen whole:\n{text}"));
        assert!(
            text[at + value.len()..].contains(choices),
            "the choices came before the end of {value:?}:\n{text}"
        );
    };
    let shown = session.wait_for(choices);
    whole_before_choices(&shown, &command);
    session.type_bytes(b"3");
    session.wait_for("Rejecting project MCP server 'long'.");
    let shown = session.wait_for_count(choices, 2);
    whole_before_choices(&shown, &format!("url: {url}"));
    session.type_bytes(b"3");
    session.wait_for("Rejecting project MCP server 'remote'.");
    session.type_bytes(b"/quit\r");
    assert_eq!(session.wait_exit().code(), Some(0));
}
