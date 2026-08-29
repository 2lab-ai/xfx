//! The question xfx asks inside the band, and the channel it hears the answer
//! on.
//!
//! `ask` is the **default-safe** permission mode, and until this module existed
//! it was the one mode a TUI session could not run in: the runtime thread built
//! a [`crate::permission::PermissionSession`] with no approval channel at all,
//! so every mutation was denied with the refusal a pipe gets. The reason was
//! never policy -- it was the terminal. `TtyPrompter` writes a question to
//! standard error and reads a line from standard input, and standard input is
//! the descriptor the UI thread owns and is sitting in `pselect(2)` on. Two
//! readers on one terminal is the bug this whole topology exists to prevent.
//!
//! So the question does not go to the terminal at all. It goes to the UI as a
//! [`UiEvent::Approval`], the UI paints it as rows of the band
//! ([`Panel`]), and the answer comes back on the **control** channel as a
//! [`TurnControl::Answer`] -- the same unbounded channel a Ctrl-C travels on,
//! for the same reason: it is drained *inside* a turn, so an answer cannot
//! queue behind a prompt the turn cannot dequeue until it ends.
//!
//! # Two readers, one receiver, and the waker they fight over
//!
//! [`ApprovalPrompter::request`] is synchronous -- it is called from inside a
//! tool call, which is inside `run_turn_saved`, which is a future the runtime
//! thread is polling -- so the only way it can wait for an answer is
//! [`park_on`], which parks the whole runtime thread. While it is parked
//! `super::worker`'s `raced_against_control` cannot poll anything, including
//! its own listen on the control channel. The prompter therefore has to read
//! that channel *itself*, which is why [`ControlChannel`] exists: one receiver,
//! two callers, and a rule that says they never run at the same instant --
//! because the second one only runs on a thread the first one has parked.
//!
//! What that costs is a **waker**. `tokio`'s receiver holds exactly one, so the
//! prompter's park-waker overwrites the turn loop's task-waker when it
//! registers, and the send that wakes the prompter *takes* it. Left alone, the
//! turn loop would come out of the approval with nothing registered on the
//! channel: a Ctrl-C arriving while the body sat on a provider that had gone
//! quiet would wake nobody, and the interrupt would be lost for as long as the
//! socket stayed silent -- which is exactly the case `raced_against_control`
//! was written for. So the loop's waker is remembered when it registers, and
//! [`ControlChannel::rearm`] wakes it the moment the prompter is done. One
//! spurious poll per approval buys back the property.
//!
//! # What the panel may answer with, and what it may not
//!
//! Three choices, and nothing else: yes once, yes and stop asking, no. Esc and
//! Ctrl-C refuse, because **a decision xfx was never given is a refusal** --
//! never an allow -- and so does a session that is shutting down or a UI that
//! has gone away. The alternate-screen diff review, the amendment draft and the
//! readiness commit gate upstream puts around the same question are Phases 2
//! and 3; what is here is the inline panel and the disclosure the line shell's
//! own prompt makes, because a narrower safety surface would be a regression
//! rather than a port.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use super::bridge::{park_on, send_ui, Cancellation, Stopped, TurnControl, UiEvent};
use crate::permission::{ApprovalAnswer, ApprovalPrompter, ApprovalRequest};

// ---------------------------------------------------------------------------
// what the panel says
// ---------------------------------------------------------------------------

/// How many rows the panel takes on an ordinary screen
/// (`interaction_state.zig:12-15`).
pub(crate) const COMPACT_ROWS: u16 = 8;

/// How many it takes on a tall one, where the summary is worth more rows than
/// the blank space it displaces.
pub(crate) const SPACIOUS_ROWS: u16 = 11;

/// The screen height at which it stops being compact.
pub(crate) const SPACIOUS_AT: u16 = 34;

/// The shortest screen the compact panel is drawn on.
///
/// Below it the panel keeps its three choices and gives up the blank row and
/// two of the summary's: what a question may never lose is what it is asking
/// and what the answers are.
const COMPACT_AT: u16 = 14;

/// How many rows the smallest panel takes: a title, one row of summary, the
/// three choices, and what "always" would grant.
///
/// A screen too short even for this is refused rather than squeezed, and the
/// refusal is [`super::layout::fits_panel`]'s to make -- it is the only place
/// that knows what the rest of the band is costing.
const TIGHT_ROWS: u16 = 6;

/// The band's own name for the tool a shell command runs under
/// (`crate::permission::ProposedAction::tool`).
const TERMINAL_TOOL: &str = "terminal";

/// What the panel calls itself.
pub(crate) const TITLE: &str = "Permission needed";

/// The first choice.
const ONCE_CHOICE: &str = "1. Yes";

/// The second, for a command (`approval_ui.zig:1986-1992`).
const ALWAYS_COMMAND: &str = "2. Yes, and don't ask again for this exact command";

/// The second, for everything else xfx advertises.
///
/// Upstream has a third wording, for an MCP server's tools. xfx advertises no
/// MCP tool, so that wording is unreachable here and is not written down as if
/// it were.
const ALWAYS_REQUEST: &str = "2. Yes, and don't ask again for this request";

/// The third, naming the key that means the same thing.
const DENY_CHOICE: &str = "3. No (esc)";

