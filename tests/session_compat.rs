//! Sessions the installed preview wrote, opened by this build.
//!
//! Every log under `tests/fixtures/preview-14ad04202fe5/sessions` was written by
//! the preview a user has today -- `xfx-preview 2026.10.06.115638`, whose
//! `status` says `build_revision=14ad04202fe5` -- driven through `ask`,
//! `ask --resume`, `ask --resume-id` from a second workspace, a turn the
//! Gateway failed, the `XFX_TUI=1` band in `ask` mode (a standing grant, an
//! amended approval, a Ctrl-C), and `setup llmux` with signed reasoning, all
//! against a loopback fake Gateway with a fake key. The one edit is the two
//! workspace paths, written `@WS_A@` and `@WS_B@`; a test puts its own in their
//! place and recomputes the manifest's byte count and digest over the result,
//! because those two fields describe exactly the bytes the log holds.
//! `.prd/tui-phase4/receipts/compat/README.md` records how each one was made
//! and what every newer field does to that binary.
//!
//! Three promises, each one owed to a user who upgrades without being asked:
//!
//! 1. **Every event the preview could write is read**, and a session made of
//!    them replays whole -- listed, shown, and sent back to the model.
//! 2. **Reading one changes nothing on disk**: no byte, no modification time,
//!    no mode, and no new file, for every command that only reads.
//! 3. **Continuing one only appends**, and only frames the preview itself
//!    reads. What the preview wrote stays, byte for byte, the head of the log;
//!    and because a frame of a kind it does not know makes it refuse the whole
//!    log (`EventEnvelope::decode`), a continued session that held one would
//!    be a session the preview can no longer open. Preview and stable are
//!    installed side by side, so that is a break a user would meet.

// The store's privacy checks, and the modes the fixtures are seeded with, are
// unix ones; the pty half needs a unix terminal.
#![cfg(unix)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::SystemTime;

use serde_json::Value;
use sha2::{Digest, Sha256};

use support::fake_gateway::{content_only, FakeGateway, Reply};
use support::fake_llmux::{anthropic_answer, catalog, FakeLlmux};
use support::pty::{Pty, Session};
use support::sandbox::Sandbox;

/// The logs the preview wrote, one directory per session, named by its id.
const PREVIEW_SESSIONS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/preview-14ad04202fe5/sessions"
);

/// Every event kind the preview knows: its `SessionEvent` at
/// `14ad04202fe5 src/session/event.rs:98-192`, eleven variants. A frame of any
/// other kind is one it refuses, and the whole log with it.
const PREVIEW_KINDS: [&str; 11] = [
    "session_started",
    "workspace_rebound",
    "preferences_changed",
    "project_context_recorded",
    "user_message",
    "assistant_message",
    "tool_result",
    "tool_feedback",
    "permission_grant_recorded",
    "usage_recorded",
    "turn_concluded",
];

// The fixture sessions, by what each one holds.

/// Three turns: a plain one, `ask --resume last`, and one under another model
/// and `--yolo` (a `preferences_changed`).
const PLAIN: &str = "1793710289633-bcb8a84ef3787872";
/// Every tool the preview offered, a command auto mode refused, and then a
/// resume from the second workspace (a `workspace_rebound`).
const TOOLS: &str = "1793710289931-261013bf2a61e1bb";
/// A turn the Gateway answered `400` after its first step.
const FAILED: &str = "1793710290481-2a1da65de6e48718";
/// The band in `ask` mode: an "always" (a standing grant), an amended
/// approval (`tool_feedback`), and a turn stopped with Ctrl-C.
const BAND: &str = "1793710290926-667439e7871f630c";
/// `setup llmux`: two turns whose signed thinking blocks are `raw_content`.
const LLMUX: &str = "1793710298000-f01f578aa7c07ba6";

const ALL: [&str; 5] = [PLAIN, TOOLS, FAILED, BAND, LLMUX];

/// The preview's sessions, in a sandbox profile of their own.
struct Preview {
    sandbox: Sandbox,
    /// The workspace `TOOLS` was rebound to: `@WS_B@`.
    other: PathBuf,
}

