//! `/sound` on this surface: the two switches, the bells they owe, and the
//! save that keeps them.
//!
//! What a level *is* and how it is written down are [`crate::sound`]'s. This
//! module is the part only a session has: **when** a bell is owed, and where
//! the save waits.
//!
//! **When.** Upstream rings for two events (`src/builtins/hooks/notifications.zig:156-240`):
//!
//! * a turn's end -- one that answered or failed, never one the user
//!   interrupted (`postTurnEndHandler`: `.interrupted, .paused => return`) --
//!   and **only once the answer has been presented**: the turn-end cue waits in
//!   a queue while the pacer still holds text (`State.queue`, `takeReady`), so
//!   the bell rings when the reader reaches the end, not when the provider did;
//! * a question opening -- a permission prompt or a batch of the model's
//!   questions (`app_worker_runtime.zig:766-767`, `:1089-1097`) -- at once, and
//!   always as a terminal bell even where a chime also plays
//!   (`Player.playAttention`, `sound.zig:167-173`), so a multiplexer marks the
//!   pane.
//!
//! Each switch gates its own event, read at the moment the event happens.
//!
//! **Where the save waits.** Upstream saves on the thread that ran the command.
//! Here that is the UI thread, which sits in `pselect(2)` holding the terminal,
//! and a settings write ends in two `fsync`s -- on a slow or networked home, a
//! stall the band would show as a frozen screen. So saves go to one writer
//! thread of their own, **one** so that two `/sound`s in a row land in the
//! order they were typed, and how each went comes back on a channel the UI
//! reads once a turn of its loop, the way [`super::copy`] does.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use crate::config::Notifications;
use crate::sound::Level;

/// The name the writer thread is given, for a debugger or a panic message.
const THREAD_NAME: &str = "xfx-settings";

/// Why a save could not even be started when there is no file to save to.
///
/// Not reachable from a real session -- the TUI refuses to open without a home
/// -- and the answer for a shell a test built, which is given no settings file
/// unless it asks for one ([`Sound::new`]).
const NO_SETTINGS_FILE: &str = "no home directory is set";

/// One save, as the writer thread is handed it.
#[derive(Debug)]
struct Order {
    /// Which save this is, so the band can say how it went where it held the
    /// level for it.
    id: u64,
    level: Level,
    /// Whether the switches in force moved, which is what the failure's words
    /// depend on ([`crate::sound::not_saved`]).
    changed: bool,
}

/// How one save went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Saved {
    /// The id [`Sound::set`] answered with.
    pub(crate) id: u64,
    pub(crate) changed: bool,
    /// The error's own words when it failed.
    pub(crate) outcome: Result<(), String>,
}

/// The writer thread, once there is one.
#[derive(Debug)]
struct Writer {
    orders: Sender<Order>,
    thread: JoinHandle<()>,
}

/// The switches, the bells, and the saves.
#[derive(Debug)]
pub(crate) struct Sound {
    notifications: Notifications,
    /// Turn ends whose answer is still being presented, each a bell once it
    /// has been.
    presenting: u16,
    /// Bells the loop owes the terminal (`super::event_loop`'s
    /// `commit_bells`).
    owed: u16,
    /// Whether the user stopped the work in hand, so its end rings nothing.
    interrupted: bool,
    /// The file the switches are saved into.
    settings: Option<PathBuf>,
    /// Started on the first save: a session that never types `/sound` has no
    /// thread for it.
    writer: Option<Writer>,
    /// Cloned into the writer, which sends one [`Saved`] per order.
    done: Sender<Saved>,
    saved: Receiver<Saved>,
    /// The next save's id.
    next: u64,
}

impl Sound {
    pub(crate) fn new(notifications: Notifications, settings: Option<PathBuf>) -> Self {
        let (done, saved) = mpsc::channel();
        Self {
            notifications,
            presenting: 0,
            owed: 0,
            interrupted: false,
            settings,
            writer: None,
            done,
            saved,
            next: 1,
        }
    }

    /// The switches in force.
    pub(crate) fn notifications(&self) -> Notifications {
        self.notifications
    }

    /// Moves both switches to `level` and starts saving it.
    ///
    /// Answers whether the switches moved, and -- when the save could not
    /// even be started -- why, so the caller can say so in upstream's order:
    /// the failure first, then the level. A save that was started is answered
    /// with its id, and says how it went later, under that id
    /// ([`Self::saved`]), for the caller to say it before the level then.
    pub(crate) fn set(&mut self, level: Level) -> (bool, Result<u64, String>) {
        let next = level.notifications();
        let changed = next != self.notifications;
        self.notifications = next;
        let id = self.next;
        let started = self.save(Order { id, level, changed }).map(|()| {
            self.next += 1;
            id
        });
        (changed, started)
    }

