//! Acceptance tests for the interactive shell, driven through a real
//! pseudoterminal.
//!
//! The shell is the one xfx surface whose contract is *about* the terminal: it
//! must refuse to run without one, keep the scrollback it was given, leave the
//! line discipline exactly as it found it, and survive an interrupt. None of
//! that can be proven against a pipe pretending to be a TTY, so every test here
//! allocates a pty, spawns the real binary on it, and asserts on the bytes the
//! terminal actually received and on the `termios` state that was left behind.
//!
//! Nothing here uses a real credential or a real endpoint. Upstream evidence is
//! pinned to `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

#![cfg(unix)]

mod support;

use std::fs::{self, OpenOptions};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};

use rustix::process::{Pid, Signal};
use rustix::termios::{ControlModes, InputModes, LocalModes, OutputModes};
use serde_json::{json, Value};

use support::fake_clipboard::FakeClipboard;
use support::fake_gateway::{content_only, sse_body, text_delta, FakeGateway, Reply};
use support::fake_llmux::FakeLlmux;
use support::pty::{modes, try_modes, wait_state, Pty, Session, TerminalState, Wait};
use support::sandbox::{edit_then_finish, edit_then_finish_called, with_notes, Sandbox, TEST_KEY};

/// The prompt the shell writes before reading a line.
const PROMPT: &str = "> ";

/// Starts a shell and waits for its first prompt.
fn start(sandbox: &Sandbox, pty: &Pty, command: Command) -> Session {
    let session = Session::spawn(pty, command);
    session.wait_for(PROMPT);
    let _ = sandbox;
    session
}

/// Types `line` and waits for the answer `reply`, received **once more than
/// it had been** before the line went.
///
/// Not `wait_for(reply)`: the banner prints the workspace, whose temporary
/// directory has a random name (`.tmp48fOS3` holds `ok`) under whatever
/// `TMPDIR` is, so a short reply can be on the terminal before anything was
/// asked, and a test that then reads the Gateway's requests reads none --
/// `slash_model_reports_the_active_model_and_changes_it_for_later_turns`
/// failed so, every run, with `ok` in `TMPDIR`.
fn type_and_wait(session: &Session, line: &str, reply: &str) -> String {
    let seen = session.text().matches(reply).count();
    session.type_line(line);
    session.wait_for_count(reply, seen + 1)
}

/// The JSON body of the request the Gateway received at `index`.
fn request_body(gateway: &FakeGateway, index: usize) -> Value {
    let requests = gateway.requests();
    assert!(
        requests.len() > index,
        "expected at least {} request(s), got {}",
        index + 1,
        requests.len()
    );
    requests[index].json()
}

/// The text of every user message in a request body, in order.
fn user_messages(body: &Value) -> Vec<String> {
    body["prompt"]
        .as_array()
        .expect("the request carries a prompt")
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_array())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| part["text"].as_str())
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------
// a shell needs a terminal
// ---------------------------------------------------------------------------

#[test]
fn a_bare_invocation_without_a_terminal_is_refused_before_anything_runs() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn xfx");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(
        stderr.contains("interactive terminal") && stderr.contains("xfx ask"),
        "the refusal must name the requirement and the alternative: {stderr}"
    );
    // Refusing is not the same as failing halfway through starting: nothing was
    // created under the profile home.
    assert!(!sandbox.home.join(".xfx").exists(), "the home was touched");
}

#[test]
fn a_shell_whose_answers_would_go_into_a_pipe_is_refused() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pty.slave_path)
        .expect("open the pty slave");
    let output = sandbox
        .command()
        .stdin(Stdio::from(slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn xfx");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "stdout must stay empty");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    assert!(stderr.contains("interactive terminal"), "{stderr}");
}

#[test]
fn a_shell_with_nowhere_to_record_says_so_instead_of_forgetting_the_conversation() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command();
    command.env_remove("HOME");
    let mut session = Session::spawn(&pty, command);

    let text = session.wait_for("cannot record");
    assert!(
        text.contains("--no-save"),
        "the refusal must say what is missing and what to use instead: {text}"
    );
    assert_eq!(session.wait_exit().code(), Some(1));
}

// ---------------------------------------------------------------------------
// the prompt loop
// ---------------------------------------------------------------------------

#[test]
fn the_shell_opens_with_what_it_resolved_and_a_prompt() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    let text = session.text();
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
    assert!(text.contains("permission_mode=auto"), "{text}");
    assert!(text.contains("sandbox=none"), "{text}");
    assert!(
        text.contains(&sandbox.workspace.display().to_string()),
        "{text}"
    );
    assert!(text.contains("/help"), "{text}");
    // An append shell never takes the alternate screen and never hides the
    // cursor: the transcript above it has to stay where the user left it.
    assert!(
        !text.contains("\u{1b}[?1049h"),
        "alternate screen: {text:?}"
    );
    assert!(!text.contains("\u{1b}[?25l"), "hidden cursor: {text:?}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn an_empty_line_asks_nothing_and_prompts_again() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // No Gateway at all: a blank line that reached the provider would fail to
    // connect, and the shell would say so.
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("");
    session.type_line("   ");
    let text = session.wait_for_count(PROMPT, 3);
    assert!(!text.contains("xfx:"), "nothing was attempted: {text}");

    assert_eq!(session.quit().code(), Some(0));
    assert!(sandbox.session_ids().is_empty(), "nothing was recorded");
}

#[test]
fn the_shell_streams_an_answer_to_a_unicode_prompt_through_the_ordinary_turn_path() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["안녕", "하세요 — ", "✓"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("설명해줘 — le café ☕");
    let text = session.wait_for("안녕하세요 — ✓");

    // The prompt reached the provider byte for byte, through the same request
    // shape `ask` uses.
    let body = request_body(&gateway, 0);
    assert_eq!(
        user_messages(&body),
        vec!["설명해줘 — le café ☕".to_string()]
    );
    assert!(
        body["tools"].as_array().expect("tools").len() > 1,
        "the shell advertises the same registry as ask"
    );
    assert!(
        !text.contains(TEST_KEY),
        "the credential reached the terminal"
    );

    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(sandbox.session_ids().len(), 1, "the turn was recorded once");
}

