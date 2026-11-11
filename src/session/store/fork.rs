//! `fork`: a new session holding another's turns before one of its prompts.
//!
//! Upstream's `/fork` (D58, `vercel-labs/fx@481104fd
//! tests/tla/session_manager/system-design.md:1168-1194`) is a store
//! operation under a small surface: `Session.fork(before_turn)`
//! (`src/core/session/session_adapter.zig:2436-2457`) asks the manager's
//! `openFork` (`src/core/session_manager/session.zig:1714-1759`) for a new
//! root session whose line 1 is its own `session_created`, naming where it
//! came from, and whose lines 2..S are the source's, where S is the line that
//! ended the turn before the chosen prompt (`findForkPoint`, `:1780-1830`).
//! Then it renames the fork after its source, marked once (`forkTitle`,
//! `session_adapter.zig:5096-5100`). The source is read without its lock and
//! never written.
//!
//! **xfx re-frames rather than copies.** Upstream copies lines 2..S byte for
//! byte, crc and all. An xfx frame carries its log's generation and an id
//! unique within that log ([`EventEnvelope`]), so the same bytes would be a
//! frame "copied in from elsewhere" -- exactly what a replay refuses. Each
//! event is written again, under the fork's generation and an id of its own,
//! **at the source's sequence number and time**: a checkpoint's coverage is a
//! frame number ([`SessionEvent::ContextCheckpoint`]), and a turn's timing is
//! its frames' times, so both mean in the fork what they meant in the source.
//!
//! **Built aside, then renamed into place**, as upstream stages under
//! `.tmp/{id}` (`stage`, `session.zig:541-580`): the log, its manifest, its
//! images and its saved-session rules are written in [`FORK_STAGE_DIR`], beside `sessions` rather than in
//! it -- a listing counts every name in `sessions` that is not a session -- and
//! the folder is renamed to `sessions/<id>` only once it is whole. A fork that
//! fails is removed with its stage; a reader never sees one half made.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{
    apply, create_private_dir, create_private_file, ensure_private_dir, io_error, lock_exclusive,
    read_snapshot, read_snapshots, set_private_dir_mode, sync_directory, validate_title,
    verify_private, workspace_key, Disk, DurableState, SessionError, SessionId, SessionStore,
    TurnStep, WritableSession, EVENTS_FILE, IMAGES_DIR, MAX_EVENT_LOG_BYTES,
    MAX_RENAMED_TITLE_BYTES,
};
use crate::permission::session_rules;
use crate::session::event::{
    new_identifier, EventEnvelope, ForkOrigin, SessionEvent, TitleSource, EVENT_SCHEMA_VERSION,
};

/// The folder under the profile home a fork is built in, one folder per fork
/// named by its id, before it is renamed into `sessions`.
///
/// Beside `sessions`, so the rename never crosses a filesystem and a listing
/// never meets a fork being built. A fork the process died building is left
/// here, owner-only and inert: no reader looks in it.
pub const FORK_STAGE_DIR: &str = "sessions.staged";

/// What a fork's title is marked with, once (`forkTitle`,
/// `session_adapter.zig:5096-5100`).
const FORK_MARK: &str = " (fork)";

