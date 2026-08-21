//! Durable sessions, resume, and bounded project context.
//!
//! Four promises are proven here, and each one is a product promise:
//!
//! 1. **A published boundary is the whole truth.** A reader replays exactly the
//!    bytes the manifest published, and an unpublished, malformed, or truncated
//!    crash tail is invisible to it. A writable open removes the excess rather
//!    than reasoning about it.
//! 2. **Everything else fails closed.** A sequence gap, a repeated event id, a
//!    manifest that does not describe its log, and an unsafe session path are
//!    refusals with names, not best-effort reads.
//! 3. **Resume is explicit.** `last` never crosses a workspace; an exact id that
//!    does records a durable rebind event before the turn runs.
//! 4. **Context is current, not remembered.** Project instructions are
//!    rediscovered after a resume and before a tool target is admitted, come
//!    only from bounded `AGENTS.md` files, and are labelled with where they came
//!    from.
//!
//! Nothing here uses a real credential or a real endpoint. Upstream evidence is
//! pinned to `vercel-labs/fx@580a0c5da9386317251968c09c1cee69e763487a`.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};
use tempfile::TempDir;

use fxr::config::PermissionMode;
use fxr::gateway::protocol::Role;
use fxr::session::{
    Clock, ListFilter, ListScope, NewSession, RecordedToolCall, Selector, SessionError,
    SessionEvent, SessionId, SessionStore, TurnConclusion, TurnStep, EVENTS_FILE, MANIFEST_FILE,
};
use fxr::workspace::{AccessScope, ProjectContext};

use support::fake_gateway::{
    content_only, finish, sse_body, text_delta, tool_call, FakeGateway, Reply,
};

/// Environment variables that must never leak in from the developer's shell.
const CONTROLLED_VARS: &[&str] = &[
    "VERCEL_OIDC_TOKEN",
    "AI_GATEWAY_API_KEY",
    "FXR_MODEL",
    "FXR_PERMISSION_MODE",
    "FXR_MAX_AGENT_STEPS",
    "FXR_GATEWAY_URL",
];

/// A test secret that must never reach a session file, stdout, or stderr.
const TEST_KEY: &str = "fxr-test-session-key-must-not-appear";

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

/// A profile directory with a store whose clock a test drives by hand.
struct Profile {
    _root: TempDir,
    dir: PathBuf,
    clock: Clock,
}

impl Profile {
    fn new() -> Self {
        let root = TempDir::new().expect("create a profile root");
        let dir = root
            .path()
            .canonicalize()
            .expect("canonicalize")
            .join(".fxr");
        Self {
            _root: root,
            dir,
            clock: Clock::manual(1_000),
        }
    }

    fn store(&self) -> SessionStore {
        SessionStore::open(&self.dir)
            .expect("open the store")
            .with_clock(self.clock.clone())
    }

    fn read_only_store(&self) -> SessionStore {
        SessionStore::read_only(&self.dir).with_clock(self.clock.clone())
    }

    fn sessions_dir(&self) -> PathBuf {
        self.dir.join("sessions")
    }
}

/// A workspace directory a session can be bound to.
struct Workspace {
    _root: TempDir,
    root: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let root = TempDir::new().expect("create a workspace");
        let path = root.path().canonicalize().expect("canonicalize");
        Self {
            _root: root,
            root: path,
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(&path, contents).expect("write fixture");
        path
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::create_dir_all(&path).expect("create directory");
        path
    }
}

fn new_session(workspace: &Path) -> NewSession {
    NewSession {
        origin_workspace_root: workspace.to_path_buf(),
        workspace_root: workspace.to_path_buf(),
        model: "test/model".to_string(),
        permission_mode: PermissionMode::Auto,
    }
}

fn id(raw: &str) -> SessionId {
    SessionId::parse(raw).expect("a safe session id")
}

