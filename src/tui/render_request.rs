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

/// How long a `SIGWINCH` waits before the band is re-solved from it.
///
/// A person dragging a window edge produces one signal per row the window
/// passes through, and answering each of them costs a `TIOCGWINSZ`, a re-solve,
/// a re-wrap of the unfinished line and a **whole-screen** repaint -- because a
/// terminal that changed size re-wrapped its own document, so the frame after
/// one can never be a difference ([`super::frame::Band::invalidate`]).
///
/// A deadline, never a sleep. The UI thread is the only reader of the terminal
/// (`super::event_loop`), so a thread parked inside the resize path is a
/// session that has stopped reading its keyboard.
///
/// The same number as [`ANIMATION_TICK`] and deliberately not the same clock:
/// this one bounds the cost of a drag and that one bounds the cost of a blink,
/// and a session that shared them would stop animating because a window moved.
pub(crate) const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);

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
    /// When the band may be re-solved from the screen's size, and `None` while
    /// no `SIGWINCH` is waiting to be answered.
    ///
    /// The deadline of the **first** winch of a burst rather than of the last:
    /// see [`RenderRequest::mark_resize`].
    resize: Option<Instant>,
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

    /// Records that the screen said it changed size at `now`.
    ///
    /// **The first winch of a burst starts the clock and the rest are free.**
    /// The deadline is not pushed forward by a later one, which is the whole of
    /// the ruling and it is the opposite of the usual debounce: a drag produces
    /// signals for as long as the mouse is down, so a deadline that re-armed
    /// would leave the band solved for a screen that no longer exists until the
    /// user let go -- seconds of a divider painted across a width the terminal
    /// gave up. Fixed, the cost of a drag is one resolve per
    /// [`RESIZE_DEBOUNCE`] however long it lasts, which is the same bound the
    /// animation clock puts on a blink, and a burst short enough to be one
    /// gesture costs exactly one.
    pub(crate) fn mark_resize(&mut self, now: Instant) {
        self.resize.get_or_insert(now + RESIZE_DEBOUNCE);
    }

    /// Whether a `SIGWINCH` is waiting to be answered.
    ///
    /// Not "is it due": this is true from the signal to the resolve, which is
    /// the interval in which **the screen has already changed and the band has
    /// not been re-solved for it yet**. Every row number the band would write
    /// in it is a coordinate out of the screen that was
    /// ([`super::shell::Shell::blind`]).
    pub(crate) fn resize_pending(&self) -> bool {
        self.resize.is_some()
    }

    /// Whether a marked resize has waited out the debounce.
    pub(crate) fn resize_due(&self, now: Instant) -> bool {
        self.resize.is_some_and(|due| now >= due)
    }

    /// Takes an outstanding resize **whatever the deadline says**, and reports
    /// whether there was one.
    ///
    /// For the exit, and for nothing else. The debounce exists so that a drag
    /// costs one re-solve rather than one per signal; at the exit there is no
    /// drag left to protect -- there is one measurement and then the session is
    /// over -- and waiting the deadline out would mean coming down without
    /// writing the answer the user was reading, which no later frame can make
    /// up ([`super::event_loop::resolve_resize_on_exit`]).
    pub(crate) fn force_resize(&mut self) -> bool {
        self.resize.take().is_some()
    }

    /// Takes the resize this tick owes, if it owes one.
    ///
    /// The combinator [`Self::animate`] is for the animation clock, and it is
    /// one call rather than a query and a clear for the same reason: "is a
    /// resize due" and "this tick is answering it" are one decision, and a
    /// caller that could ask without taking would resolve the same burst twice.
    pub(crate) fn take_resize(&mut self, now: Instant) -> bool {
        if !self.resize_due(now) {
            return false;
        }
        self.resize = None;
        true
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

    #[test]
    fn a_burst_of_winches_costs_one_resolve() {
        // What a person dragging a window edge really produces: a `SIGWINCH`
        // per pixel-row the window passes through. Re-solving the band, re-
        // wrapping the tail and repainting the whole screen for each of them
        // would make the cost of a resize a function of how slowly the mouse
        // moved. So the first one starts a clock and the rest of the burst is
        // free.
        let mut request = RenderRequest::default();
        let start = Instant::now();
        request.mark_resize(start);
        request.mark_resize(start + Duration::from_millis(10));
        request.mark_resize(start + Duration::from_millis(20));
        assert!(
            !request.resize_due(start + Duration::from_millis(49)),
            "a resize resolved before the burst could have ended"
        );
        assert!(
            request.resize_due(start + Duration::from_millis(50)),
            "the clock was restarted by a winch inside the window, so a slow \
             drag would never resolve at all"
        );
        assert!(request.take_resize(start + Duration::from_millis(50)));
        assert!(
            !request.take_resize(start + Duration::from_secs(60)),
            "the burst resolved twice"
        );
    }

    #[test]
    fn nothing_is_owed_a_resize_until_a_winch_says_so() {
        // Otherwise every session older than the debounce would re-solve its
        // band on its first tick, and read the terminal's size on every one
        // after that.
        let mut request = RenderRequest::default();
        let now = Instant::now();
        assert!(!request.resize_due(now));
        assert!(!request.take_resize(now));
    }

    #[test]
    fn a_resize_that_is_not_due_yet_is_not_taken() {
        // The half `resize_due` cannot check for itself: a `take` that resolved
        // early would make the debounce a comment.
        let mut request = RenderRequest::default();
        let start = Instant::now();
        request.mark_resize(start);
        assert!(!request.take_resize(start + RESIZE_DEBOUNCE - Duration::from_millis(1)));
        assert!(
            request.take_resize(start + RESIZE_DEBOUNCE),
            "the early take consumed the resize it refused to resolve"
        );
    }

    #[test]
    fn the_resize_debounce_is_fifty_milliseconds() {
        // Pinned as a literal rather than imported, for the reason every policy
        // number in this crate's tests is: a test that read the constant it is
        // checking would pass for whatever the module happened to declare.
        assert_eq!(RESIZE_DEBOUNCE, Duration::from_millis(50));
    }

    #[test]
    fn a_resize_and_the_animation_clock_are_two_different_facts() {
        // They are the same number and they are not the same clock. A resize
        // that armed the animation clock would make an idle session repaint
        // twenty times a second for ever after the window was dragged once.
        let mut request = RenderRequest::default();
        let start = Instant::now();
        request.mark_resize(start);
        assert!(
            !request.animation_due(start + Duration::from_secs(1)),
            "a winch started the animation clock"
        );
        request.mark_animation(start);
        assert!(
            request.take_resize(start + RESIZE_DEBOUNCE),
            "the animation clock swallowed the resize"
        );
        assert!(
            request.animation_due(start + ANIMATION_TICK),
            "resolving a resize stopped the animation clock"
        );
    }
}
