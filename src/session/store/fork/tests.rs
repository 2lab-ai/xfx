//! What a fork holds, what it leaves alone, and what it refuses.
//!
//! The receipts of the store's half of `/fork` (P5-FK1): upstream's `openFork`
//! and `Session.fork` (`vercel-labs/fx@481104fd
//! src/core/session_manager/session.zig:1714-1868`,
//! `src/core/session/session_adapter.zig:2436-2457`, `:5096-5100`), as xfx's
//! log is shaped.

use std::collections::BTreeMap;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use super::super::{
    apply, DurableState, EventEnvelope, ListFilter, ListScope, NewSession, SessionError,
    SessionEvent, SessionId, SessionRecorder, SessionStore, Snapshot, TurnStep, WritableSession,
    EVENTS_FILE, IMAGES_DIR, MANIFEST_FILE, MAX_RENAMED_TITLE_BYTES,
};
use super::{carry_image, FORK_STAGE_DIR};
use crate::config::PermissionMode;
use crate::gateway::protocol::{ContentPart, Message};
use crate::provider::Wire;
use crate::session::event::{
    ForkOrigin, RecordedToolCall, RequestUsage, TitleSource, TurnConclusion,
};
use crate::session::{Selector, EVENT_SCHEMA_VERSION};

// ---------------------------------------------------------------------------
// a hand in the stage
// ---------------------------------------------------------------------------

std::thread_local! {
    /// What [`before_rules`] does on this test's thread, and the stage it was
    /// handed. Per thread, so a test that sets it touches no other test's fork.
    static BEFORE_RULES: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
    static STAGE_SEEN: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
    /// What [`before_stage`] does on this test's thread.
    static BEFORE_STAGE: std::cell::Cell<Option<fn(&Path)>> = const { std::cell::Cell::new(None) };
}

/// Called by `fork` with the profile home once the source is read, just
/// before the stage root is made.
pub(super) fn before_stage(profile: &Path) {
    if let Some(hand) = BEFORE_STAGE.with(std::cell::Cell::get) {
        hand(profile);
    }
}

/// Called by `fork` with its stage, just before it writes the fork's
/// saved-session rules there: the moment another hand could put something
/// at the store's path.
pub(super) fn before_rules(stage: &Path) {
    STAGE_SEEN.with(|seen| *seen.borrow_mut() = Some(stage.to_path_buf()));
    if let Some(hand) = BEFORE_RULES.with(std::cell::Cell::get) {
        hand(stage);
    }
}

// ---------------------------------------------------------------------------
// a bench
// ---------------------------------------------------------------------------

struct Bench {
    root: tempfile::TempDir,
    store: SessionStore,
    workspace: PathBuf,
}

fn bench() -> Bench {
    let root = tempfile::TempDir::new().expect("a root");
    // A directory the store creates, so it is private; the temporary root
    // around it is not.
    let store = SessionStore::open(&root.path().join("profile")).expect("a store");
    let workspace = root.path().join("work");
    fs::create_dir_all(&workspace).expect("a workspace");
    Bench {
        root,
        store,
        workspace,
    }
}

impl Bench {
    fn open(&self, workspace: &Path) -> (SessionRecorder, SessionId) {
        let id = SessionId::generate();
        let session = self
            .store
            .create(
                id.clone(),
                NewSession {
                    origin_workspace_root: workspace.to_path_buf(),
                    workspace_root: workspace.to_path_buf(),
                    model: "m".to_string(),
                    permission_mode: PermissionMode::Auto,
                    provider: None,
                },
            )
            .expect("a session");
        (SessionRecorder::new(self.store.clone(), session), id)
    }

    fn fork(
        &self,
        source: &SessionId,
        before_turn: usize,
    ) -> Result<WritableSession, SessionError> {
        self.store.fork(source, before_turn, &self.workspace)
    }

    /// The session as a reader replays it, cross-checked against its
    /// manifest -- images unread.
    fn state(&self, id: &SessionId) -> DurableState {
        self.store
            .detail(&Selector::Id(id.clone()), &self.workspace)
            .expect("the session reads whole")
            .state
    }

    fn log(&self, id: &SessionId) -> Vec<EventEnvelope> {
        fs::read_to_string(self.store.session_dir(id).join(EVENTS_FILE))
            .expect("a log")
            .lines()
            .map(|line| EventEnvelope::decode(line).expect("a frame"))
            .collect()
    }

    /// Every name in `sessions`, and every name the forks are built under.
    fn names(&self) -> (Vec<String>, Vec<String>) {
        let names = |dir: &Path| -> Vec<String> {
            let mut names: Vec<String> = match fs::read_dir(dir) {
                Ok(entries) => entries
                    .map(|entry| {
                        entry
                            .expect("an entry")
                            .file_name()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect(),
                // Absent, or a plant that is not a folder: no name in it.
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    Vec::new()
                }
                Err(err) => panic!("cannot read {}: {err}", dir.display()),
            };
            names.sort();
            names
        };
        (
            names(self.store.sessions_dir()),
            names(&self.store.profile_dir().join(FORK_STAGE_DIR)),
        )
    }
}

/// The state the log adds up to through frame `seq`: what the session was
/// when that frame was its last.
fn state_through(store: &SessionStore, id: &SessionId, seq: u64) -> DurableState {
    let log = fs::read_to_string(store.session_dir(id).join(EVENTS_FILE)).expect("a log");
    let mut state = DurableState::empty();
    for line in log.lines().take(seq as usize) {
        apply(&mut state, &EventEnvelope::decode(line).expect("a frame")).expect("it applies");
    }
    state
}

fn user(text: &str) -> SessionEvent {
    SessionEvent::UserMessage {
        text: text.to_string(),
    }
}

fn answer(text: &str, calls: &[(&str, &str)]) -> SessionEvent {
    SessionEvent::AssistantMessage {
        text: text.to_string(),
        tool_calls: calls
            .iter()
            .map(|(id, name)| RecordedToolCall {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({"path": "a.png"}),
            })
            .collect(),
        raw_content: Vec::new(),
        responses_state: Vec::new(),
        wire: None,
        model: None,
    }
}

fn usage(generation: &str) -> SessionEvent {
    SessionEvent::UsageRecorded {
        input_tokens: Some(6),
        output_tokens: Some(40),
        turn_output_tokens: Some(40),
        requests: vec![RequestUsage {
            at_ms: 1_000,
            duration_ms: 7,
            model: "m".to_string(),
            input_tokens: 6,
            output_tokens: 40,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: None,
            cost: Some(0.01),
            generation_id: Some(generation.to_string()),
            origin: None,
        }],
        unaccounted: false,
        lines_added: 0,
        lines_removed: 0,
        settled: Vec::new(),
        rejected: Vec::new(),
    }
}

fn concluded() -> SessionEvent {
    SessionEvent::TurnConcluded {
        outcome: TurnConclusion::Final {
            finish_reason: "stop".to_string(),
            steps: 1,
        },
    }
}

fn renamed(title: &str) -> SessionEvent {
    SessionEvent::SessionRenamed {
        title: title.to_string(),
        source: TitleSource::User,
    }
}

/// One answered turn.
fn turn(recorder: &mut SessionRecorder, prompt: &str, reply: &str) {
    recorder.commit(user(prompt));
    recorder.commit(answer(reply, &[]));
    recorder.commit(usage(&format!("gen_{prompt}")));
    recorder.commit(concluded());
    assert_eq!(recorder.failure(), None);
}

fn shot(id: u32) -> crate::image::Image {
    crate::image::Image::new(
        id,
        PathBuf::from("/w/shot.png"),
        crate::image::MediaType::Png,
        Arc::from(crate::image::fixture::SHOT_PNG),
    )
}

/// Bytes that are not the shot, so the tool's image has a name of its own.
fn tool_shot() -> crate::image::ToolImage {
    let mut bytes = crate::image::fixture::SHOT_PNG.to_vec();
    bytes.push(0);
    crate::image::ToolImage {
        media_type: crate::image::MediaType::Png,
        data: Arc::from(bytes),
        path: "/w/a.png".to_string(),
    }
}

/// A session with a preference set before its first prompt, a turn carrying
/// an image, a turn whose tool returned one and that recorded a grant, a
/// model change between turns two and three, and a third turn.
fn rich(bench: &Bench) -> (SessionRecorder, SessionId) {
    let (mut recorder, id) = bench.open(&bench.workspace);
    recorder.commit(SessionEvent::PreferencesChanged {
        model: None,
        permission_mode: Some("ask".to_string()),
        provider: None,
        effort: None,
        fast_mode: None,
        ultrafast_mode: None,
    });
    recorder.commit(user("[Image #1] first prompt alpha"));
    recorder.attach_images(&[shot(1)]);
    recorder.commit(answer("ALPHA answer.", &[]));
    recorder.commit(usage("gen_1"));
    recorder.commit(concluded());

    recorder.commit(user("second prompt beta"));
    recorder.commit(answer("", &[("c1", "read_file")]));
    recorder.commit(SessionEvent::ToolResult {
        call_id: "c1".to_string(),
        tool: "read_file".to_string(),
        ok: true,
        output: "an image".to_string(),
        ending: None,
        lines: None,
        change: None,
    });
    recorder.attach_tool_images("c1", &[tool_shot()]);
    recorder.commit(SessionEvent::PermissionGrantRecorded {
        tool: "read_file".to_string(),
        target: "/w/a.png".to_string(),
    });
    recorder.commit(answer("BETA answer.", &[]));
    recorder.commit(usage("gen_2"));
    recorder.commit(concluded());

    recorder.commit(SessionEvent::PreferencesChanged {
        model: Some("m2".to_string()),
        permission_mode: None,
        provider: None,
        effort: None,
        fast_mode: None,
        ultrafast_mode: None,
    });
    turn(&mut recorder, "third prompt gamma", "GAMMA answer.");
    assert_eq!(recorder.failure(), None);
    (recorder, id)
}

/// What is on disk under a directory: every file's bytes and every entry's
/// modification time and mode, by relative path.
fn tree(root: &Path) -> BTreeMap<PathBuf, (Option<Vec<u8>>, SystemTime, u32)> {
    let mut out = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    out.insert(PathBuf::new(), {
        let meta = fs::symlink_metadata(root).expect("stat the root");
        (None, meta.modified().expect("an mtime"), mode(&meta))
    });
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
                (bytes, meta.modified().expect("an mtime"), mode(&meta)),
            );
        }
    }
    out
}