impl Preview {
    fn seed() -> Self {
        let sandbox = Sandbox::new();
        let other = sandbox
            .workspace
            .parent()
            .expect("the workspace has a parent")
            .join("workspace-b");
        fs::create_dir_all(&other).expect("create the second workspace");
        let other = other.canonicalize().expect("canonicalize");
        let a = json_text(&sandbox.workspace);
        let b = json_text(&other);

        let profile = sandbox.home.join(".xfx");
        let sessions = sandbox.sessions_dir();
        fs::create_dir_all(&sessions).expect("create the sessions directory");
        private(&profile, 0o700);
        private(&sessions, 0o700);
        for id in ALL {
            let from = Path::new(PREVIEW_SESSIONS).join(id);
            let log = fs::read_to_string(from.join("events.jsonl"))
                .expect("read a fixture log")
                .replace("@WS_A@", &a)
                .replace("@WS_B@", &b);
            let mut manifest: Value = serde_json::from_str(
                &fs::read_to_string(from.join("session.json"))
                    .expect("read a fixture manifest")
                    .replace("@WS_A@", &a)
                    .replace("@WS_B@", &b),
            )
            .expect("a fixture manifest is JSON");
            manifest["event_log_bytes"] = Value::from(log.len());
            manifest["event_log_sha256"] = Value::from(hex(&Sha256::digest(log.as_bytes())));

            let dir = sessions.join(id);
            fs::create_dir(&dir).expect("create a session directory");
            private(&dir, 0o700);
            fs::write(dir.join("events.jsonl"), &log).expect("write a log");
            private(&dir.join("events.jsonl"), 0o600);
            fs::write(
                dir.join("session.json"),
                serde_json::to_string(&manifest).expect("a manifest serializes"),
            )
            .expect("write a manifest");
            private(&dir.join("session.json"), 0o600);
        }
        Self { sandbox, other }
    }

    fn log_path(&self, id: &str) -> PathBuf {
        self.sandbox.sessions_dir().join(id).join("events.jsonl")
    }

    fn log(&self, id: &str) -> Vec<u8> {
        fs::read(self.log_path(id)).expect("read a log")
    }

    fn manifest(&self, id: &str) -> Value {
        let text = fs::read_to_string(self.sandbox.sessions_dir().join(id).join("session.json"))
            .expect("read a manifest");
        serde_json::from_str(&text).expect("a manifest is JSON")
    }

    /// The workspace a session is bound to now, so `--resume-id` from there
    /// continues it without a rebind.
    fn workspace_of(&self, id: &str) -> &Path {
        if id == TOOLS {
            &self.other
        } else {
            &self.sandbox.workspace
        }
    }

    fn xfx(&self, args: &[&str]) -> Output {
        self.sandbox.command().args(args).output().expect("run xfx")
    }
}

/// A path as it appears inside a JSON string, without the quotes.
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