/// What every row but the title is written into, so the panel reads as one
/// block rather than as four left-aligned sentences.
const INDENT: &str = "  ";

/// How many cells [`INDENT`] costs.
const INDENT_CELLS: u16 = 2;

/// What marks the choice Enter would take.
const MARKER: &str = "> ";

/// What a summary too long for the rows it was given ends with.
const ELLIPSIS: char = '\u{2026}';

/// The three answers, in the order they are offered and numbered.
///
/// One array rather than three branches: the digit keys, the arrows, the marker
/// and the row the caret sits on are all indices into it, so "which choice is
/// the second one" cannot be answered differently by the paint and by the key.
const CHOICES: [ApprovalAnswer; 3] = [
    ApprovalAnswer::Once,
    ApprovalAnswer::Always,
    ApprovalAnswer::Deny,
];

/// What one keystroke means to a panel that has the focus.
///
/// The panel's own vocabulary rather than [`super::input::Action`], and the
/// difference is the point: a `1` is a *character* to the decoder and an
/// *answer* here, so the shell translates rather than forwards, and a key the
/// panel does not bind cannot fall through into the composer by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// A character the user typed. Only `1`, `2` and `3` mean anything.
    Text(char),
    /// The previous choice.
    Up,
    /// The next one.
    Down,
    /// The next one, wrapping -- Tab is a cycle rather than a walk.
    Tab,
    /// Take the marked choice.
    Submit,
    /// Refuse.
    Escape,
    /// Refuse. Ctrl-C is a byte here like everywhere else on this surface.
    Cancel,
}

/// Where a question is put in front of the user.
///
/// A property of the **change**, not of the terminal: [`Self::for_request`] is
/// given the question and nothing else -- no rows, no columns -- so that the
/// answer cannot start depending on how big somebody's window happens to be.
/// Whether a screen is too short to *ask* on is a separate and later question,
/// and it is asked **once the surface is known**, of that surface: the band's
/// panel by [`super::layout::fits_panel`], which is the only thing that knows
/// what the rest of the band is costing, and the review plane by
/// [`super::approval_screen::ApprovalScreen::presents_choices`], which owns
/// every row of the screen it takes. Asking one surface's fit question about
/// the other's question is how a short window came to refuse a change the
/// plane can show whole.
///
/// Two variants and no payload: what is settled here is the choice and the
/// state that records it ([`super::shell::ScreenOwner`]); the plane's renderer,
/// its lifecycle and the `1049` bytes that enter and leave it are
/// [`super::approval_screen`]'s and [`super::frame`]'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalSurface {
    /// The band's own panel, with the document still visible above it.
    Inline,
    /// A screen of its own, for a change the band's summary cannot show.
    Alternate,
}

impl ApprovalSurface {
    /// Which surface this question belongs on.
    pub(crate) fn for_request(request: &ApprovalRequest) -> Self {
        match request.diff.as_ref() {
            // A change bigger than the sentence the band quotes. Everything
            // else -- a command, a whole-file write, a directory, an edit the
            // summary already showed whole -- is a question the band answers
            // without hiding the document behind it.
            Some(diff) if diff.wants_screen() => Self::Alternate,
            _ => Self::Inline,
        }
    }
}

/// The question, and which answer is marked.
#[derive(Debug, Clone)]
pub(crate) struct Panel {
    request: ApprovalRequest,
    /// An index into [`CHOICES`].
    selected: usize,
}

/// How many rows each elastic part of the panel gets on a screen of a given
/// height.
///
/// A value rather than three `if`s inside [`Panel::rows`], because every row
/// the panel paints is counted from exactly these numbers -- the height, the
/// caret's row, and the paint itself -- and three readings of the same
/// condition are three chances to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// A blank row under the title.
    breathing: bool,
    /// How many rows the summary may take.
    summary: u16,
    /// A blank row above the choices.
    spaced: bool,
    /// How many rows the "always" disclosure may take.
    scope: u16,
}

impl Shape {
    /// A tall screen's: the summary gets three rows and the disclosure two,
    /// with air around the title and above the choices.
    const SPACIOUS: Self = Self {
        breathing: true,
        summary: 3,
        spaced: true,
        scope: 2,
    };

    /// An ordinary screen's: the blank rows go, because rows above the band
    /// are the user's document and neither disclosure may be paid for out of
    /// them. The marker is what separates the choices from the text, and it is
    /// enough.
    const COMPACT: Self = Self {
        breathing: false,
        summary: 2,
        spaced: false,
        scope: 2,
    };

    /// A short screen's: every blank row goes, and the summary keeps one. What
    /// is being asked and what the answers are is what a question may never
    /// lose.
    const TIGHT: Self = Self {
        breathing: false,
        summary: 1,
        spaced: false,
        scope: 1,
    };

    /// The shape a screen of `terminal_rows` gets.
    fn for_screen(terminal_rows: u16) -> Self {
        if terminal_rows >= SPACIOUS_AT {
            Self::SPACIOUS
        } else if terminal_rows >= COMPACT_AT {
            Self::COMPACT
        } else {
            Self::TIGHT
        }
    }

    /// How many rows this shape paints: a title, the parts above, the three
    /// choices, and the disclosure.
    const fn height(&self) -> u16 {
        1 + self.breathing as u16
            + self.summary
            + self.spaced as u16
            + CHOICES.len() as u16
            + self.scope
    }

