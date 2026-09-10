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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use super::approval_readiness::{ApprovalId, Disclosure};
use super::bridge::{park_on, send_ui, Cancellation, Stopped, TurnControl, UiEvent};
use crate::permission::{ApprovalAnswer, ApprovalPrompter, ApprovalRequest};

// ---------------------------------------------------------------------------
// what the panel says
// ---------------------------------------------------------------------------

/// The screen height at which the panel can afford two blank rows.
///
/// The one thing about this panel's height that is still a property of the
/// *screen* rather than of the request. Everything else is measured from the
/// content ([`Shape::for_request`]): the three fixed heights that used to live
/// here allotted the always-scope two rows, every scope
/// `crate::permission::PermissionSession` builds is three at eighty columns,
/// and a question whose scope is cut is a question no frame can disclose.
pub(crate) const SPACIOUS_AT: u16 = 34;

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

/// One question, under the identity it was asked with.
///
/// The envelope the UI is told about a question in. A pair rather than a field
/// on [`crate::permission::ApprovalRequest`] because the id is a **TUI** fact:
/// `crate::permission` neither mints one nor reads one, and giving its request
/// a field only this front end fills would make the policy layer carry a
/// surface's bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovalAsked {
    pub id: ApprovalId,
    pub request: ApprovalRequest,
}

/// One composition of a question, and what it really disclosed.
///
/// The rows and the claim about them come out of **one** construction, for the
/// reason [`Panel::rows`] and [`Panel::height`] do: a disclosure computed by a
/// second pass over the composed text would be a second reading of the layout,
/// and two readings are two chances to disagree about whether a control was cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Composition {
    pub rows: Vec<String>,
    pub disclosure: Disclosure,
}

/// `text` fitted into its rows, and whether all of it got there.
///
/// `complete` is a fact about the **source** and its allotment, not about the
/// string this returns: the cut is silent -- an ellipsis on the last row that
/// survived -- so a caller inspecting the output could only guess whether a
/// summary ending in `\u{2026}` was cut or merely ends that way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fitted {
    pub rows: Vec<String>,
    pub complete: bool,
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

/// How many rows `text` really needs at `budget` cells, never fewer than one.
///
/// The **demand**, measured from the source. A row count chosen without asking
/// this is a row count that cuts something, and every cut on this surface is
/// silent ([`fitted`]).
fn demanded(text: &str, budget: u16) -> u16 {
    u16::try_from(super::wrap::wrap(text, budget).len().max(1)).unwrap_or(u16::MAX)
}

impl Shape {
    /// The shape this question really needs at `cols`, on a screen
    /// `terminal_rows` tall.
    ///
    /// **Measured from the content, not read out of a table.** The three fixed
    /// shapes this replaced allotted the always-scope two rows on an ordinary
    /// screen and one on a short one, and every scope
    /// `crate::permission::PermissionSession` builds is three wrapped rows at
    /// eighty columns: the suffix `always_scope_for` appends -- the resume-id of
    /// a saved session, or the note that this turn is not being recorded -- is
    /// unconditional. A table cannot know that, so the table cut the sentence
    /// and no ordinary screen could disclose an ordinary request.
    ///
    /// **Padding is what is spent first.** A blank row discloses nothing, so
    /// the two are taken only on a screen tall enough that they cost the
    /// question nothing ([`SPACIOUS_AT`]); everything else is demand. Whether
    /// the demand fits the band at all is [`super::layout::fits_panel`]'s
    /// question, asked of [`Panel::height`], and a question that does not fit
    /// is refused in the user's sight rather than squeezed.
    /// **Bounded by the screen.** A demand larger than the terminal is still
    /// only measured up to it, so the panel never claims more rows than exist
    /// and [`fitted`]'s ellipsis marks what was lost -- which makes the
    /// disclosure `false` and the question refusable rather than grantable. The
    /// alternative, a panel of a hundred rows on a screen of twenty-four, would
    /// be a height every reader of it has to re-clamp.
    fn for_request(request: &ApprovalRequest, cols: u16, terminal_rows: u16) -> Self {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        let air = terminal_rows >= SPACIOUS_AT;
        // A title, the three answers, and one row each for the two elastic
        // parts: the least this panel can be.
        let fixed = 1 + 2 * u16::from(air) + CHOICES.len() as u16;
        let scope = demanded(&format!("2 = {}", request.always_scope), budget)
            .min(terminal_rows.saturating_sub(fixed + 1).max(1));
        Self {
            breathing: air,
            // The summary is where the band names the target
            // (`crate::permission`'s `ApprovalRequest`), so a summary cut here
            // is a target the user was never shown.
            summary: demanded(&request.summary, budget)
                .min(terminal_rows.saturating_sub(fixed + scope).max(1)),
            spaced: air,
            scope,
        }
    }