fn ok(output: &Output, what: &str) -> String {
    assert!(
        output.status.success(),
        "{what} failed ({:?}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn kinds_of(log: &[u8]) -> Vec<String> {
    std::str::from_utf8(log)
        .expect("a log is UTF-8")
        .lines()
        .map(|line| {
            let frame: Value = serde_json::from_str(line).expect("a frame is JSON");
            frame["event"]["kind"]
                .as_str()
                .expect("a frame has a kind")
                .to_string()
        })
        .collect()
}

/// What is on disk under a directory: every file's bytes and every entry's
/// modification time and mode, by relative path.
fn tree(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, SystemTime, u32)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read a directory") {
            let path = entry.expect("a directory entry").path();
            let meta = fs::symlink_metadata(&path).expect("stat an entry");
            let bytes = if meta.is_dir() {
                pending.push(path.clone());
                None
            } else {
                Some(fs::read(&path).expect("read a file"))
            };
            let relative = path
                .strip_prefix(root)
                .expect("under the root")
                .to_path_buf();
            out.insert(
                relative,
                (
                    bytes,
                    meta.modified().expect("an mtime"),
                    meta.permissions().mode(),
                ),
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 1. every event the preview could write is read
// ---------------------------------------------------------------------------

#[test]
fn the_fixtures_hold_every_event_kind_the_preview_could_write() {
    // A guard on the fixtures rather than on the product: the promise below is
    // only as wide as the set of kinds it is proven over.
    let mut seen = BTreeSet::new();
    let mut interrupted = 0;
    let mut sealed = 0;
    for id in ALL {
        let log = fs::read_to_string(Path::new(PREVIEW_SESSIONS).join(id).join("events.jsonl"))
            .expect("read a fixture log");
        for line in log.lines() {
            let frame: Value = serde_json::from_str(line).expect("a frame is JSON");
            let event = &frame["event"];
            seen.insert(event["kind"].as_str().expect("a kind").to_string());
            if event["outcome"]["kind"] == "interrupted" {
                interrupted += 1;
            }
            if event["raw_content"].is_array() && event["wire"] == "anthropic_messages" {
                sealed += 1;
            }
        }
    }
    let every: BTreeSet<String> = PREVIEW_KINDS.iter().map(|kind| kind.to_string()).collect();
    assert_eq!(seen, every, "the fixtures and the preview's kinds differ");
    assert!(
        interrupted >= 2,
        "a failed and a cancelled turn: {interrupted}"
    );
    assert!(sealed >= 2, "signed llmux reasoning: {sealed}");
}

#[test]
fn every_session_the_preview_wrote_lists_and_replays_whole() {
    let preview = Preview::seed();

    let listed: Value = serde_json::from_str(&ok(
        &preview.xfx(&["sessions", "--all", "--json"]),
        "sessions --all --json",
    ))
    .expect("the listing is JSON");
    assert_eq!(listed["skipped_invalid"], 0, "{listed}");
    let ids: BTreeSet<&str> = listed["sessions"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|row| row["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, ALL.into_iter().collect(), "{listed}");

    for id in ALL {
        let detail: Value = serde_json::from_str(&ok(
            &preview.xfx(&["session", "--id", id, "--json"]),
            &format!("session --id {id}"),
        ))
        .expect("the detail is JSON");
        let opened = kinds_of(&preview.log(id))
            .iter()
            .filter(|kind| *kind == "user_message")
            .count();
        assert_eq!(detail["history_turns"], opened, "{id}: {detail}");
        assert_eq!(
            detail["turns"].as_array().expect("turns").len(),
            opened,
            "{id}: {detail}"
        );
        // Every turn the preview concluded is concluded here too: none lost
        // its outcome on the way in.
        for turn in detail["turns"].as_array().expect("turns") {
            assert!(turn["outcome"].is_object(), "{id}: {turn}");
        }
    }

    // What each kind carried is read as the preview meant it.
    let band: Value = serde_json::from_str(&ok(
        &preview.xfx(&["session", "--id", BAND, "--json"]),
        "the band session",
    ))
    .expect("JSON");
    assert_eq!(band["permission_mode"], "ask", "{band}");
    assert_eq!(
        band["grants"][0]["target"],
        preview
            .sandbox
            .workspace
            .join("granted.txt")
            .display()
            .to_string(),
        "{band}"
    );
    let feedback: Vec<&Value> = band["turns"][0]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter(|step| step["role"] == "feedback")
        .collect();
    assert_eq!(feedback.len(), 1, "{band}");
    assert_eq!(feedback[0]["call_id"], "t2");
    assert_eq!(feedback[0]["text"], "keep it short");
    assert_eq!(band["turns"][1]["outcome"]["kind"], "interrupted");
    assert_eq!(
        band["turns"][1]["outcome"]["reason"],
        "the turn was cancelled"
    );

    let failed: Value = serde_json::from_str(&ok(
        &preview.xfx(&["session", "--id", FAILED, "--json"]),
        "the failed session",
    ))
    .expect("JSON");
    let reason = failed["turns"][0]["outcome"]["reason"]
        .as_str()
        .expect("a reason");
    assert!(reason.contains("HTTP 400"), "{failed}");

    let plain: Value = serde_json::from_str(&ok(
        &preview.xfx(&["session", "--id", PLAIN, "--json"]),
        "the plain session",
    ))
    .expect("JSON");
    assert_eq!(plain["model"], "fake/model-b", "{plain}");
    assert_eq!(plain["permission_mode"], "yolo", "{plain}");

    let tools: Value = serde_json::from_str(&ok(
        &preview.xfx(&["session", "--id", TOOLS, "--json"]),
        "the tools session",
    ))
    .expect("JSON");
    assert_eq!(
        tools["workspace"],
        preview.other.display().to_string(),
        "{tools}"
    );
    assert_eq!(
        tools["origin_workspace"],
        preview.sandbox.workspace.display().to_string(),
        "{tools}"
    );
    let refused = tools["turns"][0]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter(|step| step["role"] == "tool" && step["ok"] == false)
        .count();
    assert_eq!(refused, 1, "the refused `rm`: {tools}");
}

#[test]
fn the_previews_signed_reasoning_goes_back_to_llmux_verbatim() {
    // The preview kept each llmux step's content blocks, signatures and all,
    // because Anthropic verifies a thinking block's signature when it comes
    // back; a step rebuilt from its text would be a 400 at the next request.
    // So the proof the field is read is the next request, not the listing.
    let preview = Preview::seed();
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["continued"]))])
        .with_catalog(catalog(&[("fake/model-a", &[])]));
    let mut command = preview.sandbox.command_with_llmux(&daemon);
    // The model the session's reasoning was sealed by: a state goes back only
    // to the model that wrote it.
    command.env("XFX_MODEL", "fake/model-a");
    let output = command
        .args(["ask", "--resume-id", LLMUX, "and once more"])
        .output()
        .expect("run xfx");
    ok(&output, "ask --resume-id on llmux");

    let request = daemon.only_message_request().json();
    let blocks: Vec<&Value> = request["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter(|message| message["role"] == "assistant")
        .flat_map(|message| message["content"].as_array().expect("content"))
        .filter(|block| block["type"] == "thinking")
        .collect();
    let signatures: Vec<&str> = blocks
        .iter()
        .map(|block| block["signature"].as_str().expect("a signature"))
        .collect();
    assert_eq!(
        signatures,
        ["sig-fake-one", "sig-fake-two", "sig-fake-three"],
        "{request}"
    );
}

// ---------------------------------------------------------------------------
// 2. reading one changes nothing on disk
// ---------------------------------------------------------------------------

#[test]
fn reading_a_session_the_preview_wrote_changes_nothing_on_disk() {
    let preview = Preview::seed();
    let before = tree(&preview.sandbox.home);

    let mut commands: Vec<Vec<&str>> = vec![
        vec!["sessions"],
        vec!["sessions", "--all"],
        vec!["sessions", "--all", "--json"],
        vec!["session", "last"],
        vec!["status"],
        vec!["doctor"],
    ];
    for id in ALL {
        commands.push(vec!["session", "--id", id]);
        commands.push(vec!["session", "--id", id, "--json"]);
    }
    for args in &commands {
        let output = preview.xfx(args);
        // `status` and `doctor` report the missing credential with a status
        // of their own; what matters here is what they leave behind.
        if args[0].starts_with("session") {
            ok(&output, &args.join(" "));
        }
        let after = tree(&preview.sandbox.home);
        assert_eq!(
            after,
            before,
            "`xfx {}` changed the profile",
            args.join(" ")
        );
    }
}

// ---------------------------------------------------------------------------
// 3. continuing one only appends, and only what the preview reads
// ---------------------------------------------------------------------------

/// What `id`'s log gained since `before`, after checking `before` is still
/// its head byte for byte.
fn appended(preview: &Preview, id: &str, before: &[u8]) -> Vec<String> {
    let after = preview.log(id);
    assert!(
        after.starts_with(before),
        "{id}: the bytes the preview wrote were rewritten"
    );
    let gained = kinds_of(&after[before.len()..]);
    let known: BTreeSet<&str> = PREVIEW_KINDS.into_iter().collect();
    for kind in &gained {
        assert!(
            known.contains(kind.as_str()),
            "{id}: appended a `{kind}`, which the preview refuses with the whole log"
        );
    }
    gained
}

#[test]
fn continuing_a_session_the_preview_wrote_appends_only_frames_the_preview_reads() {
    let preview = Preview::seed();
    for id in ALL {
        let before = preview.log(id);
        let published = preview.manifest(id);
        let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["continued here"]))]);
        let output = preview
            .sandbox
            .command_with(&gateway)
            .current_dir(preview.workspace_of(id))
            .args(["ask", "--resume-id", id, "one more question"])
            .output()
            .expect("run xfx");
        ok(&output, &format!("ask --resume-id {id}"));

        let gained = appended(&preview, id, &before);
        assert!(
            gained.contains(&"user_message".to_string()),
            "{id}: {gained:?}"
        );
        assert_eq!(gained.last().map(String::as_str), Some("turn_concluded"));

        // The published boundary moved to the new end and nothing else of
        // the manifest's identity did.
        let manifest = preview.manifest(id);
        assert_eq!(manifest["event_log_bytes"], preview.log(id).len());
        assert_eq!(
            manifest["last_event_seq"],
            published["last_event_seq"].as_u64().expect("a seq") + gained.len() as u64
        );
        for field in [
            "id",
            "log_generation",
            "created_at_ms",
            "origin_workspace_root",
        ] {
            assert_eq!(manifest[field], published[field], "{id}: {field}");
        }

        // The model was sent what the preview recorded: its first prompt
        // comes before the one just asked.
        let first = std::str::from_utf8(&before)
            .expect("UTF-8")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("JSON"))
            .find(|frame| frame["event"]["kind"] == "user_message")
            .expect("a first prompt")["event"]["text"]
            .as_str()
            .expect("text")
            .to_string();
        let body = &gateway.requests()[0].body;
        let earlier = body.find(&first).unwrap_or_else(|| panic!("{id}: {body}"));
        let asked = body.find("one more question").expect("the new prompt");
        assert!(
            earlier < asked,
            "{id}: the saved history did not come first"
        );

        ok(
            &preview.xfx(&["session", "--id", id]),
            &format!("session --id {id} after the continuation"),
        );
    }
}

