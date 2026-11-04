//! `/permissions` and Shift-Tab on this surface: the notices, the change the
//! runtime applies, the grants a bare `/permissions` panel lists, and the
//! full-access warning's clock.
//!
//! What the command means and the words it says are
//! [`crate::permission::slash`]'s, shared with the line shell. What is this
//! module's is the shape upstream's TUI gives them -- transcript notices with a
//! topic and a tone (`writeDomainNotice`), and the one-time warning on the hint
//! row (`app_permission_runtime.zig:20-67`). A bare `/permissions` is no
//! notice: it opens the band's info panel, whose rows are
//! `super::info_panel`'s (`permissions_rows`). Where they happen is
//! `super::shell`'s (the notices, the panel, the key, the hint row) and
//! `super::worker`'s (the live authority every decision reads, the grants it
//! publishes, and the record of each change: the session log and the settings
//! write).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::notice::Notice;
use crate::config::PermissionMode;
use crate::permission::allowlist::Tone;
use crate::permission::session_rules::{self, Applied, State};
use crate::permission::slash::{self, TOPIC};

/// The warning the hint row carries the first time a person is in full access
/// (`yolo_warning_text`, `src/core/permissions/permissions.zig:100`), with this
/// product's name where upstream says its own.
pub(crate) const WARNING: &str = "Full access enabled: xfx permission checks disabled";

/// The same warning, for a row too narrow for the sentence
/// (`yolo_warning_compact_text`, `app_permission_runtime.zig:20`).
pub(crate) const WARNING_COMPACT: &str = "Full access";

/// How long the warning stays on the screen, counted only while it is on it
/// (`yolo_warning_visible_ms`, `app_permission_runtime.zig:21`).
pub(crate) const WARNING_VISIBLE: Duration = Duration::from_millis(4000);

/// A change to the session's authority.
///
/// **In force the moment it is made**, including for the next tool call of a
/// turn that is already running, as upstream's are
/// (`yolo-permission-mode.test.ts:373-450`): the handle that sends it writes
/// the live authority first (`super::worker`'s `WorkHandle::change_permissions`).
/// What the runtime does with the message, between pieces of work where the
/// session log is not held by a turn, is **write it down** -- configuration,
/// log, profile -- in the order the person made the changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// `/permissions <mode>` or Shift-Tab: this mode from the next decision
    /// on, and recorded as the profile's `permission_mode`.
    Mode(PermissionMode),
    /// `/permissions reset`: `ask`, every grant given so far out of force at
    /// once, and the forgetting recorded in the session log so a resume does
    /// not bring them back.
    Reset,
    /// The full-access warning has been on the screen: record
    /// `yolo_acknowledged`, so it is shown once per person and not once per
    /// session.
    Acknowledge,
}

/// What setting a mode says: neutral, and a warning for the one that skips
/// every check (`session_commands.zig:373-383`).
pub(crate) fn selected(mode: PermissionMode) -> Notice {
    match mode {
        PermissionMode::Yolo => Notice::warning(TOPIC, slash::selected(mode)),
        PermissionMode::Ask | PermissionMode::Auto => Notice::neutral(TOPIC, slash::selected(mode)),
    }
}

/// What `reset` says.
pub(crate) fn reset() -> Notice {
    Notice::neutral(TOPIC, slash::RESET)
}

/// What an argument nobody understands is answered with: the usage line, as an
/// error with no topic (`session_commands.zig:393`).
pub(crate) fn usage() -> Notice {
    Notice::error("", slash::USAGE)
}

/// What a settings write that failed says, under upstream's `label`.
pub(crate) fn not_saved(label: &str, reason: &str) -> Notice {
    Notice::error(label, slash::not_saved(reason))
}

/// One "always" the live conversation holds, as a bare `/permissions` lists it:
/// the tool, and its target as a listing says it -- a file relative to the
/// workspace ([`slash::grant_target`]).
///
/// **Not made safe here**: a command a model wrote is in `target` as the grant
/// holds it, and the panel's rows are where every text is made safe for the
/// terminal (`super::info_panel::permissions_rows`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Granted {
    pub(crate) tool: String,
    pub(crate) target: String,
}

