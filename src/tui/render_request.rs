//! Why a frame is being drawn, and the proof that one is owed.
//!
//! A tick that repaints unconditionally makes the band's cost a function of the
//! clock rather than of what changed, and a tick that repaints on a bare
//! `bool` makes "why did that frame happen" unanswerable. So the reasons are
//! typed, they accumulate until a frame is begun, and beginning one **takes**
//! them: a commit that failed hands its [`Attempt`] back and the frame is owed
//! again rather than lost.
//!
//! The animation clock is separate from the reasons for the same purpose. It is
//! a *due* time, not a request: nothing animates in this phase, and a phase
//! that does asks whether the 50 ms tick has come round rather than requesting a
//! frame every 8 ms and hoping the diff absorbs it.

use std::time::{Duration, Instant};

/// How often an animated row may redraw (`.prd/03-tui-port.md` §"MVS ladder"
/// item 6).
pub(crate) const ANIMATION_TICK: Duration = Duration::from_millis(50);

/// What made a frame necessary.
///
/// Every variant is a distinct bit, so two reasons arriving in one tick are one
/// frame rather than two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The band is the only surface in this phase, so `FirstFrame` and
// `ExternalDamage` are the two the session raises today. The rest name the
// tasks that raise them -- transcript (7), footer and hint (9, 16), modal and
// notification (17), animation (13, 14), resize (Phase 2 item 12) -- and are
// declared together because the set is what makes "why did that frame happen"
// answerable, and a set that grows one variant per commit never gets audited.
#[allow(dead_code)]
pub(crate) enum Reason {
    FirstFrame,
    Transcript,
    Footer,
    Modal,
    Animation,
    Notification,
    Resize,
    ExternalDamage,
}

impl Reason {
    /// This reason's bit in the pending set.
    fn bit(self) -> u8 {
        match self {
            Self::FirstFrame => 1 << 0,
            Self::Transcript => 1 << 1,
            Self::Footer => 1 << 2,
            Self::Modal => 1 << 3,
            Self::Animation => 1 << 4,
            Self::Notification => 1 << 5,
            Self::Resize => 1 << 6,
            Self::ExternalDamage => 1 << 7,
        }
    }
}

/// The reasons a frame is owed, and the animation clock.
#[derive(Debug, Default)]
pub(crate) struct RenderRequest {
    pending: u8,
    /// When an animated row last redrew. `None` while nothing is animating,
    /// which is what makes "due" mean *overdue since something started* rather
    /// than *due because the process has been running for 50 ms*.
    animation: Option<Instant>,
}

/// The reasons one frame took with it.
///
/// Held by the caller for exactly as long as the commit takes, and handed back
/// to [`RenderRequest::restore`] when it failed. `#[must_use]` because dropping
/// it on a failed commit is how a frame goes missing.
#[must_use]
#[derive(Debug)]
pub(crate) struct Attempt(u8);

impl Attempt {
    /// Whether this frame is owed because **something other than the band**
    /// wrote on the screen.
    ///
    /// The one reason a painter cannot answer by painting a difference: a
    /// shadow is a claim about what is on those rows, and a resume, a `/clear`
    /// or a Ctrl-L each mean that claim is now false about all of them. The
    /// frame is therefore a whole one ([`super::frame::Band::invalidate`]).
    ///
    /// Asked of the [`Attempt`] rather than of the [`RenderRequest`] on
    /// purpose: the reasons are *taken* by `begin`, so a caller that consulted
    /// the request afterwards would be asking about a set that has already been
    /// emptied -- and would repaint whole on the tick after the damaged one
    /// instead of on the damaged one.
    pub(crate) fn damaged(&self) -> bool {
        self.0 & Reason::ExternalDamage.bit() != 0
    }
}

impl RenderRequest {
    /// Records that `reason` needs a frame.
    pub(crate) fn request(&mut self, reason: Reason) {
        self.pending |= reason.bit();
    }

    /// Takes every pending reason, or `None` when a tick has nothing to draw.
    pub(crate) fn begin(&mut self) -> Option<Attempt> {
        if self.pending == 0 {
            return None;
        }
        Some(Attempt(std::mem::take(&mut self.pending)))
    }

    /// Puts a failed attempt's reasons back, alongside anything requested since.
    pub(crate) fn restore(&mut self, attempt: Attempt) {
        self.pending |= attempt.0;
    }

    /// Records that an animated row redrew at `now`, starting the clock.
    // The activity row is what animates in this phase; the clock is here
    // because it is the render request's, not that row's, and two clocks would
    // disagree.
    pub(crate) fn mark_animation(&mut self, now: Instant) {
        self.animation = Some(now);
    }

    /// Whether the animation tick has come round again.
    pub(crate) fn animation_due(&self, now: Instant) -> bool {
        self.animation
            .is_some_and(|last| now.saturating_duration_since(last) >= ANIMATION_TICK)
    }