    /// Which row of the panel the first choice is on.
    const fn first_choice(&self) -> u16 {
        1 + self.breathing as u16 + self.summary + self.spaced as u16
    }
}

/// The three heights, tied to the three shapes at compile time.
///
/// The constants are what `super::layout` and the plan's own acceptance are
/// written against, and the skeleton in [`Panel::rows`] is what is painted.
/// Without this the two could drift by one blank row and the only symptom would
/// be a band that solved for eight rows and painted nine.
const _: () = {
    assert!(Shape::SPACIOUS.height() == SPACIOUS_ROWS);
    assert!(Shape::COMPACT.height() == COMPACT_ROWS);
    assert!(Shape::TIGHT.height() == TIGHT_ROWS);
};

impl Panel {
    pub(crate) fn new(request: ApprovalRequest) -> Self {
        Self {
            request,
            // Yes-once, which is the safest of the three that is still an
            // answer: an Enter pressed without reading grants one call rather
            // than the rest of the session.
            selected: 0,
        }
    }

    /// The panel's rows, top first, exactly as many as [`Self::height`] says.
    ///
    /// **One iterator for both** (`approval_ui.zig:268-294`): the height is the
    /// length of this, so the count the band solved its geometry from cannot
    /// drift from the paint and leave a stale row standing in the band.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        let shape = Shape::for_screen(terminal_rows);
        let mut rows = Vec::new();
        rows.push(TITLE.to_string());
        if shape.breathing {
            rows.push(String::new());
        }
        // What would happen, in the words the line shell's own prompt uses --
        // including the bounded excerpt of the change, which is where the whole
        // risk of an edit lives.
        rows.extend(fitted(&self.request.summary, cols, shape.summary));
        if shape.spaced {
            rows.push(String::new());
        }
        for (index, choice) in CHOICES.iter().enumerate() {
            let marker = if index == self.selected {
                MARKER
            } else {
                INDENT
            };
            rows.push(format!("{marker}{}", self.label(*choice)));
        }
        // And exactly what "always" would buy, which is the half of the
        // question a three-line menu is most likely to drop. Labelled with the
        // digit rather than introduced with a sentence: the prose would cost a
        // dozen cells of the one row a compact screen has for it, and what
        // matters on that row is the scope, not the grammar.
        rows.extend(fitted(
            &format!("2 = {}", self.request.always_scope),
            cols,
            shape.scope,
        ));
        // Cut to the screen **here**, by the painter's own rule
        // (`super::frame::clip`), rather than left for the painter: a choice
        // whose wording outran a narrow terminal would otherwise be measured by
        // the band at one width and drawn at another.
        rows.iter()
            .map(|row| super::frame::clip(row, cols).to_string())
            .collect()
    }

    /// How many rows the band has to give the panel.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        // The one narrowing in this module, and it is at the terminal-row
        // boundary the standing rule names: the shape bounds the count at
        // [`SPACIOUS_ROWS`], so the clamp is a proof rather than a policy.
        u16::try_from(self.rows(cols, terminal_rows).len()).unwrap_or(SPACIOUS_ROWS)
    }

    /// Which row of the panel the marked choice is on.
    ///
    /// Where the caret goes while the panel has the focus. A caret left in the
    /// composer would be a lie about which of the two the next keystroke goes
    /// to.
    pub(crate) fn caret_row(&self, terminal_rows: u16) -> u16 {
        let selected = u16::try_from(self.selected).unwrap_or(0);
        Shape::for_screen(terminal_rows)
            .first_choice()
            .saturating_add(selected)
    }

    /// The second choice's wording, which says which of the two "always" it is.
    pub(crate) fn always_choice(&self) -> &'static str {
        if self.request.tool == TERMINAL_TOOL {
            ALWAYS_COMMAND
        } else {
            ALWAYS_REQUEST
        }
    }

    /// What one choice is called.
    fn label(&self, choice: ApprovalAnswer) -> &'static str {
        match choice {
            ApprovalAnswer::Once => ONCE_CHOICE,
            ApprovalAnswer::Always => self.always_choice(),
            ApprovalAnswer::Deny => DENY_CHOICE,
        }
    }

    /// What one keystroke does. `Some` is an answer; `None` moved the marker.
    pub(crate) fn apply(&mut self, action: Action) -> Option<ApprovalAnswer> {
        answered(action, &mut self.selected)
    }
}

/// What one keystroke means to whichever surface has the focus.
///
/// A free function rather than a method, because there are two surfaces -- the
/// band's [`Panel`] and the alternate plane's
/// [`super::approval_screen::ApprovalScreen`] -- and "which key means which
/// answer" is one fact about xfx rather than one fact per surface. Two copies
/// would be two chances for a `2` to mean different things depending on how big
/// the change happened to be, which is a decision the *user* never made.
///
/// The digits answer **without** moving the marker, and that is deliberate
/// rather than incidental: a `3` is a refusal, not a refusal plus a marker left
/// on the refusal for whatever key comes next.
pub(crate) fn answered(action: Action, selected: &mut usize) -> Option<ApprovalAnswer> {
    match action {
        Action::Text('1') => Some(ApprovalAnswer::Once),
        Action::Text('2') => Some(ApprovalAnswer::Always),
        Action::Text('3') => Some(ApprovalAnswer::Deny),
        // Every other character. A surface that has the focus swallows them
        // rather than letting them fall into a composer the user cannot see the
        // caret in.
        Action::Text(_) => None,
        Action::Up => {
            *selected = (*selected + CHOICES.len() - 1) % CHOICES.len();
            None
        }
        Action::Down | Action::Tab => {
            *selected = (*selected + 1) % CHOICES.len();
            None
        }
        Action::Submit => Some(CHOICES[*selected]),
        // A decision xfx was never given is a refusal.
        Action::Escape | Action::Cancel => Some(ApprovalAnswer::Deny),
    }
}