impl SessionStore {
    /// A new session holding every turn of `source` before turn
    /// `before_turn` (counted from 1), open for writing, and bound to
    /// `workspace`.
    ///
    /// Upstream's `Session.fork` (`session_adapter.zig:2436-2457`):
    ///
    /// - **Where it is cut.** Turn 0 is refused. Before turn 1 is a session
    ///   with no turn, holding what was set before the first prompt. Before
    ///   turn `n` keeps through the frame that ended turn `n - 1`, which has
    ///   to have ended -- a conclusion, final or interrupted, as upstream's
    ///   point is a `turn_committed` or `turn_interrupted` line. What the
    ///   source wrote after that frame, a later preference or a later
    ///   compaction, is not the fork's.
    /// - **Its frames.** Line 1 is its own `session_started`: the source's
    ///   roots, model, mode and provider as the source began, a new id, and
    ///   `forked_from` naming the source and the last frame kept. Then each
    ///   kept event at its own number and time, re-framed (see the module).
    ///   When the source has withdrawn a grant those events recorded, a clear
    ///   and the grants it still holds. Then the source's title as the
    ///   source is called now, marked
    ///   ` (fork)` once -- a fork of a fork carries one -- and, when the
    ///   session was bound elsewhere at that frame, a rebind to `workspace`,
    ///   as a resume writes one.
    /// - **Its images.** Each one a kept frame names is hard-linked from the
    ///   source's folder, or copied when a link is refused or would not be a
    ///   private file of this account ([`carry_image`]); each is checked
    ///   against what its frame recorded first. One missing or damaged refuses
    ///   the fork, as upstream stops at a missing blob (`session.zig:1803-1809`).
    /// - **Its saved-session rules: the source's remembered denies, and no
    ///   allow.** Upstream's rules are `set permissions` lines in its log
    ///   (`setPermissions`, `session_adapter.zig:2205-2208`), so its fork holds
    ///   them as they stood at the cut, allows and denies, among the lines
    ///   2..S it copies (`session.zig:1744-1747`). xfx keeps them beside the
    ///   log in `permissions.json` ([`session_rules`]), which says only what
    ///   they are now. So the fork carries what never widens what it may do:
    ///   each deny the source holds now, with its id and generation, and the
    ///   source's next generation, so no id is given twice. An allow is not
    ///   carried, since nothing shows it was in force at the cut -- as an
    ///   "always" given after the cut is not the fork's -- and the fork asks
    ///   again. Read not through a link ([`session_rules::load_not_through_a_link`])
    ///   and written into the stage owner-only with the rest; a store that
    ///   cannot be read refuses the fork.
    /// - **The source is never written.** It is read as `detail` reads it --
    ///   its published boundary, cross-checked, without its lock, which the
    ///   session being forked is normally holding.
    ///
    /// The writer it returns holds the fork's lock, as every
    /// [`WritableSession`] does, and lets go of it when dropped.
    pub fn fork(
        &self,
        source: &SessionId,
        before_turn: usize,
        workspace: &Path,
    ) -> Result<WritableSession, SessionError> {
        self.require_writable()?;
        self.verify_store_dirs()?;
        let refused = |detail: String| SessionError::InvalidForkPoint {
            id: source.as_str().to_string(),
            detail,
        };
        let kept = before_turn
            .checked_sub(1)
            .ok_or_else(|| refused("there is no turn 0 to fork before".to_string()))?;

        let manifest = self.read_manifest(source)?;
        let replay = self.replay(source, &manifest)?;
        let through = fork_point(&replay.state, kept).map_err(refused)?;
        let corrupt = |detail: String| SessionError::Corrupt {
            id: source.as_str().to_string(),
            detail,
        };
        let text = std::str::from_utf8(&replay.bytes)
            .map_err(|_| corrupt("the published bytes are not UTF-8".to_string()))?;
        let frames = text
            .lines()
            .take(usize::try_from(through).unwrap_or(usize::MAX))
            .map(EventEnvelope::decode)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| corrupt(err.to_string()))?;
        let images = kept_images(&replay.state, kept);
        // Before anything is made: a store that cannot be read refuses the
        // fork, as it refuses every call the source makes -- a deny in it
        // must not lapse in the fork because its file broke.
        let rules = carried_rules(&self.session_dir(source)).map_err(|reason| {
            corrupt(format!(
                "its saved-session permission rules cannot be forked: {reason}"
            ))
        })?;

        let id = SessionId::generate();
        let destination = self.session_dir(&id);
        let stage_root = self.profile_dir.join(FORK_STAGE_DIR);
        // A test's hand on the profile home once the source has been read and
        // before anything of the fork is made; compiled out of every other
        // build.
        #[cfg(test)]
        tests::before_stage(&self.profile_dir);
        ensure_private_dir(&stage_root)?;
        let stage = Stage::create(stage_root.join(id.as_str()))?;