#[test]
fn opening_a_session_the_preview_wrote_in_the_tui_and_leaving_it_writes_nothing() {
    // `--resume` and then `/quit`, with nothing typed, leaves the session as
    // the preview left it -- every file of it, byte for byte and down to its
    // modification time -- as fx's resume
    // appends nothing (`hydrateResumedSession`, `vercel-labs/fx@c1db9191
    // src/core/app/app_session_runtime.zig:2368-2420`). What the open changed
    // -- the provider no log the preview wrote names, the launch's mode -- is
    // written ahead of the first turn instead, which the TUI continuation
    // test below holds to the preview's kinds. Red before (`de6618de`): one
    // `preferences_changed` on every open, measured in the COMPAT receipt.
    let preview = Preview::seed();
    let gateway = FakeGateway::start(Vec::new());
    for id in [BAND, LLMUX] {
        let dir = preview.sandbox.sessions_dir().join(id);
        let before = tree(&dir);
        let pty = Pty::open();
        pty.resize(24, 100);
        let mut command = preview.sandbox.command_with(&gateway);
        command.env("XFX_TUI", "1");
        command.env_remove("TMUX");
        command.current_dir(preview.workspace_of(id));
        command.arg("--resume").arg(id);
        let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
        session.wait_for("session resumed:");
        assert_eq!(session.quit().code(), Some(0), "{id}");

        // Bytes, modes and modification times: the store cuts a crash tail
        // on a writable open, and only a tail.
        assert_eq!(
            tree(&dir),
            before,
            "{id}: opening and leaving the session changed it"
        );
        ok(
            &preview.xfx(&["session", "--id", id]),
            &format!("session --id {id} after the TUI opened it"),
        );
    }
    assert_eq!(
        gateway.request_count(),
        0,
        "opening a session asked the model"
    );
    assert!(
        gateway.title_requests().is_empty(),
        "opening a session asked for a title"
    );
}

