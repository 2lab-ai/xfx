//! What the turn is doing, while it is doing it.
//!
//! One row, directly above the divider, and only while there is work: `•
//! Thinking` with how long the turn has been running and how many tokens it has
//! spent, or the name of the tool that is running instead. A session with
//! nothing in flight has no such row at all -- the row is the *evidence* that
//! something is happening, so a row that is always there says nothing.
//!
//! Two decisions carry the module, and both are about honesty rather than
//! decoration:
//!
//! * **The clock stops while a decision is pending.** An approval panel is the
//!   session waiting for the person at the terminal, and a timer that kept
//!   running through it would be measuring the user rather than the model
//!   (`activity_status.zig:37-40`). So the frozen interval is *stolen* time and
//!   is subtracted: `12s` on this row always means twelve seconds of somebody
//!   else's work.
//! * **The marker blinks on a clock the renderer already has.** The phase is
//!   the animation tick's ([`super::render_request`]), 50 ms apart, and the
//!   marker is lit for [`BLINK`] and dark for [`BLINK`] -- a quarter of a
//!   [`PHASES`]-phase cycle each way (`render_request.zig:64-84`). Nothing here
//!   reads a clock of its own: `now` and `phase` arrive as arguments, which is
//!   what makes every row below a unit test rather than a claim about the
//!   millisecond the developer happened to run it in.
//!
//! What this module does **not** do is decide when a turn begins or ends.
//! That is [`super::shell`]'s, from what the runtime has in hand, because the
//! row and the queue's depth have to be two readings of one fact rather than
//! two facts that can drift.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

/// How long the marker stays lit, and then dark.
pub(crate) const BLINK: Duration = Duration::from_millis(500);

/// How many animation phases one cycle of the marker is
/// (`render_request.zig:64-84`). At the 50 ms animation tick that is two
/// seconds, and [`BLINK`] is a quarter of it -- so the marker is lit for ten
/// phases, dark for ten, and the cycle holds two of those.
pub(crate) const PHASES: u8 = 40;

/// How many phases the marker spends lit, and then dark.
///
/// Derived from [`BLINK`] and the animation tick rather than written down, so
/// there is one clock: a blink expressed in milliseconds and a phase count that
/// disagreed with it would make the marker's period whatever the phase count
/// happened to say. `the_blink_half_period_lines_up_with_five_hundred_
/// milliseconds_of_phases` is the check that [`PHASES`] and [`BLINK`] still
/// name the same cycle.
const BLINK_PHASES: u8 =
    (BLINK.as_millis() / super::render_request::ANIMATION_TICK.as_millis()) as u8;

/// What the row is marked with while the marker is lit.
const MARKER: char = '\u{2022}';

/// What it is marked with while it is dark: the same cell, blank, so the text
/// beside it does not step left and right twice a second.
const DARK: char = ' ';

/// What the row says while the model itself is working.
const THINKING: &str = "Thinking";

/// What separates the row's segments.
const GAP: &str = "  ";

/// What a turn is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Work {
    /// Nothing. There is no row.
    Idle,
    /// The model has the turn.
    Thinking,
    /// A tool call is running, and the row says which.
    Tool { name: String },
}

/// The turn's clock, its label, and what it has spent.
pub(crate) struct Activity {
    /// When the work this row is about began, or `None` before anything has
    /// begun.
    ///
    /// An `Option` rather than a clock read at construction, and it is the
    /// difference between a number and a coincidence: a session's own start is
    /// not a turn's, so an activity that was never begun reports **no** time
    /// rather than however long xfx has been running.
    started: Option<Instant>,
    /// When the clock was stopped, while it is stopped.
    frozen_at: Option<Instant>,
    /// How much of the wall clock since [`Self::started`] was spent waiting for
    /// the user rather than for the model, and is therefore not this turn's.
    stolen: Duration,
    work: Work,
    tokens: u64,
}

impl Activity {
    pub(crate) fn new() -> Self {
        Self {
            started: None,
            frozen_at: None,
            stolen: Duration::ZERO,
            work: Work::Idle,
            tokens: 0,
        }
    }

