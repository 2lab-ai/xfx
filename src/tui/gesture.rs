//! What a repeated keystroke means.
//!
//! Two keys in this phase, and both of them mean something different the second
//! time: Ctrl-C and Escape. What makes them a module rather than two fields on
//! the shell is that the *second* meaning is the dangerous one -- one leaves the
//! session with 130, the other throws a draft away or stops a turn -- and a rule
//! that decides them has to be readable on its own, with a clock handed to it
//! rather than read inside it.
//!
//! # Ctrl-C is upstream's semantic Ctrl-C, arriving as a byte
//!
//! With `ISIG` cleared the terminal generates no `SIGINT`, so the only Ctrl-C a
//! TUI session ever sees is the byte `0x03` coming out of the decoder. What it
//! means is upstream's `handleSemanticCtrlC` (`app_input_runtime.zig:1675-1712`,
//! `c1db919`), in its order:
//!
//! 1. **A turn is running and the composer holds a draft**: the draft goes, and
//!    nothing else happens -- no turn is stopped and no exit is armed
//!    ([`Interrupt::Clear`]). A user typing ahead while an answer streams who
//!    reaches for Ctrl-C is far more likely throwing away what they typed than
//!    abandoning the answer they are reading.
//! 2. **Otherwise every press is a press of one exit window**, [`EXIT_WINDOW`]
//!    long (`gesture_state.zig:3`, strict `<`). A press inside it leaves with
//!    130 ([`Interrupt::Leave`]); a press outside it arms it, and then stops the
//!    turn when one is running ([`Interrupt::Cancel`]) or throws the draft away
//!    when none is ([`Interrupt::Clear`]).
//!
//! **The window outlives the turn it stopped**, and that is the difference from
//! the rule this module used to keep. A cancelled turn concludes a few
//! milliseconds after the press, and the hint row says `press ctrl+c again to
//! exit` for the whole window (`fx-interrupt-40x120-cc-0.3` and `-cc-1.5`, both
//! after the conclusion): a window that closed at the conclusion would leave
//! the row promising a keystroke that, a moment later, only clears a draft.
//! What closes it early is **any other keystroke** ([`Gestures::other_key`],
//! upstream's `disarmCtrlCExit` on every raw byte but `0x03` and every resolved
//! action but a paste frame), which is what keeps a user who stopped one answer
//! and typed the next question from leaving on the first Ctrl-C of the new
//! turn: the typing disarmed it.
//!
//! **What a cancellation reaches is stated here because the gesture is where a
//! user decides it**: it stops the turn that is running *and* drops everything
//! queued behind it (`super::worker`'s `abandon_pending`). One turn at a time
//! and one prompt waiting is a promise about typing ahead, not about what
//! survives an interrupt -- a queue that started running a moment after the
//! user asked everything to stop would be the same surprise the band's waiting
//! prompt is shown to prevent.
//!
//! # Escape is a two-tap, and what the second tap does depends on the work
//!
//! A lone Escape does nothing by itself, so it arms. While the runtime has
//! work in hand the second one inside [`INTERRUPT_WINDOW`] **stops the turn**
//! (`app_input_runtime.zig:3168-3185`, the window `gesture_state.zig:5`,
//! inclusive) and the hint row says `esc again to interrupt` in between; at an
//! idle prompt the second one inside [`CLEAR_WINDOW`] clears the composer and
//! the row says `esc again to clear`. The idle window is short on purpose: an
//! Escape armed for three seconds would turn an unrelated later Escape into the
//! loss of a draft.
//!
//! Nothing here reads a clock. Every decision takes the `now` its caller already
//! has, which is what lets the whole table be tested without sleeping, and what
//! keeps one read's worth of bytes from expiring each other -- the bytes of a
//! single read arrived together (`super::shell::Shell::route_bytes`).

use std::time::{Duration, Instant};

/// How long a Ctrl-C arms the one that leaves (`gesture_state.zig:3`).
pub(crate) const EXIT_WINDOW: Duration = Duration::from_millis(3000);

/// How long a first Escape at an idle prompt arms the one that clears.
pub(crate) const CLEAR_WINDOW: Duration = Duration::from_millis(500);

/// How long a first Escape over running work arms the one that stops it
/// (`gesture_state.zig:5`).
pub(crate) const INTERRUPT_WINDOW: Duration = Duration::from_millis(1000);

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
    /// Throw the draft away.
    Clear,
    /// End the session with [`INTERRUPTED_EXIT_CODE`].
    Leave,
}

/// What one Escape means.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Escape {
    /// Nothing, and nothing armed: an idle prompt with no draft has nothing a
    /// second Escape could clear (`app_input_runtime.zig:3210-3213`).
    Nothing,
    /// Nothing yet: a second Escape inside [`CLEAR_WINDOW`] will clear the
    /// composer, and until then the hint row says so.
    Armed,
    /// Clear the composer.
    Clear,
    /// Nothing yet: a second Escape inside [`INTERRUPT_WINDOW`] will stop the
    /// work in hand, and until then the hint row says so.
    InterruptArmed,
    /// Stop the work in hand.
    Interrupt,
}