#[cfg(unix)]
fn mode(meta: &fs::Metadata) -> u32 {
    meta.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn mode(_meta: &fs::Metadata) -> u32 {
    0
}

fn images_in(message: &Message) -> Vec<Option<PathBuf>> {
    message
        .content
        .iter()
        .filter_map(|part| match part {
            ContentPart::Image { saved, .. } => Some(saved.as_deref().map(Path::to_path_buf)),
            _ => None,
        })
        .collect()
}

fn refused(result: Result<WritableSession, SessionError>) -> String {
    match result {
        Err(err @ SessionError::InvalidForkPoint { .. }) => err.to_string(),
        Err(other) => panic!("refused, but not as a fork point: {other}"),
        Ok(session) => panic!("forked {} where it should refuse", session.id()),
    }
}

// ---------------------------------------------------------------------------
// what a fork holds
// ---------------------------------------------------------------------------

#[test]
fn a_fork_replays_as_the_source_did_before_the_prompt_it_was_made_at() {
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let whole = bench.state(&source);
    // S: the frame that ended turn 2 (`findForkPoint`, `session.zig:1780-1830`).
    let through = whole.turns[1].seqs.last;

    let fork = bench.fork(&source, 3).expect("forked before turn 3");
    let id = fork.id().clone();
    assert_ne!(id, source);

    // Turns 1-2 replay as the source's did, step for step, frame numbers and
    // times included; everything else is what the source was at frame S.
    let state = bench.state(&id);
    let at = state_through(&bench.store, &source, through);
    assert_eq!(state.turns, whole.turns[..2]);
    assert_eq!(state.turns, at.turns);
    assert_eq!(state.grants, at.grants);
    assert_eq!(state.usage, at.usage);
    assert_eq!(
        (state.total_input_tokens, state.total_output_tokens),
        (at.total_input_tokens, at.total_output_tokens)
    );
    assert_eq!(state.permission_mode, PermissionMode::Ask);
    assert_eq!(state.provider, at.provider);
    assert_eq!(state.context_sources, at.context_sources);
    // The model change written after S is not the fork's: upstream copies
    // lines 2..S and nothing after (research §4, "A `set` the source wrote
    // after S is not copied").
    assert_eq!(state.model, "m");
    assert_eq!(whole.model, "m2");
    assert_eq!(
        (
            state.origin_workspace_root.as_str(),
            state.workspace_root.as_str()
        ),
        (
            at.origin_workspace_root.as_str(),
            at.workspace_root.as_str()
        )
    );
    // Its title is the source's, marked; it is the next frame after S.
    assert_eq!(
        state.title().as_deref(),
        Some("[Image #1] first prompt alpha (fork)")
    );
    assert_eq!(state.last_event_seq, through + 1);

    // On disk: line 1 is its own creation, naming where it came from; lines
    // 2..S are the source's events at the source's numbers and times, framed
    // in the fork's own generation with ids of its own.
    let theirs = bench.log(&source);
    let ours = bench.log(&id);
    assert_eq!(ours.len() as u64, through + 1);
    match &ours[0].event {
        SessionEvent::SessionStarted {
            id: named,
            forked_from,
            model,
            permission_mode,
            ..
        } => {
            assert_eq!(named, id.as_str());
            assert_eq!(
                forked_from.as_ref(),
                Some(&ForkOrigin {
                    id: source.as_str().to_string(),
                    seq: through,
                })
            );
            assert_eq!((model.as_str(), permission_mode.as_str()), ("m", "auto"));
        }
        other => panic!("line 1 is {}", other.kind()),
    }
    let generation = &ours[0].log_generation;
    assert_ne!(generation, &theirs[0].log_generation);
    for (fork_frame, source_frame) in ours[1..through as usize]
        .iter()
        .zip(&theirs[1..through as usize])
    {
        assert_eq!(fork_frame.seq, source_frame.seq);
        assert_eq!(fork_frame.timestamp_ms, source_frame.timestamp_ms);
        assert_eq!(fork_frame.event, source_frame.event);
        assert_eq!(&fork_frame.log_generation, generation);
        assert_ne!(fork_frame.event_id, source_frame.event_id);
        assert_eq!(fork_frame.schema_version, EVENT_SCHEMA_VERSION);
    }
    assert_eq!(
        ours[through as usize].event,
        renamed("[Image #1] first prompt alpha (fork)")
    );

    // The writer it hands back is the one a resume would: its images read
    // back from the fork's own copies, so the next request sends them.
    let live = fork.state();
    assert_eq!(
        live.turns,
        state
            .turns
            .iter()
            .map(|turn| {
                let mut turn = turn.clone();
                for image in &mut turn.images {
                    image.snapshot = live_snapshot(&bench.store, &id, image);
                }
                for step in &mut turn.steps {
                    if let TurnStep::ToolResult { images, .. } = step {
                        for image in images {
                            image.snapshot = live_snapshot(&bench.store, &id, image);
                        }
                    }
                }
                turn
            })
            .collect::<Vec<_>>()
    );
    let messages = live
        .history_messages(Wire::VercelGateway, &live.model)
        .messages;
    let folder = bench.store.session_dir(&id).join(IMAGES_DIR);
    assert_eq!(
        images_in(&messages[0]),
        vec![Some(folder.join(shot(1).snapshot_name()))]
    );

    // Listed beside its source, under its own title, with its own count.
    let listed = bench
        .store
        .list(&ListFilter::new(ListScope::AllWorkspaces))
        .expect("a listing");
    assert_eq!(listed.skipped_invalid, 0);
    assert_eq!(listed.sessions.len(), 2);
    let row = listed
        .sessions
        .iter()
        .find(|row| row.id == id.as_str())
        .expect("the fork is listed");
    assert_eq!(
        row.title.as_deref(),
        Some("[Image #1] first prompt alpha (fork)")
    );
    assert_eq!(row.history_turns, 2);
}

fn live_snapshot(
    store: &SessionStore,
    id: &SessionId,
    image: &super::super::TurnImage,
) -> Snapshot {
    let path = store
        .session_dir(id)
        .join(IMAGES_DIR)
        .join(crate::image::snapshot_name(
            image.recorded.id,
            &image.recorded.sha256,
        ));
    Snapshot::Loaded {
        bytes: Arc::from(fs::read(&path).expect("the fork's copy")),
        path: Arc::from(path.as_path()),
    }
}

#[test]
fn a_pasted_image_is_named_by_the_forks_own_copy() {
    // A clipboard image is recorded by the copy its session keeps
    // (`SessionRecorder::attach_images`), and a resumed card links that
    // path; a fork's must be its own, so the fork stands when the source is
    // gone (D6, `session.zig:1712-1713`).
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    recorder.commit(user("[Image #1] pasted"));
    let mut pasted = shot(1);
    pasted.copied = true;
    recorder.attach_images(&[pasted.clone()]);
    recorder.commit(concluded());
    turn(&mut recorder, "second", "TWO");
    drop(recorder);

    let fork = bench.fork(&source, 2).expect("forked");
    let id = fork.id().clone();
    drop(fork);
    let named = |id: &SessionId| bench.state(id).turns[0].images[0].recorded.path.clone();
    let own = bench
        .store
        .session_dir(&id)
        .join(IMAGES_DIR)
        .join(pasted.snapshot_name());
    assert_eq!(named(&id), own.display().to_string());
    assert_ne!(named(&source), named(&id));

    // The source goes; the fork still reads, image included.
    fs::remove_dir_all(bench.store.session_dir(&source)).expect("remove the source");
    let resumed = bench
        .store
        .resume(&Selector::Id(id.clone()), &bench.workspace)
        .expect("the fork resumes alone");
    assert!(matches!(
        resumed.session.state().turns[0].images[0].snapshot,
        Snapshot::Loaded { .. }
    ));
}

#[test]
fn forking_never_writes_the_source_even_while_it_is_open() {
    let bench = bench();
    // Held open for writing, as the TUI holds the session it forks.
    let (mut recorder, source) = rich(&bench);
    let dir = bench.store.session_dir(&source);
    let before = tree(&dir);

    for before_turn in 1..=4 {
        let fork = bench.fork(&source, before_turn).expect("forked");
        drop(fork);
    }
    assert_eq!(tree(&dir), before, "the fork wrote to its source");

    // And its writer goes on: nothing it owns moved under it.
    turn(&mut recorder, "fourth prompt delta", "DELTA answer.");
    drop(recorder);
    assert_eq!(bench.state(&source).turns.len(), 4);
}

#[test]
fn a_checkpoint_the_fork_keeps_still_validates() {
    use crate::session::checkpoint::{Compacted, CompactedTurn};
    let compacted = Compacted {
        entries: Vec::new(),
        turns: vec![CompactedTurn {
            number: 1,
            users: vec!["one".to_string()],
            work: String::new(),
            final_reply: "ONE".to_string(),
            first_tool: 0,
            last_tool: 0,
            tools: Vec::new(),
        }],
        open: None,
        turn_count: 1,
        tool_count: 0,
    };
    let checkpoint = |seq: u64| SessionEvent::ContextCheckpoint {
        covers_through_seq: seq,
        compacted: compacted.clone(),
    };
    let bench = bench();

    // Between turns 1 and 2, and inside turn 2 (a compaction between its
    // steps): both before S of a fork before turn 3.
    for inside in [false, true] {
        let (mut recorder, source) = bench.open(&bench.workspace);
        turn(&mut recorder, "one", "ONE");
        let first = recorder.state().turns[0].seqs.last;
        if inside {
            recorder.commit(user("two"));
            recorder.commit(checkpoint(first));
            recorder.commit(answer("TWO", &[]));
            recorder.commit(concluded());
        } else {
            recorder.commit(checkpoint(first));
            turn(&mut recorder, "two", "TWO");
        }
        turn(&mut recorder, "three", "THREE");
        assert_eq!(recorder.failure(), None);
        drop(recorder);

        let through = bench.state(&source).turns[1].seqs.last;
        let fork = bench.fork(&source, 3).expect("forked");
        let id = fork.id().clone();
        drop(fork);
        // Replayed and cross-checked: the checkpoint's coverage is a frame
        // number, and the fork keeps every one.
        let state = bench.state(&id);
        let at = state_through(&bench.store, &source, through);
        assert!(state.checkpoint.is_some(), "inside={inside}");
        assert_eq!(state.checkpoint, at.checkpoint, "inside={inside}");
        assert_eq!(
            state.history_messages(Wire::VercelGateway, "m").messages,
            at.history_messages(Wire::VercelGateway, "m").messages,
            "inside={inside}"
        );
        assert_eq!(
            state.history_messages(Wire::VercelGateway, "m").messages[0],
            Message::user(crate::session::checkpoint::render(&compacted))
        );
        // And it goes on from there.
        let mut resumed = bench
            .store
            .resume(&Selector::Id(id), &bench.workspace)
            .expect("resumed")
            .session;
        bench
            .store
            .append(&mut resumed, user("four"))
            .expect("appended");
        bench.store.publish(&mut resumed).expect("published");
    }

    // One written after S is not the fork's, as upstream copies nothing past
    // the turn it keeps.
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    turn(&mut recorder, "two", "TWO");
    let first = recorder.state().turns[0].seqs.last;
    recorder.commit(checkpoint(first));
    turn(&mut recorder, "three", "THREE");
    drop(recorder);
    let fork = bench.fork(&source, 3).expect("forked");
    assert_eq!(bench.state(fork.id()).checkpoint, None);
}

// ---------------------------------------------------------------------------
// its title
// ---------------------------------------------------------------------------

#[test]
fn a_fork_of_a_fork_is_marked_once() {
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "first prompt alpha", "ALPHA");
    turn(&mut recorder, "second prompt beta", "BETA");
    drop(recorder);

    let first = bench.fork(&source, 2).expect("a fork");
    let first_id = first.id().clone();
    drop(first);
    let second = bench.fork(&first_id, 2).expect("a fork of the fork");
    let state = bench.state(second.id());
    assert_eq!(state.title().as_deref(), Some("first prompt alpha (fork)"));
    assert_eq!(state.renamed.as_deref(), Some("first prompt alpha (fork)"));
    match &bench.log(second.id())[0].event {
        SessionEvent::SessionStarted { forked_from, .. } => {
            assert_eq!(
                forked_from.as_ref().map(|o| o.id.as_str()),
                Some(first_id.as_str())
            );
        }
        other => panic!("line 1 is {}", other.kind()),
    }
}