/// The events of one complete turn that read, edited, and ran a command.
fn evidence_events() -> Vec<SessionEvent> {
    vec![
        SessionEvent::ProjectContextRecorded {
            sources: vec!["/w/AGENTS.md".to_string()],
            bytes: 42,
        },
        SessionEvent::UserMessage {
            text: "fix the greeting".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: "looking".to_string(),
            tool_calls: vec![
                RecordedToolCall {
                    id: "c1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "greeting.txt" }),
                },
                RecordedToolCall {
                    id: "c2".to_string(),
                    name: "edit_file".to_string(),
                    input: json!({ "path": "greeting.txt", "old_string": "hi", "new_string": "hello" }),
                },
                RecordedToolCall {
                    id: "c3".to_string(),
                    name: "terminal".to_string(),
                    input: json!({ "action": "exec", "command": "cat greeting.txt" }),
                },
            ],
        },
        SessionEvent::ToolResult {
            call_id: "c1".to_string(),
            tool: "read_file".to_string(),
            ok: true,
            output: "1\thi".to_string(),
        },
        SessionEvent::ToolResult {
            call_id: "c2".to_string(),
            tool: "edit_file".to_string(),
            ok: true,
            output: "Edited greeting.txt".to_string(),
        },
        SessionEvent::ToolResult {
            call_id: "c3".to_string(),
            tool: "terminal".to_string(),
            ok: true,
            output: "hello".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: "done".to_string(),
            tool_calls: Vec::new(),
        },
        SessionEvent::UsageRecorded {
            input_tokens: Some(11),
            output_tokens: Some(7),
        },
        SessionEvent::PermissionGrantRecorded {
            tool: "edit_file".to_string(),
            target: "greeting.txt".to_string(),
        },
        SessionEvent::TurnConcluded {
            outcome: TurnConclusion::Final {
                finish_reason: "stop".to_string(),
                steps: 2,
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// the event log and its published boundary
// ---------------------------------------------------------------------------

#[test]
fn a_committed_turn_replays_exactly_through_the_published_boundary() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("turn-evidence"), new_session(workspace.path()))
        .expect("create the session");
    for event in evidence_events() {
        store.append(&mut session, event).expect("append");
        store.publish(&mut session).expect("publish");
    }
    let published = session.published_bytes();
    drop(session);

    // A different store object, as a new process would see it.
    let reader = profile.read_only_store();
    let detail = reader
        .detail(&Selector::Id(id("turn-evidence")), workspace.path())
        .expect("read the session back");
    let state = &detail.state;

    assert_eq!(state.id, "turn-evidence");
    assert_eq!(state.model, "test/model");
    assert_eq!(state.permission_mode, PermissionMode::Auto);
    assert_eq!(state.total_input_tokens, 11);
    assert_eq!(state.total_output_tokens, 7);
    assert_eq!(state.grants.len(), 1);
    assert_eq!(state.grants[0].tool, "edit_file");
    assert_eq!(state.turns.len(), 1);

    let turn = &state.turns[0];
    assert_eq!(turn.user, "fix the greeting");
    assert_eq!(turn.steps.len(), 5, "{:?}", turn.steps);
    let tools: Vec<&str> = turn
        .steps
        .iter()
        .filter_map(|step| match step {
            TurnStep::ToolResult { tool, .. } => Some(tool.as_str()),
            TurnStep::Assistant { .. } => None,
        })
        .collect();
    assert_eq!(tools, ["read_file", "edit_file", "terminal"]);
    assert!(matches!(
        turn.outcome,
        Some(TurnConclusion::Final { steps: 2, .. })
    ));

    // The manifest publishes the exact byte boundary the reader stopped at.
    assert_eq!(detail.manifest.event_log_bytes, published);
    let log = fs::read(
        profile
            .sessions_dir()
            .join("turn-evidence")
            .join(EVENTS_FILE),
    )
    .expect("read the log");
    assert_eq!(log.len() as u64, published);

    // History is replayed in wire order: user, assistant+calls, results, answer.
    let messages = state.history_messages();
    let roles: Vec<Role> = messages.iter().map(|message| message.role).collect();
    assert_eq!(
        roles,
        [
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Tool,
            Role::Tool,
            Role::Assistant
        ]
    );
    assert_eq!(messages[0].text(), "fix the greeting");
    assert_eq!(messages[5].text(), "done");
}

#[test]
fn an_unpublished_valid_crash_tail_is_invisible_to_readers() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("crash-valid"), new_session(workspace.path()))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "published".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    let published = session.published_bytes();

    // A well-formed event that the process died before publishing.
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "unpublished".to_string(),
            },
        )
        .expect("append");
    assert!(session.pending_bytes() > 0, "the tail must be on disk");
    drop(session);

    let log = profile.sessions_dir().join("crash-valid").join(EVENTS_FILE);
    assert!(
        fs::read(&log).expect("read").len() as u64 > published,
        "the crash tail must really be in the file"
    );

    let state = profile
        .read_only_store()
        .detail(&Selector::Id(id("crash-valid")), workspace.path())
        .expect("read past the crash")
        .state;
    assert_eq!(state.turns.len(), 1);
    assert_eq!(state.turns[0].user, "published");
}

#[test]
fn a_malformed_or_truncated_crash_tail_is_invisible_to_readers() {
    for (name, tail) in [
        ("crash-malformed", "{not json at all}\n".to_string()),
        (
            "crash-truncated",
            "{\"schema_version\":1,\"seq\":2".to_string(),
        ),
        (
            "crash-halfline",
            "{\"schema_version\":1,\"log_generation\":\"00\"".to_string(),
        ),
    ] {
        let profile = Profile::new();
        let workspace = Workspace::new();
        let store = profile.store();
        let mut session = store
            .create(id(name), new_session(workspace.path()))
            .expect("create");
        store
            .append(
                &mut session,
                SessionEvent::UserMessage {
                    text: "published".to_string(),
                },
            )
            .expect("append");
        store.publish(&mut session).expect("publish");
        drop(session);

        let log = profile.sessions_dir().join(name).join(EVENTS_FILE);
        let mut bytes = fs::read(&log).expect("read");
        bytes.extend_from_slice(tail.as_bytes());
        fs::write(&log, &bytes).expect("write the crash tail");

        let state = profile
            .read_only_store()
            .detail(&Selector::Id(id(name)), workspace.path())
            .unwrap_or_else(|err| panic!("{name} must still read: {err}"))
            .state;
        assert_eq!(state.turns.len(), 1, "{name}");
        assert_eq!(state.turns[0].user, "published", "{name}");
    }
}

#[test]
fn a_writable_open_truncates_everything_past_the_published_boundary() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("truncating"), new_session(workspace.path()))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "published".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    let published = session.published_bytes();
    drop(session);

    let log = profile.sessions_dir().join("truncating").join(EVENTS_FILE);
    let mut bytes = fs::read(&log).expect("read");
    bytes.extend_from_slice(b"{\"unpublished\":true}\n");
    fs::write(&log, &bytes).expect("write a crash tail");

    let resumed = profile
        .store()
        .resume(&Selector::Id(id("truncating")), workspace.path())
        .expect("resume");
    assert_eq!(resumed.session.published_bytes(), published);
    assert_eq!(
        fs::metadata(&log).expect("stat").len(),
        published,
        "a writable open must remove the excess rather than reason about it"
    );
}