/// What the three choices are called, for a question about `tool`.
///
/// In the order [`CHOICES`] numbers them, so an index is one thing on both
/// surfaces.
pub(crate) fn labels(tool: &str) -> [&'static str; CHOICES.len()] {
    let always = if tool == TERMINAL_TOOL {
        ALWAYS_COMMAND
    } else {
        ALWAYS_REQUEST
    };
    [ONCE_CHOICE, always, DENY_CHOICE]
}

/// `text` wrapped into exactly `rows` rows of a `cols`-wide screen, indented.
///
/// Padded when it is shorter, because the panel's height is settled before its
/// text is and a row the band owns but nothing writes is a row the last frame's
/// text stays on. Cut with an [`ELLIPSIS`] when it is longer, because a summary
/// that stopped mid-word without saying so would read as the whole of what xfx
/// was about to do.
fn fitted(text: &str, cols: u16, rows: u16) -> Vec<String> {
    let budget = cols.saturating_sub(INDENT_CELLS).max(1);
    let wrapped = super::wrap::wrap(text, budget);
    let allotted = usize::from(rows);
    let mut out: Vec<String> = wrapped
        .iter()
        .take(allotted)
        .map(|row| {
            format!(
                "{INDENT}{}",
                text[row.start..row.end].trim_end_matches(['\r', '\n'])
            )
        })
        .collect();
    if wrapped.len() > allotted {
        if let Some(last) = out.last_mut() {
            // One cell is kept back for the ellipsis, by the painter's own cut
            // (`super::frame::clip`) so that the two cannot disagree about
            // where a row ends.
            let kept = super::frame::clip(last, cols.saturating_sub(1)).to_string();
            *last = format!("{kept}{ELLIPSIS}");
        }
    }
    out.resize(allotted, String::new());
    out
}

// ---------------------------------------------------------------------------
// the channel the answer comes back on
// ---------------------------------------------------------------------------

/// The control channel, read by the turn loop and by the prompter it parks.
///
/// **They never run at the same instant**, and the reason is the whole design:
/// the prompter runs on the runtime thread, inside a poll of the very future
/// `super::worker`'s `raced_against_control` is racing, so while the prompter
/// holds this receiver the loop is not running -- it is somewhere down the
/// stack, in the call that parked. The lock is therefore never contended and
/// never held across an `await`; it is here because the borrow checker cannot
/// see the argument above, not because two threads share a queue.
pub(crate) struct ControlChannel {
    rx: Mutex<UnboundedReceiver<TurnControl>>,
    /// The turn loop's task waker, remembered at each of its own polls.
    ///
    /// See the module header: the receiver holds one waker, the prompter's
    /// park-waker replaces it, and the wake that frees the prompter consumes
    /// it. This is how the loop's is put back.
    loop_waker: Mutex<Option<Waker>>,
    /// Messages the prompter had to consume to see past, waiting for the loop
    /// they were addressed to.
    ///
    /// A Ctrl-C that lands while the panel is up answers the panel -- with a
    /// refusal, because that is what a decision xfx was not given is -- and
    /// then goes on being a Ctrl-C: the turn it was pressed at still has to
    /// stop, and only the loop can stop one. Consuming it without this queue
    /// would make an interrupt typed at a panel a refusal *instead of* an
    /// interrupt.
    put_back: Mutex<VecDeque<TurnControl>>,
}

impl ControlChannel {
    pub(crate) fn new(rx: UnboundedReceiver<TurnControl>) -> Arc<Self> {
        Arc::new(Self {
            rx: Mutex::new(rx),
            loop_waker: Mutex::new(None),
            put_back: Mutex::new(VecDeque::new()),
        })
    }

    /// The turn loop's own receive.
    ///
    /// Everything the prompter put back comes out here first, and the check is
    /// inside the poll rather than in front of it: a message deposited *after*
    /// this future was created still has to be seen, and the wake that
    /// accompanies it is what brings the poll round again.
    pub(crate) async fn recv(&self) -> Option<TurnControl> {
        std::future::poll_fn(|cx| {
            if let Some(message) = self.put_back.guarded().pop_front() {
                return Poll::Ready(Some(message));
            }
            let mut slot = self.loop_waker.guarded();
            if !slot.as_ref().is_some_and(|held| held.will_wake(cx.waker())) {
                *slot = Some(cx.waker().clone());
            }
            drop(slot);
            self.rx.guarded().poll_recv(cx)
        })
        .await
    }