#[test]
fn a_second_prompt_continues_the_same_conversation() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["first"])),
        Reply::Sse(content_only(&["second"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "one", "first");
    type_and_wait(&session, "two", "second");

    let body = request_body(&gateway, 1);
    assert_eq!(
        user_messages(&body),
        vec!["one".to_string(), "two".to_string()],
        "the second request carries the first turn"
    );
    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(sandbox.session_ids().len(), 1, "one session, two turns");
}

#[test]
fn a_failed_turn_is_reported_and_the_shell_keeps_going() {
    let gateway = FakeGateway::start(vec![
        Reply::Status(401, "{\"error\":\"unauthorized\"}".to_string()),
        Reply::Sse(content_only(&["recovered"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "first", "401");
    type_and_wait(&session, "second", "recovered");

    assert_eq!(session.quit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// the canonical slash commands
// ---------------------------------------------------------------------------

#[test]
fn slash_permissions_says_the_mode_changes_it_and_records_it() {
    // The line shell's half of `/permissions`: upstream's `[permissions] …`
    // text lines, the same words as the TUI's notices, and the same profile
    // key written.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/permissions");
    let text = session.wait_for("[permissions] /permissions revoke <rule-id>");
    for row in [
        "[permissions] mode=auto",
        "[permissions] configured rules: (none)",
        "[permissions] session grants: (none)",
        "[permissions] saved-session permission rules: none",
        "[permissions] usage: /permissions [ask|auto|full-access|reset]",
        "[permissions] /permissions remember <allow|deny> <tool-name> <arguments-json>",
    ] {
        assert!(text.contains(row), "{row} missing: {text}");
    }

    session.type_line("/permissions Full-Access");
    let text = session.wait_for("mode set to full access");
    assert!(
        text.contains("[permissions] mode set to full access"),
        "{text}"
    );
    let settings = sandbox.home.join(".xfx").join("settings.json");
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&settings).expect("the profile")).expect("json");
    assert_eq!(written, json!({"permission_mode": "yolo"}));

    session.type_line("/permissions");
    session.wait_for("[permissions] mode=full access");

    session.type_line("/permissions reset");
    session.wait_for("[permissions] permissions reset to ask, session grants cleared");
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&settings).expect("the profile")).expect("json");
    assert_eq!(written, json!({"permission_mode": "ask"}));

    session.type_line("/permissions bogus");
    session.wait_for("xfx: usage: /permissions [ask|auto|full-access|reset]");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_permissions_says_when_a_higher_layer_still_decides_a_fresh_session() {
    // Upstream's shadow suffix (`appendShadowedUserSource`,
    // `session_commands.zig:225-236`): the write happened, and the environment
    // still decides what the next launch starts in.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command();
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("/permissions auto");
    let text = session.wait_for("higher-precedence permission_mode=process_override");
    assert!(
        text.contains(
            "xfx: permission-mode: saved to user settings (scope=user); fresh sessions \
             here use higher-precedence permission_mode=process_override"
        ),
        "{text}"
    );
    let settings = sandbox.home.join(".xfx").join("settings.json");
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&settings).expect("the profile")).expect("json");
    assert_eq!(written, json!({"permission_mode": "auto"}));

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_help_lists_exactly_the_canonical_commands() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/help");
    let text = session.wait_for_count(PROMPT, 2);
    // The list is read out of the binary's own declaration rather than spelled
    // here: a name added to `SLASH_COMMANDS` and forgotten by `/help` is what
    // this is for, and a copy of the list in the test could be forgotten in the
    // same edit.
    for command in xfx::interactive::SLASH_COMMANDS {
        assert!(text.contains(command), "help omits {command}: {text}");
    }
    assert!(
        text.contains("/provider") && text.contains("(also /setup)"),
        "the provider command and its alias are not advertised: {text}"
    );
    // Advertisement is a promise here too: no deferred upstream slash command
    // may appear. `/setup` left this list when it stopped being deferred, and
    // `/stats`, `/status` and `/permissions` after it, `/provider` when it
    // became the canonical name of the same command with `/setup` as its
    // alias, `/resume` and `/rename` after those, `/undo` after them, and
    // `/image`, `/images` and `/paste` after that, and `/usage` with its
    // dashboard, and `/mcp` with its client; `/models` did not, because the
    // catalog browser is reached by a bare `/model` rather than by a command
    // of its own.
    assert!(
        text.contains("(also /img)"),
        "the image command's alias is not advertised: {text}"
    );
    for absent in ["/login", "/models"] {
        assert!(!text.contains(absent), "help advertises {absent}: {text}");
    }

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_usage_on_the_line_shell_says_where_the_dashboard_is() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());
    session.type_line("/usage");
    session.wait_for("xfx: /usage shows its dashboard in the full-screen shell");
    // `/cost` is the same command.
    session.type_line("/cost");
    session.wait_for_count("xfx: /usage shows its dashboard", 2);
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_credits_asks_the_gateway_the_turn_would_use_with_the_turns_bearer() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(r#"{"balance":"42.17","used":"7.83","plan":"pro"}"#.to_string()),
        Reply::Status(404, "not found".to_string()),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/credits");
    let text = session.wait_for("[credits] plan=pro");
    assert!(
        text.contains("[credits] balance=42.17\r\n[credits] used=7.83\r\n[credits] plan=pro"),
        "{text}"
    );
    let asked = gateway.only_request();
    assert_eq!(
        (asked.method.as_str(), asked.path.as_str()),
        ("GET", "/coding-agent/v1/credits")
    );
    assert_eq!(
        asked.header("authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );

    // `/balance` is the same command, and a refusal is upstream's wording.
    session.type_line("/balance");
    let text = session.wait_for("[credits] error: HTTP 404: not found");
    assert_eq!(gateway.request_count(), 2);
    assert!(
        !text.contains(TEST_KEY),
        "the credential reached the terminal"
    );
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_credits_with_no_credential_or_on_llmux_asks_nobody() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());
    session.type_line("/credits");
    session.wait_for(
        "[credits] error: xfx needs access to Vercel AI Gateway. Set VERCEL_OIDC_TOKEN or AI_GATEWAY_API_KEY.",
    );
    assert_eq!(session.quit().code(), Some(0));

    // llmux keeps no balance (`gateway_provider.zig:89-101`'s sentence), and
    // the daemon is not asked for one.
    let daemon = FakeLlmux::start(Vec::new());
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));
    session.type_line("/credits");
    session.wait_for("[credits] error: Credits are unavailable for the selected provider.");
    assert!(daemon.requests().is_empty(), "{:?}", daemon.requests());
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_version_prints_the_same_version_the_command_line_does() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/version");
    let text = session.wait_for_count(PROMPT, 2);
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_status_reports_this_shells_own_model_and_conversation() {
    // The `status` command's lines, about the model `/model` chose and the
    // conversation this shell is holding rather than the store's last session.
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/status");
    let text = session.wait_for("agent_step_limit=");
    assert!(text.contains("[status] history_turns=0"), "{text}");
    assert!(text.contains("[status] model=zai/glm-5.2"), "{text}");

    session.type_line("/model acme/model-9");
    session.wait_for("model=acme/model-9");
    type_and_wait(&session, "hello", "ok");
    session.type_line("/status");
    let text = session.wait_for("[status] history_turns=1");
    assert!(text.contains("[status] model=acme/model-9"), "{text}");

    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

#[test]
fn slash_stats_says_where_the_counters_are_rather_than_printing_zeros() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/stats");
    let text = session.wait_for("draws none");
    assert!(!text.contains("ansi_bytes="), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_compact_says_where_compaction_runs_and_asks_nobody() {
    // Upstream's compaction shows on the full-screen shell's activity row and
    // nowhere else; this shell has no row, so it says where it is.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/compact");
    let text = session.wait_for("shows its progress");
    assert!(
        text.contains("xfx: /compact runs in the full-screen shell"),
        "{text}"
    );
    assert!(sandbox.session_ids().is_empty(), "a session was opened");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_sound_saves_the_switch_and_says_this_shell_rings_none() {
    // The same switch, saved to the same file the full-screen shell reads; and
    // the line that says so, because this shell's question is the shared stdin
    // prompter's and rings nothing.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command();
    command.env("XFX_SOUND", "off");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("/sound");
    let text = session.wait_for("rings none");
    assert!(text.contains("[shell] sound: on;"), "{text}");
    assert_eq!(
        sandbox.settings(),
        Some(json!({"notifications": {"turn_end": true, "attention_required": true}}))
    );
    session.type_line("/sound max");
    session.wait_for("usage: /sound [on|off]");
    session.type_line("/sound off");
    session.wait_for("[shell] sound: off;");
    assert_eq!(
        sandbox.settings(),
        Some(json!({"notifications": {"turn_end": false, "attention_required": false}}))
    );
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_rename_titles_a_session_before_any_prompt_and_slash_resume_says_where_the_picker_is() {
    // The line shell has no picker; it says where one is rather than resuming
    // something nobody picked.
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/resume");
    session.wait_for("has no picker");
    session.type_line("/rename");
    session.wait_for("usage: /rename <title>");
    session.type_line("/rename   My Title  ");
    session.wait_for("[shell] session renamed to \"My Title\"");
    assert_eq!(session.quit().code(), Some(0));

    // Opened for the rename, and kept with it though nothing was asked.
    let sessions: Vec<_> = std::fs::read_dir(sandbox.sessions_dir())
        .expect("the store was created")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    let manifest = std::fs::read_to_string(sessions[0].join("session.json")).expect("a manifest");
    assert!(manifest.contains("\"title\":\"My Title\""), "{manifest}");
    let log = std::fs::read_to_string(sessions[0].join("events.jsonl")).expect("a log");
    assert!(
        log.contains(r#""event":{"kind":"session_renamed","title":"My Title"}"#),
        "{log}"
    );
}

#[test]
fn slash_model_reports_the_active_model_and_changes_it_for_later_turns() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/model");
    let text = session.wait_for("model=");
    assert!(text.contains("zai/glm-5.2"), "{text}");

    session.type_line("/model acme/model-9");
    session.wait_for("model=acme/model-9");
    type_and_wait(&session, "hello", "ok");

    // The model travels as the Gateway's own header, exactly as `ask` sends it.
    let requests = gateway.requests();
    assert_eq!(
        requests[0].header("ai-language-model-id"),
        Some("acme/model-9")
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_refuses_an_unusable_name_rather_than_sending_it() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/model one two");
    let text = session.wait_for_count(PROMPT, 2);
    assert!(text.contains("one model"), "{text}");
    // The refusal did not change anything.
    session.type_line("/model");
    let text = session.wait_for("model=");
    assert!(text.contains("zai/glm-5.2"), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_reports_the_provider_and_says_the_gateway_has_no_catalog_to_browse() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/model");
    let text = session.wait_for("catalog=");
    assert!(text.contains("provider=gateway"), "{text}");
    // A fact about the provider, not a missing feature -- and not a network
    // call: nothing was contacted to find this out.
    assert!(text.contains("catalog=unavailable"), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_browses_the_catalog_the_daemon_publishes() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(json!({"models": [
        {"id": "m-1", "aliases": ["fable"], "name": "Claude Fable 5",
         "efforts": ["low", "high"], "max_context": 1_000_000}
    ]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    session.type_line("/model");
    let text = session.wait_for("catalog=");
    assert!(text.contains("provider=llmux"), "{text}");
    assert!(text.contains("catalog=1 models"), "{text}");
    assert!(text.contains("fable"), "{text}");
    assert!(text.contains("context=1000000"), "{text}");
    assert!(text.contains("efforts=low,high"), "{text}");
    assert_eq!(daemon.paths(), ["/models"], "browsing is not a ping");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_applies_an_id_the_daemon_does_not_list_and_resolves_one_it_does() {
    // Upstream's rule (`fx-model-fuzzy-40x120-unknown`): an id the catalog does
    // not list is applied as typed, not refused -- a catalog is what a provider
    // lists, not everything it will run. A query that names a listed id is
    // resolved to it (`resolveModelQuery`).
    let daemon = FakeLlmux::start(Vec::new())
        .with_catalog(json!({"models": [{"id": "m-1", "aliases": ["fable"]}]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    // Load the catalog first, the way a user does: resolution is against a
    // catalog xfx actually read.
    session.type_line("/model");
    session.wait_for("catalog=1 models");
    session.type_line("/model unlisted-xyz");
    let text = session.wait_for("[shell] model=unlisted-xyz");
    assert!(!text.contains("does not publish"), "{text}");
    // Checked against a catalog that was read, so no caveat.
    assert!(!text.contains("not checked"), "{text}");

    session.type_line("/model m");
    session.wait_for("[shell] model=m-1");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_caps_the_catalog_at_100_entries_and_names_the_remainder() {
    let mut models = vec![];
    for i in 0..105 {
        models.push(json!({"id": format!("m-{}", i), "aliases": [format!("m{}", i)]}));
    }
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(json!({"models": models}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    session.type_line("/model");
    let text = session.wait_for("... and 5 more");
    assert!(text.contains("catalog=105 models"), "{text}");
    // Verify the remainder line is shown for the 5 models not rendered.
    assert!(
        text.contains("... and 5 more"),
        "should show remainder line for 5 models: {text}"
    );
    // Verify the last rendered model entries are in the output.
    assert!(text.contains("[shell]   m95"), "should render m95: {text}");
    // Verify m100+ is NOT in the output.
    assert!(
        !text.contains("[shell]   m100"),
        "should not render m100: {text}"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_accepts_a_direct_selection_before_browsing_with_a_warning() {
    // Direct `/model <id>` before `/model` loads the catalog warns the user
    // that the selection was not checked, because CatalogState::NotLoaded has
    // not yet been resolved.
    let daemon = FakeLlmux::start(Vec::new())
        .with_catalog(serde_json::json!({"models": [{"id": "m-1", "aliases": ["fable"]}]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    // Direct selection before any /model load (catalog is NotLoaded).
    session.type_line("/model unverified-model");
    let text = session.wait_for("not checked against the provider's catalog");
    assert!(text.contains("unverified-model"), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_clear_and_slash_reset_are_slash_new_and_erase_nothing() {
    // Upstream's three names for one operation (`app_commands.zig:667-680`):
    // each drops the conversation, and the next prompt opens a new identity
    // that remembers nothing. None of them erases what the shell printed --
    // upstream hands the visible transcript to the terminal's scrollback
    // rather than taking it away, and on a line shell that is simply leaving
    // it where it is.
    for command in ["/clear", "/reset"] {
        let gateway = FakeGateway::start(vec![
            Reply::Sse(content_only(&["before"])),
            Reply::Sse(content_only(&["after"])),
        ]);
        let sandbox = Sandbox::new();
        let pty = Pty::open();
        let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

        type_and_wait(&session, "remember this", "before");
        let ids_before = sandbox.session_ids();
        assert_eq!(ids_before.len(), 1);

        session.type_line(command);
        let text = session.wait_for_count(PROMPT, 3);
        assert!(
            !text.contains("\u{1b}[2J") && !text.contains("\u{1b}[3J"),
            "{command} erased the screen or the scrollback: {text:?}"
        );
        assert!(
            text.contains("the next prompt starts a fresh conversation"),
            "{command} did not say what it did: {text:?}"
        );

        type_and_wait(&session, "and now", "after");

        let ids_after = sandbox.session_ids();
        assert_eq!(ids_after.len(), 2, "{command} kept the old identity");
        assert_eq!(
            user_messages(&request_body(&gateway, 1)),
            vec!["and now".to_string()],
            "{command} kept the conversation"
        );

        assert_eq!(session.quit().code(), Some(0));
    }
}

#[test]
fn slash_setup_switches_provider_and_the_next_prompt_carries_the_conversation() {
    // The line shell's `/setup llmux` keeps the conversation as the TUI's
    // switch does, as upstream's `/provider` keeps its session
    // (`app_auth_runtime.zig:1377-1381`): one session, and the daemon's first
    // request carries the Gateway's turn in Anthropic's shape.
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["GATEWAY-SAID-IT"]))]);
    let daemon = FakeLlmux::start(vec![Reply::Sse(support::fake_llmux::anthropic_answer(&[
        "DAEMON-SAID-IT",
    ]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_both(&gateway, &daemon));

    session.type_line("remember the gateway");
    session.wait_for("GATEWAY-SAID-IT");
    session.type_line("/setup llmux");
    let text = session.wait_for("[shell] provider=llmux");
    session.wait_for_count(PROMPT, 3);
    assert!(
        !text.contains("fresh conversation"),
        "the switch said it dropped the conversation: {text}"
    );
    session.type_line("and the daemon?");
    session.wait_for("DAEMON-SAID-IT");
    assert_eq!(session.quit().code(), Some(0));

    assert_eq!(
        sandbox.session_ids().len(),
        1,
        "the switch opened a session"
    );
    let body: Value =
        serde_json::from_str(&daemon.only_message_request().body).expect("a json body");
    let messages = body["messages"].as_array().expect("messages");
    let said: Vec<(&str, &str)> = messages
        .iter()
        .map(|message| {
            (
                message["role"].as_str().expect("a role"),
                message["content"][0]["text"]
                    .as_str()
                    .expect("a text block"),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("user", "remember the gateway"),
            ("assistant", "GATEWAY-SAID-IT"),
            ("user", "and the daemon?"),
        ]
    );
    let log = fs::read_to_string(
        sandbox
            .sessions_dir()
            .join(&sandbox.session_ids()[0])
            .join("events.jsonl"),
    )
    .expect("the log");
    assert!(
        log.lines()
            .any(|line| line.contains("\"kind\":\"preferences_changed\"")
                && line.contains("\"provider\":\"llmux\"")),
        "the switch was not recorded: {log}"
    );
}

#[test]
fn slash_new_starts_a_second_session_that_remembers_nothing() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["first"])),
        Reply::Sse(content_only(&["second"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "one", "first");
    let ids_before = sandbox.session_ids();
    assert_eq!(ids_before.len(), 1);

    session.type_line("/new");
    session.wait_for_count(PROMPT, 3);
    type_and_wait(&session, "two", "second");

    let ids_after = sandbox.session_ids();
    assert_eq!(ids_after.len(), 2, "a new identity was not created");
    assert_ne!(ids_after, ids_before);
    assert_eq!(
        user_messages(&request_body(&gateway, 1)),
        vec!["two".to_string()],
        "the new session inherited history"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_copy_puts_the_last_answer_on_the_clipboard_as_the_model_wrote_it() {
    // `vercel-labs/fx@c1db9191 app_commands.zig:1974-2001`: nothing to copy,
    // the reply's exact bytes, and a tool that refused -- each said once.
    let answer = "ALPHA answer line one.\n\nSecond paragraph with `code`.";
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&[
        "ALPHA answer line one.\n\n",
        "Second paragraph with `code`.",
    ]))]);
    let clipboard = FakeClipboard::new();
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    clipboard.first_on_path(&mut command);
    let mut session = start(&sandbox, &pty, command);

    session.type_line("/copy");
    session.wait_for("[shell] No assistant reply to copy.");
    assert_eq!(clipboard.copied(), None, "a copy ran with nothing to copy");

    session.type_line("one");
    session.wait_for("`code`.");
    session.type_line("/copy");
    session.wait_for("[shell] Copied to clipboard.");
    assert_eq!(
        clipboard.copied().expect("the clipboard was written"),
        answer.as_bytes(),
        "not the reply as the model wrote it"
    );

    clipboard.fail();
    session.type_line("/copy");
    session.wait_for("xfx: Failed to copy to clipboard.");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_undo_puts_the_changed_file_back_and_tells_the_model_nothing() {
    // `vercel-labs/fx@c1db9191 app_commands.zig:1242-1290`: nothing to undo
    // before anything changed, the edit taken back with upstream's sentence,
    // and nothing left after it. The model is not told: no request follows.
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/undo");
    session.wait_for("[shell] undo: Nothing to undo.");
    session.type_line("edit the notes");
    session.wait_for("the edit is done");
    assert_eq!(fs::read_to_string(&notes).expect("read back"), "beta\n");

    session.type_line("/undo");
    let canonical = fs::canonicalize(&notes).expect("canonicalize");
    session.wait_for(&format!("[shell] undo: Restored {}", canonical.display()));
    assert_eq!(fs::read_to_string(&notes).expect("read back"), "alpha\n");
    session.type_line("/undo");
    session.wait_for_count("[shell] undo: Nothing to undo.", 2);

    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(
        gateway.request_count(),
        3,
        "an undo was sent to the model, or the turn did not run"
    );
}

#[test]
fn slash_trace_writes_a_private_report_about_this_shells_conversation() {
    // `/trace` on the line shell: the same report, built from this shell's own
    // conversation, written under `$TMPDIR` and copied as a file, and one line
    // saying so (`app_commands.zig:75-90`).
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["TRACE_LINE_DONE"]))]);
    let clipboard = FakeClipboard::new();
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    clipboard.first_on_path(&mut command);
    clipboard.reports_into(&mut command);
    let mut session = start(&sandbox, &pty, command);

    session.type_line("one");
    session.wait_for("TRACE_LINE_DONE");
    session.type_line("/trace");
    session.wait_for(if cfg!(target_os = "macos") {
        "[shell] Trace copied to clipboard. Review and redact it before sharing."
    } else {
        "[shell] Trace saved at"
    });
    let reports = clipboard.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&reports[0]).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o077,
            0,
            "the report is readable by others: {mode:o}"
        );
    }
    if cfg!(target_os = "macos") {
        assert_eq!(clipboard.copied_file().as_deref(), reports[0].to_str());
    }
    let report = fs::read_to_string(&reports[0]).expect("the report");
    for needle in [
        "\n    one\n",
        "\n    TRACE_LINE_DONE\n",
        "\nhistory_turns: 1\n",
    ] {
        assert!(report.contains(needle), "{needle:?} is not in:\n{report}");
    }
    assert!(
        !report.contains(support::sandbox::TEST_KEY),
        "the credential reached the report"
    );

    assert_eq!(session.quit().code(), Some(0));
    assert_eq!(gateway.request_count(), 1, "/trace sent a request");
}

/// Every line xfx wrote that begins with `xfx: `, without the terminal's echo
/// of what was typed.
///
/// A refusal is what xfx *wrote*; the same bytes coming back as echo are the
/// terminal's doing and prove nothing. They are told apart by the prefix, which
/// only xfx writes.
fn diagnostics(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| line.starts_with("xfx: "))
        .map(str::to_string)
        .collect()
}

#[test]
fn an_unknown_slash_command_is_refused_with_the_same_words_every_time() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/nonesuch");
    session.wait_for("is not an xfx command");
    session.type_line("/nonesuch");
    let text = session.wait_for_count("is not an xfx command", 2);

    let refusals = diagnostics(&text);
    assert_eq!(refusals.len(), 2, "{text}");
    assert_eq!(refusals[0], refusals[1], "the refusal is not deterministic");
    // Derived, not spelled: the sentence quotes the registry's own count, so a
    // command added to the palette cannot leave this line saying a number the
    // shell no longer has.
    assert_eq!(
        refusals[0],
        format!(
            "xfx: `/nonesuch` is not an xfx command; /help lists its {} commands",
            xfx::interactive::SLASH_COMMANDS.len()
        )
    );

    // A slash command that is not one is never sent to a model: there is no
    // Gateway configured here, and no connection failure was reported.
    assert!(!text.contains("connect"), "{text}");
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn an_unknown_command_cannot_paint_on_the_terminal_through_the_refusal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // A line that starts with `/` is quoted back by xfx. If it were quoted
    // verbatim, this one would clear the screen, wipe the scrollback, retitle
    // the window, and then bury the guidance under 500 bytes of padding.
    session.type_bytes(b"/\x1b[2J\x1b[3J\x1b]0;pwned\x07\x1b[H");
    session.type_bytes("x".repeat(500).as_bytes());
    session.type_bytes(b"\r");
    let text = session.wait_for("is not an xfx command");

    let refusals = diagnostics(&text);
    assert_eq!(refusals.len(), 1, "{text}");
    let refusal = &refusals[0];
    assert!(
        !refusal.contains('\u{1b}'),
        "the refusal carries an escape: {refusal:?}"
    );
    assert!(
        !refusal.chars().any(char::is_control),
        "the refusal carries a control character: {refusal:?}"
    );
    assert!(
        refusal.len() < 200,
        "the refusal is unbounded ({} bytes)",
        refusal.len()
    );
    assert!(
        refusal.ends_with(&format!(
            "/help lists its {} commands",
            xfx::interactive::SLASH_COMMANDS.len()
        )),
        "the guidance was pushed off the line: {refusal:?}"
    );
    // The shell is unharmed and still answering.
    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn a_prompt_that_merely_mentions_a_slash_is_still_a_prompt() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["answered"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "what does a/b /help mean", "answered");
    assert_eq!(
        user_messages(&request_body(&gateway, 0)),
        vec!["what does a/b /help mean".to_string()]
    );

    assert_eq!(session.quit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// interruption and exit
// ---------------------------------------------------------------------------

#[test]
fn ctrl_c_stops_a_running_turn_and_the_shell_survives_it() {
    // A started answer that never finishes: the only state a user can really
    // interrupt.
    let gateway = FakeGateway::start(vec![
        Reply::SseThenHang(vec![sse_body(&[text_delta("a", "thinking")])]),
        Reply::Sse(content_only(&["done"])),
    ]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("start something long");
    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    let text = session.wait_for("interrupted");
    assert!(text.contains("stopping the turn"), "{text}");

    // Still alive, and still able to run a turn.
    type_and_wait(&session, "again", "done");
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn ctrl_c_at_an_idle_prompt_clears_the_line_and_twice_leaves() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // Typed but not submitted: the line discipline discards it, and xfx offers
    // a fresh prompt rather than leaving the user staring at a dead line.
    session.type_bytes(b"half a thought");
    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    session.type_bytes(&[0x03]);

    let status = session.wait_exit();
    assert_eq!(
        status.code(),
        Some(130),
        "a second interrupt exits 128+SIGINT; got {status:?}"
    );
    assert!(sandbox.session_ids().is_empty(), "nothing was recorded");
}

#[test]
fn a_submitted_line_resets_the_interrupt_count() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    session.type_line("/version");
    session.wait_for_count(PROMPT, 3);
    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 4);

    // Still alive: the earlier interrupt did not count toward this one.
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn end_of_input_leaves_the_shell_cleanly() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    // Ctrl-D on an empty line is end of input.
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
}

// ---------------------------------------------------------------------------
// what the terminal looks like afterwards
// ---------------------------------------------------------------------------

/// Requires that xfx left the terminal alone, given a reading taken while it
/// was still running and, where the platform still permits one, a second
/// reading taken after it exited.
///
/// **The reading that matters is the one taken while xfx is alive.** On
/// BSD-derived kernels -- macOS among them -- the terminal of a session leader
/// is revoked when that leader exits: every descriptor to it, including the one
/// this harness holds open, stops being a terminal, and a freshly opened one
/// gets a pristine device with the system defaults. So "read the terminal after
/// the child is gone" cannot distinguish a shell that restored the state from
/// one that never touched it *or from one that left it in raw mode* -- which is
/// exactly how the earlier version of these tests passed while proving nothing.
///
/// Sampling during the run is also the stronger question. xfx's claim is not
/// "it puts the terminal back", it is "it never changes the terminal", and a
/// during-run reading is the only one that can tell those two apart.
fn assert_terminal_untouched(pty: &Pty, before: TerminalState, during: TerminalState) {
    assert_eq!(
        before, during,
        "the terminal was changed while xfx was running"
    );
    if let Some(after) = try_modes(pty) {
        assert_eq!(before, after, "the terminal was left changed");
    }
    assert!(during.local.contains(LocalModes::ECHO), "echo is off");
    assert!(
        during.local.contains(LocalModes::ICANON),
        "canonical mode is off"
    );
}

#[test]
fn a_normal_exit_leaves_the_line_discipline_exactly_as_it_was() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/version");
    session.wait_for_count(PROMPT, 2);
    // Read while xfx is running and idle at its prompt. A shell that had taken
    // the terminal for its line editor is in raw mode right now.
    let during = modes(&pty);
    assert!(during.local.contains(LocalModes::ISIG), "signals are off");
    assert_eq!(session.quit().code(), Some(0));

    assert_terminal_untouched(&pty, before, during);
    assert!(
        !session.text().contains("\u{1b}[?1049"),
        "the alternate screen was used"
    );
}

#[test]
fn an_interrupted_turn_leaves_the_line_discipline_exactly_as_it_was() {
    let gateway = FakeGateway::start(vec![Reply::SseThenHang(vec![sse_body(&[text_delta(
        "a", "thinking",
    )])])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("start something long");
    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");
    // While the shell is still alive, having just cancelled a turn: the moment
    // a restore-on-exit path would still be hiding a raw terminal.
    let during = modes(&pty);
    assert_eq!(session.quit().code(), Some(0));

    assert_terminal_untouched(&pty, before, during);
}

#[test]
fn a_hard_exit_on_a_second_interrupt_still_leaves_a_usable_terminal() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let before = modes(&pty);
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_bytes(&[0x03]);
    session.wait_for_count(PROMPT, 2);
    // Sampled before the interrupt that ends the process: `exit(130)` runs no
    // destructor, so if xfx owed the terminal anything this is the last moment
    // it could have owed it.
    let during = modes(&pty);
    session.type_bytes(&[0x03]);
    let status = session.wait_exit();
    assert_eq!(status.code(), Some(130));
    assert_eq!(status.signal(), None, "xfx exits, it is not killed");

    assert_terminal_untouched(&pty, before, during);
}

// ---------------------------------------------------------------------------
// the shell is the same product as the command line
// ---------------------------------------------------------------------------

#[test]
fn the_shell_runs_tools_under_the_configured_permission_mode() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            support::fake_gateway::tool_call(
                "call-1",
                "read_file",
                serde_json::json!({ "path": "notes.txt" }),
            ),
            support::fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["read it"])),
    ]);
    let sandbox = Sandbox::new();
    fs::write(sandbox.workspace.join("notes.txt"), "alpha\n").expect("write the fixture");
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "read notes.txt", "read it");

    let body = request_body(&gateway, 1);
    let text = serde_json::to_string(&body).expect("serialize");
    assert!(text.contains("alpha"), "the tool result was not sent back");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn the_shell_records_a_session_the_command_line_can_read() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["recorded"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "remember me", "recorded");
    assert_eq!(session.quit().code(), Some(0));

    let listed = sandbox
        .command()
        .args(["sessions", "--json"])
        .output()
        .expect("spawn xfx sessions");
    assert_eq!(listed.status.code(), Some(0));
    let document: Value =
        serde_json::from_slice(&listed.stdout).expect("the listing is one JSON document");
    let sessions = document["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["history_turns"], 1);
}

#[test]
fn the_shell_reads_its_own_workspace_instructions() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["noted"]))]);
    let sandbox = Sandbox::new();
    fs::write(
        sandbox.workspace.join("AGENTS.md"),
        "Always answer in haiku.\n",
    )
    .expect("write the project instructions");
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    type_and_wait(&session, "hello", "noted");

    let body = serde_json::to_string(&request_body(&gateway, 0)).expect("serialize");
    assert!(body.contains("Always answer in haiku."), "{body}");

    assert_eq!(session.quit().code(), Some(0));
}

/// Keeps the `Path` import honest: the fixtures above name paths, and this is
/// the one assertion about the shape of the store's own directory.
#[test]
fn the_store_lives_where_the_documentation_says_it_does() {
    let sandbox = Sandbox::new();
    assert_eq!(
        sandbox.sessions_dir(),
        Path::new(&sandbox.home).join(".xfx").join("sessions")
    );
}

/// `xfx ask` has the same interrupt, and until the shell existed nothing proved
/// it: the notice is written by a second thread while the command holds the
/// output streams, and the turn was waiting on a stream that had gone quiet.
/// Both are facts about a terminal, so both are proven on one.
#[test]
fn ctrl_c_during_a_noninteractive_ask_says_so_and_ends_the_turn() {
    let gateway = FakeGateway::start(vec![Reply::SseThenHang(vec![sse_body(&[text_delta(
        "a", "thinking",
    )])])]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.args(["ask", "--no-save", "something long"]);
    let mut session = Session::spawn(&pty, command);

    session.wait_for("thinking");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");
    assert_eq!(
        session.wait_exit().code(),
        Some(1),
        "a cancelled turn is a turn that did not complete"
    );
}

#[test]
fn a_prompt_with_no_credential_says_so_and_records_nothing() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    // No `AI_GATEWAY_API_KEY`, and no Gateway either: a shell must still open
    // on a machine that cannot yet talk to a model -- that is the machine whose
    // user needs `/help` -- and a prompt it cannot send must not leave an empty
    // session behind, once per attempt.
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("anything at all");
    let text = session.wait_for("AI_GATEWAY_API_KEY");
    assert!(text.contains("VERCEL_OIDC_TOKEN"), "{text}");

    session.type_line("and again");
    session.wait_for_count("AI_GATEWAY_API_KEY", 2);
    assert!(
        sandbox.session_ids().is_empty(),
        "an empty session was left"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn a_model_chosen_in_the_shell_is_what_the_session_records() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with(&gateway));

    session.type_line("/model acme/model-9");
    session.wait_for("model=acme/model-9");
    type_and_wait(&session, "hello", "ok");
    assert_eq!(session.quit().code(), Some(0));

    // The durable record agrees with what the terminal was told, so a later
    // `xfx ask --resume` continues in the model the conversation was held in.
    let shown = sandbox
        .command()
        .args(["session", "last", "--json"])
        .output()
        .expect("spawn xfx session");
    let document: Value = serde_json::from_slice(&shown.stdout).expect("one JSON document");
    assert_eq!(document["model"], "acme/model-9");
}

// ---------------------------------------------------------------------------
// the permission modes, on a terminal
// ---------------------------------------------------------------------------

#[test]
fn ask_mode_asks_on_the_terminal_and_a_yes_lets_the_edit_through() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    // The prompt is the real one: it says what xfx wants, what "always" would
    // grant, and it is asked on this terminal rather than assumed.
    let asked = session.wait_for("xfx wants to");
    assert!(asked.contains("[y] yes, once"), "{asked}");
    assert!(asked.contains("[a] always"), "{asked}");
    assert!(asked.contains("notes.txt"), "{asked}");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "the edit ran before it was approved"
    );

    session.type_line("y");
    session.wait_for("the edit is done");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "an approved edit did not land"
    );

    // The answer was consumed by the approval and nothing else: the next line
    // is read by the shell as a fresh line, not as leftover bytes. Two readers
    // share one buffered stdin, and this is the assertion that they do not
    // swallow each other's input.
    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn a_rule_that_asks_before_a_read_asks_on_the_terminal_and_a_no_reads_nothing() {
    // The line shell's prompt for a read a configured rule asks about: the
    // same question the band puts (`tool_admission.zig:1426-1442`), in the
    // shell's `y/a/n` words, and a `n` is a refusal the model is told of.
    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            support::fake_gateway::tool_call(
                "read-0",
                "read_file",
                serde_json::json!({ "path": "secret.txt" }),
            ),
            support::fake_gateway::finish("tool-calls"),
        ])),
        Reply::Sse(content_only(&["MARKER-REFUSED"])),
    ]);
    let sandbox = Sandbox::new();
    fs::write(sandbox.workspace.join("secret.txt"), "hunter2\n").expect("the secret");
    let profile = sandbox.settings_path();
    let dir = profile.parent().expect("a profile dir");
    fs::create_dir_all(dir).expect("the profile dir");
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).expect("tighten");
    }
    fs::write(&profile, r#"{"permission":{"read":{"secret*":"ask"}}}"#).expect("the profile");
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "auto");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("read the secret");
    let asked = session.wait_for("[n] no");
    assert!(asked.contains("xfx wants to read `secret.txt`"), "{asked}");
    assert!(
        asked.contains("[a] always -- allow every future read_file of `secret.txt`"),
        "{asked}"
    );
    session.type_line("n");
    session.wait_for("MARKER-REFUSED");
    let body = request_body(&gateway, 1);
    assert!(body.to_string().contains("execution-denied"), "{body}");
    assert!(!body.to_string().contains("hunter2"), "the file was read");
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn an_always_answered_on_the_terminal_is_written_where_a_resume_will_read_it() {
    // The durable half of `[a] always`. The prompt promises the approval lasts
    // beyond this process -- it names the session id a later `--resume-id` would
    // use -- and the only thing that keeps that promise is a
    // `permission_grant_recorded` frame in the log. The step is the recorder's
    // and is shared with `app::ask` and the TUI's worker, so this is also what
    // says the shell still takes it.
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    session.wait_for("xfx wants to");
    session.type_line("a");
    session.wait_for("the edit is done");
    assert_eq!(fs::read_to_string(&notes).expect("read the file"), "beta\n");
    assert_eq!(session.quit().code(), Some(0));

    let granted: Vec<Value> = recorded_frames(&sandbox)
        .into_iter()
        .filter(|frame| frame["event"]["kind"] == "permission_grant_recorded")
        .collect();
    assert_eq!(
        granted.len(),
        1,
        "the terminal's `always` is not in the session log, so it ends with \
         this process and the next resume asks again"
    );
    assert_eq!(granted[0]["event"]["tool"], "edit_file");
    assert_eq!(
        granted[0]["event"]["target"].as_str().expect("a target"),
        fs::canonicalize(&notes)
            .expect("canonicalize")
            .to_string_lossy()
    );
}