    fn save(&mut self, order: Order) -> Result<(), String> {
        let Some(settings) = self.settings.clone() else {
            return Err(NO_SETTINGS_FILE.to_string());
        };
        if self.writer.is_none() {
            self.writer = Some(spawn_writer(settings, self.done.clone())?);
        }
        let writer = self.writer.as_ref().expect("the writer was just started");
        writer.orders.send(order).map_err(|_| {
            // The thread is gone, which only a panic in it does: it is not
            // started again, because whatever killed it would kill the next.
            "the settings writer stopped".to_string()
        })
    }

    /// How one save that has finished went, or `None` when none has since the
    /// last call. In the order they were typed, because one thread wrote them.
    pub(crate) fn saved(&self) -> Option<Saved> {
        self.saved.try_recv().ok()
    }

    /// [`Self::saved`], waiting up to `limit` for one: for the exit and a
    /// reset, which have the level to say and the screen to hand on.
    pub(crate) fn wait(&self, limit: std::time::Duration) -> Option<Saved> {
        self.saved.recv_timeout(limit).ok()
    }

    /// A turn -- or a piece of work that ran none (`ran` is `false`) -- has
    /// ended. Owed once its answer has been presented ([`Self::presented`]).
    pub(crate) fn turn_ended(&mut self, ran: bool) {
        let interrupted = std::mem::take(&mut self.interrupted);
        if ran && !interrupted && self.notifications.turn_end {
            self.presenting = self.presenting.saturating_add(1);
        }
    }

    /// The user stopped the work in hand: its end is theirs, and rings nothing.
    pub(crate) fn interrupted(&mut self) {
        self.interrupted = true;
    }

    /// Everything the pacer held has been presented: the turn ends waiting on
    /// that are bells now.
    pub(crate) fn presented(&mut self) {
        self.owed = self
            .owed
            .saturating_add(std::mem::take(&mut self.presenting));
    }

    /// A question has opened and the session is waiting on the user.
    pub(crate) fn attention(&mut self) {
        if self.notifications.attention_required {
            self.owed = self.owed.saturating_add(1);
        }
    }

    /// The bells owed, taken by the loop that writes them.
    pub(crate) fn take(&mut self) -> u16 {
        std::mem::take(&mut self.owed)
    }

    /// Bells the terminal refused without taking a byte, owed again.
    pub(crate) fn restore(&mut self, rings: u16) {
        self.owed = self.owed.saturating_add(rings);
    }

    /// The bells owed, without taking them.
    #[cfg(test)]
    pub(crate) fn owed(&self) -> u16 {
        self.owed
    }
}

impl Drop for Sound {
    /// Lets a save in flight land before the session is gone.
    ///
    /// The write is a staged file and a rename, so a process that left in the
    /// middle of one would leave the old file and a stage behind rather than
    /// a broken file -- but it would leave the switch the user just set
    /// unsaved. Closing the channel ends the thread once it has written what
    /// it was given.
    ///
    /// The cost is that an exit waits for a save in flight -- two `fsync`s,
    /// in the one place a wait is the right answer. The thread itself takes
    /// no signal ([`super::signals::spawn_unsignalled`]): the wait is for the
    /// save, not for `super::signals::release`.
    fn drop(&mut self) {
        if let Some(Writer { orders, thread }) = self.writer.take() {
            drop(orders);
            let _ = thread.join();
        }
    }
}