    /// Moves the animation clock on, and says whether this call is a tick.
    ///
    /// The whole of the clock's policy, in one call the session makes every
    /// turn of its loop:
    ///
    /// * **Nothing animating stops the clock**, rather than leaving it to run.
    ///   An idle TUI must not repaint twenty times a second for a row nothing
    ///   is drawing, and a clock that kept ticking would make the *next* thing
    ///   that animates start mid-phase.
    /// * **A tick is [`ANIMATION_TICK`] apart from the last one**, so what
    ///   animates counts phases rather than reading a clock of its own -- which
    ///   is what keeps a 500 ms blink 500 ms long on a loop whose own tick is
    ///   8 ms.
    ///
    /// It asks for no frame. What changed on the screen is the caller's
    /// question, and a phase that turned over without changing a row is a
    /// repaint of the whole band for nothing.
    pub(crate) fn animate(&mut self, animating: bool, now: Instant) -> bool {
        if !animating {
            self.animation = None;
            return false;
        }
        if self.animation.is_some() && !self.animation_due(now) {
            return false;
        }
        self.mark_animation(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_with_nothing_pending_is_skipped() {
        let mut request = RenderRequest::default();
        assert!(request.begin().is_none(), "an idle tick asked for a frame");
        request.request(Reason::Footer);
        assert!(request.begin().is_some());
        assert!(request.begin().is_none(), "the reason was not consumed");
    }

    #[test]
    fn only_external_damage_makes_a_frame_a_whole_one() {
        // The bit a whole repaint costs a band's worth of bytes for, so it may
        // not be raised by anything the band itself changed -- and it may not
        // be *missed* when it is raised, because the frame after a resume is
        // painted onto a screen the shell has been writing to.
        for reason in [
            Reason::FirstFrame,
            Reason::Transcript,
            Reason::Footer,
            Reason::Modal,
            Reason::Animation,
            Reason::Notification,
            Reason::Resize,
        ] {
            let mut request = RenderRequest::default();
            request.request(reason);
            assert!(
                !request.begin().expect("a frame").damaged(),
                "{reason:?} asked for a whole repaint"
            );
        }
        let mut request = RenderRequest::default();
        request.request(Reason::Transcript);
        request.request(Reason::ExternalDamage);
        assert!(
            request.begin().expect("a frame").damaged(),
            "external damage did not reach the painter"
        );
    }

    #[test]
    fn a_failed_attempt_is_restored_so_the_frame_is_not_lost() {
        let mut request = RenderRequest::default();
        request.request(Reason::Transcript);
        let attempt = request.begin().expect("a frame was requested");
        request.restore(attempt);
        assert!(request.begin().is_some(), "the restored reason vanished");
    }

    #[test]
    fn animation_is_due_every_fifty_milliseconds_and_not_before() {
        let mut request = RenderRequest::default();
        let start = Instant::now();
        request.mark_animation(start);
        assert!(!request.animation_due(start + Duration::from_millis(49)));
        assert!(request.animation_due(start + Duration::from_millis(50)));
    }

    #[test]
    fn nothing_is_animating_until_something_says_it_is() {
        // Otherwise every session with a clock older than 50 ms would repaint
        // on every tick, for a row nothing is drawing.
        let request = RenderRequest::default();
        assert!(!request.animation_due(Instant::now()));
    }

    #[test]
    fn the_animation_clock_ticks_every_fifty_milliseconds_while_something_animates() {
        let mut request = RenderRequest::default();
        let start = Instant::now();
        assert!(
            request.animate(true, start),
            "the first tick starts the clock"
        );
        assert!(!request.animate(true, start + Duration::from_millis(49)));
        assert!(request.animate(true, start + Duration::from_millis(50)));
    }

    #[test]
    fn a_tick_asks_for_no_frame_by_itself() {
        // A phase that turned over without changing a row is a repaint of the
        // whole band for nothing, on a link that may be a serial line. What
        // changed is the caller's question.
        let mut request = RenderRequest::default();
        let start = Instant::now();
        assert!(request.animate(true, start));
        assert!(
            request.begin().is_none(),
            "an animation tick asked for a frame nothing had changed for"
        );
    }

    #[test]
    fn nothing_animating_stops_the_clock_rather_than_leaving_it_running() {
        // Otherwise the next thing that animates starts mid-phase -- and, worse,
        // an idle session would answer `animation_due` for ever after the last
        // turn ended.
        let mut request = RenderRequest::default();
        let start = Instant::now();
        assert!(request.animate(true, start));
        assert!(!request.animate(false, start + Duration::from_millis(50)));
        assert!(
            !request.animation_due(start + Duration::from_secs(60)),
            "the clock kept running after the last thing animating stopped"
        );
        assert!(
            request.animate(true, start + Duration::from_millis(60)),
            "the clock did not start again for the next thing that animates"
        );
    }

    #[test]
    fn a_reason_that_arrives_while_a_frame_is_in_flight_survives_the_restore() {
        // The failure this guards: `restore` overwriting the pending set rather
        // than adding to it, which would drop whatever was requested while the
        // commit was being attempted.
        let mut request = RenderRequest::default();
        request.request(Reason::Transcript);
        let attempt = request.begin().expect("a frame was requested");
        request.request(Reason::Modal);
        request.restore(attempt);
        let recovered = request.begin().expect("the restored frame");
        assert_eq!(
            recovered.0,
            Reason::Transcript.bit() | Reason::Modal.bit(),
            "a reason was lost between the failed commit and the next tick"
        );
    }

    #[test]
    fn every_reason_is_its_own_bit_and_two_in_one_tick_are_one_frame() {
        // Two reasons sharing a bit would make one of them invisible; a set
        // that grew rather than merged would make an idle tick draw twice.
        let reasons = [
            Reason::FirstFrame,
            Reason::Transcript,
            Reason::Footer,
            Reason::Modal,
            Reason::Animation,
            Reason::Notification,
            Reason::Resize,
            Reason::ExternalDamage,
        ];
        let mut seen = 0u8;
        for reason in reasons {
            assert_eq!(seen & reason.bit(), 0, "{reason:?} shares a bit");
            seen |= reason.bit();
        }
        assert_eq!(seen, u8::MAX, "the eight reasons do not fill the set");

        let mut request = RenderRequest::default();
        request.request(Reason::Transcript);
        request.request(Reason::Footer);
        assert!(request.begin().is_some());
        assert!(
            request.begin().is_none(),
            "two reasons in one tick asked for two frames"
        );
    }
}