/// The conversation's grants as a bare `/permissions` lists them, kept where
/// the UI thread can read them without asking the runtime.
///
/// Published by the runtime wherever it publishes `/status`'s snapshot
/// (`super::worker`'s `Runtime::publish`), for the reason that one is: the
/// grants live on the runtime thread, and the UI may not wait for a thread that
/// can be a minute into a turn. Mid-turn the list is the one the turn before
/// left.
///
/// Stamped with the reset count it was made under, so a reader that knows of
/// a later reset (`crate::permission::LiveAuthority`) can tell the list is out
/// of force without waiting for the runtime to say so again.
#[derive(Debug, Clone, Default)]
pub(crate) struct Grants(Arc<Mutex<(u64, Vec<Granted>)>>);

impl Grants {
    /// Replaces what the next bare `/permissions` lists, as of reset count
    /// `resets`.
    pub(crate) fn publish(&self, resets: u64, grants: Vec<Granted>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = (resets, grants);
    }

    /// The list the runtime last published, and the reset count it was made
    /// under.
    pub(crate) fn read(&self) -> (u64, Vec<Granted>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The session's saved-session permission rules, wherever they are now.
///
/// **Before the session's first save they are held here, in memory**, as
/// upstream holds them: its session exists from launch, a confirmed
/// `remember` or `revoke` goes into the runtime's state at once
/// (`replacePermissionStateOwned`, `input_approval_runtime.zig:289-293`)
/// and into the session's held lines, which only its first turn writes
/// (`commitPermissionState` -> `setPermissions` -> `hold`,
/// `app_session_runtime.zig:4961-4968`, `session_adapter.zig:2205-2209`,
/// `src/core/session_manager/session.zig:415-429@481104fd`). xfx's session
/// opens at the first prompt -- or a `/rename` before one -- and the runtime
/// writes what is held into the new session's directory before its log
/// exists, then lets go of it here, under this one lock
/// (`super::worker`'s `Runtime::open_next`), so a confirmation landing
/// between the two cannot be held after the write and lost.
///
/// **Once the session is open the store is its file**, under its own lock, and
/// every decision reads the file afresh; so the UI writing it while a turn
/// runs is the change upstream's confirmation makes, in force for that turn's
/// next call. Published beside [`Grants`], for the same reason: the
/// conversation is the runtime thread's, and the UI may not wait for it.
#[derive(Debug, Clone)]
pub(crate) struct SavedRules(Arc<Mutex<Slot>>);

/// What [`SavedRules`] holds.
#[derive(Debug)]
pub(crate) struct Slot {
    /// The file the rules are kept in: the open session's, or the one the
    /// next save creates (`<sessions>/<next id>/permissions.json`). A
    /// confirmation names it ([`slash::Confirmation::path`]) and is made only
    /// against the store it names.
    at: PathBuf,
    /// The rules while the session is not open; `None` once it is, and the
    /// file is the store.
    held: Option<State>,
    /// Moves with every change of what this holds, so a reader that keeps a
    /// view -- the bare `/permissions` panel -- can tell it moved.
    changes: u64,
}

/// The store as it stands, read under one lock: where it is, and its rules --
/// or why its file could not be read.
#[derive(Debug, Clone)]
pub(crate) struct View {
    pub(crate) at: PathBuf,
    pub(crate) state: Result<State, String>,
}

/// Whether the store moved since a [`View`] was read: the slot's change count,
/// and, for an open session's file, its stamp ([`session_rules::stamp`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct StoreStamp {
    changes: u64,
    file: Option<session_rules::Stamp>,
}

/// A handle with no runtime behind it: a session not open yet, whose store
/// no runtime will create.
#[cfg(test)]
impl Default for SavedRules {
    fn default() -> Self {
        Self::pending(PathBuf::from("unopened").join(session_rules::FILE_NAME))
    }
}

impl SavedRules {
    /// The rules of a session that is not open yet, whose store will be `at`:
    /// none.
    pub(crate) fn pending(at: PathBuf) -> Self {
        Self(Arc::new(Mutex::new(Slot {
            at,
            held: Some(State::default()),
            changes: 0,
        })))
    }

    /// The rules of the open session whose store is `at`.
    pub(crate) fn open(at: PathBuf) -> Self {
        Self(Arc::new(Mutex::new(Slot {
            at,
            held: None,
            changes: 0,
        })))
    }

    fn slot(&self) -> MutexGuard<'_, Slot> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The slot itself, for the runtime's open of the session it is held for
    /// ([`Slot::first_save`], [`Slot::opened`]).
    pub(crate) fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot()
    }

    /// What the store is now.
    pub(crate) fn view(&self) -> View {
        let slot = self.slot();
        View {
            at: slot.at.clone(),
            state: match &slot.held {
                Some(state) => Ok(state.clone()),
                None => session_rules::load(&slot.at),
            },
        }
    }

