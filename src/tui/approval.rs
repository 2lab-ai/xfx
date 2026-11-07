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
//! Three choices, and nothing else: yes once, yes and stop asking, no. The two
//! bail-out keys are upstream's (`vercel-labs/fx@c1db919`): **Ctrl-C refuses
//! this call and the turn goes on** (`src/ui/input/runtime.zig:61`), and
//! **Escape cancels the turn** the question belongs to
//! (`src/core/app/app_input_runtime.zig:3139-3143`). Both are refusals of the
//! call, because a decision xfx was never given is a refusal -- never an allow
//! -- and so is a session that is shutting down or a UI that has gone away.
//! What the question looks like is [`super::approval_view`]'s: upstream's rows,
//! on xfx's two surfaces.

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use tokio::sync::mpsc::{Sender, UnboundedReceiver};

use super::approval_amendment::Drafts;
use super::approval_readiness::{ApprovalId, Disclosure};
use super::approval_view::{
    blank, choice_rows, draft_cols, draft_column, scope_rows, subject_rows, Kind, Review, Styles,
};
use super::bridge::{park_on, send_ui, Cancellation, Stopped, TurnControl, UiEvent};
use super::theme::Palette;
use crate::permission::{ApprovalAnswer, ApprovalPrompter, ApprovalRequest, ApprovalResponse};

// ---------------------------------------------------------------------------
// what the panel says
// ---------------------------------------------------------------------------

/// The screen height at which the panel can afford its blank rows.
///
/// Upstream's own threshold for the spacious panel
/// (`interaction_state.zig:15`, `approval_spacious_min_terminal_rows`): below
/// it the question drops the blank rows between its parts and keeps every row
/// that says something.
pub(crate) const SPACIOUS_AT: u16 = 34;

/// What the panel calls itself: the first words of its header, on both
/// surfaces (`super::approval_view`'s two titles both begin with it).
#[cfg(test)]
pub(crate) const TITLE: &str = "Permission needed";

/// How many rows of review a change may have and still be asked in the band.
///
/// A property of the **change**, never of the screen
/// ([`ApprovalSurface::for_request`]): half of an ordinary 24-row screen, which
/// is a change the band shows whole above its chrome. A longer one is reviewed
/// on a screen of its own, the surface upstream reviews every file on
/// (`approval_screen.zig:208-223`, `needsScreen`).
pub(crate) const BAND_REVIEW_LINES: usize = 12;

/// The band's rows that are not the panel, with the one document row the band
/// always leaves: the activity row and the blank under it, the two rules and
/// the hint row (`super::layout::solve_band`).
///
/// What a draft's later rows are paid for out of. The panel does not see the
/// band, so it is told what the band costs rather than asking it; an
/// overestimate costs a draft a row, and an underestimate would cost the
/// question one -- which is why the draft is the half that is cut.
const BAND_CHROME_ROWS: usize = 6;

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

/// How many answers the panel offers.
///
/// Exported so that [`super::approval_amendment`] can walk the same three
/// without a second array: which choices exist is one fact.
pub(crate) const CHOICE_COUNT: usize = CHOICES.len();

/// Which answer a choice index stands for, or nothing when there is no such
/// choice.
///
/// Total rather than indexed, because the callers are a keystroke away from a
/// UI thread holding a raw terminal and an out-of-range index is a bug to
/// report rather than a panic to take the session down with.
pub(crate) fn choice(index: usize) -> Option<ApprovalAnswer> {
    CHOICES.get(index).copied()
}

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
    /// Cancel the turn the question belongs to.
    ///
    /// Upstream's Escape at a question (`app_input_runtime.zig:3139-3143`,
    /// `cancelApprovalOperation`): this call is refused **and** the turn
    /// stops, which the shell carries out as its interrupt.
    Escape,
    /// Refuse this call and nothing else: the turn goes on, told no.
    ///
    /// Upstream's Ctrl-C at a question (`src/ui/input/runtime.zig:61`,
    /// `3 => .deny`), pinned by its e2e
    /// `tests/e2e/tui-permissions.test.ts:1099`.
    Cancel,
    /// An editing key, meaningful only while a draft is open.
    ///
    /// The shell translates only the subset a draft really answers, and only
    /// while one has the keys ([`super::shell::Shell::decide`]): `Up` and
    /// `Down` are deliberately **not** in it, because they move the choice and
    /// end editing, and routing them into the editor would take away the user's
    /// only way out of a draft.
    Edit(super::input::Action),
    /// One raw byte of a bracketed paste, while a draft is open.
    ///
    /// Separate because the composer's byte route feeds `super::paste::Paste`
    /// and would collapse a large paste into an entity summary; a draft takes
    /// its bytes here ([`super::approval_amendment::Drafts::paste_byte`]).
    PasteByte(u8),
}

/// What one keystroke did to whichever surface has the focus.
///
/// Four outcomes rather than `Option<ApprovalAnswer>`, because the draft made
/// "nothing was answered" three different facts and the shell owes a different
/// thing to each: a frame, a frame **and** a revoked readiness receipt, or
/// nothing at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    /// The keystroke meant nothing here and was swallowed.
    Ignored,
    /// Something moved and the surface owes a frame, at the same row count.
    Moved,
    /// A draft opened or closed, or the rows it takes changed, so the screen a
    /// readiness receipt was about is no longer the screen the user is looking
    /// at ([`super::approval_readiness::Readiness::invalidate`]).
    Reshaped,
    /// The user answered.
    Answer {
        answer: ApprovalAnswer,
        /// Whether this key carries the draft belonging to its answer.
        ///
        /// `false` for Escape and for Ctrl-C, and the difference is
        /// deliberate: those are the bail-outs -- a turn cancelled, a call
        /// refused with a keystroke that says nothing -- so they refuse with
        /// nothing said even when the deny draft holds text
        /// (`approval_decision.zig:132-135` discards the amendment on `deny`).
        /// Submitting `3` is a *chosen* refusal and does send it.
        amendable: bool,
    },
}

/// Where a question is put in front of the user.
///
/// A property of the **change**, not of the terminal: [`Self::for_request`] is
/// given the question and nothing else -- no rows, no columns -- so that which
/// surface a change belongs on cannot start depending on how big somebody's
/// window happens to be. Whether a screen is too short to *ask* on is a
/// separate and later question, asked **once the surface is known**, of that
/// surface: the band's panel by [`super::layout::fits_approval`], the review
/// plane by [`super::approval_screen::ApprovalScreen::presents_choices`]
/// (`super::shell::Shell::ask` moves a change the band cannot hold onto the
/// plane rather than refusing it).
///
/// Two variants and no payload: what is settled here is the choice and the
/// state that records it ([`super::shell::ScreenOwner`]); the plane's renderer,
/// its lifecycle and the `1049` bytes that enter and leave it are
/// [`super::approval_screen`]'s and [`super::frame`]'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApprovalSurface {
    /// The band's own panel, with the document still visible above it.
    Inline,
    /// A screen of its own, for a change the band's review cannot show.
    Alternate,
}

impl ApprovalSurface {
    /// Which surface this question belongs on.
    pub(crate) fn for_request(request: &ApprovalRequest) -> Self {
        if request.diff.is_none() {
            // A command, a directory: nothing to review but the subject, which
            // the band shows whole.
            return Self::Inline;
        }
        if Review::of(request).lines.len() > BAND_REVIEW_LINES {
            Self::Alternate
        } else {
            Self::Inline
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

/// The question, which answer is marked, and what the user is saying about it.
///
/// A manual `Debug`, because [`Drafts`] holds editors and the band's slot is a
/// `Debug` enum (`super::shell::Slot`). The panel is built once per question
/// and dies with the answer.
pub(crate) struct Panel {
    request: ApprovalRequest,
    /// Which of upstream's layouts the question takes.
    kind: Kind,
    /// The review of the change, built once: it is a pure function of the
    /// request, and a 64 KiB side diffed again for every frame would be the
    /// band's whole cost.
    review: Option<Review>,
    /// An index into [`CHOICES`].
    selected: usize,
    /// The two amendments this question may carry
    /// ([`super::approval_amendment`]).
    drafts: Drafts,
}

impl std::fmt::Debug for Panel {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Panel")
            .field("request", &self.request)
            .field("selected", &self.selected)
            .field("drafts", &self.drafts)
            .finish()
    }
}

/// One construction of the panel: its rows, and every fact the shell reads off
/// them.
///
/// The height, the caret, the disclosure and the paint are four readings of one
/// layout, and four constructions would be four chances to disagree.
struct Built {
    rows: Vec<String>,
    /// The index of the last choice row -- never a draft's later row.
    last_control: usize,
    /// All three choices' words reached their rows uncut.
    controls_whole: bool,
    /// The index of the marked choice's row, which is where the caret goes
    /// while no draft has the keys; read on its own only by a test.
    #[cfg_attr(not(test), allow(dead_code))]
    marked: usize,
    /// Where the caret goes: a row index, and the cells to its left.
    caret: (usize, u16),
}

impl Panel {
    pub(crate) fn new(request: ApprovalRequest) -> Self {
        let kind = Kind::of(&request);
        let review = kind.reviews_a_file().then(|| Review::of(&request));
        Self {
            request,
            kind,
            review,
            // Yes-once, which is the safest of the three that is still an
            // answer: an Enter pressed without reading grants one call rather
            // than the rest of the session.
            selected: 0,
            drafts: Drafts::default(),
        }
    }

    /// Whether an amendment has the keys.
    /// The request this question is about.
    pub(crate) fn request(&self) -> &ApprovalRequest {
        &self.request
    }

    pub(crate) fn drafting(&self) -> bool {
        self.drafts.editing()
    }

    /// What the panel owes the user about their last keystroke, taken once.
    pub(crate) fn take_notice(&mut self) -> Option<&'static str> {
        self.drafts.take_notice()
    }

    /// Drops a paste refused for the input behind its end marker, and says
    /// whether it had already outgrown the budget
    /// ([`super::approval_amendment::Drafts::refuse_paste`]).
    pub(crate) fn refuse_paste(&mut self) -> bool {
        self.drafts.refuse_paste()
    }

    /// The amendment this answer carries, and the disposal of the other draft.
    ///
    /// Called **after** the readiness gate, and that ordering is the whole
    /// point: a refused affirmative must leave the drafts exactly as the user
    /// typed them, because the question is still up and the sentence is still
    /// theirs.
    pub(crate) fn take_feedback(
        &mut self,
        answer: ApprovalAnswer,
        amendable: bool,
    ) -> Option<String> {
        if !amendable {
            self.drafts.discard();
            return None;
        }
        self.drafts.submit(answer)
    }

    /// The panel's rows as text, top first, exactly as many as
    /// [`Self::height`] says.
    ///
    /// **One construction for both** (`approval_ui.zig:268-294`): the height
    /// is the length of this, so the count the band solved its geometry from
    /// cannot drift from the paint and leave a stale row standing in the band.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        self.build(cols, terminal_rows, &Styles::PLAIN).rows
    }