#[test]
fn a_fork_is_titled_by_the_source_as_it_is_called_now_and_fits_the_title_limit() {
    // The title the source shows (`cachedSessionTitle`), so a rename written
    // after S still names the fork.
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "first", "ONE");
    turn(&mut recorder, "second", "TWO");
    recorder.commit(renamed("Named later"));
    drop(recorder);
    let fork = bench.fork(&source, 2).expect("forked");
    assert_eq!(
        bench.state(fork.id()).title().as_deref(),
        Some("Named later (fork)")
    );

    // A title already at the limit keeps the mark and stays inside it, cut
    // where a character ends; one the store would refuse is never written.
    for (long, kept) in [
        ("x".repeat(MAX_RENAMED_TITLE_BYTES), "x".repeat(233)),
        ("é".repeat(MAX_RENAMED_TITLE_BYTES / 2), "é".repeat(116)),
    ] {
        let (mut recorder, source) = bench.open(&bench.workspace);
        turn(&mut recorder, "first", "ONE");
        recorder.commit(renamed(&long));
        assert_eq!(recorder.failure(), None);
        drop(recorder);
        let fork = bench.fork(&source, 2).expect("forked");
        let title = bench.state(fork.id()).title().expect("a title");
        assert_eq!(title, format!("{kept} (fork)"));
        assert!(title.len() <= MAX_RENAMED_TITLE_BYTES, "{}", title.len());
    }
}