#[test]
fn a_sequence_gap_fails_closed() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    corrupt_log(&profile, workspace.path(), "seq-gap", |line| {
        line.replace("\"seq\":2", "\"seq\":3")
    });
    let err = profile
        .read_only_store()
        .detail(&Selector::Id(id("seq-gap")), workspace.path())
        .expect_err("a sequence gap must fail closed");
    assert!(matches!(err, SessionError::Corrupt { .. }), "{err}");
    assert!(err.to_string().contains("seq-gap"), "{err}");
}

#[test]
fn a_repeated_event_id_fails_closed() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("dup-event"), new_session(workspace.path()))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "one".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    drop(session);

    let log = profile.sessions_dir().join("dup-event").join(EVENTS_FILE);
    let text = fs::read_to_string(&log).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let first_id = event_field(&lines[0], "event_id");
    lines[1] = replace_field(&lines[1], "event_id", &first_id);
    republish(&profile, "dup-event", &lines);

    let err = profile
        .read_only_store()
        .detail(&Selector::Id(id("dup-event")), workspace.path())
        .expect_err("a repeated event id must fail closed");
    assert!(matches!(err, SessionError::Corrupt { .. }), "{err}");
}

#[test]
fn a_duplicate_session_id_is_refused_rather_than_reopened() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let session = store
        .create(id("only-once"), new_session(workspace.path()))
        .expect("create");
    drop(session);
    let err = store
        .create(id("only-once"), new_session(workspace.path()))
        .expect_err("a session id is claimed once");
    assert!(matches!(err, SessionError::AlreadyExists { .. }), "{err}");
}

#[test]
fn an_unsafe_session_id_never_becomes_a_path() {
    for raw in [
        "",
        ".",
        "..",
        "../escape",
        "a/b",
        "a\\b",
        "with space",
        "tilde~",
        &"x".repeat(65),
    ] {
        assert!(
            SessionId::parse(raw).is_err(),
            "`{raw}` must be refused as a session id"
        );
    }
    for raw in ["a", "session-1", "01abcdef-9f", &"x".repeat(64)] {
        assert!(SessionId::parse(raw).is_ok(), "`{raw}` must be accepted");
    }
}

/// One way of damaging a published manifest.
type ManifestDamage = Box<dyn Fn(&mut Value)>;

#[test]
fn a_manifest_that_does_not_describe_its_log_fails_closed() {
    let workspace = Workspace::new();
    let cases: Vec<(&str, ManifestDamage)> = vec![
        (
            "bad-digest",
            Box::new(|manifest: &mut Value| {
                manifest["event_log_sha256"] = json!("0".repeat(64));
            }),
        ),
        (
            "past-eof",
            Box::new(|manifest: &mut Value| {
                let bytes = manifest["event_log_bytes"].as_u64().expect("bytes");
                manifest["event_log_bytes"] = json!(bytes + 4_096);
            }),
        ),
        (
            "wrong-id",
            Box::new(|manifest: &mut Value| {
                manifest["id"] = json!("some-other-session");
            }),
        ),
        (
            "future-schema",
            Box::new(|manifest: &mut Value| {
                manifest["schema_version"] = json!(99);
            }),
        ),
        (
            "wrong-turn-count",
            Box::new(|manifest: &mut Value| {
                manifest["history_turns"] = json!(41);
            }),
        ),
    ];

    for (name, mutate) in cases {
        let profile = Profile::new();
        let store = profile.store();
        let mut session = store
            .create(id(name), new_session(workspace.path()))
            .expect("create");
        store
            .append(
                &mut session,
                SessionEvent::UserMessage {
                    text: "one".to_string(),
                },
            )
            .expect("append");
        store.publish(&mut session).expect("publish");
        drop(session);

        let path = profile.sessions_dir().join(name).join(MANIFEST_FILE);
        let mut manifest: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json manifest");
        mutate(&mut manifest);
        fs::write(&path, format!("{manifest}\n")).expect("write");

        let err = profile
            .read_only_store()
            .detail(&Selector::Id(id(name)), workspace.path())
            .expect_err("a mismatched manifest must fail closed");
        assert!(
            matches!(err, SessionError::Corrupt { .. }),
            "{name}: {err:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn session_state_is_private_to_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("private"), new_session(workspace.path()))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "one".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    drop(session);

    let mode = |path: &Path| {
        fs::metadata(path)
            .unwrap_or_else(|err| panic!("stat {}: {err}", path.display()))
            .permissions()
            .mode()
            & 0o777
    };
    let session_dir = profile.sessions_dir().join("private");
    assert_eq!(mode(&profile.dir), 0o700, "the profile directory");
    assert_eq!(mode(&profile.sessions_dir()), 0o700, "the sessions dir");
    assert_eq!(mode(&session_dir), 0o700, "the session dir");
    assert_eq!(mode(&session_dir.join(EVENTS_FILE)), 0o600, "the event log");
    assert_eq!(
        mode(&session_dir.join(MANIFEST_FILE)),
        0o600,
        "the manifest"
    );
}

#[cfg(unix)]
#[test]
fn a_world_readable_log_is_refused_for_writing() {
    use std::os::unix::fs::PermissionsExt;

    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let session = store
        .create(id("leaky"), new_session(workspace.path()))
        .expect("create");
    drop(session);

    let log = profile.sessions_dir().join("leaky").join(EVENTS_FILE);
    fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).expect("relax the mode");

    let err = profile
        .store()
        .resume(&Selector::Id(id("leaky")), workspace.path())
        .expect_err("a writable open must refuse a shared log");
    assert!(
        matches!(err, SessionError::InsecurePermissions { .. }),
        "{err}"
    );
}

// ---------------------------------------------------------------------------
// list and detail
// ---------------------------------------------------------------------------

#[test]
fn list_is_newest_first_and_scoped_to_the_current_workspace() {
    let profile = Profile::new();
    let here = Workspace::new();
    let elsewhere = Workspace::new();
    let store = profile.store();

    for (name, workspace, at) in [
        ("older", here.path(), 1_000),
        ("newer", here.path(), 2_000),
        ("other", elsewhere.path(), 3_000),
    ] {
        profile.clock.set(at);
        let mut session = store
            .create(id(name), new_session(workspace))
            .expect("create");
        store
            .append(
                &mut session,
                SessionEvent::UserMessage {
                    text: format!("prompt for {name}"),
                },
            )
            .expect("append");
        store.publish(&mut session).expect("publish");
    }

    let scoped = store
        .list(&ListFilter::new(ListScope::CurrentWorkspace(
            here.path().to_path_buf(),
        )))
        .expect("list");
    let ids: Vec<&str> = scoped
        .sessions
        .iter()
        .map(|summary| summary.id.as_str())
        .collect();
    assert_eq!(ids, ["newer", "older"], "newest first, this workspace only");
    assert_eq!(scoped.sessions[0].history_turns, 1);
    assert_eq!(
        scoped.sessions[0].title.as_deref(),
        Some("prompt for newer")
    );
    assert!(!scoped.has_more);

    let all = store
        .list(&ListFilter::new(ListScope::AllWorkspaces))
        .expect("list all");
    let ids: Vec<&str> = all
        .sessions
        .iter()
        .map(|summary| summary.id.as_str())
        .collect();
    assert_eq!(ids, ["other", "newer", "older"]);

    let bounded = store
        .list(&ListFilter::new(ListScope::AllWorkspaces).with_limit(2))
        .expect("list bounded");
    assert_eq!(bounded.sessions.len(), 2);
    assert!(bounded.has_more, "a bounded list says there is more");
}

#[test]
fn a_list_skips_a_corrupt_session_and_counts_it_rather_than_failing() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    for name in ["good", "broken"] {
        let mut session = store
            .create(id(name), new_session(workspace.path()))
            .expect("create");
        store
            .append(
                &mut session,
                SessionEvent::UserMessage {
                    text: name.to_string(),
                },
            )
            .expect("append");
        store.publish(&mut session).expect("publish");
    }
    fs::write(
        profile.sessions_dir().join("broken").join(MANIFEST_FILE),
        "{not a manifest}\n",
    )
    .expect("break the manifest");

    let listed = store
        .list(&ListFilter::new(ListScope::AllWorkspaces))
        .expect("a broken session must not break the listing");
    let ids: Vec<&str> = listed
        .sessions
        .iter()
        .map(|summary| summary.id.as_str())
        .collect();
    assert_eq!(ids, ["good"]);
    assert_eq!(listed.skipped_invalid, 1);
}