        // The log, in memory first: every frame reduced before one byte is
        // written, so a fork that cannot be made writes nothing.
        let mut log = Log::new(id.clone());
        let now = self.clock.now_ms();
        for frame in frames {
            let event = match frame.event {
                SessionEvent::SessionStarted {
                    origin_workspace_root,
                    workspace_root,
                    model,
                    permission_mode,
                    provider,
                    ..
                } => SessionEvent::SessionStarted {
                    id: id.as_str().to_string(),
                    origin_workspace_root,
                    workspace_root,
                    model,
                    permission_mode,
                    provider,
                    forked_from: Some(ForkOrigin {
                        id: source.as_str().to_string(),
                        seq: through,
                    }),
                },
                SessionEvent::ImagesAttached { mut images } => {
                    // A pasted image is recorded by its session's own copy;
                    // the fork's is its own, so it stands without the source.
                    for image in &mut images {
                        let name = crate::image::snapshot_name(image.id, &image.sha256);
                        let theirs = self.session_dir(source).join(IMAGES_DIR).join(&name);
                        if image.path == theirs.display().to_string() {
                            image.path = destination
                                .join(IMAGES_DIR)
                                .join(&name)
                                .display()
                                .to_string();
                        }
                    }
                    SessionEvent::ImagesAttached { images }
                }
                event => event,
            };
            // Line 1 is the fork's own, made now; the rest keep their times.
            let at = if frame.seq == 1 {
                now
            } else {
                frame.timestamp_ms
            };
            log.push(at, event)?;
        }
        // An "always" the source has withdrawn since S stays withdrawn: the
        // grants the copied frames recorded are cleared and the ones the
        // source still holds given again. Upstream copies lines 2..S as they
        // are; a withdrawn answer must not come back under a new id.
        let held = &replay.state.grants;
        if log.state.grants.iter().any(|grant| !held.contains(grant)) {
            let kept: Vec<_> = log
                .state
                .grants
                .iter()
                .filter(|grant| held.contains(grant))
                .cloned()
                .collect();
            log.push(now, SessionEvent::PermissionGrantsCleared)?;
            for grant in kept {
                log.push(
                    now,
                    SessionEvent::PermissionGrantRecorded {
                        tool: grant.tool,
                        target: grant.target,
                    },
                )?;
            }
        }
        if let Some(title) = replay.state.title().as_deref().and_then(fork_title) {
            log.push(
                now,
                SessionEvent::SessionRenamed {
                    title,
                    source: TitleSource::User,
                },
            )?;
        }
        let bound = workspace_key(workspace);
        if log.state.workspace_root != bound {
            let previous = log.state.workspace_root.clone();
            log.push(
                now,
                SessionEvent::WorkspaceRebound {
                    previous_workspace_root: previous,
                    workspace_root: bound,
                },
            )?;
        }

        if !images.is_empty() {
            let theirs = self.session_dir(source).join(IMAGES_DIR);
            match fs::symlink_metadata(&theirs) {
                Ok(metadata) if metadata.is_dir() => verify_private(&theirs, 0o700)?,
                Ok(_) => {
                    return Err(corrupt(
                        "its images folder is not a directory; xfx will not follow it".to_string(),
                    ))
                }
                // Each image then says it is missing, below.
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(io_error(&theirs, err)),
            }
            let ours = stage.path.join(IMAGES_DIR);
            create_private_dir(&ours).map_err(|err| io_error(&ours, err))?;
            set_private_dir_mode(&ours)?;
            for (name, sha256, size) in &images {
                carry_image(
                    &theirs.join(name),
                    &ours.join(name),
                    sha256,
                    *size,
                    |from, to| fs::hard_link(from, to),
                )
                .map_err(|err| match err {
                    CarryError::Unusable(reason) => corrupt(format!(
                        "the image saved as `{name}` cannot be forked: {reason}"
                    )),
                    CarryError::Store(err) => err,
                })?;
            }
            sync_directory(&ours)?;
        }

        // In the stage, so the fork is never seen without them: owner-only,
        // published with `link(2)` and read back ([`session_rules::seed`]).
        if let Some(rules) = &rules {
            // A test's hand in the stage just before the rules are written
            // there; compiled out of every other build.
            #[cfg(test)]
            tests::before_rules(&stage.path);
            session_rules::seed(&stage.path, rules)
                .map_err(|err| io_error(&session_rules::path_in(&stage.path), err))?;
        }

        let log_path = stage.path.join(EVENTS_FILE);
        let mut file = create_private_file(&log_path)?;
        // Held from the start, as `create` holds a new session's.
        lock_exclusive(&file, &log_path, &id)?;
        file.write_all(&log.bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_data())
            .map_err(|err| io_error(&log_path, err))?;
        let mut session = WritableSession {
            id: id.clone(),
            disk: Some(Disk {
                dir: stage.path.clone(),
                file,
            }),
            log_generation: log.generation,
            written_bytes: log.bytes.len() as u64,
            published_bytes: 0,
            hasher: Sha256::new_with_prefix(&log.bytes),
            next_seq: log.next_seq,
            event_ids: log.event_ids,
            state: log.state,
            manifest: None,
            poisoned: None,
        };
        // Published while it is still aside, so it is whole the moment it
        // has a name a reader looks for.
        self.publish(&mut session)?;

