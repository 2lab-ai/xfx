//! The question the model asks inside the band, and the keystrokes that answer
//! it.
//!
//! `ask_user_question` is not an approval and mints no authority
//! ([`crate::tools::question`]): what arrives is a batch of 1 to 4 questions,
//! each with 2 to 6 labelled choices plus a synthetic freeform slot, and what
//! goes back is one answer per question in **entry order** -- or nothing at all,
//! which the tool turns into its own cancellation sentinel. So none of
//! `super::approval`'s vocabulary is reused here: no [`ApprovalRequest`], no
//! `ApprovalAnswer`, no `Panel`. What is shared is the *shape* of a band
//! occupant -- `rows`, `height`, `apply` -- because the band solves its geometry
//! from exactly those three and a second shape would be a second thing for
//! `super::shell::Shell::fit` to get wrong.
//!
//! [`ApprovalRequest`]: crate::permission::ApprovalRequest
//!
//! # What is canonical and what is only painted
//!
//! The entries arrive **already terminal-safe encoded** (`terminal_safe`, run
//! before the request ever left the tools layer), and they are never rewritten
//! here. A row too wide for the screen is clipped for the *screen*
//! (`super::frame::clip`), and the answer that goes back is the canonical label
//! or the canonical draft -- whole, whatever the terminal's width. A panel that
//! answered with what it had room to paint would change the meaning of the
//! answer the model reads back.
//!
//! # One question at a time, and the freeform slot is an index
//!
//! Upstream shows one question of the batch at a time
//! (`question_prompt.zig:603-634`), Tab cycles between them, and each keeps its
//! own marked choice, its own draft and its own answer. The freeform slot is
//! appended once, at construction, and is identified by **index** --
//! `options.len() - 1` -- rather than by comparing labels: a model that named
//! one of its own options `Other` must not thereby acquire a text editor.
//!
//! # Refusals rather than surprises
//!
//! * An ordinal for a choice the window is not showing does nothing. A digit is
//!   a shorthand for what is on the screen, and a screen too short to list every
//!   choice would otherwise let a user take one they cannot read.
//! * A draft at [`MAX_FREEFORM_ENCODED_BYTES`] refuses the next character
//!   silently, measured **after** encoding, because that is the bound the
//!   executor re-checks and a panel that let the draft past it would build an
//!   answer the tool then refuses whole.
//! * Escape and Ctrl-C both cancel the **batch**, not the question: there is one
//!   tool call and it is answered once. Which of the two keys it was is the
//!   shell's to tell apart ([`Answered::Cancelled`] says only that the batch is
//!   over).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use crate::tools::question::{
    terminal_safe, QuestionEntry, QuestionOption, FREEFORM_LABEL, MAX_FREEFORM_ENCODED_BYTES,
};
use crate::tools::QuestionRequester;

use super::approval::ControlChannel;
use super::bridge::{park_on, send_ui, Cancellation, TurnControl, UiEvent};
use super::editor::Editor;
use super::input::Action;

/// What marks the choice Enter would take.
const MARKER: &str = "> ";

/// What every other row is written into, so the block reads as one.
const INDENT: &str = "  ";

/// How many cells [`INDENT`] and [`MARKER`] both cost.
const INDENT_CELLS: u16 = 2;

/// What separates a choice's label from its description on the same row.
///
/// The same row rather than one of its own, and that is the panel's own ruling
/// rather than upstream's layout: the band's rows come out of the user's
/// document, the shape is `1 + visible + draft` so that the height and the paint
/// are one derivation, and a description row per choice would double what a
/// six-choice question costs the document.
const DESCRIPTION: &str = " - ";

/// The fewest choices a screen has to be able to show before xfx will ask at
/// all.
///
/// Two, because a question with one visible answer is not a choice; the shell
/// refuses the batch rather than painting half of it
/// ([`QuestionPanel::presents_choices`]).
const MIN_VISIBLE: usize = 2;

/// Which batch a message on the control channel is about.
///
/// Minted by whoever asked (Task 5's requester) and carried back with every
/// answer, so a keystroke that lands after the turn moved on is **discardable**
/// rather than an answer to whatever is being asked now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuestionId(pub u64);

/// One batch of questions, as it crosses to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuestionRequest {
    pub id: QuestionId,
    pub entries: Vec<QuestionEntry>,
}

/// What one keystroke means to a question that has the focus.
///
/// The panel's own vocabulary rather than [`Action`], for the reason
/// `super::approval::Action` is one: a `1` is a *character* to the decoder and
/// an *ordinal* here -- until the freeform slot is open, when it is a character
/// again -- so the shell translates rather than forwards, and a key the panel
/// does not bind cannot fall through into the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    /// A character the user typed.
    Text(char),
    /// The previous choice.
    Up,
    /// The next one.
    Down,
    /// The next **question**, wrapping. Not the next choice: the batch is what
    /// Tab walks (`question_prompt.zig:603-634`).
    Tab,
    /// Take the marked choice, or the draft when the freeform slot is marked.
    Submit,
    /// Cancel the batch.
    Escape,
    /// Cancel the batch. Ctrl-C is a byte here like everywhere else on this
    /// surface, and the shell also stops the turn with it.
    Cancel,
    /// The draft's own keys, while there is a draft.
    Backspace,
    Left,
    Right,
    Home,
    End,
}

impl Act {
    /// What an ordinary decoded action means to a question, and `None` for the
    /// keystrokes a question has no meaning for at all.
    ///
    /// Here rather than in `super::shell` so that "which keys a question binds"
    /// is one fact in the module that answers them, exactly as
    /// `super::picker::PickerAction::of` is.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up => Some(Self::Up),
            Action::Down => Some(Self::Down),
            Action::Tab => Some(Self::Tab),
            Action::Submit => Some(Self::Submit),
            Action::Escape => Some(Self::Escape),
            Action::Cancel => Some(Self::Cancel),
            Action::Backspace => Some(Self::Backspace),
            Action::Left => Some(Self::Left),
            Action::Right => Some(Self::Right),
            Action::Home => Some(Self::Home),
            Action::End => Some(Self::End),
            _ => None,
        }
    }
}

/// What one keystroke did to the batch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answered {
    /// Nothing changed, so nothing is owed a frame -- a refused ordinal, a
    /// character the panel swallowed, an editing key with no draft open.
    Nothing,
    /// The panel changed and owes a frame; the batch is still open.
    Redraw,
    /// Every question has an answer, in entry order.
    Submitted(Vec<String>),
    /// The user refused the batch.
    Cancelled,
}

/// One question of the batch, with the state that is its own.
struct Entry {
    /// The canonical question text. Never rewritten.
    question: String,
    /// The canonical choices, with the freeform slot appended last.
    options: Vec<QuestionOption>,
    /// An index into [`Self::options`].
    selected: usize,
    /// The first choice the window is showing, as of the last keystroke.
    ///
    /// Re-derived at paint time ([`window`]) rather than trusted, because a
    /// resize changes how many rows the window has without any keystroke
    /// happening: a panel that painted from a remembered top would put the
    /// marked choice off the block on the first screen that shrank under it.
    top: usize,
    /// What this question has been answered with, once it has been.
    answer: Option<String>,
    /// The freeform text, whether or not the slot is marked right now.
    ///
    /// An [`Editor`] rather than a `String` because the caret is the point: a
    /// draft with no caret cannot be edited anywhere but at its end, and the
    /// arrows, Home, End and Backspace are exactly what a user reaches for in a
    /// sentence they have just typed. Kept per entry so that Tab cycles without
    /// losing what was typed.
    draft: Editor,
}