#[test]
fn reading_a_session_never_creates_profile_state() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let reader = profile.read_only_store();
    assert!(reader
        .list(&ListFilter::new(ListScope::AllWorkspaces))
        .expect("an empty listing is not an error")
        .sessions
        .is_empty());
    assert!(reader
        .detail(&Selector::Last, workspace.path())
        .is_err_and(|err| matches!(err, SessionError::NoSession { .. })));
    assert!(
        !profile.dir.exists(),
        "a read-only store must not create {}",
        profile.dir.display()
    );
}

// ---------------------------------------------------------------------------
// resume
// ---------------------------------------------------------------------------

#[test]
fn resume_by_exact_id_restores_history_and_preferences() {
    let profile = Profile::new();
    let workspace = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("restore-me"), new_session(workspace.path()))
        .expect("create");
    for event in evidence_events() {
        store.append(&mut session, event).expect("append");
        store.publish(&mut session).expect("publish");
    }
    drop(session);

    let resumed = profile
        .store()
        .resume(&Selector::Id(id("restore-me")), workspace.path())
        .expect("resume");
    assert!(!resumed.rebound, "the same workspace is not a rebind");
    let state = resumed.session.state();
    assert_eq!(state.model, "test/model");
    assert_eq!(state.permission_mode, PermissionMode::Auto);
    assert_eq!(state.turns.len(), 1);
    assert_eq!(state.grants.len(), 1);
    assert_eq!(state.history_messages().len(), 6);
}

#[test]
fn resume_last_takes_the_latest_session_of_this_workspace_and_never_crosses_one() {
    let profile = Profile::new();
    let here = Workspace::new();
    let elsewhere = Workspace::new();
    let store = profile.store();

    profile.clock.set(1_000);
    store
        .create(id("here-old"), new_session(here.path()))
        .expect("create");
    profile.clock.set(2_000);
    store
        .create(id("here-new"), new_session(here.path()))
        .expect("create");
    profile.clock.set(3_000);
    store
        .create(id("there"), new_session(elsewhere.path()))
        .expect("create");

    let resumed = profile
        .store()
        .resume(&Selector::Last, here.path())
        .expect("resume last");
    assert_eq!(resumed.session.id().as_str(), "here-new");
    assert!(!resumed.rebound);

    // A workspace with no session of its own does not silently borrow one.
    let empty = Workspace::new();
    let err = profile
        .store()
        .resume(&Selector::Last, empty.path())
        .expect_err("`last` must not cross a workspace");
    assert!(matches!(err, SessionError::NoSession { .. }), "{err}");
}