    /// How the store stands, for telling whether it moved.
    pub(crate) fn stamp(&self) -> StoreStamp {
        let slot = self.slot();
        StoreStamp {
            changes: slot.changes,
            file: slot.held.is_none().then(|| session_rules::stamp(&slot.at)),
        }
    }

    /// Makes a confirmed `remember` or `revoke` (`submitRuleManagementChoice`,
    /// `input_approval_runtime.zig:277-323@481104fd`): in memory while the
    /// session is held, in force the moment it is, as upstream's is; in the
    /// file once it is open ([`slash::confirm`]). A confirmation for a store
    /// this no longer is -- the session it was shown for was left by `/new`
    /// or a resume, or the store refused it and the next one waits under
    /// another id -- is not made anywhere, and says so as the store's own
    /// move under a confirmation says it ([`slash::CHANGED`]).
    pub(crate) fn confirm(&self, confirmation: &slash::Confirmation) -> (Tone, String) {
        let mut slot = self.slot();
        if slot.at != confirmation.path {
            return (Tone::Warning, slash::CHANGED.to_string());
        }
        let Some(held) = slot.held.as_mut() else {
            return slash::confirm(confirmation);
        };
        let applied = held.apply(&confirmation.event);
        if let Applied::Applied(next) = &applied {
            *held = next.clone();
            slot.changes += 1;
        }
        slash::answer(Ok(applied))
    }

    /// `/new`, `/clear` or `/reset`, at the keystroke: what is held is the
    /// session being left, and goes with it, as upstream's reset clears the
    /// state it would have written (`SessionRuntime.reset` ->
    /// `clearPermissionState`, `src/core/session/session.zig:1430-1437`, from
    /// `applyIdleLiveSessionTransition`, `app_session_runtime.zig:1816@481104fd`).
    /// An open session's rules stay in its file; the runtime moves this to
    /// the next session's once it has dropped that one
    /// ([`Self::publish_pending`]).
    pub(crate) fn forget_held(&self) {
        let mut slot = self.slot();
        if let Some(held) = slot.held.as_mut() {
            if *held != State::default() {
                *held = State::default();
                slot.changes += 1;
            }
        }
    }

    /// The runtime's word that the open session's store is `at`: a resume, or
    /// the session the first save opened. What was held for a session that
    /// was not opened -- a resume replaces it -- goes, as upstream's resume
    /// resets it (`prepareLiveSessionResume`, `app_session_runtime.zig:1744-1746`,
    /// `:1816@481104fd`).
    pub(crate) fn publish_open(&self, at: PathBuf) {
        let mut slot = self.slot();
        if slot.held.is_some() || slot.at != at {
            slot.at = at;
            slot.held = None;
            slot.changes += 1;
        }
    }

    /// The runtime's word that no session is open and the next save opens
    /// the one whose store is `at`. Coming from an open one -- `/new` dropped
    /// it -- nothing is held. Already held, what is held is kept, under the
    /// new id when the store refused the last one: the next attempt writes it.
    pub(crate) fn publish_pending(&self, at: PathBuf) {
        let mut slot = self.slot();
        let mut moved = false;
        if slot.held.is_none() {
            slot.held = Some(State::default());
            moved = true;
        }
        if slot.at != at {
            slot.at = at;
            moved = true;
        }
        if moved {
            slot.changes += 1;
        }
    }
}

impl Slot {
    /// The rules to write into the session whose store is `at`, which is
    /// about to be created, and from here on the store this names: `None`
    /// when nothing has been held -- no file is written, which reads as no
    /// rules -- and once something has, the whole state, so a generation a
    /// revoke spent is never handed out again.
    pub(crate) fn first_save(&mut self, at: &Path) -> Option<State> {
        if self.at != at {
            self.at = at.to_path_buf();
            self.changes += 1;
        }
        self.held
            .as_ref()
            .filter(|held| **held != State::default())
            .cloned()
    }

    /// The session whose store is `at` was created, with what
    /// [`Self::first_save`] gave it: its file is the store now.
    pub(crate) fn opened(&mut self, at: PathBuf) {
        self.at = at;
        self.held = None;
        self.changes += 1;
    }
}