impl Entry {
    fn new(entry: QuestionEntry) -> Self {
        let mut options = entry.options;
        // Appended **once**, here, so every later reader can identify it by
        // index. A label comparison would make a model's own `Other` open a text
        // editor, and would stop working the moment the wording changed.
        options.push(QuestionOption {
            label: FREEFORM_LABEL.to_string(),
            description: None,
        });
        Self {
            question: entry.question,
            options,
            selected: 0,
            top: 0,
            answer: None,
            draft: Editor::new(),
        }
    }

    /// Which index the freeform slot is.
    fn freeform(&self) -> usize {
        self.options.len().saturating_sub(1)
    }

    /// Whether the freeform slot is the marked choice, which is the whole of
    /// "there is a draft open".
    fn drafting(&self) -> bool {
        self.selected == self.freeform()
    }

    /// The answer this entry's marked choice stands for.
    fn choice(&self) -> String {
        if self.drafting() {
            return self.draft.text().to_string();
        }
        self.options
            .get(self.selected)
            .map(|option| option.label.clone())
            .unwrap_or_default()
    }

    /// The row of the draft the caret is on, wrapped to `cols`.
    ///
    /// The wrap is the editor's own, so the row shown and the caret reported are
    /// measured by one thing: the caret's row is the window, which is what keeps
    /// a draft longer than the screen showing the end the user is typing at
    /// rather than the beginning they have scrolled past.
    fn draft_row(&self, cols: u16) -> String {
        let (row, _) = self.draft.point(cols);
        self.draft
            .rows(cols)
            .get(row)
            .cloned()
            .unwrap_or_else(String::new)
    }
}

/// The batch, and where the user has got to in it.
pub(crate) struct QuestionPanel {
    id: QuestionId,
    entries: Vec<Entry>,
    /// An index into [`Self::entries`]: the one question that is shown.
    current: usize,
}

/// A manual `Debug`, because [`Editor`] has none and the band's slot is a
/// `Debug` enum (`super::shell::Slot`). What is printed is what a failure would
/// be about -- which question, which choice, what was typed -- rather than the
/// composer machinery behind the draft.
impl std::fmt::Debug for QuestionPanel {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut shown = out.debug_struct("QuestionPanel");
        shown.field("id", &self.id).field("current", &self.current);
        for (index, entry) in self.entries.iter().enumerate() {
            shown.field(
                &format!("entry{index}"),
                &(
                    &entry.question,
                    entry.selected,
                    entry.draft.text(),
                    &entry.answer,
                ),
            );
        }
        shown.finish()
    }
}

/// How many rows the panel takes on a screen of a given size, and whether that
/// screen can really hold it.
///
/// A value rather than three readings of the same conditions inside `rows`,
/// `height` and `apply`: every row the panel paints, the caret's row and the
/// window the ordinals are checked against all come from exactly these numbers,
/// and three derivations are three chances to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// How many choice rows the block has.
    visible: usize,
    /// Whether the draft row is one of them.
    drafting: bool,
    /// Whether the screen can hold the block **and** show enough of it to be a
    /// question.
    fits: bool,
}

impl QuestionPanel {
    pub(crate) fn new(request: QuestionRequest) -> Self {
        Self {
            id: request.id,
            entries: request.entries.into_iter().map(Entry::new).collect(),
            current: 0,
        }
    }

    /// Which batch this panel is answering.
    pub(crate) fn id(&self) -> QuestionId {
        self.id
    }

    /// The question that is shown, or nothing at all for a batch with no
    /// questions in it.
    ///
    /// Total rather than indexed: the bounds are the tool's
    /// (`crate::tools::question::parse` admits 1 to 4), and this type is
    /// constructible from anywhere in the crate -- so an empty batch is a panel
    /// that presents no choices and is refused by the shell, rather than a
    /// panic on the UI thread.
    fn entry(&self) -> Option<&Entry> {
        self.entries.get(self.current)
    }

    /// How many rows the panel gets on this screen, and how many of them are
    /// choices.
    ///
    /// `visible = min(options, max(2, rows / 3))`, then reduced until the whole
    /// block is one [`super::layout::fits_panel`] admits: the band is what the
    /// panel's rows come out of, so the screen's opinion is the last word.
    fn shape(&self, cols: u16, terminal_rows: u16) -> Shape {
        let Some(entry) = self.entry() else {
            return Shape {
                visible: 0,
                drafting: false,
                fits: false,
            };
        };
        let drafting = entry.drafting();
        // The title, and the draft row when there is a draft.
        let fixed = 1 + u16::from(drafting);
        let wanted = entry
            .options
            .len()
            .min(usize::from(terminal_rows / 3).max(MIN_VISIBLE));
        for visible in (1..=wanted).rev() {
            let height = fixed.saturating_add(u16::try_from(visible).unwrap_or(u16::MAX));
            if super::layout::fits_panel(terminal_rows, cols, height) {
                return Shape {
                    visible,
                    drafting,
                    // A screen that can show only one of several choices cannot
                    // ask the question, whatever it can paint.
                    fits: visible >= MIN_VISIBLE.min(entry.options.len()),
                };
            }
        }
        Shape {
            // Still a coherent block, so `rows` and `height` agree about a
            // screen nothing will be painted on: the shell refuses the batch
            // before anything is installed.
            visible: 1.min(entry.options.len()),
            drafting,
            fits: false,
        }
    }