#[test]
fn resume_by_exact_id_across_a_workspace_records_a_durable_rebind() {
    let profile = Profile::new();
    let origin = Workspace::new();
    let elsewhere = Workspace::new();
    let store = profile.store();
    let mut session = store
        .create(id("travels"), new_session(origin.path()))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "asked at home".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    drop(session);

    let resumed = profile
        .store()
        .resume(&Selector::Id(id("travels")), elsewhere.path())
        .expect("an exact id may move");
    assert!(resumed.rebound, "moving a session is an explicit event");
    assert_eq!(
        resumed.session.state().workspace_root,
        elsewhere.path().to_string_lossy()
    );
    assert_eq!(
        resumed.session.state().origin_workspace_root,
        origin.path().to_string_lossy(),
        "the origin is remembered, not overwritten"
    );
    drop(resumed);

    // The rebind is durable: a later reader sees the new binding.
    let state = profile
        .read_only_store()
        .detail(&Selector::Id(id("travels")), elsewhere.path())
        .expect("read back")
        .state;
    assert_eq!(state.workspace_root, elsewhere.path().to_string_lossy());
    assert_eq!(state.turns.len(), 1);

    // And `last` in the origin workspace no longer finds it.
    assert!(profile
        .store()
        .resume(&Selector::Last, origin.path())
        .is_err_and(|err| matches!(err, SessionError::NoSession { .. })));
}

// ---------------------------------------------------------------------------
// project context
// ---------------------------------------------------------------------------

fn workspace_scope(workspace: &Workspace) -> AccessScope {
    AccessScope::primary_only(workspace.path()).expect("a usable root")
}

#[test]
fn project_context_runs_root_to_workspace_and_labels_every_source() {
    let outer = Workspace::new();
    let workspace_root = outer.mkdir("project");
    outer.write("AGENTS.md", "OUTER RULE\n");
    outer.write("project/AGENTS.md", "PROJECT RULE\n");

    let scope = AccessScope::primary_only(&workspace_root).expect("a usable root");
    let context = ProjectContext::discover(&scope);
    let rendered = context.render();

    let outer_source = outer.path().join("AGENTS.md");
    let project_source = workspace_root.join("AGENTS.md");
    assert!(rendered.contains("OUTER RULE"), "{rendered}");
    assert!(rendered.contains("PROJECT RULE"), "{rendered}");
    assert!(
        rendered.contains(&format!(
            "<ancestor-rules from=\"{}\"",
            outer_source.display()
        )),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!(
            "<project-rules from=\"{}\"",
            project_source.display()
        )),
        "{rendered}"
    );
    // The narrowest scope is last, which is where a reader resolves a conflict.
    let outer_at = rendered.find("OUTER RULE").expect("outer");
    let project_at = rendered.find("PROJECT RULE").expect("project");
    assert!(outer_at < project_at, "{rendered}");
    // The guidance says these are conventions, not authority.
    assert!(
        rendered.contains("<project-instructions-guidance>"),
        "{rendered}"
    );
    assert!(rendered.contains("take precedence"), "{rendered}");

    let sources: Vec<String> = context
        .sources()
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    assert_eq!(
        sources,
        [
            outer_source.display().to_string(),
            project_source.display().to_string()
        ]
    );
}

#[test]
fn a_nested_target_admits_only_its_own_scope_and_only_once() {
    let workspace = Workspace::new();
    workspace.write("AGENTS.md", "PROJECT RULE\n");
    workspace.write("src/AGENTS.md", "SRC RULE\n");
    workspace.write("src/deep/AGENTS.md", "DEEP RULE\n");
    workspace.write("other/AGENTS.md", "OTHER RULE\n");
    workspace.write("src/deep/main.rs", "fn main() {}\n");

    let scope = workspace_scope(&workspace);
    let mut context = ProjectContext::discover(&scope);
    assert!(!context.render().contains("SRC RULE"));

    let delta = context
        .admit_target(&workspace.path().join("src/deep/main.rs"))
        .expect("a nested target contributes its scopes");
    assert!(delta.contains("SRC RULE"), "{delta}");
    assert!(delta.contains("DEEP RULE"), "{delta}");
    assert!(!delta.contains("OTHER RULE"), "{delta}");
    // The narrowest applicable scope is rendered last here too.
    assert!(delta.find("SRC RULE") < delta.find("DEEP RULE"), "{delta}");
    assert!(
        delta.contains("<scoped-rules from="),
        "a nested rule is labelled as scoped: {delta}"
    );

    // A second target under the same scopes delivers nothing new.
    assert_eq!(
        context.admit_target(&workspace.path().join("src/deep/other.rs")),
        None,
        "a source is delivered once"
    );
}

#[test]
fn context_never_reads_an_additional_root_or_a_claude_file() {
    let workspace = Workspace::new();
    let extra = Workspace::new();
    workspace.write("CLAUDE.md", "CLAUDE RULE\n");
    extra.write("AGENTS.md", "ADDED ROOT RULE\n");

    let scope = AccessScope::new(workspace.path(), [extra.path()]).expect("scope");
    let mut context = ProjectContext::discover(&scope);
    let rendered = context.render();
    assert!(
        !rendered.contains("CLAUDE RULE"),
        "fxr does not claim to read CLAUDE.md: {rendered}"
    );
    assert!(!rendered.contains("CLAUDE.md"), "{rendered}");
    assert!(!rendered.contains("ADDED ROOT RULE"), "{rendered}");

    // Nor through a target that points into the additional root.
    assert_eq!(context.admit_target(&extra.path().join("file.rs")), None);
}

