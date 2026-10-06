//! What a vector of terminal output *does*, decoded before it is written.
//!
//! Every emitter in this module tree builds one immutable vector and hands it
//! to a writer. This module is the question asked in between: **does that
//! vector, decoded against a model of the terminal, produce the state its
//! emitter says it wants?** A vector whose decoded effect disagrees with its
//! emitter's declared intent is refused, and refused *before* the write -- so
//! the disagreement costs an error the existing failure budget already knows
//! how to count, rather than a screen nobody can describe.
//!
//! Three properties make the answer worth having, and each is a decision:
//!
//! * **The bytes checked are the bytes written.** No emitter reconstructs a
//!   copy for the checker: the vector is assembled once, checked, and that same
//!   slice is the one emitted. Checking a copy would verify the copy.
//! * **The intent is computed from the emitter's inputs and the geometry, never
//!   from the decoded bytes.** An intent read out of the output would agree
//!   with it by construction.
//! * **Cell equality is semantic, not [`Cell`]'s.** `Cell::eq` compares
//!   `SgrState::reopen()`, which is the very function the painter emits with --
//!   emitter, applier and comparison would be one function, and a defect in it
//!   would cancel against itself. Here the expected side reads the stored
//!   foreground slot ([`super::pacer::SgrState::color`]) and the wire side
//!   decodes the emitted parameters, two parsers written apart on purpose.
//!
//! **What this module does not claim.** Nothing about bytes that left: a check
//! that passes says the vector was right, not that it arrived. Partial writes,
//! recovery and retention are a later unit's, and no function here touches the
//! transport, the adoption of a landed frame, or a signal path.
//!
//! The model is **per vector and holds nothing between them**. It is seeded
//! from state the product already maintains -- the shadow, the caret it last
//! placed, the title it last told the terminal -- and dropped when the vector
//! has been decoded. So there is no standing claim about a terminal this
//! process shares with whatever else can write to it.

use std::fmt;
use std::io;
use std::ops::RangeInclusive;

use unicode_segmentation::UnicodeSegmentation;

use super::grid::{Cell, Grid, Placed};

/// A foreground colour, as a value rather than as the bytes some painter spelt
/// it with.
///
/// **`Indexed` is not resolved to the theme's RGB.** Resolving would make the
/// comparison theme-dependent and would smuggle the palette back into a check
/// whose whole point is to be independent of it: the painter emitted an index,
/// so an index is what is compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// A stored attribute slot that is not one of the three shapes this crate
/// emits, returned rather than guessed at.
///
/// It is a rejection and never a skip: an unknown attribute in a cell is a cell
/// nothing can say the colour of, and saying nothing is how a colour defect
/// stays invisible.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MalformedSlot;

/// Why a vector was refused.
///
/// **It carries names and numbers only.** No row text, no title, no `OSC`
/// payload, no grapheme: a rejection travels into an error message and a log,
/// and the rows of this screen are the user's and a tool's text while the title
/// carries a model id.
#[derive(Debug, Clone)]
pub(crate) struct Reject {
    /// What kind of thing disagreed, as a fixed name from this file.
    shape: &'static str,
    /// How far into the vector the decoder had reached.
    at: usize,
    /// The cell the disagreement is about, when it is about one.
    cell: Option<(u16, u16)>,
}

impl Reject {
    fn new(shape: &'static str, at: usize) -> Self {
        Self {
            shape,
            at,
            cell: None,
        }
    }

    fn at_cell(shape: &'static str, at: usize, row: u16, column: u16) -> Self {
        Self {
            shape,
            at,
            cell: Some((row, column)),
        }
    }

    /// The name of what disagreed, for a test that wants to pin *which* rule
    /// fired rather than merely that one did.
    #[cfg(test)]
    pub(crate) fn shape(&self) -> &'static str {
        self.shape
    }
}

impl fmt::Display for Reject {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "output check refused {} at byte {}",
            self.shape, self.at
        )?;
        if let Some((row, column)) = self.cell {
            write!(out, " (row {row}, column {column})")?;
        }
        Ok(())
    }
}

impl From<MalformedSlot> for Reject {
    fn from(MalformedSlot: MalformedSlot) -> Self {
        Reject::new("an attribute slot this crate does not emit", 0)
    }
}

impl From<MalformedSlot> for io::Error {
    fn from(slot: MalformedSlot) -> Self {
        Reject::from(slot).into()
    }
}

impl From<Reject> for io::Error {
    /// A refusal is an `io::Error` because that is what every caller already
    /// routes into the frame budget. It introduces no new failure policy: a
    /// refused vector is counted exactly as a refused write is.
    fn from(reject: Reject) -> Self {
        io::Error::other(reject.to_string())
    }
}

/// A grapheme cluster, as one modelled cell holds it.
///
/// **Inline when it is short**, which nearly every cluster is, and on the heap
/// when it is not. Replaying a long append puts a cluster in a cell for every
/// column the vector writes and every column the declaration expects, and
/// drops them all again as the rows scroll away; held as a `String`, each one
/// was an allocation and a free, and together they were the largest single
/// cost of the replay.
///
/// It is the text and nothing else. Two glyphs are equal when their bytes are,
/// however each is held -- which is exactly what comparing two `String`s was.
#[derive(Clone)]
enum Glyph {
    Inline { len: u8, bytes: [u8; Glyph::INLINE] },
    Heap(String),
}

impl Glyph {
    /// The longest cluster held inline, in bytes. Fifteen keeps a cell the size
    /// a `String` made it, and holds a letter with its marks, a flag, a keycap
    /// and a toned emoji; a ZWJ family of three is eighteen and goes to the
    /// heap.
    const INLINE: usize = 15;

    fn new(text: &str) -> Self {
        let mut bytes = [0u8; Self::INLINE];
        match (bytes.get_mut(..text.len()), u8::try_from(text.len())) {
            (Some(slot), Ok(len)) => {
                slot.copy_from_slice(text.as_bytes());
                Self::Inline { len, bytes }
            }
            _ => Self::Heap(text.to_string()),
        }
    }

    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Inline { len, bytes } => &bytes[..usize::from(*len)],
            Self::Heap(text) => text.as_bytes(),
        }
    }

    /// The cluster with `more` joined to the end of it, as a combining mark
    /// joins the cluster in front of it -- moving to the heap if it no longer
    /// fits.
    fn push_str(&mut self, more: &str) {
        match self {
            Self::Heap(text) => text.push_str(more),
            Self::Inline { len, bytes } => {
                let start = usize::from(*len);
                let end = start + more.len();
                if let (Some(slot), Ok(grown)) = (bytes.get_mut(start..end), u8::try_from(end)) {
                    slot.copy_from_slice(more.as_bytes());
                    *len = grown;
                    return;
                }
                // Whole text went in, so whole UTF-8 is what comes out.
                let held = std::str::from_utf8(&bytes[..start]).expect("a glyph built from text");
                let mut text = String::with_capacity(end);
                text.push_str(held);
                text.push_str(more);
                *self = Self::Heap(text);
            }
        }
    }
}

impl PartialEq for Glyph {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for Glyph {}

impl fmt::Debug for Glyph {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&String::from_utf8_lossy(self.as_bytes()), out)
    }
}

/// One column of the modelled screen.
///
/// A cell the model was seeded with but did not author is [`Foreign`](Self::Foreign),
/// carrying the linear index it had **at seed time**. The address moves when the
/// screen scrolls and the token does not, which is what lets [`preserve`] ask
/// the only question worth asking about somebody else's cell: *is it still the
/// same one, at the address the declared scroll puts it?*
///
/// The scope of that proof is [`preserve`]'s and is stated there: displacement
/// by the declared scroll, and disappearance only where the declaration
/// licenses it. It is not a replay of the order in which the vector worked.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CellState {
    Empty,
    Lead {
        grapheme: Glyph,
        width: u8,
        color: Color,
    },
    Continuation,
    Foreign(u32),
}

impl CellState {
    /// The modelled cell one of the product's own grid cells describes.
    fn of(cell: &Cell) -> Result<Self, MalformedSlot> {
        Ok(match cell {
            Cell::Empty => Self::Empty,
            Cell::Continuation => Self::Continuation,
            Cell::Lead {
                grapheme,
                width,
                sgr,
            } => Self::Lead {
                grapheme: Glyph::new(grapheme),
                width: *width,
                color: sgr.color()?,
            },
        })
    }

    /// Whether a terminal would be showing the same thing in both.
    ///
    /// A space and an untouched cell are one thing on a screen and the painter
    /// spells one as the other; everything else compares exactly, tokens
    /// included.
    ///
    /// The first two arms are the general rule's own answer for the two pairs
    /// nearly every comparison is made of, reached without building the shown
    /// form of either side: two untouched cells show the same space, and two
    /// leads show the same thing exactly when their fields agree. Every other
    /// pair goes the general way.
    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) => true,
            (
                Self::Lead {
                    grapheme,
                    width,
                    color,
                },
                Self::Lead {
                    grapheme: other_grapheme,
                    width: other_width,
                    color: other_color,
                },
            ) => width == other_width && color == other_color && grapheme == other_grapheme,
            _ => match (self.shown(), other.shown()) {
                (Some(shown), Some(other)) => shown == other,
                _ => self == other,
            },
        }
    }

    /// What a terminal would be showing here, with a plain space and an
    /// untouched cell being the same thing -- which they are on a screen, and
    /// the painter relies on it: [`Grid::diff`] spells an empty target cell as
    /// a space. The text is given as its bytes, which compare exactly as the
    /// text does.
    fn shown(&self) -> Option<(&[u8], u8, Color)> {
        match self {
            Self::Empty => Some((b" ", 1, Color::Default)),
            Self::Lead {
                grapheme,
                width,
                color,
            } => Some((grapheme.as_bytes(), *width, *color)),
            Self::Continuation | Self::Foreign(_) => None,
        }
    }
}

/// One screen's cells.
///
/// Row-major and `rows * cols` long, but **read through a rotating origin**:
/// [`top`](Self::top) is where row one currently begins. A scroll then costs one
/// row rather than a whole screen, which is what makes an append of thousands of
/// rows -- every one of them compared as it leaves the top -- linear in the rows
/// it delivers instead of quadratic. Nothing outside this type may index
/// [`cells`](Self::cells) directly; [`index`](Self::index),
/// [`logical`](Self::logical) and [`row`](Self::row) are the doors.
///
/// **A row never wraps.** The plane is a whole number of rows and the origin
/// only ever moves by a whole row, so `top` is always a multiple of `cols` and
/// every row is one contiguous run of [`cells`](Self::cells) -- which is what
/// lets [`row`](Self::row) hand one out as a slice.
#[derive(Debug, Clone)]
struct Plane {
    rows: u16,
    cols: u16,
    /// Where row one begins in [`cells`](Self::cells).
    top: usize,
    cells: Vec<CellState>,
}

impl Plane {
    fn blank(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            top: 0,
            cells: vec![CellState::Empty; usize::from(rows) * usize::from(cols)],
        }
    }

    /// One cell by its **logical** position: zero-based, row-major, as if the
    /// screen had never scrolled.
    fn logical(&self, at: usize) -> &CellState {
        let len = self.cells.len();
        &self.cells[(self.top + at) % len.max(1)]
    }

    /// A screen whose every cell is somebody else's, tokenized by its
    /// **zero-based** linear index at seed time.
    fn foreign(rows: u16, cols: u16) -> Self {
        let count = usize::from(rows) * usize::from(cols);
        Self {
            rows,
            cols,
            top: 0,
            cells: (0..count)
                .map(|at| CellState::Foreign(u32::try_from(at).unwrap_or(u32::MAX)))
                .collect(),
        }
    }

    /// The model of what `shadow` claims the terminal is holding, on a screen
    /// of `rows` x `cols`.
    ///
    /// **A cell the shadow does not reach is foreign, not blank.** A band that
    /// has painted nothing keeps a `0x0` shadow, and a band whose screen has
    /// grown keeps one of the old size; in both the product's own claim stops
    /// where the shadow does, and a model that filled the rest with blanks
    /// would be asserting an emptiness nothing established.
    fn from_shadow(shadow: &Grid, rows: u16, cols: u16) -> Result<Self, MalformedSlot> {
        let mut plane = Self::foreign(rows, cols);
        for row in 1..=shadow.rows().min(rows) {
            for column in 1..=shadow.cols().min(cols) {
                let Some(cell) = shadow.cell(row, column) else {
                    continue;
                };
                let at = plane.index(row, column).expect("a cell of this plane");
                plane.cells[at] = CellState::of(cell)?;
            }
        }
        Ok(plane)
    }

    fn index(&self, row: u16, column: u16) -> Option<usize> {
        if row == 0 || column == 0 || row > self.rows || column > self.cols {
            return None;
        }
        let len = self.cells.len();
        if len == 0 {
            return None;
        }
        let at = usize::from(row - 1) * usize::from(self.cols) + usize::from(column - 1);
        Some((self.top + at) % len)
    }

    /// One whole row, by its one-based number, as the run of cells it is.
    fn row(&self, row: u16) -> Option<&[CellState]> {
        let start = self.index(row, 1)?;
        self.cells.get(start..start + usize::from(self.cols))
    }

    fn row_mut(&mut self, row: u16) -> Option<&mut [CellState]> {
        let start = self.index(row, 1)?;
        self.cells.get_mut(start..start + usize::from(self.cols))
    }

    /// Blanks `[column ..]` of one row, and the orphaned lead of a wide cluster
    /// the erase begins inside.
    fn erase_from(&mut self, row: u16, column: u16) {
        if column == 0 || column > self.cols {
            return;
        }
        let Some(cells) = self.row_mut(row) else {
            return;
        };
        let start = usize::from(column - 1);
        if start > 0 && matches!(cells[start], CellState::Continuation) {
            cells[start - 1] = CellState::Empty;
        }
        for cell in &mut cells[start..] {
            *cell = CellState::Empty;
        }
    }

    /// Moves every row up by one and blanks the row that frees.
    ///
    /// The top row's cells **leave the plane** -- which is what a linefeed on
    /// the bottom margin does, and why a foreign token that survived a declared
    /// scroll is a defect rather than a comfort.
    fn scroll_up(&mut self) {
        if self.rows == 0 || self.cols == 0 {
            return;
        }
        let width = usize::from(self.cols);
        let len = self.cells.len();
        // The row that is leaving becomes the row the scroll frees at the
        // bottom, blanked; the origin then moves past it. One row of work,
        // whatever the screen's height -- a `drain` from the front would move
        // every remaining cell on every linefeed, and an append delivering
        // thousands of rows makes one linefeed per row.
        debug_assert_eq!(self.top % width, 0, "an origin inside a row");
        for cell in &mut self.cells[self.top..self.top + width] {
            *cell = CellState::Empty;
        }
        self.top = (self.top + width) % len;
    }
}

/// Which of the two buffers the modelled bytes are landing on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaneKind {
    Primary,
    Alternate,
}

/// The terminal modes this crate sets and gives back.
///
/// A record of *state*, written out by hand at each declaration site rather
/// than parsed out of [`super::term::MODE_SET`]: an expectation read from the
/// constant the vector is built from would agree with it whatever either said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModeSet {
    /// `CSI > 4 ; N m`.
    modify_other_keys: Option<u8>,
    /// The kitty keyboard stack, pushed by `CSI > 1 u` and popped by `CSI < u`.
    kitty_keyboard: Option<bool>,
    /// `CSI ? 2004 h/l`.
    bracketed_paste: Option<bool>,
    /// `CSI ? 7 h/l`.
    autowrap: Option<bool>,
    /// `CSI ? 2031 h/l`, the theme-change subscription
    /// ([`super::term::MODE_SET`]).
    theme_notifications: Option<bool>,
}