    /// The panel's rows, top first: the question, the choices the window is
    /// showing, and the draft while the freeform slot is marked.
    ///
    /// Clipped to the screen **here**, by the painter's own rule
    /// (`super::frame::clip`), rather than left for the painter: a row measured
    /// by the band at one width and drawn at another is how a stale row is left
    /// standing in the band. The clip is a *screen* cut and nothing else -- the
    /// canonical strings are untouched, and it is the canonical label that
    /// becomes the answer.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        let Some(entry) = self.entry() else {
            return Vec::new();
        };
        let shape = self.shape(cols, terminal_rows);
        let mut rows = Vec::with_capacity(1 + shape.visible + usize::from(shape.drafting));
        rows.push(format!(
            "{} ({} of {})",
            entry.question,
            self.current + 1,
            self.entries.len()
        ));
        let top = window(
            entry.top,
            entry.selected,
            shape.visible,
            entry.options.len(),
        );
        for index in top..(top + shape.visible).min(entry.options.len()) {
            let marker = if index == entry.selected {
                MARKER
            } else {
                INDENT
            };
            // **Absolute** ordinals, so the number on the row is the number the
            // user types whatever the window is showing.
            let mut row = format!("{marker}{}. {}", index + 1, entry.options[index].label);
            if let Some(description) = entry.options[index].description.as_deref() {
                row.push_str(DESCRIPTION);
                row.push_str(description);
            }
            rows.push(row);
        }
        if shape.drafting {
            rows.push(format!("{INDENT}{}", entry.draft_row(draft_cols(cols))));
        }
        rows.into_iter()
            .map(|row| super::frame::clip(&row, cols).to_string())
            .collect()
    }

    /// How many rows the band has to give the panel.
    ///
    /// The length of [`Self::rows`], so the count the band solved its geometry
    /// from cannot drift from the paint.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
        // The panel is at most a title, seven choices and a draft row, so the
        // narrowing is a proof rather than a policy.
        u16::try_from(self.rows(cols, terminal_rows).len()).unwrap_or(u16::MAX)
    }

    /// Whether this screen can put the question in front of the user at all.
    ///
    /// A `false` is **not** a smaller panel: it is a screen xfx cannot ask on,
    /// and the shell cancels the batch on the user's behalf rather than painting
    /// choices below the last row (`super::shell::Shell::ask_question`).
    pub(crate) fn presents_choices(&self, cols: u16, terminal_rows: u16) -> bool {
        self.shape(cols, terminal_rows).fits
    }

    /// Which row of the panel the marked choice is on.
    pub(crate) fn caret_row(&self, cols: u16, terminal_rows: u16) -> u16 {
        let Some(entry) = self.entry() else {
            return 0;
        };
        let shape = self.shape(cols, terminal_rows);
        let top = window(
            entry.top,
            entry.selected,
            shape.visible,
            entry.options.len(),
        );
        let offset = entry.selected.saturating_sub(top);
        // Inside the block whatever the arithmetic above was handed: a caret
        // reported below the panel's last row would be a caret on the divider.
        u16::try_from(1 + offset.min(shape.visible.saturating_sub(1))).unwrap_or(1)
    }

    /// Where the caret goes **while the freeform slot is open**: the draft's
    /// row within the panel, and the cells to the left of it there.
    ///
    /// `None` the rest of the time, which is what says the caret belongs on the
    /// marked choice instead ([`Self::caret_row`]). Two answers rather than one,
    /// because they are two different claims about what the next keystroke does:
    /// a caret in a draft says typing goes into it, and a caret on a choice says
    /// a digit takes one.
    ///
    /// **Takes `terminal_rows` as well as `cols`**, which the plan's signature
    /// did not: the draft is the panel's last row, and how many rows the panel
    /// has is a function of the screen's height ([`Self::shape`]). A caret
    /// derived from the width alone would be reported on a choice row on every
    /// screen short enough to have shrunk the window.
    pub(crate) fn caret(&self, cols: u16, terminal_rows: u16) -> Option<(u16, u16)> {
        let entry = self.entry()?;
        let shape = self.shape(cols, terminal_rows);
        if !shape.drafting {
            return None;
        }
        let row = self.height(cols, terminal_rows).saturating_sub(1);
        let column = INDENT_CELLS
            .saturating_add(entry.draft.point(draft_cols(cols)).1)
            // On the screen, always: a column past the last one is a cursor the
            // terminal clamps silently onto a row it was not placed on.
            .min(cols.saturating_sub(1));
        Some((row, column))
    }

    /// What one keystroke does.
    pub(crate) fn apply(&mut self, act: Act, cols: u16, terminal_rows: u16) -> Answered {
        // The window this keystroke is answered against, taken before anything
        // moves: an ordinal is about the block the user is looking at.
        let shape = self.shape(cols, terminal_rows);
        if self.entries.get(self.current).is_none() {
            // A batch with no questions cannot be answered, and inventing an
            // empty submission would hand the model an answer document nobody
            // wrote. It is refused instead.
            return Answered::Cancelled;
        }
        match act {
            // Both cancel the **batch**. Which key it was is the shell's to
            // tell apart: Ctrl-C stops the turn as well.
            Act::Escape | Act::Cancel => Answered::Cancelled,
            Act::Tab => {
                self.current = (self.current + 1) % self.entries.len();
                Answered::Redraw
            }
            Act::Up | Act::Down => {
                let count = self.entry_mut().options.len();
                let entry = self.entry_mut();
                entry.selected = if matches!(act, Act::Up) {
                    (entry.selected + count - 1) % count
                } else {
                    (entry.selected + 1) % count
                };
                entry.top = window(entry.top, entry.selected, shape.visible, count);
                Answered::Redraw
            }
            Act::Text(character) => self.typed(character, shape),
            Act::Submit => {
                let taken = self.entry_mut().choice();
                self.record(taken)
            }
            Act::Backspace | Act::Left | Act::Right | Act::Home | Act::End => {
                if !self.entry_mut().drafting() {
                    // A key that means something only to a draft, with no draft
                    // open. Swallowed rather than passed on -- the question has
                    // the focus -- and it changed nothing, so it owes no frame.
                    return Answered::Nothing;
                }
                let action = match act {
                    Act::Backspace => Action::Backspace,
                    Act::Left => Action::Left,
                    Act::Right => Action::Right,
                    Act::Home => Action::Home,
                    _ => Action::End,
                };
                let width = draft_cols(cols);
                self.entry_mut().draft.apply(action, width);
                Answered::Redraw
            }
        }
    }

    /// The question that is shown, for the paths that have already established
    /// there is one.
    ///
    /// Every caller is inside [`Self::apply`], past its own guard, so the
    /// fallback below is unreachable rather than a policy -- and it is a panic
    /// nobody can trigger from a keystroke, which is what a UI thread holding a
    /// raw terminal is owed.
    fn entry_mut(&mut self) -> &mut Entry {
        let current = self.current.min(self.entries.len().saturating_sub(1));
        self.entries
            .get_mut(current)
            .expect("apply refuses a batch with no questions before it reaches here")
    }

    /// One typed character: a choice, an editing keystroke, or nothing.
    fn typed(&mut self, character: char, shape: Shape) -> Answered {
        if self.entry_mut().drafting() {
            return self.insert(character);
        }
        let Some(ordinal) = character.to_digit(10).filter(|digit| *digit > 0) else {
            // Every other character. A question that has the focus swallows
            // them rather than letting them fall into a composer whose caret is
            // somewhere else.
            return Answered::Nothing;
        };
        let index = usize::try_from(ordinal - 1).unwrap_or(usize::MAX);
        let entry = self.entry_mut();
        let count = entry.options.len();
        if index >= count {
            return Answered::Nothing;
        }
        // **Only what the window is showing.** A digit is a shorthand for a row
        // on the screen, so a choice the user cannot read cannot be taken by
        // number (`question_prompt.zig:322`).
        let top = window(entry.top, entry.selected, shape.visible, count);
        if index < top || index >= top + shape.visible {
            return Answered::Nothing;
        }
        entry.selected = index;
        entry.top = window(entry.top, index, shape.visible, count);
        if entry.drafting() {
            // The freeform ordinal **opens the editor** rather than answering
            // (`question_prompt.zig:392`): there is nothing to answer with yet.
            return Answered::Redraw;
        }
        let taken = entry.choice();
        self.record(taken)
    }

    /// One character into the current draft, or a silent refusal.
    fn insert(&mut self, character: char) -> Answered {
        let mut encoded = [0u8; 4];
        let typed = character.encode_utf8(&mut encoded);
        let entry = self.entry_mut();
        // **Measured after encoding**, because that is the bound the executor
        // re-checks on the way out (`crate::tools::question`'s `bounded`): a
        // draft that passed a byte count here and failed the encoded one there
        // would be a refusal the user gets after they have finished typing.
        let held = terminal_safe(entry.draft.text()).len();
        let adding = terminal_safe(typed).len();
        if held.saturating_add(adding) > MAX_FREEFORM_ENCODED_BYTES {
            // Silently, like every other keystroke a budget refuses on this
            // surface (`super::shell::Shell::type_character`).
            return Answered::Nothing;
        }
        match entry.draft.insert(typed) {
            Some(_) => Answered::Redraw,
            None => Answered::Nothing,
        }
    }

    /// Records `answer` for the question that is shown, and moves on.
    ///
    /// The next **unanswered** entry, cycling from this one
    /// (`question_prompt.zig:603-634`), so a batch answered out of order by Tab
    /// still ends the moment nothing is left -- and the answers are handed back
    /// in **entry** order, which is the order the questions were asked in and
    /// the order the tool pairs them up in.
    fn record(&mut self, answer: String) -> Answered {
        self.entry_mut().answer = Some(answer);
        let count = self.entries.len();
        for step in 1..=count {
            let index = (self.current + step) % count;
            if self.entries[index].answer.is_none() {
                self.current = index;
                return Answered::Redraw;
            }
        }
        Answered::Submitted(
            self.entries
                .iter()
                .map(|entry| entry.answer.clone().unwrap_or_default())
                .collect(),
        )
    }

    /// The current question's freeform text, and `None` when the slot is not
    /// marked.
    #[cfg(test)]
    fn draft(&self) -> Option<&str> {
        let entry = self.entry()?;
        entry.drafting().then(|| entry.draft.text())
    }
}