// ---------------------------------------------------------------------------
// where a fork can be made
// ---------------------------------------------------------------------------

#[test]
fn a_fork_before_the_first_prompt_is_empty_and_there_is_no_turn_zero() {
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let opened = bench.state(&source).turns[0].seqs.opened;

    // Upstream's `Session.fork` asks for the turn before the one given, and
    // turn 0 is "before the first turn" (`session_adapter.zig:2437-2442`,
    // `session.zig:1787`, `:1827`): a session with no turn, holding what was
    // set before the first prompt, titled by that prompt.
    let empty = bench.fork(&source, 1).expect("forked before turn 1");
    let state = bench.state(empty.id());
    assert!(state.turns.is_empty());
    assert_eq!(state.permission_mode, PermissionMode::Ask);
    assert_eq!(
        state.title().as_deref(),
        Some("[Image #1] first prompt alpha (fork)")
    );
    let kinds: Vec<&str> = bench
        .log(empty.id())
        .iter()
        .map(|frame| frame.event.kind())
        .collect();
    assert_eq!(
        kinds,
        ["session_started", "preferences_changed", "session_renamed"]
    );
    assert_eq!(state.last_event_seq, opened);
    let manifest = empty.manifest().expect("published");
    assert_eq!(manifest.history_turns, 0);
    let empty_id = empty.id().clone();
    drop(empty);
    // Nothing to fork there in turn, but it is a session like any other.
    assert!(bench.fork(&empty_id, 1).is_ok());

    let sessions = bench.names();
    // `Session.fork`'s `InvalidForkPoint` (`session_adapter.zig:2437`).
    assert!(refused(bench.fork(&source, 0)).contains("turn 0"));
    // Past the turns there are: four kept would need a fourth.
    assert!(refused(bench.fork(&source, 5)).contains("3 turn"));
    // Every refusal leaves the store as it was.
    assert_eq!(bench.names(), sessions);

    // Before a turn that is one past the last is every turn, as upstream's
    // `openFork` finds the last turn's end.
    let all = bench.fork(&source, 4).expect("forked after the last turn");
    assert_eq!(bench.state(all.id()).turns.len(), 3);
}

#[test]
fn a_turn_that_has_not_ended_cannot_be_kept() {
    // Upstream's fork point is a turn's `turn_committed` or
    // `turn_interrupted` line; one with neither is not found
    // (`findForkPoint`, `session.zig:1811-1828`).
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    recorder.commit(user("two"));
    recorder.commit(answer("TW", &[]));
    let sessions = bench.names();
    assert!(refused(bench.fork(&source, 3)).contains("turn 2 has not ended"));
    assert_eq!(bench.names(), sessions);
    // Before the turn still running is fine: it keeps turn 1.
    let fork = bench.fork(&source, 2).expect("forked before the open turn");
    assert_eq!(bench.state(fork.id()).turns.len(), 1);

    // An interrupted turn ended, as upstream's `turn_interrupted` does.
    recorder.commit(SessionEvent::TurnConcluded {
        outcome: TurnConclusion::Interrupted {
            reason: "cancelled".to_string(),
            terminal_reason: Some(crate::session::TerminalReason::Cancelled),
            cancellation_origin: Default::default(),
        },
    });
    assert_eq!(recorder.failure(), None);
    let fork = bench
        .fork(&source, 3)
        .expect("forked after an interrupted turn");
    assert_eq!(bench.state(fork.id()).turns.len(), 2);
}

#[test]
fn a_fork_belongs_to_the_workspace_it_is_made_in() {
    // Upstream opens the fork in the caller's workspace (`ForkOptions`,
    // `session.zig:1701-1706`); xfx keeps the source's roots on line 1 --
    // the copied rebinds check them -- and rebinds after, as a resume does.
    let bench = bench();
    let elsewhere = bench.root.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("a second workspace");
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    turn(&mut recorder, "two", "TWO");
    drop(recorder);
    let resumed = bench
        .store
        .resume(&Selector::Id(source.clone()), &elsewhere)
        .expect("resumed elsewhere");
    assert!(resumed.rebound);
    let mut recorder = SessionRecorder::new(bench.store.clone(), resumed.session);
    turn(&mut recorder, "three", "THREE");
    drop(recorder);

    let fork = bench
        .store
        .fork(&source, 3, &elsewhere)
        .expect("forked elsewhere");
    let state = bench.state(fork.id());
    let key = |path: &Path| crate::session::workspace_key(path);
    assert_eq!(state.workspace_root, key(&elsewhere));
    assert_eq!(state.origin_workspace_root, key(&bench.workspace));
    let kinds: Vec<&str> = bench
        .log(fork.id())
        .iter()
        .rev()
        .take(2)
        .map(|frame| frame.event.kind())
        .collect();
    assert_eq!(kinds, ["workspace_rebound", "session_renamed"]);
    let here = bench
        .store
        .list(&ListFilter::new(ListScope::CurrentWorkspace(
            elsewhere.clone(),
        )))
        .expect("a listing");
    assert!(here.sessions.iter().any(|row| row.id == fork.id().as_str()));

    // Made where its turns were, it needs no rebind.
    let home = bench.fork(&source, 3).expect("forked at home");
    let last = bench.log(home.id()).last().expect("a frame").event.kind();
    assert_eq!(last, "session_renamed");
}

// ---------------------------------------------------------------------------
// files: private, inside the store, and never half there
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_fork_is_private_and_links_its_images_from_inside_the_store() {
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let fork = bench.fork(&source, 3).expect("forked");
    let dir = bench.store.session_dir(fork.id());
    let mode_of = |path: &Path| mode(&fs::symlink_metadata(path).expect("stat"));
    assert_eq!(mode_of(&dir), 0o700);
    assert_eq!(mode_of(&dir.join(EVENTS_FILE)), 0o600);
    assert_eq!(mode_of(&dir.join(MANIFEST_FILE)), 0o600);
    assert_eq!(mode_of(&dir.join(IMAGES_DIR)), 0o700);
    let theirs = bench.store.session_dir(&source).join(IMAGES_DIR);
    let names = [
        shot(1).snapshot_name(),
        crate::image::snapshot_name(1, &crate::image::digest(&tool_shot().data)),
    ];
    let mut listed: Vec<String> = fs::read_dir(dir.join(IMAGES_DIR))
        .expect("the fork's images")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    listed.sort();
    let mut expected = names.to_vec();
    expected.sort();
    assert_eq!(listed, expected, "only the images the kept turns name");
    for name in &names {
        let ours = fs::symlink_metadata(dir.join(IMAGES_DIR).join(name)).expect("a copy");
        let source_copy = fs::symlink_metadata(theirs.join(name)).expect("the source's");
        assert_eq!(mode(&ours), 0o600);
        assert!(ours.is_file());
        // A hard link: the same file, in the fork's own folder (D6).
        assert_eq!(
            (ours.dev(), ours.ino()),
            (source_copy.dev(), source_copy.ino())
        );
    }
    // The stage is the store's, private, and empty once the fork is in place.
    let stage = bench.store.profile_dir().join(FORK_STAGE_DIR);
    assert_eq!(mode_of(&stage), 0o700);
    assert_eq!(bench.names().1, Vec::<String>::new());

    // A fork before the turn that carried them takes none.
    let bare = bench.fork(&source, 1).expect("forked before turn 1");
    assert!(!bench.store.session_dir(bare.id()).join(IMAGES_DIR).exists());
}

