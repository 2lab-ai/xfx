//! The question the model asks inside the band, and the keystrokes that answer
//! it.
//!
//! `ask_user_question` is not an approval and mints no authority
//! ([`crate::tools::question`]): what arrives is a batch of 1 to 20 questions,
//! each with 2 to 6 labelled choices plus a synthetic freeform slot, and what
//! goes back is one answer per question in **entry order** -- or nothing at all
//! (`crate::tools::question::Reply`). So none of
//! `super::approval`'s vocabulary is reused here: no [`ApprovalRequest`], no
//! `ApprovalAnswer`, no `Panel`. What is shared is the *shape* of a band
//! occupant -- `rows`, `painted`, `height`, `hint`, `apply` -- because the band
//! solves its geometry from exactly those and a second shape would be a second
//! thing for `super::shell::Shell::fit` to get wrong.
//!
//! [`ApprovalRequest`]: crate::permission::ApprovalRequest
//!
//! # Upstream's layout (`vercel-labs/fx@c1db919`)
//!
//! The panel is `composeQuestionPanelText` (`src/ui/footer/question_ui.zig:40-66`)
//! row for row: the question in bold two columns in, a blank row, one block per
//! choice -- `    1) label`, with the description in a dim column of its own
//! on a wide screen (`questionDescriptionColumn`, `:401-405`) or on rows of its
//! own under the label on a narrow one -- and a blank row above the bottom rule.
//! No marker glyph: the marked choice is the one in the bold hint grey and every
//! other is dim (`:110-112`). The `Other` slot, marked, is a field: the answer
//! wrapped by cell onto rows under its first character, the caret one reversed
//! cell (`super::freeform`). Batch progress is not in the panel at all but on
//! the hint row, right-aligned, and only for a batch of more than one
//! (`questionInteractionHint`, `src/ui/footer/input_presentation.zig:192-249`).
//! Once the batch is over the document keeps a summary of it -- each question
//! under its ordinal with the answer hung beneath it, or `▪ Cancelled`
//! (`composeQuestionResolutions`, `question_ui.zig:418-519`).
//!
//! # What is canonical and what is only painted
//!
//! The entries arrive **already terminal-safe encoded** (`terminal_safe`, run
//! before the request ever left the tools layer), and they are never rewritten
//! here. A row is wrapped and, on a screen narrower than a choice's own
//! prefix, clipped for the *screen* (`super::frame::clip`), and the answer that
//! goes back is the canonical label or the canonical draft -- whole, whatever
//! the terminal's width. A panel that answered with what it had room to paint
//! would change the meaning of the answer the model reads back.
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
//! * A draft at [`MAX_FREEFORM_ENCODED_BYTES`] refuses the next character --
//!   or the whole paste -- silently, measured **after** encoding, because that
//!   is the bound the executor re-checks and a panel that let the draft past it
//!   would build an answer the tool then refuses whole.
//! * Escape and Ctrl-C both end the **batch**, not the question: there is one
//!   tool call and it is answered once. What that does to the turn -- both stop
//!   it, as upstream's do -- is the shell's ([`Answered::Cancelled`] says only
//!   that the batch is over), and so is the Escape at a draft with text in it,
//!   which upstream arms a clear with rather than cancelling
//!   (`app_input_runtime.zig:3120-3135`; [`QuestionPanel::holds_draft`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc::Sender;

use crate::tools::question::{
    answer_safe, QuestionEntry, QuestionOption, FREEFORM_LABEL, MAX_FREEFORM_ENCODED_BYTES,
};
use crate::tools::{QuestionReply, QuestionRequester};

use super::approval::ControlChannel;
use super::approval_view::safe_rows;
use super::bridge::{park_on, send_ui, Cancellation, TurnControl, UiEvent};
use super::freeform::{Direction, Freeform};
use super::input::Action;
// What the hint row's right side says while an Escape at a freeform draft has
// armed the clear (`input_presentation.zig:530-537`): the composer's own
// words, because it is the composer's own gesture.
use super::shell::ESCAPE_ARMED;
use super::theme::{DocumentRow, Palette, CANCELLED_MARK};
use super::wrap::width;

/// What the question is written into (`question_ui.zig:78`).
const TITLE_INDENT: &str = "  ";

/// What every choice row is written into, the marked one included: selection
/// is said by colour alone, so there is no marker column
/// (`question_freeform_layout.zig:22-24`).
const OPTION_INDENT: &str = "    ";

/// The fewest choices a screen has to be able to show before xfx will ask at
/// all.
///
/// Two, because a question with one visible answer is not a choice; the shell
/// refuses the batch rather than painting half of it
/// ([`QuestionPanel::presents_choices`]).
const MIN_VISIBLE: usize = 2;

/// The hint row while a model choice is marked, widest first
/// (`input_presentation.zig:186-190`): the first that fits is shown when the
/// full row ([`choose_hint`]) does not.
const CHOICE_HINTS: [&str; 3] = [
    "↑↓ options · tab questions · enter answer · esc cancel",
    "tab questions · enter answer · esc cancel",
    "enter answer · esc cancel",
];

/// The hint row while the freeform slot is marked
/// (`input_presentation.zig:212-214`): the plain arrows walk the answer's rows
/// and the shifted ones choose ([`Act::SelectUp`]).
const FREEFORM_HINT: &str =
    "type answer    ↑↓←→ cursor    shift+↑↓ options    tab questions    enter answer    esc cancel";

/// The same, widest first, every one keeping Enter and Escape
/// (`input_presentation.zig:179-184`).
const FREEFORM_HINTS: [&str; 4] = [
    "↑↓ cursor · shift+↑↓ options · tab questions · enter answer · esc cancel",
    "shift+↑↓ options · tab questions · enter answer · esc cancel",
    "tab questions · enter answer · esc cancel",
    "enter answer · esc cancel",
];

/// What the document says about a batch the user refused
/// (`writeCancelledResolution`, `question_ui.zig:513-519`).
const CANCELLED: &str = "Cancelled";

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
    /// The previous choice -- or, while the freeform answer has a row above
    /// the caret's, that row (`input_question_runtime.zig:155-163`).
    Up,
    /// The next choice, or the answer's next row.
    Down,
    /// The previous choice, wherever the caret is: Shift+Up
    /// (`src/ui/input/runtime.zig:180-185`, `.move_choice`).
    SelectUp,
    /// The next one: Shift+Down.
    SelectDown,
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
    /// The draft's caret left -- or, with no draft open, the previous
    /// **question** (`retreatToPrevious`, `input_question_runtime.zig:186-192`).
    Left,
    /// The draft's caret right. Nothing at a choice: Enter is what moves on,
    /// and a key that skipped a question would skip an answer
    /// (`input_question_runtime.zig:193-195`).
    Right,
    /// The start and the end of the whole answer (`question_prompt.zig:455-466`).
    Home,
    End,
    /// The grapheme after the caret (`freeformDeleteNext`).
    Delete,
    /// The answer's word moves and kill keys, as upstream's field binds them
    /// (`question_prompt.zig:361-378`): word left and right, `C-w`, `C-u`,
    /// `C-k`. Word left here is Ctrl-Left, Alt-Left and Shift-Alt-b, and word
    /// right Ctrl-Right, Alt-f and Shift-Alt-f: to upstream all but Alt-f are a
    /// composer move and Alt-f is `.word_right`, and at a choice none of them
    /// does anything (`input_question_runtime.zig:181-200@481104fd`).
    WordLeft,
    /// Alt-b: the draft's word left, as [`Act::WordLeft`] -- or, with no
    /// draft open, the previous **question**, as [`Act::Left`]. Upstream routes
    /// its `.word_left`, which only Alt-b decodes to, with Left to
    /// `retreatToPrevious` (`input_question_runtime.zig:190-195@481104fd`;
    /// [`super::input::Action::AltB`]).
    AltB,
    WordRight,
    DeleteWordLeft,
    KillToStart,
    KillToEnd,
    /// A bracketed paste begins, ends, and the bytes between: into the
    /// answer while it is open, and dropped while a choice is marked
    /// (`paste_framing.zig:111-135`, `input_paste_runtime.zig:226-283`).
    PasteStart,
    PasteEnd,
    PasteByte(u8),
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
            Action::SelectUp => Some(Self::SelectUp),
            Action::SelectDown => Some(Self::SelectDown),
            Action::Delete => Some(Self::Delete),
            Action::WordLeft => Some(Self::WordLeft),
            Action::AltB => Some(Self::AltB),
            Action::WordRight => Some(Self::WordRight),
            // `C-w`, and Alt-Backspace, which the decoder reads as a deletion
            // by word characters: this field has the whitespace one only.
            Action::DeleteWordLeft | Action::DeleteWordBackward => Some(Self::DeleteWordLeft),
            Action::KillToStart => Some(Self::KillToStart),
            // `C-k` is its own action to the decoder -- a menu's Up -- and the
            // kill to the line's end here (`shortcuts.zig:18`,
            // `fromFocusedEditorControlByte`, `:31-41`).
            Action::KillToEnd | Action::ControlK => Some(Self::KillToEnd),
            Action::PasteStart => Some(Self::PasteStart),
            Action::PasteEnd => Some(Self::PasteEnd),
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
    /// The freeform answer, whether or not the slot is marked right now: its
    /// text, its caret and its rows ([`Freeform`], upstream's field). Kept per
    /// entry so that Tab cycles without losing what was typed.
    draft: Freeform,
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
            draft: Freeform::default(),
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
}

/// The sequences a panel row is painted with, or none at all.
///
/// One construction for the text and the paint ([`QuestionPanel::rows`] and
/// [`QuestionPanel::painted`]): a sequence costs no cells, so the two cannot be
/// different heights or wrap in different places.
#[derive(Debug, Clone, Copy)]
struct Pens {
    /// The question (`bold_style`).
    bold: &'static str,
    /// The marked choice (`selected_completion_style`, which is the welcome
    /// line's bold hint grey in both modes -- `super::picker`'s marked row).
    marked: &'static str,
    /// Every other choice, a description, and the hint row (`dim_style`).
    dim: &'static str,
    /// The answer's one-cell caret (`ESC[7m`, `question_ui.zig:328`).
    reverse: &'static str,
    /// What ends a run.
    reset: &'static str,
}

impl Pens {
    /// No sequences at all: the rows as text.
    const PLAIN: Self = Self {
        bold: "",
        marked: "",
        dim: "",
        reverse: "",
        reset: "",
    };

    fn of(palette: &Palette) -> Self {
        Self {
            bold: palette.bold(),
            marked: palette.subtitle(),
            dim: palette.dim(),
            reverse: palette.reverse(),
            reset: palette.reset(),
        }
    }

    /// `text` in `pen`, ended; nothing around an empty run.
    fn run(&self, pen: &str, text: &str) -> String {
        if pen.is_empty() || text.is_empty() {
            return text.to_string();
        }
        format!("{pen}{text}{}", self.reset)
    }

    /// A row cut to `cols` by the painter's own rule, with a run that may have
    /// been open at the cut ended **after** it: the cut drops everything past
    /// the last cell that fits, the closing reset included, and a row that lost
    /// it would leave its colour open over the next one.
    fn finish(&self, row: &str, cols: u16) -> String {
        let cut = super::frame::clip(row, cols);
        if cut.len() == row.len() || self.reset.is_empty() || !cut.contains('\u{1b}') {
            return cut.to_string();
        }
        format!("{cut}{}", self.reset)
    }
}

/// The batch, and where the user has got to in it.
pub(crate) struct QuestionPanel {
    id: QuestionId,
    entries: Vec<Entry>,
    /// An index into [`Self::entries`]: the one question that is shown.
    current: usize,
    /// The bytes of a bracketed paste, while one is arriving into an open
    /// answer. `None` between pastes, and for a paste that began while a
    /// choice was marked -- which is dropped whole.
    paste: Option<Vec<u8>>,
}