/// Every `permission_grant_recorded` frame in the sandbox's session logs.
fn recorded_grants(sandbox: &Sandbox) -> Vec<Value> {
    recorded_frames(sandbox)
        .into_iter()
        .filter(|frame| frame["event"]["kind"] == "permission_grant_recorded")
        .collect()
}

#[test]
fn an_always_answered_at_an_ask_prompt_admits_the_same_change_on_the_next_resume() {
    // **`xfx ask`'s own path**, not the shell's. `app::ask` builds the durable
    // permission session, restores what a past turn recorded, and records what
    // this one collected; each of those three steps is what makes the sentence
    // the prompt sells -- "and in every later `xfx ask --resume-id <id>` of this
    // saved session" -- true. Until now the command that makes that promise was
    // only pinned by unit tests either side of it and by suites that *seeded* a
    // grant into the log by hand, so nothing proved that an "always" a person
    // really typed at a real terminal ends up where the next `--resume-id`
    // reads it.
    //
    // It runs here rather than in `tests/sessions.rs` because it needs the one
    // thing that file has no machinery for: `TtyPrompter` exists only when
    // stdin and stderr are both terminals, so the question can be asked at all
    // only on a pty.
    //
    // Two processes, and the second one is the assertion. Both are driven on a
    // real terminal, so the second one *could* be asked -- a resume that had
    // lost the grant would print the question rather than proceeding, and the
    // absence asserted at the end is therefore an absence from a run that had
    // every means of producing it.
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let target = fs::canonicalize(&notes)
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();

    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    command.args(["ask", "fix the notes"]);
    let mut first = Session::spawn(&pty, command);

    let asked = first.wait_for("xfx wants to");
    assert!(asked.contains("[a] always"), "{asked}");
    first.type_line("a");
    first.wait_for("the edit is done");
    assert_eq!(first.wait_exit().code(), Some(0));
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "the approved edit did not land"
    );
    drop(gateway);

    // One grant, naming the file by absolute path -- the key the next run will
    // match on, rather than a count.
    let granted = recorded_grants(&sandbox);
    assert_eq!(
        granted.len(),
        1,
        "`ask`'s own `always` is not in the session log, so it ended with the \
         process that collected it: {granted:?}"
    );
    assert_eq!(granted[0]["event"]["tool"], "edit_file");
    assert_eq!(granted[0]["event"]["target"], target);

    // And the id the question promised the approval would survive into is the
    // id this test resumes with. Read off the terminal rather than from the
    // store, so the promise being kept is the promise that was *made*.
    let session_id = sandbox
        .session_ids()
        .into_iter()
        .next()
        .expect("one recorded session");
    assert!(
        first
            .settled_text()
            .contains(&format!("xfx ask --resume-id {session_id}")),
        "the prompt did not name the session the grant would survive into"
    );

    // The same change again, in a new process, on a terminal that can ask.
    fs::write(&notes, "alpha\n").expect("put the fixture back");
    // Distinct call ids, because the resumed turn replays the first one's tool
    // calls as history and one id may name exactly one call in a conversation.
    // The *mutation* is identical, which is the claim; only the correlation
    // handles differ.
    let resumed_gateway = FakeGateway::start(edit_then_finish_called("resumed"));
    let pty = Pty::open();
    let mut command = sandbox.command_with(&resumed_gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    command.args(["ask", "--resume-id", &session_id, "fix the notes again"]);
    let mut second = Session::spawn(&pty, command);
    second.wait_for("the edit is done");
    assert_eq!(second.wait_exit().code(), Some(0));

    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "the restored grant did not admit the same change"
    );
    let text = second.settled_text();
    assert!(
        !text.contains("xfx wants to"),
        "the resume asked a question the user had already answered: {text:?}"
    );
    assert_eq!(
        recorded_grants(&sandbox).len(),
        1,
        "the restored grant was written down a second time, so a resumed \
         session's log grows one frame per resume"
    );
    drop(resumed_gateway);
}