#[test]
fn continuing_a_session_the_preview_wrote_in_the_tui_appends_only_frames_the_preview_reads() {
    // The TUI's own continuation, beside `ask`'s above: `--resume`, one
    // prompt, `/quit`. What the open changed is written just ahead of that
    // prompt -- the provider no log the preview wrote names, and the launch's
    // mode over the band session's `ask` -- as a `preferences_changed`, a kind
    // the preview reads; and the saved bytes stay the head of the log.
    let preview = Preview::seed();
    for id in [BAND, PLAIN] {
        let before = preview.log(id);
        let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["MARKER-TUI-TURN"]))]);
        let pty = Pty::open();
        pty.resize(24, 100);
        let mut command = preview.sandbox.command_with(&gateway);
        command.env("XFX_TUI", "1");
        command.env_remove("TMUX");
        command.current_dir(preview.workspace_of(id));
        command.arg("--resume").arg(id);
        let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
        session.wait_for("session resumed:");
        session.type_bytes(b"one more question\r");
        session.wait_for("MARKER-TUI-TURN");
        // The turn's conclusion is on disk before the band shows it ended,
        // but not before its answer: wait for the log to say so.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !kinds_of(&preview.log(id)[before.len()..]).contains(&"turn_concluded".to_string()) {
            assert!(
                std::time::Instant::now() < deadline,
                "{id}: the turn never concluded"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(session.quit().code(), Some(0), "{id}");

        let gained = appended(&preview, id, &before);
        assert_eq!(
            gained[..3],
            [
                "preferences_changed",
                "project_context_recorded",
                "user_message"
            ],
            "{id}: {gained:?}"
        );
        let first: Value = serde_json::from_str(
            std::str::from_utf8(&preview.log(id)[before.len()..])
                .expect("UTF-8")
                .lines()
                .next()
                .expect("a frame"),
        )
        .expect("a frame");
        assert_eq!(first["event"]["provider"], "gateway", "{id}: {first}");
        if id == BAND {
            assert_eq!(first["event"]["permission_mode"], "auto", "{id}: {first}");
        }
        assert_eq!(
            preview.manifest(id)["event_log_bytes"],
            preview.log(id).len()
        );
        ok(
            &preview.xfx(&["session", "--id", id]),
            &format!("session --id {id} after the TUI continued it"),
        );
    }
}