    /// Which row of the panel the first choice is on.
    const fn first_choice(&self) -> u16 {
        1 + self.breathing as u16 + self.summary + self.spaced as u16
    }
}

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
        // The absolute row the disclosure would carry is nothing to a caller
        // that only wants the text, so this asks for the composition at the top
        // of a notional screen and takes the half it is about. **One**
        // construction, which is what keeps the height, the caret and the paint
        // from drifting apart.
        self.compose(cols, terminal_rows, 1, 0).rows
    }

    /// The panel's rows and what they really disclosed, for a panel whose first
    /// row is at terminal row `band_top + offset`.
    ///
    /// The two placing arguments are readiness's alone. `band_rows` puts the
    /// activity row before the panel when the geometry has one
    /// ([`super::shell::Shell::band_rows`]), so a panel-local index is not a row
    /// of anybody's terminal -- and `Intent::capture`'s `last_control_row <=
    /// rows` guard is about the terminal.
    pub(crate) fn compose(
        &self,
        cols: u16,
        terminal_rows: u16,
        band_top: u16,
        offset: u16,
    ) -> Composition {
        let shape = Shape::for_request(&self.request, cols, terminal_rows);
        let mut rows = Vec::new();
        rows.push(TITLE.to_string());
        if shape.breathing {
            rows.push(String::new());
        }
        // What would happen, in the words the line shell's own prompt uses --
        // including the bounded excerpt of the change, which is where the whole
        // risk of an edit lives.
        let summary = fitted(&self.request.summary, cols, shape.summary);
        rows.extend(summary.rows);
        if shape.spaced {
            rows.push(String::new());
        }
        // **Whether the clip changed anything**, not whether something survived
        // it. `super::approval_screen::ApprovalScreen::presents_choices` asks
        // the weaker question -- is the row longer than its marker -- and that
        // is the right question for "may xfx ask here at all"; it is the wrong
        // one for "may this be granted", because `2. Yes, and don't ask again
        // for th` passes it while the words that separate one call from the rest
        // of the session are the ones that got cut.
        let mut controls_whole = true;
        let mut last_control = 0usize;
        for (index, choice) in CHOICES.iter().enumerate() {
            let marker = if index == self.selected {
                MARKER
            } else {
                INDENT
            };
            let row = format!("{marker}{}", self.label(*choice));
            controls_whole &= super::frame::clip(&row, cols) == row;
            last_control = rows.len();
            rows.push(row);
        }
        // And exactly what "always" would buy, which is the half of the
        // question a three-line menu is most likely to drop. Labelled with the
        // digit rather than introduced with a sentence: the prose would cost a
        // dozen cells of the one row a compact screen has for it, and what
        // matters on that row is the scope, not the grammar.
        let scope = fitted(
            &format!("2 = {}", self.request.always_scope),
            cols,
            shape.scope,
        );
        rows.extend(scope.rows);
        let disclosure = Disclosure {
            // The panel's first row is at `band_top + offset`, so a local index
            // needs no further one-based correction: `band_top` is already a
            // terminal row as the terminal counts them
            // ([`super::layout::Geometry::band_top`]).
            last_control_row: band_top
                .saturating_add(offset)
                .saturating_add(u16::try_from(last_control).unwrap_or(u16::MAX)),
            controls_whole,
            scope_whole: scope.complete,
            // The summary carries the target (`crate::permission`'s
            // `ApprovalRequest`), so a summary that reached its rows whole is a
            // subject the user really saw.
            subject_whole: summary.complete,
            // An inline question is only ever chosen for a change the summary
            // shows whole ([`ApprovalSurface::for_request`]), so on this surface
            // the change **is** the subject.
            change_visible: summary.complete,
        };
        Composition {
            // Cut to the screen **here**, by the painter's own rule
            // (`super::frame::clip`), rather than left for the painter: a
            // choice whose wording outran a narrow terminal would otherwise be
            // measured by the band at one width and drawn at another.
            rows: rows
                .iter()
                .map(|row| super::frame::clip(row, cols).to_string())
                .collect(),
            disclosure,
        }
    }

    /// How many rows the band has to give the panel.
    ///
    /// **Content-measured**, so it really is a question of the request as well
    /// as of the screen. What the band does when the answer is more rows than
    /// it has is [`super::layout::fits_panel`]'s, and the shell refuses in the
    /// user's sight rather than painting a question with half its disclosure.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        u16::try_from(self.rows(cols, terminal_rows).len()).unwrap_or(u16::MAX)
    }

    /// Which row of the panel the marked choice is on.
    ///
    /// Where the caret goes while the panel has the focus. A caret left in the
    /// composer would be a lie about which of the two the next keystroke goes
    /// to.
    ///
    /// **Takes `cols`**, because the rows above the choices are now measured
    /// from text that wraps at a width: a caret derived from a different
    /// reading of the layout than the paint would sit on the wrong row the
    /// moment a summary or a scope needed one row more. The same shape function
    /// answers both ([`Shape::for_request`]), which is what
    /// `super::question::QuestionPanel::caret_row` already does with the same
    /// two arguments.
    pub(crate) fn caret_row(&self, cols: u16, terminal_rows: u16) -> u16 {
        let selected = u16::try_from(self.selected).unwrap_or(0);
        Shape::for_request(&self.request, cols, terminal_rows)
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
///
/// `complete` says the source reached the screen whole: every wrapped row of it
/// fitted in the allotment, so nothing was dropped and no ellipsis was added.
fn fitted(text: &str, cols: u16, rows: u16) -> Fitted {
    let budget = cols.saturating_sub(INDENT_CELLS).max(1);
    let wrapped = super::wrap::wrap(text, budget);
    let allotted = usize::from(rows);
    let complete = wrapped.len() <= allotted;
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
    Fitted {
        rows: out,
        complete,
    }
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
    ///
    /// `pub(crate)` because the prompter is no longer the only parked reader:
    /// `super::question::TuiQuestioner` waits here too, for the same interval
    /// and under the same rule -- the loop is somewhere down the stack while it
    /// does. **A caller that polls this to ready has consumed the loop's waker**
    /// and owes it a [`rearm`](Self::rearm) on every exit.
    pub(crate) async fn answered(&self) -> Option<TurnControl> {
        std::future::poll_fn(|cx| {
            let polled = self.rx.guarded().poll_recv(cx);
            if polled.is_ready() {
                self.rearm();
            }
            polled
        })
        .await
    }

    /// Hands a message a parked reader could not act on back to the loop.
    pub(crate) fn give_back(&self, message: TurnControl) {
        self.put_back.guarded().push_back(message);
        self.rearm();
    }

    /// Wakes the turn loop, so its `select!` registers on this channel again.
    ///
    /// Spurious as far as the loop is concerned -- it polls, finds whatever is
    /// there or nothing, and registers -- and that is the point: the
    /// registration is the thing, and without it the next control message would
    /// wake a waker the prompter's wait already consumed.
    pub(crate) fn rearm(&self) {
        if let Some(waker) = self.loop_waker.guarded().take() {
            waker.wake();
        }
    }

    /// What is waiting, for a test that asks rather than awaits.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> Option<TurnControl> {
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
    /// The next id to mint, shared across clones.
    ///
    /// Cloning shares the counter as well as the channel, because it **is**
    /// identity: two clones minting the same id would make a stale answer
    /// indistinguishable from a fresh one, which is the one thing the id exists
    /// to prevent. The same arrangement, for the same reason, as
    /// [`super::question::TuiQuestioner`]'s.
    next: Arc<AtomicU64>,
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
            // One rather than nought, so the first question is `ApprovalId(1)`
            // and an `ApprovalId(0)` in a log is a value nothing minted.
            next: Arc::new(AtomicU64::new(1)),
        }
    }

    /// The id this question is asked under, or nothing when one cannot be
    /// minted.
    ///
    /// **Fails closed.** `fetch_add` wraps, and a wrapped counter would hand out
    /// an id some earlier question was asked under -- at which point a keystroke
    /// left over from that one is accepted as a permission decision about this
    /// one. A question that cannot be given an identity is not asked at all.
    fn mint(&self) -> Option<ApprovalId> {
        self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                held.checked_add(1)
            })
            .ok()
            .map(ApprovalId)
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
        // A question that cannot be given an identity is not asked: an id
        // handed out twice is a stale keystroke that matches.
        let id = self.mint().ok_or_else(nobody_to_ask)?;
        let token = self.cancel.token();
        // Through `send_ui`, which makes the event inert at the channel: the
        // summary quotes a bounded excerpt of the file a call would change,
        // which is the likeliest place in the whole product for an escape
        // sequence to be sitting.
        match park_on(send_ui(
            &self.events,
            &token,
            UiEvent::Approval(ApprovalAsked {
                id,
                request: request.clone(),
            }),
        )) {
            Ok(()) => {}
            // The session is going down. A question nobody will see is not one
            // to wait for an answer to, and the answer xfx does not have is no.
            Err(Stopped::Cancelled) => return Ok(ApprovalAnswer::Deny),
            Err(Stopped::UiGone) => return Err(nobody_to_ask()),
        }
        park_on(async {
            loop {
                let message = tokio::select! {
                    biased;
                    // The session's root, not a turn's. The UI's control sender
                    // outlives a cancelled session, so `answered()` alone would
                    // never return -- the receiver never closes and nobody is
                    // left to answer. A decision xfx was not given is a refusal,
                    // which is the same answer this prompter gives when the send
                    // above is cancelled.
                    () = token.cancelled() => {
                        self.control.rearm();
                        return Ok(ApprovalAnswer::Deny);
                    }
                    message = self.control.answered() => message,
                };
                match message {
                    Some(TurnControl::Answer {
                        id: answered,
                        answer,
                    }) if answered == id => return Ok(answer),
                    // An answer to a question that has gone. It grants nothing
                    // and refuses nothing, and it is **consumed** rather than
                    // put back: handed to the loop it would be handed on to the
                    // next question as though it had been typed at that one,
                    // and an `Always` inherited that way is the rest of the
                    // session. This arm sits above the `stop` catch-all below
                    // for exactly that reason -- underneath it, a stale answer
                    // would fall into `stop`, be pushed back onto the channel,
                    // and refuse the live question.
                    Some(TurnControl::Answer { .. }) => continue,
                    // A keystroke left over from a question batch the model
                    // asked (`super::question`), which is a **different** panel
                    // with a different vocabulary: it grants nothing and refuses
                    // nothing. Consumed so the next batch does not inherit it,
                    // and then ignored -- reading it as an answer here would
                    // turn an answer to the model's question into a permission
                    // decision the user never made. The wait goes on, because
                    // the approval this call is about is still up.
                    Some(
                        TurnControl::QuestionAnswer { .. } | TurnControl::QuestionCancelled { .. },
                    ) => continue,
                    // A Ctrl-C or a shutdown. Both refuse this call, and both go
                    // back on the channel for the loop that can act on them --
                    // see [`ControlChannel::put_back`]. `give_back` rearms.
                    Some(stop) => {
                        self.control.give_back(stop);
                        return Ok(ApprovalAnswer::Deny);
                    }
                    // The UI dropped its sender. `answered` rearmed on the ready
                    // poll; rearming again is spurious and harmless, and keeps
                    // "every exit rearms" true without a reader having to check.
                    None => {
                        self.control.rearm();
                        return Err(nobody_to_ask());
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::approval_readiness::{Intent, Surface};
    use crate::permission::ApprovalDiff;
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

    /// A question **`crate::permission` really built**, for a session that is
    /// or is not being recorded.
    ///
    /// The fixture above is hand-written and short; this one is the sentence a
    /// user is actually shown, and the difference is the whole of why the
    /// panel's rows are measured from content. `PermissionSession::ask` appends
    /// one of two suffixes to every always-scope it builds (`policy.rs`'s
    /// `always_scope_for`) -- the resume-id of a saved session, or the note that
    /// this turn is not being recorded -- and neither is optional. A panel
    /// sized against the short form is a panel that cuts the long one.
    fn as_the_session_builds_it(durable: bool) -> ApprovalRequest {
        use crate::permission::{
            MutationKind, MutationPlan, PermissionMode, PermissionSession, Preimage,
            ProposedAction, TargetScope,
        };

        /// Keeps the question it was asked and answers no, so nothing is
        /// granted by the building of a fixture.
        struct Recording(Arc<Mutex<Vec<ApprovalRequest>>>);

        impl ApprovalPrompter for Recording {
            fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer> {
                self.0.guarded().push(request.clone());
                Ok(ApprovalAnswer::Deny)
            }
        }

        let plan = MutationPlan::new(
            MutationKind::Edit,
            // The long absolute path a real workspace has, which is what the
            // review plane shows as its subject.
            std::path::PathBuf::from(
                "/private/var/folders/f4/mj8750512wdb85799rpkct880000gn/T/.tmpqwGs5r/workspace/notes.txt",
            ),
            "notes.txt".to_string(),
            TargetScope::PrimaryWorkspace,
            Preimage::Absent,
            b"beta".to_vec(),
        );
        let asked = Arc::new(Mutex::new(Vec::new()));
        let session = PermissionSession::new(PermissionMode::Ask);
        let session = if durable {
            // The shape of a real saved-session id.
            session.with_durable_session("01K5Z8QF3V6TQ7B2N4H9J0XWRC")
        } else {
            session
        };
        let mut session = session.with_prompter(Box::new(Recording(Arc::clone(&asked))));
        session.decide(ProposedAction::Mutation(&plan));
        let mut asked = asked.guarded();
        assert_eq!(asked.len(), 1, "the session asked something else");
        asked.remove(0)
    }

    #[test]
    fn the_scope_a_real_session_asks_about_takes_three_rows_and_is_disclosed_whole() {
        // **The exact cause of the nine PTY failures and the three smoke
        // failures**, pinned deterministically. The suffix `always_scope_for`
        // appends is unconditional, so the sentence is three wrapped rows at an
        // ordinary 80-column screen in both session modes -- and a panel that
        // allotted it a fixed two would cut it, at which point no frame on any
        // 80x24 screen could disclose the request and every real approval would
        // be ungrantable.
        for durable in [true, false] {
            let asked = as_the_session_builds_it(durable);
            let scope = format!("2 = {}", asked.always_scope);
            let wanted = super::super::wrap::wrap(&scope, 80 - INDENT_CELLS).len();
            assert!(
                wanted >= 3,
                "durable={durable}: the real scope wants only {wanted} rows, so this case no \
                 longer reproduces what it was written for: {scope:?}"
            );

            // And an ordinary screen discloses it whole anyway, because the
            // panel's rows are measured from the content rather than read out
            // of a table.
            let panel = Panel::new(asked);
            let composed = panel.compose(80, 24, 1, 0);
            assert!(
                composed.disclosure.scope_whole,
                "durable={durable}: the always-scope was cut: {:?}",
                composed.rows
            );
            assert!(
                composed.disclosure.controls_whole && composed.disclosure.subject_whole,
                "durable={durable}: {:?}",
                composed.rows
            );
            assert!(
                Intent::capture(ApprovalId(1), Surface::Inline, 80, 24, composed.disclosure)
                    .is_some(),
                "durable={durable}: an ordinary screen could not disclose an ordinary request"
            );
        }
    }

    /// A prompter, the events it sends, the channel it waits on, and the
    /// sending half of that channel.
    ///
    /// The test keeps the sender because [`ControlChannel::new`] takes only the
    /// receiver and the type has no sender accessor -- and it is not given one:
    /// a production API existing solely for a test is exactly the wrong
    /// direction.
    fn a_prompter() -> (
        TuiPrompter,
        mpsc::Receiver<UiEvent>,
        Arc<ControlChannel>,
        mpsc::UnboundedSender<TurnControl>,
    ) {
        let (events, incoming) = mpsc::channel(8);
        let (answers, replies) = mpsc::unbounded_channel();
        let control = ControlChannel::new(replies);
        let prompter = TuiPrompter::new(
            events,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        (prompter, incoming, control, answers)
    }

    /// The id of the next question on the wire.
    ///
    /// **Receives** rather than peeks, so two calls read the first and the
    /// second question instead of the same one twice.
    fn asked_id(events: &mut mpsc::Receiver<UiEvent>) -> ApprovalId {
        match events.try_recv().expect("a question was sent") {
            UiEvent::Approval(asked) => asked.id,
            other => panic!("not a question: {other:?}"),
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
        // **Coordinates changed with the dynamic sizing.** These used to be
        // bounds against three fixed heights; the height is now the request's
        // own demand, so what is asserted is the demand's two properties. The
        // taller screen differs from the ordinary one by exactly the two blank
        // rows it can afford, and nothing else.
        let panel = Panel::new(request("terminal"));
        assert_eq!(
            panel.height(80, 40),
            panel.height(80, 24) + 2,
            "the tall screen bought something other than air"
        );
        // And no panel ever claims more rows than the screen has, however long
        // the request is: past that the cut is marked and the question is
        // refused rather than shown with half its disclosure.
        let mut vast = request("terminal");
        vast.summary = "x".repeat(9_000);
        vast.always_scope = "y".repeat(9_000);
        for rows in [6u16, 12, 24, 40] {
            assert!(
                Panel::new(vast.clone()).height(80, rows) <= rows,
                "a {rows}-row screen was given a taller panel than it has rows"
            );
        }
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
                usize::from(panel.caret_row(80, 24)),
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
        // **Coordinates changed with the dynamic sizing.** A summary is now
        // given the rows it asks for, so the cut happens where the *screen*
        // runs out rather than at a fixed two -- the panel takes the whole of a
        // twenty-four-row terminal and the ellipsis lands on its last summary
        // row. What is asserted is unchanged: the cut is marked.
        let rows = panel.rows(40, 24);
        assert_eq!(rows.len(), 24, "the panel did not take the rows it needed");
        let last_summary = rows
            .iter()
            .rposition(|row| row.starts_with(&format!("{INDENT}x")))
            .expect("the summary is on the panel");
        assert!(
            rows[last_summary].ends_with(ELLIPSIS),
            "the cut is silent: {:?}",
            &rows[..=last_summary]
        );
        // And a cut summary is a subject the user was never shown, so no frame
        // of this panel may be granted.
        assert!(!panel.compose(40, 24, 1, 0).disclosure.subject_whole);
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
        // Six: a title, the one row this summary needs, the three answers and
        // the one row this scope needs. Derived rather than looked up -- the
        // three fixed heights are gone, and six is what this request demands at
        // this width.
        assert_eq!(rows.len(), 6, "{rows:?}");
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
        // **The claim changed with the dynamic sizing, and it got stronger.**
        // It used to be that a taller screen showed *more* of the summary,
        // because an ordinary one was capped at two rows and cut the rest.
        // There is no cap any more: both screens say the whole summary, and the
        // only thing the taller one buys is the air. A test still asserting the
        // old inequality would be asserting that the short screen cuts.
        let compact = panel.rows(60, 24);
        let spacious = panel.rows(60, 40);
        let told = |rows: &[String]| {
            rows.iter()
                .filter(|row| {
                    row.contains("alpha") || row.contains("bravo") || row.contains("golf")
                })
                .count()
        };
        assert_eq!(
            told(&spacious),
            told(&compact),
            "one of the two screens cut the summary: {spacious:?} {compact:?}"
        );
        assert!(
            panel.compose(60, 24, 1, 0).disclosure.subject_whole,
            "an ordinary screen cut a summary it had room for: {compact:?}"
        );
        assert_eq!(
            spacious.len(),
            compact.len() + 2,
            "the taller screen bought something other than its two blank rows"
        );
        assert_eq!(
            spacious.iter().filter(|row| row.is_empty()).count(),
            2,
            "{spacious:?}"
        );
    }

    // -----------------------------------------------------------------------
    // what the composition really disclosed
    // -----------------------------------------------------------------------

    #[test]
    fn a_clipped_always_label_is_not_a_disclosed_control() {
        // The clip has to be a **no-op**, not merely "something survived beside
        // the marker": `2. Yes, and don't ask again for th` is exactly the
        // sentence with the difference between one call and the whole session
        // cut out of it.
        let panel = Panel::new(request("edit_file"));
        let wide = panel.compose(80, 24, 1, 0);
        assert!(wide.disclosure.controls_whole && wide.disclosure.scope_whole);
        // 30 cells cuts the second label mid-sentence.
        let narrow = panel.compose(30, 24, 1, 0);
        assert!(
            !narrow.disclosure.controls_whole,
            "a half-shown grant read as disclosed"
        );
    }

    #[test]
    fn an_ellipsised_summary_is_not_a_disclosed_subject() {
        // The band's summary carries the target (`crate::permission`'s
        // `ApprovalRequest`), so a summary cut at its allotment is a target the
        // user was never shown -- which is how a long path chosen to share a
        // visible prefix with a benign one would be approved.
        let mut asked = request("edit_file");
        asked.summary =
            "write_file wants to replace ".to_string() + &"deep/".repeat(60) + "notes.md";
        let panel = Panel::new(asked);
        // **A changed coordinate.** At 80x24 this summary is no longer cut --
        // the panel asks for the five rows it needs and gets them, which is the
        // whole point of the dynamic sizing. The cut is real where the screen
        // genuinely cannot carry it, and *there* is where the disclosure must
        // still be refused rather than assumed.
        assert!(
            panel.compose(80, 24, 1, 0).disclosure.subject_whole,
            "a screen with room for the target still hid it"
        );
        let short = panel.compose(80, 8, 1, 0);
        assert!(
            !short.disclosure.subject_whole,
            "a screen with no room for the target claimed to have shown it: {:?}",
            short.rows
        );
    }

    #[test]
    fn the_last_control_row_is_the_terminal_row_that_row_is_really_on() {
        // The band puts its activity row before the panel when the geometry has
        // one (`super::super::shell::Shell::band_rows`), so a panel-local index
        // is not a row of anybody's screen -- and the guard the disclosure feeds
        // (`Intent::capture`) is about the screen.
        let panel = Panel::new(request("edit_file"));
        let rows = panel.rows(80, 24);
        let last_choice = rows
            .iter()
            .rposition(|row| row.contains("3. No"))
            .expect("the refusal is a row of the panel");
        for (band_top, offset) in [(10u16, 0u16), (10, 1), (1, 0)] {
            assert_eq!(
                panel
                    .compose(80, 24, band_top, offset)
                    .disclosure
                    .last_control_row,
                band_top + offset + u16::try_from(last_choice).expect("a small index"),
                "{band_top}+{offset}"
            );
        }
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
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Once,
        })
        .expect("the channel is open");
        assert_eq!(
            woken.0.load(Ordering::Acquire),
            0,
            "the loop's waker was still registered, so this test proves nothing"
        );

        assert_eq!(
            asking.as_mut().poll(&mut Context::from_waker(&park)),
            Poll::Ready(Some(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once
            }))
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
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Always,
        })
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
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Once,
        })
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
            UiEvent::Approval(ApprovalAsked {
                id: ApprovalId(1),
                request: asked
            })
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
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Once,
        })
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
            !delivered.request.summary.contains('\u{1b}'),
            "an escape sequence reached the band: {:?}",
            delivered.request.summary
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

    #[test]
    fn a_question_answered_at_the_wrong_panel_is_consumed_and_the_approval_still_waits() {
        // The regression the model's own questions introduced. Both of
        // `TurnControl`'s question arms can be on this channel while an approval
        // is up -- a keystroke on a batch the turn has already given up on -- and
        // they are **not** decisions about permission. Reading one as an answer
        // here would deny a mutation the user never refused, and leaving it on
        // the channel would hand it to the next batch as though it had been
        // typed at that one.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::QuestionCancelled {
            id: super::super::question::QuestionId(1),
        })
        .expect("the channel is open");
        tx.send(TurnControl::QuestionAnswer {
            id: super::super::question::QuestionId(2),
            answers: vec!["a keystroke at a panel that has gone".to_string()],
        })
        .expect("the channel is open");
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Always,
        })
        .expect("the channel is open");
        let (events, _seen) = mpsc::channel(4);
        let mut prompter = TuiPrompter::new(
            events,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );

        assert_eq!(
            prompter.request(&request("edit_file")).expect("an answer"),
            ApprovalAnswer::Always,
            "a stale question answer was read as the user's permission decision"
        );
        assert_eq!(
            control.waiting(),
            None,
            "the stale question traffic was left for the next reader to inherit"
        );
    }

    #[test]
    fn an_interrupt_behind_stale_question_traffic_still_reaches_the_loop() {
        // The other half of the arm above: consuming a question message must not
        // consume the interrupt queued behind it, and the interrupt is still the
        // loop's rather than this prompter's.
        for stop in [TurnControl::Cancel { through: 4 }, TurnControl::Shutdown] {
            let (tx, rx) = mpsc::unbounded_channel();
            let control = ControlChannel::new(rx);
            tx.send(TurnControl::QuestionAnswer {
                id: super::super::question::QuestionId(9),
                answers: vec!["stale".to_string()],
            })
            .expect("the channel is open");
            tx.send(stop.clone()).expect("the channel is open");
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
            assert_eq!(control.waiting(), Some(stop));
        }
    }

    #[test]
    fn two_identical_questions_are_asked_under_different_ids_across_clones() {
        // The same sentence twice is two questions, and the counter is the
        // prompter's rather than each clone's: `crate::permission` builds one
        // prompter per session and clones it, so two clones minting from
        // separate counters would make the second question's id one the first
        // had already used.
        let (mut prompter, mut events, _control, answers) = a_prompter();
        let mut clone = prompter.clone();
        // The counter starts at 1, so the two calls take 1 and then 2.
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
            })
            .expect("the channel is open");
        prompter.request(&request("edit_file")).expect("answered");
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(2),
                answer: ApprovalAnswer::Once,
            })
            .expect("the channel is open");
        clone.request(&request("edit_file")).expect("answered");
        assert_ne!(
            asked_id(&mut events),
            asked_id(&mut events),
            "the same sentence twice is two questions"
        );
    }

    #[test]
    fn an_answer_carrying_another_requests_id_is_consumed_and_ignored() {
        // A keystroke left over at a question that has gone grants nothing.
        // **Consumed rather than put back**, because a stale `Answer` handed to
        // the loop would be handed on to the next question as though it had
        // been typed at that one -- and the one being answered here is an
        // `Always`, which is the whole rest of the session.
        let (mut prompter, mut events, control, answers) = a_prompter();
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(9_999),
                answer: ApprovalAnswer::Always,
            })
            .expect("the channel is open");
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
            })
            .expect("the channel is open");
        assert_eq!(
            prompter.request(&request("edit_file")).expect("answered"),
            ApprovalAnswer::Deny
        );
        assert!(
            control.waiting().is_none(),
            "the stale answer was consumed, not put back"
        );
        let _ = asked_id(&mut events);
    }

    #[test]
    fn a_session_cancelled_while_the_panel_is_up_refuses_rather_than_waiting_for_ever() {
        // The sender is deliberately alive, which is the whole hazard: the
        // session's root can be cancelled while the UI still holds its control
        // sender, so the receiver never closes and a wait on `answered()` alone
        // would never return. This test would hang rather than fail without the
        // `select!` -- so it is run on a thread and given a bound.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let (events, mut seen) = mpsc::channel(4);
        let cancel = Cancellation::new(crate::gateway::CancelToken::new());
        let mut prompter = TuiPrompter::new(events, Arc::clone(&control), cancel.clone());

        let finished = Arc::new(AtomicBool::new(false));
        let answered = std::thread::scope(|scope| {
            let done = Arc::clone(&finished);
            let asking = scope.spawn(move || {
                let answer = prompter.request(&request("edit_file"));
                done.store(true, Ordering::SeqCst);
                answer
            });
            // Cancelled only **after** the panel is really up, so what is
            // measured is a wait that ends rather than a send that never began.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                match seen.try_recv() {
                    Ok(UiEvent::Approval(_)) => break,
                    Ok(_) => continue,
                    Err(err) => assert!(
                        std::time::Instant::now() < deadline,
                        "the panel never reached the UI: {err:?}"
                    ),
                }
            }
            assert!(!tx.is_closed(), "the sender is deliberately still alive");
            cancel.cancel();
            // The bound. If the cancellation is not an exit from that wait, the
            // sender is dropped instead -- which *is* one -- so the case fails on
            // the assertion below rather than hanging the suite for ever.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while std::time::Instant::now() < deadline && !finished.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if !finished.load(Ordering::SeqCst) {
                drop(tx);
            }
            asking.join().expect("the asking thread")
        });

        assert_eq!(
            answered.expect("the wait never ended on the cancellation alone"),
            ApprovalAnswer::Deny,
            "a question xfx can no longer get an answer to is a no"
        );
    }
}