#[cfg(unix)]
#[test]
fn an_image_that_will_not_link_or_is_not_private_is_copied_privately() {
    let bench = bench();
    let folder = bench.root.path().join("images");
    fs::create_dir(&folder).expect("a folder");
    let from = folder.join("from.bin");
    fs::write(&from, crate::image::fixture::SHOT_PNG).expect("an image");
    fs::set_permissions(&from, fs::Permissions::from_mode(0o600)).expect("private");
    let sha = crate::image::digest(crate::image::fixture::SHOT_PNG);
    let size = crate::image::fixture::SHOT_PNG.len() as u64;

    // A link the filesystem refuses -- across devices, say -- is a copy.
    let to = folder.join("refused.bin");
    carry_image(&from, &to, &sha, size, |_, _| {
        Err(io::Error::from(io::ErrorKind::CrossesDevices))
    })
    .expect("copied");
    let copy = fs::symlink_metadata(&to).expect("a copy");
    assert_eq!(
        fs::read(&to).expect("read"),
        crate::image::fixture::SHOT_PNG
    );
    assert_eq!(mode(&copy), 0o600);
    assert_ne!(copy.ino(), fs::metadata(&from).expect("stat").ino());

    // A source copy other accounts can read would lend the fork its mode
    // through a link, so it is copied instead -- and left as it was.
    fs::set_permissions(&from, fs::Permissions::from_mode(0o644)).expect("open it up");
    let to = folder.join("open.bin");
    carry_image(&from, &to, &sha, size, |a, b| fs::hard_link(a, b)).expect("copied");
    let copy = fs::symlink_metadata(&to).expect("a copy");
    assert_eq!(mode(&copy), 0o600);
    assert_ne!(copy.ino(), fs::metadata(&from).expect("stat").ino());
    assert_eq!(mode(&fs::metadata(&from).expect("stat")), 0o644);

    // Bytes that are not the ones recorded are refused, link or not.
    let wrong = "0".repeat(64);
    assert!(
        carry_image(&from, &folder.join("wrong.bin"), &wrong, size, |a, b| {
            fs::hard_link(a, b)
        })
        .is_err()
    );
    assert!(!folder.join("wrong.bin").exists());
}

#[cfg(unix)]
#[test]
fn a_fork_whose_source_image_is_gone_or_not_its_own_is_refused_and_leaves_nothing() {
    let bench = bench();
    let outside = bench.root.path().join("outside");
    fs::create_dir(&outside).expect("a directory outside the store");
    let planted = outside.join("shot.bin");
    fs::write(&planted, crate::image::fixture::SHOT_PNG).expect("the same bytes, outside");

    type Damage = fn(&Path, &Path);
    let damages: [(&str, Damage); 3] = [
        ("missing", |images, _| {
            fs::remove_file(images.join(shot(1).snapshot_name())).expect("remove");
        }),
        // A link out of the store, to a file with the very bytes recorded.
        ("a symlink", |images, planted| {
            let copy = images.join(shot(1).snapshot_name());
            fs::remove_file(&copy).expect("remove");
            std::os::unix::fs::symlink(planted, &copy).expect("plant a link");
        }),
        // The whole folder, swapped for a link out of the store.
        ("a linked folder", |images, planted| {
            let parent = planted.parent().expect("a parent");
            let moved = parent.join("moved");
            fs::rename(images, &moved).expect("move the folder out");
            std::os::unix::fs::symlink(&moved, images).expect("plant a link");
        }),
    ];
    for (what, damage) in damages {
        let (recorder, source) = rich(&bench);
        drop(recorder);
        damage(&bench.store.session_dir(&source).join(IMAGES_DIR), &planted);
        let before = bench.names();
        match bench.fork(&source, 3) {
            Err(SessionError::Corrupt { id, detail }) => {
                assert_eq!(id, source.as_str(), "{what}");
                assert!(detail.contains("image"), "{what}: {detail}");
            }
            Err(other) => panic!("{what}: refused as {other:?}"),
            Ok(session) => panic!("{what}: forked {}", session.id()),
        }
        // Fail closed: no session, no stage, nothing half there.
        assert_eq!(bench.names(), before, "{what}");
        assert_eq!(bench.names().1, Vec::<String>::new(), "{what}");
        // A fork before the turn that names it is untouched by it.
        assert!(bench.fork(&source, 1).is_ok(), "{what}");
        let _ = fs::remove_file(outside.join("moved"));
        let _ = fs::remove_dir_all(outside.join("moved"));
    }
}

#[test]
fn a_fork_holds_its_own_lock_until_it_is_dropped() {
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    turn(&mut recorder, "two", "TWO");
    drop(recorder);

    let mut fork = bench.fork(&source, 2).expect("forked");
    let id = fork.id().clone();
    match bench
        .store
        .resume_without_waiting(&Selector::Id(id.clone()), &bench.workspace)
    {
        Err(SessionError::Busy { id: busy }) => assert_eq!(busy, id.as_str()),
        other => panic!("a fork in hand was not busy: {other:?}"),
    }
    // It is a writer like any other: a turn goes on in it.
    bench
        .store
        .append(&mut fork, user("again"))
        .expect("appended");
    bench
        .store
        .append(&mut fork, concluded())
        .expect("appended");
    bench.store.publish(&mut fork).expect("published");
    drop(fork);
    let resumed = bench
        .store
        .resume_without_waiting(&Selector::Id(id), &bench.workspace)
        .expect("let go when dropped");
    let users: Vec<&str> = resumed
        .session
        .state()
        .turns
        .iter()
        .map(|turn| turn.user.as_str())
        .collect();
    assert_eq!(users, ["one", "again"]);
}