#[test]
fn context_is_bounded_by_file_size_and_total_size() {
    let workspace = Workspace::new();
    workspace.write("AGENTS.md", &"a".repeat(64 * 1024 + 1));
    workspace.write("src/AGENTS.md", "SMALL RULE\n");
    workspace.write("src/main.rs", "fn main() {}\n");

    let scope = workspace_scope(&workspace);
    let mut context = ProjectContext::discover(&scope);
    let rendered = context.render();
    assert!(
        !rendered.contains(&"a".repeat(1024)),
        "an oversized rule file is omitted, not clipped into the prompt"
    );
    assert!(
        rendered.contains("reason=\"oversized\""),
        "the omission is disclosed: {rendered}"
    );

    context.admit_target(&workspace.path().join("src/main.rs"));
    assert!(context.total_bytes() <= 256 * 1024, "the total is bounded");
}

// ---------------------------------------------------------------------------
// binary-level acceptance
// ---------------------------------------------------------------------------

struct Sandbox {
    _root: TempDir,
    home: PathBuf,
    workspace: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = TempDir::new().expect("create sandbox root");
        let home = root.path().join("home");
        let workspace = root.path().join("workspace");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&workspace).expect("create workspace");
        Self {
            home: home.canonicalize().expect("canonicalize home"),
            workspace: workspace.canonicalize().expect("canonicalize workspace"),
            _root: root,
        }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.workspace.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parents");
        }
        fs::write(&path, contents).expect("write fixture");
        path
    }

    fn profile_dir(&self) -> PathBuf {
        self.home.join(".fxr")
    }

    fn sessions_dir(&self) -> PathBuf {
        self.profile_dir().join("sessions")
    }

    fn saved_ids(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.sessions_dir()) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        ids.sort();
        ids
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        self.run_in(&self.workspace.clone(), args, env)
    }

    /// Every byte fxr wrote under the profile home, file by file.
    ///
    /// Used for the secret scan: checking only `events.jsonl` would miss a
    /// manifest, a staged temporary, or anything a later change adds.
    fn profile_bytes(&self) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push((path.clone(), fs::read(&path).expect("read")));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.profile_dir(), &mut out);
        out
    }

    fn run_in(&self, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fxr"));
        command.current_dir(cwd);
        command.env("HOME", &self.home);
        for key in CONTROLLED_VARS {
            command.env_remove(key);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        command.args(args);
        Run::of(command.output().expect("spawn fxr"))
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn of(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8(output.stdout).expect("stdout is utf-8"),
            stderr: String::from_utf8(output.stderr).expect("stderr is utf-8"),
        }
    }

    fn json(&self) -> Value {
        assert_eq!(
            self.stdout.matches('\n').count(),
            1,
            "one document, got {:?}",
            self.stdout
        );
        serde_json::from_str(self.stdout.trim_end()).expect("stdout parses as JSON")
    }

    fn assert_no_secret(&self) {
        assert!(!self.stdout.contains(TEST_KEY), "the key reached stdout");
        assert!(!self.stderr.contains(TEST_KEY), "the key reached stderr");
    }
}

fn gateway_env<'a>(gateway: &'a str, key: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("AI_GATEWAY_API_KEY", key), ("FXR_GATEWAY_URL", gateway)]
}