        match fs::symlink_metadata(&destination) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(SessionError::AlreadyExists {
                    id: id.as_str().to_string(),
                })
            }
            Err(err) => return Err(io_error(&destination, err)),
        }
        fs::rename(&stage.path, &destination).map_err(|err| io_error(&destination, err))?;
        stage.placed();
        if let Some(disk) = session.disk.as_mut() {
            disk.dir = destination.clone();
        }
        // Whole from here: a failure to make the name durable is reported,
        // and the fork, which is complete, is left where a reader finds it,
        // as upstream leaves a published one (`session.zig:578`).
        sync_directory(&self.sessions_dir)?;
        // Its images read back from its own copies, as a resume reads them.
        // Each was carried whole just now (`carry_image`), so none makes the
        // fork unreadable; one gone since is the request's to fail
        // ([`DurableState::unsendable`]).
        let _ = read_snapshots(&destination, &mut session.state);
        Ok(session)
    }
}

/// The last frame a fork before turn `kept + 1` holds, or why there is none.
fn fork_point(state: &DurableState, kept: usize) -> Result<u64, String> {
    if kept == 0 {
        // Everything set before the first prompt.
        return Ok(state
            .turns
            .first()
            .map_or(state.last_event_seq, |turn| turn.seqs.opened - 1));
    }
    match state.turns.get(kept - 1) {
        None => Err(format!(
            "it has {} turn(s), so a fork cannot keep {kept}",
            state.turns.len()
        )),
        Some(turn) if turn.outcome.is_none() => Err(format!(
            "turn {kept} has not ended, so a fork cannot keep it"
        )),
        Some(turn) => Ok(turn.seqs.last),
    }
}

/// The saved-session rules a fork of the session in `source_dir` holds: each
/// deny of its store as it stands, its id and generation kept, under its next
/// generation; `None` when it has no store or no deny in it, and why when its
/// store cannot be read or is a link.
fn carried_rules(source_dir: &Path) -> Result<Option<session_rules::State>, String> {
    let held = session_rules::load_not_through_a_link(&session_rules::path_in(source_dir))?;
    let denies: Vec<_> = held
        .rules
        .into_iter()
        .filter(|rule| rule.decision == session_rules::Decision::Deny)
        .collect();
    Ok((!denies.is_empty()).then_some(session_rules::State {
        next_generation: held.next_generation,
        rules: denies,
    }))
}

/// Every image the first `kept` turns name: its copy's name, digest and size,
/// each once.
fn kept_images(state: &DurableState, kept: usize) -> Vec<(String, String, u64)> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for turn in &state.turns[..kept] {
        let tool_images = turn.steps.iter().flat_map(|step| match step {
            TurnStep::ToolResult { images, .. } => images.iter(),
            _ => [].iter(),
        });
        for image in turn.images.iter().chain(tool_images) {
            let recorded = &image.recorded;
            let name = crate::image::snapshot_name(recorded.id, &recorded.sha256);
            if seen.insert((name.clone(), recorded.sha256.clone())) {
                out.push((name, recorded.sha256.clone(), recorded.bytes));
            }
        }
    }
    out
}

/// The source's title as a fork's: marked ` (fork)` once, inside the title
/// limit, or `None` where no title the store would write can be made of it.
///
/// Upstream appends the mark and checks nothing (`forkTitle`,
/// `session_adapter.zig:5096-5100`; its `rename`, `:2295-2302`). xfx's
/// reducer refuses a title past [`MAX_RENAMED_TITLE_BYTES`] or with a control
/// character ([`validate_title`]), and a title derived from a prompt can be
/// either -- clipped to the limit already, or holding an escape -- so the base
/// is cut where a character ends to leave room for the mark, and every control
/// character in it is a space: C0 and DEL, which the reducer refuses, and C1
/// (U+0080-U+009F, a CSI among them), which it does not and a terminal may
/// read. A title already marked is cleaned the same way.
fn fork_title(title: &str) -> Option<String> {
    let clean: String = title
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let clean = clean.trim_matches(' ');
    let marked = if clean.ends_with(FORK_MARK) {
        clean.to_string()
    } else {
        let mut end = clean.len().min(MAX_RENAMED_TITLE_BYTES - FORK_MARK.len());
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}{FORK_MARK}", clean[..end].trim_end())
    };
    (validate_title(&marked) == Ok(marked.as_str())).then_some(marked)
}