/// A manual `Debug`, because the band's slot is a `Debug` enum
/// (`super::shell::Slot`) and a derived one would print the field's caret
/// bookkeeping. What is printed is what a failure would
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

/// Which choices the panel shows on a screen of a given size, and whether that
/// screen can really hold it.
///
/// A value rather than three readings of the same conditions inside `rows`,
/// `height` and `apply`: every row the panel paints, the caret's row and the
/// window the ordinals are checked against all come from exactly these numbers,
/// and three derivations are three chances to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    /// The first choice shown.
    top: usize,
    /// How many choices are shown, from [`Self::top`].
    visible: usize,
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
            paste: None,
        }
    }

    /// Which batch this panel is answering.
    /// The question being shown, as the model wrote it: untrusted text.
    pub(crate) fn question(&self) -> &str {
        self.entries
            .get(self.current)
            .map_or("", |entry| entry.question.as_str())
    }

    pub(crate) fn id(&self) -> QuestionId {
        self.id
    }

    /// The question that is shown, or nothing at all for a batch with no
    /// questions in it.
    ///
    /// Total rather than indexed: the bounds are the tool's
    /// (`crate::tools::question::parse` admits 1 to 20), and this type is
    /// constructible from anywhere in the crate -- so an empty batch is a panel
    /// that presents no choices and is refused by the shell, rather than a
    /// panic on the UI thread.
    fn entry(&self) -> Option<&Entry> {
        self.entries.get(self.current)
    }

    /// Which choices the panel shows on this screen.
    ///
    /// `visible = min(options, max(2, rows / 3))` choices, then fewer until the
    /// whole block is one [`super::layout::fits_approval`] admits: the
    /// composer gives the question its rows, as it gives a permission question
    /// its own (`super::shell::Shell::fit`), and the screen's opinion is the
    /// last word. Rows rather than choices are what the screen is asked about,
    /// because a choice whose label or description wraps takes more than one.
    fn shape(&self, cols: u16, terminal_rows: u16) -> Shape {
        let Some(entry) = self.entry() else {
            return Shape {
                top: 0,
                visible: 0,
                fits: false,
            };
        };
        let count = entry.options.len();
        // The question and the blank row under it, and the blank row above the
        // bottom rule.
        let fixed = title_rows(&entry.question, cols, Pens::PLAIN).len() + 2;
        let wanted = count.min(usize::from(terminal_rows / 3).max(MIN_VISIBLE));
        for visible in (1..=wanted).rev() {
            let top = window(entry.top, entry.selected, visible, count);
            let height = fixed
                + (top..top + visible)
                    .map(|index| option_height(entry, index, cols, terminal_rows))
                    .sum::<usize>();
            let height = u16::try_from(height).unwrap_or(u16::MAX);
            if super::layout::fits_approval(terminal_rows, cols, height) {
                return Shape {
                    top,
                    visible,
                    // A screen that can show only one of several choices cannot
                    // ask the question, whatever it can paint.
                    fits: visible >= MIN_VISIBLE.min(count),
                };
            }
        }
        // Still a coherent block, so `rows` and `height` agree about a screen
        // nothing will be painted on: the shell refuses the batch before
        // anything is installed.
        let visible = 1.min(count);
        Shape {
            top: window(entry.top, entry.selected, visible, count),
            visible,
            fits: false,
        }
    }

    /// The panel's rows as text, top first: the question, a blank row, the
    /// choices the window is showing, and a blank row
    /// (`composeQuestionPanelText`, `question_ui.zig:40-66`).
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        self.build(cols, terminal_rows, Pens::PLAIN)
    }

    /// The same rows in the palette's colours, for the band to paint.
    pub(crate) fn painted(&self, cols: u16, terminal_rows: u16, palette: &Palette) -> Vec<String> {
        self.build(cols, terminal_rows, Pens::of(palette))
    }

    /// The one construction [`Self::rows`] and [`Self::painted`] share.
    ///
    /// Every row is finished to the screen **here**, by the painter's own rule
    /// (`super::frame::clip`), rather than left for the painter: a row measured
    /// by the band at one width and drawn at another is how a stale row is left
    /// standing in the band. The cut is a *screen* cut and nothing else -- the
    /// canonical strings are untouched, and it is the canonical label that
    /// becomes the answer.
    fn build(&self, cols: u16, terminal_rows: u16, pens: Pens) -> Vec<String> {
        let Some(entry) = self.entry() else {
            return Vec::new();
        };
        let shape = self.shape(cols, terminal_rows);
        let mut rows = title_rows(&entry.question, cols, pens);
        rows.push(String::new());
        for index in shape.top..(shape.top + shape.visible).min(entry.options.len()) {
            rows.extend(option_rows(entry, index, cols, terminal_rows, pens));
        }
        rows.push(String::new());
        rows.into_iter()
            .map(|row| pens.finish(&row, cols))
            .collect()
    }

    /// How many rows the band has to give the panel.
    ///
    /// The length of [`Self::rows`], so the count the band solved its geometry
    /// from cannot drift from the paint.
    pub(crate) fn height(&self, cols: u16, terminal_rows: u16) -> u16 {
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

    /// Which row of the panel the marked choice begins on.
    pub(crate) fn caret_row(&self, cols: u16, terminal_rows: u16) -> u16 {
        let Some(entry) = self.entry() else {
            return 0;
        };
        let shape = self.shape(cols, terminal_rows);
        let above = title_rows(&entry.question, cols, Pens::PLAIN).len()
            + 1
            + (shape.top..entry.selected.max(shape.top))
                .map(|index| option_height(entry, index, cols, terminal_rows))
                .sum::<usize>();
        // Inside the block whatever the arithmetic above was handed: a caret
        // reported below the panel's last row would be a caret on the divider.
        let last = usize::from(self.height(cols, terminal_rows).saturating_sub(1));
        u16::try_from(above.min(last)).unwrap_or(0)
    }

    /// Where the caret goes **while the freeform slot is open**: the answer's
    /// row the caret is on, within the panel, and the cells to the left of it
    /// there.
    ///
    /// `None` the rest of the time, which is what says the caret belongs on the
    /// marked choice instead ([`Self::caret_row`]). Two answers rather than one,
    /// because they are two different claims about what the next keystroke does:
    /// a caret in a draft says typing goes into it, and a caret on a choice says
    /// a digit takes one.
    ///
    /// The cell there is painted as upstream paints it -- reverse video
    /// (`writeSelectedFreeformOptionLines`, `question_ui.zig:323-399`) -- and
    /// the terminal's own caret is placed on it as well.
    pub(crate) fn caret(&self, cols: u16, terminal_rows: u16) -> Option<(u16, u16)> {
        let entry = self.entry()?;
        if !entry.drafting() {
            return None;
        }
        let slot = entry.freeform();
        let (row, column) = entry.draft.caret(draft_cols(slot, cols));
        let shown = field_window(&entry.draft, cols, slot, terminal_rows);
        let row = usize::from(self.caret_row(cols, terminal_rows))
            .saturating_add(row.saturating_sub(shown.start));
        let last = usize::from(self.height(cols, terminal_rows).saturating_sub(1));
        let row = u16::try_from(row.min(last)).unwrap_or(0);
        let column = prefix_width(slot)
            .saturating_add(column)
            // On the screen, always: a column past the last one is a cursor the
            // terminal clamps silently onto a row it was not placed on.
            .min(cols.saturating_sub(1));
        Some((row, column))
    }

    /// The row under the band's bottom rule while this batch is up: what the
    /// keys do, and -- for a batch of more than one -- which question this is
    /// (`questionInteractionHint`, `input_presentation.zig:192-249`).
    ///
    /// `escape_armed` is whether an Escape at a freeform draft has armed the
    /// clear, which takes the row's right side as it does under the composer
    /// (`input_presentation.zig:530-537`).
    pub(crate) fn hint(&self, cols: u16, escape_armed: bool, palette: &Palette) -> String {
        self.hint_with(cols, escape_armed, Pens::of(palette))
    }

    /// The same row as text.
    #[cfg(test)]
    fn hint_text(&self, cols: u16, escape_armed: bool) -> String {
        self.hint_with(cols, escape_armed, Pens::PLAIN)
    }

    fn hint_with(&self, cols: u16, escape_armed: bool, pens: Pens) -> String {
        let budget = usize::from(cols);
        let drafting = self.entry().is_some_and(Entry::drafting);
        let full = if drafting {
            FREEFORM_HINT.to_string()
        } else {
            choose_hint(self.entry().map_or(0, |entry| entry.options.len()))
        };
        let mut line = if usize::from(width(&full)) <= budget {
            full
        } else if drafting {
            widest_fitting(&FREEFORM_HINTS, budget).to_string()
        } else {
            widest_fitting(&CHOICE_HINTS, budget).to_string()
        };
        // Batch progress, right-aligned, and only where it leaves two cells
        // between itself and the keys (`input_presentation.zig:228-246`). A
        // lone question needs no counter.
        if self.entries.len() > 1 {
            let counter = format!("Question {} of {}", self.current + 1, self.entries.len());
            let used = usize::from(width(&line));
            let counted = usize::from(width(&counter));
            if used + counted + 2 <= budget {
                line.push_str(&" ".repeat(budget - used - counted));
                line.push_str(&counter);
            }
        }
        if !escape_armed {
            return pens.finish(&pens.run(pens.dim, &line), cols);
        }
        // The armed clear outranks the counter: the left side is cut to leave
        // the warning a column of air, and the warning ends at the row's last
        // cell (`input_presentation.zig:539-569`).
        let warning = usize::from(width(ESCAPE_ARMED));
        let left_cells = if budget > warning {
            budget - warning - 1
        } else {
            budget
        };
        let left = super::frame::clip(&line, u16::try_from(left_cells).unwrap_or(cols));
        let mut row = pens.run(pens.dim, left);
        let shown = usize::from(width(left));
        if budget > shown + warning {
            row.push_str(&" ".repeat(budget - warning - shown));
            row.push_str(&pens.run(pens.dim, ESCAPE_ARMED));
        }
        pens.finish(&row, cols)
    }

    /// Whether an Escape now would only arm the clear of a draft rather than
    /// cancel the batch: the freeform slot is marked and holds text.
    ///
    /// Upstream's composer contract, extended to the draft
    /// (`app_input_runtime.zig:3120-3135`): Escape at a non-empty draft arms,
    /// a second inside the window clears it, and only an empty field lets
    /// Escape cancel. Asked by the shell, which owns the window
    /// (`super::gesture`).
    pub(crate) fn holds_draft(&self) -> bool {
        self.entry()
            .is_some_and(|entry| entry.drafting() && !entry.draft.is_empty())
    }

    /// Empties the open draft, leaving the slot marked so the user can type
    /// again or cancel (`clearFreeformDraft`, `question_prompt.zig:428-435`).
    /// `false` when there was nothing to clear.
    pub(crate) fn clear_draft(&mut self) -> bool {
        if !self.holds_draft() {
            return false;
        }
        self.entry_mut().draft.clear();
        true
    }

    /// The rows the document keeps once every question is answered: each
    /// question under its ordinal, and the answer hung beneath it in the
    /// answer grey (`writeQuestionAnswerResolution`, `question_ui.zig:465-501`).
    ///
    /// Wrapped here, at this width, with every continuation hung under the
    /// field's first column -- the document does not re-wrap a finished row,
    /// and a hanging indent is not a gutter it has. The answer is shown as the
    /// tool encodes it for the model (`answer_safe`), so what the user reads
    /// back is what was sent.
    ///
    /// Each row comes with what it is (`super::theme::DocumentRow`): an
    /// answer's rows are recorded as answers, so a theme report repaints their
    /// grey as the answer's and not as a turn summary's, which is the same
    /// number on a dark screen.
    pub(crate) fn answered_rows(
        &self,
        answers: &[String],
        cols: u16,
        palette: &Palette,
    ) -> Vec<(String, DocumentRow)> {
        self.answered_with(answers, cols, palette.answer(), palette.reset())
    }

    fn answered_with(
        &self,
        answers: &[String],
        cols: u16,
        pen: &str,
        reset: &str,
    ) -> Vec<(String, DocumentRow)> {
        let mut rows = Vec::new();
        for (index, (entry, answer)) in self.entries.iter().zip(answers).enumerate() {
            let prefix = format!("  {}) ", index + 1);
            let indent = " ".repeat(usize::from(width(&prefix)));
            let budget = cols.saturating_sub(width(&prefix)).max(1);
            for (row, segment) in safe_rows(&entry.question, budget).into_iter().enumerate() {
                let lead = if row == 0 { &prefix } else { &indent };
                rows.push((format!("{lead}{segment}"), DocumentRow::Plain));
            }
            for segment in safe_rows(&answer_safe(answer), budget) {
                rows.push((
                    if pen.is_empty() {
                        format!("{indent}{segment}")
                    } else {
                        format!("{pen}{indent}{segment}{reset}")
                    },
                    DocumentRow::Answer,
                ));
            }
        }
        rows
    }

    /// What one keystroke does.
    pub(crate) fn apply(&mut self, act: Act, cols: u16, terminal_rows: u16) -> Answered {
        if self.entries.get(self.current).is_none() {
            // A batch with no questions cannot be answered, and inventing an
            // empty submission would hand the model an answer document nobody
            // wrote. It is refused instead.
            return Answered::Cancelled;
        }
        let drafting = self.entry_mut().drafting();
        let content = draft_cols(self.entry_mut().freeform(), cols);
        match act {
            // Both cancel the **batch**. Which key it was is the shell's to
            // tell apart: Ctrl-C stops the turn as well.
            Act::Escape | Act::Cancel => Answered::Cancelled,
            Act::Tab => {
                self.paste = None;
                self.current = (self.current + 1) % self.entries.len();
                Answered::Redraw
            }
            // The plain arrows walk the answer's rows while it is open, and
            // choose only where it has no row that way
            // (`input_question_runtime.zig:150-170`).
            Act::Up | Act::Down if drafting => {
                let direction = if act == Act::Up {
                    Direction::Up
                } else {
                    Direction::Down
                };
                if self.entry_mut().draft.move_vertical(content, direction) {
                    return Answered::Redraw;
                }
                self.choose(act == Act::Up, cols, terminal_rows)
            }
            Act::Up | Act::SelectUp => self.choose(true, cols, terminal_rows),
            Act::Down | Act::SelectDown => self.choose(false, cols, terminal_rows),
            Act::Text(character) if drafting => {
                let mut encoded = [0u8; 4];
                self.insert(character.encode_utf8(&mut encoded))
            }
            Act::Text(character) => {
                // The window this keystroke is answered against: an ordinal
                // is about the block the user is looking at.
                let shape = self.shape(cols, terminal_rows);
                self.typed(character, shape)
            }
            Act::Submit => {
                self.paste = None;
                let taken = self.entry_mut().choice();
                self.record(taken)
            }
            // At a choice, Left is the previous question and stops at the
            // first; nothing about the one it leaves changes, its answer
            // included (`retreatToPrevious`, `question_prompt.zig:605-615`).
            // Alt-b too: upstream binds `.word_left` beside `.cursor_left`
            // (`input_question_runtime.zig:190-195@481104fd`).
            Act::Left | Act::AltB if !drafting => {
                if self.current == 0 {
                    return Answered::Nothing;
                }
                self.current -= 1;
                Answered::Redraw
            }
            // A paste while a choice is marked is dropped whole, its bytes
            // included (`paste_framing.zig:113`, `.decision_prompt`).
            Act::PasteStart => {
                self.paste = drafting.then(Vec::new);
                Answered::Nothing
            }
            Act::PasteByte(byte) => {
                if let Some(held) = self.paste.as_mut() {
                    held.push(byte);
                }
                Answered::Nothing
            }
            Act::PasteEnd => match self.paste.take() {
                Some(bytes) if drafting => self.insert(&pasted(&bytes)),
                _ => Answered::Nothing,
            },
            Act::Backspace
            | Act::Delete
            | Act::Left
            | Act::Right
            | Act::Home
            | Act::End
            | Act::WordLeft
            | Act::AltB
            | Act::WordRight
            | Act::DeleteWordLeft
            | Act::KillToStart
            | Act::KillToEnd => {
                if !drafting {
                    // A key that means something only to a draft, with no draft
                    // open. Swallowed rather than passed on -- the question has
                    // the focus -- and it changed nothing, so it owes no frame.
                    return Answered::Nothing;
                }
                let draft = &mut self.entry_mut().draft;
                match act {
                    Act::Backspace => {
                        draft.backspace();
                    }
                    Act::Delete => {
                        draft.delete();
                    }
                    Act::DeleteWordLeft => {
                        draft.delete_token_left();
                    }
                    Act::KillToStart => {
                        draft.kill_to_start();
                    }
                    Act::KillToEnd => {
                        draft.kill_to_end();
                    }
                    Act::Left => draft.left(),
                    Act::Right => draft.right(),
                    Act::WordLeft | Act::AltB => draft.word_left(),
                    Act::WordRight => draft.word_right(),
                    Act::Home => draft.home(),
                    _ => draft.end(),
                }
                Answered::Redraw
            }
        }
    }

    /// The choice above (`up`) or below the marked one, wrapping
    /// (`moveChoice`, `question_prompt.zig:677-687`), with the window moved to
    /// keep it on the screen.
    fn choose(&mut self, up: bool, cols: u16, terminal_rows: u16) -> Answered {
        let visible = self.shape(cols, terminal_rows).visible;
        let entry = self.entry_mut();
        let count = entry.options.len();
        entry.selected = if up {
            (entry.selected + count - 1) % count
        } else {
            (entry.selected + 1) % count
        };
        entry.top = window(entry.top, entry.selected, visible, count);
        self.paste = None;
        Answered::Redraw
    }

    /// The question that is shown, for the paths that have already established
    /// there is one.
    ///
    /// Every caller is inside [`Self::apply`] or [`Self::clear_draft`], past
    /// their own guards, so the fallback below is unreachable rather than a
    /// policy -- and it is a panic nobody can trigger from a keystroke, which is
    /// what a UI thread holding a raw terminal is owed.
    fn entry_mut(&mut self) -> &mut Entry {
        let current = self.current.min(self.entries.len().saturating_sub(1));
        self.entries
            .get_mut(current)
            .expect("apply refuses a batch with no questions before it reaches here")
    }

    /// One typed character at a marked choice: an ordinal, or nothing. (In an
    /// open answer it is text, and [`Self::apply`] inserts it.)
    fn typed(&mut self, character: char, shape: Shape) -> Answered {
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
        if index < shape.top || index >= shape.top + shape.visible {
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

    /// `typed` into the open answer at its caret, or a silent refusal of all
    /// of it.
    fn insert(&mut self, typed: &str) -> Answered {
        if typed.is_empty() {
            return Answered::Nothing;
        }
        let entry = self.entry_mut();
        // **Measured after encoding**, because that is the bound the executor
        // re-checks on the way out (`crate::tools::question`'s
        // `bounded_answer`): a draft that passed a byte count here and failed
        // the encoded one there would be a refusal the user gets after they
        // have finished typing. Whole or not at all, so a paste is never half
        // taken (`canInsert`, `input_paste_runtime.zig:270-274`).
        let held = answer_safe(entry.draft.text()).len();
        let adding = answer_safe(typed).len();
        if held.saturating_add(adding) > MAX_FREEFORM_ENCODED_BYTES {
            // Silently, like every other keystroke a budget refuses on this
            // surface (`super::shell::Shell::type_character`).
            return Answered::Nothing;
        }
        entry.draft.insert(typed);
        Answered::Redraw
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

/// The row the document keeps for a batch the user refused, by either key:
/// `▪ Cancelled` ([`CANCELLED_MARK`]), the glyph in the outcome-label grey and
/// the word in none (`writeCancelledResolution`, `question_ui.zig:513-519`;
/// `red_style` is `252` in the dark and `238` in the light, which is
/// [`Palette::notice_label`]).
pub(crate) fn cancelled_row(palette: &Palette) -> String {
    format!(
        "{}{CANCELLED_MARK}{} {CANCELLED}",
        palette.notice_label(),
        palette.reset()
    )
}

/// What a bracketed paste puts into an answer: its text with every line
/// ending a line feed, a tab a space, and nothing else a terminal would obey
/// (`paste_framing.zig:119-127` keeps CR, LF and printables and turns a tab
/// into a space for this owner; `normalizeLineEndingsInPlace` at
/// `input_paste_runtime.zig:227-229`). Bytes that are not UTF-8 become the
/// replacement character rather than being dropped.
fn pasted(bytes: &[u8]) -> String {
    let kept: Vec<u8> = bytes
        .iter()
        .filter_map(|&byte| match byte {
            b'\t' => Some(b' '),
            b'\r' | b'\n' => Some(byte),
            0x00..=0x1f | 0x7f => None,
            _ => Some(byte),
        })
        .collect();
    String::from_utf8_lossy(&kept)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// The full choice hint, with the count of choices on this question
/// (`input_presentation.zig:215-219`): `1–N` with an en dash, as upstream
/// writes it.
fn choose_hint(count: usize) -> String {
    format!(
        "1\u{2013}{count} choose now    ↑↓ options    tab questions    enter answer    esc cancel"
    )
}

/// The first of `variants` that fits in `budget` cells, or the last when none
/// does (`widestFitting`, `display_width.zig:153-159`).
fn widest_fitting<'a>(variants: &[&'a str], budget: usize) -> &'a str {
    variants
        .iter()
        .copied()
        .find(|variant| usize::from(width(variant)) <= budget)
        .unwrap_or(variants[variants.len() - 1])
}

/// The question's rows: two columns in, bold, wrapped at the rest of the
/// screen with every continuation under the first
/// (`writeQuestionPanelQuestion`, `question_ui.zig:68-90`).
fn title_rows(question: &str, cols: u16, pens: Pens) -> Vec<String> {
    let budget = cols.saturating_sub(width(TITLE_INDENT)).max(1);
    safe_rows(question, budget)
        .into_iter()
        .map(|segment| format!("{TITLE_INDENT}{}", pens.run(pens.bold, &segment)))
        .collect()
}

/// `N) ` for the choice at `index`: absolute, so the number on the row is the
/// number the user types whatever the window is showing.
fn ordinal(index: usize) -> String {
    format!("{}) ", index + 1)
}

/// The cells in front of a choice's text: the indent and its ordinal
/// (`optionPrefixWidth`, `question_freeform_layout.zig:26-30`).
fn prefix_width(index: usize) -> u16 {
    width(OPTION_INDENT).saturating_add(width(&ordinal(index)))
}

/// How wide the draft's text is: the screen past the slot's prefix
/// (`contentWidth`, `question_freeform_layout.zig:32-34`).
fn draft_cols(slot: usize, cols: u16) -> u16 {
    cols.saturating_sub(prefix_width(slot)).max(1)
}

/// The column a description starts one cell before, or `0` for a screen too
/// narrow to give descriptions a column of their own
/// (`questionDescriptionColumn`, `question_ui.zig:401-405`).
fn description_column(cols: u16) -> u16 {
    if cols >= 72 {
        38
    } else if cols >= 52 {
        30
    } else {
        0
    }
}

/// The rows of the choice at `index` (`writeQuestionPanelOptionLine`,
/// `question_ui.zig:96-220`).
///
/// * The open answer -- the freeform slot while it is marked -- is as many
///   rows as it wraps to, by cell, with its caret's cell reversed.
/// * A choice with a description on a screen wide enough for a description
///   column: the label wrapped short of the column, the description wrapped in
///   it, side by side, as many rows as the longer of the two.
/// * Otherwise the label wrapped at the screen, and any description on rows of
///   its own under it, in the same indent.
///
/// The freeform slot, unmarked, reads as what was typed into it, once
/// something has been (`questionOptionLabel`, `question_ui.zig:242-251`).
fn option_rows(
    entry: &Entry,
    index: usize,
    cols: u16,
    terminal_rows: u16,
    pens: Pens,
) -> Vec<String> {
    let Some(option) = entry.options.get(index) else {
        return Vec::new();
    };
    let marked = index == entry.selected;
    let pen = if marked { pens.marked } else { pens.dim };
    let lead = format!("{OPTION_INDENT}{}", ordinal(index));
    let prefix = prefix_width(index);
    let hang = " ".repeat(usize::from(prefix));
    let slot = index == entry.freeform();
    if marked && slot {
        // The open answer, every row of it: the ordinal on the first, the
        // prefix's width of blank on the rest, the text in the marked grey and
        // the caret's cell reversed (`writeSelectedFreeformOptionLines`,
        // `question_ui.zig:323-399`).
        let field = entry.draft.rows(draft_cols(index, cols));
        let shown = field_window(&entry.draft, cols, index, terminal_rows);
        return field[shown]
            .iter()
            .enumerate()
            .map(|(row, field)| {
                let head = if row == 0 { lead.as_str() } else { "" };
                let mut line = if row == 0 {
                    String::new()
                } else {
                    hang.clone()
                };
                line.push_str(&pens.run(pen, &format!("{head}{}", field.before)));
                if let Some(unit) = field.caret {
                    let unit = if unit.is_empty() { " " } else { unit };
                    line.push_str(pens.reverse);
                    line.push_str(unit);
                    line.push_str(pens.reset);
                }
                line.push_str(&pens.run(pen, field.after));
                line
            })
            .collect();
    }
    let label = if slot && !entry.draft.is_empty() {
        entry.draft.text()
    } else {
        option.label.as_str()
    };
    let description = option.description.as_deref().unwrap_or("");
    let column = if description.is_empty() {
        0
    } else {
        description_column(cols)
    };
    let mut rows = Vec::new();
    if column > 0 {
        let label_end = if column > prefix.saturating_add(2) {
            column - 2
        } else {
            cols
        };
        let labels = safe_rows(label, label_end.saturating_sub(prefix).max(1));
        let descriptions = safe_rows(description, cols.saturating_sub(column).max(1));
        // The description starts one cell before the column
        // (`question_ui.zig:125`), and at least one cell after the label.
        let indent = usize::from(column - 1);
        for row in 0..labels.len().max(descriptions.len()) {
            let segment = labels.get(row).map_or("", String::as_str);
            let mut line = if row == 0 {
                pens.run(pen, &format!("{lead}{segment}"))
            } else {
                format!("{hang}{}", pens.run(pen, segment))
            };
            if let Some(said) = descriptions.get(row) {
                let used = usize::from(prefix) + usize::from(width(segment));
                line.push_str(&" ".repeat(indent.saturating_sub(used).max(1)));
                line.push_str(&pens.run(pens.dim, said));
            }
            rows.push(line);
        }
        return rows;
    }
    let budget = cols.saturating_sub(prefix).max(1);
    for (row, segment) in safe_rows(label, budget).into_iter().enumerate() {
        rows.push(if row == 0 {
            pens.run(pen, &format!("{lead}{segment}"))
        } else {
            format!("{hang}{}", pens.run(pen, &segment))
        });
    }
    if !description.is_empty() {
        for segment in safe_rows(description, budget) {
            rows.push(format!("{hang}{}", pens.run(pens.dim, &segment)));
        }
    }
    rows
}

/// How many rows the choice at `index` takes: [`option_rows`]'s count, without
/// painting them. The open answer's rows are counted by the field itself, which
/// is one pass over the text and no row strings.
fn option_height(entry: &Entry, index: usize, cols: u16, terminal_rows: u16) -> usize {
    if index == entry.selected && index == entry.freeform() {
        return field_window(&entry.draft, cols, index, terminal_rows).len();
    }
    option_rows(entry, index, cols, terminal_rows, Pens::PLAIN).len()
}

/// The open answer's rows that are shown on a screen `terminal_rows` tall.
///
/// All of them, as upstream shows them, until there are more than a third of
/// the screen (the share the choices get, [`QuestionPanel::shape`]); past that,
/// the third that ends at the caret's row, so the row being typed on is always
/// on the screen. Upstream lets a field that tall push its rows past the band
/// and clips them (`question footer clips composed rows to solved footer
/// band`, `question_ui.zig:600-630`), which can clip the caret's row away.
fn field_window(
    draft: &Freeform,
    cols: u16,
    slot: usize,
    terminal_rows: u16,
) -> std::ops::Range<usize> {
    let content = draft_cols(slot, cols);
    let total = draft.rows(content).len();
    let cap = usize::from(terminal_rows / 3).max(1);
    if total <= cap {
        return 0..total;
    }
    let (row, _) = draft.caret(content);
    let start = row.saturating_add(1).saturating_sub(cap);
    start..start + cap
}

/// The first choice a `visible`-choice window shows, given where the mark is.
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
        // fetch_update is renamed try_update in Rust 1.99; kept for the 1.96 MSRV.
        #[allow(deprecated)]
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
    ///
    /// **Declined and interrupted are two answers.** A batch the band could
    /// not show, or that no UI was there to show, is declined: the model is
    /// told so and the turn goes on. A batch the turn was stopped at -- the
    /// interrupt the band sends for Escape and for Ctrl-C alike, a shutdown, the
    /// session ending -- is interrupted, and the model is never told the user
    /// declined it (`crate::tools::question::ABORTED_OUTPUT`).
    fn request(&self, entries: &[QuestionEntry]) -> QuestionReply {
        let Some(id) = self.mint() else {
            return QuestionReply::Declined;
        };
        let token = self.cancel.token();
        let request = QuestionRequest {
            id,
            entries: entries.to_vec(),
        };
        if park_on(send_ui(&self.events, &token, UiEvent::Question(request))).is_err() {
            // No UI to ask, or no session to ask in -- and which of the two is
            // what the session's root says: a root that was cancelled while the
            // question waited for room is a stop, and anything else is a
            // question nobody could be asked.
            return if token.is_cancelled() {
                QuestionReply::Interrupted
            } else {
                QuestionReply::Declined
            };
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
                        return QuestionReply::Interrupted;
                    }
                    message = self.control.answered() => message,
                };
                match message {
                    Some(TurnControl::QuestionAnswer { id: seen, answers }) if seen == id => {
                        return QuestionReply::Answered(answers)
                    }
                    Some(TurnControl::QuestionCancelled { id: seen }) if seen == id => {
                        return QuestionReply::Declined
                    }
                    // A keystroke on a panel that has already gone, or an
                    // approval answer nobody is waiting for: consumed, so the
                    // next question does not inherit it -- the turn loop's rule
                    // (`super::worker`'s `raced_against_control`).
                    Some(
                        TurnControl::QuestionAnswer { .. }
                        | TurnControl::QuestionCancelled { .. }
                        | TurnControl::Answer { .. },
                    ) => continue,
                    // `/model` while the model's question is up: about the next
                    // turn, so put aside for the loop, and the question stays up
                    // -- it is not the user cancelling the batch
                    // (`super::approval::ControlChannel::defer`).
                    Some(TurnControl::Model(change)) => {
                        self.control.defer(change);
                        continue;
                    }
                    // `/fast` or `/ultrafast` likewise.
                    Some(TurnControl::Speed(change)) => {
                        self.control.defer_speed(change);
                        continue;
                    }
                    // `/undo` while the model's question is up: done once the
                    // turn is over, and the batch stays up -- it is not the
                    // user cancelling it (`ControlChannel::defer_undo`).
                    Some(TurnControl::Undo { through }) => {
                        self.control.defer_undo(through, false);
                        continue;
                    }
                    // `/rename` while the model's question is up: written once
                    // the turn is over, and the batch stays up
                    // (`ControlChannel::defer_rename`).
                    Some(TurnControl::Rename { title, announced }) => {
                        self.control.defer_rename(title, false, announced);
                        continue;
                    }
                    // `/model`'s menu asking for the catalog: the read starts
                    // now, on a thread of its own this wait does not hold, as
                    // the approval prompter's does
                    // (`super::approval::ControlChannel::ask_catalog`), and the
                    // batch stays up.
                    Some(TurnControl::Catalog(ask)) => {
                        if let Some(answer) = self.control.ask_catalog(ask) {
                            let _ = send_ui(&self.events, &token, answer).await;
                        }
                        continue;
                    }
                    // The interrupt and the shutdown belong to the loop that can
                    // act on them (`super::approval::ControlChannel::put_back`);
                    // `give_back` rearms.
                    Some(stop) => {
                        self.control.give_back(stop);
                        return QuestionReply::Interrupted;
                    }
                    // The UI dropped its sender, which is the session ending.
                    // `answered` rearmed on the ready poll; rearming again is
                    // spurious and harmless, and keeps "every exit rearms" true
                    // without a reader having to check.
                    None => {
                        self.control.rearm();
                        return QuestionReply::Interrupted;
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

    /// The record's rows as text, without what each is.
    fn texts(rows: Vec<(String, DocumentRow)>) -> Vec<String> {
        rows.into_iter().map(|(row, _)| row).collect()
    }

    /// One bracketed paste of `text`, byte by byte, as the decoder hands it on.
    fn paste(panel: &mut QuestionPanel, text: &str, cols: u16, rows: u16) {
        panel.apply(Act::PasteStart, cols, rows);
        for byte in text.bytes() {
            panel.apply(Act::PasteByte(byte), cols, rows);
        }
        panel.apply(Act::PasteEnd, cols, rows);
    }

    /// The cells in front of a `two()` question's freeform slot: `    3) `.
    const SLOT_PREFIX: u16 = 7;

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
        // Upstream's `N) ` ordinals, and the batch's progress on the hint row
        // rather than in the panel (`question_ui.zig:56`: "batch progress lives
        // in the hint row").
        let panel = two();
        let text = panel.rows(60, 40).join("\n");
        for expected in [
            "Which depth?",
            "1) Thorough",
            "2) Quick",
            "3) Other",
            "reads every file",
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
        assert!(
            !text.contains("of 2"),
            "the counter is in the panel: {text}"
        );
        assert!(panel.hint_text(120, false).ends_with("Question 1 of 2"));
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
            panel.hint_text(120, false).ends_with("Question 2 of 2"),
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
            panel.hint_text(120, false).ends_with("Question 1 of 2"),
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
        let encoded = answer_safe("\u{200b}").len();
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
            answer_safe(&answer).len() <= MAX_FREEFORM_ENCODED_BYTES,
            "the answer encodes to {} bytes",
            answer_safe(&answer).len()
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

    /// A batch of `count` questions, `Pick N?` with `Keep N` and `Change N`,
    /// each choice described: `p5-input-question20`'s batch.
    fn batch(count: usize) -> QuestionPanel {
        QuestionPanel::new(QuestionRequest {
            id: QuestionId(20),
            entries: (1..=count)
                .map(|index| QuestionEntry {
                    question: format!("Pick {index}?"),
                    options: vec![
                        QuestionOption {
                            label: format!("Keep {index}"),
                            description: Some("Leave it as it is.".to_string()),
                        },
                        QuestionOption {
                            label: format!("Change {index}"),
                            description: Some("Apply the change.".to_string()),
                        },
                    ],
                })
                .collect(),
        })
    }

    #[test]
    fn a_batch_of_twenty_counts_in_two_digits_only_where_the_row_has_room() {
        // `questionInteractionHint` (`input_presentation.zig:198-246@481104fd`):
        // the keys, the widest variant that fits when they do not, and the
        // counter right-aligned only where two cells of air are left. fx's
        // screens (`fx-p5-input-question20-*-q20-panel`, `-q20-tab10`): at 120
        // both counters show, at 72 the short keys leave room for
        // `Question 1 of 20` and not for `Question 10 of 20`, and at 80 the
        // full keys fit and no counter does.
        let mut panel = batch(20);
        let full = "1\u{2013}3 choose now    \u{2191}\u{2193} options    tab questions    \
                    enter answer    esc cancel";
        let short =
            "\u{2191}\u{2193} options \u{b7} tab questions \u{b7} enter answer \u{b7} esc cancel";
        let row =
            |panel: &QuestionPanel, cols: u16| panel.hint_text(cols, false).trim_end().to_string();
        assert_eq!(
            row(&panel, 120),
            format!(
                "{full}{}Question 1 of 20",
                " ".repeat(120 - full.chars().count() - 16)
            )
        );
        assert_eq!(row(&panel, 80), full);
        assert_eq!(row(&panel, 72), format!("{short}  Question 1 of 20"));
        for _ in 0..9 {
            panel.apply(Act::Tab, 120, 40);
        }
        assert!(panel.rows(120, 40).join("\n").contains("Pick 10?"));
        assert_eq!(
            row(&panel, 120),
            format!(
                "{full}{}Question 10 of 20",
                " ".repeat(120 - full.chars().count() - 17)
            )
        );
        assert_eq!(row(&panel, 72), short);
    }

    #[test]
    fn a_batch_of_twenty_is_walked_by_tab_and_left_and_answered_in_entry_order() {
        // `cycleQuestionPage` wraps past the last page, `retreatToPrevious`
        // stops at the first, and a decision moves to the next unanswered page
        // (`question_prompt.zig:609-615`, `:621-668`, `:670-676@481104fd`;
        // `fx-p5-input-question20-40x120-q20-{left,back9,tab20,wrap,first}`).
        let mut panel = batch(20);
        let shown = |panel: &QuestionPanel| panel.hint_text(120, false);
        assert!(matches!(panel.apply(Act::Left, 120, 40), Answered::Nothing));
        assert!(shown(&panel).ends_with("Question 1 of 20"));
        for _ in 0..19 {
            panel.apply(Act::Tab, 120, 40);
        }
        assert!(shown(&panel).ends_with("Question 20 of 20"));
        panel.apply(Act::Left, 120, 40);
        assert!(shown(&panel).ends_with("Question 19 of 20"));
        panel.apply(Act::Tab, 120, 40);
        panel.apply(Act::Tab, 120, 40);
        assert!(shown(&panel).ends_with("Question 1 of 20"), "Tab wraps");
        // Answered from the tenth: ten to twenty, then one to nine.
        for _ in 0..9 {
            panel.apply(Act::Tab, 120, 40);
        }
        let mut last = Answered::Nothing;
        for step in 0..20 {
            last = panel.apply(Act::Text(if step % 2 == 0 { '1' } else { '2' }), 120, 40);
            if step < 19 {
                assert!(matches!(last, Answered::Redraw), "step {step}");
            }
        }
        let Answered::Submitted(answers) = last else {
            panic!("the twentieth decision submits the batch");
        };
        // Question 10 + k got step k's choice: Keep at an even step.
        let expected: Vec<String> = (1..=20)
            .map(|question: usize| {
                let step = (question + 20 - 10) % 20;
                format!(
                    "{} {question}",
                    if step.is_multiple_of(2) {
                        "Keep"
                    } else {
                        "Change"
                    }
                )
            })
            .collect();
        assert_eq!(answers, expected);
    }

    #[test]
    fn twenty_answers_hang_under_their_numbers_at_one_and_two_digits() {
        // `composeQuestionResolutions` (`question_ui.zig:418-519@481104fd`) as
        // fx's e2e reads it: `  N) <question>` with the answer one row down
        // under the question's first character, `  10) ` six cells wide
        // (`tui-resume.test.ts:6176-6184@481104fd`;
        // `fx-p5-input-question20-40x120-q20-answered.full`).
        let panel = batch(20);
        let answers: Vec<String> = (1..=20).map(|index| format!("Keep {index}")).collect();
        let rows = texts(panel.answered_with(&answers, 120, "", ""));
        assert_eq!(rows.len(), 40);
        assert_eq!(rows[0], "  1) Pick 1?");
        assert_eq!(rows[1], "     Keep 1");
        assert_eq!(rows[18], "  10) Pick 10?");
        assert_eq!(rows[19], "      Keep 10");
        assert_eq!(rows[38], "  20) Pick 20?");
        assert_eq!(rows[39], "      Keep 20");
    }

    #[test]
    fn an_ordinal_for_a_choice_the_window_is_not_showing_is_refused() {
        let mut panel = six();
        assert!(
            !panel.rows(60, 12).join("\n").contains("7) Other"),
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
        // The marked slot is its open field, `    7) ` and the draft.
        assert!(
            panel
                .rows(60, 12)
                .iter()
                .any(|row| row.starts_with("    7) ")),
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
        assert!(panel
            .rows(60, 40)
            .iter()
            .any(|row| row.starts_with("    7) ")));
        let shrunk = panel.rows(60, 12).join("\n");
        assert!(
            shrunk.contains("    7) "),
            "the marked choice left the block on a screen that shrank: {shrunk}"
        );
        assert!(
            !shrunk.contains("1) file1.rs"),
            "the window did not move at all: {shrunk}"
        );
        // And the caret is still inside the block it is painted from.
        assert!(panel.caret_row(60, 12) < panel.height(60, 12));
    }

    #[test]
    fn a_long_question_is_wrapped_for_the_screen_and_not_in_the_answer() {
        // Wrapped rather than clipped, as upstream's is (`question_ui.zig:68-90`;
        // e2e `tui-decision-prompts.test.ts:2625`): every character of the
        // question is on the screen, on rows two columns in.
        let mut panel = QuestionPanel::new(one_entry(&"L".repeat(400)));
        assert_eq!(panel.id(), QuestionId(1));
        let rows = panel.rows(40, 40);
        assert!(rows.iter().all(|row| row.chars().count() <= 40));
        let title: Vec<&String> = rows.iter().take_while(|row| !row.is_empty()).collect();
        assert!(title.iter().all(|row| row.starts_with("  L")), "{title:?}");
        assert_eq!(
            title
                .iter()
                .map(|row| row.matches('L').count())
                .sum::<usize>(),
            400,
            "the question lost characters on the way to the screen"
        );
        assert_eq!(
            submitted(&mut panel, &[Act::Text('1')], 40, 40)[0],
            "Yes",
            "the answer is the label, whole"
        );
    }

    #[test]
    fn no_question_row_can_be_drawn_past_the_margin() {
        // SEC-A4b. The model writes the question, its choices and their
        // descriptions, and they are laid out by xfx's count: a run of
        // characters a terminal draws wider than that pushes what follows it
        // past the margin, so the user answers a question they were not shown.
        use super::super::approval_view::wider_terminal::{hides, runs};
        let tail = "TAILSEEN";
        let mut hidden = Vec::new();
        for (run, character, cells) in runs() {
            let text = format!("x{run}{tail}");
            for (what, question, label, description) in [
                ("question", text.clone(), "Yes".to_string(), None),
                ("label", "Which?".to_string(), text.clone(), None),
                (
                    "description",
                    "Which?".to_string(),
                    "Yes".to_string(),
                    Some(text.clone()),
                ),
            ] {
                let panel = QuestionPanel::new(QuestionRequest {
                    id: QuestionId(7),
                    entries: vec![QuestionEntry {
                        question,
                        options: vec![
                            QuestionOption { label, description },
                            QuestionOption {
                                label: "No".to_string(),
                                description: None,
                            },
                        ],
                    }],
                });
                let rows = panel.rows(120, 60);
                if let Some(why) = hides(&rows, 120, character, cells, tail) {
                    hidden.push(format!("{what}: {why}"));
                }
                // And the rows the document keeps once it is answered, the
                // question and the answer the user gave.
                let answered: Vec<String> = panel
                    .answered_with(std::slice::from_ref(&text), 120, "", "")
                    .into_iter()
                    .map(|(row, _)| row)
                    .collect();
                if let Some(why) = hides(&answered, 120, character, cells, tail) {
                    hidden.push(format!("{what}, answered: {why}"));
                }
            }
        }
        assert!(hidden.is_empty(), "{hidden:#?}");
    }

    #[test]
    fn a_label_one_cluster_wider_than_the_screen_is_split_and_shown_whole() {
        // SEC-A4e: a 201-cell Devanagari cluster in a choice, on a 40-cell view.
        let chain = "\u{915}".to_string() + &"\u{94d}\u{915}".repeat(200);
        let panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(8),
            entries: vec![QuestionEntry {
                question: "Which?".to_string(),
                options: vec![
                    QuestionOption {
                        label: format!("{chain} TAILSEEN"),
                        description: None,
                    },
                    QuestionOption {
                        label: "No".to_string(),
                        description: None,
                    },
                ],
            }],
        });
        super::super::approval_view::wider_terminal::assert_shown_whole(
            "question label",
            &panel.rows(40, 60),
            40,
            "\u{915}",
            201,
            "TAILSEEN",
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
        // Under the question and the blank row that follows it.
        assert_eq!(panel.caret_row(60, 40), 2, "the first choice's row");
        panel.apply(Act::Down, 60, 40);
        assert_eq!(panel.caret_row(60, 40), 3);

        panel.apply(Act::Text('3'), 60, 40);
        let (row, column) = panel.caret(60, 40).expect("the draft has the caret");
        assert_eq!(
            row, 4,
            "the draft is the slot's own row, as upstream's is (`question_ui.zig:110-114`)"
        );
        assert_eq!(
            column, SLOT_PREFIX,
            "an empty draft's caret is just past `    3) `"
        );

        // **Cells, not bytes.** A three-byte glyph two cells wide moves the
        // caret two columns; a caret counted in bytes would be three columns to
        // the right of the character it is supposed to be after.
        panel.apply(Act::Text('네'), 60, 40);
        let wide = super::super::wrap::width("네");
        assert_eq!(wide, 2, "the width of this glyph changed");
        assert_eq!(panel.caret(60, 40).expect("drafting").1, SLOT_PREFIX + wide);
        panel.apply(Act::Backspace, 60, 40);
        assert_eq!(panel.caret(60, 40).expect("drafting").1, SLOT_PREFIX);
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
        // The slot's own row, which is the last choice, over the blank row
        // above the bottom rule.
        assert_eq!(row, panel.height(30, 40) - 2);
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
        // Not a shape the tool can produce (`parse` admits 1 to 20), and this
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
    // upstream's rows, row for row (`fx-question-40x120-*`)
    // -----------------------------------------------------------------------

    /// The batch the captures were taken with
    /// (`scripts/fxcap/scenarios/question.json`): two questions, the first
    /// with two described choices and one bare one, the second with a bare
    /// choice and a described one.
    fn captured() -> QuestionPanel {
        QuestionPanel::new(QuestionRequest {
            id: QuestionId(5),
            entries: vec![
                QuestionEntry {
                    question: "Which database should I use?".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "Postgres".to_string(),
                            description: Some("Relational, strong consistency".to_string()),
                        },
                        QuestionOption {
                            label: "SQLite".to_string(),
                            description: Some("Single file, embedded".to_string()),
                        },
                        QuestionOption {
                            label: "DynamoDB".to_string(),
                            description: None,
                        },
                    ],
                },
                QuestionEntry {
                    question: "Deploy now?".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "Yes".to_string(),
                            description: None,
                        },
                        QuestionOption {
                            label: "No".to_string(),
                            description: Some("Wait for review".to_string()),
                        },
                    ],
                },
            ],
        })
    }

    /// The palette the captures were taken in.
    const DARK: Palette = Palette {
        mode: super::super::theme::Mode::Dark,
    };

    #[test]
    fn the_panel_is_upstreams_rows_at_120_columns() {
        // `fx-question-40x120-q1.txt` rows 6-12, between the two rules.
        let panel = captured();
        assert_eq!(
            panel.rows(120, 40),
            vec![
                "  Which database should I use?",
                "",
                "    1) Postgres                      Relational, strong consistency",
                "    2) SQLite                        Single file, embedded",
                "    3) DynamoDB",
                "    4) Other",
                "",
            ]
        );
        assert_eq!(panel.height(120, 40), 7);
    }

    #[test]
    fn the_panel_is_painted_in_upstreams_sequences() {
        // `fx-question-40x120-q1.ansi`: the question bold, the marked choice
        // in the bold hint grey with no marker glyph, every other choice and
        // every description dim (`question_ui.zig:110-112`, `:150-152`).
        assert_eq!(
            captured().painted(120, 40, &DARK),
            vec![
                "  \u{1b}[1mWhich database should I use?\u{1b}[0m".to_string(),
                String::new(),
                "\u{1b}[1;38;5;255m    1) Postgres\u{1b}[0m                      \
                 \u{1b}[38;5;245mRelational, strong consistency\u{1b}[0m"
                    .to_string(),
                "\u{1b}[38;5;245m    2) SQLite\u{1b}[0m                        \
                 \u{1b}[38;5;245mSingle file, embedded\u{1b}[0m"
                    .to_string(),
                "\u{1b}[38;5;245m    3) DynamoDB\u{1b}[0m".to_string(),
                "\u{1b}[38;5;245m    4) Other\u{1b}[0m".to_string(),
                String::new(),
            ]
        );
    }

    #[test]
    fn the_marked_choice_is_told_apart_by_its_colour_alone() {
        // `q1-down`: Down moves the bold, and nothing else on the row changes.
        let mut panel = captured();
        panel.apply(Act::Down, 120, 40);
        let painted = panel.painted(120, 40, &DARK);
        assert!(painted[2].starts_with("\u{1b}[38;5;245m    1) Postgres\u{1b}[0m"));
        assert!(painted[3].starts_with("\u{1b}[1;38;5;255m    2) SQLite\u{1b}[0m"));
        assert_eq!(panel.rows(120, 40), captured().rows(120, 40));
        // `q1-down4` and `q1-back`: the walk wraps both ways.
        for _ in 0..3 {
            panel.apply(Act::Down, 120, 40);
        }
        assert!(panel.painted(120, 40, &DARK)[2].starts_with("\u{1b}[1;38;5;255m    1)"));
        panel.apply(Act::Up, 120, 40);
        assert!(panel.painted(120, 40, &DARK)[5].starts_with("\u{1b}[1;38;5;255m    4) "));
    }

    #[test]
    fn tab_shows_the_next_question_and_its_count() {
        // `fx-question-40x120-q1-tab`: the second page, a description in the
        // same column, the counter moved on.
        let mut panel = captured();
        panel.apply(Act::Tab, 120, 40);
        assert_eq!(
            panel.rows(120, 40),
            vec![
                "  Deploy now?",
                "",
                "    1) Yes",
                "    2) No                            Wait for review",
                "    3) Other",
                "",
            ]
        );
        assert_eq!(
            panel.hint_text(120, false),
            format!(
                "1–3 choose now    ↑↓ options    tab questions    enter answer    esc cancel{}Question 2 of 2",
                " ".repeat(30)
            )
        );
    }

    #[test]
    fn the_hint_row_is_upstreams_and_counts_a_batch_of_more_than_one() {
        // `fx-question-40x120-q1` row 14, to the last cell, and in the dim grey
        // (`input_presentation.zig:552`).
        let panel = captured();
        let expected = format!(
            "1–4 choose now    ↑↓ options    tab questions    enter answer    esc cancel{}Question 1 of 2",
            " ".repeat(30)
        );
        assert_eq!(panel.hint_text(120, false), expected);
        assert_eq!(width(&expected), 120);
        assert_eq!(
            panel.hint(120, false, &DARK),
            format!("\u{1b}[38;5;245m{expected}\u{1b}[0m")
        );
        // `fx-question2-40x120-q`: a lone question has no counter.
        let lone = QuestionPanel::new(one_entry("Proceed?"));
        assert_eq!(
            lone.hint_text(120, false),
            "1–3 choose now    ↑↓ options    tab questions    enter answer    esc cancel"
        );
    }

    #[test]
    fn a_narrow_hint_row_keeps_enter_and_escape_and_drops_the_count_first() {
        // The ladder (`input_presentation.zig:186-190`): the first variant
        // that fits, and the counter only with two cells of air beside it.
        let panel = captured();
        assert_eq!(
            panel.hint_text(72, false),
            format!(
                "↑↓ options · tab questions · enter answer · esc cancel{}Question 1 of 2",
                " ".repeat(72 - 54 - 15)
            )
        );
        assert_eq!(
            panel.hint_text(60, false),
            "↑↓ options · tab questions · enter answer · esc cancel"
        );
        assert_eq!(panel.hint_text(30, false), "enter answer · esc cancel");
        assert!(width(&panel.hint_text(20, false)) <= 20);
    }

    #[test]
    fn the_freeform_hint_is_upstreams_and_its_keys_do_what_it_says() {
        // `fx-question-40x120-q1-down3` row 14 and, at 80 columns,
        // `fx-question-24x80-q1-down3` (`input_presentation.zig:179-184`,
        // `:212-214`). Claim changed: xfx used to say `←→ cursor    ↑↓
        // options`, because its draft was one row and Shift was dropped.
        let mut panel = captured();
        panel.apply(Act::Text('4'), 120, 40);
        assert_eq!(
            panel.hint_text(120, false),
            format!(
                "type answer    ↑↓←→ cursor    shift+↑↓ options    tab questions    enter answer    esc cancel{}Question 1 of 2",
                " ".repeat(12)
            )
        );
        assert_eq!(
            panel.hint_text(80, false),
            "↑↓ cursor · shift+↑↓ options · tab questions · enter answer · esc cancel"
        );
        // A one-row answer has no row above or below, so the plain arrows
        // choose -- upstream's fallback (`input_question_runtime.zig:155-170`).
        panel.apply(Act::Text('x'), 120, 40);
        panel.apply(Act::Down, 120, 40);
        assert_eq!(panel.draft(), None, "Down did not leave a one-row answer");
    }

    #[test]
    fn plain_arrows_walk_a_multi_row_answer_and_shifted_ones_choose() {
        // e2e `tui-decision-prompts.test.ts:2825`: a pasted three-line answer,
        // Up onto `xy`, `!` typed there; Shift+Up chooses `2)`, Shift+Down
        // comes back to the answer, which kept its text and its caret.
        let mut panel = captured();
        panel.apply(Act::Text('4'), 64, 20);
        paste(&mut panel, "line-one\nxy\nline-three", 64, 20);
        assert_eq!(panel.draft(), Some("line-one\nxy\nline-three"));
        assert_eq!(panel.apply(Act::Up, 64, 20), Answered::Redraw);
        panel.apply(Act::Text('!'), 64, 20);
        assert_eq!(panel.draft(), Some("line-one\nxy!\nline-three"));
        // The rows hang under the answer's first column.
        let rows = panel.rows(64, 20);
        let at = |needle: &str| {
            rows.iter()
                .find_map(|row| row.find(needle))
                .unwrap_or_else(|| panic!("{needle:?} is not on the panel: {rows:?}"))
        };
        assert_eq!(at("line-one"), 7);
        assert_eq!(at("xy!"), 7);
        assert_eq!(at("line-three"), 7);
        assert_eq!(panel.apply(Act::SelectUp, 64, 20), Answered::Redraw);
        assert_eq!(panel.draft(), None, "Shift+Up did not choose");
        assert!(panel
            .rows(64, 20)
            .iter()
            .any(|row| row == "    4) line-one"));
        panel.apply(Act::SelectDown, 64, 20);
        assert_eq!(panel.draft(), Some("line-one\nxy!\nline-three"));
        // And the answer goes back whole, line breaks and all.
        assert_eq!(
            submitted(&mut panel, &[Act::Submit, Act::Text('1')], 64, 20),
            vec!["line-one\nxy!\nline-three".to_string(), "Yes".to_string()]
        );
    }

    #[test]
    fn a_long_answer_wraps_onto_rows_under_its_first_column() {
        // e2e `tui-decision-prompts.test.ts:2701`: one word longer than the
        // field, cut by cell (`prefixByWidth`) onto continuation rows that
        // hang under the answer's first character; the caret's reversed
        // space on the last.
        let mut panel = captured();
        panel.apply(Act::Text('4'), 48, 20);
        let answer =
            "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789wraps-below";
        for character in answer.chars() {
            panel.apply(Act::Text(character), 48, 20);
        }
        let rows = panel.rows(48, 20);
        let first = rows
            .iter()
            .position(|row| row.starts_with("    4) "))
            .expect("the field's first row");
        let field: Vec<&String> = rows.iter().skip(first).take(3).collect();
        assert_eq!(field[0], &format!("    4) {}", &answer[..41]));
        assert_eq!(field[1], &format!("       {}", &answer[41..82]));
        assert_eq!(field[2], &format!("       {} ", &answer[82..]));
        let last = u16::try_from(first + 2).expect("a row");
        assert_eq!(panel.caret(48, 20), Some((last, 7 + 1)));
        assert!(rows.iter().all(|row| width(row) <= 48), "{rows:?}");
    }

    #[test]
    fn a_paste_goes_into_an_open_answer_and_nowhere_else() {
        // e2e `tui-decision-prompts.test.ts:2785` and `:2968`: into the field,
        // line breaks kept, CR/LF folded and a tab made a space
        // (`paste_framing.zig:119-127`); at a marked choice the paste is
        // dropped whole and selects nothing.
        let mut panel = captured();
        paste(&mut panel, "1\x1b[2Jdrop me", 120, 40);
        assert!(panel.hint_text(120, false).ends_with("Question 1 of 2"));
        assert_eq!(panel.draft(), None);
        panel.apply(Act::Text('4'), 120, 40);
        paste(&mut panel, "pasted with cmd-v\r\nand\tthis\x03", 120, 40);
        assert_eq!(panel.draft(), Some("pasted with cmd-v\nand this"));
        let rows = panel.rows(120, 40);
        assert!(
            rows.contains(&"    4) pasted with cmd-v".to_string()),
            "{rows:?}"
        );
        assert!(rows.contains(&"       and this ".to_string()), "{rows:?}");
        // A paste past the answer's budget is refused whole.
        paste(&mut panel, &"x".repeat(MAX_FREEFORM_ENCODED_BYTES), 120, 40);
        assert_eq!(panel.draft(), Some("pasted with cmd-v\nand this"));
    }

    #[test]
    fn an_answer_taller_than_its_share_of_the_screen_keeps_the_caret_row_shown() {
        // A third of the screen, ending at the caret's row (`field_window`):
        // upstream clips a field that tall at the band, which can take the
        // row being typed on with it.
        let mut panel = captured();
        panel.apply(Act::Text('4'), 40, 24);
        paste(
            &mut panel,
            &(0..20)
                .map(|n| format!("row{n}"))
                .collect::<Vec<_>>()
                .join("\n"),
            40,
            24,
        );
        let rows = panel.rows(40, 24);
        assert!(
            rows.iter().any(|row| row.trim_end() == "       row19"),
            "{rows:?}"
        );
        assert!(!rows.iter().any(|row| row.contains("row0")), "{rows:?}");
        assert!(panel.presents_choices(40, 24));
        let (row, _) = panel.caret(40, 24).expect("drafting");
        assert!(rows[usize::from(row)].contains("row19"), "{rows:?}");
        // Walking up moves the window with the caret.
        for _ in 0..19 {
            panel.apply(Act::Up, 40, 24);
        }
        let rows = panel.rows(40, 24);
        let (row, _) = panel.caret(40, 24).expect("drafting");
        assert!(rows[usize::from(row)].contains("row0"), "{rows:?}");
        assert_eq!(panel.rows(40, 24).len(), usize::from(panel.height(40, 24)));
    }

    #[test]
    fn ctrl_k_and_alt_backspace_reach_the_answer_as_its_kill_keys() {
        assert_eq!(Act::of(Action::ControlK), Some(Act::KillToEnd));
        assert_eq!(Act::of(Action::KillToEnd), Some(Act::KillToEnd));
        assert_eq!(
            Act::of(Action::DeleteWordBackward),
            Some(Act::DeleteWordLeft)
        );
    }

    #[test]
    fn the_answers_word_and_kill_keys_reach_it_and_nothing_at_a_choice() {
        let mut panel = captured();
        for act in [Act::WordLeft, Act::KillToEnd, Act::DeleteWordLeft] {
            assert_eq!(panel.apply(act, 120, 40), Answered::Nothing, "{act:?}");
        }
        panel.apply(Act::Text('4'), 120, 40);
        for character in "keep the last".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        panel.apply(Act::DeleteWordLeft, 120, 40);
        assert_eq!(panel.draft(), Some("keep the "));
        panel.apply(Act::WordLeft, 120, 40);
        panel.apply(Act::KillToEnd, 120, 40);
        assert_eq!(panel.draft(), Some("keep "));
        panel.apply(Act::KillToStart, 120, 40);
        assert_eq!(panel.draft(), Some(""));
    }

    #[test]
    fn delete_takes_the_grapheme_after_the_caret() {
        let mut panel = captured();
        panel.apply(Act::Text('4'), 120, 40);
        for character in "ab네c".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        panel.apply(Act::Home, 120, 40);
        panel.apply(Act::Right, 120, 40);
        panel.apply(Act::Right, 120, 40);
        panel.apply(Act::Delete, 120, 40);
        assert_eq!(panel.draft(), Some("abc"));
        assert_eq!(
            panel.apply(Act::Delete, 120, 40),
            Answered::Redraw,
            "a key the open answer binds owes a frame"
        );
    }

    #[test]
    fn an_armed_escape_takes_the_right_of_the_hint_row() {
        // The warning ends at the last cell, the keys are cut to leave it a
        // column of air, and the counter is what goes
        // (`input_presentation.zig:539-569`).
        let panel = captured();
        let hint = panel.hint_text(120, true);
        assert_eq!(width(&hint), 120);
        assert!(hint.starts_with("1–4 choose now"), "{hint}");
        assert!(hint.ends_with(" esc again to clear"), "{hint}");
        assert!(!hint.contains("Question"), "{hint}");
        assert!(panel
            .hint(120, true, &DARK)
            .ends_with("\u{1b}[38;5;245mesc again to clear\u{1b}[0m"));
    }

    #[test]
    fn a_description_takes_its_column_by_the_screens_width() {
        // `questionDescriptionColumn`: from 72 columns it starts at the 38th,
        // from 52 at the 30th, and below that it goes on a row of its own under
        // the label, in the label's indent.
        let panel = captured();
        let at = |cols: u16| panel.rows(cols, 40)[2].clone();
        assert_eq!(at(80).find("Relational"), Some(37));
        assert_eq!(at(72).find("Relational"), Some(37));
        assert_eq!(at(71).find("Relational"), Some(29));
        assert_eq!(at(52).find("Relational"), Some(29));
        let narrow = panel.rows(51, 40);
        assert_eq!(narrow[2], "    1) Postgres");
        assert_eq!(narrow[3], "       Relational, strong consistency");
        assert_eq!(narrow[4], "    2) SQLite");
        assert_eq!(narrow[5], "       Single file, embedded");
        assert_eq!(
            panel.painted(51, 40, &DARK)[3],
            "       \u{1b}[38;5;245mRelational, strong consistency\u{1b}[0m"
        );
    }

    #[test]
    fn a_long_label_and_its_description_wrap_side_by_side() {
        // e2e `tui-decision-prompts.test.ts:2625`: continuation rows aligned
        // under the label's first column and under the description's.
        let panel = QuestionPanel::new(QuestionRequest {
            id: QuestionId(6),
            entries: vec![QuestionEntry {
                question: "Which?".to_string(),
                options: vec![
                    QuestionOption {
                        label: "Run the complete verification suite before pushing".to_string(),
                        description: Some(
                            "Keep the entire explanation visible even when it wraps".to_string(),
                        ),
                    },
                    QuestionOption {
                        label: "Skip".to_string(),
                        description: None,
                    },
                ],
            }],
        });
        let rows = panel.rows(80, 40);
        // The label wraps short of the column (`38 - 2 - 7` cells), the
        // description inside it (`80 - 38`), starting one cell before the
        // column on every row -- or a cell after a label that reached it.
        assert_eq!(
            rows[2],
            "    1) Run the complete verification Keep the entire explanation visible even"
        );
        assert_eq!(
            rows[3],
            "       suite before pushing          when it wraps"
        );
        assert_eq!(rows[2].find("Keep"), rows[3].find("when"));
        assert_eq!(rows[4], "    2) Skip");
        assert!(rows.iter().all(|row| width(row) <= 80));
        // And the caret of the next choice is under both rows.
        let mut panel = panel;
        panel.apply(Act::Down, 80, 40);
        assert_eq!(panel.caret_row(80, 40), 4);
    }

    #[test]
    fn the_open_slot_is_its_ordinal_and_the_draft_in_the_marked_grey() {
        // `q1-down3` and `fx-question2-40x120-q-typed`: the slot's own row
        // becomes the field, and the caret is a reversed cell after the text
        // (`ESC[1mESC[38;5;255m    4) ESC[0;7m ESC[0m`; claim changed: it was
        // the terminal's caret alone).
        let mut panel = captured();
        panel.apply(Act::Text('4'), 120, 40);
        assert_eq!(panel.rows(120, 40)[5], "    4)  ");
        assert_eq!(
            panel.painted(120, 40, &DARK)[5],
            "\u{1b}[1;38;5;255m    4) \u{1b}[0m\u{1b}[7m \u{1b}[0m"
        );
        assert_eq!(panel.caret(120, 40), Some((5, 7)));
        for character in "MariaDB please".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        assert_eq!(panel.rows(120, 40)[5], "    4) MariaDB please ");
        assert_eq!(
            panel.painted(120, 40, &DARK)[5],
            "\u{1b}[1;38;5;255m    4) MariaDB please\u{1b}[0m\u{1b}[7m \u{1b}[0m"
        );
        assert_eq!(panel.caret(120, 40), Some((5, 7 + 14)));
        assert_eq!(panel.height(120, 40), 7, "the field is one row");
        // A caret inside the text reverses the character under it.
        panel.apply(Act::Home, 120, 40);
        assert_eq!(
            panel.painted(120, 40, &DARK)[5],
            "\u{1b}[1;38;5;255m    4) \u{1b}[0m\u{1b}[7mM\u{1b}[0m\u{1b}[1;38;5;255mariaDB please\u{1b}[0m"
        );
    }

    #[test]
    fn an_unmarked_slot_reads_as_what_was_typed_into_it() {
        // `questionOptionLabel` (`question_ui.zig:242-251`): `Other` until
        // something is typed, the draft after.
        let mut panel = captured();
        panel.apply(Act::Text('4'), 120, 40);
        assert!(panel.rows(120, 40)[5].ends_with("4)  "));
        panel.apply(Act::Up, 120, 40);
        assert_eq!(panel.rows(120, 40)[5], "    4) Other");
        panel.apply(Act::Down, 120, 40);
        for character in "Maria".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        panel.apply(Act::Up, 120, 40);
        assert_eq!(panel.rows(120, 40)[5], "    4) Maria");
        assert_eq!(
            panel.painted(120, 40, &DARK)[5],
            "\u{1b}[38;5;245m    4) Maria\u{1b}[0m"
        );
    }

    #[test]
    fn left_goes_back_a_question_and_right_goes_nowhere() {
        // `retreatToPrevious` and the inert Right
        // (`input_question_runtime.zig:186-195`).
        let mut panel = captured();
        assert_eq!(
            panel.apply(Act::Left, 120, 40),
            Answered::Nothing,
            "the first"
        );
        assert_eq!(panel.apply(Act::Right, 120, 40), Answered::Nothing);
        panel.apply(Act::Down, 120, 40);
        panel.apply(Act::Submit, 120, 40);
        assert!(panel.hint_text(120, false).ends_with("Question 2 of 2"));
        assert_eq!(panel.apply(Act::Right, 120, 40), Answered::Nothing);
        assert_eq!(panel.apply(Act::Left, 120, 40), Answered::Redraw);
        assert!(panel.hint_text(120, false).ends_with("Question 1 of 2"));
        assert!(
            panel.painted(120, 40, &DARK)[3].starts_with("\u{1b}[1;38;5;255m    2) SQLite"),
            "the answered question kept its marked choice"
        );
        // Inside a draft the same two keys are the caret's.
        panel.apply(Act::Text('4'), 120, 40);
        for character in "ab".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        assert_eq!(panel.apply(Act::Left, 120, 40), Answered::Redraw);
        assert!(panel.hint_text(120, false).ends_with("Question 1 of 2"));
        panel.apply(Act::Text('z'), 120, 40);
        assert_eq!(panel.draft(), Some("azb"));
    }

    #[test]
    fn alt_b_goes_back_a_question_at_a_choice_and_a_word_in_a_draft() {
        // `.cursor_left, .word_left => retreatToPrevious`, one page of the
        // batch back and none past the first, while the arrows' composer move,
        // word right and Right do nothing at a choice
        // (`input_question_runtime.zig:190-199`, `question_prompt.zig:609-615`
        // `@481104fd`; `fx-p5-question-word-left-40x120-{alt-b,ctrl-left,
        // alt-f,right,alt-b-first,draft-alt-b}`).
        let mut panel = batch(20);
        let page = |panel: &QuestionPanel| panel.hint_text(120, false);
        assert_eq!(panel.apply(Act::AltB, 120, 40), Answered::Nothing);
        // Page 3 left with its second choice marked, so the page Alt-b lands
        // on can be told from one that came back with the first.
        panel.apply(Act::Tab, 120, 40);
        panel.apply(Act::Tab, 120, 40);
        panel.apply(Act::Down, 120, 40);
        panel.apply(Act::Tab, 120, 40);
        assert!(page(&panel).ends_with("Question 4 of 20"));
        assert!(
            panel.painted(120, 40, &DARK)[2].starts_with("\u{1b}[1;38;5;255m    1) Keep 4"),
            "page 4 opens on its own first choice"
        );
        for act in [Act::WordLeft, Act::WordRight, Act::Right] {
            assert_eq!(panel.apply(act, 120, 40), Answered::Nothing, "{act:?}");
            assert!(page(&panel).ends_with("Question 4 of 20"), "{act:?}");
        }
        assert_eq!(panel.apply(Act::AltB, 120, 40), Answered::Redraw);
        assert!(
            page(&panel).ends_with("Question 3 of 20"),
            "one question back: {}",
            page(&panel)
        );
        let painted = panel.painted(120, 40, &DARK);
        assert!(
            painted[3].starts_with("\u{1b}[1;38;5;255m    2) Change 3")
                && painted[2].starts_with("\u{1b}[38;5;245m    1) Keep 3"),
            "the page it lands on keeps its own marked choice: {painted:?}"
        );
        panel.apply(Act::AltB, 120, 40);
        panel.apply(Act::AltB, 120, 40);
        assert!(page(&panel).ends_with("Question 1 of 20"));
        assert_eq!(
            panel.apply(Act::AltB, 120, 40),
            Answered::Nothing,
            "stops at the first"
        );
        // Inside a draft it is the caret's word move, on the same page.
        panel.apply(Act::Tab, 120, 40);
        panel.apply(Act::Text('3'), 120, 40);
        for character in "ab cd".chars() {
            panel.apply(Act::Text(character), 120, 40);
        }
        assert_eq!(panel.apply(Act::AltB, 120, 40), Answered::Redraw);
        panel.apply(Act::Text('z'), 120, 40);
        assert_eq!(panel.draft(), Some("ab zcd"));
        assert!(page(&panel).ends_with("Question 2 of 20"));
    }

    #[test]
    fn only_a_draft_with_text_in_it_is_what_escape_would_clear() {
        let mut panel = captured();
        assert!(!panel.holds_draft(), "no draft is open");
        assert!(!panel.clear_draft());
        panel.apply(Act::Text('4'), 120, 40);
        assert!(
            !panel.holds_draft(),
            "an empty draft is cancelled, not cleared"
        );
        panel.apply(Act::Text('x'), 120, 40);
        assert!(panel.holds_draft());
        assert!(panel.clear_draft());
        assert_eq!(panel.draft(), Some(""), "the slot stays marked");
        assert!(!panel.holds_draft());
    }

    #[test]
    fn the_document_keeps_each_question_with_its_answer_hung_under_it() {
        // `fx-question-40x120-q2-num` rows 5-8: the question under its batch
        // ordinal, the answer five columns in, in the answer grey -- the
        // indent painted with it (`question_ui.zig:465-501`).
        let panel = captured();
        let answers = vec!["SQLite".to_string(), "Yes".to_string()];
        assert_eq!(
            texts(panel.answered_with(&answers, 120, "", "")),
            vec![
                "  1) Which database should I use?",
                "     SQLite",
                "  2) Deploy now?",
                "     Yes",
            ]
        );
        // And each row says what it is, so a theme report repaints the
        // answer's grey as the answer's (`DocumentRow::Answer`).
        assert_eq!(
            panel.answered_rows(&answers, 120, &DARK),
            vec![
                (
                    "  1) Which database should I use?".to_string(),
                    DocumentRow::Plain
                ),
                (
                    "\u{1b}[38;5;245m     SQLite\u{1b}[0m".to_string(),
                    DocumentRow::Answer
                ),
                ("  2) Deploy now?".to_string(), DocumentRow::Plain),
                (
                    "\u{1b}[38;5;245m     Yes\u{1b}[0m".to_string(),
                    DocumentRow::Answer
                ),
            ]
        );
    }

    #[test]
    fn a_long_question_and_answer_hang_under_the_question_text() {
        // e2e `tui-decision-prompts.test.ts:2625-2694`: no ellipsis, every
        // continuation five columns in.
        let panel = QuestionPanel::new(one_entry(
            "When you ask fx to ask a question the question text must remain fully visible",
        ));
        let rows = texts(panel.answered_with(
            &["a freeform answer that is much too long for one row".to_string()],
            40,
            "",
            "",
        ));
        assert_eq!(
            rows,
            vec![
                "  1) When you ask fx to ask a question",
                "     the question text must remain fully",
                "     visible",
                "     a freeform answer that is much too",
                "     long for one row",
            ]
        );
    }

    #[test]
    fn an_answer_is_recorded_the_way_the_model_receives_it() {
        // The tool encodes a freeform answer before it is sent
        // (`crate::tools::question`'s `bounded`), and the record shows the
        // encoding rather than an invisible character the model was told about.
        let panel = QuestionPanel::new(one_entry("Proceed?"));
        assert_eq!(
            texts(panel.answered_with(&["a\u{200b}b".to_string()], 80, "", ""))[1],
            "     a\\u{200b}b"
        );
        // An empty freeform answer is still a row.
        assert_eq!(
            texts(panel.answered_with(&[String::new()], 80, "", "")),
            vec!["  1) Proceed?", "     "]
        );
        // A line break in an answer is a row break in the record, hung like
        // the rest (`writeResolutionField`, `question_ui.zig:478-496`).
        assert_eq!(
            texts(panel.answered_with(&["one\ntwo".to_string()], 80, "", "")),
            vec!["  1) Proceed?", "     one", "     two"]
        );
    }

    #[test]
    fn the_height_the_window_is_chosen_by_is_the_height_that_is_painted() {
        // `shape` counts the open field as one row without wrapping the draft
        // (`option_height`); the paint wraps it. The two have to agree on every
        // kind of choice, or the band is solved for a block of one height and
        // painted with another.
        let mut panel = captured();
        let counted = |panel: &QuestionPanel, cols: u16, rows: u16| {
            let entry = panel.entry().expect("a question");
            let shape = panel.shape(cols, rows);
            title_rows(&entry.question, cols, Pens::PLAIN).len()
                + 2
                + (shape.top..shape.top + shape.visible)
                    .map(|index| option_height(entry, index, cols, rows))
                    .sum::<usize>()
        };
        for cols in [20, 40, 51, 60, 80, 120] {
            assert_eq!(counted(&panel, cols, 40), panel.rows(cols, 40).len());
        }
        panel.apply(Act::Text('4'), 120, 40);
        for _ in 0..300 {
            panel.apply(Act::Text('w'), 120, 40);
        }
        for cols in [20, 40, 51, 60, 80, 120] {
            assert_eq!(counted(&panel, cols, 40), panel.rows(cols, 40).len());
        }
        // Unmarked, the slot reads as the draft and wraps like a label.
        panel.apply(Act::Up, 120, 40);
        for cols in [20, 40, 51, 60, 80, 120] {
            assert_eq!(counted(&panel, cols, 40), panel.rows(cols, 40).len());
        }
        assert!(panel.rows(40, 40).len() > captured().rows(40, 40).len());
    }

    #[test]
    fn a_refused_batch_leaves_one_row() {
        // `fx-question2-40x120-q2-esc` row 14: the glyph in the outcome grey,
        // the word in none (`question_ui.zig:513-519`).
        assert_eq!(cancelled_row(&DARK), "\u{1b}[38;5;252m▪\u{1b}[0m Cancelled");
        let light = Palette {
            mode: super::super::theme::Mode::Light,
        };
        assert_eq!(
            cancelled_row(&light),
            "\u{1b}[38;5;238m▪\u{1b}[0m Cancelled"
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
        ) -> QuestionReply {
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
        assert_eq!(
            answers,
            QuestionReply::Answered(vec!["Thorough".to_string()])
        );
    }

    #[test]
    fn a_stale_answer_or_a_stale_approval_is_discarded_and_the_wait_continues() {
        let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx
                .send(TurnControl::Answer {
                    id: super::super::approval_readiness::ApprovalId(1),
                    answer: ApprovalAnswer::Deny,
                    feedback: None,
                })
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
        assert_eq!(answers, QuestionReply::Answered(vec!["fresh".to_string()]));
    }

    #[test]
    fn a_model_change_at_the_models_question_is_put_aside_and_the_batch_stays_up() {
        // `/model` while the model's question batch waits: not the user
        // cancelling the batch, so the wait goes on to the batch's own answer,
        // and the change waits for the loop.
        let mut harness = Harness::new(4);
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
        let sent = change.clone();
        let answers = harness.request_while(move |events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx.send(TurnControl::Model(sent)).unwrap();
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: QuestionId(1),
                    answers: vec!["kept".into()],
                })
                .unwrap();
        });
        assert_eq!(answers, QuestionReply::Answered(vec!["kept".to_string()]));
        assert_eq!(
            harness.control.take_deferred(),
            vec![super::super::approval::Deferred::Model(change)]
        );
        assert_eq!(harness.control.waiting(), None);
    }

    #[test]
    fn an_undo_at_the_models_question_is_put_aside_and_the_batch_stays_up() {
        // `/undo` while the model's question batch waits: not the user
        // cancelling the batch -- the catch-all below the arm would read it as
        // an interrupt -- so the wait goes on to the batch's own answer, and
        // the undo waits for the loop.
        let mut harness = Harness::new(4);
        let answers = harness.request_while(move |events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx
                .send(TurnControl::Undo {
                    through: crate::tools::undo::Mark::LATEST,
                })
                .unwrap();
            control_tx
                .send(TurnControl::QuestionAnswer {
                    id: QuestionId(1),
                    answers: vec!["kept".into()],
                })
                .unwrap();
        });
        assert_eq!(answers, QuestionReply::Answered(vec!["kept".to_string()]));
        assert_eq!(
            harness.control.take_deferred(),
            vec![super::super::approval::Deferred::Undo {
                through: crate::tools::undo::Mark::LATEST,
                after_reset: false
            }]
        );
        assert_eq!(harness.control.waiting(), None);
    }

    #[test]
    fn the_panels_own_cancellation_answers_with_no_answer() {
        let answers = Harness::new(4).request_while(|events_rx, control_tx, _cancel| {
            shown(events_rx);
            control_tx
                .send(TurnControl::QuestionCancelled { id: QuestionId(1) })
                .unwrap();
        });
        assert_eq!(
            answers,
            QuestionReply::Declined,
            "the panel's own refusal is the cancellation sentinel"
        );
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
            assert_eq!(
                answers,
                QuestionReply::Interrupted,
                "a stopped turn's question is aborted, never declined"
            );
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
        assert_eq!(answers, QuestionReply::Interrupted);
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
        assert_eq!(answers, QuestionReply::Interrupted);
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
        assert_eq!(
            harness.questioner.request(&entries()),
            QuestionReply::Declined,
            "with no UI to ask, nobody declined and nobody stopped anything"
        );
    }

    #[test]
    fn a_full_ui_channel_makes_the_question_wait_rather_than_be_dropped() {
        let mut harness = Harness::new(1);
        // Occupied **before** the request, so `send_ui` must wait for room. A
        // `blocking_recv` on an empty channel here would deadlock instead.
        harness
            .events_tx
            .blocking_send(UiEvent::Notice(super::super::notice::Notice::neutral(
                "",
                "holding the one permit",
            )))
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
        assert_eq!(answers, QuestionReply::Answered(vec!["ok".to_string()]));
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
            .blocking_send(UiEvent::Notice(super::super::notice::Notice::neutral(
                "",
                "holding the one permit",
            )))
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
            answers,
            QuestionReply::Interrupted,
            "a question the session ended under before it could be shown is an aborted one"
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
            QuestionReply::Declined,
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