impl ModeSet {
    /// **What nothing has said yet**, which is what every seed starts from.
    ///
    /// `None` per mode rather than a guess at the terminal's defaults, and the
    /// difference is the whole of what it buys: a band frame declares no mode
    /// at all, so *any* mode sequence inside one moves a field away from
    /// "nothing has said" and is refused -- including the ones whose value
    /// happens to match what a fresh terminal would have had. A seed that
    /// guessed `autowrap: true` would have let a frame put autowrap back on,
    /// which is what makes a band row at the last column scroll the document.
    pub(crate) fn fresh() -> Self {
        Self {
            modify_other_keys: None,
            kitty_keyboard: None,
            bracketed_paste: None,
            autowrap: None,
            theme_notifications: None,
        }
    }

    /// What [`super::term::MODE_SET`] leaves behind, spelled out.
    pub(crate) fn announced(tmux: bool) -> Self {
        Self {
            modify_other_keys: Some(2),
            // The kitty push breaks key input under tmux and is not written
            // there (`term::MODE_SET_TMUX`), so under tmux nothing has said --
            // which is a different statement from "it is off".
            kitty_keyboard: if tmux { None } else { Some(true) },
            bracketed_paste: Some(true),
            autowrap: Some(false),
            // Written in both sets: unlike the kitty push there is no evidence
            // it breaks anything under tmux, and a terminal without the mode
            // ignores it (`super::term::MODE_SET`).
            theme_notifications: Some(true),
        }
    }

    /// What [`super::term::RESTORE`] leaves behind.
    ///
    /// The tmux restore carries no `<u` for the push it never made, so the
    /// keyboard stack there is still what nothing has said.
    pub(crate) fn restored(tmux: bool) -> Self {
        Self {
            modify_other_keys: Some(0),
            kitty_keyboard: if tmux { None } else { Some(false) },
            bracketed_paste: Some(false),
            autowrap: Some(true),
            // **`Some(false)`, not `None`.** The subscription was made in both
            // sets, so it is given back in both restores, and a restore that
            // merely said nothing about it would pass while leaving the user's
            // next program reading theme reports off its own input.
            theme_notifications: Some(false),
        }
    }
}

/// A query this crate writes, in the order the terminal is meant to parse them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryId {
    /// `OSC 11 ; ?`, the background colour ([`super::theme::QUERY`]).
    Background,
    /// `CSI ? 996 n`, which way round the terminal is now
    /// ([`super::theme::MODE_QUERY`]).
    ThemeMode,
    /// `CSI 6 n`, the cursor report.
    CursorPosition,
}

/// Where the caret is, or that nothing can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Caret {
    Known((u16, u16)),
    Unknown,
}

/// What one thing a vector did to the cells was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Did {
    Erase,
    Place,
    Scroll,
}

/// One thing a vector did to the cells, **and which buffer it did it on**.
///
/// The plane is not decoration: the two planes share one row-number space, so a
/// footprint spelt in row numbers alone cannot tell "row 1 of the surface this
/// question is painting" from "row 1 of the screen the terminal is about to save
/// and hand back". A vector that wrote on the normal buffer before taking the
/// borrowed one would satisfy every row range and leave a cell on the user's own
/// screen that nothing in this phase repaints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Effect {
    plane: PlaneKind,
    did: Did,
    row: u16,
}

/// A modelled terminal: one vector's worth, and no more.
#[derive(Debug, Clone)]
pub(crate) struct TerminalModel {
    primary: Plane,
    /// `Some` only while `?1049` is set.
    alternate: Option<Plane>,
    /// The slot `?1049h` fills and `?1049l` drains.
    saved_cursor: Option<(u16, u16)>,
    plane: PlaneKind,
    caret: Caret,
    /// `None` until this vector has said, which is what the seed starts at: a
    /// session that has written nothing cannot claim the terminal's cursor is
    /// showing, and the emitters that care about it all spell it out.
    cursor_visible: Option<bool>,
    /// `?2026` is DECSET/DECRST -- set or reset, with no depth to it.
    sync_open: bool,
    shown_title: Option<String>,
    /// `CSI 22 ; 2 t` pushes and `CSI 23 ; 2 t` pops.
    title_stack: u8,
    modes: ModeSet,
    /// `ED 3`, which no grid models further.
    scrollback_erased: bool,
    queries: Vec<QueryId>,
    /// What the last SGR left switched on, and therefore what the next cell is
    /// painted in.
    pen: Color,
    /// What this vector did, in order.
    effects: Vec<Effect>,
    /// Which buffer each `?1049` in this vector moved the terminal **to**, in
    /// order.
    ///
    /// The final plane alone is not the question: a vector that took the
    /// borrowed buffer and gave it back has saved and restored the user's
    /// screen in between, and one that did it twice has overwritten the save
    /// slot with a screen of its own. The path is declared, so the path is
    /// compared.
    moves: Vec<PlaneKind>,
}

impl TerminalModel {
    fn new(primary: Plane, plane: PlaneKind, modes: ModeSet, title_stack: u8) -> Self {
        let (rows, cols) = (primary.rows, primary.cols);
        Self {
            primary,
            alternate: match plane {
                PlaneKind::Alternate => Some(Plane::blank(rows, cols)),
                PlaneKind::Primary => None,
            },
            saved_cursor: None,
            plane,
            caret: Caret::Unknown,
            cursor_visible: None,
            sync_open: false,
            shown_title: None,
            title_stack,
            modes,
            scrollback_erased: false,
            queries: Vec::new(),
            pen: Color::Default,
            effects: Vec::new(),
            moves: Vec::new(),
        }
    }

    /// The seed a band's own frames are checked against: what its shadow claims
    /// is on the screen, where it last put the caret, and what it last told the
    /// terminal the title was.
    ///
    /// **Taken at the emitter's entry**, before it plans: `commit` moves
    /// `painted` and `restore_primary` clears `shown_title` while building, so
    /// a seed read afterwards would compare a vector against state the vector
    /// itself moved.
    pub(crate) fn seed_primary(
        shadow: &Grid,
        rows: u16,
        cols: u16,
        caret: Option<(u16, u16)>,
        shown_title: Option<&str>,
        plane: PlaneKind,
    ) -> Result<Self, MalformedSlot> {
        // A frame writes no mode sequence, so the modes it is seeded with are
        // the ones no frame intent compares: the session's own mode set is
        // `announce`'s to declare and the exit's to give back.
        let mut model = Self::new(
            Plane::from_shadow(shadow, rows, cols)?,
            plane,
            ModeSet::fresh(),
            1,
        );
        model.caret = caret.map_or(Caret::Unknown, Caret::Known);
        model.shown_title = shown_title.map(str::to_string);
        Ok(model)
    }

    /// The same seed, with the borrowed plane holding what the band's own cache
    /// says is on it.
    ///
    /// It is what makes a repaint the screen already holds checkable at all: a
    /// vector of no bytes is compared against the surface the band believes is
    /// up, rather than against a blank plane it would disagree with.
    pub(crate) fn holding(
        mut self,
        surface: &Grid,
        caret: (u16, u16),
    ) -> Result<Self, MalformedSlot> {
        let (rows, cols) = (self.primary.rows, self.primary.cols);
        self.alternate = Some(Plane::from_shadow(surface, rows, cols)?);
        self.caret = Caret::Known(caret);
        Ok(self)
    }

    /// A seed for a screen this process did not author: every cell foreign,
    /// every token derived from its own address.
    pub(crate) fn seed_foreign(rows: u16, cols: u16, caret: Option<(u16, u16)>) -> Self {
        let mut model = Self::new(
            Plane::foreign(rows, cols),
            PlaneKind::Primary,
            ModeSet::fresh(),
            0,
        );
        model.caret = caret.map_or(Caret::Unknown, Caret::Known);
        model
    }

    /// A seed for the vectors that carry no cells: the mode set and the exit's
    /// restore.
    pub(crate) fn seed_modes(
        rows: u16,
        cols: u16,
        modes: ModeSet,
        title_stack: u8,
        plane: PlaneKind,
    ) -> Self {
        Self::new(Plane::foreign(rows, cols), plane, modes, title_stack)
    }

    /// The plane the modelled bytes are landing on.
    fn screen(&mut self) -> &mut Plane {
        match self.plane {
            PlaneKind::Primary => &mut self.primary,
            PlaneKind::Alternate => self.alternate.get_or_insert_with(|| {
                let (rows, cols) = (self.primary.rows, self.primary.cols);
                Plane::blank(rows, cols)
            }),
        }
    }

    /// Records what this vector just did, on the buffer it is on.
    fn record(&mut self, did: Did, row: u16) {
        let plane = self.plane;
        self.effects.push(Effect { plane, did, row });
    }

    /// Either buffer, by name.
    fn plane_of(&self, plane: PlaneKind) -> &Plane {
        match (plane, &self.alternate) {
            (PlaneKind::Alternate, Some(alternate)) => alternate,
            _ => &self.primary,
        }
    }

    fn screen_ref(&self) -> &Plane {
        match (self.plane, &self.alternate) {
            (PlaneKind::Alternate, Some(plane)) => plane,
            _ => &self.primary,
        }
    }
}

/// A row range a vector may address, and how far it may scroll.
///
/// Ordering is part of it rather than decoration: a vacated document row must
/// be erased **before** the scroll that would otherwise carry it into native
/// scrollback, where nothing can take it back.
#[derive(Debug, Clone)]
pub(crate) enum Seg {
    Erase(RangeInclusive<u16>),
    Place(RangeInclusive<u16>),
    /// A count rather than a row number, and `u32` rather than `u16` for the
    /// reason `render_append` counts its rows in `usize`: how many rows an
    /// append carries is a property of the text, and one submission wrapped on
    /// a narrow screen is well past a `u16`.
    Scroll {
        rows: u32,
    },
}

/// How a vector may move the terminal between its two buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaneMove {
    /// It stays on the buffer it started on and never leaves it.
    Stay,
    /// It takes the borrowed buffer, once (`?1049h`).
    Take,
    /// It gives the borrowed buffer back, once (`?1049l`).
    Give,
}

impl PlaneMove {
    /// The buffers a vector making this move ends each transition on.
    fn path(self) -> &'static [PlaneKind] {
        match self {
            Self::Stay => &[],
            Self::Take => &[PlaneKind::Alternate],
            Self::Give => &[PlaneKind::Primary],
        }
    }
}

/// Every row a vector may touch, on which buffer, and every scroll it may make.
#[derive(Debug, Clone)]
pub(crate) struct Footprint {
    segs: Vec<Seg>,
    /// The buffer the rows above are rows **of**. An effect on the other one is
    /// refused whatever its row number.
    plane: PlaneKind,
    moves: PlaneMove,
}

impl Footprint {
    /// A vector that may touch no cell at all.
    pub(crate) fn none(plane: PlaneKind) -> Self {
        Self {
            segs: Vec::new(),
            plane,
            moves: PlaneMove::Stay,
        }
    }

    pub(crate) fn new(plane: PlaneKind, segs: Vec<Seg>) -> Self {
        Self {
            segs,
            plane,
            moves: PlaneMove::Stay,
        }
    }

    /// The same footprint, for a vector that changes buffers on its way.
    pub(crate) fn moving(mut self, moves: PlaneMove) -> Self {
        self.moves = moves;
        self
    }

    fn scroll_rows(&self) -> u32 {
        self.segs
            .iter()
            .map(|seg| match seg {
                Seg::Scroll { rows } => *rows,
                _ => 0,
            })
            .sum()
    }

    fn may_erase(&self, row: u16) -> bool {
        self.segs.iter().any(|seg| match seg {
            // A placement erases the tail of the row it writes, so a row a
            // vector may place on is a row it may erase.
            Seg::Erase(range) | Seg::Place(range) => range.contains(&row),
            Seg::Scroll { .. } => false,
        })
    }

    fn may_place(&self, row: u16) -> bool {
        self.segs.iter().any(|seg| match seg {
            Seg::Place(range) => range.contains(&row),
            Seg::Erase(_) | Seg::Scroll { .. } => false,
        })
    }
}

/// What an emitter says its vector is for.
///
/// Every variant is built from the emitter's own inputs and the geometry, never
/// from the bytes: an intent derived from the output would agree with it by
/// construction.
#[derive(Debug)]
pub(crate) enum Intent<'a> {
    /// A band frame on the normal buffer: these cells, this caret, this title.
    ///
    /// The caret is an `Option` for one emitter and one reason: an append
    /// leaves it wherever its last placement put it, and the band records
    /// exactly that by forgetting it (`frame.rs`'s `caret = None`, "the next
    /// frame owes a `CUP`"). A `None` here is that declaration -- *this vector
    /// makes no claim about where the caret ends up* -- rather than a check
    /// that was skipped: every emitter that does place the caret declares it.
    Primary {
        grid: &'a Grid,
        caret: Option<(u16, u16)>,
        /// `Some(true)` for every vector that carries the frame delimiters, and
        /// `None` for the two that carry no `?25` at all -- an append and a
        /// carry, which scroll the screen and say nothing about the cursor.
        /// `None` is that declaration rather than a check left out: a frame
        /// that lost its `?25h` is refused.
        cursor_visible: Option<bool>,
        title: Option<&'a str>,
    },
    /// A whole-screen paint of the buffer a question borrows.
    Alternate {
        grid: &'a Grid,
        caret: (u16, u16),
        cursor_visible: Option<bool>,
    },
    /// A vector that paints **rows** rather than a screen: the document
    /// emitters, whose own target grid is the size of a shadow that may be
    /// older than the screen they are addressing.
    ///
    /// The rows it names are compared against an expectation built from the
    /// emitter's own row text; every other cell is held to the seed by the
    /// preservation sweep, displaced by exactly the declared scroll. So a
    /// screen that grew between two frames is compared rather than skipped,
    /// which is what the shadow-sized target could not do.
    Document {
        script: &'a Script<'a>,
        caret: Option<(u16, u16)>,
        cursor_visible: Option<bool>,
        title: Option<&'a str>,
    },
    /// `/clear`: a blank screen, a caret at its origin, and the scrollback
    /// erased.
    Cleared,
    /// The mode set, or the exit's restore of it.
    ///
    /// `cursor_visible` is an `Option` because the mode set says nothing about
    /// the cursor and the restore says everything: `None` is that declaration,
    /// and the exit passes `Some(true)` -- a session that left with the cursor
    /// hidden leaves the user's shell with no caret.
    Modes {
        modes: ModeSet,
        title_stack: u8,
        plane: PlaneKind,
        cursor_visible: Option<bool>,
    },
    /// The launch queries, **in order**: the fence only works if the terminal
    /// parses them in the order they were written.
    Queries(&'a [QueryId]),
    /// A scroll of a screen this process did not author, checked at the caret
    /// and the count and nothing else -- there is nothing else it can honestly
    /// be checked at.
    ScrollOnly { caret: (u16, u16), rows: u16 },
    /// The exit's last line: erase from the band's top row downward, leave the
    /// caret on a clean row, and show the cursor.
    Cleanup {
        top: u16,
        caret: (u16, u16),
        cursor_visible: bool,
    },
}