/// How wide the draft's text is: the screen without the indent it is written
/// into.
fn draft_cols(cols: u16) -> u16 {
    cols.saturating_sub(INDENT_CELLS).max(1)
}

/// The first choice a `visible`-row window shows, given where the mark is.
///
/// **Derived at every reading rather than remembered**, which is what makes a
/// resize under a standing question correct without a keystroke: the window is
/// a function of the mark and of how many rows the screen left, so a screen that
/// shrank re-derives one the mark is inside of. Total on purpose -- every
/// subtraction is saturating -- because the inputs include a `0x0` terminal.
fn window(top: usize, selected: usize, visible: usize, count: usize) -> usize {
    let visible = visible.max(1);
    let mut top = top.min(count.saturating_sub(visible));
    if selected < top {
        top = selected;
    } else if selected >= top.saturating_add(visible) {
        top = selected.saturating_add(1).saturating_sub(visible);
    }
    top
}

// ---------------------------------------------------------------------------
// the way the tools layer asks this surface
// ---------------------------------------------------------------------------

/// Asks the batch through the band, from the runtime thread, and waits.
///
/// The TUI's implementation of [`QuestionRequester`], and the only place the
/// tools layer's seam meets this surface. It shares **the** control channel with
/// [`super::approval::TuiPrompter`] rather than opening one of its own: a second
/// receiver on that channel would take messages the turn loop is owed, and a
/// second runtime would be a second thread the terminal is not owned by
/// (`super::approval::ControlChannel` documents why one receiver read from two
/// places is sound).
///
/// Cloning shares the identity counter as well as the channel, because it is
/// identity: two clones minting the same id would make a stale answer
/// indistinguishable from a fresh one, which is the one thing the id exists to
/// prevent.
#[derive(Clone)]
pub(crate) struct TuiQuestioner {
    events: Sender<UiEvent>,
    control: Arc<ControlChannel>,
    /// The **session's** cancellation, not a turn's, for the reason
    /// [`super::approval::TuiPrompter`]'s is: this requester is built once per
    /// session and outlives every turn's token, so a turn's token captured here
    /// would be cancelled by the time the second turn asked anything.
    cancel: Cancellation,
    /// The next id to mint, shared across clones.
    next: Arc<AtomicU64>,
}

impl TuiQuestioner {
    pub(crate) fn new(
        events: Sender<UiEvent>,
        control: Arc<ControlChannel>,
        cancel: Cancellation,
    ) -> Self {
        Self {
            events,
            control,
            cancel,
            // One rather than nought so that the first batch is `QuestionId(1)`
            // and a `QuestionId(0)` in a log is a value nothing minted.
            next: Arc::new(AtomicU64::new(1)),
        }
    }

    /// The id this batch is asked under, or nothing when one cannot be minted.
    ///
    /// **Fails closed.** `fetch_add` wraps, and a wrapped counter would hand out
    /// an id some earlier batch was asked under -- at which point a keystroke
    /// left over from that batch is accepted as an answer to this one. A
    /// question that cannot be given an identity is not asked at all.
    fn mint(&self) -> Option<QuestionId> {
        self.next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                held.checked_add(1)
            })
            .ok()
            .map(QuestionId)
    }
}