/// The full-access warning's clock (`YoloWarning`,
/// `app_permission_runtime.zig:28-67`).
///
/// Armed when a session enters full access before the person has been shown
/// the warning, and good for [`WARNING_VISIBLE`] of **time on the screen**: the
/// clock runs only while the row really carries it, so a question, a menu or an
/// armed Escape that takes the row pauses it rather than spending it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Warning {
    active: bool,
    remaining: Duration,
    since: Option<Instant>,
}

impl Default for Warning {
    fn default() -> Self {
        Self {
            active: false,
            remaining: WARNING_VISIBLE,
            since: None,
        }
    }
}

impl Warning {
    /// Starts the warning afresh.
    pub(crate) fn arm(&mut self) {
        *self = Self {
            active: true,
            ..Self::default()
        };
    }

    /// Takes it away, spent or not.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether it still has time on the screen to spend.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// One look at the clock: `shown` is whether the row on the screen carries
    /// the warning now. Answers whether the warning has **just** run out, which
    /// is the one moment the row owes a frame without it.
    pub(crate) fn observe(&mut self, now: Instant, shown: bool) -> bool {
        if !self.active {
            return false;
        }
        match (shown, self.since) {
            (true, None) => {
                self.since = Some(now);
                false
            }
            (true, Some(started)) => {
                if now.saturating_duration_since(started) < self.remaining {
                    return false;
                }
                self.clear();
                true
            }
            (false, Some(started)) => {
                self.remaining = self
                    .remaining
                    .saturating_sub(now.saturating_duration_since(started));
                self.since = None;
                if self.remaining.is_zero() {
                    self.clear();
                }
                false
            }
            (false, None) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::notice::Tone;
    use super::*;

    #[test]
    fn each_answer_is_said_in_upstreams_words_and_tone() {
        let auto = selected(PermissionMode::Auto);
        assert_eq!(
            (auto.tone, auto.body.as_str()),
            (Tone::Neutral, "mode set to auto")
        );
        let ask = selected(PermissionMode::Ask);
        assert_eq!(
            (ask.tone, ask.body.as_str()),
            (Tone::Neutral, "mode set to ask")
        );
        let full = selected(PermissionMode::Yolo);
        assert_eq!(
            (full.tone, full.topic.as_str(), full.body.as_str()),
            (Tone::Warning, "permissions", "mode set to full access")
        );
        let reset = reset();
        assert_eq!(
            (reset.tone, reset.body.as_str()),
            (
                Tone::Neutral,
                "permissions reset to ask, session grants cleared"
            )
        );
        let usage = usage();
        assert_eq!(
            (usage.tone, usage.topic.as_str(), usage.body.as_str()),
            (Tone::Error, "", slash::USAGE)
        );
        assert!(usage
            .body
            .starts_with("usage: /permissions [ask|auto|full-access|reset]\n"));
        let failed = not_saved("permission-mode", "no home directory");
        assert_eq!(
            (failed.tone, failed.topic.as_str()),
            (Tone::Error, "permission-mode")
        );
    }

    #[test]
    fn the_board_hands_back_what_was_published_last() {
        let board = Grants::default();
        assert_eq!(board.read(), (0, Vec::new()));
        let granted = Granted {
            tool: "write_file".to_string(),
            target: "a".to_string(),
        };
        board.publish(0, vec![granted.clone()]);
        let reader = board.clone();
        assert_eq!(reader.read(), (0, vec![granted]));
        board.publish(3, Vec::new());
        assert_eq!(reader.read(), (3, Vec::new()));
    }

    #[test]
    fn the_warning_spends_only_the_time_it_is_on_the_screen() {
        let start = Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut warning = Warning::default();
        assert!(!warning.active());
        assert!(
            !warning.observe(at(0), true),
            "an unarmed warning never runs out"
        );

        warning.arm();
        assert!(warning.active());
        // Not on the screen yet: nothing is spent.
        assert!(!warning.observe(at(0), false));
        assert!(!warning.observe(at(10_000), false));
        // Shown for a second, hidden for an hour, shown again.
        assert!(!warning.observe(at(10_000), true));
        assert!(!warning.observe(at(11_000), true));
        assert!(!warning.observe(at(11_000), false));
        assert!(!warning.observe(at(3_611_000), false));
        assert!(!warning.observe(at(3_611_000), true));
        assert!(
            !warning.observe(at(3_613_999), true),
            "4 s are not spent yet"
        );
        assert!(warning.observe(at(3_614_000), true), "and now they are");
        assert!(!warning.active());
        assert!(!warning.observe(at(3_615_000), true), "it runs out once");

        warning.arm();
        warning.clear();
        assert!(!warning.active());
    }
}