/// Why an image could not be carried into a fork.
#[derive(Debug)]
pub(super) enum CarryError {
    /// The source's copy is not the image its frame recorded.
    Unusable(String),
    /// The fork's side could not be written.
    Store(SessionError),
}

/// Puts the image saved at `from` -- checked against the `sha256` and `size`
/// its frame recorded -- at `to`: a hard link, as upstream links every blob
/// (D6, `linkBlobs`, `session.zig:611-628`), or a private copy of the bytes
/// just checked.
///
/// A link is kept only when what it names is still the image and is a private
/// file of this account: a link shares its file's mode and owner, so a source
/// copy someone opened up would lend the fork its mode. Anything else -- a
/// filesystem that refuses links, a link that is not that -- is a copy, made
/// new and owner-only. `link` is the operation, so a test can refuse it.
pub(super) fn carry_image(
    from: &Path,
    to: &Path,
    sha256: &str,
    size: u64,
    link: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<(), CarryError> {
    // Not through a link, a regular file, its size, its digest.
    let bytes =
        read_snapshot(from, sha256, size).map_err(|loss| CarryError::Unusable(loss.to_string()))?;
    if link(from, to).is_ok() {
        if read_snapshot(to, sha256, size).is_ok() && owned_and_private(to) {
            return Ok(());
        }
        fs::remove_file(to).map_err(|err| CarryError::Store(io_error(to, err)))?;
    }
    let mut file = create_private_file(to).map_err(CarryError::Store)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|err| CarryError::Store(io_error(to, err)))
}

/// Whether `path` is a regular file this account owns that nobody else can
/// read or write.
#[cfg(unix)]
fn owned_and_private(path: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && metadata.uid() == rustix::process::geteuid().as_raw()
            && metadata.permissions().mode() & 0o777 == 0o600
    })
}

#[cfg(not(unix))]
fn owned_and_private(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// A fork's log as it is built: its frames, encoded, and the state they
/// reduce to.
struct Log {
    id: SessionId,
    generation: String,
    next_seq: u64,
    event_ids: BTreeSet<String>,
    state: DurableState,
    bytes: Vec<u8>,
}

impl Log {
    fn new(id: SessionId) -> Self {
        Self {
            id,
            generation: new_identifier(),
            next_seq: 1,
            event_ids: BTreeSet::new(),
            state: DurableState::empty(),
            bytes: Vec::new(),
        }
    }

    /// Frames `event` as the next frame, at `timestamp_ms`, and reduces it.
    fn push(&mut self, timestamp_ms: i64, event: SessionEvent) -> Result<(), SessionError> {
        let corrupt = |detail: String| SessionError::Corrupt {
            id: self.id.as_str().to_string(),
            detail,
        };
        let mut event_id = new_identifier();
        while self.event_ids.contains(&event_id) {
            event_id = new_identifier();
        }
        let envelope = EventEnvelope {
            schema_version: EVENT_SCHEMA_VERSION,
            log_generation: self.generation.clone(),
            seq: self.next_seq,
            event_id,
            timestamp_ms,
            event,
        };
        let line = envelope.encode().map_err(|err| corrupt(err.to_string()))?;
        if (self.bytes.len() + line.len()) as u64 > MAX_EVENT_LOG_BYTES {
            return Err(corrupt(format!(
                "the fork's event log would exceed its {MAX_EVENT_LOG_BYTES}-byte limit"
            )));
        }
        apply(&mut self.state, &envelope).map_err(corrupt)?;
        self.bytes.extend_from_slice(line.as_bytes());
        self.event_ids.insert(envelope.event_id);
        self.next_seq += 1;
        Ok(())
    }
}

/// The folder a fork is built in, removed with everything in it unless it
/// was renamed into place.
///
/// Its name is the fork's fresh id, so it is never another process's.
struct Stage {
    path: PathBuf,
    placed: bool,
}

impl Stage {
    fn create(path: PathBuf) -> Result<Self, SessionError> {
        if let Err(err) = create_private_dir(&path) {
            return Err(io_error(&path, err));
        }
        let stage = Self {
            path,
            placed: false,
        };
        set_private_dir_mode(&stage.path)?;
        Ok(stage)
    }

    fn placed(mut self) {
        self.placed = true;
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.placed {
            // Only ever this fork's own folder; `remove_dir_all` does not
            // follow a link it meets inside it.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests;