    /// Starts the clock for a new piece of work.
    ///
    /// Everything the last one accumulated goes with it: a turn that inherited
    /// the previous turn's frozen interval or token count would report a number
    /// that is nobody's.
    pub(crate) fn begin(&mut self, now: Instant) {
        self.started = Some(now);
        self.frozen_at = None;
        self.stolen = Duration::ZERO;
        self.tokens = 0;
    }

    /// Says what the work now is.
    pub(crate) fn set(&mut self, work: Work) {
        self.work = work;
    }

    /// Records how many tokens the turn has spent so far.
    // Staged. Nothing on the Phase-1 path produces a usage number: the streamed
    // event set is closed (`crate::output::Event`) and carries none, so the row
    // shows a token count only once a turn reports one. The formatting and its
    // absence are both tested, so the day the event arrives this is a call site
    // rather than a feature.
    #[allow(dead_code)]
    pub(crate) fn tokens(&mut self, count: u64) {
        self.tokens = count;
    }

    /// Stops the clock, because the session is waiting for the user.
    ///
    /// A second freeze does **not** move the moment it stopped at: the interval
    /// belongs to whatever asked first, and re-stamping it here would give the
    /// turn back time it never worked.
    // Staged with [`Self::thaw`]: the only thing in this design that waits for
    // a person is the approval panel, and `UiEvent::Approval` is unreachable
    // until Task 17 attaches a prompter to the runtime's permission session.
    #[allow(dead_code)]
    pub(crate) fn freeze(&mut self, now: Instant) {
        self.frozen_at.get_or_insert(now);
    }

    /// Starts it again, keeping the interval it stood still for.
    #[allow(dead_code)]
    pub(crate) fn thaw(&mut self, now: Instant) {
        if let Some(at) = self.frozen_at.take() {
            self.stolen = self
                .stolen
                .saturating_add(now.saturating_duration_since(at));
        }
    }

    /// Ends the work, and with it the row.
    ///
    /// **The clock is unstarted, not merely stopped**, and that is what makes
    /// the next turn's reading its own: [`Self::started`] is how the session
    /// tells "this work has a clock" from "this work is waiting for one", so a
    /// finished turn that left its `started` behind would hand the next turn
    /// the time the last one took.
    pub(crate) fn end(&mut self) {
        self.work = Work::Idle;
        self.started = None;
    }

    /// Whether there is work to say anything about.
    pub(crate) fn working(&self) -> bool {
        self.work != Work::Idle
    }

    /// Whether that work has had a clock started for it.
    ///
    /// The seam between the two halves of a turn beginning: the runtime says
    /// *that* one has (`super::bridge::UiEvent::TurnStarted`), and the session
    /// says *when* on its own next tick, from the clock every other row of the
    /// band is settled against.
    pub(crate) fn started(&self) -> bool {
        self.started.is_some()
    }

    /// How long the work has been running, less what was spent waiting for the
    /// user.
    ///
    /// Nothing at all until a turn has begun, because until then there is no
    /// interval to report -- and reporting the session's own age instead would
    /// put a plausible number on the row that measures nothing.
    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        let Some(started) = self.started else {
            return Duration::ZERO;
        };
        self.frozen_at
            .unwrap_or(now)
            .saturating_duration_since(started)
            .saturating_sub(self.stolen)
    }

    /// The row itself, or `None` when there is no work.
    ///
    /// Clipped by [`super::frame::clip`] -- the painter's own cut, not a second
    /// one -- so a tool with a very long name loses its tail rather than the
    /// row losing its shape, and the row is never wider than the screen it was
    /// measured for.
    pub(crate) fn row(&self, now: Instant, phase: u8, cols: u16) -> Option<String> {
        let label = match &self.work {
            Work::Idle => return None,
            Work::Thinking => THINKING,
            Work::Tool { name } => name.as_str(),
        };
        let mut row = String::with_capacity(label.len() + 24);
        row.push(if lit(phase) { MARKER } else { DARK });
        row.push(' ');
        row.push_str(label);
        row.push_str(GAP);
        row.push_str(&spoken(self.elapsed(now)));
        if self.tokens > 0 {
            // `write!` into a `String` cannot fail, and the result is discarded
            // rather than unwrapped for that reason.
            let _ = write!(row, "{GAP}{} tokens", self.tokens);
        }
        Some(super::frame::clip(&row, cols).to_string())
    }
}