impl QuestionRequester for TuiQuestioner {
    /// Shows the batch and waits for the band's answer, on the runtime thread.
    ///
    /// **Every exit rearms the turn loop's waker.**
    /// [`ControlChannel::answered`] consumes it when it polls ready, so a return
    /// that did not put it back would leave the loop parked on a channel it is
    /// no longer registered on -- and the next control message, the Ctrl-C that
    /// would end the turn included, would wake nobody.
    fn request(&self, entries: &[QuestionEntry]) -> Option<Vec<String>> {
        let id = self.mint()?;
        let token = self.cancel.token();
        let request = QuestionRequest {
            id,
            entries: entries.to_vec(),
        };
        if park_on(send_ui(&self.events, &token, UiEvent::Question(request))).is_err() {
            // No UI to ask, or no session to ask in. The honest result is the
            // cancellation sentinel, which the turn can still carry
            // (`crate::tools::question::CANCEL_SENTINEL`).
            return None;
        }
        park_on(async {
            loop {
                let message = tokio::select! {
                    biased;
                    // The session's root, not a turn's. The UI's control sender
                    // outlives a cancelled session, so the receiver never closes
                    // and waiting on `answered()` alone would never return.
                    () = token.cancelled() => {
                        self.control.rearm();
                        return None;
                    }
                    message = self.control.answered() => message,
                };
                match message {
                    Some(TurnControl::QuestionAnswer { id: seen, answers }) if seen == id => {
                        return Some(answers)
                    }
                    Some(TurnControl::QuestionCancelled { id: seen }) if seen == id => return None,
                    // A keystroke on a panel that has already gone, or an
                    // approval answer nobody is waiting for: consumed, so the
                    // next question does not inherit it -- the turn loop's rule
                    // (`super::worker`'s `raced_against_control`).
                    Some(
                        TurnControl::QuestionAnswer { .. }
                        | TurnControl::QuestionCancelled { .. }
                        | TurnControl::Answer(_),
                    ) => continue,
                    // The interrupt and the shutdown belong to the loop that can
                    // act on them (`super::approval::ControlChannel::put_back`);
                    // `give_back` rearms.
                    Some(stop) => {
                        self.control.give_back(stop);
                        return None;
                    }
                    // The UI dropped its sender. `answered` rearmed on the ready
                    // poll; rearming again is spurious and harmless, and keeps
                    // "every exit rearms" true without a reader having to check.
                    None => {
                        self.control.rearm();
                        return None;
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A batch of two questions, two choices each, named so an answer document
    /// is predictable. The first choice carries a description, because a
    /// description is the one part of a choice that is optional.
    fn two() -> QuestionPanel {
        QuestionPanel::new(QuestionRequest {
            id: QuestionId(1),
            entries: vec![
                QuestionEntry {
                    question: "Which depth?".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "Thorough".to_string(),
                            description: Some("reads every file".to_string()),
                        },
                        QuestionOption {
                            label: "Quick".to_string(),
                            description: None,
                        },
                    ],
                },
                QuestionEntry {
                    question: "Ship it?".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "Yes".to_string(),
                            description: None,
                        },
                        QuestionOption {
                            label: "No".to_string(),
                            description: None,
                        },
                    ],
                },
            ],
        })
    }

    /// One question with six choices, which becomes seven with the freeform
    /// slot -- the largest batch the tool admits, and the one a short screen
    /// cannot show whole.
    fn six() -> QuestionPanel {
        QuestionPanel::new(QuestionRequest {
            id: QuestionId(2),
            entries: vec![QuestionEntry {
                question: "Which file?".to_string(),
                options: (1..=6)
                    .map(|index| QuestionOption {
                        label: format!("file{index}.rs"),
                        description: None,
                    })
                    .collect(),
            }],
        })
    }

    /// One question with `text` as its question and two plain choices.
    fn one_entry(text: &str) -> QuestionRequest {
        QuestionRequest {
            id: QuestionId(1),
            entries: vec![QuestionEntry {
                question: text.to_string(),
                options: vec![
                    QuestionOption {
                        label: "Yes".to_string(),
                        description: None,
                    },
                    QuestionOption {
                        label: "No".to_string(),
                        description: None,
                    },
                ],
            }],
        }
    }

    /// The answers `acts` produce, or a failure naming what came out instead.
    fn submitted(panel: &mut QuestionPanel, acts: &[Act], cols: u16, rows: u16) -> Vec<String> {
        let mut last = Answered::Nothing;
        for act in acts {
            last = panel.apply(*act, cols, rows);
        }
        match last {
            Answered::Submitted(answers) => answers,
            other => panic!("expected a submitted batch, got {other:?}"),
        }
    }

    #[test]
    fn one_question_is_visible_at_a_time_with_ordinals_and_a_freeform_slot() {
        let text = two().rows(60, 40).join("\n");
        for expected in [
            "Which depth?",
            "1. Thorough",
            "2. Quick",
            "3. Other",
            "reads every file",
            "1 of 2",
        ] {
            assert!(
                text.contains(expected),
                "the panel does not show {expected:?}:\n{text}"
            );
        }
        assert!(
            !text.contains("Ship it?"),
            "the second question is not shown yet"
        );
    }

    #[test]
    fn a_model_ordinal_submits_at_once() {
        // A fresh panel: no freeform slot is selected, so a digit is a choice.
        let mut panel = two();
        assert!(matches!(
            panel.apply(Act::Text('1'), 60, 40),
            Answered::Redraw
        ));
        assert!(
            panel.rows(60, 40).join("\n").contains("2 of 2"),
            "the batch advanced"
        );
        assert_eq!(
            submitted(&mut panel, &[Act::Text('2')], 60, 40),
            vec!["Thorough".to_string(), "No".to_string()],
            "the last answer submits the batch"
        );
    }

    #[test]
    fn the_freeform_ordinal_only_opens_editing_and_later_digits_are_text() {
        // Its own panel, because once `Other` is selected a digit is *typing*
        // (`question_prompt.zig:322` -- `insert_ascii`), not a second choice. A
        // case that selected `Other` and then pressed `1` would be asserting
        // about a draft, not about an ordinal.
        let mut panel = two();
        assert!(
            matches!(panel.apply(Act::Text('3'), 60, 40), Answered::Redraw),
            "`Other` opens the editor rather than answering (question_prompt.zig:392)"
        );
        assert!(
            panel.rows(60, 40).join("\n").contains("1 of 2"),
            "and nothing was answered"
        );
        panel.apply(Act::Text('1'), 60, 40);
        assert_eq!(
            panel.draft(),
            Some("1"),
            "the digit was typed into the draft"
        );
        // Leaving the slot restores the ordinal meaning.
        panel.apply(Act::Up, 60, 40);
        assert_eq!(panel.draft(), None);
        assert!(matches!(
            panel.apply(Act::Text('1'), 60, 40),
            Answered::Redraw
        ));
        assert_eq!(
            submitted(&mut panel, &[Act::Text('1')], 60, 40)[0],
            "Thorough"
        );
    }

    #[test]
    fn arrows_move_and_enter_takes_the_marked_choice() {
        let mut panel = two();
        panel.apply(Act::Down, 60, 40);
        assert!(matches!(panel.apply(Act::Submit, 60, 40), Answered::Redraw));
        assert_eq!(submitted(&mut panel, &[Act::Text('1')], 60, 40)[0], "Quick");
    }

    #[test]
    fn other_takes_unicode_text_and_an_empty_draft_is_still_an_answer() {
        let mut typed: Vec<Act> = vec![Act::Text('3')];
        typed.extend("네—ok".chars().map(Act::Text));
        typed.extend([Act::Submit, Act::Text('1')]);
        assert_eq!(submitted(&mut two(), &typed, 60, 40)[0], "네—ok");
        assert_eq!(
            submitted(
                &mut two(),
                &[Act::Text('3'), Act::Submit, Act::Text('1')],
                60,
                40
            )[0],
            "",
            "upstream accepts an empty freeform"
        );
    }

    #[test]
    fn a_draft_at_the_cap_refuses_the_next_character_silently() {
        let mut panel = two();
        panel.apply(Act::Text('3'), 60, 40);
        for _ in 0..MAX_FREEFORM_ENCODED_BYTES {
            panel.apply(Act::Text('x'), 60, 40);
        }
        assert!(matches!(
            panel.apply(Act::Text('x'), 60, 40),
            Answered::Nothing
        ));
        assert_eq!(
            submitted(&mut panel, &[Act::Submit, Act::Text('1')], 60, 40)[0].len(),
            MAX_FREEFORM_ENCODED_BYTES
        );
    }

    #[test]
    fn a_draft_is_bounded_by_what_it_encodes_to_rather_than_by_what_it_holds() {
        // The other side of the same cap, and the reason it is measured after
        // encoding: a character the encoder expands is worth its **encoded**
        // bytes, because that is what the executor re-checks and what the model
        // reads back. A panel counting raw bytes would accept six times the
        // budget of zero-width spaces and have the tool refuse the whole answer.
        let mut panel = two();
        panel.apply(Act::Text('3'), 60, 40);
        let zero_width = '\u{200b}'; // encodes to `\u{200b}`: eight bytes
        let encoded = terminal_safe("\u{200b}").len();
        assert_eq!(encoded, 8, "the encoder no longer expands this character");
        for _ in 0..MAX_FREEFORM_ENCODED_BYTES / encoded {
            panel.apply(Act::Text(zero_width), 60, 40);
        }
        assert!(
            matches!(
                panel.apply(Act::Text(zero_width), 60, 40),
                Answered::Nothing
            ),
            "the draft took more than the encoded budget"
        );
        let answer = submitted(&mut panel, &[Act::Submit, Act::Text('1')], 60, 40)[0].clone();
        assert!(
            terminal_safe(&answer).len() <= MAX_FREEFORM_ENCODED_BYTES,
            "the answer encodes to {} bytes",
            terminal_safe(&answer).len()
        );
    }

    #[test]
    fn escape_cancels_the_whole_batch_not_the_current_question() {
        let mut panel = two();
        panel.apply(Act::Text('1'), 60, 40);
        assert!(matches!(
            panel.apply(Act::Escape, 60, 40),
            Answered::Cancelled
        ));
    }

    #[test]
    fn ctrl_c_cancels_the_batch_the_same_way_escape_does() {
        // The panel does not tell the two apart -- one tool call, one refusal.
        // What Ctrl-C additionally means on this surface (stop the turn) is the
        // shell's, and it is asserted there.
        let mut panel = two();
        assert!(matches!(
            panel.apply(Act::Cancel, 60, 40),
            Answered::Cancelled
        ));
    }

    #[test]
    fn tab_cycles_between_questions_and_keeps_each_draft() {
        let mut panel = two();
        panel.apply(Act::Text('3'), 60, 40);
        // A character no label, question or ordinal contains, asserted against
        // the draft itself: `contains('a')` would pass on the standing
        // `Thorough` row and prove nothing about whether the draft survived.
        panel.apply(Act::Text('§'), 60, 40);
        assert_eq!(panel.draft(), Some("§"));
        panel.apply(Act::Tab, 60, 40);
        assert!(panel.rows(60, 40).join("\n").contains("Ship it?"));
        assert_eq!(
            panel.draft(),
            None,
            "the second question has no freeform slot selected"
        );
        panel.apply(Act::Tab, 60, 40);
        assert_eq!(panel.draft(), Some("§"), "the draft survived the cycle");
    }

    #[test]
    fn an_ordinal_for_a_choice_the_window_is_not_showing_is_refused() {
        let mut panel = six();
        assert!(
            !panel.rows(60, 12).join("\n").contains("7. Other"),
            "the window is bounded"
        );
        assert!(
            matches!(panel.apply(Act::Text('7'), 60, 12), Answered::Nothing),
            "a choice the user cannot see cannot be taken by number"
        );
    }

    #[test]
    fn the_window_follows_the_selection_and_a_tiny_screen_is_refused() {
        let mut panel = six();
        for _ in 0..6 {
            panel.apply(Act::Down, 60, 12);
        }
        assert!(
            panel.rows(60, 12).join("\n").contains("7. Other"),
            "the marked choice is visible"
        );
        assert!(
            !panel.presents_choices(20, 4),
            "a four-row screen cannot hold a question"
        );
        assert!(
            panel.rows(20, 4).len() as u16 <= panel.height(20, 4),
            "rows never exceed the height"
        );
    }

    #[test]
    fn a_screen_that_shrank_under_the_question_re_derives_its_window() {
        // No keystroke happens on a resize, so a window remembered from the
        // last one would leave the marked choice off the block -- and the user
        // pressing Enter on a choice they cannot see.
        let mut panel = six();
        for _ in 0..6 {
            panel.apply(Act::Down, 60, 40);
        }
        assert!(panel.rows(60, 40).join("\n").contains("7. Other"));
        let shrunk = panel.rows(60, 12).join("\n");
        assert!(
            shrunk.contains("7. Other"),
            "the marked choice left the block on a screen that shrank: {shrunk}"
        );
        assert!(
            !shrunk.contains("1. file1.rs"),
            "the window did not move at all: {shrunk}"
        );
        // And the caret is still inside the block it is painted from.
        assert!(panel.caret_row(60, 12) < panel.height(60, 12));
    }

    #[test]
    fn a_long_question_is_clipped_for_the_screen_and_not_in_the_answer() {
        let mut panel = QuestionPanel::new(one_entry(&"L".repeat(400)));
        assert_eq!(panel.id(), QuestionId(1));
        assert!(panel
            .rows(40, 40)
            .iter()
            .all(|row| row.chars().count() <= 40));
        assert_eq!(
            submitted(&mut panel, &[Act::Text('1')], 40, 40)[0],
            "Yes",
            "the answer is the label, whole"
        );
    }

    #[test]
    fn a_label_too_wide_for_the_screen_is_still_the_answer_whole() {
        // The clip is a *screen* cut. The answer is the canonical label, and the
        // model reads that string back beside the question it belongs to -- a
        // panel that answered with what it had room to paint would change what
        // the user chose into something they never saw.
        let label = "keep the ".repeat(20);
        let mut panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(3),
            entries: vec![QuestionEntry {
                question: "Which?".to_string(),
                options: vec![
                    QuestionOption {
                        label: label.clone(),
                        description: None,
                    },
                    QuestionOption {
                        label: "no".to_string(),
                        description: None,
                    },
                ],
            }],
        });
        assert!(panel
            .rows(30, 40)
            .iter()
            .all(|row| row.chars().count() <= 30));
        assert_eq!(submitted(&mut panel, &[Act::Text('1')], 30, 40)[0], label);
    }