    /// The same rows in the palette's colours, for the band to paint.
    ///
    /// The same construction as [`Self::rows`] with sequences in front of each
    /// run, and a sequence costs no cells: the two cannot be different heights
    /// or cut in different places.
    pub(crate) fn painted(&self, cols: u16, terminal_rows: u16, palette: &Palette) -> Vec<String> {
        self.build(cols, terminal_rows, &Styles::of(palette)).rows
    }

    /// The row under the band's bottom rule while this question is up: what
    /// the keys do here (`approval_ui.zig:20-25`).
    pub(crate) fn hint(&self, cols: u16, palette: &Palette) -> String {
        super::approval_view::hint(
            self.kind,
            amendable(self.selected),
            false,
            cols,
            &Styles::of(palette),
        )
    }

    /// The panel's rows and what they really disclosed, for a panel whose first
    /// row is at terminal row `first_row`.
    ///
    /// The placing argument is readiness's alone. The band puts rows of its own
    /// before the panel -- the rule that heads it, and the blank row above a
    /// panel over a document that does not end on one
    /// ([`super::shell::Shell::band_rows`]) -- so a panel-local index is not a
    /// row of anybody's terminal, and `Intent::capture`'s `last_control_row <=
    /// rows` guard is about the terminal. The caller hands the row the geometry
    /// paints the panel from ([`super::layout::Geometry::panel_first`]) rather
    /// than counting what is above it.
    ///
    /// The subject, the scope and the change are whole **by construction**:
    /// every one of them is wrapped onto as many rows as it needs, never cut,
    /// and a panel taller than the band can hold is not installed at all
    /// ([`super::layout::fits_approval`]). What a narrow screen can still cut is
    /// a choice's words, and that is measured.
    pub(crate) fn compose(&self, cols: u16, terminal_rows: u16, first_row: u16) -> Composition {
        let built = self.build(cols, terminal_rows, &Styles::PLAIN);
        let disclosure = Disclosure {
            // The panel's first row is `first_row`, so a local index needs no
            // further one-based correction: it is already a terminal row as the
            // terminal counts them ([`super::layout::Geometry::panel_first`]).
            last_control_row: first_row
                .saturating_add(u16::try_from(built.last_control).unwrap_or(u16::MAX)),
            controls_whole: built.controls_whole,
            scope_whole: true,
            subject_whole: true,
            change_visible: true,
        };
        Composition {
            rows: built.rows,
            disclosure,
        }
    }

    /// How many rows the band has to give the panel.
    ///
    /// **Content-measured**, so it really is a question of the request as well
    /// as of the screen. What the band does when the answer is more rows than
    /// it has is [`super::layout::fits_approval`]'s.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        u16::try_from(self.rows(cols, terminal_rows).len()).unwrap_or(u16::MAX)
    }

    /// Which row of the panel the marked choice is on.
    ///
    /// `#[cfg(test)]`: production places the caret with [`Self::caret`], which
    /// answers both of the places it can be, and a second reader of the same
    /// layout is the drift this module keeps saying it will not have.
    #[cfg(test)]
    pub(crate) fn caret_row(&self, cols: u16, terminal_rows: u16) -> u16 {
        u16::try_from(self.build(cols, terminal_rows, &Styles::PLAIN).marked).unwrap_or(u16::MAX)
    }

    /// Where the caret really goes: a row of the panel, and the cells to the
    /// left of it on that row.
    ///
    /// The marked choice while no amendment is open, and **inside the draft**
    /// while one is: the caret is what the terminal says the next keystroke
    /// goes to, and a caret left on the choice's first cell while a draft has
    /// the keys would be a lie about which of the two a character lands in.
    pub(crate) fn caret(&self, cols: u16, terminal_rows: u16) -> (u16, u16) {
        let (row, column) = self.build(cols, terminal_rows, &Styles::PLAIN).caret;
        (u16::try_from(row).unwrap_or(u16::MAX), column)
    }

    /// The one construction everything above reads.
    ///
    /// Upstream's two layouts, in xfx's band. A file is its review above the
    /// chrome -- the change, a dotted rule, `Permission needed · Review change`
    /// with the counts on the right, the path and the question, the choices
    /// (`fx-approvals-40x120-edit-ask`, `approval_screen.zig:183-206`); a
    /// command is the header, the question and the command under `$`, then the
    /// choices (`fx-approvals-40x120-cmd-ask`, `approval_ui.zig:1513-1541`).
    /// Under the choices, dim, what `2` would buy ([`scope_rows`]). The blank
    /// rows between the parts are spent only on a screen tall enough for them
    /// ([`SPACIOUS_AT`]).
    fn build(&self, cols: u16, terminal_rows: u16, styles: &Styles) -> Built {
        let air = terminal_rows >= SPACIOUS_AT;
        let mut rows = Vec::new();
        if let Some(review) = self.review.as_ref() {
            rows.extend(review.rows(cols, styles).into_iter().map(|(row, _)| row));
            rows.push(super::approval_view::dotted_rule(cols, styles));
        }
        rows.push(super::approval_view::header(
            self.kind,
            self.review.as_ref(),
            cols,
            styles,
        ));
        if air {
            rows.push(blank());
        }
        rows.extend(subject_rows(self.kind, &self.request, cols, styles));
        if air {
            rows.push(blank());
        }
        let scope = scope_rows(&self.request, cols, styles);
        // **The drafts are paid for last, out of what is left.** They are the
        // one block on this panel that is not the question, so a screen too
        // short for everything shows less of the amendment -- never less of the
        // subject, the controls or the scope, all three of which a readiness
        // receipt is about. A draft's first row costs nothing: it is its
        // choice's row.
        let fixed = rows.len() + CHOICE_COUNT + scope.len() + usize::from(air);
        let mut budget = usize::from(terminal_rows).saturating_sub(fixed + BAND_CHROME_ROWS);
        let blocks = self
            .drafts
            .blocks(|index| draft_cols(self.kind, index, cols));
        let mut last_control = 0;
        let mut controls_whole = true;
        let mut marked = 0;
        let mut caret = (0, 0);
        for index in 0..CHOICE_COUNT {
            let block = blocks.iter().find(|block| block.choice == index);
            let later = block
                .map_or(0, |block| block.rows.len().saturating_sub(1))
                .min(budget);
            budget -= later;
            let (choice_rows, whole) = choice_rows(
                self.kind,
                index,
                index == self.selected,
                block,
                later,
                cols,
                styles,
            );
            controls_whole &= whole;
            last_control = rows.len();
            if index == self.selected {
                marked = rows.len();
                caret = (rows.len(), 0);
            }
            if let Some((within, column)) = block.and_then(|block| block.caret) {
                caret = (
                    rows.len() + within.min(choice_rows.len() - 1),
                    // On the screen, always: a column past the last one is a
                    // cursor the terminal clamps silently onto a row it was
                    // not placed on.
                    draft_column(self.kind, index)
                        .saturating_add(column)
                        .min(cols.saturating_sub(1)),
                );
            }
            rows.extend(choice_rows);
        }
        rows.extend(scope);
        if air {
            rows.push(blank());
        }
        Built {
            rows,
            last_control,
            controls_whole,
            marked,
            caret,
        }
    }

    /// What one keystroke does, on a panel `cols` cells wide.
    ///
    /// **Takes `cols`** because a draft is text at a width: the keystroke that
    /// inserts a character is the one that decides whether the draft now needs
    /// another row, and the answer is the draft's own width on its choice's row
    /// ([`draft_cols`]) rather than the screen's.
    pub(crate) fn apply(&mut self, action: Action, cols: u16) -> Reply {
        let kind = self.kind;
        answered(action, &mut self.selected, &mut self.drafts, &|index| {
            draft_cols(kind, index, cols)
        })
    }
}