// ---------------------------------------------------------------------------
// 4. a session this build starts is one the preview reads
// ---------------------------------------------------------------------------

#[test]
fn a_session_the_tui_starts_holds_its_early_settings_in_frames_the_preview_reads() {
    // The other way round: a log this build writes, for the preview to open.
    // The effort and the speed chosen before a session's first prompt are
    // written at its first save, as fx writes the settings it held until its
    // first turn (`src/core/session_manager/session.zig:415-429`,
    // `:503-539@481104fd`): one `preferences_changed` right behind
    // `session_started`. A kind the preview knows, `model` and
    // `permission_mode` written as the `null` it reads as "unchanged", and
    // three fields it predates and reads past -- its variants have no
    // `deny_unknown_fields` (`14ad04202fe5 src/session/event.rs:96-118`).
    // Every frame of the log is one of the preview's kinds, so the preview
    // opens it whole (`.prd/tui-phase5/receipts/session-early-prefs/README.md`
    // runs the preview's own `session --id` and `ask --resume-id` on one).
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["MARKER-EARLY-PREFS"]))])
        .with_catalog(serde_json::json!({"object": "list", "data": [
            {"id": "anthropic/claude-opus-5", "type": "language", "owned_by": "anthropic",
             "released": 1_775_000_000, "tags": ["tool-use", "reasoning"],
             "context_window": 1_000_000, "max_tokens": 64_000,
             "reasoning_options": [{"type": "effort",
                                    "values": ["low", "medium", "high", "max"]}],
             "fast_options": [{"type": "toggle"}]}
        ]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    pty.resize(40, 120);
    let mut command = sandbox.command_with(&gateway);
    command.env("XFX_TUI", "1");
    command.env_remove("TMUX");
    let mut session = Session::spawn_without_taking_the_terminal(&pty, command);
    // Bracketed paste on: the composer is up.
    session.wait_for("\u{1b}[?2004h");
    // Pasted, and Enter after the paste has landed: typed key by key, the
    // space after the id is the inline column's.
    session.type_bytes(b"\x1b[200~/model anthropic/claude-opus-5 high fast\x1b[201~");
    session.wait_for("claude-opus-5 high fast");
    session.type_bytes(b"\r");
    session.wait_for("Switched to anthropic/claude-opus-5 (effort: high, speed: fast)");
    assert!(
        sandbox.session_ids().is_empty(),
        "a choice before the first prompt wrote a session"
    );
    session.type_bytes(b"first prompt please\r");
    session.wait_for("MARKER-EARLY-PREFS");
    let id = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let ids = sandbox.session_ids();
            if let [id] = ids.as_slice() {
                let log = fs::read(sandbox.sessions_dir().join(id).join("events.jsonl"))
                    .unwrap_or_default();
                if kinds_of(&log).contains(&"turn_concluded".to_string()) {
                    break id.clone();
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the turn never concluded: {ids:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    assert_eq!(session.quit().code(), Some(0));

    let log = fs::read(sandbox.sessions_dir().join(&id).join("events.jsonl")).expect("the log");
    let kinds = kinds_of(&log);
    let known: BTreeSet<&str> = PREVIEW_KINDS.into_iter().collect();
    for kind in &kinds {
        assert!(
            known.contains(kind.as_str()),
            "a `{kind}`, which the preview refuses with the whole log: {kinds:?}"
        );
    }
    assert_eq!(
        kinds[..4],
        [
            "session_started",
            "preferences_changed",
            "project_context_recorded",
            "user_message"
        ],
        "{kinds:?}"
    );
    let events: Vec<Value> = std::str::from_utf8(&log)
        .expect("UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a frame")["event"].clone())
        .collect();
    assert_eq!(
        events[0]["model"], "anthropic/claude-opus-5",
        "{}",
        events[0]
    );
    assert_eq!(
        events[1],
        serde_json::json!({
            "kind": "preferences_changed",
            "model": null,
            "permission_mode": null,
            "effort": "high",
            "fast_mode": true,
            "ultrafast_mode": false,
        })
    );
    // And the turn was asked in them.
    let asked = gateway.requests()[0].json();
    assert_eq!(asked["reasoning"], "high", "{asked}");
    assert_eq!(
        asked["providerOptions"]["gateway"]["speed"], "fast",
        "{asked}"
    );
}

// ---------------------------------------------------------------------------
// 5. forking one leaves it as the preview left it
// ---------------------------------------------------------------------------

/// A fork of each session the preview wrote, before its last turn (P5-FK1,
/// upstream's `openFork`, `vercel-labs/fx@481104fd
/// src/core/session_manager/session.zig:1714-1759`): the source keeps every
/// byte, time and mode the preview gave it -- so the preview opens it as it
/// did -- and the fork holds what the preview wrote and a rename, which this
/// build reads whole.
#[test]
fn forking_a_session_the_preview_wrote_leaves_it_as_the_preview_left_it() {
    let preview = Preview::seed();
    let sessions = preview.sandbox.sessions_dir();
    let store =
        xfx::session::SessionStore::open(&preview.sandbox.home.join(".xfx")).expect("the store");
    let before: Vec<_> = ALL.iter().map(|id| tree(&sessions.join(id))).collect();
    let known: BTreeSet<&str> = PREVIEW_KINDS.into_iter().collect();

    for id in ALL {
        let turns = kinds_of(&preview.log(id))
            .iter()
            .filter(|kind| *kind == "user_message")
            .count();
        let source = xfx::session::SessionId::parse(id).expect("an id");
        let fork = store
            .fork(&source, turns, preview.workspace_of(id))
            .unwrap_or_else(|err| panic!("{id}: {err}"));
        let fork_id = fork.id().to_string();
        drop(fork);

        let detail: Value = serde_json::from_str(&ok(
            &preview.xfx(&["session", "--id", &fork_id, "--json"]),
            &format!("the fork of {id}"),
        ))
        .expect("the detail is JSON");
        assert_eq!(detail["history_turns"], turns - 1, "{id}: {detail}");

        // Every frame is one the preview wrote or reads, but the rename.
        let log = fs::read(sessions.join(&fork_id).join("events.jsonl")).expect("the fork's log");
        let kinds = kinds_of(&log);
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| *kind == "session_renamed")
                .count(),
            1,
            "{id}: {kinds:?}"
        );
        for kind in &kinds {
            assert!(
                kind == "session_renamed" || known.contains(kind.as_str()),
                "{id}: {kind}"
            );
        }
        let first: Value = serde_json::from_str(
            std::str::from_utf8(&log)
                .expect("UTF-8")
                .lines()
                .next()
                .expect("line 1"),
        )
        .expect("a frame");
        assert_eq!(first["event"]["forked_from"]["id"], id, "{id}: {first}");
    }

    let after: Vec<_> = ALL.iter().map(|id| tree(&sessions.join(id))).collect();
    assert_eq!(after, before, "a fork wrote to a session the preview wrote");
    // Each listed beside its source, under the source's title marked once.
    let listed: Value = serde_json::from_str(&ok(
        &preview.xfx(&["sessions", "--all", "--json"]),
        "sessions --all --json",
    ))
    .expect("the listing is JSON");
    assert_eq!(listed["skipped_invalid"], 0, "{listed}");
    let rows = listed["sessions"].as_array().expect("a list");
    assert_eq!(rows.len(), 2 * ALL.len(), "{listed}");
    let forks = rows
        .iter()
        .filter(|row| !ALL.contains(&row["id"].as_str().expect("an id")))
        .filter(|row| {
            row["title"].as_str().is_some_and(|title| {
                title.ends_with(" (fork)") && !title.ends_with(" (fork) (fork)")
            })
        })
        .count();
    assert_eq!(forks, ALL.len(), "{listed}");
    for id in ALL {
        ok(
            &preview.xfx(&["session", "--id", id]),
            &format!("session --id {id} after it was forked"),
        );
    }
}