/// One edit a row-painting vector says it makes, in the order it makes it.
#[derive(Debug)]
enum Step<'a> {
    /// Blank this row, at the number it has **now**.
    Erase(u16),
    /// Put this text on that row, at the number it has now.
    Place(u16, &'a str),
    /// Move every row up by one; the top row leaves the screen.
    Scroll,
}

/// The ordered edits a row-painting vector says it makes.
///
/// **Ordered, and in emission coordinates**, because the thing being protected
/// is a row that stops existing partway through: an append delivering more rows
/// than the document area holds paints each one on the bottom row and scrolls,
/// so its first rows are in the terminal's own scrollback before the vector has
/// finished. A declaration spelt in final coordinates cannot describe them --
/// they have no final coordinate -- and a comparison made at the end cannot see
/// them at all.
///
/// Built from the row text and the counts the emitter was handed, never from
/// the bytes it wrote or from what the decoder saw them do.
#[derive(Debug)]
pub(crate) struct Script<'a> {
    steps: Vec<Step<'a>>,
    /// The screen those rows were built for, which is what clips their text.
    geometry: &'a super::layout::Geometry,
}

impl<'a> Script<'a> {
    pub(crate) fn new(geometry: &'a super::layout::Geometry) -> Self {
        Self {
            steps: Vec::new(),
            geometry,
        }
    }

    pub(crate) fn erase(&mut self, row: u16) {
        self.steps.push(Step::Erase(row));
    }

    pub(crate) fn place(&mut self, row: u16, text: &'a str) {
        self.steps.push(Step::Place(row, text));
    }

    pub(crate) fn scroll(&mut self) {
        self.steps.push(Step::Scroll);
    }
}

/// A script being replayed beside the vector that claims to perform it.
///
/// It holds one plane -- what the screen should hold -- and walks the script in
/// step with the decoder. **The row about to leave the screen is compared
/// before it goes**, which is the whole reason this exists: everything else can
/// be compared at the end, and that row cannot be compared at all afterwards.
///
/// Nothing here archives what left. The plane is the screen's, the same size it
/// always was, and it is dropped with the vector: a row that has gone into the
/// terminal's own scrollback is *checked* on its way out, not kept.
struct Expected<'a> {
    plane: Plane,
    steps: &'a [Step<'a>],
    /// The next step to perform.
    at: usize,
    geometry: &'a super::layout::Geometry,
}

impl<'a> Expected<'a> {
    fn new(seed: &Plane, script: &'a Script<'a>) -> Self {
        Self {
            plane: seed.clone(),
            steps: &script.steps,
            at: 0,
            geometry: script.geometry,
        }
    }

    /// Performs the script's row edits up to its next scroll.
    fn edits(&mut self, at: usize) -> Result<(), Reject> {
        while let Some(step) = self.steps.get(self.at) {
            match step {
                Step::Scroll => return Ok(()),
                Step::Erase(row) => {
                    let row = *row;
                    self.plane.erase_from(row, 1);
                }
                Step::Place(row, text) => {
                    let (row, text) = (*row, *text);
                    self.place(row, text, at)?;
                }
            }
            self.at += 1;
        }
        Ok(())
    }

    /// Puts one row of expected text on the expected plane.
    fn place(&mut self, row: u16, text: &str, at: usize) -> Result<(), Reject> {
        let cols = self.plane.cols;
        let Some(cells) = self.plane.row_mut(row) else {
            return Ok(());
        };
        for cell in cells.iter_mut() {
            *cell = CellState::Empty;
        }
        // Through the product's own tokenizer, which is the point of there
        // being one: what a row *may carry* and what a grid believes a terminal
        // is showing must not be two answers. `Grid::place_row` writes what the
        // tokenizer finds into a grid; this writes it straight into the
        // expected row, blanked first as that blanks its own, so these are the
        // cells a grid this wide would hold without a grid to read them from.
        //
        // A lead whose colour cannot be read is refused at its column, the
        // first such column -- leads arrive in column order, and the rest of
        // the row is not looked at once one has been found.
        let mut malformed = None;
        super::grid::tokenize_row(text, cols, self.geometry, |placed| match placed {
            _ if malformed.is_some() => {}
            Placed::Lead {
                column,
                cluster,
                width,
                sgr,
            } => {
                let Ok(color) = sgr.color() else {
                    malformed = Some(column);
                    return;
                };
                cells[column] = CellState::Lead {
                    grapheme: Glyph::new(cluster),
                    width: u8::try_from(width).unwrap_or(u8::MAX),
                    color,
                };
                for offset in 1..width {
                    cells[column + offset] = CellState::Continuation;
                }
            }
            Placed::Join { column, cluster } => {
                if let CellState::Lead { grapheme, .. } = &mut cells[column] {
                    grapheme.push_str(cluster);
                }
            }
        });
        match malformed {
            Some(column) => Err(Reject::at_cell(
                "an attribute slot this crate does not emit",
                at,
                row,
                u16::try_from(column + 1).unwrap_or(u16::MAX),
            )),
            None => Ok(()),
        }
    }

    /// The vector is about to scroll: the row that is leaving is compared, and
    /// then both planes move.
    fn scrolling(&mut self, model: &Plane, at: usize) -> Result<(), Reject> {
        self.edits(at)?;
        match self.steps.get(self.at) {
            Some(Step::Scroll) => self.at += 1,
            _ => {
                return Err(Reject::new(
                    "a scroll the declaration does not make here",
                    at,
                ))
            }
        }
        // **Row one, before it goes.** What leaves the top of the screen is in
        // the terminal's own scrollback, and this phase never repaints a
        // document row: a row that left with content nobody declared is a row
        // the user keeps for good.
        compare_row(
            model,
            &self.plane,
            1,
            at,
            "a row carried off the top of the screen",
        )?;
        self.plane.scroll_up();
        Ok(())
    }

    /// The vector is over: the rest of the script is performed and the whole
    /// screen compared.
    fn finish(&mut self, model: &Plane, at: usize) -> Result<(), Reject> {
        self.edits(at)?;
        if self.at != self.steps.len() {
            return Err(Reject::new("a scroll the vector did not make", at));
        }
        for row in 1..=self.plane.rows {
            compare_row(
                model,
                &self.plane,
                row,
                at,
                "a cell the vector did not leave as intended",
            )?;
        }
        Ok(())
    }
}

/// One row of the decoded screen against one row of the expected one.
fn compare_row(
    model: &Plane,
    expected: &Plane,
    row: u16,
    at: usize,
    shape: &'static str,
) -> Result<(), Reject> {
    // A row either plane does not have compares nothing, and a column only one
    // of them has is not compared: the two rows are read side by side, as far
    // as the narrower goes.
    let (Some(found), Some(wanted)) = (model.row(row), expected.row(row)) else {
        return Ok(());
    };
    for (offset, (found, wanted)) in found.iter().zip(wanted).enumerate() {
        if found.same_as(wanted) {
            continue;
        }
        let column = u16::try_from(offset + 1).unwrap_or(u16::MAX);
        return Err(Reject::at_cell(shape, at, row, column));
    }
    Ok(())
}

/// An intent and the rows it is allowed to reach.
#[derive(Debug)]
pub(crate) struct Declared<'a> {
    intent: Intent<'a>,
    footprint: Footprint,
}

impl<'a> Declared<'a> {
    pub(crate) fn new(intent: Intent<'a>, footprint: Footprint) -> Self {
        Self { intent, footprint }
    }
}

/// Decodes `bytes` against `seed` and compares the result with what the emitter
/// declared.
///
/// The whole vector at once, and to its end: an emitter's vector is complete by
/// construction, so a sequence still open when the bytes run out is a defect
/// rather than a continuation.
pub(crate) fn preflight(
    seed: &TerminalModel,
    bytes: &[u8],
    declared: &Declared<'_>,
) -> Result<TerminalModel, Reject> {
    let mut model = seed.clone();
    // A row-painting vector is replayed **beside** its declaration rather than
    // compared to it at the end: the rows an append delivers past the height of
    // the document area are in the terminal's own scrollback before the vector
    // is over, and a comparison made afterwards cannot reach them.
    let mut expected = match &declared.intent {
        Intent::Document { script, .. } => Some(Expected::new(
            seed.plane_of(declared.footprint.plane),
            script,
        )),
        _ => None,
    };
    apply(&mut model, bytes, expected.as_mut())?;
    if let Some(expected) = &mut expected {
        expected.finish(model.plane_of(declared.footprint.plane), bytes.len())?;
    }
    check_footprint(&model, &declared.footprint, bytes.len())?;
    check_metadata(seed, &model, declared, bytes.len())?;
    check_cells(seed, &model, &declared.intent, bytes.len())?;
    check_preservation(
        seed,
        &model,
        &declared.footprint,
        expected.is_some(),
        bytes.len(),
    )?;
    Ok(model)
}

/// The fixed vector [`preflight_recovery_cleanup`] accepts, and the only one
/// [`super::frame::Band::recover_primary`] ever emits.
///
/// `CAN` first, on the chance the terminal's parser is mid-sequence for the
/// prefix it was left holding -- the one byte this crate's normal alphabet
/// refuses everywhere else ([`apply`]) and the one an in-progress `OSC`,
/// `CSI` or UTF-8 sequence cannot itself contain, so it is the one byte that
/// ends whatever that sequence was without becoming part of it. Then a
/// hyperlink close, an SGR reset, synchronized output off, autowrap off and
/// the cursor shown: state this crate's own writes can leave set and a torn
/// vector might have left half-set. Fixed and never built from a runtime
/// value, which is what makes a byte-exact check of it meaningful rather
/// than circular.
pub(crate) const RECOVERY_CLEANUP: &str = "\x18\x1b]8;;\x07\x1b[0m\x1b[?2026l\x1b[?7l\x1b[?25h";

/// Whether `bytes` is exactly [`RECOVERY_CLEANUP`] -- the one vector this
/// entry point exists to license -- and nothing else.
///
/// **Not [`preflight`].** That function decodes a vector against a seeded
/// model of the screen it is about to land on, which is exactly what a
/// terminal holding an unknown prefix cannot be given: there is no seed for
/// "some parser state this crate did not choose." This asks the only
/// question that state still admits an answer to -- is the vector the one
/// fixed reset this crate has chosen to try, byte for byte -- and licenses
/// nothing else, including any of the sequences `RECOVERY_CLEANUP` itself is
/// built from: [`apply`]'s own alphabet already refuses a bare `CAN` or an
/// `OSC 8` outside this path, and nothing here loosens that.
///
/// The confidence this buys is bounded and stated where it is spent
/// ([`super::frame::Band::recover_primary`]): an independent terminal
/// experiment, not this function's own logic, is what stands behind the
/// choice of these particular bytes.
pub(crate) fn preflight_recovery_cleanup(bytes: &[u8]) -> Result<(), Reject> {
    if bytes == RECOVERY_CLEANUP.as_bytes() {
        Ok(())
    } else {
        Err(Reject::new(
            "a vector other than the fixed recovery cleanup",
            0,
        ))
    }
}

/// Every effect this vector had is one the footprint allows -- on the buffer it
/// allows it on -- and every scroll it makes is one the footprint asked for.
fn check_footprint(model: &TerminalModel, footprint: &Footprint, at: usize) -> Result<(), Reject> {
    if model.moves != footprint.moves.path() {
        return Err(Reject::new(
            "a buffer transition the vector did not declare",
            at,
        ));
    }
    let mut scrolled = 0u32;
    let mut scroll_seen = false;
    for effect in &model.effects {
        // The two buffers share one row-number space, so the plane is asked
        // first: a row range cannot tell the surface a question is painting
        // from the screen the terminal is about to save.
        if effect.plane != footprint.plane {
            return Err(Reject::at_cell(
                "a cell written on the buffer this vector may not touch",
                at,
                effect.row,
                1,
            ));
        }
        match effect.did {
            Did::Erase => {
                if !footprint.may_erase(effect.row) {
                    return Err(Reject::at_cell(
                        "an erase outside the footprint",
                        at,
                        effect.row,
                        1,
                    ));
                }
                // A row the vector is not placing on is a row whose erase must
                // happen before anything scrolls: a document row carried off
                // the top of the screen unerased is one nothing can retract.
                if scroll_seen && !footprint.may_place(effect.row) {
                    return Err(Reject::at_cell(
                        "an erase after a scroll",
                        at,
                        effect.row,
                        1,
                    ));
                }
            }
            Did::Place => {
                if !footprint.may_place(effect.row) {
                    return Err(Reject::at_cell(
                        "a placement outside the footprint",
                        at,
                        effect.row,
                        1,
                    ));
                }
            }
            Did::Scroll => {
                scroll_seen = true;
                scrolled += 1;
            }
        }
    }
    if scrolled != footprint.scroll_rows() {
        return Err(Reject::new(
            "a scroll count the footprint did not ask for",
            at,
        ));
    }
    Ok(())
}

/// Everything about a terminal that is not a cell or a caret.
///
/// Built as **values** from the seed plus the intent's declared delta, and then
/// compared field by field. That is the shape rather than a list of fields each
/// intent remembers to check, because the failure mode of the list is silence:
/// a sequence that moves a field nobody named -- an `ED 3` that takes the
/// user's scrollback, a `?7h` that puts autowrap back, a `CSI 6n` whose answer
/// lands in the composer -- moves no cell, pushes no effect, and passes.
#[derive(Debug, PartialEq, Eq)]
struct Metadata {
    plane: PlaneKind,
    cursor_visible: Option<bool>,
    shown_title: Option<String>,
    title_stack: u8,
    modes: ModeSet,
    scrollback_erased: bool,
    queries: Vec<QueryId>,
    saved_cursor: Option<(u16, u16)>,
}

impl Metadata {
    /// What the terminal's metadata is, as decoded.
    fn of(model: &TerminalModel) -> Self {
        Self {
            plane: model.plane,
            cursor_visible: model.cursor_visible,
            shown_title: model.shown_title.clone(),
            title_stack: model.title_stack,
            modes: model.modes,
            scrollback_erased: model.scrollback_erased,
            queries: model.queries.clone(),
            saved_cursor: model.saved_cursor,
        }
    }

    /// The first field that disagrees, named rather than dumped.
    fn disagreement(&self, other: &Self) -> Option<&'static str> {
        if self.plane != other.plane {
            return Some("a buffer that is not the one asked for");
        }
        if self.cursor_visible != other.cursor_visible {
            return Some("a cursor visibility nobody asked for");
        }
        if self.shown_title != other.shown_title {
            return Some("a title that is not the one asked for");
        }
        if self.title_stack != other.title_stack {
            return Some("a title stack left at the wrong depth");
        }
        if self.modes != other.modes {
            return Some("a terminal mode this vector did not declare");
        }
        if self.scrollback_erased != other.scrollback_erased {
            return Some("a scrollback erase this vector did not declare");
        }
        if self.queries != other.queries {
            return Some(if other.queries.is_empty() {
                "a query this vector did not declare"
            } else {
                "queries in an order the fence does not have"
            });
        }
        if self.saved_cursor != other.saved_cursor {
            return Some("a saved cursor this vector did not declare");
        }
        None
    }
}