/// Whether the marked choice carries a draft -- what makes Tab amend rather
/// than move, and what the hint row says about it.
pub(crate) fn amendable(selected: usize) -> bool {
    choice(selected).is_some_and(|answer| answer != ApprovalAnswer::Always)
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
/// `width` is how wide each choice's draft is ([`draft_cols`]): the draft sits
/// on its choice's row, after words of a different length on each.
///
/// The digits answer **without** moving the marker, and that is deliberate
/// rather than incidental: a `3` is a refusal, not a refusal plus a marker left
/// on the refusal for whatever key comes next.
pub(crate) fn answered(
    action: Action,
    selected: &mut usize,
    drafts: &mut Drafts,
    width: &dyn Fn(usize) -> u16,
) -> Reply {
    // Measured **before** and after, at the width the rows are painted at, so
    // "the panel changed shape" is read off the composition rather than guessed
    // at from which key it was: a character that pushed a draft onto a second
    // row costs the band a row exactly as opening the draft did.
    let before = shape(drafts, width);
    let reply = keyed(action, selected, drafts, width);
    match reply {
        Reply::Moved if shape(drafts, width) != before => Reply::Reshaped,
        other => other,
    }
}

/// Which drafts are showing and how many rows they are asking for.
fn shape(drafts: &Drafts, width: &dyn Fn(usize) -> u16) -> (usize, usize) {
    let blocks = drafts.blocks(width);
    (
        blocks.len(),
        blocks.iter().map(|block| block.rows.len()).sum(),
    )
}

/// What one keystroke means, before the reshape is measured.
fn keyed(
    action: Action,
    selected: &mut usize,
    drafts: &mut Drafts,
    width: &dyn Fn(usize) -> u16,
) -> Reply {
    let answer = |answer| Reply::Answer {
        answer,
        amendable: true,
    };
    match action {
        // The digits, and they answer **without** moving the marker: a `3` is a
        // refusal, not a refusal plus a marker left on it for whatever key comes
        // next. While a draft has the keys they are characters instead, which is
        // the numeric-draft ruling (`approval_decision.zig::apply`): a user
        // typing "3 files is too many" is typing, not answering.
        Action::Text(digit @ ('1' | '2' | '3')) if !drafts.editing() => {
            let index = usize::from(digit as u8 - b'1');
            answer(CHOICES[index])
        }
        // Every other character. A surface that has the focus swallows them
        // rather than letting them fall into a composer the user cannot see the
        // caret in -- unless a draft is open, which is what a draft *is*.
        Action::Text(character) => {
            if !drafts.editing() {
                return Reply::Ignored;
            }
            drafts.typed(character);
            Reply::Moved
        }
        // **Arrow navigation is what ends editing** (`approval_decision.zig:214-225`:
        // `moveChoice` updates `selected` and then calls `clearActive`), and it
        // keeps both buffers: a user stepping from the allow side to the deny
        // side to say why has not thrown away what they typed at either.
        Action::Up => {
            drafts.leave();
            *selected = (*selected + CHOICES.len() - 1) % CHOICES.len();
            Reply::Moved
        }
        Action::Down => {
            drafts.leave();
            *selected = (*selected + 1) % CHOICES.len();
            Reply::Moved
        }
        // Tab **enters** a draft; it never leaves one. A second meaning for the
        // key that opened the editor would be the only way out being the key
        // that got in, which is a surface the user has to guess at.
        Action::Tab if drafts.editing() => Reply::Ignored,
        Action::Tab => {
            if drafts.enter(*selected).is_none() {
                // Not draft-eligible -- choice 1, `Always` -- so Tab is the
                // cycle it has always been.
                *selected = (*selected + 1) % CHOICES.len();
            }
            Reply::Moved
        }
        Action::Submit => answer(CHOICES[*selected]),
        // Both are refusals of this call, and neither carries what the user
        // typed: Escape is the turn being cancelled and Ctrl-C a refusal by a
        // key that says nothing (`approval_decision.zig:132-135` discards the
        // amendment on `deny`). What *else* each one does -- Escape stops the
        // turn, Ctrl-C lets it go on -- is the shell's, which owns the turn.
        Action::Escape | Action::Cancel => Reply::Answer {
            answer: ApprovalAnswer::Deny,
            amendable: false,
        },
        // The editing subset, which the shell translates only while a draft has
        // the keys ([`super::shell::Shell::decide`]).
        Action::Edit(editing) => {
            let Some(open) = drafts.active() else {
                return Reply::Ignored;
            };
            drafts.edit(editing, width(open));
            Reply::Moved
        }
        Action::PasteByte(byte) => {
            if !drafts.editing() {
                return Reply::Ignored;
            }
            drafts.paste_byte(byte);
            // Nothing is painted per byte: a frame per byte of a paste is a
            // session that stops answering the keyboard, and the text does not
            // reach the draft until `PasteEnd` decides whether it may.
            Reply::Ignored
        }
    }
}

/// Whether a decoded action is one a draft answers.
///
/// The whitelist, **named exhaustively** rather than written as "everything
/// that is not a choice key": an action added to `super::input::Action` later
/// has to be routed on purpose, and the two that must never be here are `Up`
/// and `Down` -- they move the choice and end editing, so an editor that
/// swallowed them would leave a user inside a draft with no way out but a
/// refusal.
///
/// `Submit`, `Escape`, `Cancel` and `Tab` are absent for the opposite reason:
/// they are the panel's own keys and mean the same thing whether or not a draft
/// is open, so a draft that took them would change what answering a question
/// means. `InsertNewline`, `Eof`, `ControlP`, `HistoryNext`, `Redraw`
/// and `Ignore` are the session's or the panel's and are not a draft's at all.
pub(crate) fn edits(action: super::input::Action) -> bool {
    use super::input::Action as Key;
    matches!(
        action,
        Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::WordLeft
            | Key::WordRight
            | Key::Backspace
            | Key::Delete
            | Key::DeleteWordLeft
            | Key::DeleteWordBackward
            | Key::DeleteWordRight
            | Key::KillToEnd
            | Key::ControlK
            | Key::KillToStart
            | Key::Undo
            | Key::Redo
            | Key::Yank
            | Key::PasteStart
            | Key::PasteEnd
    )
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
    /// A cancel that lands while the panel is up -- Escape at the panel, or
    /// an interrupt from anywhere -- answers the panel with a refusal, because
    /// that is what a decision xfx was not given is, and then goes on being a
    /// cancel: the turn it was pressed at still has to stop, and only the loop
    /// can stop one. Consuming it without this queue would make a cancel typed
    /// at a panel a refusal *instead of* a cancel.
    put_back: Mutex<VecDeque<TurnControl>>,
    /// Model and speed changes and undos read while work was running,
    /// waiting for it to end ([`Self::defer`], [`Self::defer_speed`],
    /// [`Self::defer_undo`]), in the order they were read.
    deferred: Mutex<Vec<Deferred>>,
    /// What answers [`TurnControl::Catalog`] for the work that is running,
    /// lent for as long as it runs ([`Self::lend_catalog`]), so a prompter
    /// parked on a question answers it too.
    catalog: Mutex<Option<Arc<dyn CatalogAsker>>>,
}

/// What answers `/model`'s ask for the catalog ([`TurnControl::Catalog`])
/// for a piece of work that holds the runtime -- read by the work's own race,
/// or by a prompter a question has parked the thread in.
///
/// **Here so the question does not hold the read**, as upstream's does not:
/// its catalog load runs on a thread of its own, whatever its worker is
/// waiting for (`ensureModelCache` from `openModelBrowseCatalog`,
/// `vercel-labs/fx@c1db9191 src/core/app/app_input_runtime.zig:2346-2349`;
/// `startWarmup`, `src/core/app/model_cache_runtime.zig:321-349`). The read
/// it starts runs on a thread of its own as well
/// (`crate::provider::model::CatalogReader`), and tells the band from there.
pub(crate) trait CatalogAsker: Send + Sync {
    /// Starts the read, or says what the band is told now instead -- the
    /// catalog as it is held, or a failure asked about again too soon
    /// (`crate::provider::model::RETRY_FLOOR`). `None` when the read is
    /// started, or one is on its way that will tell its own answer.
    fn ask_catalog(&self, control: &ControlChannel) -> Option<UiEvent>;
}

/// A [`CatalogAsker`] lent to a [`ControlChannel`]: given back -- whatever
/// was lent before it put back -- when this is dropped, however the work it
/// was lent for ends.
pub(crate) struct CatalogLent<'a> {
    control: &'a ControlChannel,
    before: Option<Arc<dyn CatalogAsker>>,
}

impl Drop for CatalogLent<'_> {
    fn drop(&mut self) {
        *self.control.catalog.guarded() = self.before.take();
    }
}