/// Every frame of every session log in the sandbox, oldest first.
///
/// Read after the process has exited: the log is `fsync`ed and published before
/// the turn that wrote it returns, so nothing here polls.
fn recorded_frames(sandbox: &Sandbox) -> Vec<Value> {
    let mut frames = Vec::new();
    for id in sandbox.session_ids() {
        let log = sandbox.sessions_dir().join(id).join("events.jsonl");
        let Ok(raw) = fs::read_to_string(&log) else {
            continue;
        };
        frames.extend(
            raw.lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok()),
        );
    }
    frames
}

#[test]
fn ask_mode_takes_no_for_an_answer_and_the_file_is_untouched() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    session.wait_for("xfx wants to");
    session.type_line("n");
    // The refusal is a tool result the model can act on, so the turn continues
    // and finishes normally.
    session.wait_for("the edit is done");
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "a refused edit changed the file"
    );

    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn ctrl_c_at_an_approval_prompt_does_not_hang_the_shell() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "ask");
    let mut session = start(&sandbox, &pty, command);

    session.type_line("fix the notes");
    session.wait_for("xfx wants to");
    session.type_bytes(&[0x03]);
    session.wait_for("interrupted");

    // The question is still on the terminal and still needs an answer -- the
    // interrupt stopped the turn, it did not answer for the user. What must not
    // happen is a shell that never comes back.
    session.type_line("n");
    session.wait_for_count(PROMPT, 2);
    // The file is the assertion, not the transcript: the question itself quotes
    // an excerpt of the change it is asking about, so the new text is on the
    // terminal by design and means nothing about what happened on disk.
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "alpha\n",
        "a cancelled and refused edit changed the file"
    );

    session.type_line("/version");
    session.wait_for(env!("CARGO_PKG_VERSION"));
    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn yolo_mode_warns_once_and_then_asks_nobody_anything() {
    let gateway = FakeGateway::start(edit_then_finish());
    let sandbox = Sandbox::new();
    let notes = with_notes(&sandbox);
    let pty = Pty::open();
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_PERMISSION_MODE", "yolo");
    let mut session = Session::spawn(&pty, command);

    // The warning is on the way in, before a prompt exists to type at.
    let opening = session.wait_for(PROMPT);
    assert!(
        opening.to_lowercase().contains("yolo"),
        "yolo must announce itself: {opening}"
    );
    assert!(opening.contains("permission_mode=yolo"), "{opening}");

    session.type_line("fix the notes");
    session.wait_for("the edit is done");
    let text = session.text();
    assert!(
        !text.contains("xfx wants to"),
        "yolo asked a question: {text}"
    );
    assert_eq!(
        fs::read_to_string(&notes).expect("read the file"),
        "beta\n",
        "yolo did not run the edit"
    );

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn status_reports_the_mode_the_shell_is_running_under() {
    // The same fact from the other side of the product: what the shell warned
    // about is what `status` says, so the two cannot disagree.
    let sandbox = Sandbox::new();
    let listed = sandbox
        .command()
        .args(["status", "--json"])
        .env("XFX_PERMISSION_MODE", "yolo")
        .output()
        .expect("spawn xfx status");
    let document: Value = serde_json::from_slice(&listed.stdout).expect("one JSON document");
    assert_eq!(document["permission_mode"], "yolo");
    assert_eq!(document["sandbox"], "none");
}

#[test]
fn the_terminal_comparator_notices_a_change_in_any_single_field() {
    // The regression this pins: the comparator used to fold the input and
    // output flag words together with a bitwise OR before comparing. The two
    // words have overlapping bit values, so a flag gained in one and lost in
    // the other cancelled out and a changed terminal compared equal. Each field
    // now stands on its own, and this proves each one is actually looked at.
    let pty = Pty::open();
    let base = modes(&pty);

    let variants = [
        (
            "input",
            TerminalState {
                input: base.input ^ InputModes::ICRNL,
                ..base
            },
        ),
        (
            "output",
            TerminalState {
                output: base.output ^ OutputModes::OPOST,
                ..base
            },
        ),
        (
            "control",
            TerminalState {
                control: base.control ^ ControlModes::CREAD,
                ..base
            },
        ),
        (
            "local",
            TerminalState {
                local: base.local ^ LocalModes::ECHO,
                ..base
            },
        ),
        (
            "VMIN",
            TerminalState {
                min: base.min.wrapping_add(1),
                ..base
            },
        ),
        (
            "VTIME",
            TerminalState {
                time: base.time.wrapping_add(1),
                ..base
            },
        ),
        (
            "size",
            TerminalState {
                size: (base.size.0.wrapping_add(1), base.size.1),
                ..base
            },
        ),
    ];
    for (field, changed) in variants {
        assert_ne!(base, changed, "a change to {field} compares equal");
    }
    assert_eq!(base, modes(&pty), "reading twice must be stable");
}

// ---------------------------------------------------------------------------
// the harness, tested against itself
// ---------------------------------------------------------------------------

/// A child that puts the terminal in raw mode with echo off and leaves it that
/// way, which is the exact thing every restoration test above claims xfx never
/// does.
///
/// `take_terminal` chooses whether it does so as a session leader owning the
/// terminal, the way xfx runs, or as an ordinary process. The two are not
/// interchangeable: a session leader's terminal is revoked when it exits on
/// BSD-derived kernels, and that revocation is the whole reason the old harness
/// saw nothing.
fn leave_the_terminal_in_raw_mode(pty: &Pty, take_terminal: bool) -> Session {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty raw -echo"]);
    let mut session = if take_terminal {
        Session::spawn(pty, command)
    } else {
        Session::spawn_without_taking_the_terminal(pty, command)
    };
    assert_eq!(
        session.wait_exit().code(),
        Some(0),
        "stty failed; terminal so far:\n{}",
        session.text()
    );
    session
}

#[test]
fn the_harness_can_tell_when_a_child_leaves_the_terminal_changed() {
    // Without this the whole "the terminal was left exactly as it was found"
    // section is a green light with nothing behind it. Here a child really does
    // wreck the terminal, and the comparator has to say so.
    let pty = Pty::open();
    let before = modes(&pty);
    // Not a session leader, so the terminal survives its exit to be inspected.
    // The session-leader case is covered by the during-run reading below, which
    // is the one the restoration tests actually take.
    let _child = leave_the_terminal_in_raw_mode(&pty, false);

    let after = modes(&pty);
    assert_ne!(before, after, "a raw-mode child compared equal");
    assert!(!after.local.contains(LocalModes::ECHO), "echo survived raw");
    assert!(
        !after.local.contains(LocalModes::ICANON),
        "canonical mode survived raw"
    );
    assert!(before.local.contains(LocalModes::ECHO), "{before:?}");
}

/// The old blind spot, reproduced -- on macOS only, because only a BSD-derived
/// kernel can be made to show it.
///
/// The child is spawned exactly the way xfx is: its own session, owning the
/// terminal. When such a child exits, a BSD-derived kernel revokes its
/// terminal, and the next open of that device name is a pristine one carrying
/// the system defaults. A harness that opens a slave per reading therefore
/// reports "the terminal is exactly as it was found" about the very raw-mode
/// child `the_harness_can_tell_when_a_child_leaves_the_terminal_changed`
/// catches.
///
/// **This fixture is not portable, and its `cfg` is the fix rather than a
/// concession.** Linux does not revoke an exiting session leader's terminal, so
/// a freshly opened slave there still reports the raw mode the child left:
/// `before != after`, and asserting blindness fails on a kernel that is not
/// blind. That is exactly what both Linux jobs of run 32505490051 reported
/// (`assertion left == right failed` at this test, `ECHO | ICANON | ISIG`
/// present before and absent after). macOS is the only BSD-derived target xfx
/// ships, so `target_os = "macos"` is the whole of the supported blind
/// platform.
///
/// Nothing about the harness's guarantees is scoped by this. What proves the
/// harness can see a change is
/// `the_harness_can_tell_when_a_child_leaves_the_terminal_changed` and
/// `a_during_run_reading_sees_a_terminal_the_running_child_has_changed`, and
/// both of those, like every restoration test above them, run on every
/// platform.
#[cfg(target_os = "macos")]
#[test]
fn on_macos_a_harness_that_retains_nothing_cannot_see_the_change() {
    let pty = Pty::open_without_a_retained_slave();
    let before = modes(&pty);
    let _child = leave_the_terminal_in_raw_mode(&pty, true);

    let after = modes(&pty);
    assert_eq!(
        before, after,
        "macOS reported an exiting session leader's terminal as the child left \
         it, so it no longer revokes and this macOS-only fixture has nothing \
         left to document; delete it rather than repair it"
    );
    assert!(
        after.local.contains(LocalModes::ECHO),
        "a fresh descriptor did not report defaults, so the retained one is not \
         what made the difference"
    );
}

#[test]
fn a_during_run_reading_sees_a_terminal_the_running_child_has_changed() {
    // The restoration tests above take their reading while xfx is alive, and
    // this is the proof that such a reading can fail. The child is spawned
    // exactly as xfx is -- its own session, holding this terminal -- so the
    // path under test is the same one, down to the descriptor.
    let pty = Pty::open();
    let before = modes(&pty);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "stty raw -echo; echo ready; sleep 30"]);
    let session = Session::spawn(&pty, command);
    session.wait_for("ready");

    let during = modes(&pty);
    assert_ne!(before, during, "a live raw-mode child compared equal");
    assert!(!during.local.contains(LocalModes::ECHO), "{during:?}");
    assert!(!during.local.contains(LocalModes::ICANON), "{during:?}");
    // Dropping the session kills the child, which is the point: nothing here
    // waits for a `sleep 30`.
}