#[test]
fn a_fork_origin_xfx_would_not_have_written_is_refused() {
    let started = |origin: Option<ForkOrigin>| EventEnvelope {
        schema_version: EVENT_SCHEMA_VERSION,
        log_generation: "a".repeat(32),
        seq: 1,
        event_id: "b".repeat(32),
        timestamp_ms: 1,
        event: SessionEvent::SessionStarted {
            id: "s1".to_string(),
            origin_workspace_root: "/w".to_string(),
            workspace_root: "/w".to_string(),
            model: "m".to_string(),
            permission_mode: "auto".to_string(),
            provider: None,
            forked_from: origin,
        },
    };
    let origin = |id: &str, seq: u64| {
        Some(ForkOrigin {
            id: id.to_string(),
            seq,
        })
    };
    assert!(apply(&mut DurableState::empty(), &started(None)).is_ok());
    assert!(apply(&mut DurableState::empty(), &started(origin("s0", 4))).is_ok());
    for bad in [
        origin("../s0", 4),
        origin("s0\u{1b}[2J", 4),
        origin("", 4),
        origin("s1", 4),
        origin("s0", 0),
    ] {
        assert!(
            apply(&mut DurableState::empty(), &started(bad.clone())).is_err(),
            "{bad:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// FK1b: what a fork must not bring back, and what it must not expose
// ---------------------------------------------------------------------------

fn grant(tool: &str, target: &str) -> SessionEvent {
    SessionEvent::PermissionGrantRecorded {
        tool: tool.to_string(),
        target: target.to_string(),
    }
}

#[test]
fn a_grant_the_source_withdrew_after_the_fork_point_stays_withdrawn() {
    // An "always" is the user's answer to this session (`store.rs`'s
    // `PermissionGrantsCleared` withdraws every one before it). Upstream
    // copies lines 2..S as they are, which would bring back one the source
    // has withdrawn since; xfx carries only the ones the source still holds.
    let bench = bench();
    let a = crate::permission::Grant::new("terminal", "ls");
    let b = crate::permission::Grant::new("write_file", "/w/a.txt");
    let (mut recorder, source) = bench.open(&bench.workspace);
    recorder.commit(user("one"));
    recorder.commit(grant(&a.tool, &a.target));
    recorder.commit(answer("ONE", &[]));
    recorder.commit(concluded());
    recorder.commit(user("two"));
    recorder.commit(grant(&b.tool, &b.target));
    recorder.commit(answer("TWO", &[]));
    recorder.commit(concluded());
    // After S of a fork before turn 3: every grant withdrawn, then `a` given
    // again.
    recorder.commit(SessionEvent::PermissionGrantsCleared);
    recorder.commit(user("three"));
    recorder.commit(grant(&a.tool, &a.target));
    recorder.commit(answer("THREE", &[]));
    recorder.commit(concluded());
    assert_eq!(recorder.failure(), None);
    drop(recorder);
    let through = bench.state(&source).turns[1].seqs.last;
    assert_eq!(
        state_through(&bench.store, &source, through).grants,
        [a.clone(), b.clone()]
    );

    let fork = bench.fork(&source, 3).expect("forked");
    let id = fork.id().clone();
    assert_eq!(fork.state().grants, std::slice::from_ref(&a));
    drop(fork);
    assert_eq!(
        bench.state(&id).grants,
        std::slice::from_ref(&a),
        "the withdrawn grant came back"
    );
    let resumed = bench
        .store
        .resume(&Selector::Id(id.clone()), &bench.workspace)
        .expect("resumed");
    assert_eq!(resumed.session.state().grants, std::slice::from_ref(&a));
    drop(resumed);
    // Withdrawn after S in the log's own words: a clear, then what is held.
    let kinds: Vec<&str> = bench.log(&id)[through as usize..]
        .iter()
        .map(|frame| frame.event.kind())
        .collect();
    assert_eq!(
        kinds,
        [
            "permission_grants_cleared",
            "permission_grant_recorded",
            "session_renamed"
        ]
    );

    // Nothing withdrawn, nothing added: before turn 2 the fork holds `a`,
    // which the source still holds, and its frames are upstream's.
    let fork = bench.fork(&source, 2).expect("forked");
    assert_eq!(bench.state(fork.id()).grants, std::slice::from_ref(&a));
    let kinds: Vec<&str> = bench
        .log(fork.id())
        .iter()
        .map(|frame| frame.event.kind())
        .collect();
    assert_eq!(kinds.last(), Some(&"session_renamed"));
    assert!(!kinds.contains(&"permission_grants_cleared"), "{kinds:?}");

    // All withdrawn and none given again: none.
    let (mut recorder, source) = bench.open(&bench.workspace);
    recorder.commit(user("one"));
    recorder.commit(grant(&a.tool, &a.target));
    recorder.commit(concluded());
    recorder.commit(SessionEvent::PermissionGrantsCleared);
    drop(recorder);
    let fork = bench.fork(&source, 2).expect("forked");
    assert_eq!(bench.state(fork.id()).grants, []);
}

#[test]
fn a_forks_title_takes_no_control_character_c1_included() {
    // `char::is_control`: C0, DEL and C1 (U+0080-U+009F) -- U+009B is a CSI
    // to a terminal that reads C1 -- each a space, the mark kept once.
    for (title, forked) in [
        ("a\u{9b}2Jb", "a 2Jb (fork)"),
        ("a\u{85}b", "a b (fork)"),
        ("x\u{1b}[31my", "x [31my (fork)"),
        ("x\u{9b}y (fork)", "x y (fork)"),
        ("\u{9b}lead", "lead (fork)"),
    ] {
        assert_eq!(
            super::fork_title(title).as_deref(),
            Some(forked),
            "{title:?}"
        );
    }
    // Nothing left but controls: no title, rather than a mark alone.
    assert_eq!(super::fork_title("\u{9b}\u{85}"), None);

    // Through a fork: a rename the source holds -- `validate_title` takes
    // every byte at or above 0x80 -- is not carried as it is.
    let bench = bench();
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    recorder.commit(renamed("named \u{9b}2J here"));
    assert_eq!(recorder.failure(), None);
    drop(recorder);
    let fork = bench.fork(&source, 2).expect("forked");
    let title = bench.state(fork.id()).title().expect("a title");
    assert!(!title.chars().any(char::is_control), "{title:?}");
    assert_eq!(title, "named  2J here (fork)");
}

/// The case below, in a copy of this test binary started under `umask 0`.
#[cfg(unix)]
const UNDER_OPEN_UMASK_CASE: &str =
    "session::store::fork::tests::under_an_open_umask_a_forks_folders_are_private_from_creation";

/// Set in the copy, which alone runs the case's body.
#[cfg(unix)]
const UNDER_OPEN_UMASK: &str = "XFX_TEST_UNDER_OPEN_UMASK";

#[cfg(unix)]
#[test]
fn under_an_open_umask_a_forks_folders_are_private_from_creation() {
    if std::env::var_os(UNDER_OPEN_UMASK).is_none() {
        return;
    }
    let root = tempfile::TempDir::new().expect("a root");
    let mode_of = |path: &Path| mode(&fs::symlink_metadata(path).expect("stat"));
    // The umask took: a folder made the plain way is open to everyone.
    let plain = root.path().join("plain");
    fs::create_dir(&plain).expect("a plain folder");
    assert_eq!(mode_of(&plain), 0o777, "the copy is not under umask 0");
    // Every folder the fork makes is made by this, and is owner-only the
    // moment it exists -- before the chmod that follows it.
    let made = root.path().join("made");
    super::super::create_private_dir(&made).expect("made");
    assert_eq!(
        mode_of(&made),
        0o700,
        "open between its creation and its chmod"
    );

    // And a whole fork made here: every folder 0700, every file 0600.
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let fork = bench.fork(&source, 3).expect("forked");
    for dir in [
        bench.store.session_dir(fork.id()),
        bench.store.profile_dir().join(FORK_STAGE_DIR),
    ] {
        for (path, (bytes, _, mode)) in tree(&dir) {
            let want = if bytes.is_some() { 0o600 } else { 0o700 };
            assert_eq!(mode, want, "{}", path.display());
        }
    }
}

#[cfg(unix)]
#[test]
fn a_forks_folders_are_private_from_creation_under_any_umask() {
    use std::os::unix::process::CommandExt;
    let mut runner = crate::process::command(std::env::current_exe().expect("this test binary"));
    runner.args(["--exact", "--test-threads=1", UNDER_OPEN_UMASK_CASE]);
    runner.env(UNDER_OPEN_UMASK, "1");
    // SAFETY: `umask` is async-signal-safe and touches nothing of the parent.
    unsafe {
        runner.pre_exec(|| {
            libc::umask(0);
            Ok(())
        });
    }
    // Bounded: a child that hangs is killed and reaped, and the case fails.
    let output = crate::process::output_within(&mut runner, CHILD_LIMIT).expect("a runner");
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Counted, not only exited zero: a filter that matched nothing passes.
    assert!(
        output.status.success() && report.contains("test result: ok. 1 passed"),
        "{report}"
    );
}

/// How long the umask case's child may run: the copy runs one case, which
/// takes well under a second; a child still running after this is hung.
#[cfg(unix)]
const CHILD_LIMIT: std::time::Duration = std::time::Duration::from_secs(120);

#[cfg(unix)]
#[test]
fn a_profile_home_gone_before_the_stage_is_made_is_not_made_again() {
    // `ensure_private_dir` makes no parent: a `~/.xfx` removed under a
    // running session -- here, after the source was read and before the
    // stage root is made -- fails the fork rather than coming back with the
    // umask's mode (it did, through a `create_dir_all` of the parent).
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let profile = bench.store.profile_dir().to_path_buf();
    BEFORE_STAGE.with(|cell| {
        cell.set(Some(|profile| {
            fs::remove_dir_all(profile).expect("the profile home taken away")
        }))
    });
    let result = bench.fork(&source, 3);
    BEFORE_STAGE.with(|cell| cell.set(None));
    match result {
        Err(SessionError::Io { path, source }) => {
            assert_eq!(path, profile.join(FORK_STAGE_DIR));
            assert_eq!(source.kind(), io::ErrorKind::NotFound, "{source}");
        }
        other => panic!("a fork with its home gone: {other:?}"),
    }
    assert!(
        fs::symlink_metadata(&profile).is_err(),
        "the fork made the profile home again"
    );
}

#[cfg(unix)]
#[test]
fn a_stage_folder_that_is_not_the_stores_own_is_refused_and_left_alone() {
    // `sessions.staged` is where every fork is built, so it is held to what
    // `~/.xfx` and `sessions` are held to (`verify_store_dir`).
    let bench = bench();
    let (recorder, source) = rich(&bench);
    drop(recorder);
    let stage = bench.store.profile_dir().join(FORK_STAGE_DIR);
    let outside = bench.root.path().join("outside-stage");
    fs::create_dir(&outside).expect("a folder outside the store");
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).expect("private");

    type Plant = fn(&Path, &Path);
    let plants: [(&str, Plant); 3] = [
        ("a symlink out of the store", |stage, outside| {
            std::os::unix::fs::symlink(outside, stage).expect("plant a link");
        }),
        ("open to the group", |stage, _| {
            fs::create_dir(stage).expect("a folder");
            fs::set_permissions(stage, fs::Permissions::from_mode(0o750)).expect("open it");
        }),
        ("a file", |stage, _| {
            fs::write(stage, b"").expect("a file");
        }),
    ];
    for (what, plant) in plants {
        plant(&stage, &outside);
        let sessions = bench.names().0;
        match bench.fork(&source, 3) {
            Err(SessionError::InsecureParent { path, .. }) => assert_eq!(path, stage, "{what}"),
            Err(other) => panic!("{what}: refused as {other:?}"),
            Ok(fork) => panic!("{what}: forked {}", fork.id()),
        }
        // Nothing made: no session, nothing where the link points.
        assert_eq!(bench.names().0, sessions, "{what}");
        assert_eq!(
            fs::read_dir(&outside).expect("the outside folder").count(),
            0,
            "{what}"
        );
        if fs::symlink_metadata(&stage).is_ok_and(|meta| meta.is_dir()) {
            fs::remove_dir(&stage).expect("clear the plant");
        } else {
            fs::remove_file(&stage).expect("clear the plant");
        }
    }

    // One another account owns can be made only by root; where the tests
    // run as anyone else it cannot be planted, and this part says so.
    fs::create_dir(&stage).expect("a folder");
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700)).expect("private");
    let euid = rustix::process::geteuid().as_raw();
    match std::os::unix::fs::chown(&stage, Some(euid.wrapping_add(1)), None) {
        Ok(()) => match bench.fork(&source, 3) {
            Err(SessionError::InsecureParent { path, detail }) => {
                assert_eq!(path, stage);
                assert!(detail.contains("owned by uid"), "{detail}");
            }
            other => panic!("a stage another account owns: {other:?}"),
        },
        Err(err) => eprintln!(
            "not planted: a stage another account owns needs root to make ({err}); \
             uid {euid} runs these tests"
        ),
    }
}