/// One message put aside while work ran, for the turn loop once it is over.
///
/// One queue for all of them rather than one each, so they are done in the
/// order the person typed them: a `/model`, a `/fast` and an `/undo` are
/// answered in that order -- the speed after the model change it may follow,
/// which turns the switches off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Deferred {
    /// [`TurnControl::Model`].
    Model(super::bridge::ModelChange),
    /// [`TurnControl::Speed`].
    Speed(super::bridge::SpeedChange),
    /// [`TurnControl::Undo`].
    ///
    /// `through` is the position the keystroke saw, carried as it came.
    /// `after_reset` when the loop had already read a `/new`, `/clear` or
    /// `/reset` aimed at the same work: that reset drops the conversation
    /// once the work is over, and an undo typed after it is about the
    /// conversation it leaves -- which has changed nothing yet -- rather than
    /// the one it drops.
    Undo {
        through: crate::tools::undo::Mark,
        after_reset: bool,
    },
    /// [`TurnControl::Rename`], `after_reset` as [`Self::Undo`]'s: a title
    /// typed after a reset the loop had already read is the session that
    /// reset starts, so it is written once the drop is done. `announced` as
    /// the shell sent it.
    Rename {
        title: String,
        after_reset: bool,
        announced: bool,
    },
    /// [`TurnControl::Catalog`] read while work that could not start a read
    /// ran -- a provider switch, whose catalog is not yet the one to read --
    /// answered as soon as it is over.
    Catalog,
}

impl ControlChannel {
    pub(crate) fn new(rx: UnboundedReceiver<TurnControl>) -> Arc<Self> {
        Arc::new(Self {
            rx: Mutex::new(rx),
            loop_waker: Mutex::new(None),
            put_back: Mutex::new(VecDeque::new()),
            deferred: Mutex::new(Vec::new()),
            catalog: Mutex::new(None),
        })
    }