/// Starts the one thread that writes this session's saves, in order.
fn spawn_writer(settings: PathBuf, done: Sender<Saved>) -> Result<Writer, String> {
    let (orders, received) = mpsc::channel::<Order>();
    let thread = super::signals::spawn_unsignalled(THREAD_NAME, move || {
        for order in received {
            let outcome = crate::sound::save(&settings, order.level).map_err(|err| err.to_string());
            // Nobody left to tell is a session that has ended, and the file
            // was written for it all the same.
            let _ = done.send(Saved {
                id: order.id,
                changed: order.changed,
                outcome,
            });
        }
    })
    .map_err(|err| format!("no thread to save with: {err}"))?;
    Ok(Writer { orders, thread })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Waits for one save to finish, or fails the case after `limit`.
    fn saved(sound: &Sound, limit: Duration) -> Saved {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(saved) = sound.saved() {
                return saved;
            }
            assert!(Instant::now() < deadline, "the save never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn quiet() -> Sound {
        Sound::new(Notifications::both(false), None)
    }

    fn loud() -> Sound {
        Sound::new(Notifications::both(true), None)
    }

    #[test]
    fn a_turn_end_is_owed_only_once_its_answer_has_been_presented() {
        let mut sound = loud();
        sound.turn_ended(true);
        assert_eq!(sound.take(), 0, "rang before the reader reached the end");
        sound.presented();
        assert_eq!(sound.take(), 1);
        // And once.
        sound.presented();
        assert_eq!(sound.take(), 0);
    }

    #[test]
    fn work_that_ran_no_turn_rings_nothing() {
        // `/model`, `/setup` and a catalog load end with the turn's own event.
        let mut sound = loud();
        sound.turn_ended(false);
        sound.presented();
        assert_eq!(sound.take(), 0);
    }

    #[test]
    fn an_interrupted_turn_rings_nothing_and_the_next_one_does() {
        let mut sound = loud();
        sound.interrupted();
        sound.turn_ended(true);
        sound.presented();
        assert_eq!(sound.take(), 0, "the user's own stop rang");
        sound.turn_ended(true);
        sound.presented();
        assert_eq!(sound.take(), 1, "the interrupt outlived its turn");
    }

    #[test]
    fn a_question_is_owed_at_once() {
        let mut sound = loud();
        sound.attention();
        assert_eq!(sound.take(), 1);
    }

    #[test]
    fn each_switch_gates_its_own_event() {
        let mut quiet = quiet();
        quiet.turn_ended(true);
        quiet.presented();
        quiet.attention();
        assert_eq!(quiet.take(), 0);
        let mut ends_only = Sound::new(
            Notifications {
                turn_end: true,
                attention_required: false,
            },
            None,
        );
        ends_only.attention();
        assert_eq!(ends_only.take(), 0);
        ends_only.turn_ended(true);
        ends_only.presented();
        assert_eq!(ends_only.take(), 1);
    }

    #[test]
    fn a_refused_bell_is_owed_again() {
        let mut sound = loud();
        sound.attention();
        sound.attention();
        let taken = sound.take();
        assert_eq!(taken, 2);
        sound.restore(taken);
        assert_eq!(sound.owed(), 2);
    }

    #[test]
    fn setting_a_level_moves_both_switches_and_says_whether_they_moved() {
        let mut sound = quiet();
        let (changed, started) = sound.set(Level::On);
        assert!(changed);
        assert_eq!(started, Err(NO_SETTINGS_FILE.to_string()));
        assert_eq!(sound.notifications(), Notifications::both(true));
        let (changed, _) = sound.set(Level::On);
        assert!(!changed, "the same level again moved nothing");
    }

    #[test]
    fn saves_land_in_the_file_in_the_order_they_were_typed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(".xfx").join("settings.json");
        let mut sound = Sound::new(Notifications::both(false), Some(path.clone()));
        for (id, level) in [Level::On, Level::Off, Level::On, Level::Off, Level::On]
            .into_iter()
            .enumerate()
        {
            assert_eq!(sound.set(level).1, Ok(id as u64 + 1));
        }
        for id in 1..=5 {
            let saved = saved(&sound, Duration::from_secs(10));
            assert_eq!((saved.id, saved.outcome), (id, Ok(())));
        }
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            written,
            serde_json::json!({"notifications": {"turn_end": true, "attention_required": true}}),
            "the last level typed is the one the file keeps"
        );
    }

    #[test]
    fn a_save_that_failed_says_so_with_the_errors_words() {
        let dir = crate::profile_home::temp_home();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").expect("seed");
        let mut sound = Sound::new(Notifications::both(false), Some(path.clone()));
        assert_eq!(sound.set(Level::On).1, Ok(1));
        let saved = saved(&sound, Duration::from_secs(10));
        assert!(saved.changed);
        let reason = saved
            .outcome
            .expect_err("an unreadable file is not written");
        assert!(reason.contains("not valid JSON"), "{reason}");
        assert_eq!(std::fs::read(&path).expect("read"), b"{ not json");
    }

    #[test]
    fn a_session_that_ends_lets_its_last_save_land() {
        let dir = crate::profile_home::temp_home();
        let path = dir.path().join("settings.json");
        let mut sound = Sound::new(Notifications::both(true), Some(path.clone()));
        assert_eq!(sound.set(Level::Off).1, Ok(1));
        drop(sound);
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("landed")).expect("json");
        assert_eq!(written["notifications"]["turn_end"], false);
    }
}
