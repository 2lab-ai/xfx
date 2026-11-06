//! A session the local usage ledger was kept in, opened by a build without it.
//!
//! fx v0.0.16 stopped keeping its own usage ledger: `/usage` reads the
//! provider's reports, and "saved sessions with old usage data still load and
//! resume; old usage fields and files are left in place and ignored"
//! (`vercel-labs/fx@a41fc2d4`, in v0.0.16 `481104fd`). xfx's ledger was the
//! `usage_recorded` frame's `requests`, `unaccounted`, `lines_added` and
//! `lines_removed`, the runtime's own `settled` and `rejected` frames, the
//! bills it asked for with `GET /v1/generation`, and `~/.xfx/usage.json`.
//!
//! The one session under `tests/fixtures/ledger-120f7b20/sessions` was written
//! by the build before the ledger was retired (`120f7b20`) in its TUI, against a
//! loopback fake Gateway with a fake key: a billed turn, a turn whose bill came
//! later (`settled`), one whose bill was refused (`rejected`), one cut off after
//! its text (`unaccounted`) and one whose bill was still owed (a pending
//! request, its `origin` written `@ORIGIN@`). The workspace is written
//! `@WS_A@`; a test puts its own in place of both and recomputes the
//! manifest's byte count and digest, as `tests/session_compat.rs` does.

#![cfg(unix)]

mod support;

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use support::fake_gateway::{content_only, sse_body, text_delta, FakeGateway, Reply};
use support::pty::{Pty, Session};
use support::sandbox::{Sandbox, TEST_KEY};

/// The log the ledger build wrote.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ledger-120f7b20/sessions"
);

/// Its one session.
const OLD: &str = "1793983613244-206c9ac7f3059b4a";

/// The keys a `usage_recorded` frame may carry once the ledger is retired:
/// the answering step's two totals, which the context meter and a replayed
/// summary read, and the turn's summary-row output.
const KEPT: [&str; 4] = [
    "kind",
    "input_tokens",
    "output_tokens",
    "turn_output_tokens",
];

/// The keys the ledger wrote, every one of which the fixture holds.
const LEDGER: [&str; 4] = ["requests", "unaccounted", "settled", "rejected"];

/// The ledger build's session, in a sandbox profile of its own, its pending
/// request owed by `origin`.
fn seed(sandbox: &Sandbox, origin: &str) {
    let profile = sandbox.home.join(".xfx");
    let sessions = sandbox.sessions_dir();
    fs::create_dir_all(&sessions).expect("create the sessions directory");
    private(&profile, 0o700);
    private(&sessions, 0o700);
    let workspace = json_text(&sandbox.workspace);
    let from = Path::new(FIXTURE).join(OLD);
    let log = fs::read_to_string(from.join("events.jsonl"))
        .expect("read the fixture log")
        .replace("@WS_A@", &workspace)
        .replace("@ORIGIN@", origin);
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(from.join("session.json"))
            .expect("read the fixture manifest")
            .replace("@WS_A@", &workspace),
    )
    .expect("the manifest is JSON");
    manifest["event_log_bytes"] = Value::from(log.len());
    manifest["event_log_sha256"] = Value::from(hex(&Sha256::digest(log.as_bytes())));
    let dir = sessions.join(OLD);
    fs::create_dir(&dir).expect("create the session directory");
    private(&dir, 0o700);
    fs::write(dir.join("events.jsonl"), &log).expect("write the log");
    private(&dir.join("events.jsonl"), 0o600);
    fs::write(
        dir.join("session.json"),
        serde_json::to_string(&manifest).expect("the manifest serializes"),
    )
    .expect("write the manifest");
    private(&dir.join("session.json"), 0o600);
}

fn log_path(sandbox: &Sandbox, id: &str) -> PathBuf {
    sandbox.sessions_dir().join(id).join("events.jsonl")
}

fn frames(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .expect("a log is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a frame is JSON"))
        .collect()
}