    /// The prompter's receive, from the thread it parked.
    ///
    /// Nothing put back is read here: the prompter is what puts messages back,
    /// and a prompter that re-read its own would answer the same interrupt
    /// twice and never hand it to the loop that can act on it.
    async fn answered(&self) -> Option<TurnControl> {
        std::future::poll_fn(|cx| {
            let polled = self.rx.guarded().poll_recv(cx);
            if polled.is_ready() {
                self.rearm();
            }
            polled
        })
        .await
    }

    /// Hands a message the prompter could not act on back to the loop.
    fn give_back(&self, message: TurnControl) {
        self.put_back.guarded().push_back(message);
        self.rearm();
    }

    /// Wakes the turn loop, so its `select!` registers on this channel again.
    ///
    /// Spurious as far as the loop is concerned -- it polls, finds whatever is
    /// there or nothing, and registers -- and that is the point: the
    /// registration is the thing, and without it the next control message would
    /// wake a waker the prompter's wait already consumed.
    fn rearm(&self) {
        if let Some(waker) = self.loop_waker.guarded().take() {
            waker.wake();
        }
    }

    /// What is waiting, for a test that asks rather than awaits.
    #[cfg(test)]
    fn waiting(&self) -> Option<TurnControl> {
        self.put_back
            .guarded()
            .pop_front()
            .or_else(|| self.rx.guarded().try_recv().ok())
    }
}

/// The lock, taken past a poisoning rather than panicking on one.
///
/// Every guard in this module is held for the length of one non-panicking
/// statement, so a poisoned lock here means some *other* code panicked while
/// this thread happened to hold it -- and a runtime thread whose turn panicked
/// is one the UI is already being told about ([`UiEvent::Fatal`]). Turning that
/// into a second panic inside the approval channel would replace a reportable
/// failure with an unreportable one.
trait Guarded<T> {
    fn guarded(&self) -> MutexGuard<'_, T>;
}

impl<T> Guarded<T> for Mutex<T> {
    fn guarded(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Asks the person at the terminal through the band, and waits for the band's
/// answer.
///
/// Built once per conversation and held by the session's
/// [`crate::permission::PermissionSession`], which is what makes an "always"
/// answer worth what it says: a prompter rebuilt per turn would sit in a
/// session rebuilt per turn, and the grant would expire with the turn that gave
/// it.
#[derive(Clone)]
pub(crate) struct TuiPrompter {
    events: Sender<UiEvent>,
    control: Arc<ControlChannel>,
    /// The **session's** cancellation, not a turn's.
    ///
    /// A turn's token is minted per turn (`super::bridge::Cancellation::turn`)
    /// and this prompter outlives every one of them, so a token captured at
    /// construction would be the first turn's -- cancelled by the time the
    /// second turn asked anything, and every later panel would be abandoned
    /// before it was painted. The session's root is the question this send
    /// really wants answered: is there still a UI to ask.
    cancel: Cancellation,
}

impl TuiPrompter {
    pub(crate) fn new(
        events: Sender<UiEvent>,
        control: Arc<ControlChannel>,
        cancel: Cancellation,
    ) -> Self {
        Self {
            events,
            control,
            cancel,
        }
    }
}

/// What a prompter returns when there is no longer anybody to ask.
///
/// The same error `TtyPrompter` returns when its terminal goes away, so that
/// `PermissionSession::ask` reports one fact -- the approval channel failed --
/// however the channel was built.
fn nobody_to_ask() -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "the terminal closed before answering",
    )
}

