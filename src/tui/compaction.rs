//! A compaction, on the row above the divider.
//!
//! Upstream shows a compaction **only** there, and never in the transcript
//! (`compactionProjection`, `vercel-labs/fx@c1db9191
//! src/core/output/activity_status.zig:16-41`; its e2e asserts neither the
//! inline nor the full transcript mentions one, `tui-compaction-activity.test.ts:228-241`):
//!
//! - while it runs, `• Preparing compaction (0s)` until the notes request is
//!   about to go, then `• Compacting (3s)`, and `• Stopping compaction` once
//!   the user has asked it to stop -- the turn row's blinking marker and
//!   clock, with no token count (`:254`);
//! - when it ends, nothing for a success, and otherwise one sentence in its
//!   tone's grey (`compactionFeedbackLabel`, `:43-67`). "No context to
//!   compact." and the busy refusal go after 1500 ms; a failure and a cancel
//!   stay until an Escape or the next submission dismisses them
//!   (`compaction_activity.zig:118-124`, `input_submit_runtime.zig:628-635`,
//!   `app_input_runtime.zig:3158`).
//!
//! A manual compaction's clock is its own; an automatic one runs inside a
//! turn and reads the turn's (`compactionClock`, `activity_status.zig:9-14`).

use std::time::{Duration, Instant};

use super::activity::{marked_row, Colours};

/// How long "No context to compact." and the busy refusal stay
/// (`compaction_activity.zig:123`).
pub(crate) const TRANSIENT: Duration = Duration::from_millis(1500);

/// How a compaction ended (`compaction_activity.Outcome` and the failures
/// `compactionFeedbackLabel` tells apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Feedback {
    /// Saved: the row goes.
    Succeeded,
    /// Nothing older than the turns kept as they are, or no conversation.
    NoOp,
    /// A turn is running or a prompt is waiting.
    Busy,
    /// The user stopped it.
    Cancelled,
    /// It could not start: the provider could not be reached to ask.
    NotStarted,
    /// The notes could not be written, or the checkpoint could not be saved.
    Failed,
    /// Even compacting everything leaves a request the model cannot take.
    TooLarge,
    /// The checkpoint was written and whether it landed is not known.
    Unconfirmed,
}

/// The grey a feedback sentence is painted in (`activity_status.zig:29-35`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Neutral,
    Warning,
    Danger,
}

impl Feedback {
    /// Upstream's sentence, verbatim (`compactionFeedbackLabel`,
    /// `activity_status.zig:43-67`). Empty for a success, which shows
    /// nothing.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Succeeded => "",
            Self::NoOp => "No context to compact.",
            Self::Busy => "Wait for the active work to finish before compacting context.",
            Self::Cancelled => "Compaction cancelled. Try /compact again when ready.",
            Self::NotStarted => "Compaction was not started. Try /compact again.",
            Self::Failed => "Compaction failed. Try /compact again.",
            Self::TooLarge => {
                "Context is too large to compact. Choose a model with a larger context window."
            }
            Self::Unconfirmed => {
                "Compaction could not confirm saved context. Reopen the session before retrying."
            }
        }
    }

    pub(crate) fn tone(self) -> Tone {
        match self {
            Self::Busy => Tone::Warning,
            Self::NotStarted | Self::Failed | Self::TooLarge | Self::Unconfirmed => Tone::Danger,
            Self::Succeeded | Self::NoOp | Self::Cancelled => Tone::Neutral,
        }
    }

    /// How long the sentence stays, or `None` for one that stays until it is
    /// dismissed.
    fn lifetime(self) -> Option<Duration> {
        match self {
            Self::NoOp | Self::Busy => Some(TRANSIENT),
            _ => None,
        }
    }
}

/// What the runtime tells the band about a compaction
/// (`super::bridge::UiEvent::Compaction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Update {
    /// One has been chosen and nothing is sent yet. `automatic` is a turn's.
    Began { automatic: bool },
    /// The notes request is about to be sent.
    Summarizing,
    /// It is over.
    Settled(Feedback),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Preparing,
    Compacting,
    Stopping,
    Settled(Feedback),
}

/// The compaction the row is about, running or just ended.
#[derive(Debug, Clone)]
pub(crate) struct Compaction {
    automatic: bool,
    phase: Phase,
    /// When a manual one's clock started; settled on the band's next tick,
    /// as the turn's is.
    started: Option<Instant>,
    /// When its sentence goes, for one that goes by itself.
    expires: Option<Instant>,
}

impl Compaction {
    /// One that has begun and sent nothing yet.
    pub(crate) fn began(automatic: bool) -> Self {
        Self {
            automatic,
            phase: Phase::Preparing,
            started: None,
            expires: None,
        }
    }

    /// One that ended before it began: refused at the keystroke.
    pub(crate) fn refused(feedback: Feedback, now: Instant) -> Self {
        let mut compaction = Self::began(false);
        compaction.settle(feedback, now);
        compaction
    }

    /// Whether it is still running.
    pub(crate) fn active(&self) -> bool {
        !matches!(self.phase, Phase::Settled(_))
    }

    pub(crate) fn automatic(&self) -> bool {
        self.automatic
    }

    pub(crate) fn summarizing(&mut self) {
        if self.phase == Phase::Preparing {
            self.phase = Phase::Compacting;
        }
    }

    /// The user has asked it to stop (`State.stopping`,
    /// `compaction_activity.zig:111-116`).
    pub(crate) fn stopping(&mut self) {
        if self.active() {
            self.phase = Phase::Stopping;
        }
    }