/// Every key of every `usage_recorded` frame in `bytes` that is not one the
/// retired ledger leaves.
fn ledger_keys(bytes: &[u8]) -> Vec<String> {
    frames(bytes)
        .iter()
        .filter(|frame| frame["event"]["kind"] == "usage_recorded")
        .flat_map(|frame| {
            frame["event"]
                .as_object()
                .expect("an event is an object")
                .keys()
                .filter(|key| !KEPT.contains(&key.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect()
}

fn json_text(path: &Path) -> String {
    let quoted = serde_json::to_string(&path.display().to_string()).expect("a path serializes");
    quoted[1..quoted.len() - 1].to_string()
}

fn private(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set a mode");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// How long one CLI run may take before it is killed and the test fails:
/// every one here is a single scripted turn on loopback, or a read.
const RUN_DEADLINE: Duration = Duration::from_secs(60);

/// Runs `command` to its end, but no longer than [`RUN_DEADLINE`]: past it,
/// the child -- this test's own -- is killed and reaped, and the test fails.
/// Its two pipes are drained on threads of their own, so a full pipe cannot
/// stall it either.
fn run(command: &mut Command, what: &str) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start xfx");
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll xfx") {
            break status;
        }
        if started.elapsed() > RUN_DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{what}: still running after {RUN_DEADLINE:?}; killed");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: stdout.join().expect("stdout"),
        stderr: stderr.join().expect("stderr"),
    }
}

fn ok(output: &Output, what: &str) -> String {
    assert!(
        output.status.success(),
        "{what} failed ({:?}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

/// The `GET /v1/generation` requests `gateway` was sent.
fn bill_asks(gateway: &FakeGateway) -> Vec<String> {
    gateway
        .requests()
        .into_iter()
        .filter(|request| request.method == "GET" && request.path.contains("/v1/generation"))
        .map(|request| request.path)
        .collect()
}

/// An origin as the store wrote it: the scheme and authority the request
/// went to.
fn origin_of(gateway: &FakeGateway) -> String {
    format!("http://127.0.0.1:{}", gateway.port())
}

/// A turn whose stream names its generation and says nothing of its cost:
/// the shape the ledger kept pending and asked the Gateway about.
fn deferred(generation: &str, text: &str) -> Reply {
    Reply::Sse(sse_body(&[
        json!({ "type": "response-metadata", "modelId": "fake/model-a",
                "timestamp": "2026-11-07T00:00:01.000Z" }),
        text_delta("answer_0", text),
        json!({
            "type": "finish",
            "finishReason": { "unified": "stop", "raw": "stop" },
            "usage": {
                "inputTokens": { "total": 1200 },
                "outputTokens": { "total": 40 }
            },
            "providerMetadata": { "gateway": { "generationId": generation } }
        }),
    ]))
}

// ---------------------------------------------------------------------------
// what the fixture is
// ---------------------------------------------------------------------------

#[test]
fn the_fixture_holds_every_frame_the_ledger_wrote() {
    let log = fs::read(Path::new(FIXTURE).join(OLD).join("events.jsonl")).expect("the log");
    let events: Vec<Value> = frames(&log)
        .into_iter()
        .map(|frame| frame["event"].clone())
        .filter(|event| event["kind"] == "usage_recorded")
        .collect();
    for key in LEDGER {
        assert!(
            events.iter().any(|event| event.get(key).is_some()),
            "no `usage_recorded` frame of the fixture carries `{key}`"
        );
    }
    // A request with a cost, one still owed (an `origin`), and a frame of
    // the runtime's own that settles one and rejects another.
    let requests: Vec<&Value> = events
        .iter()
        .filter_map(|event| event["requests"].as_array())
        .flatten()
        .collect();
    assert!(requests.iter().any(|request| request["cost"].is_number()));
    assert!(requests
        .iter()
        .any(|request| request["origin"] == "@ORIGIN@"));
    assert!(events
        .iter()
        .any(|event| event["settled"].is_array() && event["input_tokens"].is_null()));
    assert!(events
        .iter()
        .any(|event| event["rejected"].is_array() && event["input_tokens"].is_null()));
}

// ---------------------------------------------------------------------------
// S1: nothing asks for a bill
// ---------------------------------------------------------------------------

/// The binary has no route to a generation's bill and no name for the
/// ledger's profile file: nothing in it can ask for one, or open the other.
#[test]
fn the_binary_holds_no_bill_route_and_no_ledger_file_name() {
    let binary = fs::read(env!("CARGO_BIN_EXE_xfx")).expect("read the binary");
    for needle in [&b"/v1/generation"[..], &b"usage.json"[..]] {
        assert!(
            !binary.windows(needle.len()).any(|window| window == needle),
            "the binary holds `{}`",
            String::from_utf8_lossy(needle)
        );
    }
}

/// The TUI asks the Gateway for no bill: not for the old session's pending
/// request when it is resumed, not for a turn whose stream named its
/// generation and no cost, and not while `/usage` is open.
#[test]
fn the_tui_asks_for_no_bill_on_a_resume_a_turn_or_usage() {
    let gateway = FakeGateway::start(vec![
        deferred("gen_01JABCDEFGHJKMNPQRSTVWXYZ4", "LEDGER-SIX owed later"),
        Reply::Status(404, "not found".to_string()),
        Reply::Status(404, "not found".to_string()),
        Reply::Status(404, "not found".to_string()),
    ]);
    let sandbox = Sandbox::new();
    seed(&sandbox, &origin_of(&gateway));
    let before = fs::read(log_path(&sandbox, OLD)).expect("the log");
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    command.arg("--resume").arg(OLD);
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    session.wait_for("session resumed:");
    session.wait_for("LEDGER-FIVE left pending");
    // The ledger build asked for the pending bill as the session opened.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(
        bill_asks(&gateway),
        Vec::<String>::new(),
        "the resume asked for a bill"
    );

    session.type_bytes(b"sixth turn\r");
    session.wait_for("LEDGER-SIX owed later");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while frames(&fs::read(log_path(&sandbox, OLD)).expect("the log"))
        .iter()
        .filter(|frame| frame["event"]["kind"] == "turn_concluded")
        .count()
        < 6
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the sixth turn never concluded"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(
        bill_asks(&gateway),
        Vec::<String>::new(),
        "the turn asked for a bill"
    );

    session.type_bytes(b"/usage\r");
    session.wait_for("API key (AI_GATEWAY_API_KEY)");
    std::thread::sleep(std::time::Duration::from_millis(1000));
    session.type_bytes(&[0x1b]);
    std::thread::sleep(std::time::Duration::from_millis(300));
    session.type_bytes(&[0x04]);
    assert_eq!(session.wait_exit().code(), Some(0));
    assert_eq!(
        bill_asks(&gateway),
        Vec::<String>::new(),
        "something asked for a bill"
    );
    // And what the turn appended carries none of the ledger.
    let after = fs::read(log_path(&sandbox, OLD)).expect("the log");
    assert!(after.starts_with(&before), "the old bytes were rewritten");
    assert_eq!(ledger_keys(&after[before.len()..]), Vec::<String>::new());
    assert!(!session.settled_text().contains(TEST_KEY));
}

// ---------------------------------------------------------------------------
// S2: the old session loads, replays and goes on
// ---------------------------------------------------------------------------

/// `ask --resume-id` on the ledger build's session: it replays whole -- the
/// model is sent its five prompts and five answers, in order, before the new
/// one, and `session --json` reads its exact totals -- the turn
/// appends after the old bytes without touching them, and what it appends
/// carries none of the ledger. The session reads, and resumes again, after.
#[test]
fn the_ledger_builds_session_resumes_replays_and_appends_without_its_fields() {
    let gateway = FakeGateway::start(vec![
        Reply::Sse(content_only(&["continued once"])),
        Reply::Sse(content_only(&["continued twice"])),
    ]);
    let sandbox = Sandbox::new();
    seed(&sandbox, &origin_of(&gateway));
    let before = fs::read(log_path(&sandbox, OLD)).expect("the log");

    // Read first: the totals the old frames add up to, as they always did --
    // the five answering steps' (12000 + 1200 x 3 in, 800 + 40 x 3 out; the
    // cut turn and the runtime's two answer frames add nothing).
    let shown: Value = serde_json::from_str(&ok(
        &run(
            sandbox.command().args(["session", "--id", OLD, "--json"]),
            "session --id --json",
        ),
        "session --id --json",
    ))
    .expect("JSON");
    assert_eq!(fs::read(log_path(&sandbox, OLD)).expect("the log"), before);
    assert_eq!(
        (&shown["total_input_tokens"], &shown["total_output_tokens"]),
        (&json!(15600), &json!(920)),
        "{shown}"
    );

    ok(
        &run(
            sandbox
                .command_with(&gateway)
                .args(["ask", "--resume-id", OLD, "one more question"]),
            "ask --resume-id",
        ),
        "ask --resume-id",
    );
    // The model is sent the whole conversation, message by message: every
    // prompt and every answer -- the rejected turn's too -- the cut turn
    // closed as a turn that ended early, then the new prompt.
    let body = gateway.requests()[0].json();
    let sent: Vec<(String, String)> = body["prompt"]
        .as_array()
        .expect("a prompt array")
        .iter()
        .map(|message| {
            let text = match &message["content"] {
                Value::String(text) => text.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(""),
                other => panic!("content {other}"),
            };
            (message["role"].as_str().expect("a role").to_string(), text)
        })
        .collect();
    let roles: Vec<&str> = sent.iter().map(|(role, _)| role.as_str()).collect();
    assert_eq!(
        roles,
        [
            "user",
            "assistant",
            "user",
            "assistant",
            "user",
            "assistant",
            "user",
            "assistant",
            "user",
            "user",
            "assistant",
            "user"
        ],
        "{sent:?}"
    );
    let texts: Vec<&str> = sent.iter().map(|(_, text)| text.as_str()).collect();
    assert_eq!(
        [&texts[..8], &texts[9..]].concat(),
        [
            "first billed turn",
            "LEDGER-ONE billed answer",
            "second deferred turn",
            "LEDGER-TWO settled later",
            "third deferred turn",
            "LEDGER-THREE rejected later",
            "fourth cut turn",
            "LEDGER-FOUR cut short\n\nThe previous response ended before completion.",
            "fifth deferred turn",
            "LEDGER-FIVE left pending",
            "one more question",
        ],
        "{sent:?}"
    );
    assert!(texts[8].starts_with("<turn_aborted>\n"), "{sent:?}");
    let once = fs::read(log_path(&sandbox, OLD)).expect("the log");
    assert!(once.starts_with(&before), "the old bytes were rewritten");
    let gained = &once[before.len()..];
    assert!(
        frames(gained)
            .iter()
            .any(|frame| frame["event"]["kind"] == "usage_recorded"),
        "the turn recorded no usage"
    );
    assert_eq!(ledger_keys(gained), Vec::<String>::new());

    // And it goes on from there.
    ok(
        &run(
            sandbox
                .command_with(&gateway)
                .args(["ask", "--resume-id", OLD, "and again"]),
            "the second ask --resume-id",
        ),
        "the second ask --resume-id",
    );
    let twice = fs::read(log_path(&sandbox, OLD)).expect("the log");
    assert!(twice.starts_with(&once));
    assert_eq!(ledger_keys(&twice[before.len()..]), Vec::<String>::new());
    ok(
        &run(
            sandbox.command().args(["session", "--id", OLD]),
            "session --id after the continuation",
        ),
        "session --id after the continuation",
    );
    assert_eq!(bill_asks(&gateway), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// S4: the ledger's profile file is left as it was
// ---------------------------------------------------------------------------

/// A `usage.json` an older build wrote stays where it was, byte for byte and
/// owner-only, through a billed turn and `/usage`; fx leaves its old ledger
/// files in place and ignored (`a41fc2d4`). (That nothing opens it is the
/// binary's: it no longer holds the name.)
#[test]
fn an_old_usage_json_is_left_as_it_was() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["a turn"]))]);
    let sandbox = Sandbox::new();
    sandbox.write_profile("{}");
    let marker = sandbox.home.join(".xfx").join("usage.json");
    fs::write(&marker, "{\"tracking_started_at_ms\":5}\n").expect("an old marker");
    private(&marker, 0o600);
    let before = (
        fs::read(&marker).expect("read"),
        fs::metadata(&marker)
            .expect("stat")
            .modified()
            .expect("mtime"),
    );
    ok(
        &run(
            sandbox.command_with(&gateway).args(["ask", "count this"]),
            "ask",
        ),
        "ask",
    );
    let after = (
        fs::read(&marker).expect("read"),
        fs::metadata(&marker)
            .expect("stat")
            .modified()
            .expect("mtime"),
    );
    assert_eq!(after, before, "usage.json was rewritten");
    assert_eq!(
        fs::metadata(&marker).expect("stat").permissions().mode() & 0o777,
        0o600
    );
}