impl ApprovalPrompter for TuiPrompter {
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer> {
        let token = self.cancel.token();
        // Through `send_ui`, which makes the event inert at the channel: the
        // summary quotes a bounded excerpt of the file a call would change,
        // which is the likeliest place in the whole product for an escape
        // sequence to be sitting.
        match park_on(send_ui(
            &self.events,
            &token,
            UiEvent::Approval(request.clone()),
        )) {
            Ok(()) => {}
            // The session is going down. A question nobody will see is not one
            // to wait for an answer to, and the answer xfx does not have is no.
            Err(Stopped::Cancelled) => return Ok(ApprovalAnswer::Deny),
            Err(Stopped::UiGone) => return Err(nobody_to_ask()),
        }
        match park_on(self.control.answered()) {
            Some(TurnControl::Answer(answer)) => Ok(answer),
            // A Ctrl-C or a shutdown. Both refuse this call, and both go back
            // on the channel for the loop that can act on them -- see
            // [`ControlChannel::put_back`].
            Some(stop) => {
                self.control.give_back(stop);
                Ok(ApprovalAnswer::Deny)
            }
            None => Err(nobody_to_ask()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::permission::ApprovalDiff;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Wake};

    use tokio::sync::mpsc;

    fn request(tool: &'static str) -> ApprovalRequest {
        ApprovalRequest {
            tool,
            target: "notes.txt".into(),
            summary: "replace `alpha` with `beta` in notes.txt".into(),
            always_scope:
                "allow every future edit_file to `notes.txt` for the rest of this session".into(),
            diff: None,
        }
    }

    /// The same question, carrying a change of `bytes` on each side.
    fn with_diff(bytes: usize) -> ApprovalRequest {
        let mut asked = request("edit_file");
        asked.diff = Some(ApprovalDiff {
            before: "a".repeat(bytes),
            after: "b".repeat(bytes),
        });
        asked
    }

    #[test]
    fn a_change_bigger_than_the_bands_own_summary_is_reviewed_on_a_screen_of_its_own() {
        // The rule is about the *change*, and [`ApprovalSurface::for_request`]
        // is given nothing else to decide from -- no rows, no columns. A rule
        // keyed on the terminal's height would put a one-word edit on a full
        // screen the moment somebody made their window short, and would leave a
        // hundred-kilobyte replacement in a two-row summary on a tall one.
        assert_eq!(
            ApprovalSurface::for_request(&with_diff(161)),
            ApprovalSurface::Alternate
        );
        let mut one_sided = request("edit_file");
        one_sided.diff = Some(ApprovalDiff {
            before: String::new(),
            after: "b".repeat(161),
        });
        assert_eq!(
            ApprovalSurface::for_request(&one_sided),
            ApprovalSurface::Alternate,
            "a change that only adds is still a change too big for the band"
        );
    }

    #[test]
    fn a_small_change_and_a_question_with_no_diff_at_all_stay_in_the_band() {
        // Two separate cases with one answer. A command has no diff to review;
        // an edit whose whole before and after the summary already quotes has
        // nothing a second surface would add, and taking the screen for it
        // would hide the document to say what the band just said.
        assert_eq!(
            ApprovalSurface::for_request(&request("terminal")),
            ApprovalSurface::Inline
        );
        assert_eq!(
            ApprovalSurface::for_request(&request("edit_file")),
            ApprovalSurface::Inline
        );
        assert_eq!(
            ApprovalSurface::for_request(&with_diff(160)),
            ApprovalSurface::Inline
        );
    }

    #[test]
    fn the_always_wording_says_which_of_the_two_it_is() {
        // approval_ui.zig:1986-1992. xfx advertises no MCP tool, so upstream's
        // third wording is unreachable here and is not written down as if it
        // were.
        assert_eq!(
            Panel::new(request("terminal")).always_choice(),
            "2. Yes, and don't ask again for this exact command"
        );
        assert_eq!(
            Panel::new(request("edit_file")).always_choice(),
            "2. Yes, and don't ask again for this request"
        );
    }

    #[test]
    fn the_panel_discloses_what_always_would_grant() {
        // The line shell's prompt says this, so the panel that replaces it must
        // too -- a narrower safety surface is a regression, not a port.
        let panel = Panel::new(request("edit_file"));
        let rows = panel.rows(80, 24).join("\n");
        assert!(rows.contains("replace `alpha` with `beta`"), "{rows}");
        assert!(rows.contains("for the rest of this session"), "{rows}");
    }

    #[test]
    fn the_measured_height_is_the_number_of_rows_actually_painted() {
        // approval_ui.zig:268-294: one iterator for both, so the count cannot
        // drift from the paint and leave a stale row in the band.
        for (cols, terminal_rows) in [(80u16, 24u16), (40, 24), (80, 40), (30, 40)] {
            let panel = Panel::new(request("terminal"));
            assert_eq!(
                panel.height(cols, terminal_rows) as usize,
                panel.rows(cols, terminal_rows).len(),
                "{cols}x{terminal_rows}"
            );
        }
        assert!(Panel::new(request("terminal")).height(80, 40) >= SPACIOUS_ROWS);
        assert!(Panel::new(request("terminal")).height(80, 24) <= COMPACT_ROWS);
    }

    #[test]
    fn every_key_the_panel_answers_to_produces_the_documented_outcome() {
        // ui/input/runtime.zig:65-77
        let mut panel = Panel::new(request("edit_file"));
        assert_eq!(panel.apply(Action::Text('1')), Some(ApprovalAnswer::Once));
        assert_eq!(panel.apply(Action::Text('2')), Some(ApprovalAnswer::Always));
        assert_eq!(panel.apply(Action::Text('3')), Some(ApprovalAnswer::Deny));
        assert_eq!(panel.apply(Action::Escape), Some(ApprovalAnswer::Deny));
        assert_eq!(panel.apply(Action::Cancel), Some(ApprovalAnswer::Deny));
        assert_eq!(panel.apply(Action::Down), None, "moving is not answering");
        assert_eq!(panel.apply(Action::Submit), Some(ApprovalAnswer::Always));
        panel.apply(Action::Tab);
        assert_eq!(panel.apply(Action::Submit), Some(ApprovalAnswer::Deny));
    }

    #[test]
    fn the_arrows_and_tab_walk_the_same_three_choices_and_wrap() {
        // Without the wrap, an Up at the top and a Tab at the bottom would each
        // be a keystroke that does nothing, on a panel whose whole job is to be
        // answerable without reading a manual.
        let mut panel = Panel::new(request("edit_file"));
        assert_eq!(panel.apply(Action::Up), None);
        assert_eq!(
            panel.apply(Action::Submit),
            Some(ApprovalAnswer::Deny),
            "Up from the first choice did not wrap to the last"
        );
        assert_eq!(panel.apply(Action::Tab), None);
        assert_eq!(
            panel.apply(Action::Submit),
            Some(ApprovalAnswer::Once),
            "Tab from the last choice did not wrap to the first"
        );
        assert_eq!(panel.apply(Action::Down), None);
        assert_eq!(panel.apply(Action::Up), None);
        assert_eq!(
            panel.apply(Action::Submit),
            Some(ApprovalAnswer::Once),
            "a step each way did not come back"
        );
    }

    #[test]
    fn the_marker_is_on_the_choice_enter_would_take_and_the_caret_is_on_that_row() {
        // The two halves of "which one is selected" -- what the eye reads and
        // where the terminal puts the caret -- from one index, so they cannot
        // point at different rows.
        let mut panel = Panel::new(request("edit_file"));
        for expected in [0usize, 1, 2] {
            let rows = panel.rows(80, 24);
            let marked: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.starts_with(MARKER))
                .map(|(index, _)| index)
                .collect();
            assert_eq!(marked.len(), 1, "{rows:?}");
            assert_eq!(
                marked[0],
                usize::from(panel.caret_row(24)),
                "the caret is not on the marked row"
            );
            assert!(
                rows[marked[0]].contains(&format!("{}.", expected + 1)),
                "{rows:?}"
            );
            panel.apply(Action::Down);
        }
    }

    #[test]
    fn a_summary_longer_than_its_rows_is_cut_where_the_painter_would_cut_it() {
        // A summary that stopped mid-word without saying so would read as the
        // whole of what xfx was about to do.
        let mut asked = request("edit_file");
        asked.summary = "x".repeat(4000);
        let panel = Panel::new(asked);
        let rows = panel.rows(40, 24);
        assert_eq!(rows.len(), usize::from(COMPACT_ROWS));
        let summary: Vec<&String> = rows[1..3].iter().collect();
        assert!(
            summary[1].ends_with(ELLIPSIS),
            "the cut is silent: {summary:?}"
        );
        for row in &rows {
            assert!(
                super::super::wrap::width(row) <= 40,
                "a panel row outgrew the screen: {row:?}"
            );
        }
    }

    #[test]
    fn a_short_screen_keeps_the_choices_and_gives_up_the_blank_rows() {
        // The reduction has to be about what a question can lose. The three
        // answers and what is being asked cannot be among them.
        let panel = Panel::new(request("edit_file"));
        let rows = panel.rows(80, 12);
        assert_eq!(rows.len(), usize::from(TIGHT_ROWS));
        let joined = rows.join("\n");
        assert!(joined.contains(TITLE), "{joined}");
        assert!(joined.contains(ONCE_CHOICE), "{joined}");
        assert!(joined.contains(ALWAYS_REQUEST), "{joined}");
        assert!(joined.contains(DENY_CHOICE), "{joined}");
        assert!(joined.contains("for the rest of this session"), "{joined}");
        assert!(
            !rows.iter().any(String::is_empty),
            "a short panel spent a row on nothing: {rows:?}"
        );
    }

    #[test]
    fn a_taller_screen_spends_the_rows_it_has_on_the_summary_rather_than_on_air() {
        let mut asked = request("edit_file");
        asked.summary = "alpha bravo charlie delta echo foxtrot golf hotel ".repeat(4);
        let panel = Panel::new(asked);
        let compact = panel.rows(60, 24);
        let spacious = panel.rows(60, 40);
        assert_eq!(compact.len(), usize::from(COMPACT_ROWS));
        assert_eq!(spacious.len(), usize::from(SPACIOUS_ROWS));
        let told = |rows: &[String]| {
            rows.iter()
                .filter(|row| {
                    row.contains("alpha") || row.contains("bravo") || row.contains("golf")
                })
                .count()
        };
        assert!(
            told(&spacious) > told(&compact),
            "the taller panel said no more than the short one: {spacious:?}"
        );
    }

    // -----------------------------------------------------------------------
    // the channel
    // -----------------------------------------------------------------------

    /// A waker that counts, so "the loop was woken" is a number.
    struct Counting(AtomicUsize);

    impl Wake for Counting {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Release);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }

    /// A waker whose wakes nobody is counting, for the half of a test that is
    /// only registering.
    fn inert_waker() -> Waker {
        Waker::from(Arc::new(Counting(AtomicUsize::new(0))))
    }

    #[test]
    fn the_prompters_wait_puts_the_turn_loops_waker_back() {
        // The hazard this exists for, reproduced in production order: the loop
        // registers, the prompter's own wait *replaces* the registration
        // without waking it, and the send that frees the prompter consumes what
        // it replaced it with. Coming out of that with nothing registered would
        // lose the next Ctrl-C for as long as the turn's body stayed parked on
        // something quiet -- which is the exact case the loop's listen exists
        // for.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let woken = Arc::new(Counting(AtomicUsize::new(0)));
        let loop_waker = Waker::from(Arc::clone(&woken));

        let mut listening = Box::pin(control.recv());
        assert!(listening
            .as_mut()
            .poll(&mut Context::from_waker(&loop_waker))
            .is_pending());

        let park = inert_waker();
        let mut asking = Box::pin(control.answered());
        assert!(asking
            .as_mut()
            .poll(&mut Context::from_waker(&park))
            .is_pending());
        tx.send(TurnControl::Answer(ApprovalAnswer::Once))
            .expect("the channel is open");
        assert_eq!(
            woken.0.load(Ordering::Acquire),
            0,
            "the loop's waker was still registered, so this test proves nothing"
        );

        assert_eq!(
            asking.as_mut().poll(&mut Context::from_waker(&park)),
            Poll::Ready(Some(TurnControl::Answer(ApprovalAnswer::Once)))
        );
        assert_eq!(
            woken.0.load(Ordering::Acquire),
            1,
            "the turn loop was never woken, so it never registered again"
        );
    }