    /// Whether the user has asked it to stop and it has not said it did.
    pub(crate) fn stopped(&self) -> bool {
        self.phase == Phase::Stopping
    }

    pub(crate) fn settle(&mut self, feedback: Feedback, now: Instant) {
        self.phase = Phase::Settled(feedback);
        self.expires = feedback.lifetime().map(|lifetime| now + lifetime);
    }

    /// Starts a manual one's clock, on the band's tick.
    pub(crate) fn tick(&mut self, now: Instant) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    /// Whether there is anything on the row (`Operation.visible`,
    /// `compaction_activity.zig:52-59`).
    pub(crate) fn visible(&self, now: Instant) -> bool {
        match self.phase {
            Phase::Settled(Feedback::Succeeded) => false,
            Phase::Settled(_) => self.expires.is_none_or(|expiry| now < expiry),
            _ => true,
        }
    }

    /// The row, or `None` when there is nothing on it. `turn` is how long the
    /// turn has been running, which an automatic compaction's clock is.
    pub(crate) fn row(
        &self,
        now: Instant,
        phase: u8,
        cols: u16,
        turn: Duration,
        colours: Colours,
    ) -> Option<Row> {
        if !self.visible(now) {
            return None;
        }
        let label = match self.phase {
            Phase::Preparing => "Preparing compaction",
            Phase::Compacting => "Compacting",
            Phase::Stopping => "Stopping compaction",
            Phase::Settled(feedback) => return Some(Row::Feedback(feedback)),
        };
        let elapsed = if self.automatic {
            turn
        } else {
            self.started.map_or(Duration::ZERO, |started| {
                now.saturating_duration_since(started)
            })
        };
        Some(Row::Running(marked_row(
            label,
            elapsed,
            phase,
            cols,
            (0, 0),
            colours,
        )))
    }
}

/// What the row holds: a running compaction's painted label, or the
/// sentence it ended with, for the shell to paint in its tone's grey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Running(String),
    Feedback(Feedback),
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Colours = Colours {
        label: "<",
        dim: "~",
        reset: ">",
    };

    fn running(compaction: &Compaction, now: Instant, phase: u8) -> String {
        match compaction.row(now, phase, 120, Duration::from_secs(9), PLAIN) {
            Some(Row::Running(row)) => row,
            other => panic!("not running: {other:?}"),
        }
    }

    #[test]
    fn a_manual_compaction_reads_its_own_clock_and_says_its_phase() {
        let now = Instant::now();
        let mut compaction = Compaction::began(false);
        compaction.tick(now);
        // `receipts/w4/fx-compact-40x120-compacting-1.9s`: the marker lit.
        assert_eq!(
            running(&compaction, now, 0),
            "<\u{2022}>< Preparing compaction (0s)>"
        );
        compaction.summarizing();
        assert_eq!(
            running(&compaction, now + Duration::from_millis(1800), 10),
            " < Compacting (1s)>",
            "dark phase: the marker cell blank (`-compacting-0.4s`)"
        );
        compaction.stopping();
        assert!(running(&compaction, now, 0).contains("Stopping compaction (0s)"));
    }

    #[test]
    fn an_automatic_compaction_reads_the_turns_clock() {
        let mut compaction = Compaction::began(true);
        compaction.summarizing();
        assert!(running(&compaction, Instant::now(), 0).contains("Compacting (9s)"));
    }

    #[test]
    fn a_success_shows_nothing_and_the_refusals_go_after_a_second_and_a_half() {
        let now = Instant::now();
        let mut done = Compaction::began(false);
        done.settle(Feedback::Succeeded, now);
        assert!(!done.visible(now));
        for transient in [Feedback::NoOp, Feedback::Busy] {
            let refused = Compaction::refused(transient, now);
            assert!(refused.visible(now + Duration::from_millis(1499)));
            assert!(!refused.visible(now + TRANSIENT));
        }
        for lasting in [
            Feedback::Cancelled,
            Feedback::Failed,
            Feedback::NotStarted,
            Feedback::TooLarge,
        ] {
            let ended = Compaction::refused(lasting, now);
            assert!(
                ended.visible(now + Duration::from_secs(3600)),
                "{lasting:?}"
            );
        }
    }

    #[test]
    fn every_sentence_is_upstreams_in_its_tone() {
        let table = [
            (Feedback::NoOp, "No context to compact.", Tone::Neutral),
            (
                Feedback::Busy,
                "Wait for the active work to finish before compacting context.",
                Tone::Warning,
            ),
            (
                Feedback::Cancelled,
                "Compaction cancelled. Try /compact again when ready.",
                Tone::Neutral,
            ),
            (
                Feedback::NotStarted,
                "Compaction was not started. Try /compact again.",
                Tone::Danger,
            ),
            (
                Feedback::Failed,
                "Compaction failed. Try /compact again.",
                Tone::Danger,
            ),
            (
                Feedback::TooLarge,
                "Context is too large to compact. Choose a model with a larger context window.",
                Tone::Danger,
            ),
            (
                Feedback::Unconfirmed,
                "Compaction could not confirm saved context. Reopen the session before retrying.",
                Tone::Danger,
            ),
        ];
        for (feedback, label, tone) in table {
            assert_eq!(feedback.label(), label);
            assert_eq!(feedback.tone(), tone, "{feedback:?}");
        }
    }
}