#[test]
fn ask_saves_a_turn_by_default_and_no_save_writes_nothing() {
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["saved answer"]))]);
    let sandbox = Sandbox::new();
    let url = gateway.chat_url();

    let run = sandbox.run(&["ask", "remember this"], &gateway_env(&url, TEST_KEY));
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    let ids = sandbox.saved_ids();
    assert_eq!(ids.len(), 1, "the default records one session: {ids:?}");
    let dir = sandbox.sessions_dir().join(&ids[0]);
    assert!(dir.join(EVENTS_FILE).exists());
    assert!(dir.join(MANIFEST_FILE).exists());
    let log = fs::read_to_string(dir.join(EVENTS_FILE)).expect("read log");
    assert!(log.contains("remember this"), "{log}");
    assert!(log.contains("saved answer"), "{log}");
    // Not one file: everything fxr wrote under the profile home, so a manifest
    // or a staged temporary cannot hold what the log does not.
    let written = sandbox.profile_bytes();
    assert!(!written.is_empty(), "the turn must have written something");
    for (path, bytes) in &written {
        assert!(
            !String::from_utf8_lossy(bytes).contains(TEST_KEY),
            "a credential reached {}",
            path.display()
        );
        for variable in CONTROLLED_VARS {
            assert!(
                !String::from_utf8_lossy(bytes).contains(variable),
                "{} names the credential source {variable}",
                path.display()
            );
        }
    }
    run.assert_no_secret();

    // A second, separate sandbox proves `--no-save` creates nothing at all.
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["unsaved"]))]);
    let sandbox = Sandbox::new();
    let url = gateway.chat_url();
    let run = sandbox.run(
        &["ask", "--no-save", "forget this"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert!(
        !sandbox.profile_dir().exists(),
        "--no-save must not create {}",
        sandbox.profile_dir().display()
    );
}

#[test]
fn ask_resume_replays_durable_history_in_the_next_request() {
    let sandbox = Sandbox::new();

    let first = FakeGateway::start(vec![Reply::Sse(content_only(&["the capital is Seoul"]))]);
    let url = first.chat_url();
    let run = sandbox.run(
        &["ask", "what is the capital"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    drop(first);

    let second = FakeGateway::start(vec![Reply::Sse(content_only(&["about 9.6 million"]))]);
    let url = second.chat_url();
    let run = sandbox.run(
        &["ask", "--resume", "last", "how many people live there"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let body = second.only_request().json();
    let prompt = body["prompt"].as_array().expect("a prompt array");
    let texts: Vec<String> = prompt
        .iter()
        .map(|message| {
            format!(
                "{}:{}",
                message["role"].as_str().unwrap_or(""),
                message["content"]
                    .as_array()
                    .map(|parts| parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<String>())
                    .unwrap_or_else(|| message["content"].as_str().unwrap_or("").to_string())
            )
        })
        .collect();
    assert_eq!(
        texts,
        [
            "user:what is the capital",
            "assistant:the capital is Seoul",
            "user:how many people live there",
        ],
        "durable history precedes the current user message"
    );

    // One session, two turns: resume continued rather than forked.
    assert_eq!(sandbox.saved_ids().len(), 1);
}

#[test]
fn a_request_is_static_context_then_overlay_then_history_then_user_then_suffix() {
    let sandbox = Sandbox::new();
    sandbox.write("AGENTS.md", "ROOT PROJECT RULE\n");
    sandbox.write("src/AGENTS.md", "NESTED SRC RULE\n");
    sandbox.write("src/notes.md", "the note\n");

    let gateway = FakeGateway::start(vec![
        Reply::Sse(sse_body(&[
            tool_call("c1", "read_file", json!({ "path": "src/notes.md" })),
            finish("tool-calls"),
        ])),
        Reply::Sse(sse_body(&[text_delta("a0", "read it"), finish("stop")])),
    ]);
    let url = gateway.chat_url();
    let run = sandbox.run(
        &["ask", "--no-save", "read src/notes.md"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let requests = gateway.requests();
    assert_eq!(requests.len(), 2);

    let first = requests[0].json();
    let first_prompt = first["prompt"].as_array().expect("prompt");
    assert_eq!(first_prompt[0]["role"], "system");
    assert!(
        first_prompt[0]["content"]
            .as_str()
            .expect("a system string")
            .contains("ROOT PROJECT RULE"),
        "static project context leads the request"
    );
    assert_eq!(first_prompt[1]["role"], "user");
    assert_eq!(first_prompt.len(), 2);

    let second = requests[1].json();
    let second_prompt = second["prompt"].as_array().expect("prompt");
    let roles: Vec<&str> = second_prompt
        .iter()
        .map(|message| message["role"].as_str().expect("role"))
        .collect();
    assert_eq!(
        roles,
        ["system", "system", "user", "assistant", "tool"],
        "static, overlay, current user, then the within-turn suffix"
    );
    assert!(second_prompt[0]["content"]
        .as_str()
        .expect("static")
        .contains("ROOT PROJECT RULE"));
    let overlay = second_prompt[1]["content"].as_str().expect("overlay");
    assert!(
        overlay.contains("NESTED SRC RULE"),
        "the nested target's rules arrive as an overlay: {overlay}"
    );
    assert!(
        !second_prompt[0]["content"]
            .as_str()
            .expect("static")
            .contains("NESTED SRC RULE"),
        "the static section is not rewritten mid-turn"
    );
}

#[test]
fn a_resumed_turn_rediscovers_project_context_rather_than_replaying_it() {
    let sandbox = Sandbox::new();
    sandbox.write("AGENTS.md", "FIRST RULE\n");

    let first = FakeGateway::start(vec![Reply::Sse(content_only(&["ok"]))]);
    let url = first.chat_url();
    assert_eq!(
        sandbox
            .run(&["ask", "hello"], &gateway_env(&url, TEST_KEY))
            .code,
        Some(0)
    );
    drop(first);

    // The project's instructions change between the two turns.
    sandbox.write("AGENTS.md", "SECOND RULE\n");

    let second = FakeGateway::start(vec![Reply::Sse(content_only(&["ok again"]))]);
    let url = second.chat_url();
    let run = sandbox.run(
        &["ask", "--resume", "last", "again"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let body = second.only_request().json();
    let system = body["prompt"][0]["content"]
        .as_str()
        .expect("a system string");
    assert!(system.contains("SECOND RULE"), "{system}");
    assert!(
        !system.contains("FIRST RULE"),
        "stale persisted context must not outrank the current file: {system}"
    );
}

#[test]
fn sessions_and_session_report_the_same_facts_as_text_and_json() {
    let sandbox = Sandbox::new();
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["an answer"]))]);
    let url = gateway.chat_url();
    assert_eq!(
        sandbox
            .run(&["ask", "a question"], &gateway_env(&url, TEST_KEY))
            .code,
        Some(0)
    );
    let session_id = sandbox.saved_ids().into_iter().next().expect("one session");

    let listed = sandbox.run(&["sessions", "--json"], &[]);
    assert_eq!(listed.code, Some(0), "stderr={:?}", listed.stderr);
    let json = listed.json();
    assert_eq!(json["kind"], "sessions");
    assert_eq!(json["scope"], "workspace");
    assert_eq!(json["count"], 1);
    assert_eq!(json["sessions"][0]["id"], session_id);
    assert_eq!(json["sessions"][0]["history_turns"], 1);
    assert_eq!(json["sessions"][0]["title"], "a question");

    let text = sandbox.run(&["sessions"], &[]);
    assert_eq!(text.code, Some(0));
    assert!(
        text.stdout.contains("[sessions] scope=workspace count=1"),
        "{}",
        text.stdout
    );
    assert!(
        text.stdout.contains(&format!("[session] id={session_id}")),
        "{}",
        text.stdout
    );

    let detail = sandbox.run(&["session", "last", "--json"], &[]);
    assert_eq!(detail.code, Some(0), "stderr={:?}", detail.stderr);
    let json = detail.json();
    assert_eq!(json["kind"], "session");
    assert_eq!(json["id"], session_id);
    assert_eq!(json["history_turns"], 1);
    assert_eq!(json["turns"][0]["user"], "a question");
    assert_eq!(json["turns"][0]["outcome"]["kind"], "final");

    let by_id = sandbox.run(&["session", "--id", &session_id, "--json"], &[]);
    assert_eq!(by_id.code, Some(0), "stderr={:?}", by_id.stderr);
    assert_eq!(by_id.json()["id"], session_id);

    // Deterministic: the same read twice is byte-identical.
    let again = sandbox.run(&["sessions", "--json"], &[]);
    assert_eq!(again.stdout, listed.stdout);
}

#[test]
fn read_only_session_commands_create_no_profile_state() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["sessions"],
        vec!["sessions", "--json"],
        vec!["sessions", "--all"],
        vec!["status"],
        vec!["doctor"],
    ] {
        let run = sandbox.run(&args, &[]);
        assert_eq!(run.code, Some(0), "{args:?} stderr={:?}", run.stderr);
        assert!(
            !sandbox.profile_dir().exists(),
            "{args:?} must not create {}",
            sandbox.profile_dir().display()
        );
    }
    // Asking for a session that cannot exist is a named failure, not a creation.
    let run = sandbox.run(&["session", "last"], &[]);
    assert_eq!(run.code, Some(1));
    assert!(!run.stderr.is_empty());
    assert!(!sandbox.profile_dir().exists());
}

#[test]
fn status_reports_real_history_and_grant_counts_once_a_session_exists() {
    let sandbox = Sandbox::new();
    let before = sandbox.run(&["status", "--json"], &[]).json();
    assert_eq!(before["history_turns"], 0);
    assert_eq!(before["session_permission_grants"], 0);

    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["one"]))]);
    let url = gateway.chat_url();
    assert_eq!(
        sandbox
            .run(&["ask", "first question"], &gateway_env(&url, TEST_KEY))
            .code,
        Some(0)
    );

    let after = sandbox.run(&["status", "--json"], &[]).json();
    assert_eq!(after["history_turns"], 1, "{after}");
    assert_eq!(after["session_permission_grants"], 0);
}