/// A stop, a resume, and an exit are three different answers, and none of them
/// may come back as a plain `Running`.
///
/// The regression this pins: the first version of `wait_state` asked with
/// `waitpid`, which *consumes* the event it reports. The second reading of a
/// child that was still stopped came back `Wait::Running`, and the resume after
/// it was invisible -- so a test waiting for "the second stop" was satisfied by
/// the first one still sitting there, and proved nothing about the resume in
/// between.
#[test]
fn a_stop_a_resume_and_an_exit_are_three_states_the_harness_keeps_apart() {
    let pty = Pty::open();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "echo ready; read line; exit 7"]);
    let mut session = Session::spawn(&pty, command);
    session.wait_for("ready");
    assert_eq!(
        session.state(),
        Wait::Running,
        "a live child before any signal"
    );

    assert_eq!(
        session.stop_with(Signal::STOP, "the stop"),
        Wait::Stopped(Signal::STOP.as_raw())
    );
    // Read it again. The child has not moved, so the answer must not either.
    assert_eq!(
        session.state(),
        Wait::Stopped(Signal::STOP.as_raw()),
        "a second reading of a still-stopped child changed its story"
    );

    session.signal(Signal::CONT);
    assert_eq!(
        session.wait_state("the resume", |state| !matches!(state, Wait::Stopped(_))),
        Wait::Continued,
        "a resumed child must stay distinguishable from one never stopped"
    );

    session.type_line("go");
    assert_eq!(
        session.wait_state("the exit", |state| matches!(state, Wait::Exited(_))),
        Wait::Exited(7)
    );
    // Reading the exit did not steal it: `wait_exit` still has a status to
    // collect, which every other test in this file depends on.
    assert_eq!(session.wait_exit().code(), Some(7));
}