    #[test]
    fn a_message_put_back_reaches_the_loop_even_though_it_was_already_waiting() {
        // The put-back is deposited *after* the loop's future was created and
        // polled, which is the only order production ever produces: the
        // prompter runs inside a poll of the body the loop is racing.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let waker = inert_waker();
        let mut listening = Box::pin(control.recv());
        assert!(listening
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending());

        control.give_back(TurnControl::Cancel { through: 7 });
        assert_eq!(
            listening.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(Some(TurnControl::Cancel { through: 7 })),
            "an interrupt the panel answered never reached the turn it was about"
        );
        drop(tx);
    }

    #[test]
    fn an_answer_already_waiting_is_taken_without_parking() {
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Answer(ApprovalAnswer::Always))
            .expect("the channel is open");
        let (events, _seen) = mpsc::channel(4);
        let mut prompter = TuiPrompter::new(
            events,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter.request(&request("edit_file")).expect("an answer"),
            ApprovalAnswer::Always
        );
    }

    #[test]
    fn the_question_reaches_the_ui_before_the_answer_is_waited_for() {
        let (events, mut seen) = mpsc::channel(4);
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Answer(ApprovalAnswer::Once))
            .expect("the channel is open");
        let mut prompter = TuiPrompter::new(
            events,
            control,
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        let asked = request("edit_file");
        assert_eq!(
            prompter.request(&asked).expect("an answer"),
            ApprovalAnswer::Once
        );
        assert_eq!(
            seen.try_recv().expect("the UI was asked"),
            UiEvent::Approval(asked)
        );
    }

    #[test]
    fn a_question_carrying_an_escape_sequence_reaches_the_band_inert() {
        // The summary quotes a bounded excerpt of the file a call would change,
        // so this is the likeliest place in the product for a sequence the
        // terminal would obey to be sitting.
        let (events, mut seen) = mpsc::channel(4);
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Answer(ApprovalAnswer::Once))
            .expect("the channel is open");
        let mut prompter = TuiPrompter::new(
            events,
            control,
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        let mut asked = request("edit_file");
        asked.summary = "edit `notes.txt`: \u{1b}[2Jgone".to_string();
        prompter.request(&asked).expect("an answer");
        let UiEvent::Approval(delivered) = seen.try_recv().expect("the UI was asked") else {
            panic!("the UI was told something other than a question");
        };
        assert!(
            !delivered.summary.contains('\u{1b}'),
            "an escape sequence reached the band: {:?}",
            delivered.summary
        );
    }

    #[test]
    fn an_interrupt_answers_the_panel_with_a_refusal_and_still_stops_the_turn() {
        // Both halves. Answering it and eating it would make a Ctrl-C typed at
        // a panel a refusal *instead of* an interrupt.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Cancel { through: 3 })
            .expect("the channel is open");
        let (events, _seen) = mpsc::channel(4);
        let mut prompter = TuiPrompter::new(
            events,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter.request(&request("edit_file")).expect("an answer"),
            ApprovalAnswer::Deny
        );
        assert_eq!(control.waiting(), Some(TurnControl::Cancel { through: 3 }));
    }

    #[test]
    fn a_shutdown_refuses_and_is_handed_on_rather_than_swallowed() {
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Shutdown).expect("the channel is open");
        let (events, _seen) = mpsc::channel(4);
        let mut prompter = TuiPrompter::new(
            events,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter.request(&request("edit_file")).expect("an answer"),
            ApprovalAnswer::Deny
        );
        assert_eq!(control.waiting(), Some(TurnControl::Shutdown));
    }

    #[test]
    fn a_ui_that_is_gone_fails_the_channel_rather_than_waiting_for_a_person() {
        // Both directions of gone, because they are different facts: the event
        // channel's receiver dropped, and the control channel's sender dropped.
        let (events, seen) = mpsc::channel(4);
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut prompter = TuiPrompter::new(
            events,
            ControlChannel::new(rx),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter
                .request(&request("edit_file"))
                .expect_err("a question nobody can answer")
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
        drop(seen);

        let (events, seen) = mpsc::channel(4);
        drop(seen);
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut prompter = TuiPrompter::new(
            events,
            ControlChannel::new(rx),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter
                .request(&request("edit_file"))
                .expect_err("a question nobody can see")
                .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn a_session_that_is_shutting_down_refuses_rather_than_asking() {
        // Never an allow: a question xfx cannot get an answer to is a no.
        let (events, mut seen) = mpsc::channel(4);
        let (_tx, rx) = mpsc::unbounded_channel();
        let cancel = Cancellation::new(crate::gateway::CancelToken::new());
        cancel.cancel();
        let mut prompter = TuiPrompter::new(events, ControlChannel::new(rx), cancel);
        assert_eq!(
            prompter.request(&request("edit_file")).expect("an answer"),
            ApprovalAnswer::Deny
        );
        assert!(
            seen.try_recv().is_err(),
            "a cancelled session painted a panel nobody would answer"
        );
    }
}