#[test]
fn a_turn_that_could_not_start_records_no_empty_session() {
    // A machine with no credential must not accumulate one empty session
    // directory per attempt: the checks that cost nothing come first.
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "hello"], &[]);
    assert_eq!(run.code, Some(1));
    assert!(run.stderr.contains("VERCEL_OIDC_TOKEN"), "{}", run.stderr);
    assert!(
        !sandbox.profile_dir().exists(),
        "a turn that never started must not create {}",
        sandbox.profile_dir().display()
    );
}

#[test]
fn resume_and_no_save_cannot_be_asked_for_together() {
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["ask", "--resume", "last", "--no-save", "hi"], &[]);
    assert_eq!(run.code, Some(1));
    assert!(run.stdout.is_empty());
    assert!(!run.stderr.is_empty());
}

#[test]
fn an_unknown_session_is_a_named_refusal_rather_than_a_new_session() {
    let sandbox = Sandbox::new();
    let gateway = FakeGateway::start(vec![Reply::Sse(content_only(&["unreached"]))]);
    let url = gateway.chat_url();
    let run = sandbox.run(
        &["ask", "--resume-id", "no-such-session", "hi"],
        &gateway_env(&url, TEST_KEY),
    );
    assert_eq!(run.code, Some(1));
    assert!(
        run.stderr.contains("no-such-session"),
        "stderr={:?}",
        run.stderr
    );
    assert_eq!(gateway.request_count(), 0, "nothing was asked of the model");
    assert!(sandbox.saved_ids().is_empty());
}

// ---------------------------------------------------------------------------
// helpers that damage a log on purpose
// ---------------------------------------------------------------------------

/// Writes a two-event session, rewrites its second line, and republishes the
/// manifest so the only thing wrong is the thing under test.
fn corrupt_log(profile: &Profile, workspace: &Path, name: &str, damage: impl Fn(&str) -> String) {
    let store = profile.store();
    let mut session = store
        .create(id(name), new_session(workspace))
        .expect("create");
    store
        .append(
            &mut session,
            SessionEvent::UserMessage {
                text: "one".to_string(),
            },
        )
        .expect("append");
    store.publish(&mut session).expect("publish");
    drop(session);

    let log = profile.sessions_dir().join(name).join(EVENTS_FILE);
    let text = fs::read_to_string(&log).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[1] = damage(&lines[1]);
    republish(profile, name, &lines);
}

/// Rewrites the log from `lines` and repairs the manifest's boundary and digest,
/// so a reader's only reason to refuse is the damaged content itself.
fn republish(profile: &Profile, name: &str, lines: &[String]) {
    use sha2::{Digest, Sha256};

    let dir = profile.sessions_dir().join(name);
    let mut body = String::new();
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }
    fs::write(dir.join(EVENTS_FILE), &body).expect("write the log");

    let path = dir.join(MANIFEST_FILE);
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("json");
    manifest["event_log_bytes"] = json!(body.len());
    manifest["event_log_sha256"] = json!(format!("{:x}", Sha256::digest(body.as_bytes())));
    fs::write(&path, format!("{manifest}\n")).expect("write the manifest");
}

fn event_field(line: &str, key: &str) -> String {
    let value: Value = serde_json::from_str(line).expect("an event frame");
    value[key].as_str().expect("a string field").to_string()
}

fn replace_field(line: &str, key: &str, value: &str) -> String {
    let mut parsed: Value = serde_json::from_str(line).expect("an event frame");
    parsed[key] = json!(value);
    serde_json::to_string(&parsed).expect("re-encode")
}