/// The session's memory of the keystroke before this one.
#[derive(Debug, Default)]
pub(crate) struct Gestures {
    /// When the Ctrl-C that armed the exit was, while it still does.
    exit_armed_at: Option<Instant>,
    /// When the Escape that armed the clear was, while it still does.
    escaped_at: Option<Instant>,
    /// When the Escape that armed the interrupt was, while it still does.
    interrupt_armed_at: Option<Instant>,
}

impl Gestures {
    /// What this Ctrl-C means, given whether the runtime has work in hand and
    /// whether the composer holds a draft.
    pub(crate) fn interrupt(&mut self, now: Instant, running: bool, draft: bool) -> Interrupt {
        // Upstream's raw-byte rule: anything that is not an Escape disarms both
        // Escape gestures, and this byte is not one.
        self.escaped_at = None;
        self.interrupt_armed_at = None;
        if running && draft {
            return Interrupt::Clear;
        }
        if armed(self.exit_armed_at, now, EXIT_WINDOW) {
            return Interrupt::Leave;
        }
        self.exit_armed_at = Some(now);
        if running {
            Interrupt::Cancel
        } else {
            Interrupt::Clear
        }
    }

    /// What this Escape means, given whether the runtime has work in hand and
    /// whether there is a draft for it to clear.
    pub(crate) fn escape(&mut self, now: Instant, running: bool, draft: bool) -> Escape {
        // A resolved Escape is a semantic key, and every one of those but a
        // Ctrl-C disarms the exit (`routeResolvedEscapeAction`,
        // `app_input_runtime.zig:1007-1010`).
        self.exit_armed_at = None;
        if running {
            self.escaped_at = None;
            if armed_through(self.interrupt_armed_at, now, INTERRUPT_WINDOW) {
                self.interrupt_armed_at = None;
                return Escape::Interrupt;
            }
            self.interrupt_armed_at = Some(now);
            return Escape::InterruptArmed;
        }
        self.interrupt_armed_at = None;
        // Upstream's idle Escape with no draft disarms and does nothing: a
        // row saying `esc again to clear` over an empty composer would be
        // advertising a keystroke with nothing to do.
        if !draft {
            self.escaped_at = None;
            return Escape::Nothing;
        }
        if armed(self.escaped_at, now, CLEAR_WINDOW) {
            self.escaped_at = None;
            return Escape::Clear;
        }
        self.escaped_at = Some(now);
        Escape::Armed
    }

    /// Any keystroke but the two this module times: the exit, and the
    /// interrupt an Escape would have confirmed, are both off. Returns whether
    /// either was on, so the caller knows the hint row has something to take
    /// back.
    ///
    /// The armed **clear** is left alone: what disarms it is the module's own
    /// two keys and a submitted line, as it always has been.
    pub(crate) fn other_key(&mut self) -> bool {
        let was = self.exit_armed_at.is_some() || self.interrupt_armed_at.is_some();
        self.exit_armed_at = None;
        self.interrupt_armed_at = None;
        was
    }

    /// Whether a Ctrl-C would leave right now.
    ///
    /// Read by the band so the hint row can say so, with the same window the
    /// decision uses rather than a copy of it: a row still saying `press ctrl+c
    /// again to exit` after the window had closed would be advertising a
    /// keystroke that no longer does it, and one that stopped saying it early
    /// would let a press leave unannounced.
    pub(crate) fn exit_armed(&self, now: Instant) -> bool {
        armed(self.exit_armed_at, now, EXIT_WINDOW)
    }

    /// Whether a second Escape would clear the composer right now.
    pub(crate) fn escape_armed(&self, now: Instant) -> bool {
        armed(self.escaped_at, now, CLEAR_WINDOW)
    }

    /// Whether a second Escape would stop the work in hand right now.
    pub(crate) fn interrupt_armed(&self, now: Instant) -> bool {
        armed_through(self.interrupt_armed_at, now, INTERRUPT_WINDOW)
    }

    /// The work an armed interrupt was about is over.
    ///
    /// Upstream's `operation_settled` disarm (`app_input_runtime.zig:3035-3043`):
    /// an armed interrupt guards one piece of work, and carried into whatever
    /// starts next it would let a single Escape stop a turn the user never
    /// armed anything against.
    ///
    /// **The exit stays armed**: it is a window of its own, and the row is
    /// still saying so. The Escape that arms a clear stays too -- it is about
    /// a draft, and a turn ending does not change what is in the composer.
    pub(crate) fn turn_ended(&mut self) {
        self.interrupt_armed_at = None;
    }