/// The metadata the declaration says this vector leaves behind.
fn expected_metadata(seed: &TerminalModel, declared: &Declared<'_>) -> Metadata {
    let moves = declared.footprint.moves;
    // `?1049h` saves the caret into the slot and `?1049l` drains it, so the slot
    // follows the declared transition rather than being asserted unchanged.
    let saved_cursor = match (moves, seed.caret) {
        (PlaneMove::Take, Caret::Known(caret)) => Some(caret),
        (PlaneMove::Take, Caret::Unknown) | (PlaneMove::Give, _) => None,
        (PlaneMove::Stay, _) => seed.saved_cursor,
    };
    let unchanged = |cursor_visible: Option<bool>| Metadata {
        plane: *moves.path().last().unwrap_or(&seed.plane),
        cursor_visible: cursor_visible.or(seed.cursor_visible),
        shown_title: seed.shown_title.clone(),
        title_stack: seed.title_stack,
        modes: seed.modes,
        scrollback_erased: seed.scrollback_erased,
        queries: Vec::new(),
        saved_cursor,
    };
    match &declared.intent {
        Intent::Primary {
            cursor_visible,
            title,
            ..
        }
        | Intent::Document {
            cursor_visible,
            title,
            ..
        } => Metadata {
            shown_title: title.map(str::to_string),
            ..unchanged(*cursor_visible)
        },
        Intent::Alternate { cursor_visible, .. } => unchanged(*cursor_visible),
        Intent::Cleared => Metadata {
            // The one emitter in the TUI that erases a terminal's scrollback,
            // and the only declaration that admits it.
            scrollback_erased: true,
            ..unchanged(None)
        },
        Intent::Modes {
            modes,
            title_stack,
            plane,
            cursor_visible,
        } => Metadata {
            plane: *plane,
            modes: *modes,
            title_stack: *title_stack,
            // A pop hands the window title back to the terminal's own user, so
            // a declaration that pops is a declaration that the title this
            // session set is gone. Local to this vector: no claim is made about
            // a stack depth across earlier ones.
            shown_title: if *title_stack < seed.title_stack {
                None
            } else {
                seed.shown_title.clone()
            },
            ..unchanged(*cursor_visible)
        },
        Intent::Queries(asked) => Metadata {
            queries: asked.to_vec(),
            ..unchanged(None)
        },
        Intent::ScrollOnly { .. } => unchanged(None),
        Intent::Cleanup { cursor_visible, .. } => unchanged(Some(*cursor_visible)),
    }
}

fn check_metadata(
    seed: &TerminalModel,
    model: &TerminalModel,
    declared: &Declared<'_>,
    at: usize,
) -> Result<(), Reject> {
    // A vector boundary is a screen a terminal presents, so synchronized output
    // opened inside one is closed inside it -- whatever the vector was for.
    if model.sync_open {
        return Err(Reject::new(
            "a vector that left synchronized output open",
            at,
        ));
    }
    let wanted = expected_metadata(seed, declared);
    match Metadata::of(model).disagreement(&wanted) {
        Some(shape) => Err(Reject::new(shape, at)),
        None => Ok(()),
    }
}

/// The cells and the caret this vector says it leaves.
fn check_cells(
    seed: &TerminalModel,
    model: &TerminalModel,
    intent: &Intent<'_>,
    at: usize,
) -> Result<(), Reject> {
    match intent {
        Intent::Primary { grid, caret, .. } => {
            compare_cells(model.screen_ref(), grid, at)?;
            expect_declared_caret(model, *caret, at)
        }
        Intent::Alternate { grid, caret, .. } => {
            compare_cells(model.screen_ref(), grid, at)?;
            expect_caret(model, *caret, at)
        }
        // Its cells were compared as the vector ran, row by row, including the
        // ones that had left the screen before it ended.
        Intent::Document { caret, .. } => expect_declared_caret(model, *caret, at),
        Intent::Cleared => {
            expect_blank(model.screen_ref(), 1, at)?;
            expect_caret(model, (1, 1), at)
        }
        Intent::Modes { .. } | Intent::Queries(_) => {
            if !model.effects.is_empty() {
                return Err(Reject::new(
                    "a vector that moved a cell it did not declare",
                    at,
                ));
            }
            // **The caret they leave is the one they found.** These vectors
            // address no row, so a `CUP` inside one moves the user's own cursor
            // on a screen this session is in the middle of taking or giving
            // back -- and it pushes no effect, so the emptiness above cannot
            // see it.
            if model.caret != seed.caret {
                return Err(Reject::new("a caret this vector did not declare", at));
            }
            Ok(())
        }
        Intent::ScrollOnly { caret, rows } => {
            let scrolled = model
                .effects
                .iter()
                .filter(|effect| matches!(effect.did, Did::Scroll))
                .count();
            if scrolled != usize::from(*rows) {
                return Err(Reject::new("a scroll of the wrong number of rows", at));
            }
            if model
                .effects
                .iter()
                .any(|effect| !matches!(effect.did, Did::Scroll))
            {
                return Err(Reject::new("a scroll vector that wrote a cell", at));
            }
            expect_caret(model, *caret, at)
        }
        Intent::Cleanup { top, caret, .. } => {
            expect_blank(model.screen_ref(), *top, at)?;
            expect_caret(model, *caret, at)
        }
    }
}

/// Every cell from `top` down is blank.
fn expect_blank(plane: &Plane, top: u16, at: usize) -> Result<(), Reject> {
    for row in top..=plane.rows {
        for column in 1..=plane.cols {
            let Some(index) = plane.index(row, column) else {
                continue;
            };
            if plane.cells[index] != CellState::Empty {
                return Err(Reject::at_cell(
                    "a row this vector said it would blank",
                    at,
                    row,
                    column,
                ));
            }
        }
    }
    Ok(())
}

/// Every cell the seed described is where the declaration puts it, or was
/// covered by something the declaration allows.
///
/// **What this proves, stated as narrowly as it is implemented.** Each seeded
/// cell -- a foreign one by its token, an authored one by its content -- must
/// appear exactly `scroll` rows above where it was, where `scroll` is the count
/// the *footprint* declared rather than the one the bytes performed. A cell that
/// is not there must have been licensed: its row, before or after that scroll,
/// inside a declared erase or placement. The rows a scroll frees at the bottom
/// must be blank or placed on.
///
/// **What it does not prove**: the order in which the vector did those things.
/// The ordering rules that exist are `check_footprint`'s, and they are separate.
fn check_preservation(
    seed: &TerminalModel,
    model: &TerminalModel,
    footprint: &Footprint,
    replayed: bool,
    at: usize,
) -> Result<(), Reject> {
    // The buffer the footprint speaks for, and the other one -- which may not be
    // touched at all, whatever its row numbers would have allowed.
    //
    // A replayed vector's own buffer is **not** swept: the replay compared every
    // row of it, at the moment each one could still be compared, which is
    // strictly more than a sweep of what survived can say.
    if !(replayed && footprint.plane == PlaneKind::Primary) {
        preserve(
            &seed.primary,
            &model.primary,
            footprint,
            PlaneKind::Primary,
            at,
        )?;
    }
    if let (Some(before), Some(after)) = (&seed.alternate, &model.alternate) {
        if !(replayed && footprint.plane == PlaneKind::Alternate) {
            preserve(before, after, footprint, PlaneKind::Alternate, at)?;
        }
    }
    Ok(())
}

fn preserve(
    before: &Plane,
    after: &Plane,
    footprint: &Footprint,
    plane: PlaneKind,
    at: usize,
) -> Result<(), Reject> {
    if before.rows != after.rows || before.cols != after.cols {
        return Err(Reject::new("a screen that changed size mid-vector", at));
    }
    let cols = usize::from(before.cols);
    if cols == 0 {
        return Ok(());
    }
    // Zero for the buffer this vector was not declared against: nothing on it
    // may move at all.
    let scrolled = if plane == footprint.plane {
        usize::try_from(footprint.scroll_rows()).unwrap_or(usize::MAX)
    } else {
        0
    };
    let displaced = scrolled.saturating_mul(cols);
    let licensed = licence(footprint, plane, before.rows, scrolled);
    let row_of = |index: usize| index / cols + 1;
    for index in 0..before.cells.len() {
        let cell = before.logical(index);
        let Some(moved) = index.checked_sub(displaced) else {
            // It left the top of the screen, which is what a declared scroll is
            // for.
            continue;
        };
        if after.logical(moved) == cell || licensed[row_of(moved)] {
            continue;
        }
        let row = u16::try_from(row_of(moved)).unwrap_or(u16::MAX);
        let column = u16::try_from(moved % cols + 1).unwrap_or(u16::MAX);
        return Err(Reject::at_cell(
            "a cell this vector did not declare it would change",
            at,
            row,
            column,
        ));
    }
    // And the rows the scroll freed at the bottom: known blank, unless this
    // vector declared it would write on them.
    if displaced > 0 {
        let first = after.cells.len().saturating_sub(displaced);
        for index in first..after.cells.len() {
            if *after.logical(index) == CellState::Empty || licensed[row_of(index)] {
                continue;
            }
            let row = u16::try_from(row_of(index)).unwrap_or(u16::MAX);
            let column = u16::try_from(index % cols + 1).unwrap_or(u16::MAX);
            return Err(Reject::at_cell(
                "a row a scroll freed and this vector did not blank",
                at,
                row,
                column,
            ));
        }
    }
    Ok(())
}

/// Which **final** rows a vector was licensed to change, by row number.
///
/// **Not a union over every scroll offset.** Unioning the licence across every
/// step a row could have passed through makes it saturate: on a vector that
/// scrolls as far as the screen is tall, one range anywhere licenses every row,
/// and a sweep built on it protects nothing exactly where the content is
/// irreversible. So the licence is read at the one place it is true.
///
/// Every emitter that reaches this function performs its row edits **before**
/// its scrolls -- the exit's cleanup erases and then linefeeds once; the push
/// and the carry edit no row at all; the frame and `/clear` do not scroll. A row
/// edited at number `r` therefore ends at `r - scrolled`, and that is the only
/// position the licence is read at. A vector that interleaves edits and scrolls
/// declares an ordered [`Script`] instead and is replayed rather than swept.
fn licence(footprint: &Footprint, plane: PlaneKind, rows: u16, scrolled: usize) -> Vec<bool> {
    let mut licensed = vec![false; usize::from(rows) + 1];
    if plane != footprint.plane {
        // The buffer this vector was not declared against: nothing on it is
        // licensed, whatever row numbers the footprint names.
        return licensed;
    }
    let Ok(scrolled) = u16::try_from(scrolled) else {
        return licensed;
    };
    for final_row in 1..=rows {
        let Some(edited) = final_row.checked_add(scrolled) else {
            continue;
        };
        if footprint.may_erase(edited) || footprint.may_place(edited) {
            licensed[usize::from(final_row)] = true;
        }
    }
    licensed
}

fn expect_caret(model: &TerminalModel, caret: (u16, u16), at: usize) -> Result<(), Reject> {
    if model.caret == Caret::Known(caret) {
        return Ok(());
    }
    Err(Reject::new("a caret left somewhere else", at))
}

/// The caret, where the intent declared one.
///
/// A `None` is a declaration and not a gap: an append leaves the caret wherever
/// its last placement put it, which is exactly what the band records by
/// forgetting it, and the next frame owes a `CUP`.
fn expect_declared_caret(
    model: &TerminalModel,
    caret: Option<(u16, u16)>,
    at: usize,
) -> Result<(), Reject> {
    match caret {
        Some(caret) => expect_caret(model, caret, at),
        None => Ok(()),
    }
}

/// One decoded cell against the one an expectation holds.
fn agree(found: &CellState, wanted: &Cell, at: usize, row: u16, column: u16) -> Result<(), Reject> {
    // Field by field rather than by building the cell this one would be: a
    // large screen is sixty thousand cells, and a comparison that cloned a
    // `String` for each of them would put an allocation per cell in front of
    // every released frame.
    let wanted = match wanted {
        // A space and an untouched cell are the same thing on a screen, and the
        // painter spells one as the other -- [`Grid::diff`] writes a space
        // where the target cell is empty.
        Cell::Empty => Some((&b" "[..], 1u8, Color::Default)),
        Cell::Continuation => None,
        Cell::Lead {
            grapheme,
            width,
            sgr,
        } => Some((
            grapheme.as_bytes(),
            *width,
            sgr.color().map_err(|MalformedSlot| {
                Reject::at_cell(
                    "an attribute slot this crate does not emit",
                    at,
                    row,
                    column,
                )
            })?,
        )),
    };
    let agreed = match wanted {
        Some(wanted) => found.shown() == Some(wanted),
        None => matches!(found, CellState::Continuation),
    };
    if agreed {
        return Ok(());
    }
    Err(Reject::at_cell(
        "a cell the vector did not leave as intended",
        at,
        row,
        column,
    ))
}

/// The decoded cells against the ones the emitter says it wants.
///
/// **Asymmetric on purpose.** A cell the intent knows must be known and equal;
/// an unexamined cell is a failure rather than a skip, which is why the grid
/// handed in has to cover the screen this vector is addressing. A target that
/// cannot describe those rows is a **declaration defect** -- it is how an
/// expectation built from a stale shadow silently stops comparing exactly when
/// the screen has moved -- so it is refused rather than intersected away.
fn compare_cells(plane: &Plane, grid: &Grid, at: usize) -> Result<(), Reject> {
    if grid.rows() < plane.rows || grid.cols() < plane.cols {
        return Err(Reject::new(
            "a declared screen too small to describe the one this vector addresses",
            at,
        ));
    }
    for row in 1..=plane.rows {
        for column in 1..=plane.cols {
            let (Some(index), Some(cell)) = (plane.index(row, column), grid.cell(row, column))
            else {
                continue;
            };
            agree(&plane.cells[index], cell, at, row, column)?;
        }
    }
    Ok(())
}

/// Decodes one complete vector into `model`.
///
/// A byte scanner over the alphabet this crate emits and **nothing else**: an
/// unknown sequence is a rejection rather than a skip, which is the whole of
/// what "the emitted alphabet is closed" buys.
fn apply(
    model: &mut TerminalModel,
    bytes: &[u8],
    mut expected: Option<&mut Expected<'_>>,
) -> Result<(), Reject> {
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            0x1b => at += sequence(model, bytes, at)?,
            b'\n' => {
                linefeed(model, expected.as_deref_mut(), at)?;
                at += 1;
            }
            byte if byte < 0x20 || byte == 0x7f => {
                return Err(Reject::new("a control byte outside the alphabet", at))
            }
            _ => {
                let end = bytes[at..]
                    .iter()
                    .position(|byte| *byte == 0x1b || *byte < 0x20 || *byte == 0x7f)
                    .map_or(bytes.len(), |offset| at + offset);
                let text = std::str::from_utf8(&bytes[at..end])
                    .map_err(|_| Reject::new("text that is not UTF-8", at))?;
                write_text(model, text, at)?;
                at = end;
            }
        }
    }
    Ok(())
}