// ---------------------------------------------------------------------------
// FK1c: two gaps the FK1b review found in the tests above
// ---------------------------------------------------------------------------

#[test]
fn a_grant_given_only_after_the_fork_point_is_not_the_forks() {
    // `a` and `b` before S; after it `b` withdrawn -- every grant cleared,
    // `a` given again -- and `c` given in turn 3. The source holds `[a, c]`;
    // the fork holds what it recorded and the source still holds: `[a]`.
    // Re-recording all the source holds would widen it to `c`.
    let bench = bench();
    let a = crate::permission::Grant::new("terminal", "ls");
    let b = crate::permission::Grant::new("write_file", "/w/a.txt");
    let c = crate::permission::Grant::new("terminal", "rm -rf build");
    let (mut recorder, source) = bench.open(&bench.workspace);
    recorder.commit(user("one"));
    recorder.commit(grant(&a.tool, &a.target));
    recorder.commit(grant(&b.tool, &b.target));
    recorder.commit(answer("ONE", &[]));
    recorder.commit(concluded());
    turn(&mut recorder, "two", "TWO");
    recorder.commit(SessionEvent::PermissionGrantsCleared);
    recorder.commit(grant(&a.tool, &a.target));
    recorder.commit(user("three"));
    recorder.commit(grant(&c.tool, &c.target));
    recorder.commit(answer("THREE", &[]));
    recorder.commit(concluded());
    assert_eq!(recorder.failure(), None);
    drop(recorder);
    assert_eq!(bench.state(&source).grants, [a.clone(), c.clone()]);

    let fork = bench.fork(&source, 3).expect("forked");
    assert_eq!(
        fork.state().grants,
        std::slice::from_ref(&a),
        "a grant given only after S reached the fork"
    );
    let id = fork.id().clone();
    drop(fork);
    assert_eq!(bench.state(&id).grants, std::slice::from_ref(&a));
}

#[test]
fn every_folder_a_fork_makes_is_made_private_from_creation() {
    // The umask case above proves `create_private_dir` and the final modes,
    // which the chmod after each creation makes 0700 whatever made the
    // folder; this pins every call site to the helper. A plain
    // `create_dir` at any of them is the umask's mode until the chmod.
    let fork = include_str!("../fork.rs");
    let code: String = fork
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for plain in ["fs::create_dir(", "create_dir_all(", "DirBuilder"] {
        assert!(
            !code.contains(plain),
            "fork.rs makes a folder with `{plain}`"
        );
    }
    // The stage and its images folder, and the stage root through
    // `ensure_private_dir`.
    assert_eq!(code.matches("create_private_dir(&").count(), 2, "{code}");
    assert_eq!(code.matches("ensure_private_dir(&stage_root)").count(), 1);
    let store = include_str!("../../store.rs");
    let start = store
        .find("fn ensure_private_dir(")
        .expect("ensure_private_dir");
    let body = &store[start..start + store[start..].find("\n}\n").expect("its end")];
    assert!(body.contains("match create_private_dir(path)"), "{body}");
    assert!(!body.contains("fs::create_dir("), "{body}");
}

// ---------------------------------------------------------------------------
// saved-session rules: the source's denies, and nothing wider
// ---------------------------------------------------------------------------

/// A store with `rules` remembered in order, each a new rule, then the rule
/// with id `revoked` taken out when one is named.
fn remembered(
    rules: &[(&str, crate::permission::session_rules::Decision)],
    revoked: Option<u64>,
) -> crate::permission::session_rules::State {
    use crate::permission::session_rules::{Applied, Event, KeyKind, RuleKey, State};
    let mut state = State::default();
    for (command, decision) in rules {
        let event = Event::Set {
            key: RuleKey::new(KeyKind::Command, command.as_bytes().to_vec()).expect("a key"),
            display: format!("terminal.exec {command}"),
            decision: *decision,
            expected: None,
        };
        state = match state.apply(&event) {
            Applied::Applied(next) => next,
            other => panic!("not applied: {other:?}"),
        };
    }
    if let Some(id) = revoked {
        let expected = state.rule_for_id(id).expect("the rule").generation;
        state = match state.apply(&Event::Revoke { id, expected }) {
            Applied::Applied(next) => next,
            other => panic!("not revoked: {other:?}"),
        };
    }
    state
}

/// A session with one ended turn and `store` as its saved-session rules.
fn with_rules(bench: &Bench, store: Option<&crate::permission::session_rules::State>) -> SessionId {
    let (mut recorder, source) = bench.open(&bench.workspace);
    turn(&mut recorder, "one", "ONE");
    assert_eq!(recorder.failure(), None);
    drop(recorder);
    if let Some(store) = store {
        let path = crate::permission::session_rules::path_in(&bench.store.session_dir(&source));
        fs::write(&path, store.encode()).expect("the source's rules");
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("private");
    }
    source
}