    /// A line the user really sent: every chain starts again from here.
    pub(crate) fn submitted(&mut self) {
        self.exit_armed_at = None;
        self.escaped_at = None;
        self.interrupt_armed_at = None;
    }
}

/// Whether `at` is recent enough to still arm the keystroke happening now:
/// strictly inside `window`.
///
/// `saturating_duration_since` rather than a subtraction: `now` is the caller's
/// clock and a caller that handed back a slightly older reading -- one read's
/// `Instant` settling a decoder after a later one -- must not panic on the way
/// to answering "not armed".
fn armed(at: Option<Instant>, now: Instant, window: Duration) -> bool {
    at.is_some_and(|at| now.saturating_duration_since(at) < window)
}

/// The same, with the window's last instant included -- upstream's rule for the
/// Escape interrupt (`gesture_state.zig:93`, `<=`).
fn armed_through(at: Option<Instant>, now: Instant, window: Duration) -> bool {
    at.is_some_and(|at| now.saturating_duration_since(at) <= window)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::{Duration, Instant};

    #[test]
    fn a_running_turn_takes_the_first_interrupt_and_the_second_leaves() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true, false), Interrupt::Cancel);
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(100), true, false),
            Interrupt::Leave
        );
    }

    #[test]
    fn at_an_idle_prompt_the_first_interrupt_clears_and_a_prompt_line_resets_it() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, false, true), Interrupt::Clear);
        gestures.submitted();
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(10), false, false),
            Interrupt::Clear
        );
    }

    #[test]
    fn two_interrupts_three_seconds_apart_do_not_leave() {
        // Strictly inside the window (`gesture_state.zig:65`, `<`): the press
        // on the window's last instant arms it again.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, false, false), Interrupt::Clear);
        assert_eq!(
            gestures.interrupt(now + EXIT_WINDOW, false, false),
            Interrupt::Clear
        );
        assert_eq!(
            gestures.interrupt(now + EXIT_WINDOW + Duration::from_millis(1), false, false),
            Interrupt::Leave,
            "the press that re-armed the window did not arm it"
        );
    }

    #[test]
    fn a_draft_typed_over_a_running_turn_is_cleared_first_and_arms_nothing() {
        // `handleSemanticCtrlC`'s first branch and `fx-interrupt2-…-draft-cc`:
        // the draft goes, the answer goes on, the hint row stays the session's.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true, true), Interrupt::Clear);
        assert!(!gestures.exit_armed(now), "clearing a draft armed the exit");
        // `draft-cc2`: the second press, at an empty composer, stops the turn
        // and arms the exit -- it does not leave.
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(400), true, false),
            Interrupt::Cancel
        );
        assert!(gestures.exit_armed(now + Duration::from_millis(400)));
    }

    #[test]
    fn the_exit_a_cancel_armed_outlives_the_turn_it_stopped() {
        // Claim changed (was `a_turn_that_ended_between_two_interrupts_does_not_
        // carry_the_first_one_over`): the hint row says `press ctrl+c again to
        // exit` after the cancelled turn has concluded (`fx-interrupt-…-cc-1.5`),
        // so a press inside the window leaves whatever the turn did since.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true, false), Interrupt::Cancel);
        gestures.turn_ended();
        assert!(gestures.exit_armed(now + Duration::from_millis(50)));
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(50), false, false),
            Interrupt::Leave
        );
    }

    #[test]
    fn a_turn_that_stopped_long_ago_is_stopped_again_rather_than_left() {
        // Claim changed (was `a_turn_that_was_already_asked_to_stop_is_left_
        // however_long_the_user_waited`): every press is a press of one window,
        // so a wedged turn is walked away from by two presses inside it, and a
        // press after it has closed asks the turn to stop again and arms it.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true, false), Interrupt::Cancel);
        let later = now + EXIT_WINDOW * 10;
        assert_eq!(gestures.interrupt(later, true, false), Interrupt::Cancel);
        assert_eq!(
            gestures.interrupt(later + Duration::from_millis(10), true, false),
            Interrupt::Leave
        );
    }

    #[test]
    fn typing_between_two_interrupts_starts_the_chain_again() {
        // Claim changed (was `a_turn_that_ended_takes_the_cancellation_that_
        // stopped_it_with_it`): what keeps the first Ctrl-C of the *next* turn
        // from leaving is the typing that asked for that turn, not the
        // conclusion of the last one.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.interrupt(now, true, false), Interrupt::Cancel);
        assert!(gestures.other_key(), "the armed exit was not reported");
        assert!(!gestures.exit_armed(now));
        assert_eq!(
            gestures.interrupt(now + Duration::from_millis(10), true, false),
            Interrupt::Cancel,
            "the next turn's first Ctrl-C ended the session instead of \
             stopping that turn"
        );
    }

    #[test]
    fn the_hint_says_exit_exactly_while_a_press_would_leave() {
        // Claim changed (was `the_notice_a_cancel_writes_promises_the_exit_this_
        // table_really_honours`): the promise is the hint row's now, and it is
        // read with the decision's own window on both sides of its edge.
        let now = Instant::now();
        for (running, draft) in [(true, false), (false, false), (false, true), (true, true)] {
            for offset in [
                Duration::ZERO,
                EXIT_WINDOW - Duration::from_millis(1),
                EXIT_WINDOW,
            ] {
                let mut gestures = Gestures::default();
                gestures.interrupt(now, running, draft);
                let promised = gestures.exit_armed(now + offset);
                let left = gestures.interrupt(now + offset, false, false) == Interrupt::Leave;
                assert_eq!(
                    promised, left,
                    "running={running} draft={draft} +{offset:?}: the row and the press disagree"
                );
            }
        }
    }

    #[test]
    fn double_escape_within_five_hundred_milliseconds_clears_the_composer() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
        assert_eq!(
            gestures.escape(now + Duration::from_millis(499), false, true),
            Escape::Clear
        );
        assert_eq!(
            gestures.escape(now + Duration::from_millis(999), false, true),
            Escape::Armed
        );
    }

    #[test]
    fn double_escape_over_running_work_interrupts_it_through_the_whole_second() {
        // `pressEscapeInterrupt` (`gesture_state.zig:90-103`): inclusive.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now, true, true), Escape::InterruptArmed);
        assert!(gestures.interrupt_armed(now + INTERRUPT_WINDOW));
        assert!(!gestures.escape_armed(now), "the clear was armed too");
        assert_eq!(
            gestures.escape(now + INTERRUPT_WINDOW, true, true),
            Escape::Interrupt
        );
        // And past it, the second Escape only arms again.
        let mut late = Gestures::default();
        assert_eq!(late.escape(now, true, true), Escape::InterruptArmed);
        assert_eq!(
            late.escape(
                now + INTERRUPT_WINDOW + Duration::from_millis(1),
                true,
                true
            ),
            Escape::InterruptArmed
        );
    }

    #[test]
    fn an_interrupt_armed_over_one_turn_does_not_carry_into_the_next() {
        // `operation_settled`: the turn the Escape was about ended by itself.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now, true, true), Escape::InterruptArmed);
        gestures.turn_ended();
        assert!(!gestures.interrupt_armed(now));
        assert_eq!(
            gestures.escape(now + Duration::from_millis(10), true, true),
            Escape::InterruptArmed
        );
    }

    #[test]
    fn each_of_the_two_keys_disarms_the_other() {
        let now = Instant::now();
        let mut gestures = Gestures::default();
        gestures.interrupt(now, false, false);
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
        assert!(!gestures.exit_armed(now), "an Escape left the exit armed");

        let mut gestures = Gestures::default();
        assert_eq!(gestures.escape(now, true, true), Escape::InterruptArmed);
        assert_eq!(gestures.interrupt(now, true, false), Interrupt::Cancel);
        assert!(
            !gestures.interrupt_armed(now),
            "a Ctrl-C left the Escape interrupt armed"
        );
    }

    #[test]
    fn an_escape_at_an_empty_idle_composer_arms_nothing() {
        // `app_input_runtime.zig:3210-3213`: no draft, nothing to clear, and
        // a clear that was armed is taken back.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now, false, false), Escape::Nothing);
        assert!(!gestures.escape_armed(now));
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
        assert_eq!(
            gestures.escape(now + Duration::from_millis(10), false, false),
            Escape::Nothing
        );
        assert!(!gestures.escape_armed(now + Duration::from_millis(10)));
        // Over running work the draft does not matter: the pair interrupts.
        assert_eq!(gestures.escape(now, true, false), Escape::InterruptArmed);
    }

    #[test]
    fn a_turn_that_ended_does_not_disarm_an_escape_about_the_draft() {
        // A turn ending says nothing about what is in the composer, so the
        // warning the band is showing stays true.
        let mut gestures = Gestures::default();
        let now = Instant::now();
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
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
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
        assert!(gestures.escape_armed(now + CLEAR_WINDOW - Duration::from_millis(1)));
        assert!(!gestures.escape_armed(now + CLEAR_WINDOW));
    }

    #[test]
    fn a_line_the_user_sent_disarms_every_gesture() {
        let mut gestures = Gestures::default();
        let now = Instant::now();
        gestures.interrupt(now, false, false);
        assert_eq!(gestures.escape(now, false, true), Escape::Armed);
        gestures.submitted();
        assert!(!gestures.escape_armed(now));
        assert!(!gestures.exit_armed(now));
        assert_eq!(
            gestures.escape(now + Duration::from_millis(10), false, true),
            Escape::Armed
        );
    }
}