/// A linefeed: a scroll from the bottom margin, and a caret that walks down
/// from anywhere else.
///
/// The difference is the whole reason [`super::frame::scroll_one`] writes a
/// `CUP` in front of every one of them, so it is modelled rather than assumed:
/// a linefeed whose `CUP` went missing moves the caret and scrolls nothing, and
/// the count then disagrees with the declared one.
fn linefeed(
    model: &mut TerminalModel,
    expected: Option<&mut Expected<'_>>,
    at: usize,
) -> Result<(), Reject> {
    let Caret::Known((row, column)) = model.caret else {
        return Err(Reject::new("a linefeed from a caret nothing can place", at));
    };
    if row >= model.screen_ref().rows {
        // **Before the screen moves.** The row at the top is about to be in the
        // terminal's own scrollback, where nothing in this phase reaches it
        // again, so this is the last moment it can be compared at all.
        if let Some(expected) = expected {
            expected.scrolling(model.screen_ref(), at)?;
        }
        model.screen().scroll_up();
        model.record(Did::Scroll, row);
    } else {
        model.caret = Caret::Known((row + 1, column));
    }
    Ok(())
}

/// Puts `text` on the screen at the caret, cluster by cluster.
fn write_text(model: &mut TerminalModel, text: &str, at: usize) -> Result<(), Reject> {
    for cluster in clusters(text) {
        let Caret::Known((row, column)) = model.caret else {
            return Err(Reject::new("text written at a caret nothing can place", at));
        };
        let width = super::wrap::width(cluster);
        if width == 0 {
            // A combining mark belongs to the cluster in front of it, exactly
            // as `Grid::place_row` gives it no cell of its own.
            let previous = column.checked_sub(1).and_then(|at| {
                let cols = model.screen_ref().cols;
                let _ = cols;
                model.screen().index(row, at)
            });
            if let Some(index) = previous {
                if let CellState::Lead { grapheme, .. } = &mut model.screen().cells[index] {
                    grapheme.push_str(cluster);
                }
            }
            continue;
        }
        let color = model.pen;
        let plane = model.screen();
        if usize::from(column) + usize::from(width) - 1 > usize::from(plane.cols) {
            return Err(Reject::at_cell(
                "a cluster written past the last column",
                at,
                row,
                column,
            ));
        }
        let Some(index) = plane.index(row, column) else {
            return Err(Reject::at_cell(
                "a cell outside the screen",
                at,
                row,
                column,
            ));
        };
        // Every half of a wide cluster this write breaks, on **each** column it
        // covers rather than only on the first.
        //
        // The narrow case is the one that made this general: a delete shifts a
        // row left, so a two-column cluster lands one column to the left of the
        // one already there and its *second* column covers the old cluster's
        // lead -- leaving the old cluster's continuation a column further
        // right, with no lead, as half a character nothing would erase.
        for offset in 0..width {
            let Some(covered) = plane.index(row, column + offset) else {
                continue;
            };
            match plane.cells[covered] {
                // The left half of a cluster this write lands inside.
                CellState::Continuation => {
                    if let Some(lead) = (column + offset)
                        .checked_sub(1)
                        .and_then(|left| plane.index(row, left))
                    {
                        plane.cells[lead] = CellState::Empty;
                    }
                }
                // And the right half of one it covers.
                CellState::Lead { width: old, .. } => {
                    for tail in 1..u16::from(old) {
                        if let Some(tail) = plane.index(row, column + offset + tail) {
                            if matches!(plane.cells[tail], CellState::Continuation) {
                                plane.cells[tail] = CellState::Empty;
                            }
                        }
                    }
                }
                CellState::Empty | CellState::Foreign(_) => {}
            }
        }
        plane.cells[index] = CellState::Lead {
            grapheme: Glyph::new(cluster),
            width: u8::try_from(width).unwrap_or(u8::MAX),
            color,
        };
        for offset in 1..width {
            if let Some(tail) = plane.index(row, column + offset) {
                plane.cells[tail] = CellState::Continuation;
            }
        }
        model.record(Did::Place, row);
        model.caret = Caret::Known((row, column + width));
    }
    Ok(())
}

/// The grapheme clusters of one run of text, in order.
///
/// A run of printable ASCII -- space through `~` -- is one cluster per byte: no
/// rule of the segmentation joins two of them, and everything that can join a
/// character to its neighbour (a combining mark, a joiner, a variation
/// selector) is outside that range. Nearly every run [`apply`] hands over is
/// one, so it is walked a byte at a time instead of being segmented, and a
/// single byte outside the range sends the whole run the general way. The
/// precondition is `super::wrap::width`'s, and so is the promise: a faster
/// route to the same clusters, not a second answer.
fn clusters(text: &str) -> impl Iterator<Item = &str> {
    let ascii = text.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
    let mut graphemes = text.graphemes(true);
    let mut start = 0usize;
    std::iter::from_fn(move || {
        if !ascii {
            return graphemes.next();
        }
        let cluster = text.get(start..start + 1)?;
        start += 1;
        Some(cluster)
    })
}

/// How many bytes the sequence at `at` takes, once it has been applied.
fn sequence(model: &mut TerminalModel, bytes: &[u8], at: usize) -> Result<usize, Reject> {
    match bytes.get(at + 1) {
        Some(b'[') => control_sequence(model, bytes, at),
        Some(b']') => operating_system_command(model, bytes, at),
        _ => Err(Reject::new("an escape outside the alphabet", at)),
    }
}

/// `CSI` and everything this crate spells with one.
fn control_sequence(model: &mut TerminalModel, bytes: &[u8], at: usize) -> Result<usize, Reject> {
    let mut end = at + 2;
    while end < bytes.len() && !(0x40..=0x7e).contains(&bytes[end]) {
        end += 1;
    }
    if end >= bytes.len() {
        return Err(Reject::new("a control sequence with no final byte", at));
    }
    let final_byte = bytes[end];
    let body = std::str::from_utf8(&bytes[at + 2..end])
        .map_err(|_| Reject::new("a control sequence that is not text", at))?;
    let (prefix, params) = match body.as_bytes().first() {
        Some(b'?' | b'>' | b'<') => (body.as_bytes()[0], &body[1..]),
        _ => (b' ', body),
    };
    let length = end + 1 - at;
    match (prefix, final_byte) {
        (b' ', b'H') => {
            let (row, column) = position(params, at)?;
            let plane = model.screen_ref();
            if row == 0 || column == 0 || row > plane.rows || column > plane.cols {
                return Err(Reject::at_cell(
                    "a caret placed off the screen",
                    at,
                    row,
                    column,
                ));
            }
            model.caret = Caret::Known((row, column));
        }
        (b' ', b'J') => match params {
            "" | "0" => erase_below(model, at)?,
            "2" => erase_screen(model),
            "3" => model.scrollback_erased = true,
            _ => return Err(Reject::new("an erase this crate does not write", at)),
        },
        (b' ', b'K') => match params {
            "" | "0" => {
                let Caret::Known((row, column)) = model.caret else {
                    return Err(Reject::new("an erase from a caret nothing can place", at));
                };
                model.screen().erase_from(row, column);
                model.record(Did::Erase, row);
            }
            _ => return Err(Reject::new("a line erase this crate does not write", at)),
        },
        (b' ', b'm') => model.pen = wire_color(params, at)?,
        (b' ', b'n') => match params {
            "6" => model.queries.push(QueryId::CursorPosition),
            _ => return Err(Reject::new("a report this crate does not ask for", at)),
        },
        (b' ', b't') => match params {
            "22;2" => model.title_stack = model.title_stack.saturating_add(1),
            "23;2" => {
                if model.title_stack == 0 {
                    return Err(Reject::new("a title stack popped past its bottom", at));
                }
                model.title_stack -= 1;
                // What the terminal gives back is its user's, not this
                // session's: whatever was set inside the pair is gone.
                model.shown_title = None;
            }
            _ => {
                return Err(Reject::new(
                    "a window operation this crate does not write",
                    at,
                ))
            }
        },
        (b'?', set @ (b'h' | b'l')) => private_mode(model, params, set == b'h', at)?,
        // **A separate arm from the `CSI 6 n` above, deliberately.** The
        // private marker makes this a different sequence with a different
        // answer -- `CSI ? 996 n` is answered with a theme report and `CSI 6 n`
        // with a cursor position -- so widening the plain arm to carry both
        // would let a vector declare one and write the other. One parameter and
        // one only: `? 6 n` is the private cursor report, which this crate does
        // not write, and a grammar that admitted every private `n` would admit
        // it.
        (b'?', b'n') => match params {
            "996" => model.queries.push(QueryId::ThemeMode),
            _ => return Err(Reject::new("a report this crate does not ask for", at)),
        },
        (b'>', b'm') => match params {
            "4;2" => model.modes.modify_other_keys = Some(2),
            "4;0" => model.modes.modify_other_keys = Some(0),
            _ => {
                return Err(Reject::new(
                    "a key-reporting mode this crate does not write",
                    at,
                ))
            }
        },
        (b'>', b'u') => match params {
            "1" => model.modes.kitty_keyboard = Some(true),
            _ => return Err(Reject::new("a keyboard push this crate does not write", at)),
        },
        (b'<', b'u') => match params {
            "" => model.modes.kitty_keyboard = Some(false),
            _ => return Err(Reject::new("a keyboard pop this crate does not write", at)),
        },
        _ => return Err(Reject::new("a control sequence outside the alphabet", at)),
    }
    Ok(length)
}

/// `CSI r ; c H`, and the bare `CSI H` that means the screen's origin.
fn position(params: &str, at: usize) -> Result<(u16, u16), Reject> {
    if params.is_empty() {
        return Ok((1, 1));
    }
    let mut parts = params.split(';');
    let row = parts
        .next()
        .and_then(|part| part.parse::<u16>().ok())
        .ok_or_else(|| Reject::new("a caret address that is not a number", at))?;
    let column = parts
        .next()
        .and_then(|part| part.parse::<u16>().ok())
        .ok_or_else(|| Reject::new("a caret address that is not a number", at))?;
    if parts.next().is_some() {
        return Err(Reject::new("a caret address with too many parameters", at));
    }
    Ok((row, column))
}

/// `ED 0`: the rest of this row, and every row below it.
fn erase_below(model: &mut TerminalModel, at: usize) -> Result<(), Reject> {
    let Caret::Known((row, column)) = model.caret else {
        return Err(Reject::new("an erase from a caret nothing can place", at));
    };
    let rows = model.screen_ref().rows;
    model.screen().erase_from(row, column);
    model.record(Did::Erase, row);
    for line in row + 1..=rows {
        model.screen().erase_from(line, 1);
        model.record(Did::Erase, line);
    }
    Ok(())
}

/// `ED 2`: every cell, wherever the caret is.
fn erase_screen(model: &mut TerminalModel) {
    let rows = model.screen_ref().rows;
    for line in 1..=rows {
        model.screen().erase_from(line, 1);
        model.record(Did::Erase, line);
    }
}

/// The `DEC` private modes this crate sets and resets.
fn private_mode(
    model: &mut TerminalModel,
    params: &str,
    set: bool,
    at: usize,
) -> Result<(), Reject> {
    match params {
        "2026" => model.sync_open = set,
        "25" => model.cursor_visible = Some(set),
        "2004" => model.modes.bracketed_paste = Some(set),
        "7" => model.modes.autowrap = Some(set),
        "2031" => model.modes.theme_notifications = Some(set),
        "1049" => {
            if set {
                model.saved_cursor = match model.caret {
                    Caret::Known(caret) => Some(caret),
                    Caret::Unknown => None,
                };
                let (rows, cols) = (model.primary.rows, model.primary.cols);
                model.alternate = Some(Plane::blank(rows, cols));
                model.plane = PlaneKind::Alternate;
                model.moves.push(PlaneKind::Alternate);
            } else {
                model.plane = PlaneKind::Primary;
                model.moves.push(PlaneKind::Primary);
                model.alternate = None;
                // The terminal restores the cursor it saved, and where nothing
                // saved one there is nothing to restore: `Unknown` rather than
                // the stale address the caret happens to hold.
                model.caret = model
                    .saved_cursor
                    .take()
                    .map_or(Caret::Unknown, Caret::Known);
            }
        }
        _ => return Err(Reject::new("a private mode outside the alphabet", at)),
    }
    Ok(())
}

/// `OSC 2` and the background query, and nothing else.
fn operating_system_command(
    model: &mut TerminalModel,
    bytes: &[u8],
    at: usize,
) -> Result<usize, Reject> {
    let mut end = at + 2;
    let mut terminator = 0usize;
    while end < bytes.len() {
        if bytes[end] == 0x07 {
            terminator = 1;
            break;
        }
        if bytes[end] == 0x1b && bytes.get(end + 1) == Some(&b'\\') {
            terminator = 2;
            break;
        }
        end += 1;
    }
    if terminator == 0 {
        return Err(Reject::new("a string sequence with no terminator", at));
    }
    let body = std::str::from_utf8(&bytes[at + 2..end])
        .map_err(|_| Reject::new("a string sequence that is not text", at))?;
    if let Some(title) = body.strip_prefix("2;") {
        if terminator != 1 {
            return Err(Reject::new(
                "a title this crate does not spell that way",
                at,
            ));
        }
        if title.chars().any(char::is_control) {
            return Err(Reject::new("a title carrying a control character", at));
        }
        model.shown_title = Some(title.to_string());
    } else if body == "11;?" {
        if terminator != 2 {
            return Err(Reject::new(
                "a query this crate does not spell that way",
                at,
            ));
        }
        model.queries.push(QueryId::Background);
    } else {
        return Err(Reject::new("a string sequence outside the alphabet", at));
    }
    Ok(end + terminator - at)
}

/// The colour a `CSI ... m` on the wire means.
///
/// **The second of the two parsers**, and deliberately not the one
/// [`super::pacer::SgrState::color`] is: a single shared parser would make an
/// assembly defect cancel against itself, both sides mis-reading the same bytes
/// and agreeing.
fn wire_color(params: &str, at: usize) -> Result<Color, Reject> {
    let malformed = || Reject::new("a colour outside the three shapes this crate emits", at);
    if params.is_empty() || params == "0" {
        return Ok(Color::Default);
    }
    let mut parts = params.split(';');
    if parts.next() != Some("38") {
        return Err(malformed());
    }
    match parts.next() {
        Some("5") => {
            let index = parts.next().ok_or_else(malformed)?;
            if parts.next().is_some() {
                return Err(malformed());
            }
            Ok(Color::Indexed(number(index).ok_or_else(malformed)?))
        }
        Some("2") => {
            let red = parts.next().ok_or_else(malformed)?;
            let green = parts.next().ok_or_else(malformed)?;
            let blue = parts.next().ok_or_else(malformed)?;
            if parts.next().is_some() {
                return Err(malformed());
            }
            Ok(Color::Rgb(
                number(red).ok_or_else(malformed)?,
                number(green).ok_or_else(malformed)?,
                number(blue).ok_or_else(malformed)?,
            ))
        }
        _ => Err(malformed()),
    }
}