#[test]
fn a_fork_carries_the_sources_remembered_denies_and_no_allow() {
    // fx v0.0.16 keeps the rules as `set permissions` lines its fork copies
    // with lines 2..S (`session_adapter.zig:2205-2208`, `session.zig:
    // 1744-1747@481104fd`), so its fork holds them as of the cut. xfx's file
    // says only what they are now: the fork takes every deny -- which can
    // only narrow it -- and no allow, which nothing shows was in force at
    // the cut. Red before: the fork had no `permissions.json`, so a deny the
    // source remembered decided nothing in it.
    use crate::permission::session_rules::{self, Decision, KeyKind, RuleKey, Verdict};
    let bench = bench();
    // ids 1-4; `rm` (id 3) revoked, which spends generation 5.
    let source_rules = remembered(
        &[
            ("git push", Decision::Deny),
            ("ls", Decision::Allow),
            ("rm -rf build", Decision::Deny),
            ("cat notes.txt", Decision::Deny),
        ],
        Some(3),
    );
    assert_eq!(source_rules.next_generation, 6);
    let source = with_rules(&bench, Some(&source_rules));
    let theirs = session_rules::path_in(&bench.store.session_dir(&source));
    let before = fs::read(&theirs).expect("the source's rules");

    for before_turn in [1, 2] {
        let fork = bench.fork(&source, before_turn).expect("forked");
        let ours = session_rules::path_in(&bench.store.session_dir(fork.id()));
        let held = session_rules::load(&ours).expect("the fork's rules");
        let key = |command: &str| {
            RuleKey::new(KeyKind::Command, command.as_bytes().to_vec()).expect("a key")
        };
        assert_eq!(held.decide(&key("git push")), Verdict::Deny);
        assert_eq!(held.decide(&key("cat notes.txt")), Verdict::Deny);
        assert_eq!(held.decide(&key("ls")), Verdict::Unresolved, "an allow");
        assert_eq!(held.decide(&key("rm -rf build")), Verdict::Unresolved);
        // Each deny as the source holds it, and the next generation, so a
        // rule the fork remembers takes no id the source gave.
        let ids: Vec<(u64, u64)> = held
            .rules
            .iter()
            .map(|rule| (rule.id, rule.generation))
            .collect();
        assert_eq!(ids, [(1, 1), (4, 4)]);
        assert_eq!(held.next_generation, 6);
        assert_eq!(
            held.rules,
            source_rules
                .rules
                .iter()
                .filter(|rule| rule.decision == Decision::Deny)
                .cloned()
                .collect::<Vec<_>>()
        );
        #[cfg(unix)]
        {
            let meta = fs::symlink_metadata(&ours).expect("stat");
            assert!(meta.is_file());
            assert_eq!(mode(&meta), 0o600);
            assert_ne!(
                meta.ino(),
                fs::metadata(&theirs).expect("stat").ino(),
                "the fork's rules are its own file"
            );
        }
        // Nothing else of the store's is left in the fork's folder (sorted).
        let mut names: Vec<String> = fs::read_dir(bench.store.session_dir(fork.id()))
            .expect("the fork's folder")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [EVENTS_FILE, session_rules::FILE_NAME, MANIFEST_FILE],
            "before turn {before_turn}"
        );
    }
    assert_eq!(
        fs::read(&theirs).expect("read"),
        before,
        "the source was written"
    );
}

#[test]
fn a_fork_of_a_session_with_no_deny_has_no_rules_file() {
    use crate::permission::session_rules::{self, Decision};
    let bench = bench();
    let allows = remembered(&[("ls", Decision::Allow)], None);
    let revoked = remembered(&[("git push", Decision::Deny)], Some(1));
    for (what, store) in [
        ("no store", None),
        ("allows only", Some(&allows)),
        ("every deny revoked", Some(&revoked)),
    ] {
        let source = with_rules(&bench, store);
        let fork = bench.fork(&source, 2).expect("forked");
        assert!(
            !session_rules::path_in(&bench.store.session_dir(fork.id())).exists(),
            "{what}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_fork_whose_source_rules_cannot_be_read_or_are_a_link_is_refused_and_leaves_nothing() {
    // The source refuses every call while its store cannot be read
    // (`PermissionSession::decide_with_feedback`), so a fork that dropped the
    // store would let by what the source refuses. A link is refused even to
    // a store that reads, as a fork takes no image through one.
    use crate::permission::session_rules::{self, Decision};
    let bench = bench();
    let outside = bench.root.path().join("outside");
    fs::create_dir(&outside).expect("a directory outside the store");
    let planted = outside.join("permissions.json");
    let deny = remembered(&[("git push", Decision::Deny)], None);
    fs::write(&planted, deny.encode()).expect("a store outside");
    fs::set_permissions(&planted, fs::Permissions::from_mode(0o600)).expect("private");

    type Damage = fn(&Path, &Path);
    let damages: [(&str, Damage); 5] = [
        ("broken", |path, _| {
            fs::write(path, "{broken").expect("write")
        }),
        ("a link to a store", |path, planted| {
            std::os::unix::fs::symlink(planted, path).expect("plant a link")
        }),
        ("a dangling link", |path, planted| {
            std::os::unix::fs::symlink(planted.with_extension("gone"), path).expect("plant a link")
        }),
        ("a folder", |path, _| {
            fs::create_dir(path).expect("a folder")
        }),
        // Opened without waiting for a writer, then refused. Made by
        // `mkfifo(1)` through the one door every child process goes through.
        ("a FIFO", |path, _| {
            let status = crate::process::command("mkfifo")
                .arg(path)
                .status()
                .expect("run mkfifo");
            assert!(status.success(), "mkfifo failed: {status:?}");
        }),
    ];
    for (what, damage) in damages {
        let source = with_rules(&bench, None);
        damage(
            &session_rules::path_in(&bench.store.session_dir(&source)),
            &planted,
        );
        let before = bench.names();
        match bench.fork(&source, 2) {
            Err(SessionError::Corrupt { id, detail }) => {
                assert_eq!(id, source.as_str(), "{what}");
                assert!(
                    detail.contains("saved-session permission rules"),
                    "{what}: {detail}"
                );
            }
            Err(other) => panic!("{what}: refused as {other:?}"),
            Ok(session) => panic!("{what}: forked {}", session.id()),
        }
        assert_eq!(bench.names(), before, "{what}");
        assert_eq!(bench.names().1, Vec::<String>::new(), "{what}");
    }
}

#[cfg(unix)]
#[test]
fn a_fork_whose_rules_cannot_be_written_is_refused_and_its_stage_is_gone() {
    // `session_rules::seed` refuses whatever is already at the store's path
    // in the stage -- a file another hand put there, a folder, a link -- and
    // the fork's `?` on it drops the stage, whose `Drop` removes it with
    // everything in it: no session, no stage, the source as it was.
    use crate::permission::session_rules::{self, Decision};
    let bench = bench();
    let deny = remembered(&[("git push", Decision::Deny)], None);
    let source = with_rules(&bench, Some(&deny));
    let theirs = session_rules::path_in(&bench.store.session_dir(&source));
    let before_rules = fs::read(&theirs).expect("the source's rules");

    type Hand = fn(&Path);
    let hands: [(&str, Hand); 3] = [
        ("a file", |stage| {
            fs::write(session_rules::path_in(stage), b"{}").expect("a file there")
        }),
        ("a folder", |stage| {
            fs::create_dir(session_rules::path_in(stage)).expect("a folder there")
        }),
        ("a link", |stage| {
            std::os::unix::fs::symlink("/dev/null", session_rules::path_in(stage))
                .expect("a link there")
        }),
    ];
    for (what, hand) in hands {
        STAGE_SEEN.with(|seen| *seen.borrow_mut() = None);
        BEFORE_RULES.with(|cell| cell.set(Some(hand)));
        let before = bench.names();
        let result = bench.fork(&source, 2);
        BEFORE_RULES.with(|cell| cell.set(None));
        match result {
            Err(SessionError::Io { path, .. }) => {
                assert!(
                    path.ends_with(session_rules::FILE_NAME),
                    "{what}: {}",
                    path.display()
                );
            }
            Err(other) => panic!("{what}: refused as {other:?}"),
            Ok(session) => panic!("{what}: forked {}", session.id()),
        }
        // The seam ran in a stage, and that stage is gone.
        let stage = STAGE_SEEN
            .with(|seen| seen.borrow_mut().take())
            .unwrap_or_else(|| panic!("{what}: the rules were never written"));
        assert!(
            stage.starts_with(bench.store.profile_dir().join(FORK_STAGE_DIR)),
            "{what}: {}",
            stage.display()
        );
        assert!(
            fs::symlink_metadata(&stage).is_err(),
            "{what}: the stage was left behind"
        );
        assert_eq!(bench.names(), before, "{what}");
        assert_eq!(bench.names().1, Vec::<String>::new(), "{what}");
        assert_eq!(fs::read(&theirs).expect("read"), before_rules, "{what}");
    }
    // With no other hand, the same fork is made, and its rules are there.
    let fork = bench.fork(&source, 2).expect("forked");
    assert_eq!(
        session_rules::load(&session_rules::path_in(&bench.store.session_dir(fork.id())))
            .expect("the fork's rules"),
        deny
    );
}
