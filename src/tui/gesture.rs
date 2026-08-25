//! What a repeated keystroke means.
//!
//! Two keys in this phase, and both of them mean something different the second
//! time: Ctrl-C and Escape. What makes them a module rather than two fields on
//! the shell is that the *second* meaning is the dangerous one -- one leaves the
//! session with 130, the other throws a draft away -- and a rule that decides
//! them has to be readable on its own, with a clock handed to it rather than
//! read inside it.
//!
//! # Ctrl-C is the line shell's rule, arriving as a byte
//!
//! With `ISIG` cleared the terminal generates no `SIGINT`, so the only Ctrl-C a
//! TUI session ever sees is the byte `0x03` coming out of the decoder. What it
//! means is nevertheless exactly what it means in `xfx`'s line-oriented shell,
//! whose policy this reproduces (`crate::interactive`'s `Interrupts::signalled`,
//! `src/interactive.rs:275-302`):
//!
//! | what is happening | first Ctrl-C | the one after it |
//! |---|---|---|
//! | a turn is running | cancel the turn ([`Interrupt::Cancel`]) | leave with 130 ([`Interrupt::Leave`]) |
//! | the prompt is idle | clear the draft ([`Interrupt::Clear`]) | leave with 130, **within [`EXIT_WINDOW`]** |
//!
//! The two rows differ in one way that is deliberate rather than an oversight.
//! **The idle chain expires and the cancelled-turn one does not.** An idle
//! Ctrl-C is a keystroke the user may repeat minutes apart for unrelated
//! reasons, and upstream bounds exactly that pair with a three-second window
//! (`gesture_state.zig:3-4`); a Ctrl-C at a turn that has *already been asked to
//! stop* is the user saying they will not wait for it, and putting a clock on
//! that would mean a turn wedged for four seconds could no longer be walked away
//! from -- which is the one moment the gesture exists for. The line shell has no
//! clock on that arm either.
//!
//! [`Gestures::submitted`] and [`Gestures::turn_ended`] are what end a chain: a
//! line the user really sent, or a turn that finished, is a session that is
//! going somewhere, so the next Ctrl-C starts again from the first column of
//! the table. The second of the two is what keeps the right-hand column from
//! outliving the turn it was about -- a Ctrl-C that stopped *yesterday's*
//! answer must not be the reason today's first one exits the session.
//!
//! **What a cancellation reaches is stated here because the gesture is where a
//! user decides it**: it stops the turn that is running *and* drops everything
//! queued behind it (`super::worker`'s `abandon_pending`). One turn at a time
//! and one prompt waiting is a promise about typing ahead, not about what
//! survives an interrupt -- a queue that started running a moment after the
//! user asked everything to stop would be the same surprise the band says
//! `queued 1` to prevent.
//!
//! # Escape is a two-tap because one tap has no meaning here
//!
//! A lone Escape does nothing -- there is no mode to leave and no selection to
//! drop -- so it arms, and a second one inside [`CLEAR_WINDOW`] clears the
//! composer. The window is short on purpose: an Escape armed for three seconds
//! would turn an unrelated later Escape into the loss of a draft.
//!
//! Nothing here reads a clock. Every decision takes the `now` its caller already
//! has, which is what lets the whole table be tested without sleeping, and what
//! keeps one read's worth of bytes from expiring each other -- the bytes of a
//! single read arrived together (`super::shell::Shell::route_bytes`).

use std::time::{Duration, Instant};

/// How long a first Ctrl-C at an idle prompt arms the second one for
/// (`gesture_state.zig:3-4`).
pub(crate) const EXIT_WINDOW: Duration = Duration::from_millis(3000);

/// How long a first Escape arms the second one for.
pub(crate) const CLEAR_WINDOW: Duration = Duration::from_millis(500);

/// What a process that stopped because it was interrupted exits with.
///
/// The same number `crate::interactive` exits with on the line-oriented path,
/// spelled again here because that one is private to a module this task does not
/// own -- and as a `u8` because this side hands it to `ExitCode::from`.
pub(crate) const INTERRUPTED_EXIT_CODE: u8 = 130;

/// What one Ctrl-C means.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Interrupt {
    /// Stop the turn that is running. The session goes on.
    Cancel,
    /// Throw the draft away. Nothing was running.
    Clear,
    /// End the session with [`INTERRUPTED_EXIT_CODE`].
    Leave,
}

/// What one Escape means.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Escape {
    /// Nothing yet: a second Escape inside [`CLEAR_WINDOW`] will clear the
    /// composer, and until then the hint row says so.
    Armed,
    /// Clear the composer.
    Clear,
}