/// A child that exited before anyone asked is reported as exited, its status is
/// still there for `wait_exit` to collect, and it stays readable afterwards.
///
/// The regression this pins: reading the state used to reap the child, so
/// `Child::try_wait` inside `wait_exit` was left with `ECHILD` on a process this
/// harness had buried itself.
#[test]
fn an_early_exit_is_reported_and_the_status_survives_the_reading() {
    let pty = Pty::open();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "echo ready; exit 3"]);
    let mut session = Session::spawn(&pty, command);
    session.wait_for("ready");

    assert_eq!(
        session.wait_state("the exit", |state| matches!(state, Wait::Exited(_))),
        Wait::Exited(3)
    );
    assert_eq!(
        session.wait_exit().code(),
        Some(3),
        "the reading consumed the status the session was still owed"
    );
    // And once `std` has collected it, the session answers from what it kept:
    // the kernel has nothing left to be asked about a child that is gone.
    assert_eq!(session.state(), Wait::Exited(3));
}

/// pid 1 is never a child of a test binary.
///
/// The regression this pins: the first version answered `Wait::Running` for
/// every error `waitpid` could return, so it reported a healthy child where
/// there was no child at all, and the timeout that followed blamed the process
/// under test. A harness may fail; it may not lie.
#[test]
#[should_panic(expected = "is not a child of this process")]
fn asking_about_a_process_that_is_not_a_child_fails_loudly() {
    let _ = wait_state(Pid::from_raw(1).expect("pid 1 is a pid"));
}
