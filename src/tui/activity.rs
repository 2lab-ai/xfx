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

/// What the row says before the model has written anything (`TurnPhase`,
/// `activity_status.zig:105-111`).
const THINKING: &str = "Thinking";

/// What it says once the answer is streaming.
const GENERATING: &str = "Generatinq";

/// What it says while a tool runs.
const RUNNING: &str = "Running";

/// What a turn is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Work {
    /// Nothing. There is no row.
    Idle,
    /// The model has the turn and has written nothing yet.
    Thinking,
    /// The model is writing its answer.
    Generating,
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
}

impl Activity {
    pub(crate) fn new() -> Self {
        Self {
            started: None,
            frozen_at: None,
            stolen: Duration::ZERO,
            work: Work::Idle,
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
    }

    /// Says what the work now is.
    pub(crate) fn set(&mut self, work: Work) {
        self.work = work;
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

    /// The row itself, painted, or `None` when there is no work.
    ///
    /// Upstream's label (`buildTurnLabel`, `activity_status.zig:113-121`):
    /// `• Generating (2s) (↑4 ↓61)` -- the marker in the activity grey while
    /// it is lit and a blank cell while it is dark, the phase and the clock in
    /// the activity grey, and the token suffix receding into the dim grey
    /// (`writeThinkingBlinkText`, `shimmer_runtime.zig:262-291`). `tokens` is
    /// the turn's (in, out) estimate, and is left off while both are zero.
    pub(crate) fn row(
        &self,
        now: Instant,
        phase: u8,
        cols: u16,
        tokens: (u64, u64),
        colours: Colours,
    ) -> Option<String> {
        let label = match &self.work {
            Work::Idle => return None,
            Work::Thinking => THINKING,
            Work::Generating => GENERATING,
            Work::Tool { .. } => RUNNING,
        };
        Some(marked_row(
            label,
            self.elapsed(now),
            phase,
            cols,
            tokens,
            colours,
        ))
    }
}

/// A row with the blinking marker: `• <label> (<elapsed>)`, and the token
/// suffix when `tokens` is not nought (`writeThinkingBlinkText`,
/// `shimmer_runtime.zig:262-291`). The turn's row, and a compaction's in
/// progress (`compactionProjection`, `activity_status.zig:16-41`), which has
/// no tokens to say.
pub(crate) fn marked_row(
    label: &str,
    elapsed: Duration,
    phase: u8,
    cols: u16,
    tokens: (u64, u64),
    colours: Colours,
) -> String {
    let mut text = format!(" {label} ({})", spoken(elapsed));
    let mut suffix = String::new();
    if tokens != (0, 0) {
        let _ = write!(
            suffix,
            " (\u{2191}{} \u{2193}{})",
            compact_tokens(tokens.0),
            compact_tokens(tokens.1)
        );
    }
    // Measured as the painter measures, then cut: the marker cell, the
    // label, and as much of the suffix as is left.
    let width = usize::from(cols);
    let label_room = width.saturating_sub(1);
    text = super::frame::clip(&text, u16::try_from(label_room).unwrap_or(u16::MAX)).to_string();
    let suffix_room = label_room.saturating_sub(usize::from(super::wrap::width(&text)));
    let suffix =
        super::frame::clip(&suffix, u16::try_from(suffix_room).unwrap_or(u16::MAX)).to_string();
    let marker = if lit(phase) {
        format!("{}{MARKER}{}", colours.label, colours.reset)
    } else {
        DARK.to_string()
    };
    let mut row = format!("{marker}{}{text}", colours.label);
    if !suffix.is_empty() {
        row.push_str(colours.dim);
        row.push_str(&suffix);
    }
    row.push_str(colours.reset);
    row
}

/// The three sequences the row is painted with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Colours {
    pub(crate) label: &'static str,
    pub(crate) dim: &'static str,
    pub(crate) reset: &'static str,
}

/// `formatTokenCountCompact` (`activity_status.zig:259-268`).
pub(crate) fn compact_tokens(tokens: u64) -> String {
    if tokens < 1000 {
        return tokens.to_string();
    }
    let (whole, tenths) = (tokens / 1000, (tokens % 1000) / 100);
    if whole < 10 && tenths > 0 {
        format!("{whole}.{tenths}k")
    } else {
        format!("{whole}k")
    }
}

/// Whether the marker is lit in this phase.
///
/// Half a cycle lit and half dark, measured in phases so that the blink is the
/// animation tick's multiple rather than a second clock that drifts against it.
fn lit(phase: u8) -> bool {
    ((phase % PHASES) / BLINK_PHASES).is_multiple_of(2)
}

/// How a duration is said on the row: `5s`, `18m0s`, `1h2m3s` -- leading units
/// that are zero dropped (`writeElapsed`, `activity_status.zig:144-158`).
///
/// Whole seconds. The phase this row is drawn from moves on every 50 ms, and a
/// row is repainted whenever its **text** changes, so a finer clock would be a
/// repaint of the band for a digit nobody reads at that speed.
fn spoken(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, (seconds / 60) % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h{minutes}m{seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream's dark activity greys, spelled out.
    const PLAIN: Colours = Colours {
        label: "\u{1b}[38;5;252m",
        dim: "\u{1b}[38;5;245m",
        reset: "\u{1b}[0m",
    };

    /// The row's text with its colours taken out.
    fn text(row: &str) -> String {
        row.replace(PLAIN.label, "")
            .replace(PLAIN.dim, "")
            .replace(PLAIN.reset, "")
    }

    fn row(activity: &Activity, now: Instant, phase: u8, cols: u16) -> Option<String> {
        activity
            .row(now, phase, cols, (0, 0), PLAIN)
            .map(|row| text(&row))
    }

    #[test]
    fn an_idle_shell_has_no_activity_row_at_all() {
        let activity = Activity::new();
        assert_eq!(row(&activity, Instant::now(), 0, 80), None);
    }

    #[test]
    fn thinking_shows_the_elapsed_time_and_the_tokens_so_far() {
        // activity_status.zig:113-121, and the colours shimmer_runtime.zig:262-291
        // gives it: the marker and the label in the activity grey, the token
        // suffix in the dim one.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        let painted = activity
            .row(start + Duration::from_secs(7), 0, 80, (1234, 61), PLAIN)
            .expect("a row while thinking");
        assert_eq!(
            painted,
            "\u{1b}[38;5;252m\u{2022}\u{1b}[0m\u{1b}[38;5;252m Thinking (7s)\
             \u{1b}[38;5;245m (\u{2191}1.2k \u{2193}61)\u{1b}[0m"
        );
    }

    #[test]
    fn the_phase_says_generating_once_the_answer_streams_and_running_for_a_tool() {
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Generating);
        assert_eq!(
            row(&activity, start, 0, 80).as_deref(),
            Some("\u{2022} Generating (0s)")
        );
        activity.set(Work::Tool {
            name: "read_file".into(),
        });
        assert_eq!(
            row(&activity, start, 0, 80).as_deref(),
            Some("\u{2022} Running (0s)")
        );
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
    fn a_running_tool_says_running_instead_of_thinking() {
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Tool {
            name: "read_file".into(),
        });
        let row = row(&activity, start, 0, 80).expect("a row while a tool runs");
        assert!(row.contains("Running"), "{row:?}");
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
        activity.set(Work::Thinking);
        for cols in [1u16, 5, 12, 20, 40] {
            let row = row(&activity, start, 0, cols).expect("a row");
            let wide = activity
                .row(start, 0, cols, (123_456, 7_890), PLAIN)
                .map(|row| text(&row))
                .expect("a row");
            for row in [row, wide] {
                assert!(
                    usize::from(crate::tui::wrap::width(&row)) <= usize::from(cols),
                    "the activity row overflowed {cols}: {row:?}"
                );
            }
        }
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
        let row = row(&activity, Instant::now(), 0, 80).expect("a row");
        assert!(row.contains("(0s)"), "{row:?}");
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
        let bright = row(&activity, start, 0, 80).expect("a lit row");
        let dark = row(&activity, start, BLINK_PHASES, 80).expect("a row between blinks");
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
        // appendTokenProgressSuffix (`activity_status.zig:279-283`): nothing
        // when both counts are zero, rather than a measurement nobody made.
        let mut activity = Activity::new();
        let start = Instant::now();
        activity.begin(start);
        activity.set(Work::Thinking);
        let row = row(&activity, start, 0, 80).expect("a row");
        assert!(!row.contains('\u{2191}'), "{row:?}");
    }

    #[test]
    fn a_new_piece_of_work_starts_from_nothing() {
        // The freeze and the stolen time are the last turn's, and a turn that
        // kept either would report a number that belongs to work the user has
        // already been told about. (The tokens are the shell's tally, begun
        // again with each turn.)
        let mut activity = Activity::new();
        let first = Instant::now();
        activity.begin(first);
        activity.set(Work::Thinking);
        activity.freeze(first + Duration::from_secs(1));
        activity.thaw(first + Duration::from_secs(5));

        let second = first + Duration::from_secs(10);
        activity.begin(second);
        assert_eq!(
            activity.elapsed(second + Duration::from_secs(3)),
            Duration::from_secs(3)
        );
        let row = row(&activity, second, 0, 80).expect("a row");
        assert!(row.contains("(0s)"), "{row:?}");
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
        assert_eq!(row(&activity, start + Duration::from_secs(9), 0, 80), None);

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
    fn minutes_and_hours_are_said_once_there_are_any_to_say() {
        // writeElapsed, activity_status.zig:144-158.
        assert_eq!(spoken(Duration::from_secs(0)), "0s");
        assert_eq!(spoken(Duration::from_secs(59)), "59s");
        assert_eq!(spoken(Duration::from_secs(60)), "1m0s");
        assert_eq!(spoken(Duration::from_secs(671)), "11m11s");
        assert_eq!(spoken(Duration::from_secs(3723)), "1h2m3s");
    }

    #[test]
    fn token_counts_are_compact_as_upstream_writes_them() {
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(1_000), "1k");
        assert_eq!(compact_tokens(1_234), "1.2k");
        assert_eq!(compact_tokens(50_000), "50k");
    }
}