/// The session's memory of the keystroke before this one.
#[derive(Debug, Default)]
pub(crate) struct Gestures {
    /// When the last idle Ctrl-C was, while it still arms the next one.
    ///
    /// `None` between chains: cleared by [`Gestures::submitted`] and by the
    /// interrupt that acts on it, so a chain is at most two keystrokes long and
    /// a third one starts a new chain rather than continuing an old one.
    interrupted_at: Option<Instant>,
    /// Whether the turn that is running has already been asked to stop.
    ///
    /// Not an `Instant`: see the module header for why this arm carries no
    /// clock.
    asked_to_stop: bool,
    /// When the Escape that armed the next one was, while it still does.
    escaped_at: Option<Instant>,
}

impl Gestures {
    /// What this Ctrl-C means, given whether the runtime has work in hand.
    pub(crate) fn interrupt(&mut self, now: Instant, running: bool) -> Interrupt {
        if running {
            // The idle chain is not continued by a turn's interrupt and does not
            // continue into one: the two columns of the table are different
            // gestures that happen to share a key.
            self.interrupted_at = None;
            if self.asked_to_stop {
                return Interrupt::Leave;
            }
            self.asked_to_stop = true;
            return Interrupt::Cancel;
        }
        self.asked_to_stop = false;
        if self.armed(self.interrupted_at, now, EXIT_WINDOW) {
            self.interrupted_at = None;
            return Interrupt::Leave;
        }
        self.interrupted_at = Some(now);
        Interrupt::Clear
    }

    /// What this Escape means.
    pub(crate) fn escape(&mut self, now: Instant) -> Escape {
        if self.armed(self.escaped_at, now, CLEAR_WINDOW) {
            self.escaped_at = None;
            return Escape::Clear;
        }
        self.escaped_at = Some(now);
        Escape::Armed
    }

    /// Whether a second Escape would clear the composer right now.
    ///
    /// Read by the band so the hint row can say so, and read with the same
    /// window the decision uses rather than with a copy of it: a row that
    /// promised `esc again to clear` after the window had closed would be
    /// advertising a keystroke that no longer does anything.
    pub(crate) fn escape_armed(&self, now: Instant) -> bool {
        self.armed(self.escaped_at, now, CLEAR_WINDOW)
    }

    /// A turn is over: whatever the last Ctrl-C was about is over with it.
    ///
    /// The line shell's `end_turn` (`interactive.rs:257-259`), and it is not
    /// belt-and-braces. Without it the *cancelled* half of a chain outlives the
    /// turn it belonged to: a user who stops one answer and asks another
    /// question would find that the first Ctrl-C of the **new** turn ended the
    /// session, because the session still remembered being asked to stop -- a
    /// keystroke doing something it has never done in the shell next door, at
    /// the moment it is least recoverable.
    ///
    /// The idle chain goes with it, for the same reason it does there: a turn
    /// having run is the session going somewhere, and the count of "how many
    /// times has the user asked to leave" starts again from a session that is.
    /// The Escape is untouched -- it is about a draft, and a turn ending does
    /// not change what is in the composer.
    pub(crate) fn turn_ended(&mut self) {
        self.interrupted_at = None;
        self.asked_to_stop = false;
    }

    /// A line the user really sent: every chain starts again from here.
    ///
    /// The line shell's `line_submitted` and `begin_turn` in one call, because
    /// on this surface they are one event -- there is no line discipline
    /// between the keystroke and the turn.
    pub(crate) fn submitted(&mut self) {
        self.interrupted_at = None;
        self.asked_to_stop = false;
        self.escaped_at = None;
    }