/// One colour parameter: ASCII digits, and inside the range a terminal reads.
///
/// The range is the rule [`super::pacer`]'s shape check does *not* enforce --
/// it admits any run of digits -- so a `38;5;256` passes there and is refused
/// here, on both sides of the comparison.
pub(crate) fn number(param: &str) -> Option<u8> {
    if param.is_empty() || !param.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    param.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The decoder on its own, with no declaration beside it, for the cases
    /// that are about what a sequence *means* rather than about what an
    /// emitter promised.
    fn apply_for_tests(model: &mut TerminalModel, bytes: &[u8]) -> Result<(), Reject> {
        apply(model, bytes, None)
    }

    use crate::tui::layout::{self, Geometry};
    use std::time::{Duration, Instant};

    const COLOUR: &str = "\u{1b}[38;5;250m";
    const RESET: &str = "\u{1b}[0m";
    /// A three-person ZWJ family: one grapheme, two cells.
    const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";

    fn geometry() -> Geometry {
        layout::solve(24, 80, 1).expect("a band")
    }

    fn seed(grid: &Grid, caret: Option<(u16, u16)>) -> TerminalModel {
        TerminalModel::seed_primary(
            grid,
            grid.rows(),
            grid.cols(),
            caret,
            None,
            PlaneKind::Primary,
        )
        .expect("a shadow this crate painted")
    }

    /// A whole-screen footprint, for the cases that are about the cells rather
    /// than about the rows a vector may reach.
    fn anywhere(rows: u16, scroll: u32) -> Footprint {
        Footprint::new(
            PlaneKind::Primary,
            vec![
                Seg::Erase(1..=rows),
                Seg::Place(1..=rows),
                Seg::Scroll { rows: scroll },
            ],
        )
    }

    fn painted(line: u16, text: &str) -> Grid {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(line, text, &geometry);
        grid
    }

    /// Runs `pass` `passes` times, timing each run on its own, and returns the
    /// mean of them followed by the fastest, the middle and the slowest -- the
    /// middle being the upper of the two for an even count. The cost tests
    /// assert the mean and print the other three beside it as its spread.
    fn timed(passes: u32, mut pass: impl FnMut()) -> (Duration, Duration, Duration, Duration) {
        let mut each: Vec<Duration> = (0..passes)
            .map(|_| {
                let began = Instant::now();
                pass();
                began.elapsed()
            })
            .collect();
        let mean = each.iter().sum::<Duration>() / passes;
        each.sort_unstable();
        let count = each.len();
        (mean, each[0], each[count / 2], each[count - 1])
    }

    #[test]
    fn a_frame_that_paints_what_it_declares_is_accepted() {
        let before = Grid::blank(24, 80);
        let after = painted(10, "abc");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x1b[?2026h\x1b[?25l");
        before.diff(&after, &geometry(), &mut bytes);
        bytes.extend_from_slice(b"\x1b[10;4H");
        bytes.extend_from_slice(b"\x1b[?2026l\x1b[?25h");
        let declared = Declared::new(
            Intent::Primary {
                grid: &after,
                caret: Some((10, 4)),
                cursor_visible: Some(true),
                title: None,
            },
            anywhere(24, 0),
        );
        preflight(&seed(&before, None), &bytes, &declared).expect("the frame it declared");
    }

    #[test]
    fn a_frame_that_lost_its_cursor_show_is_refused() {
        // §5.7: the delimiters hardcode it, which is exactly the condition
        // under which a regression would otherwise be invisible.
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[?2026h\x1b[?25l\x1b[1;1H\x1b[?2026l".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((1, 1)),
                cursor_visible: Some(true),
                title: None,
            },
            anywhere(24, 0),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a hidden cursor");
        assert_eq!(reject.shape(), "a cursor visibility nobody asked for");
    }

    #[test]
    fn a_vector_that_addresses_a_row_outside_the_footprint_is_refused() {
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[5;1Hx".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((5, 2)),
                cursor_visible: Some(true),
                title: None,
            },
            Footprint::new(PlaneKind::Primary, vec![Seg::Place(20..=24)]),
        );
        let reject =
            preflight(&seed(&grid, None), &bytes, &declared).expect_err("a document row written");
        assert_eq!(reject.shape(), "a placement outside the footprint");
    }

    #[test]
    fn a_scroll_of_one_row_too_many_is_refused() {
        let grid = Grid::blank(24, 80);
        let mut bytes = Vec::new();
        for _ in 0..3 {
            bytes.extend_from_slice(b"\x1b[24;1H\n");
        }
        let declared = Declared::new(
            Intent::ScrollOnly {
                caret: (24, 1),
                rows: 2,
            },
            Footprint::new(PlaneKind::Primary, vec![Seg::Scroll { rows: 2 }]),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared)
            .expect_err("a row pushed into scrollback for good");
        assert_eq!(
            reject.shape(),
            "a scroll count the footprint did not ask for"
        );
    }

    #[test]
    fn a_linefeed_whose_caret_move_went_missing_scrolls_nothing_and_is_refused() {
        // The `CUP` in front of every linefeed is what makes it a scroll:
        // from anywhere else it walks the caret down and the screen does not
        // move at all.
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[24;1H\n\n".to_vec();
        let declared = Declared::new(
            Intent::ScrollOnly {
                caret: (24, 1),
                rows: 3,
            },
            Footprint::new(PlaneKind::Primary, vec![Seg::Scroll { rows: 3 }]),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a short scroll");
        assert_eq!(
            reject.shape(),
            "a scroll count the footprint did not ask for"
        );
    }

    #[test]
    fn a_foreign_cell_departs_with_a_declared_scroll_and_the_bottom_row_is_blank() {
        // §5.4's two halves at once: the top row's tokens leave the plane, and
        // the row the scroll frees is *known* blank rather than unknown.
        let mut model = TerminalModel::seed_foreign(24, 80, Some((24, 1)));
        apply_for_tests(&mut model, b"\x1b[24;1H\n").expect("a scroll");
        assert_eq!(
            *model.primary.logical(0),
            CellState::Foreign(80),
            "the token that was on row two is not on row one"
        );
        assert_eq!(
            *model.primary.logical(23 * 80),
            CellState::Empty,
            "the row the scroll freed is not known blank"
        );
    }

    #[test]
    fn a_foreign_cell_the_vector_did_not_declare_a_scroll_for_stays_where_it_was() {
        let mut model = TerminalModel::seed_foreign(24, 80, Some((1, 1)));
        apply_for_tests(&mut model, b"\x1b[2;1Hx").expect("one cell");
        assert_eq!(*model.primary.logical(0), CellState::Foreign(0));
        assert_eq!(*model.primary.logical(80 + 1), CellState::Foreign(81));
    }

    #[test]
    fn a_declared_scroll_bigger_than_the_screen_empties_it() {
        let mut model = TerminalModel::seed_foreign(6, 20, Some((6, 1)));
        for _ in 0..6 {
            apply_for_tests(&mut model, b"\x1b[6;1H\n").expect("a scroll");
        }
        assert!(
            (0..model.primary.cells.len()).all(|at| *model.primary.logical(at) == CellState::Empty),
            "a plane scrolled by its own height still holds a foreign cell"
        );
    }

    #[test]
    fn a_colour_out_of_range_is_refused_by_both_parsers() {
        // §5.6: `is_palette_sgr` checks shape and digits, not range, so this is
        // the rule that has to live in both decoders.
        assert!(wire_color("38;5;256", 0).is_err());
        assert!(wire_color("38;2;1;2", 0).is_err());
        assert!(wire_color("1", 0).is_err());
        assert!(wire_color("48;5;1", 0).is_err());
        assert_eq!(
            wire_color("38;5;250", 0).expect("a colour"),
            Color::Indexed(250)
        );
        assert_eq!(
            wire_color("38;2;1;2;3", 0).expect("a colour"),
            Color::Rgb(1, 2, 3)
        );
        assert_eq!(wire_color("", 0).expect("a reset"), Color::Default);
    }

    #[test]
    fn an_indexed_colour_is_never_the_rgb_a_theme_would_render_it_as() {
        assert_ne!(Color::Indexed(4), Color::Rgb(0, 0, 238));
    }

    #[test]
    fn a_cell_painted_in_the_wrong_colour_is_refused() {
        // The assembly check with the two parsers doing the work: the vector
        // opens a colour the intent does not have, and nothing about the
        // grapheme differs.
        let before = Grid::blank(24, 80);
        let after = painted(10, "abc");
        let bytes = format!("\x1b[10;1H{COLOUR}abc{RESET}\x1b[10;4H").into_bytes();
        let declared = Declared::new(
            Intent::Primary {
                grid: &after,
                caret: Some((10, 4)),
                cursor_visible: None,
                title: None,
            },
            anywhere(24, 0),
        );
        let reject = preflight(&seed(&before, None), &bytes, &declared)
            .expect_err("a colour nobody asked for");
        assert_eq!(
            reject.shape(),
            "a cell the vector did not leave as intended"
        );

        // And the same vector against the intent that *does* carry the colour.
        let coloured = painted(10, &format!("{COLOUR}abc{RESET}"));
        let declared = Declared::new(
            Intent::Primary {
                grid: &coloured,
                caret: Some((10, 4)),
                cursor_visible: None,
                title: None,
            },
            anywhere(24, 0),
        );
        preflight(&seed(&before, None), &bytes, &declared).expect("the colour it declared");
    }

    #[test]
    fn a_sequence_outside_the_alphabet_is_refused_rather_than_skipped() {
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[10;1H\x1b[2 q".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((10, 1)),
                cursor_visible: None,
                title: None,
            },
            anywhere(24, 0),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a cursor style");
        assert_eq!(reject.shape(), "a control sequence outside the alphabet");
    }

    #[test]
    fn a_title_the_intent_did_not_ask_for_is_refused() {
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b]2;xfx \xc2\xb7 a-model\x07\x1b[1;1H".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((1, 1)),
                cursor_visible: None,
                title: Some("xfx \u{b7} another-model"),
            },
            anywhere(24, 0),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("the wrong title");
        assert_eq!(reject.shape(), "a title that is not the one asked for");
    }

    #[test]
    fn a_wide_cluster_takes_two_cells_and_its_continuation_is_modelled() {
        let after = painted(10, FAMILY);
        let bytes = format!("\x1b[10;1H{FAMILY}\x1b[10;3H").into_bytes();
        let declared = Declared::new(
            Intent::Primary {
                grid: &after,
                caret: Some((10, 3)),
                cursor_visible: None,
                title: None,
            },
            anywhere(24, 0),
        );
        preflight(&seed(&Grid::blank(24, 80), None), &bytes, &declared).expect("a family");
    }

    #[test]
    fn a_query_vector_asserts_the_order_the_fence_needs() {
        let model = TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Primary);
        let reversed = b"\x1b[6n\x1b]11;?\x1b\\".to_vec();
        let declared = Declared::new(
            Intent::Queries(&[QueryId::Background, QueryId::CursorPosition]),
            Footprint::none(PlaneKind::Primary),
        );
        let reject = preflight(&model, &reversed, &declared).expect_err("a reversed fence");
        assert_eq!(
            reject.shape(),
            "queries in an order the fence does not have"
        );

        let ordered = b"\x1b]11;?\x1b\\\x1b[6n".to_vec();
        preflight(&model, &ordered, &declared).expect("the order the fence needs");
    }

    #[test]
    fn the_theme_mode_query_is_a_query_and_only_inside_one() {
        // `CSI ? 996 n` is the running session's "which way round are you
        // now?" (`super::super::theme::MODE_QUERY`). Two things have to be
        // true of it and neither follows from the other: a vector that says it
        // asks it is accepted, and a vector that asks it without saying so is
        // refused -- a query whose answer arrives in the composer is exactly
        // the failure `queries` exists to catch.
        let model = TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Primary);
        let asked = b"\x1b[?996n".to_vec();
        preflight(
            &model,
            &asked,
            &Declared::new(
                Intent::Queries(&[QueryId::ThemeMode]),
                Footprint::none(PlaneKind::Primary),
            ),
        )
        .expect("a declared theme-mode query");

        let grid = Grid::blank(24, 80);
        let smuggled = b"\x1b[?996n\x1b[1;1H".to_vec();
        let reject = preflight(
            &seed(&grid, None),
            &smuggled,
            &Declared::new(
                Intent::Primary {
                    grid: &grid,
                    caret: Some((1, 1)),
                    cursor_visible: None,
                    title: None,
                },
                anywhere(24, 0),
            ),
        )
        .expect_err("a query inside a band frame");
        assert_eq!(reject.shape(), "a query this vector did not declare");
    }

    #[test]
    fn a_private_report_this_crate_does_not_ask_for_is_still_outside_the_alphabet() {
        // The arm that admits `? 996 n` admits **that** report and no other.
        // `CSI ? 6 n` is the private cursor report -- a real sequence, one byte
        // from the one above -- and this crate does not write it; a grammar
        // that let the private marker through on any parameter would accept it
        // and every other private `n` a future edit mistyped.
        let model = TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Primary);
        for spelling in [&b"\x1b[?6n"[..], b"\x1b[?997n", b"\x1b[?996;1n"] {
            let reject = preflight(
                &model,
                spelling,
                &Declared::new(
                    Intent::Queries(&[QueryId::ThemeMode]),
                    Footprint::none(PlaneKind::Primary),
                ),
            )
            .expect_err("a private report this crate does not write");
            assert_eq!(
                reject.shape(),
                "a report this crate does not ask for",
                "{spelling:?}"
            );
        }
    }

    #[test]
    fn the_cursor_report_did_not_become_the_theme_query() {
        // The other half of the same boundary: the plain `CSI 6 n` arm was not
        // widened to carry the new one. A vector that asks the cursor where it
        // is, declared as a theme query, is a mismatch -- and would not be if
        // one arm answered for both.
        let model = TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Primary);
        let reject = preflight(
            &model,
            b"\x1b[6n",
            &Declared::new(
                Intent::Queries(&[QueryId::ThemeMode]),
                Footprint::none(PlaneKind::Primary),
            ),
        )
        .expect_err("a cursor report declared as a theme query");
        assert_eq!(
            reject.shape(),
            "queries in an order the fence does not have"
        );
    }

    #[test]
    fn the_mode_set_declares_its_theme_subscription_and_a_frame_may_not_smuggle_one() {
        // `?2031h` is a mode like every other one this crate writes, which
        // means both halves of the rule that covers `?2004` and `?7` cover it:
        // the vector that announces the session declares it, and a band frame
        // that carried one would be moving a mode nothing asked to move --
        // here, subscribing a terminal to reports that reach the composer.
        //
        // The bytes are the announce vector spelled out; the expectation is
        // `ModeSet::announced`, which is written by hand rather than parsed out
        // of `term::MODE_SET`, so the two sides cannot agree by construction.
        let model = TerminalModel::seed_modes(1, 1, ModeSet::fresh(), 0, PlaneKind::Primary);
        let announced = b"\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l\x1b[?2031h\x1b[22;2t".to_vec();
        preflight(
            &model,
            &announced,
            &Declared::new(
                Intent::Modes {
                    modes: ModeSet::announced(false),
                    title_stack: 1,
                    plane: PlaneKind::Primary,
                    cursor_visible: None,
                },
                Footprint::none(PlaneKind::Primary),
            ),
        )
        .expect("the announce vector");

        let grid = Grid::blank(24, 80);
        let smuggled = b"\x1b[?2031h\x1b[1;1H".to_vec();
        let reject = preflight(
            &seed(&grid, None),
            &smuggled,
            &Declared::new(
                Intent::Primary {
                    grid: &grid,
                    caret: Some((1, 1)),
                    cursor_visible: None,
                    title: None,
                },
                anywhere(24, 0),
            ),
        )
        .expect_err("a subscription inside a band frame");
        assert_eq!(
            reject.shape(),
            "a terminal mode this vector did not declare"
        );
    }

    #[test]
    fn the_restore_gives_the_theme_subscription_back() {
        // What the exit's vector says it leaves behind, and the one field of it
        // this slice adds. A restore that dropped its `?2031l` would leave the
        // user's next program reading theme reports off its own standard input,
        // and no cell anywhere would have moved to say so.
        let model =
            TerminalModel::seed_modes(1, 1, ModeSet::announced(false), 1, PlaneKind::Primary);
        let restore =
            b"\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?2031l\x1b[?7h\x1b[?25h".to_vec();
        preflight(
            &model,
            &restore,
            &Declared::new(
                Intent::Modes {
                    modes: ModeSet::restored(false),
                    title_stack: 0,
                    plane: PlaneKind::Primary,
                    cursor_visible: Some(true),
                },
                Footprint::none(PlaneKind::Primary),
            ),
        )
        .expect("the restore vector");

        let forgotten = b"\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?7h\x1b[?25h".to_vec();
        let reject = preflight(
            &model,
            &forgotten,
            &Declared::new(
                Intent::Modes {
                    modes: ModeSet::restored(false),
                    title_stack: 0,
                    plane: PlaneKind::Primary,
                    cursor_visible: Some(true),
                },
                Footprint::none(PlaneKind::Primary),
            ),
        )
        .expect_err("a restore that kept the subscription");
        assert_eq!(
            reject.shape(),
            "a terminal mode this vector did not declare"
        );
    }

    #[test]
    fn a_reject_carries_no_row_text() {
        // §10's containment rule, on the reject path too: a refusal travels
        // into an error message, and the rows are the user's text.
        let grid = Grid::blank(24, 80);
        let secret = "sk-not-a-real-credential-000";
        let bytes = format!("\x1b[3;1H{secret}").into_bytes();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((3, 1)),
                cursor_visible: None,
                title: None,
            },
            Footprint::new(PlaneKind::Primary, vec![Seg::Place(20..=24)]),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a refusal");
        let reported = reject.to_string();
        assert!(
            !reported.contains(secret) && !reported.contains("sk-"),
            "a reject carried the row it refused: {reported}"
        );
    }

    #[test]
    fn an_erase_after_a_scroll_on_a_row_the_vector_does_not_place_on_is_refused() {
        // §5.9's ordering: the vacated document rows are erased *before* the
        // scroll, or they reach native scrollback with their old text on them.
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[24;1H\n\x1b[5;1H\x1b[K".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((5, 1)),
                cursor_visible: None,
                title: None,
            },
            Footprint::new(
                PlaneKind::Primary,
                vec![
                    Seg::Erase(1..=10),
                    Seg::Scroll { rows: 1 },
                    Seg::Place(20..=24),
                ],
            ),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a late erase");
        assert_eq!(reject.shape(), "an erase after a scroll");
    }

    #[test]
    fn a_caret_addressed_off_the_screen_is_refused() {
        let grid = Grid::blank(24, 80);
        let bytes = b"\x1b[25;1H".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: Some((25, 1)),
                cursor_visible: None,
                title: None,
            },
            anywhere(24, 0),
        );
        let reject = preflight(&seed(&grid, None), &bytes, &declared).expect_err("a clamped CUP");
        assert_eq!(reject.shape(), "a caret placed off the screen");
    }

    #[test]
    fn giving_the_borrowed_plane_back_restores_the_caret_it_saved() {
        let grid = Grid::blank(24, 80);
        let mut model = seed(&grid, Some((7, 3)));
        apply_for_tests(&mut model, b"\x1b[?1049h").expect("the plane");
        assert_eq!(model.plane, PlaneKind::Alternate);
        apply_for_tests(&mut model, b"\x1b[?1049l").expect("the plane back");
        assert_eq!(model.caret, Caret::Known((7, 3)));

        // And where nothing saved one, the caret is unknown rather than stale.
        let mut model =
            TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Alternate);
        model.caret = Caret::Known((4, 4));
        apply_for_tests(&mut model, b"\x1b[?1049l").expect("the plane back");
        assert_eq!(model.caret, Caret::Unknown);
    }

    /// What one preflight costs on a screen, measured rather than assumed.
    ///
    /// This unit is what puts a decoder in front of **every** released frame,
    /// so the cost is its own to discharge and not a later unit's: the ladder's
    /// painter budget is 8 ms a frame at 80x24 and 32 ms at 300x200
    /// (`.prd/tui-phase3/ssot.md` P3-WRAP), and a checker that ate one of those
    /// would be a checker nobody could ship. The four vectors are the shapes a
    /// session really writes: an unchanged frame, one changed cell, a whole
    /// band repainted, and a scroll.
    ///
    /// The numbers are printed under `--nocapture` so a run can be read as
    /// evidence rather than as a pass mark, and there is no sampling anywhere:
    /// every released frame is checked, and a miss here escalates.
    ///
    /// **The budget is the shipped binary's, and it is asserted only there.**
    /// An unoptimized build measures `rustc -O0` rather than the product, and
    /// -- the reason this changed -- the ordinary suite runs its tests in
    /// parallel, so an elapsed-time assertion inside it is read against whatever
    /// else the machine is doing. This test therefore *measures* in every
    /// profile, and *asserts* only under optimization; the release assertion is
    /// run deliberately, one test at a time, by
    /// `scripts/check-tui-preflight-cost.sh`.
    ///
    /// Running the two timing tests alone does not make the machine exclusive
    /// to them. It removes the one confound this repository controls -- the
    /// suite's own parallelism -- and nothing more.
    ///
    /// The workload itself runs in both profiles and still asserts what it
    /// always did about **correctness**: every vector below is a real
    /// `preflight` that must accept what it declared.
    #[test]
    fn a_preflight_costs_a_small_fraction_of_one_frames_budget() {
        for (rows, cols, budget) in [(24u16, 80u16, 8u128), (200, 300, 32)] {
            let geometry = layout::solve(rows, cols, 1).expect("a band");
            let mut shadow = Grid::blank(rows, cols);
            for line in 1..=rows {
                shadow.place_row(line, &"x".repeat(usize::from(cols) / 2), &geometry);
            }
            let mut full = shadow.clone();
            for line in 1..=rows {
                full.place_row(line, &"y".repeat(usize::from(cols) - 1), &geometry);
            }
            let mut sparse = shadow.clone();
            sparse.place_row(rows / 2, "one cell changed", &geometry);
            let mut scrolled = shadow.clone();
            scrolled.scroll_up(1);

            let seed = seed(&shadow, Some((rows, 1)));
            for (name, target, scroll) in [
                ("unchanged", &shadow, 0u32),
                ("sparse", &sparse, 0),
                ("full", &full, 0),
                ("scroll", &scrolled, 1),
            ] {
                let mut bytes = Vec::new();
                for _ in 0..scroll {
                    bytes.extend_from_slice(format!("\u{1b}[{rows};1H").as_bytes());
                    bytes.push(b'\n');
                }
                let mut moved = shadow.clone();
                for _ in 0..scroll {
                    moved.scroll_up(1);
                }
                moved.diff(target, &geometry, &mut bytes);
                let declared = Declared::new(
                    Intent::Primary {
                        grid: target,
                        caret: None,
                        cursor_visible: None,
                        title: None,
                    },
                    anywhere(rows, scroll),
                );
                let (mean, min, median, max) = timed(20, || {
                    preflight(&seed, &bytes, &declared).expect("the vector it declared");
                });
                eprintln!(
                    "preflight {rows}x{cols} {name}: {} bytes in {mean:?} mean, {min:?} min, {median:?} median, {max:?} max (budget {budget} ms)",
                    bytes.len()
                );
                // The mean of twenty passes is what is held to the budget, as
                // it always was; the fastest, the middle and the slowest pass
                // are printed beside it as a diagnostic and assert nothing. The
                // spread is there because of the replay below: on GitHub's
                // shared `macos-15-intel` runner that one averaged 6.55, 7.10,
                // 7.36, 7.71, 8.40 and 10.32 ms against its 8 ms budget -- two
                // failures -- where the other three runners measured 2.8 to
                // 4.5 ms, and a mean on its own cannot say whether a miss like
                // that was every pass or a few slow ones.
                assert!(
                    cfg!(debug_assertions) || mean.as_millis() < budget,
                    "a {rows}x{cols} {name} preflight took {mean:?} on average ({min:?} min, {median:?} median, {max:?} max) against a {budget} ms frame budget"
                );
            }
        }
    }

    #[test]
    fn a_cell_changed_where_the_expectation_cannot_describe_it_is_refused() {
        // The target grid a band builds is the size of its own shadow, and the
        // shadow is resized by the frame path alone -- so a vector may be
        // declared against a grid with fewer rows than the screen it is
        // addressing. Comparing only the overlap turns those rows into cells
        // nobody looks at; they are compared against the seed instead, and a
        // change there is a change nothing declared.
        // A band whose shadow is still the smaller screen's, seeded at the
        // screen the vector is really being built for.
        let stale = Grid::blank(24, 80);
        let seed =
            TerminalModel::seed_primary(&stale, 40, 80, Some((1, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let bytes = b"\x1b[30;1Hx".to_vec();
        let declared = Declared::new(
            Intent::Primary {
                grid: &stale,
                caret: Some((30, 2)),
                cursor_visible: None,
                title: None,
            },
            anywhere(40, 0),
        );
        let refused = preflight(&seed, &bytes, &declared)
            .expect_err("a cell the expectation cannot describe");
        assert!(
            refused.to_string().contains("describe") || refused.to_string().contains("declared"),
            "the refusal did not name the declaration: {refused}"
        );
    }

    /// A seed whose every cell is somebody else's, and a declaration that
    /// scrolls it by `rows`.
    fn foreign_scroll(rows: u32) -> (TerminalModel, Footprint) {
        (
            TerminalModel::seed_foreign(6, 20, Some((6, 1))),
            Footprint::new(PlaneKind::Primary, vec![Seg::Scroll { rows }]),
        )
    }

    #[test]
    fn a_declared_scroll_moves_every_surviving_token_to_exactly_its_new_address() {
        // The token is the identity: it is assigned once at seed time and never
        // recomputed, so "this foreign cell was preserved" is an equality
        // between the cell that is there now and the cell that was there then,
        // at the address the **declared** scroll predicts -- not at whatever
        // address the decoded bytes happened to leave it.
        let (seed, footprint) = foreign_scroll(1);
        let declared = Declared::new(
            Intent::ScrollOnly {
                caret: (6, 1),
                rows: 1,
            },
            footprint,
        );
        let after = preflight(&seed, b"\x1b[6;1H\n", &declared).expect("one declared row");
        assert_eq!(
            *after.primary.logical(0),
            CellState::Foreign(20),
            "the token that was on row two is not on row one"
        );
        assert_eq!(
            *after.primary.logical(5 * 20),
            CellState::Empty,
            "the row the scroll freed is not known blank"
        );
    }

    #[test]
    fn a_token_that_stayed_where_a_declared_scroll_should_have_moved_it_rejects() {
        // The departure is **required**, not merely tolerated: the top rows of
        // a declared scroll leave the plane, and a screen still holding them is
        // a screen that did not scroll -- whatever its linefeeds said.
        let (seed, footprint) = foreign_scroll(1);
        let geometry = geometry();
        let script = Script::new(&geometry);
        let declared = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            footprint,
        );
        // A vector that scrolls nothing at all: the count catches it.
        let refused =
            preflight(&seed, b"\x1b[1;1H\n", &declared).expect_err("a scroll that never happened");
        assert_eq!(
            refused.shape(),
            "a scroll count the footprint did not ask for"
        );
    }

    #[test]
    fn a_foreign_cell_a_declaration_covers_may_be_overwritten_and_one_it_does_not_may_not() {
        // The asymmetry the brief asks for, both ways round: an unknown payload
        // is not permission to erase it, and a declared erase is.
        let seed = TerminalModel::seed_foreign(6, 20, Some((1, 1)));
        let geometry = geometry();
        // The edit itself, declared in order: the footprint below is what the
        // two halves of this case differ in.
        let mut script = Script::new(&geometry);
        script.erase(3);
        let declared = |range: RangeInclusive<u16>| {
            Declared::new(
                Intent::Document {
                    script: &script,
                    caret: None,
                    cursor_visible: None,
                    title: None,
                },
                Footprint::new(PlaneKind::Primary, vec![Seg::Erase(range)]),
            )
        };
        // Row three, erased, inside a declaration that names row three.
        preflight(&seed, b"\x1b[3;1H\x1b[K", &declared(3..=3)).expect("the row it declared");
        // And the same erase where the declaration names another row.
        let refused = preflight(&seed, b"\x1b[3;1H\x1b[K", &declared(4..=4))
            .expect_err("a foreign row nobody declared");
        assert_eq!(refused.shape(), "an erase outside the footprint");
    }

    #[test]
    fn a_row_a_scroll_freed_must_be_blank_or_declared() {
        // The bottom of a scroll is a **known** blank row, and a vector that
        // left something on one without saying so is refused. Built by hand,
        // because no emitter can produce it: the applier blanks what a scroll
        // frees, so the case is reachable only by declaring a scroll the vector
        // does not make -- which the count refuses first -- or by writing on
        // the freed row, which is this.
        let seed = TerminalModel::seed_foreign(6, 20, Some((6, 1)));
        let geometry = geometry();
        let mut script = Script::new(&geometry);
        script.scroll();
        script.place(6, "x");
        let declared = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            Footprint::new(
                PlaneKind::Primary,
                vec![Seg::Scroll { rows: 1 }, Seg::Place(6..=6)],
            ),
        );
        // Declared, and therefore allowed: the row the scroll freed is the row
        // this vector said it would place on.
        preflight(&seed, b"\x1b[6;1H\n\x1b[6;1Hx", &declared).expect("the row it declared");

        // And the same write with nothing declaring it.
        let bare = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            Footprint::new(PlaneKind::Primary, vec![Seg::Scroll { rows: 1 }]),
        );
        let refused = preflight(&seed, b"\x1b[6;1H\n\x1b[6;1Hx", &bare)
            .expect_err("a cell on a row nobody declared");
        assert_eq!(refused.shape(), "a placement outside the footprint");
    }

    #[test]
    fn a_vector_that_wrote_on_the_other_buffer_is_refused_whatever_its_row_numbers() {
        // The two planes share one row-number space, so a footprint spelt in
        // rows cannot tell them apart: the plane an effect landed on is what
        // this compares. The vector below writes on the normal buffer, takes
        // the borrowed one, and paints exactly what it declared there.
        let blank = Grid::blank(6, 20);
        let seed =
            TerminalModel::seed_primary(&blank, 6, 20, Some((1, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let surface = Grid::blank(6, 20);
        let declared = Declared::new(
            Intent::Alternate {
                grid: &surface,
                caret: (1, 1),
                cursor_visible: None,
            },
            Footprint::new(
                PlaneKind::Alternate,
                vec![Seg::Erase(1..=6), Seg::Place(1..=6)],
            )
            .moving(PlaneMove::Take),
        );
        let refused = preflight(&seed, b"\x1b[1;1HX\x1b[?1049h\x1b[1;1H", &declared)
            .expect_err("a cell left on the buffer the terminal saved");
        assert_eq!(
            refused.shape(),
            "a cell written on the buffer this vector may not touch"
        );

        // Without the stray cell the same vector is accepted.
        preflight(&seed, b"\x1b[?1049h\x1b[1;1H", &declared).expect("the surface it declared");
    }

    #[test]
    fn a_vector_that_took_the_other_buffer_and_gave_it_back_is_refused() {
        // The final plane is not the question: a vector that took the borrowed
        // buffer and handed it back has saved and restored the user's screen in
        // between, and one that did it twice has overwritten the save slot.
        let blank = Grid::blank(6, 20);
        let seed =
            TerminalModel::seed_primary(&blank, 6, 20, Some((1, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let geometry = geometry();
        let script = Script::new(&geometry);
        let declared = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            Footprint::none(PlaneKind::Primary),
        );
        let refused = preflight(&seed, b"\x1b[?1049h\x1b[?1049l", &declared)
            .expect_err("a round trip through the other buffer");
        assert_eq!(
            refused.shape(),
            "a buffer transition the vector did not declare"
        );
    }

    #[test]
    fn a_replayed_append_costs_a_small_fraction_of_one_frames_budget() {
        // The other half of the cost gate, and the one the ordered replay
        // added: a long append is checked row by row *as it scrolls*, so what
        // is measured here is a vector that delivers more rows than the
        // document area holds -- every one of them compared before it leaves.
        //
        // A thousand rows rather than thirty, because the shape of the cost is
        // the question: per scroll the replay writes one row of expectation,
        // compares one row, and moves both planes, so a vector ten times longer
        // should cost about ten times as much and not a hundred.
        for (rows, cols, budget) in [(24u16, 80u16, 8u128), (200, 300, 32)] {
            let geometry = layout::solve(rows, cols, 1).expect("a band");
            let bottom = geometry.band_top().saturating_sub(1);
            let text: Vec<String> = (0..1000).map(|index| format!("row-{index:04}")).collect();
            let mut bytes = Vec::new();
            let mut script = Script::new(&geometry);
            for row in &text {
                bytes.extend_from_slice(format!("\u{1b}[{rows};1H").as_bytes());
                bytes.push(b'\n');
                script.scroll();
                bytes.extend_from_slice(format!("\u{1b}[{bottom};1H{row}\u{1b}[K").as_bytes());
                script.place(bottom, row);
            }
            let declared = Declared::new(
                Intent::Document {
                    script: &script,
                    caret: None,
                    cursor_visible: None,
                    title: None,
                },
                Footprint::new(
                    PlaneKind::Primary,
                    vec![
                        Seg::Place(bottom..=bottom),
                        Seg::Scroll {
                            rows: u32::try_from(text.len()).expect("a thousand"),
                        },
                    ],
                ),
            );
            let seed = TerminalModel::seed_foreign(rows, cols, Some((rows, 1)));

            let (mean, min, median, max) = timed(15, || {
                preflight(&seed, &bytes, &declared).expect("the rows it declared");
            });
            eprintln!(
                "replay {rows}x{cols} 1000 rows: {} bytes in {mean:?} mean, {min:?} min, {median:?} median, {max:?} max (budget {budget} ms)",
                bytes.len()
            );
            // Asserted under optimization only, and run one test at a time by
            // `scripts/check-tui-preflight-cost.sh`: an elapsed-time assertion
            // inside a suite that runs its tests in parallel measures the
            // machine's other work as well as this one's. The workload runs in
            // every profile either way, and every vector in it must still be
            // accepted -- which is the correctness half, and is not timed.
            //
            // What is held to the budget is the mean of fifteen passes, the
            // same acceptance as ever; fifteen rather than five only makes
            // that mean steadier. Each pass is timed on its own so that the
            // fastest, the middle and the slowest can be printed beside it,
            // and they are a diagnostic that asserts nothing. They are printed
            // because running alone does not give a shared runner's scheduler
            // back: on GitHub's `macos-15-intel` runner the mean of five
            // passes at 24x80 came to 6.55, 7.10, 7.36, 7.71, 8.40 and
            // 10.32 ms against 8 ms -- two failures -- while the other three
            // runners measured 2.8 to 4.5 ms for the same code, and a mean on
            // its own cannot say whether a miss like that was every pass or a
            // few slow ones.
            assert!(
                cfg!(debug_assertions) || mean.as_millis() < budget,
                "a {rows}x{cols} thousand-row append replay took {mean:?} on average ({min:?} min, {median:?} median, {max:?} max) against a {budget} ms frame budget"
            );
        }
    }

    #[test]
    fn a_scroll_does_not_license_a_row_the_footprint_names_nowhere_near() {
        // The licence is read at the one position the edit really happened at,
        // not unioned across every position the row passed through. Unioned, a
        // single range plus a scroll as tall as the screen licensed *every*
        // row -- which emptied the sweep exactly on the vectors that move the
        // most irreversible content.
        let seed = TerminalModel::seed_foreign(24, 80, None);
        let declared = Declared::new(
            Intent::Cleanup {
                top: 24,
                caret: (24, 1),
                cursor_visible: true,
            },
            Footprint::new(
                PlaneKind::Primary,
                vec![Seg::Erase(24..=24), Seg::Scroll { rows: 1 }],
            ),
        );
        // The exit's own cleanup line from the screen's last row: erase, show
        // the cursor, and one linefeed that scrolls.
        preflight(&seed, b"\x1b[24;1H\x1b[J\x1b[?25h\n", &declared)
            .expect("the cleanup line as it stands");

        // And the same line with a row far above it disturbed on the way.
        let refused = preflight(
            &seed,
            b"\x1b[10;1H\x1b[K\x1b[24;1H\x1b[J\x1b[?25h\n",
            &declared,
        )
        .expect_err("a row the declaration names nowhere near");
        assert_eq!(refused.shape(), "an erase outside the footprint");
    }

    #[test]
    fn a_document_vector_is_replayed_in_order_rather_than_compared_at_the_end() {
        // The unit test under the emitter-level repro: a vector that writes a
        // row, scrolls it off, and leaves the final screen exactly as declared.
        // Only a comparison made **before** the eviction can see it.
        let geometry = layout::solve(24, 80, 1).expect("a band");
        let blank = Grid::blank(6, 20);
        let seed =
            TerminalModel::seed_primary(&blank, 6, 20, Some((6, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let mut script = Script::new(&geometry);
        script.scroll();
        script.place(6, "kept");
        let declared = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            // The top row is inside what this footprint permits, which is the
            // shape the append really has: every row it delivers is written on
            // a row it is allowed to write on, and the scrolls behind it carry
            // them away.
            Footprint::new(
                PlaneKind::Primary,
                vec![
                    Seg::Place(1..=1),
                    Seg::Place(6..=6),
                    Seg::Scroll { rows: 1 },
                ],
            ),
        );
        // What the declaration says: scroll, then place.
        preflight(&seed, b"\x1b[6;1H\n\x1b[6;1Hkept", &declared).expect("the row it declared");

        // And a row written on the **top** row before the scroll that carries it
        // into the terminal's own scrollback. The final screen is identical --
        // "kept" on row six, blanks above it -- the scroll count is identical,
        // and the footprint admits the row it was written on. Only a comparison
        // made before the eviction can see it.
        let refused = preflight(&seed, b"\x1b[1;1Hgone\x1b[6;1H\n\x1b[6;1Hkept", &declared)
            .expect_err("a row written on its way off the screen");
        assert_eq!(refused.shape(), "a row carried off the top of the screen");
    }

    #[test]
    fn a_row_leaving_the_top_is_compared_to_its_last_column() {
        // The departing row is read as one run of cells rather than cell by
        // cell through the origin, so this pins that the run reaches the end
        // of the row: a single glyph in the last column of a row about to be
        // carried off is refused, and at that column. A space written where
        // the declaration expects nothing is still nothing -- the one pair
        // of different kinds of cell that shows the same thing.
        let blank = Grid::blank(6, 20);
        let seed =
            TerminalModel::seed_primary(&blank, 6, 20, Some((6, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let geometry = geometry();
        let mut script = Script::new(&geometry);
        script.scroll();
        let declared = Declared::new(
            Intent::Document {
                script: &script,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            Footprint::new(
                PlaneKind::Primary,
                vec![Seg::Place(1..=1), Seg::Scroll { rows: 1 }],
            ),
        );
        let refused = preflight(&seed, b"\x1b[1;20Hx\x1b[6;1H\n", &declared)
            .expect_err("a glyph in the last column of a departing row");
        assert_eq!(refused.shape(), "a row carried off the top of the screen");
        assert_eq!(refused.cell, Some((1, 20)));

        preflight(&seed, b"\x1b[1;20H \x1b[6;1H\n", &declared)
            .expect("a space, which is what an untouched cell shows");
    }

    #[test]
    fn a_cluster_too_long_to_hold_inline_is_still_compared_whole() {
        // A modelled cell holds a short cluster inline and a long one on the
        // heap. This one is a letter and twelve combining marks, twenty-five
        // bytes, and both sides build it by joining: the marks come after a
        // colour, so the decoder and the tokenizer each find them as a cluster
        // of no width and add them to the letter in front of it, outgrowing the
        // inline space as they do.
        let blank = Grid::blank(6, 20);
        let seed =
            TerminalModel::seed_primary(&blank, 6, 20, Some((1, 1)), None, PlaneKind::Primary)
                .expect("a shadow this crate painted");
        let geometry = geometry();
        let written = format!("e{COLOUR}{}", "\u{301}".repeat(12));
        let bytes = format!("\u{1b}[3;1H{written}\u{1b}[K");
        let footprint = || Footprint::new(PlaneKind::Primary, vec![Seg::Place(3..=3)]);

        let mut same = Script::new(&geometry);
        same.place(3, &written);
        let declared = Declared::new(
            Intent::Document {
                script: &same,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            footprint(),
        );
        preflight(&seed, bytes.as_bytes(), &declared).expect("the row it declared");

        // The same length, and the same bytes up to the last mark.
        let other = format!("e{COLOUR}{}\u{302}", "\u{301}".repeat(11));
        let mut differs = Script::new(&geometry);
        differs.place(3, &other);
        let declared = Declared::new(
            Intent::Document {
                script: &differs,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            footprint(),
        );
        let refused = preflight(&seed, bytes.as_bytes(), &declared)
            .expect_err("a cluster that differs in its last mark");
        assert_eq!(
            refused.shape(),
            "a cell the vector did not leave as intended"
        );
        assert_eq!(refused.cell, Some((3, 1)));
    }

    #[test]
    fn a_declared_row_is_expected_as_exactly_the_cells_a_grid_would_hold() {
        // The replay does not place a declared row in a grid and read it back:
        // it takes the clusters of the same tokenizer `Grid::place_row` runs
        // straight into the cells it expects. This holds the two consumers of
        // that tokenizer to one answer on the rows where they could part -- a
        // colour mid-row, a wide cluster that would straddle the last column,
        // a mark that lands behind a wide cluster's second half, a mark with
        // nothing in front of it, a tab, controls the painter drops, a row
        // longer than the screen, and clusters too long to hold inline.
        let geometry = geometry();
        let script = Script::new(&geometry);
        let joined = format!("e{COLOUR}{}", "\u{301}".repeat(12));
        let coloured = format!("ab{COLOUR}cd{RESET}ef");
        let behind_wide = format!("\u{4e2d}{COLOUR}\u{301}x");
        let long = "x".repeat(100);
        let rows = [
            "plain text",
            "",
            coloured.as_str(),
            "abcdefghi\u{4e2d}",
            behind_wide.as_str(),
            "\u{301}abc",
            "a\tb",
            "a\u{7}b\u{1b}[2Jc",
            long.as_str(),
            FAMILY,
            joined.as_str(),
        ];
        for cols in [10u16, 80] {
            for row in rows {
                let mut grid = Grid::blank(1, cols);
                grid.place_row(1, row, &geometry);
                let mut expected = Expected::new(&Plane::blank(1, cols), &script);
                expected
                    .place(1, row, 0)
                    .expect("a row this crate can paint");
                let cells = expected.plane.row(1).expect("the one row");
                for column in 1..=cols {
                    let held = CellState::of(grid.cell(1, column).expect("a cell of the row"))
                        .expect("a slot this crate wrote");
                    assert_eq!(
                        cells[usize::from(column - 1)],
                        held,
                        "{row:?} at column {column} of {cols}"
                    );
                }
            }
        }
    }

    #[test]
    fn preflight_recovery_cleanup_accepts_only_the_exact_fixed_vector() {
        preflight_recovery_cleanup(RECOVERY_CLEANUP.as_bytes())
            .expect("the fixed vector this entry point exists to license");

        let mut mutated = RECOVERY_CLEANUP.as_bytes().to_vec();
        *mutated.last_mut().expect("a non-empty vector") ^= 1;
        preflight_recovery_cleanup(&mutated).expect_err("a mutated recovery vector");

        let truncated = &RECOVERY_CLEANUP.as_bytes()[..RECOVERY_CLEANUP.len() - 1];
        preflight_recovery_cleanup(truncated).expect_err("a short recovery vector");

        let extended = {
            let mut bytes = RECOVERY_CLEANUP.as_bytes().to_vec();
            bytes.push(b'x');
            bytes
        };
        preflight_recovery_cleanup(&extended).expect_err("a padded recovery vector");
    }

    #[test]
    fn the_ordinary_parser_still_refuses_the_recovery_vectors_bytes_on_their_own() {
        // The recovery-only entry point licenses this vector; the general one
        // still refuses the `CAN` and the `OSC 8` inside it exactly as it
        // refuses them anywhere else -- this entry point loosens nothing in
        // `apply`'s alphabet.
        let grid = Grid::blank(24, 80);
        let declared = Declared::new(
            Intent::Primary {
                grid: &grid,
                caret: None,
                cursor_visible: None,
                title: None,
            },
            Footprint::none(PlaneKind::Primary),
        );
        preflight(&seed(&grid, None), RECOVERY_CLEANUP.as_bytes(), &declared)
            .expect_err("the ordinary alphabet accepted the recovery vector");
    }

    #[test]
    fn the_recovery_cleanup_takes_and_gives_back_no_plane() {
        // What lets the one fixed vector recover a torn repaint of the
        // **alternate** plane as well as of the primary band
        // (`super::super::frame::Band::recover_alternate`): it lands on the
        // buffer the tear did and leaves the terminal there. A `?1049` in it
        // would hand the user's own screen back in the middle of a question,
        // or save a screen over the one the terminal is holding for them.
        //
        // The entry point that licenses it is a byte comparison and knows
        // nothing about planes, so it accepts the vector whichever one is up;
        // what this asserts is the vector itself. Its head is the `CAN` and the
        // `OSC 8` close this decoder's alphabet refuses on purpose -- a byte
        // and a string sequence, neither of which can carry a mode -- and
        // everything after them is decoded here, from a terminal that is on
        // the borrowed buffer, and must end on it having made no transition.
        preflight_recovery_cleanup(RECOVERY_CLEANUP.as_bytes())
            .expect("the fixed vector, whichever plane is up");
        let tail = RECOVERY_CLEANUP
            .strip_prefix("\x18\x1b]8;;\x07")
            .expect("the cleanup opens with the CAN and the hyperlink close");
        let mut model =
            TerminalModel::seed_modes(24, 80, ModeSet::fresh(), 0, PlaneKind::Alternate);
        apply_for_tests(&mut model, tail.as_bytes())
            .expect("everything after the head is a sequence this decoder knows");
        assert_eq!(
            model.moves,
            Vec::<PlaneKind>::new(),
            "the cleanup moved the terminal between its buffers"
        );
        assert_eq!(
            model.plane,
            PlaneKind::Alternate,
            "the cleanup left the buffer it landed on"
        );
        assert!(
            model.alternate.is_some(),
            "the cleanup gave the borrowed buffer back"
        );
    }
}