/// Whether the marker is lit in this phase.
///
/// Half a cycle lit and half dark, measured in phases so that the blink is the
/// animation tick's multiple rather than a second clock that drifts against it.
fn lit(phase: u8) -> bool {
    ((phase % PHASES) / BLINK_PHASES).is_multiple_of(2)
}

/// How a duration is said on one row: `7s`, and `2m07s` once there are minutes
/// to say.
///
/// Whole seconds. The phase this row is drawn from moves on every 50 ms, and a
/// row is repainted whenever its **text** changes -- so a tenth of a second
/// here would be five repaints of the whole band a second, for a digit nobody
/// reads at that speed. Seconds cost one repaint each and are the number the
/// question is really about.
fn spoken(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m{:02}s", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_shell_has_no_activity_row_at_all() {
        let activity = Activity::new();
        assert_eq!(activity.row(Instant::now(), 0, 80), None);
    }

    #[test]
    fn thinking_shows_the_elapsed_time_and_the_tokens_so_far() {
        // activity_status.zig:26-33
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        activity.tokens(1234);
        let row = activity
            .row(start + Duration::from_secs(7), 0, 80)
            .expect("a row while thinking");
        assert!(row.contains("Thinking"), "{row:?}");
        assert!(row.contains("7s"), "{row:?}");
        assert!(row.contains("1234"), "{row:?}");
    }

    #[test]
    fn the_clock_freezes_while_a_decision_is_pending() {
        // activity_status.zig:37-40 -- a pending approval measures the user,
        // and the user is not the model.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        activity.freeze(start + Duration::from_secs(2));
        assert_eq!(
            activity.elapsed(start + Duration::from_secs(9)),
            Duration::from_secs(2)
        );
        activity.thaw(start + Duration::from_secs(9));
        assert_eq!(
            activity.elapsed(start + Duration::from_secs(10)),
            Duration::from_secs(3)
        );
    }

    #[test]
    fn a_running_tool_names_itself_instead_of_saying_thinking() {
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Tool {
            name: "read_file".into(),
        });
        let row = activity.row(start, 0, 80).expect("a row while a tool runs");
        assert!(row.contains("read_file"), "{row:?}");
        assert!(!row.contains("Thinking"), "{row:?}");
    }

    #[test]
    fn the_blink_half_period_lines_up_with_five_hundred_milliseconds_of_phases() {
        // render_request.zig:64-84: 10 phases of 50 ms is exactly the blink.
        assert_eq!(u32::from(PHASES) / 4 * 50, BLINK.as_millis() as u32);
    }

    #[test]
    fn the_row_is_clipped_to_the_terminal_width() {
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Tool {
            name: "a".repeat(200),
        });
        let row = activity.row(start, 0, 40).expect("a row");
        assert!(
            crate::tui::wrap::width(&row) <= 40,
            "the activity row overflowed: {row:?}"
        );
    }

    #[test]
    fn work_whose_clock_was_never_started_reports_no_time_rather_than_the_sessions() {
        // The row is a turn's, and a turn that never began has run for no time.
        // The alternative -- a clock started when the session was -- puts a
        // plausible number on the row that measures nothing anybody asked
        // about, and it grows all day.
        let mut activity = Activity::new();
        activity.set(Work::Thinking);
        assert_eq!(activity.elapsed(Instant::now()), Duration::ZERO);
        let row = activity.row(Instant::now(), 0, 80).expect("a row");
        assert!(row.contains("0s"), "{row:?}");
    }

    #[test]
    fn the_marker_is_lit_for_half_a_blink_cycle_and_dark_for_the_other_half() {
        // The blink itself, phase by phase: without this, a `lit` that answered
        // `true` for every phase would pass every case above -- none of them
        // looks at a second phase -- and the row would simply stop blinking.
        let lit_phases: Vec<u8> = (0..PHASES).filter(|phase| lit(*phase)).collect();
        assert_eq!(
            lit_phases,
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29],
            "the marker's on and off runs are not {BLINK_PHASES} phases each"
        );
    }

    #[test]
    fn the_marker_takes_a_cell_of_its_own_whether_it_is_lit_or_not() {
        // A blink that added and removed a cell would step the whole row left
        // and right twice a second, which is a worse thing to look at than a
        // marker that does not blink at all.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        let bright = activity.row(start, 0, 80).expect("a lit row");
        let dark = activity
            .row(start, BLINK_PHASES, 80)
            .expect("a row between blinks");
        assert!(bright.starts_with(MARKER), "{bright:?}");
        assert!(dark.starts_with(DARK), "{dark:?}");
        assert_eq!(
            &bright[MARKER.len_utf8()..],
            &dark[DARK.len_utf8()..],
            "the row moved when the marker went out"
        );
    }

    #[test]
    fn a_turn_that_says_nothing_about_tokens_does_not_report_zero_of_them() {
        // Phase 1 has no usage event, so every real row takes this path: a row
        // that said `0 tokens` for the whole of a turn would be reporting a
        // measurement nobody made.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        let row = activity.row(start, 0, 80).expect("a row");
        assert!(!row.contains("tokens"), "{row:?}");
    }

    #[test]
    fn a_new_piece_of_work_starts_from_nothing() {
        // The freeze, the stolen time and the token count are the last turn's,
        // and a turn that kept any of them would report a number that belongs
        // to work the user has already been told about.
        let mut activity = Activity::new();
        let first = Instant::now();
        activity.begin(first);
        activity.set(Work::Thinking);
        activity.tokens(99);
        activity.freeze(first + Duration::from_secs(1));
        activity.thaw(first + Duration::from_secs(5));

        let second = first + Duration::from_secs(10);
        activity.begin(second);
        assert_eq!(
            activity.elapsed(second + Duration::from_secs(3)),
            Duration::from_secs(3)
        );
        let row = activity.row(second, 0, 80).expect("a row");
        assert!(!row.contains("99"), "{row:?}");
    }

    #[test]
    fn a_second_freeze_does_not_give_the_turn_back_the_time_it_was_already_frozen_for() {
        // Two things asking for the same pause is not two pauses; re-stamping
        // the moment it stopped at would credit the turn with the interval
        // between them.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        activity.freeze(start + Duration::from_secs(2));
        activity.freeze(start + Duration::from_secs(8));
        activity.thaw(start + Duration::from_secs(10));
        assert_eq!(
            activity.elapsed(start + Duration::from_secs(10)),
            Duration::from_secs(2),
            "the clock ran while it was supposed to be stopped"
        );
    }

    #[test]
    fn work_that_ended_leaves_no_row_and_the_next_turn_an_unstopped_clock() {
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        activity.freeze(start + Duration::from_secs(1));
        activity.end();
        assert!(!activity.working());
        assert_eq!(activity.row(start + Duration::from_secs(9), 0, 80), None);

        // And the next turn's clock runs from the moment it began rather than
        // standing still at a freeze the last one never came out of -- which
        // is a turn that would report `0s` for as long as it ran.
        let next = start + Duration::from_secs(20);
        activity.begin(next);
        activity.set(Work::Thinking);
        assert_eq!(
            activity.elapsed(next + Duration::from_secs(4)),
            Duration::from_secs(4)
        );
    }

    #[test]
    fn minutes_are_said_once_there_are_minutes_to_say() {
        assert_eq!(spoken(Duration::from_secs(0)), "0s");
        assert_eq!(spoken(Duration::from_secs(59)), "59s");
        assert_eq!(spoken(Duration::from_secs(60)), "1m00s");
        assert_eq!(spoken(Duration::from_secs(671)), "11m11s");
    }
}