    /// Lends `asker` for the work that is about to run: a prompter parked on
    /// a question while it runs answers [`TurnControl::Catalog`] through it
    /// ([`Self::ask_catalog`]) instead of handing it back for when the
    /// question lets the thread go.
    pub(crate) fn lend_catalog(&self, asker: Arc<dyn CatalogAsker>) -> CatalogLent<'_> {
        let before = self.catalog.guarded().replace(asker);
        CatalogLent {
            control: self,
            before,
        }
    }

    /// [`TurnControl::Catalog`], read by a prompter parked on a question:
    /// answered through what the running work lent ([`Self::lend_catalog`]),
    /// and handed back to the loop when nothing was lent. **A catalog request
    /// answers no question**: the question stays up. What the band is to be
    /// told now, if anything.
    pub(crate) fn ask_catalog(&self) -> Option<UiEvent> {
        // Cloned out, so the lock is not held while it is asked.
        let asker = self.catalog.guarded().clone();
        match asker {
            Some(asker) => asker.ask_catalog(self),
            None => {
                self.give_back(TurnControl::Catalog);
                None
            }
        }
    }

    /// Puts a model change aside until the work that is running ends
    /// ([`TurnControl::Model`]).
    ///
    /// Not [`Self::give_back`]: a message put back is the loop's to act on
    /// *now*, and a model change is not -- the turn that is running keeps the
    /// model it began with, and the change belongs to the one after it. Called
    /// by whichever reader is holding the channel when the change arrives: the
    /// turn loop's race, or a prompter parked on a question, which goes on
    /// waiting for its answer -- **a model change answers no question**, and a
    /// prompter that treated it as the stop it is not would refuse a call the
    /// user never refused.
    pub(crate) fn defer(&self, change: super::bridge::ModelChange) {
        self.deferred.guarded().push(Deferred::Model(change));
    }

    /// Puts a speed change aside until the work that is running ends
    /// ([`TurnControl::Speed`]), for [`Self::defer`]'s reasons: the turn that
    /// is running keeps the speed it began with, and **a speed change answers
    /// no question**.
    pub(crate) fn defer_speed(&self, change: super::bridge::SpeedChange) {
        self.deferred.guarded().push(Deferred::Speed(change));
    }

    /// Puts an `/undo` aside until the work that is running ends
    /// ([`TurnControl::Undo`]).
    ///
    /// Not [`Self::give_back`], for the reason [`Self::defer`] is not: the
    /// loop must not act on it while the work runs, and **an undo answers no
    /// question** -- a prompter that took it for a stop would refuse a call
    /// the user never refused.
    pub(crate) fn defer_undo(&self, through: crate::tools::undo::Mark, after_reset: bool) {
        self.deferred.guarded().push(Deferred::Undo {
            through,
            after_reset,
        });
    }

    /// Puts a `/rename` aside until the work that is running ends
    /// ([`TurnControl::Rename`]): the conversation's log is that work's to
    /// write until then, and **a rename answers no question** either.
    pub(crate) fn defer_rename(&self, title: String, after_reset: bool, announced: bool) {
        self.deferred.guarded().push(Deferred::Rename {
            title,
            after_reset,
            announced,
        });
    }

    /// Puts a catalog request aside until the work that is running ends
    /// ([`Deferred::Catalog`]). **A catalog request answers no question**
    /// either.
    pub(crate) fn defer_catalog(&self) {
        self.deferred.guarded().push(Deferred::Catalog);
    }

    /// Whether a `/rename` of the conversation the work is in is put aside
    /// for when the work ends: one typed before a reset the loop had read is
    /// the next conversation's, and does not count.
    pub(crate) fn renaming(&self) -> bool {
        self.deferred.guarded().iter().any(|deferred| {
            matches!(
                deferred,
                Deferred::Rename {
                    after_reset: false,
                    ..
                }
            )
        })
    }

    /// Everything put aside, oldest first, for the turn loop to do now that
    /// the work it waited for is over.
    pub(crate) fn take_deferred(&self) -> Vec<Deferred> {
        std::mem::take(&mut *self.deferred.guarded())
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

    /// Whether a parked reader has handed back a stop the loop has not read
    /// yet -- so the refusal it answered with was the stop's, not the user's.
    pub(crate) fn stopping(&self) -> bool {
        self.put_back.guarded().iter().any(|message| {
            matches!(
                message,
                TurnControl::Cancel { .. } | TurnControl::Reset { .. } | TurnControl::Shutdown
            )
        })
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
        // fetch_update is renamed try_update in Rust 1.99; kept for the 1.96 MSRV.
        #[allow(deprecated)]
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
    /// The answer alone, for a caller that has nothing to do with a sentence.
    ///
    /// **A projection of [`Self::respond`] rather than a second body**: two
    /// implementations of the same wait would be two chances to disagree about
    /// which ids are stale and which messages are put back, and only one of
    /// them would be the one production takes.
    fn request(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalAnswer> {
        self.respond(request).map(|response| response.answer)
    }

    fn respond(&mut self, request: &ApprovalRequest) -> io::Result<ApprovalResponse> {
        // A question that cannot be given an identity is not asked: an id
        // handed out twice is a stale keystroke that matches.
        let id = self.mint().ok_or_else(nobody_to_ask)?;
        let token = self.cancel.token();
        // Through `send_ui`, which makes the event inert at the channel: the
        // request quotes the file a call would change, which is the likeliest
        // place in the whole product for an escape sequence to be sitting.
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
            // to wait for an answer to, and the answer xfx does not have is no
            // -- said with nothing beside it, because nobody was asked.
            Err(Stopped::Cancelled) => return Ok(ApprovalResponse::plain(ApprovalAnswer::Deny)),
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
                        return Ok(ApprovalResponse::plain(ApprovalAnswer::Deny));
                    }
                    message = self.control.answered() => message,
                };
                match message {
                    Some(TurnControl::Answer {
                        id: answered,
                        answer,
                        feedback,
                    }) if answered == id => return Ok(ApprovalResponse { answer, feedback }),
                    // An answer to a question that has gone. It grants nothing
                    // and refuses nothing, and it is **consumed** rather than
                    // put back: handed to the loop it would be handed on to the
                    // next question as though it had been typed at that one,
                    // and an `Always` inherited that way is the rest of the
                    // session. This arm sits above the `stop` catch-all below
                    // for exactly that reason -- underneath it, a stale answer
                    // would fall into `stop`, be pushed back onto the channel,
                    // and refuse the live question. **Its feedback goes with
                    // it**: a sentence typed at a question that has gone is not
                    // context about this one, and inheriting it would put words
                    // in the user's mouth about a call they never saw.
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
                    // `/model` typed while this question is up. It is about
                    // the turn after this one, so it is put aside for the loop
                    // and the question stays up: answering it would refuse a
                    // call the user never refused.
                    Some(TurnControl::Model(change)) => {
                        self.control.defer(change);
                        continue;
                    }
                    // `/fast` or `/ultrafast` likewise.
                    Some(TurnControl::Speed(change)) => {
                        self.control.defer_speed(change);
                        continue;
                    }
                    // `/undo` typed while this question is up: done once the
                    // turn is over, and the question stays up -- it refuses
                    // nothing ([`ControlChannel::defer_undo`]). Above the
                    // `stop` catch-all for the reason the arm before it is.
                    Some(TurnControl::Undo { through }) => {
                        self.control.defer_undo(through, false);
                        continue;
                    }
                    // `/rename` typed while this question is up: written once
                    // the turn is over, and the question stays up
                    // ([`ControlChannel::defer_rename`]).
                    Some(TurnControl::Rename { title, announced }) => {
                        self.control.defer_rename(title, false, announced);
                        continue;
                    }
                    // `/model`'s menu asking for the catalog while this question
                    // is up: the read starts now, on a thread of its own, and
                    // tells the band itself -- this wait does not hold it
                    // ([`ControlChannel::ask_catalog`]) -- and the question
                    // stays up. What the band is told instead, when it is told
                    // something now, goes from here.
                    Some(TurnControl::Catalog) => {
                        if let Some(answer) = self.control.ask_catalog() {
                            let _ = send_ui(&self.events, &token, answer).await;
                        }
                        continue;
                    }
                    // A cancel (Escape at the panel, or an interrupt) or a
                    // shutdown. Both refuse this call, and both go back on the
                    // channel for the loop that can act on them -- see
                    // [`ControlChannel::put_back`]. `give_back` rearms.
                    Some(stop) => {
                        self.control.give_back(stop);
                        return Ok(ApprovalResponse::plain(ApprovalAnswer::Deny));
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
        let subject = if tool == "terminal" {
            crate::permission::ApprovalSubject::Command {
                command: "cat notes.txt".into(),
                cwd: ".".into(),
            }
        } else {
            crate::permission::ApprovalSubject::Path("notes.txt".into())
        };
        ApprovalRequest {
            tool,
            target: "notes.txt".into(),
            summary: "replace `alpha` with `beta` in notes.txt".into(),
            always_scope:
                "allow every future edit_file to `notes.txt` for the rest of this session".into(),
            diff: None,
            subject,
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
            // The long absolute path a real workspace has.
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
        session.decide_with_feedback(ProposedAction::Mutation(&plan));
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
            let wanted = super::super::wrap::wrap(&scope, 78).len();
            assert!(
                wanted >= 3,
                "durable={durable}: the real scope wants only {wanted} rows, so this case no \
                 longer reproduces what it was written for: {scope:?}"
            );

            // And an ordinary screen discloses it whole anyway, because the
            // panel's rows are measured from the content rather than read out
            // of a table.
            let panel = Panel::new(asked);
            let composed = panel.compose(80, 24, 1);
            let shown = unwrapped(&composed.rows);
            assert!(
                shown.contains(&scope),
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

    /// Rows as one sentence, with the wrap taken back out, so a case can ask
    /// whether a sentence reached the panel whole wherever the wrap fell.
    fn unwrapped(rows: &[String]) -> String {
        rows.join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
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

    /// The same question, replacing `lines` lines of a file with as many new
    /// ones: a review of twice as many rows.
    fn with_lines(lines: usize) -> ApprovalRequest {
        let mut asked = request("edit_file");
        asked.diff = Some(ApprovalDiff {
            before: (0..lines).map(|line| format!("alpha {line}\n")).collect(),
            after: (0..lines).map(|line| format!("beta {line}\n")).collect(),
            first_line: 1,
            lines_below: 0,
        });
        asked
    }

    #[test]
    fn a_change_reviewed_in_more_rows_than_the_band_holds_is_reviewed_on_a_screen_of_its_own() {
        // The rule is about the *change*, and [`ApprovalSurface::for_request`]
        // is given nothing else to decide from -- no rows, no columns. A rule
        // keyed on the terminal's height would put a one-word edit on a full
        // screen the moment somebody made their window short. (Claim moved:
        // this was the change's bytes against the 160 the band's sentence
        // quoted; the band shows the review now, so it is the review's rows.)
        assert_eq!(
            ApprovalSurface::for_request(&with_lines(7)),
            ApprovalSurface::Alternate,
            "fourteen rows of review were asked in the band"
        );
        let mut one_sided = request("write_file");
        one_sided.diff = Some(ApprovalDiff {
            before: String::new(),
            after: (0..=BAND_REVIEW_LINES)
                .map(|line| format!("{line}\n"))
                .collect(),
            first_line: 1,
            lines_below: 0,
        });
        assert_eq!(
            ApprovalSurface::for_request(&one_sided),
            ApprovalSurface::Alternate,
            "a change that only adds is still a change too big for the band"
        );
    }

    #[test]
    fn a_small_change_and_a_question_with_no_diff_at_all_stay_in_the_band() {
        // A command has no diff to review; a change whose review fits the band
        // has nothing a second surface would add, and taking the screen for it
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
            ApprovalSurface::for_request(&with_lines(BAND_REVIEW_LINES / 2)),
            ApprovalSurface::Inline
        );
        // **Rows, not bytes**: a one-line change at the top of a long file is
        // a short review -- the change and the five lines under it -- however
        // many kilobytes the file is.
        let long: String = (0..4_000).map(|line| format!("line {line}\n")).collect();
        let mut near_the_top = request("edit_file");
        near_the_top.diff = Some(ApprovalDiff {
            before: long.clone(),
            after: long.replacen("line 0\n", "LINE ZERO\n", 1),
            first_line: 1,
            lines_below: 0,
        });
        assert_eq!(
            ApprovalSurface::for_request(&near_the_top),
            ApprovalSurface::Inline
        );
    }

    #[test]
    fn the_band_question_is_upstreams_file_review_row_for_row() {
        // `fx-approvals-40x120-edit-ask`, rows 6-18 (the rows between the two
        // solid rules), and the scope row xfx adds under the choices.
        let mut asked = request("edit_file");
        asked.subject = crate::permission::ApprovalSubject::Path("a.txt".into());
        asked.always_scope = "allow every future edit_file to `a.txt`".into();
        asked.diff = Some(ApprovalDiff {
            before: "alpha line 1\nalpha line 2\nalpha line 3\n".into(),
            after: "alpha line 1\nALPHA LINE TWO\nalpha line 3\n".into(),
            first_line: 1,
            lines_below: 0,
        });
        let rows = Panel::new(asked).rows(120, 40);
        let header = format!(
            "  Permission needed · Review change{}Edit · +1  -1",
            " ".repeat(120 - 4 - 33 - 13)
        );
        assert_eq!(
            rows,
            [
                "      1   alpha line 1".to_string(),
                "      2 - alpha line 2".to_string(),
                "      2 + ALPHA LINE TWO".to_string(),
                "      3   alpha line 3".to_string(),
                "┄".repeat(120),
                header,
                String::new(),
                "  a.txt  ·  Apply this change?".to_string(),
                String::new(),
                "  ❯ 1  Apply once".to_string(),
                "    2  Apply + allow future edits to this file for this session".to_string(),
                "    3  Don't apply".to_string(),
                "  2 = allow every future edit_file to `a.txt`".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn the_band_question_is_upstreams_command_question_row_for_row() {
        // `fx-approvals-40x120-cmd-ask`, rows 13-23, in xfx's terms: the
        // command's comment names xfx's tool and where it runs.
        let mut asked = request("terminal");
        asked.subject = crate::permission::ApprovalSubject::Command {
            command: "echo hi > out.txt && cat out.txt".into(),
            cwd: ".".into(),
        };
        asked.always_scope = "allow every future run of exactly `echo hi` in .".into();
        let rows = Panel::new(asked).rows(120, 40);
        let header = format!(
            "  Permission needed · Choose one{}Command",
            " ".repeat(120 - 4 - 30 - 7)
        );
        assert_eq!(
            rows,
            [
                header,
                String::new(),
                "  Would you like to run the following command?".to_string(),
                String::new(),
                "  $ # terminal.exec cwd=.".to_string(),
                "    echo hi > out.txt && cat out.txt".to_string(),
                String::new(),
                "  ❯ 1. Yes".to_string(),
                "    2. Yes, and don't ask again for this exact command".to_string(),
                "    3. No".to_string(),
                "  2 = allow every future run of exactly `echo hi` in .".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn the_hint_row_says_tab_amends_on_the_two_choices_that_carry_a_draft() {
        // `edit-ask` and `edit-down1`: Tab amends on `1` and `3`, and moves on
        // `2` -- so the row says `↑↓ or tab options` there.
        let palette = crate::tui::theme::Palette {
            mode: crate::tui::theme::Mode::Dark,
            depth: crate::tui::theme::Depth::Ansi256,
        };
        let mut panel = Panel::new(request("edit_file"));
        assert!(panel.hint(120, &palette).contains("tab amend"));
        panel.apply(Action::Down, 120);
        let hint = panel.hint(120, &palette);
        assert!(
            hint.contains("↑↓ or tab options") && !hint.contains("tab amend"),
            "{hint:?}"
        );
        panel.apply(Action::Down, 120);
        assert!(panel.hint(120, &palette).contains("tab amend"));
    }

    #[test]
    fn the_always_wording_says_which_of_the_two_it_is() {
        // approval_ui.zig:1966-1972 for a command; a file's names xfx's grant,
        // which is this tool on this file rather than upstream's workspace
        // file access. xfx advertises no MCP tool, so upstream's third wording
        // is unreachable here and is not written down as if it were.
        use super::super::approval_view::label;
        assert_eq!(
            label(Kind::Command, 1),
            "2. Yes, and don't ask again for this exact command"
        );
        assert_eq!(
            label(Kind::Other, 1),
            "2. Yes, and don't ask again for this request"
        );
        assert_eq!(
            label(Kind::Edit, 1),
            "Apply + allow future edits to this file for this session"
        );
        assert_eq!(
            label(Kind::Write, 1),
            "Apply + allow future writes to this file for this session"
        );
    }

    #[test]
    fn the_panel_discloses_what_always_would_grant() {
        // The line shell's prompt says this, so the panel that replaces it must
        // too -- a narrower safety surface is a regression, not a port.
        let panel = Panel::new(request("edit_file"));
        let rows = panel.rows(80, 24).join("\n");
        assert!(rows.contains("notes.txt  ·  Apply this change?"), "{rows}");
        assert!(rows.contains("for the rest of this session"), "{rows}");
    }

    #[test]
    fn the_measured_height_is_the_number_of_rows_actually_painted() {
        // approval_ui.zig:268-294: one construction for both, so the count
        // cannot drift from the paint and leave a stale row in the band.
        for (cols, terminal_rows) in [(80u16, 24u16), (40, 24), (80, 40), (30, 40)] {
            for tool in ["terminal", "edit_file", "create_folder"] {
                let panel = Panel::new(request(tool));
                assert_eq!(
                    panel.height(cols, terminal_rows) as usize,
                    panel.rows(cols, terminal_rows).len(),
                    "{tool} {cols}x{terminal_rows}"
                );
            }
        }
        // The taller screen differs from the ordinary one by exactly the three
        // blank rows it can afford -- under the header, under the subject and
        // under the scope -- and nothing else.
        let panel = Panel::new(request("terminal"));
        assert_eq!(
            panel.height(80, 40),
            panel.height(80, 24) + 3,
            "the tall screen bought something other than air"
        );
        // **A changed claim.** A panel used to be clamped to the screen, with
        // an ellipsis where it was cut; it is never cut now -- every part is
        // wrapped onto the rows it needs -- so a request too long for the band
        // is one the band does not install (`super::super::shell::Shell::ask`
        // moves a change to the plane and refuses a command).
        let mut vast = request("terminal");
        vast.subject = crate::permission::ApprovalSubject::Command {
            command: "x".repeat(9_000),
            cwd: ".".into(),
        };
        let height = Panel::new(vast).height(80, 24);
        assert!(height > 24, "{height}");
        assert!(!super::super::layout::fits_approval(24, 80, height));
    }

    /// The answer a key produced, for a test that is about the decision rather
    /// than about the frame it owes.
    ///
    /// Kept as a helper rather than spelling [`Reply`] out at every assertion,
    /// so the cases below read as they did before the amendment draft gave
    /// "nothing was answered" three separate meanings.
    fn answer(reply: Reply) -> Option<ApprovalAnswer> {
        match reply {
            Reply::Answer { answer, .. } => Some(answer),
            _ => None,
        }
    }

    #[test]
    fn every_key_the_panel_answers_to_produces_the_documented_outcome() {
        // ui/input/runtime.zig:59-71. Escape and Ctrl-C both refuse *this call*
        // here; what each does to the turn is the shell's
        // (`super::super::shell::Shell::decide`).
        let mut panel = Panel::new(request("edit_file"));
        assert_eq!(
            answer(panel.apply(Action::Text('1'), 80)),
            Some(ApprovalAnswer::Once)
        );
        assert_eq!(
            answer(panel.apply(Action::Text('2'), 80)),
            Some(ApprovalAnswer::Always)
        );
        assert_eq!(
            answer(panel.apply(Action::Text('3'), 80)),
            Some(ApprovalAnswer::Deny)
        );
        assert_eq!(
            answer(panel.apply(Action::Escape, 80)),
            Some(ApprovalAnswer::Deny)
        );
        assert_eq!(
            answer(panel.apply(Action::Cancel, 80)),
            Some(ApprovalAnswer::Deny)
        );
        assert_eq!(
            answer(panel.apply(Action::Down, 80)),
            None,
            "moving is not answering"
        );
        assert_eq!(
            answer(panel.apply(Action::Submit, 80)),
            Some(ApprovalAnswer::Always)
        );
        // Tab from `Always`, which is the one choice with no draft, so it is
        // still the cycle it has always been. Tab from an *eligible* choice
        // opens that choice's amendment instead, which is
        // `tab_enters_the_draft_of_an_eligible_choice_and_cycles_past_an_ineligible_one`.
        panel.apply(Action::Tab, 80);
        assert_eq!(
            answer(panel.apply(Action::Submit, 80)),
            Some(ApprovalAnswer::Deny)
        );
    }

    #[test]
    fn the_arrows_walk_the_same_three_choices_and_wrap() {
        // Without the wrap, an Up at the top and a Down at the bottom would each
        // be a keystroke that does nothing, on a panel whose whole job is to be
        // answerable without reading a manual (`edit-down3-wrap`).
        let mut panel = Panel::new(request("edit_file"));
        assert_eq!(answer(panel.apply(Action::Up, 80)), None);
        assert_eq!(
            answer(panel.apply(Action::Submit, 80)),
            Some(ApprovalAnswer::Deny),
            "Up from the first choice did not wrap to the last"
        );
        assert_eq!(answer(panel.apply(Action::Down, 80)), None);
        assert_eq!(
            answer(panel.apply(Action::Submit, 80)),
            Some(ApprovalAnswer::Once),
            "Down from the last choice did not wrap to the first"
        );
        assert_eq!(answer(panel.apply(Action::Down, 80)), None);
        assert_eq!(answer(panel.apply(Action::Up, 80)), None);
        assert_eq!(
            answer(panel.apply(Action::Submit, 80)),
            Some(ApprovalAnswer::Once),
            "a step each way did not come back"
        );
    }

    #[test]
    fn the_marker_is_on_the_choice_enter_would_take_and_the_caret_is_on_that_row() {
        // The two halves of "which one is selected" -- what the eye reads and
        // where the terminal puts the caret -- from one index, so they cannot
        // point at different rows.
        for tool in ["edit_file", "terminal"] {
            let mut panel = Panel::new(request(tool));
            for expected in [0usize, 1, 2] {
                let rows = panel.rows(80, 24);
                let marked: Vec<usize> = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.starts_with("  ❯ "))
                    .map(|(index, _)| index)
                    .collect();
                assert_eq!(marked.len(), 1, "{rows:?}");
                assert_eq!(
                    marked[0],
                    usize::from(panel.caret_row(80, 24)),
                    "the caret is not on the marked row"
                );
                assert!(
                    rows[marked[0]].contains(&format!("{}", expected + 1)),
                    "{rows:?}"
                );
                panel.apply(Action::Down, 80);
            }
        }
    }

    #[test]
    fn a_long_path_is_wrapped_whole_rather_than_cut() {
        // **A changed claim.** The band used to quote the target inside a
        // sentence with an allotment, cut it with an ellipsis and refuse the
        // grant; the target has rows of its own now and takes as many as it
        // needs, so it is never cut -- the way a long path chosen to share a
        // visible prefix with a benign one gets approved.
        let mut asked = request("edit_file");
        let path = "deep/".repeat(60) + "notes.md";
        asked.subject = crate::permission::ApprovalSubject::Path(path.clone());
        let panel = Panel::new(asked);
        let composed = panel.compose(80, 24, 1);
        let joined: String = composed
            .rows
            .iter()
            .map(|row| row.trim())
            .collect::<Vec<_>>()
            .join("");
        assert!(
            joined.contains(&path),
            "the path was cut: {:?}",
            composed.rows
        );
        assert!(composed.disclosure.subject_whole);
        for row in &composed.rows {
            assert!(
                super::super::wrap::width(row) <= 80,
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
        // Eight: the review's one row (this fixture has no diff), the dotted
        // rule, the header, the path and its question, the three answers and
        // the one row this scope needs.
        assert_eq!(rows.len(), 8, "{rows:?}");
        let joined = rows.join("\n");
        assert!(joined.contains(TITLE), "{joined}");
        assert!(joined.contains("1  Apply once"), "{joined}");
        assert!(joined.contains("2  Apply + allow future edits"), "{joined}");
        assert!(joined.contains("3  Don't apply"), "{joined}");
        assert!(joined.contains("for the rest of this session"), "{joined}");
        assert!(
            !rows.iter().any(String::is_empty),
            "a short panel spent a row on nothing: {rows:?}"
        );
    }

    #[test]
    fn a_taller_screen_buys_only_the_blank_rows() {
        // (Claim narrowed: there is no summary left to give more rows to, and
        // every part is whole on both screens; what remains is that the taller
        // one buys its blank rows and nothing else.)
        let panel = Panel::new(request("edit_file"));
        let compact = panel.rows(60, 24);
        let spacious = panel.rows(60, 40);
        assert_eq!(
            spacious.len(),
            compact.len() + 3,
            "the taller screen bought something other than its three blank rows"
        );
        assert_eq!(
            spacious.iter().filter(|row| row.is_empty()).count(),
            3,
            "{spacious:?}"
        );
        let said = |rows: &[String]| {
            rows.iter()
                .filter(|row| !row.is_empty())
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(said(&spacious), said(&compact));
    }

    // -----------------------------------------------------------------------
    // what the composition really disclosed
    // -----------------------------------------------------------------------

    #[test]
    fn a_clipped_always_label_is_not_a_disclosed_control() {
        // The clip has to be a **no-op**, not merely "something survived beside
        // the marker": `Apply + allow future edits to th` is exactly the
        // sentence with the difference between one call and the whole session
        // cut out of it.
        let panel = Panel::new(request("edit_file"));
        let wide = panel.compose(80, 24, 1);
        assert!(wide.disclosure.controls_whole && wide.disclosure.scope_whole);
        // 30 cells cuts the second label mid-sentence.
        let narrow = panel.compose(30, 24, 1);
        assert!(
            !narrow.disclosure.controls_whole,
            "a half-shown grant read as disclosed"
        );
    }

    #[test]
    fn the_last_control_row_is_the_terminal_row_that_row_is_really_on() {
        // The band puts rows of its own before the panel
        // (`super::super::shell::Shell::band_rows`), so a panel-local index is
        // not a row of anybody's screen -- and the guard the disclosure feeds
        // (`Intent::capture`) is about the screen. Where the band really paints
        // it is the shell's case
        // (`a_question_over_a_forks_plain_line_keeps_the_blank_row_and_names_its_controls_row`).
        let panel = Panel::new(request("edit_file"));
        let rows = panel.rows(80, 24);
        let last_choice = rows
            .iter()
            .rposition(|row| row.contains("3  Don't apply"))
            .expect("the refusal is a row of the panel");
        for first_row in [10u16, 11, 1] {
            assert_eq!(
                panel.compose(80, 24, first_row).disclosure.last_control_row,
                first_row + u16::try_from(last_choice).expect("a small index"),
                "{first_row}"
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
            feedback: None,
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
                answer: ApprovalAnswer::Once,
                feedback: None,
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
            feedback: None,
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
            feedback: None,
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
            feedback: None,
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
            feedback: None,
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
    fn a_model_change_at_a_question_is_put_aside_and_the_question_stays_up() {
        // `/model` typed while a permission question waits is about the next
        // turn: the prompter must not take it for the stop it is not -- that
        // would refuse a call the user never refused -- and must not lose it
        // either. It goes aside for the loop, and the question is answered by
        // its own answer.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let change = super::super::bridge::ModelChange {
            name: "next/model".to_string(),
            effort: None,
            picker: false,
            announced: true,
            next_turn: true,
            speed: None,
            provider: crate::provider::ProviderId::Gateway,
            effort_only: false,
        };
        tx.send(TurnControl::Model(change.clone()))
            .expect("the channel is open");
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Once,
            feedback: None,
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
            ApprovalAnswer::Once,
            "a model change answered the question"
        );
        assert_eq!(control.take_deferred(), vec![Deferred::Model(change)]);
        assert_eq!(
            control.waiting(),
            None,
            "the change was handed on as a stop"
        );
    }

    #[test]
    fn an_undo_at_a_question_is_put_aside_and_the_question_stays_up() {
        // `/undo` typed while a permission question waits: the prompter's
        // catch-all treats anything it does not know as a stop and refuses
        // the call, so without an arm of its own an undo would deny a change
        // the user never denied. It goes aside for the loop, in typed order
        // with a model change, and the question is answered by its own answer.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        let change = super::super::bridge::ModelChange {
            name: "next/model".to_string(),
            effort: None,
            speed: None,
            picker: false,
            announced: true,
            next_turn: true,
            provider: crate::provider::ProviderId::Gateway,
            effort_only: false,
        };
        let speed = super::super::bridge::SpeedChange {
            fast: true,
            ultrafast: false,
            topic: "fast",
            provider: crate::provider::ProviderId::Gateway,
        };
        let typed = crate::tools::undo::Mark::now();
        tx.send(TurnControl::Undo { through: typed })
            .expect("the channel is open");
        tx.send(TurnControl::Model(change.clone()))
            .expect("the channel is open");
        // A `/fast` too: it answers no question either, and waits behind the
        // model change it was typed after.
        tx.send(TurnControl::Speed(speed.clone()))
            .expect("the channel is open");
        tx.send(TurnControl::Answer {
            id: ApprovalId(1),
            answer: ApprovalAnswer::Once,
            feedback: None,
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
            ApprovalAnswer::Once,
            "an undo answered the question"
        );
        assert_eq!(
            control.take_deferred(),
            vec![
                Deferred::Undo {
                    through: typed,
                    after_reset: false
                },
                Deferred::Model(change),
                Deferred::Speed(speed)
            ]
        );
        assert_eq!(control.waiting(), None, "the undo was handed on as a stop");
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
                feedback: None,
            })
            .expect("the channel is open");
        prompter.request(&request("edit_file")).expect("answered");
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(2),
                answer: ApprovalAnswer::Once,
                feedback: None,
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
                feedback: None,
            })
            .expect("the channel is open");
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
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

    // -----------------------------------------------------------------------
    // the amendment draft
    // -----------------------------------------------------------------------

    /// The choice a keystroke left marked, read through the answer Enter would
    /// take.
    ///
    /// Asked of `Submit` rather than of the private index, because what a test
    /// about navigation is really claiming is what Enter would now do.
    fn marked(panel: &mut Panel) -> ApprovalAnswer {
        match panel.apply(Action::Submit, 80) {
            Reply::Answer { answer, .. } => answer,
            other => panic!("Submit did not answer: {other:?}"),
        }
    }

    /// Types `text` into whatever draft has the keys.
    fn type_into(panel: &mut Panel, text: &str) {
        for character in text.chars() {
            panel.apply(Action::Text(character), 80);
        }
    }

    #[test]
    fn tab_enters_the_draft_of_an_eligible_choice_and_cycles_past_an_ineligible_one() {
        // Two halves of one key (`approval_decision.zig::apply`). On choice 0
        // and choice 2 Tab opens that side's amendment; on choice 1 -- `Always`,
        // which buys the rest of the session and whose scope is keyed by tool
        // and target -- there is no draft to open, so Tab is the cycle it has
        // always been.
        let mut panel = Panel::new(request("edit_file"));
        assert!(!panel.drafting());
        assert_eq!(
            panel.apply(Action::Tab, 80),
            Reply::Reshaped,
            "opening a draft did not change the panel's row count"
        );
        assert!(panel.drafting(), "Tab on `1. Yes` did not open its draft");
        // And it never leaves one: Tab enters a draft, it does not exit it.
        assert_eq!(panel.apply(Action::Tab, 80), Reply::Ignored);
        assert!(panel.drafting());

        // Choice 1 is the ineligible one. Down ends editing and marks `Always`;
        // Tab there cycles to `Deny`, and a second Tab opens *that* draft.
        panel.apply(Action::Down, 80);
        assert!(!panel.drafting(), "an arrow did not end the editing");
        assert_eq!(
            panel.apply(Action::Tab, 80),
            Reply::Moved,
            "Tab past the ineligible choice reshaped a panel it only walked"
        );
        assert!(!panel.drafting(), "`Always` acquired a draft");
        assert_eq!(panel.apply(Action::Tab, 80), Reply::Reshaped);
        assert!(panel.drafting(), "Tab on `3. No` did not open its draft");
    }

    #[test]
    fn moving_the_choice_leaves_editing_and_keeps_both_drafts() {
        // `approval_decision.zig:285-302`, transcribed: tab, type `y`,
        // `move_choice.previous` to choice 2, tab, type `n` -- and the two
        // drafts survive separately. `moveChoice` updates `selected` and then
        // calls `amendment.clearActive()` (`:214-225`), so moving the choice is
        // what ends editing.
        let mut panel = Panel::new(request("edit_file"));
        panel.apply(Action::Tab, 80);
        type_into(&mut panel, "y");
        assert_eq!(panel.apply(Action::Up, 80), Reply::Moved);
        assert!(!panel.drafting(), "the Up did not end the editing");
        panel.apply(Action::Tab, 80);
        type_into(&mut panel, "n");

        // Each on its own choice's row, as upstream puts the one being typed
        // (`approval_ui.zig:1559-1592`) -- and the one navigated away from
        // stays shown, because it still travels if that choice is submitted.
        let painted = panel.rows(80, 24).join("\n");
        assert!(
            painted.contains("Apply once, y") && painted.contains("Don't apply, n"),
            "the two drafts do not both survive on the panel: {painted:?}"
        );

        // And only the submitted side travels: the refusal carries `n`, and the
        // reason typed at the allow side is discarded rather than sent.
        let answer = marked(&mut panel);
        assert_eq!(answer, ApprovalAnswer::Deny);
        assert_eq!(
            panel.take_feedback(answer, true),
            Some("n".to_string()),
            "the wrong draft travelled, or none did"
        );
    }

    #[test]
    fn digits_typed_into_a_draft_are_characters_and_digits_outside_one_are_answers() {
        // Both directions, in one pair, because they are one ruling: a `3` is a
        // shorthand for the third answer until a draft has the keys, and a
        // character the moment one does. Without the second half a user typing
        // "3 files is too many" would refuse the call on the first keystroke.
        let mut outside = Panel::new(request("edit_file"));
        assert_eq!(
            outside.apply(Action::Text('3'), 80),
            Reply::Answer {
                answer: ApprovalAnswer::Deny,
                amendable: true
            },
            "a digit outside a draft stopped being an answer"
        );

        let mut inside = Panel::new(request("edit_file"));
        inside.apply(Action::Tab, 80);
        type_into(&mut inside, "3 files is too many");
        assert!(
            inside.drafting(),
            "a digit answered the question from a draft"
        );
        let answer = marked(&mut inside);
        assert_eq!(answer, ApprovalAnswer::Once);
        assert_eq!(
            inside.take_feedback(answer, true),
            Some("3 files is too many".to_string()),
            "the digits were not draft characters"
        );
    }

    #[test]
    fn escape_and_ctrl_c_refuse_without_the_sentence_and_enter_sends_it() {
        // **The M6 pair, in one case so it cannot drift.** Same draft text, two
        // exits. Escape cancels the turn and Ctrl-C refuses with a key that
        // says nothing (`approval_decision.zig:132-135`), so a user bailing out
        // of a panel did not choose to say the sentence that happens to be in
        // the deny draft. Submitting `3` with Enter is a *chosen* refusal and
        // does send it. (Renamed: Ctrl-C is no longer the interrupt here.)
        let filled = |panel: &mut Panel| {
            panel.apply(Action::Down, 80);
            panel.apply(Action::Down, 80);
            panel.apply(Action::Tab, 80);
            type_into(panel, "this would delete the fixtures");
        };

        for bail in [Action::Escape, Action::Cancel] {
            let mut panel = Panel::new(request("edit_file"));
            filled(&mut panel);
            let Reply::Answer { answer, amendable } = panel.apply(bail, 80) else {
                panic!("{bail:?} did not answer");
            };
            assert_eq!(answer, ApprovalAnswer::Deny);
            assert!(!amendable, "{bail:?} offered to carry the draft");
            assert_eq!(
                panel.take_feedback(answer, amendable),
                None,
                "{bail:?} sent a sentence the user bailed out of"
            );
        }

        let mut panel = Panel::new(request("edit_file"));
        filled(&mut panel);
        let Reply::Answer { answer, amendable } = panel.apply(Action::Submit, 80) else {
            panic!("Enter did not answer");
        };
        assert_eq!(answer, ApprovalAnswer::Deny);
        assert!(amendable);
        assert_eq!(
            panel.take_feedback(answer, amendable),
            Some("this would delete the fixtures".to_string()),
            "a chosen refusal did not carry its reason"
        );
    }

    #[test]
    fn a_draft_never_shortens_the_target_the_controls_or_the_scope() {
        // The drafts are paid for **last**, out of what is left: they are the
        // one block on this panel that is not the question, and all three of the
        // things a readiness receipt is about sit above them.
        for durable in [true, false] {
            let mut panel = Panel::new(as_the_session_builds_it(durable));
            panel.apply(Action::Tab, 80);
            type_into(&mut panel, &"reason ".repeat(60));
            let composed = panel.compose(80, 24, 1);
            assert!(
                composed.disclosure.subject_whole,
                "durable={durable}: a draft cut the target: {:?}",
                composed.rows
            );
            assert!(
                composed.disclosure.controls_whole,
                "durable={durable}: a draft cut an answer: {:?}",
                composed.rows
            );
            assert!(
                composed.disclosure.scope_whole,
                "durable={durable}: a draft cut what `always` would buy: {:?}",
                composed.rows
            );
            assert!(
                composed.rows.len() <= 24,
                "durable={durable}: a draft grew the panel past the screen: {}",
                composed.rows.len()
            );
            assert!(
                Intent::capture(ApprovalId(1), Surface::Inline, 80, 24, composed.disclosure)
                    .is_some(),
                "durable={durable}: an ordinary screen could not disclose an amended request"
            );
        }
    }

    #[test]
    fn a_draft_wraps_and_places_its_caret_at_its_own_width_on_its_choices_row() {
        // One width per draft, three consumers ([`draft_cols`]): the editor
        // that applies a key, the rows the panel paints, and the caret the
        // shell places. A caret computed at one width and a wrap computed at
        // another is a caret standing on the wrong cell. (Claim changed: the
        // draft sits on its choice's row after the choice's words, as
        // upstream's does, so its width is what those words leave rather than
        // the panel's inner width.)
        for cols in [80u16, 40] {
            assert_eq!(
                draft_cols(Kind::Edit, 0, cols),
                cols - draft_column(Kind::Edit, 0),
                "the draft is not as wide as its row leaves it"
            );
        }

        let mut panel = Panel::new(request("edit_file"));
        panel.apply(Action::Tab, 80);
        // Wide scalars and an emoji, so a harness counting scalars rather than
        // cells would put the caret in the wrong column.
        type_into(&mut panel, &"가나다라마바사아자차카타파하".repeat(5));

        for cols in [80u16, 40, 80] {
            let rows = panel.rows(cols, 24);
            let (row, column) = panel.caret(cols, 24);
            let painted = rows
                .get(usize::from(row))
                .unwrap_or_else(|| panic!("{cols}: the caret is not on a row of the panel"));
            // The caret sits past the text on its row -- the choice's words and
            // the draft on the first row, the draft's column and text on the
            // rest -- on the one reverse-video cell upstream paints there
            // (`writeApprovalDraft`, `approval_ui.zig:1662-1672`), which is the
            // row's last, and never off the screen.
            assert_eq!(
                column,
                super::super::wrap::width(painted)
                    .saturating_sub(1)
                    .min(cols.saturating_sub(1)),
                "{cols}: the caret is not on the last cell of the row it is on: {painted:?}"
            );
            assert!(
                usize::from(column) >= usize::from(draft_column(Kind::Edit, 0)),
                "{cols}: the caret is not inside the draft: {painted:?}"
            );
            for row in &rows {
                assert!(
                    super::super::wrap::width(row) <= cols,
                    "{cols}: a draft row outgrew the screen: {row:?}"
                );
            }
        }
    }

    #[test]
    fn a_stale_approval_id_is_consumed_with_its_feedback() {
        // A keystroke left over at a question that has gone grants nothing and
        // **says** nothing: inheriting its sentence would put words in the
        // user's mouth about a call they never saw.
        let (mut prompter, mut events, control, answers) = a_prompter();
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(9_999),
                answer: ApprovalAnswer::Always,
                feedback: Some("retired question's sentence".to_string()),
            })
            .expect("the channel is open");
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Deny,
                feedback: None,
            })
            .expect("the channel is open");

        let response = prompter.respond(&request("edit_file")).expect("answered");
        assert_eq!(response.answer, ApprovalAnswer::Deny);
        assert_eq!(
            response.feedback, None,
            "the live question inherited a retired question's sentence"
        );
        assert!(
            control.waiting().is_none(),
            "the stale answer was consumed, not put back"
        );
        let _ = asked_id(&mut events);
    }

    #[test]
    fn the_answer_the_user_gave_travels_with_whatever_they_said_about_it() {
        // `request` is a projection of `respond` rather than a second body, so
        // the two cannot disagree about which ids are stale; and every early-out
        // reports `feedback: None`, because nobody was asked or nobody answered.
        let (mut prompter, mut events, _control, answers) = a_prompter();
        answers
            .send(TurnControl::Answer {
                id: ApprovalId(1),
                answer: ApprovalAnswer::Once,
                feedback: Some("only this once, and log it".to_string()),
            })
            .expect("the channel is open");
        assert_eq!(
            prompter.respond(&request("edit_file")).expect("answered"),
            ApprovalResponse {
                answer: ApprovalAnswer::Once,
                feedback: Some("only this once, and log it".to_string()),
            }
        );
        let _ = asked_id(&mut events);

        // The interrupt: a refusal, and nothing said.
        let (tx, rx) = mpsc::unbounded_channel();
        let control = ControlChannel::new(rx);
        tx.send(TurnControl::Cancel { through: 3 })
            .expect("the channel is open");
        let (sender, _seen) = mpsc::channel(4);
        let mut prompter = TuiPrompter::new(
            sender,
            Arc::clone(&control),
            Cancellation::new(crate::gateway::CancelToken::new()),
        );
        assert_eq!(
            prompter.respond(&request("edit_file")).expect("an answer"),
            ApprovalResponse::plain(ApprovalAnswer::Deny)
        );
        assert_eq!(control.waiting(), Some(TurnControl::Cancel { through: 3 }));

        // A session shutting down, which never reaches a panel at all.
        let (sender, _seen) = mpsc::channel(4);
        let (_tx, rx) = mpsc::unbounded_channel();
        let cancel = Cancellation::new(crate::gateway::CancelToken::new());
        cancel.cancel();
        let mut prompter = TuiPrompter::new(sender, ControlChannel::new(rx), cancel);
        assert_eq!(
            prompter.respond(&request("edit_file")).expect("an answer"),
            ApprovalResponse::plain(ApprovalAnswer::Deny)
        );
    }
}