    #[test]
    fn the_caret_is_in_the_draft_only_while_the_freeform_slot_is_open() {
        // Where the caret is *is* what the terminal says the focus is: on a
        // choice it says a digit takes one, and in the draft it says a digit is
        // a character.
        let mut panel = two();
        assert_eq!(panel.caret(60, 40), None, "the caret is on a choice");
        assert_eq!(panel.caret_row(60, 40), 1, "the first choice's row");
        panel.apply(Act::Down, 60, 40);
        assert_eq!(panel.caret_row(60, 40), 2);

        panel.apply(Act::Text('3'), 60, 40);
        let (row, column) = panel.caret(60, 40).expect("the draft has the caret");
        assert_eq!(
            row,
            panel.height(60, 40) - 1,
            "the draft is the panel's last row"
        );
        assert_eq!(
            column, INDENT_CELLS,
            "an empty draft's caret is at its start"
        );

        // **Cells, not bytes.** A three-byte glyph two cells wide moves the
        // caret two columns; a caret counted in bytes would be three columns to
        // the right of the character it is supposed to be after.
        panel.apply(Act::Text('네'), 60, 40);
        let wide = super::super::wrap::width("네");
        assert_eq!(wide, 2, "the width of this glyph changed");
        assert_eq!(
            panel.caret(60, 40).expect("drafting").1,
            INDENT_CELLS + wide
        );
        panel.apply(Act::Backspace, 60, 40);
        assert_eq!(panel.caret(60, 40).expect("drafting").1, INDENT_CELLS);
        assert_eq!(panel.draft(), Some(""), "the whole glyph went");
    }

    #[test]
    fn the_drafts_own_keys_reach_it_and_do_nothing_at_a_choice() {
        let mut panel = two();
        // No draft is open, so these are keystrokes the question swallows.
        for act in [Act::Backspace, Act::Left, Act::Right, Act::Home, Act::End] {
            assert!(
                matches!(panel.apply(act, 60, 40), Answered::Nothing),
                "{act:?} did something at a choice"
            );
        }
        panel.apply(Act::Text('3'), 60, 40);
        for character in "abc".chars() {
            panel.apply(Act::Text(character), 60, 40);
        }
        panel.apply(Act::Home, 60, 40);
        panel.apply(Act::Text('z'), 60, 40);
        assert_eq!(panel.draft(), Some("zabc"), "Home did not reach the draft");
        panel.apply(Act::End, 60, 40);
        panel.apply(Act::Backspace, 60, 40);
        assert_eq!(panel.draft(), Some("zab"));
        panel.apply(Act::Left, 60, 40);
        panel.apply(Act::Backspace, 60, 40);
        assert_eq!(panel.draft(), Some("zb"), "Left did not reach the draft");
        assert_eq!(
            submitted(&mut panel, &[Act::Submit, Act::Text('1')], 60, 40)[0],
            "zb"
        );
    }

    #[test]
    fn a_draft_longer_than_the_row_keeps_the_caret_on_the_screen() {
        // The draft gets **one** row of the band, so a sentence longer than the
        // screen is windowed: what is shown is the part the caret is in, and the
        // caret is reported inside the screen rather than off the right of it.
        let mut panel = two();
        panel.apply(Act::Text('3'), 60, 40);
        for _ in 0..500 {
            panel.apply(Act::Text('x'), 60, 40);
        }
        let rows = panel.rows(30, 40);
        assert!(
            rows.iter().all(|row| row.chars().count() <= 30),
            "a row outran the screen: {rows:?}"
        );
        let (row, column) = panel.caret(30, 40).expect("drafting");
        assert!(
            column < 30,
            "the caret is off the screen at column {column}"
        );
        assert_eq!(row, panel.height(30, 40) - 1);
        // And the answer is every character that was typed, not the row.
        assert_eq!(
            submitted(&mut panel, &[Act::Submit, Act::Text('1')], 30, 40)[0].len(),
            500
        );
    }

    #[test]
    fn a_screen_the_terminal_will_not_describe_is_refused_rather_than_panicked_on() {
        // `0x0` is what a pty whose size was never set answers with
        // (`super::term::window_size`), and every number below is derived by
        // subtraction from the screen's.
        let mut panel = two();
        for (cols, rows) in [(0, 0), (1, 1), (20, 6), (0, 40)] {
            let painted = panel.rows(cols, rows);
            assert_eq!(
                painted.len(),
                usize::from(panel.height(cols, rows)),
                "the rows and the height disagree at {cols}x{rows}"
            );
            for row in &painted {
                assert!(
                    super::super::wrap::width(row) <= cols,
                    "a row outran a {cols}-column screen: {row:?}"
                );
            }
            assert!(panel.caret_row(cols, rows) <= panel.height(cols, rows));
        }
        assert!(!panel.presents_choices(0, 0));
        assert!(matches!(
            panel.apply(Act::Text('1'), 0, 0),
            Answered::Nothing | Answered::Redraw
        ));
    }