    /// Whether `at` is recent enough to still arm the keystroke happening now.
    ///
    /// `saturating_duration_since` rather than a subtraction: `now` is the
    /// caller's clock and a caller that handed back a slightly older reading --
    /// one read's `Instant` settling a decoder after a later one -- must not
    /// panic on the way to answering "not armed".
    fn armed(&self, at: Option<Instant>, now: Instant, window: Duration) -> bool {
        at.is_some_and(|at| now.saturating_duration_since(at) < window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{Duration, Instant};

    #[test]
    fn a_running_turn_takes_the_first_interrupt_and_the_second_leaves() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true), Interrupt::Cancel);
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(100), true),
            Interrupt::Leave
        );
    }

    #[test]
    fn at_an_idle_prompt_the_first_interrupt_clears_and_a_prompt_line_resets_it() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, false), Interrupt::Clear);
        gestures.submitted();
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(10), false),
            Interrupt::Clear
        );
    }

    #[test]
    fn two_interrupts_more_than_three_seconds_apart_do_not_leave() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, false), Interrupt::Clear);
        assert_eq!(
            gestures.interrupt(now + EXIT_WINDOW + Duration::from_millis(1), false),
            Interrupt::Clear
        );
    }

    #[test]
    fn double_escape_within_five_hundred_milliseconds_clears_the_composer() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now), Escape::Armed);
        assert_eq!(
            gestures.escape(now + Duration::from_millis(499)),
            Escape::Clear
        );
        assert_eq!(
            gestures.escape(now + Duration::from_millis(999)),
            Escape::Armed
        );
    }

    #[test]
    fn a_turn_that_was_already_asked_to_stop_is_left_however_long_the_user_waited() {
        // The arm that deliberately carries no clock: a wedged turn must still
        // be walkable away from four seconds later, which is the one moment
        // the gesture exists for.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true), Interrupt::Cancel);
        assert_eq!(
            gestures.interrupt(now + EXIT_WINDOW * 10, true),
            Interrupt::Leave
        );
    }

    #[test]
    fn a_turn_that_ended_between_two_interrupts_does_not_carry_the_first_one_over() {
        // `Cancel` then a turn that finished: the next Ctrl-C is at an idle
        // prompt and clears the draft rather than ending the session, because a
        // user who stopped a turn has not thereby asked to leave.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true), Interrupt::Cancel);
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(50), false),
            Interrupt::Clear
        );
    }

    #[test]
    fn a_press_that_lands_before_the_place_comes_back_arms_rather_than_leaves() {
        // The interleaving the terminal cannot be asked to arrange, at the seam
        // that can: `running` is a parameter precisely so that the window
        // between a turn's terminal event and the runtime giving its place back
        // is a case rather than a wait.
        //
        // The window is real and it is not the UI's. `Shell::interrupt` reads
        // `WorkHandle::outstanding` once **per press** (`super::shell`), and the
        // runtime decrements it on its own thread *after* the terminal event
        // (`super::worker`'s `turn_loop`). So a press can be answered from the
        // running column while the very next one -- of the same read -- is
        // answered from the idle column, with `turn_ended` having already
        // cleared what the first one armed.
        //
        // What that costs is one press, and it is why
        // `tui.rs`'s `ctrl_c_is_a_byte_that_cancels_the_turn_and_a_second_one_exits_130`
        // types three: the middle press here spends itself arming the idle
        // chain, and it is the third that leaves.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        // The conclusion has reached the UI: whatever the last Ctrl-C was about
        // is over ([`Gestures::turn_ended`]).
        gestures.turn_ended();

        // ...but the place is still held, so this is the running column.
        assert_eq!(gestures.interrupt(now, true), Interrupt::Cancel);
        // ...and now it is not, so this one is the idle column's *first* press
        // rather than the second half of anything.
        assert_eq!(
            gestures.interrupt(now, false),
            Interrupt::Clear,
            "a press answered from the running column armed an exit the idle \
             column then honoured"
        );
        assert_eq!(
            gestures.interrupt(now, false),
            Interrupt::Leave,
            "the idle pair did not leave, so no number of presses would"
        );
    }

    #[test]
    fn a_turn_that_ended_takes_the_cancellation_that_stopped_it_with_it() {
        // The chain must not outlive the turn it was about. A user who stops one
        // answer and asks another question would otherwise find the first
        // Ctrl-C of the new turn ending the session.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true), Interrupt::Cancel);

        gestures.turn_ended();

        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(10), true),
            Interrupt::Cancel,
            "the next turn's first Ctrl-C ended the session instead of \
             stopping that turn"
        );
    }

    #[test]
    fn a_turn_that_ended_does_not_disarm_an_escape_about_the_draft() {
        // A turn ending says nothing about what is in the composer, so the
        // warning the band is showing stays true.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now), Escape::Armed);
        gestures.turn_ended();
        assert!(gestures.escape_armed(now + Duration::from_millis(10)));
    }

    #[test]
    fn the_armed_escape_stops_being_armed_when_its_window_closes() {
        // What the hint row reads. A row still promising `esc again to clear`
        // after the window had closed would advertise a keystroke that no
        // longer does anything.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now), Escape::Armed);
        assert!(gestures.escape_armed(now + CLEAR_WINDOW - Duration::from_millis(1)));
        assert!(!gestures.escape_armed(now + CLEAR_WINDOW));
    }

    #[test]
    fn a_line_the_user_sent_disarms_the_escape_as_well_as_the_interrupt() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now), Escape::Armed);
        gestures.submitted();
        assert!(!gestures.escape_armed(now));
        assert_eq!(
            gestures.escape(now + Duration::from_millis(10)),
            Escape::Armed
        );
    }
}