    #[test]
    fn a_batch_with_no_questions_presents_nothing_and_answers_nothing() {
        // Not a shape the tool can produce (`parse` admits 1 to 4), and this
        // type is constructible from anywhere in the crate -- so it is a refusal
        // rather than an index out of bounds on the thread holding the terminal.
        let mut panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(9),
            entries: Vec::new(),
        });
        assert!(panel.rows(60, 40).is_empty());
        assert_eq!(panel.height(60, 40), 0);
        assert!(!panel.presents_choices(60, 40));
        assert!(matches!(
            panel.apply(Act::Submit, 60, 40),
            Answered::Cancelled
        ));
    }

    #[test]
    fn the_freeform_slot_is_the_last_index_rather_than_a_label() {
        // A model that calls one of its own choices `Other` must not thereby
        // acquire a text editor at that ordinal: the slot is `options.len() - 1`
        // and nothing else (`ask_user_question.zig` appends it once).
        let mut panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(4),
            entries: vec![QuestionEntry {
                question: "Which?".to_string(),
                options: vec![
                    QuestionOption {
                        label: FREEFORM_LABEL.to_string(),
                        description: None,
                    },
                    QuestionOption {
                        label: "second".to_string(),
                        description: None,
                    },
                ],
            }],
        });
        // `1` is the model's own `Other`, and it answers like any other label.
        assert_eq!(
            submitted(&mut panel, &[Act::Text('1')], 60, 40)[0],
            FREEFORM_LABEL,
            "the model's own label opened an editor instead of answering"
        );
        // `3` is the synthetic slot, and it is the one that opens the editor.
        let mut panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(4),
            entries: vec![QuestionEntry {
                question: "Which?".to_string(),
                options: vec![
                    QuestionOption {
                        label: FREEFORM_LABEL.to_string(),
                        description: None,
                    },
                    QuestionOption {
                        label: "second".to_string(),
                        description: None,
                    },
                ],
            }],
        });
        panel.apply(Act::Text('3'), 60, 40);
        assert_eq!(panel.draft(), Some(""));
    }

    #[test]
    fn the_answers_come_back_in_entry_order_however_the_batch_was_walked() {
        // Tab makes the walk arbitrary; the document the tool builds pairs
        // answer *n* with question *n* (`crate::tools::question::encode_answers`),
        // so the order they are collected in is the entries' and not the
        // keystrokes'.
        let mut panel = two();
        panel.apply(Act::Tab, 60, 40); // the second question first
        assert!(panel.rows(60, 40).join("\n").contains("Ship it?"));
        assert!(matches!(
            panel.apply(Act::Text('2'), 60, 40),
            Answered::Redraw
        ));
        assert_eq!(
            submitted(&mut panel, &[Act::Text('1')], 60, 40),
            vec!["Thorough".to_string(), "No".to_string()],
            "the answers came back in the order they were typed"
        );
    }

    // -----------------------------------------------------------------------
    // the requester the runtime thread asks through
    // -----------------------------------------------------------------------

    use crate::gateway::CancelToken;
    use crate::permission::ApprovalAnswer;
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use std::task::Wake;
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::sync::mpsc::{Receiver, UnboundedSender};

    /// How long anything in these tests waits before it fails instead.
    ///
    /// Every wait here is bounded and every bound ends the **session**, which is
    /// the one exit this requester always has: a test that would otherwise park
    /// for ever fails on its own assertion rather than hanging the suite. Long
    /// enough that a loaded machine does not fail a healthy requester, which
    /// finishes in microseconds.
    const DEADLINE: Duration = Duration::from_secs(5);

    /// One well-formed batch, as the tools layer hands it over: already encoded,
    /// one question, two choices.
    fn entries() -> Vec<QuestionEntry> {
        vec![QuestionEntry {
            question: "Which depth?".to_string(),
            options: vec![
                QuestionOption {
                    label: "Thorough".to_string(),
                    description: None,
                },
                QuestionOption {
                    label: "Quick".to_string(),
                    description: None,
                },
            ],
        }]
    }

    /// A requester with both of its channels held open by the test, which is the
    /// shape the worker thread builds: one `UiEvent` sender, one control
    /// channel, one session cancellation.
    struct Harness {
        questioner: TuiQuestioner,
        /// Kept so a test can occupy the UI channel's permits before the
        /// request.
        events_tx: Sender<UiEvent>,
        events_rx: Receiver<UiEvent>,
        control_tx: UnboundedSender<TurnControl>,
        control: Arc<ControlChannel>,
        cancel: Cancellation,
    }

    impl Harness {
        fn new(ui_capacity: usize) -> Self {
            let (events_tx, events_rx) = tokio::sync::mpsc::channel(ui_capacity);
            let (control_tx, control_rx) = tokio::sync::mpsc::unbounded_channel();
            let control = ControlChannel::new(control_rx);
            // `Cancellation::new` takes the turn-mirror token it resets; it is
            // not a no-argument constructor.
            let cancel = Cancellation::new(CancelToken::new());
            Self {
                questioner: TuiQuestioner::new(
                    events_tx.clone(),
                    Arc::clone(&control),
                    cancel.clone(),
                ),
                events_tx,
                events_rx,
                control_tx,
                control,
                cancel,
            }
        }

        /// Runs one `request` on another thread while the test thread drives the
        /// channels. `park_on` needs no runtime, so this is the shape the worker
        /// thread uses.
        ///
        /// The fields are destructured first **on purpose**: `drive` needs the
        /// event receiver mutably while the spawned closure holds the
        /// questioner, and `&self.questioner` alongside `drive(&mut self)` is
        /// E0502. Split borrows of disjoint fields are not.
        fn request_while(
            &mut self,
            drive: impl FnOnce(&mut Receiver<UiEvent>, &UnboundedSender<TurnControl>, &Cancellation),
        ) -> Option<Vec<String>> {
            let Harness {
                questioner,
                events_rx,
                control_tx,
                cancel,
                ..
            } = self;
            let finished = Arc::new(AtomicBool::new(false));
            std::thread::scope(|scope| {
                let done = Arc::clone(&finished);
                let handle = scope.spawn(move || {
                    let answers = questioner.request(&entries());
                    done.store(true, Ordering::SeqCst);
                    answers
                });
                // **The bound on every wait in this module.** A requester that
                // did not answer -- because the wait it is in has no exit, which
                // is exactly the regression these tests are about -- is ended by
                // cancelling the session under it, so the assertion below fails
                // rather than the test hanging. It watches a flag instead of
                // sleeping the whole deadline, so a healthy test costs nothing.
                let watchdog = cancel.clone();
                let watched = Arc::clone(&finished);
                scope.spawn(move || {
                    let deadline = Instant::now() + DEADLINE;
                    while Instant::now() < deadline {
                        if watched.load(Ordering::SeqCst) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    watchdog.cancel();
                });
                drive(events_rx, control_tx, cancel);
                handle.join().expect("the requester thread")
            })
        }
    }

    /// The next event the UI is given, or a failure that it never arrived.
    ///
    /// `blocking_recv` would be the obvious spelling and is the wrong one: a
    /// requester that never sent the question leaves it waiting for ever, on a
    /// channel whose sender the harness itself is holding open.
    fn shown(events_rx: &mut Receiver<UiEvent>) -> UiEvent {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match events_rx.try_recv() {
                Ok(event) => return event,
                Err(err) => {
                    assert!(
                        err == TryRecvError::Empty && Instant::now() < deadline,
                        "the UI was never given the event this test is about: {err:?}"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    /// A waker that counts, so a test can assert a wake happened rather than
    /// infer it. Built the way `super::super::approval`'s own `inert_waker` is.
    struct Counting(Arc<AtomicUsize>);

    impl Wake for Counting {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_question_that_is_shown_carries_the_id_its_answer_is_matched_against() {
        // The first batch of a session is `QuestionId(1)`: the answer below is
        // accepted because it names that id, which is the whole of what makes a
        // later keystroke discardable.
        let mut harness = Harness::new(4);
        let answers = harness.request_while(|events_rx, control_tx, _cancel| {
            let event = shown(events_rx);
            let UiEvent::Question(request) = event else {
                panic!("the requester sent something other than a question: {event:?}");
            };
            assert_eq!(request.id, QuestionId(1));
            assert_eq!(
                request.entries,
                entries(),
                "the batch was rewritten on the way"
            );
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: request.id,
                    answers: vec!["Thorough".into()],
                })
                .unwrap();
        });
        assert_eq!(answers, Some(vec!["Thorough".to_string()]));
    }

    #[test]
    fn a_stale_answer_or_a_stale_approval_is_discarded_and_the_wait_continues() {
        let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx
                .send(TurnControl::Answer(ApprovalAnswer::Deny))
                .unwrap();
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: QuestionId(999),
                    answers: vec!["stale".into()],
                })
                .unwrap();
            control_tx
                .send(TurnControl::QuestionCancelled {
                    id: QuestionId(999),
                })
                .unwrap();
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: QuestionId(1),
                    answers: vec!["fresh".into()],
                })
                .unwrap();
        });
        assert_eq!(answers, Some(vec!["fresh".to_string()]));
    }

    #[test]
    fn the_panels_own_cancellation_answers_with_no_answer() {
        let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx
                .send(TurnControl::QuestionCancelled { id: QuestionId(1) })
                .unwrap();
        });
        assert_eq!(answers, None, "no answer is the cancellation sentinel");
    }

    #[test]
    fn an_interrupt_and_a_shutdown_cancel_the_question_and_are_handed_back() {
        for stop in [TurnControl::Cancel { through: 3 }, TurnControl::Shutdown] {
            let mut harness = Harness::new(4);
            let expected = format!("{stop:?}");
            let answers = harness.request_while(|events_rx, control_tx, _cancel| {
                shown(events_rx);
                control_tx.send(stop).unwrap();
            });
            assert_eq!(answers, None);
            assert_eq!(
                format!("{:?}", harness.control.waiting().expect("handed back")),
                expected,
                "the stop still belongs to the loop that can act on it"
            );
        }
    }

    #[test]
    fn a_session_cancelled_after_the_question_is_shown_does_not_hang() {
        // The regression this `select!` exists for: the control sender is still
        // alive, so `answered()` alone would never return.
        let answers = Harness::new(4).request_while(|events_rx, control_tx, cancel| {
            shown(events_rx);
            assert!(
                !control_tx.is_closed(),
                "the sender is deliberately still alive"
            );
            cancel.cancel();
        });
        assert_eq!(answers, None);
    }

    #[test]
    fn a_cancelled_question_wakes_the_turn_loop_it_parked() {
        // The loop registers **first** and is Pending: that is the waker the
        // question's wait is about to consume, and the one a missing `rearm`
        // would strand. Queueing a message afterwards and finding `recv` ready
        // would not prove anything -- an unwoken loop's channel still holds its
        // message.
        let mut harness = Harness::new(4);
        let control = Arc::clone(&harness.control); // cloned, so `harness` stays borrowable
        let woken = Arc::new(AtomicUsize::new(0));
        let waker = std::task::Waker::from(Arc::new(Counting(Arc::clone(&woken))));
        let mut context = std::task::Context::from_waker(&waker);
        let mut parked = Box::pin(control.recv());
        assert!(
            parked.as_mut().poll(&mut context).is_pending(),
            "the loop is parked on the channel"
        );
        assert_eq!(woken.load(Ordering::SeqCst), 0);

        let answers = harness.request_while(|events_rx, _control_tx, cancel| {
            shown(events_rx);
            cancel.cancel();
        });
        assert_eq!(answers, None);
        assert_eq!(
            woken.load(Ordering::SeqCst),
            1,
            "the cancelled exit rearmed the loop's waker rather than stranding it"
        );
    }

    #[test]
    fn a_ui_that_is_gone_before_the_question_is_shown_cancels_it() {
        let mut harness = Harness::new(4);
        drop(std::mem::replace(
            &mut harness.events_rx,
            tokio::sync::mpsc::channel(1).1,
        ));
        assert_eq!(harness.questioner.request(&entries()), None);
    }

    #[test]
    fn a_full_ui_channel_makes_the_question_wait_rather_than_be_dropped() {
        let mut harness = Harness::new(1);
        // Occupied **before** the request, so `send_ui` must wait for room. A
        // `blocking_recv` on an empty channel here would deadlock instead.
        harness
            .events_tx
            .blocking_send(UiEvent::Notice("holding the one permit".into()))
            .unwrap();
        let answers = harness.request_while(|events_rx, control_tx, _cancel| {
            std::thread::sleep(Duration::from_millis(20));
            assert!(
                matches!(shown(events_rx), UiEvent::Notice(_)),
                "the occupying event comes out first, freeing the permit"
            );
            assert!(
                matches!(shown(events_rx), UiEvent::Question(_)),
                "and the question arrived once there was room, rather than being dropped"
            );
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: QuestionId(1),
                    answers: vec!["ok".into()],
                })
                .unwrap();
        });
        assert_eq!(answers, Some(vec!["ok".to_string()]));
    }

    #[test]
    fn a_session_cancelled_while_the_question_waits_for_room_ends_the_wait() {
        // The other side of the case above, and the one a receiver that is still
        // alive makes dangerous: the UI holds its receiver but has stopped
        // draining, so the permit never frees and `send` alone would wait for
        // ever. `send_ui` races the session's root, and this is the requester
        // reaching that exit rather than the send's own contract being restated.
        let mut harness = Harness::new(1);
        harness
            .events_tx
            .blocking_send(UiEvent::Notice("holding the one permit".into()))
            .unwrap();
        let answers = harness.request_while(|events_rx, control_tx, cancel| {
            // Neither end goes away: the receiver is held and never read, and
            // the control sender stays open.
            assert!(!events_rx.is_closed(), "the UI kept its receiver");
            assert!(!control_tx.is_closed(), "the UI kept its sender");
            std::thread::sleep(Duration::from_millis(20));
            cancel.cancel();
        });
        assert_eq!(
            answers, None,
            "a question that could not even be shown is a cancelled one"
        );
    }

    #[test]
    fn identity_allocation_fails_closed_rather_than_reusing_a_request_id() {
        let mut harness = Harness::new(4);
        // The field rather than a setter: the tests are the module's own, and a
        // `#[cfg(test)]` setter would be a second way to write a counter whose
        // whole contract is that only `mint` writes it.
        harness.questioner.next.store(u64::MAX, Ordering::Relaxed);
        assert_eq!(
            harness.questioner.request(&entries()),
            None,
            "an id that cannot be minted is a cancelled question, not a reused one"
        );
        assert_eq!(
            harness.events_rx.try_recv(),
            Err(TryRecvError::Empty),
            "a question with no identity was shown to the user anyway"
        );
    }

    #[test]
    fn every_clone_mints_from_the_one_counter() {
        // The worker wraps this in an `Arc` once, but a clone must not be a
        // second identity space: two batches asked under one id is exactly the
        // confusion the id exists to prevent.
        let harness = Harness::new(4);
        let cloned = harness.questioner.clone();
        assert_eq!(harness.questioner.mint(), Some(QuestionId(1)));
        assert_eq!(cloned.mint(), Some(QuestionId(2)));
        assert_eq!(harness.questioner.mint(), Some(QuestionId(3)));
    }
}
