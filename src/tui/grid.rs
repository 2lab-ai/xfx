//! The shadow of the band: what the terminal is holding, cell by cell, and the
//! bytes that turn it into what it should be holding.
//!
//! Phase 1 repainted the whole band every frame, and said so in
//! `docs/parity.md`: with no model of what was already on the screen, "repaint
//! everything" is the only honest thing a painter can do. This module is that
//! model. It is a **screen**, not a band -- a document append scrolls the
//! band's own rows up with everything else, so a shadow that covered only the
//! band would be wrong about where the band is the moment a turn answers.
//!
//! Three rules make the cells comparable at all:
//!
//! * **A cell holds a grapheme cluster, not a scalar.** `\u{65}\u{301}` is one
//!   cell, and so is a ZWJ family; `super::wrap::width` is what says how many
//!   columns one takes, and it is the same function the wrap and the clip use,
//!   so the grid cannot disagree with the row it was built from about where a
//!   column is.
//! * **A wide cluster owns its second column**, as a [`Cell::Continuation`]
//!   that is never emitted by itself. That is what makes "a family was
//!   overwritten by an `x`" a *two*-cell change: without it the second half of
//!   the emoji stays on the screen with nothing to erase it.
//! * **A cell remembers the attribute state it was painted under**
//!   ([`SgrState`]), canonically rather than as the bytes some row happened to
//!   spell it with. A diff starts writing in the middle of a row, so whatever
//!   the untouched prefix opened is not on the terminal any more and has to be
//!   re-opened at the first cell that is written.
//!
//! What the diff emits is `CUP`, replacement text with its attributes and its
//! OSC 8 links (`super::hyperlink`, closed at the end of every row), and `EL`.
//! An erase to the end of a row is what says a row got shorter. Around one
//! display unit it emits three more, upstream's (`src/core/terminal/
//! engine.zig:2548-2589@481104fd`), and so does every row a frame writes whole
//! ([`write_row_text`]): `ECH` over the two cells of a unit whose suffix holds a
//! variation selector, `CHA` to the column after such a unit, and `?7h`/`?7l`
//! around a wide unit with a suffix at the right margin ([`put_unit`]).
//! `super::check` and `scripts/smoke-tui.sh`'s oracle model all three.

use std::borrow::Cow;
use std::io::Write as _;

use super::frame::cup;
use super::layout::Geometry;
use super::pacer::SgrState;
use super::theme::{DocumentRow, Palette, Redraw};

/// Erase from the cursor to the end of the row it is on.
const ERASE_LINE: &str = "\u{1b}[K";

/// `ECH 2`: blank the two cells from the cursor, and leave it where it is.
const CLEAR_TWO: &str = "\u{1b}[2X";

/// Autowrap on, and off again: `super::term::MODE_SET` turns it off for the
/// whole session, so these two only ever bracket one unit ([`put_unit`]).
const AUTOWRAP_ON: &str = "\u{1b}[?7h";
const AUTOWRAP_OFF: &str = "\u{1b}[?7l";

/// Every attribute off.
///
/// Spelled here rather than taken from [`super::theme`]: this is the reset the
/// *painter* needs to close a run it opened, and a painter that reached for the
/// palette's would stop closing its runs the day the palette stopped having
/// one.
const RESET: &str = "\u{1b}[0m";

/// One column of the screen.
#[derive(Debug, Clone, Default)]
pub(crate) enum Cell {
    /// Nothing has been written here, and a terminal draws a space.
    #[default]
    Empty,
    /// The first column of a grapheme cluster, and everything needed to put it
    /// back: what it is, how many columns it takes, and what was switched on
    /// when it was painted.
    Lead {
        grapheme: String,
        width: u8,
        sgr: SgrState,
    },
    /// The second column of a wide cluster. Never emitted on its own -- the
    /// lead in front of it carries both -- and never the target of a `CUP`.
    Continuation,
}

impl PartialEq for Cell {
    /// Two cells are the same when a terminal would be showing the same thing.
    ///
    /// [`SgrState`] is compared by what it would *replay*, which is the whole
    /// of what it means to a screen: the model is a set of slots rather than a
    /// recording, so two rows that spelled one colour differently leave equal
    /// cells and cost no bytes.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty, Self::Empty) | (Self::Continuation, Self::Continuation) => true,
            (
                Self::Lead {
                    grapheme,
                    width,
                    sgr,
                },
                Self::Lead {
                    grapheme: other_grapheme,
                    width: other_width,
                    sgr: other_sgr,
                },
            ) => {
                grapheme == other_grapheme
                    && width == other_width
                    && sgr.reopen() == other_sgr.reopen()
            }
            _ => false,
        }
    }
}

/// One thing [`tokenize_row`] finds a row's text putting on the row.
#[derive(Debug)]
pub(crate) enum Placed<'a> {
    /// A cluster that takes cells: the zero-based column it starts in, the
    /// cluster, how many columns it takes, and what was switched on when it
    /// was reached.
    Lead {
        column: usize,
        cluster: &'a str,
        width: usize,
        sgr: &'a SgrState,
    },
    /// A cluster that takes no cell, for the cell in `column` -- the one in
    /// front of it -- to take as its own if that cell is a lead.
    Join { column: usize, cluster: &'a str },
}

/// What placing `row` on a row `cols` cells wide puts there, cluster by
/// cluster and in column order, handed to `put` one at a time.
///
/// The **one** tokenizer, and that is the point of routing it through
/// [`super::frame::row_text`]: the same function decides what a row may carry
/// when it is written to a terminal and what a grid believes a terminal is
/// showing. A second reading of a row -- one that kept a sequence the painter
/// drops, or measured a cluster differently -- would make the diff emit bytes
/// for cells that never changed and, worse, skip cells that did.
/// [`Grid::place_row`] writes what it finds into grid cells, and
/// `super::check`'s replay of a declared row writes it into the cells it
/// expects, so neither can read a row the other would not.
///
/// A [`Placed::Lead`] never starts left of the end of the one before it, so
/// nothing it reports is overwritten by what it reports later.
pub(crate) fn tokenize_row(
    row: &str,
    cols: u16,
    geometry: &Geometry,
    mut put: impl FnMut(Placed<'_>),
) {
    #[cfg(test)]
    note_tokenized(row.len());
    let budget = usize::from(cols);
    // Clipped to the narrower of the two, because a row built for a screen
    // this row is not the width of is a row it cannot hold.
    let text = super::frame::row_text(row, cols.min(geometry.cols));
    let mut sgr = SgrState::default();
    let mut column = 0usize;
    let mut rest = text.as_ref();
    while !rest.is_empty() {
        if let Some(len) = super::pacer::escape_at(rest) {
            // `row_text` keeps colours and admitted links and removes
            // everything else, so the only sequences that reach here are the
            // ones a cell is allowed to remember. Handed to the model whole;
            // what it does not model, it drops.
            sgr.observe(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        let Some(cluster) = super::wrap::first_cluster(rest) else {
            break;
        };
        rest = &rest[cluster.len()..];
        let width = usize::from(super::wrap::width(cluster));
        if width == 0 {
            // A combining mark that arrived on its own -- across a colour,
            // or as the first thing on the row. It belongs to the cluster
            // in front of it rather than to a cell of its own; with nothing
            // in front of it there is no cell for it to join and a terminal
            // would draw nothing either.
            if let Some(column) = column.checked_sub(1) {
                put(Placed::Join { column, cluster });
            }
            continue;
        }
        if column + width > budget {
            // `row_text` clips to the same width, so this is unreachable
            // through it; a cluster that straddled the last column would be
            // drawn in a column the layout believes is empty.
            break;
        }
        put(Placed::Lead {
            column,
            cluster,
            width,
            sgr: &sgr,
        });
        column += width;
    }
}

/// Writes one display unit at zero-based `column`, `width` cells wide, on a
/// screen `cols` wide: `grapheme` is the grid cell's, what the rule below is
/// decided on, and `write` puts its bytes on the wire (they can be split by a
/// sequence: [`write_cell`], [`write_row_text`]); `more` is whether anything
/// else is written on the row after it, a cell or an erase.
///
/// Upstream's frame writer, cell by cell (`src/core/terminal/engine.zig:
/// 2548-2589@481104fd`), and the **one** spelling of it: [`Grid::diff_from`]
/// writes each cell through this and [`write_row_text`] each unit of a row
/// written whole, so a full paint and a partial one cannot disagree.
///
/// * **A suffix that holds U+FE0E or U+FE0F** (`hasVariationSelector`,
///   `:3367-3369`) makes the unit's width one terminals dispute: Ghostty draws
///   U+26A1 U+FE0E in one cell and tmux in two (`95e6c8fd`). Such a unit two
///   cells wide has both cleared first (`ECH 2`), so a terminal that draws it
///   one cell narrow leaves no stale second cell; and when anything follows it
///   on the row the cursor is put on the column after it (`CHA`, one-based),
///   so what follows lands where this grid holds it however wide the terminal
///   drew the unit. An `EL` counts as following, which upstream's diff never
///   writes: an erase sent from a caret the terminal advanced by its own width
///   starts a cell early or late, and late leaves a stale cell -- the defect
///   the anchor is for.
/// * **A wide unit with any suffix that ends at the right margin** is written
///   with autowrap on, and autowrap is turned off again after it
///   (`guard_margin_suffix`): with it off, a terminal can drop the suffix of a
///   unit whose base fills the last column. Upstream asks whether autowrap is
///   off; it is off for the whole of a session here (`super::term::MODE_SET`),
///   on every frame this writes. Nothing follows such a unit, so it is never
///   anchored.
///
/// Upstream's order: the autowrap on, the clear, the unit, the autowrap off,
/// the anchor. Its colour and its link are already on (the caller's), so the
/// clear blanks the cells in the unit's own background.
fn put_unit(
    out: &mut Vec<u8>,
    grapheme: &str,
    column: usize,
    width: usize,
    cols: usize,
    more: bool,
    write: impl FnOnce(&mut Vec<u8>),
) {
    let mut scalars = grapheme.chars();
    scalars.next();
    let suffix = scalars.as_str();
    let disputed = suffix.contains(['\u{fe0e}', '\u{fe0f}']);
    let guarded = !suffix.is_empty() && width == 2 && column + width == cols;
    if guarded {
        out.extend_from_slice(AUTOWRAP_ON.as_bytes());
    }
    if disputed && width == 2 {
        out.extend_from_slice(CLEAR_TWO.as_bytes());
    }
    write(out);
    if guarded {
        out.extend_from_slice(AUTOWRAP_OFF.as_bytes());
    }
    let after = column + width;
    if disputed && more && after < cols {
        // Writing into a `Vec` cannot fail.
        let _ = write!(out, "\u{1b}[{}G", after + 1);
    }
}

/// Writes a grid cell's grapheme, `width` cells wide, so a terminal reads it
/// back as that one cell; `pen` is the attributes open as it is written (the
/// cell's own, [`reopen`]).
///
/// **Together when it reads back so**, which is nearly always. The one cell it
/// does not is a one-cell glyph that [`Grid::place_row`] joined a cluster to
/// across a sequence -- `☀` (one cell), a colour, U+FE0F: the grid keeps the
/// glyph's one cell, and the pair written together is one cluster two cells
/// wide (`super::wrap::width`), which a terminal draws, and `super::check`
/// reads, as two. So that cell is written split as its row was: the longest
/// part of it that is still its width, the pen again (a reset with nothing
/// on, the attributes otherwise -- a sequence that changes nothing), and the
/// rest, which a terminal joins to the cell in front of it as the grid did.
fn write_cell(out: &mut Vec<u8>, grapheme: &str, width: usize, pen: &str) {
    if usize::from(super::wrap::width(grapheme)) == width {
        out.extend_from_slice(grapheme.as_bytes());
        return;
    }
    let split = grapheme
        .char_indices()
        .map(|(at, _)| at)
        .skip(1)
        .filter(|&at| usize::from(super::wrap::width(&grapheme[..at])) == width)
        .last();
    let Some(at) = split else {
        out.extend_from_slice(grapheme.as_bytes());
        return;
    };
    out.extend_from_slice(&grapheme.as_bytes()[..at]);
    out.extend_from_slice(if pen.is_empty() { RESET } else { pen }.as_bytes());
    out.extend_from_slice(&grapheme.as_bytes()[at..]);
}

/// One thing [`write_row_text`] finds in a row, in the order the row has it.
enum Piece<'a> {
    /// A sequence the row keeps (a colour, a link), or a cluster that takes no
    /// cell: written where it stands.
    Raw(&'a str),
    /// A cluster that takes cells, as the row has it. `grapheme` is the cell
    /// [`Grid::place_row`] makes of it -- with the clusters that take no cell
    /// it joins to it, which it does only for a one-cell unit -- and `end` the
    /// index of the last piece it joined, so the unit's bytes are this piece
    /// through that one, the sequences between them included.
    Unit {
        cluster: &'a str,
        grapheme: Cow<'a, str>,
        column: usize,
        width: usize,
        end: usize,
    },
}

/// Writes `text` from the first column, as [`Grid::diff_from`] writes cells,
/// with nothing written after it on the row: `text` is a row
/// [`super::frame::row_text`] has made safe and cut to `cols`, the screen's
/// width.
///
/// What a frame's rows written whole owe the terminal around a unit, the same
/// as the diff's ([`put_unit`]): the alternate plane's paint and the band's
/// whole repaints write a row's text where the diff writes its cells, and a
/// row written whole that went on past a glyph with a variation selector would
/// put what follows it where the terminal's own advance does rather than where
/// the grid holds it -- `p5-transcript-diffwide2`'s read row on the full
/// transcript, `x` three cells left of where upstream's `ESC[42G` puts it
/// (`.prd/tui-phase5/receipts/engine-vs-anchor`). A document row is not a
/// frame's and is written as it stands (`super::frame`'s `place`).
///
/// **The row's bytes are written in the row's order**, and only the clear, the
/// anchor and the bracket are added around a unit. The rule is decided on the
/// cell [`tokenize_row`] and [`Grid::place_row`] make of the unit: a cluster
/// that takes no cell, after a sequence, is that cell's when the unit is one
/// cell wide (`☀`, a colour, U+FE0F is a selector glyph, anchored after the
/// selector) and no cell's when it is two (`한`, a colour, U+0301: the grid
/// keeps `한`). Moving such a cluster ahead of its sequence would make the
/// two one cluster on the wire, which `super::check` reads as one -- `한́`,
/// or `☀️` two cells wide -- where the grid holds another, and refuses
/// (review round 1). A row of ASCII alone has no unit with a suffix, and is
/// written as it stands.
pub(crate) fn write_row_text(out: &mut Vec<u8>, text: &str, cols: u16) {
    if text.is_ascii() {
        out.extend_from_slice(text.as_bytes());
        return;
    }
    let mut pieces: Vec<Piece<'_>> = Vec::new();
    let mut last_unit = None;
    let mut column = 0usize;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(len) = super::pacer::escape_at(rest) {
            pieces.push(Piece::Raw(&rest[..len]));
            rest = &rest[len..];
            continue;
        }
        let Some(cluster) = super::wrap::first_cluster(rest) else {
            break;
        };
        rest = &rest[cluster.len()..];
        let width = usize::from(super::wrap::width(cluster));
        if width == 0 {
            let at = pieces.len();
            pieces.push(Piece::Raw(cluster));
            // `place_row` joins it to the cell in front of the next free
            // column: the unit's own when it is one cell wide, its
            // continuation -- which takes nothing -- when it is two.
            if let Some(Piece::Unit {
                grapheme,
                width: 1,
                end,
                ..
            }) = last_unit.and_then(|unit| pieces.get_mut(unit))
            {
                grapheme.to_mut().push_str(cluster);
                *end = at;
            }
            continue;
        }
        last_unit = Some(pieces.len());
        pieces.push(Piece::Unit {
            cluster,
            grapheme: Cow::Borrowed(cluster),
            column,
            width,
            end: pieces.len(),
        });
        column += width;
    }
    let cols = usize::from(cols);
    let mut at = 0usize;
    while let Some(piece) = pieces.get(at) {
        match piece {
            Piece::Raw(bytes) => {
                out.extend_from_slice(bytes.as_bytes());
                at += 1;
            }
            Piece::Unit {
                grapheme,
                column,
                width,
                end,
                ..
            } => {
                let more = last_unit.is_some_and(|last| at < last);
                // The unit's own cluster and what it joined, as the row has
                // them: nothing in between takes a cell.
                let span = &pieces[at..=*end];
                put_unit(out, grapheme, *column, *width, cols, more, |out| {
                    for piece in span {
                        let (Piece::Raw(bytes) | Piece::Unit { cluster: bytes, .. }) = piece;
                        out.extend_from_slice(bytes.as_bytes());
                    }
                });
                at = end + 1;
            }
        }
    }
}

/// What a terminal is holding, or is about to be holding.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    rows: u16,
    cols: u16,
    /// Row-major, `rows * cols` long, and that length is an invariant every
    /// method below relies on: [`Grid::span`] is the only place a row becomes
    /// an index range.
    cells: Vec<Cell>,
    /// What each row is, `rows` long, as its writer recorded it
    /// ([`Grid::record`]): what a theme report repaints a row's cells as, or
    /// draws it again from ([`Grid::retint_document`]). Moved with the cells by
    /// every scroll, and plain again wherever a row is erased.
    records: Vec<Record>,
}

/// What a row's writer said about it: what it is, and how it draws it in each
/// mode when it can be drawn again ([`Redraw`]) -- and what this grid knows of
/// how its cells were made ([`Laid`]). One record per row rather than a vector
/// of each, so nothing that moves a row can move one half of what is known
/// about it.
#[derive(Debug, Clone, Default)]
struct Record {
    kind: DocumentRow,
    redraw: Option<Redraw>,
    laid: Option<Laid>,
}

/// The text whose placement ([`Grid::place_row`]) a row's cells are, and
/// everything else the placement read: the grid's width and the screen's
/// ([`tokenize_row`] cuts at the narrower and places within the grid's).
///
/// **Kept only while no cell of the row has changed since.** Every method that
/// changes a cell without placing a text forgets it -- an erase, a recolour, a
/// drawing put on the row ([`Grid::retint_document`]) -- and the moves
/// ([`Grid::scroll_up`], [`Grid::remove_rows`]) carry it with the cells. So
/// "the row was laid from this text" is a fact about the cells, which is what
/// lets [`Grid::row_matches`] answer for a row without reading it: comparing
/// the text costs one pass over its bytes, where placing it again reads it
/// cluster by cluster -- 133-171 ms for 8 MiB of marks
/// (`.prd/tui-phase5/receipts/cancel-marker/round3/probe.txt`).
///
/// Shared, not copied, between the shadow and the target cloned from it.
#[derive(Debug, Clone)]
struct Laid {
    text: std::sync::Arc<str>,
    cols: u16,
    screen: u16,
}

/// What a tool group's header row opens with (`super::toolgroup`'s header,
/// upstream's `●`).
const GROUP_MARK: &str = "\u{25cf}";

impl Grid {
    /// A screen with nothing on it.
    pub(crate) fn blank(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Cell::Empty; usize::from(rows) * usize::from(cols)],
            records: vec![Record::default(); usize::from(rows)],
        }
    }

    /// A screen of a different size, with nothing on it.
    ///
    /// Blanked rather than reflowed, and that is the honest answer rather than
    /// the cheap one: a terminal that was resized has re-wrapped its own
    /// document by rules xfx does not model, so *nothing* about what is on
    /// those rows is knowable any more. The next frame repaints the band whole.
    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        self.rows = rows;
        self.cols = cols;
        self.cells.clear();
        self.cells
            .resize(usize::from(rows) * usize::from(cols), Cell::Empty);
        self.records.clear();
        self.records.resize(usize::from(rows), Record::default());
    }

    pub(crate) fn rows(&self) -> u16 {
        self.rows
    }

    pub(crate) fn cols(&self) -> u16 {
        self.cols
    }

    /// Where row `line` -- one-based, as the terminal counts -- lives in
    /// [`cells`](Self::cells), or `None` when this grid has no such row.
    fn span(&self, line: u16) -> Option<std::ops::Range<usize>> {
        if line == 0 || line > self.rows {
            return None;
        }
        let start = usize::from(line - 1) * usize::from(self.cols);
        Some(start..start + usize::from(self.cols))
    }

    /// One cell of this grid, read-only, or `None` when there is no such cell.
    ///
    /// The **one** reader this grid grew for [`super::check`], and deliberately
    /// the only one: the checker keeps its own decoded cells and compares them
    /// semantically, so a whole-grid equality here would either be `Cell::eq` --
    /// the `reopen()`-based comparison the check exists to avoid -- or a second
    /// comparison nothing else uses.
    pub(crate) fn cell(&self, line: u16, column: u16) -> Option<&Cell> {
        if column == 0 || column > self.cols {
            return None;
        }
        let span = self.span(line)?;
        self.cells.get(span.start + usize::from(column - 1))
    }

    /// Blanks one row, as `EL` from its first column does.
    pub(crate) fn erase_row(&mut self, line: u16) {
        let Some(span) = self.span(line) else {
            return;
        };
        for cell in &mut self.cells[span] {
            *cell = Cell::Empty;
        }
        // Plain, with nothing to draw it again from, and laid from no text.
        if let Some(slot) = self.slot(line) {
            *slot = Record::default();
        }
    }

    /// Records what row `line` is, as its writer says
    /// (`super::transcript::Append::kinds`), with nothing to draw it again
    /// from. A row with no such line is nothing to record.
    #[cfg(test)]
    pub(crate) fn set_kind(&mut self, line: u16, kind: DocumentRow) {
        self.record(line, kind, None);
    }

    /// Records what row `line` is and, when its writer can draw it again, how
    /// it draws it in each mode (`super::transcript::Append::redraws`): the
    /// whole of what its writer says about the row, replacing what it said.
    /// What the row's cells were laid from ([`Laid`]) is not the writer's to
    /// say, and stays.
    pub(crate) fn record(&mut self, line: u16, kind: DocumentRow, redraw: Option<Redraw>) {
        if let Some(slot) = self.slot(line) {
            slot.kind = kind;
            slot.redraw = redraw;
        }
    }

    /// Row `line`'s record, or `None` when this grid has no such row.
    fn slot(&mut self, line: u16) -> Option<&mut Record> {
        line.checked_sub(1)
            .and_then(|index| self.records.get_mut(usize::from(index)))
    }

    /// What row `line` was recorded as, and plain for a row with no record.
    pub(crate) fn kind(&self, line: u16) -> DocumentRow {
        line.checked_sub(1)
            .and_then(|index| self.records.get(usize::from(index)))
            .map(|record| record.kind)
            .unwrap_or_default()
    }

    /// How row `line`'s writer draws it in each mode, when it recorded that.
    pub(crate) fn redraw(&self, line: u16) -> Option<&Redraw> {
        line.checked_sub(1)
            .and_then(|index| self.records.get(usize::from(index)))
            .and_then(|record| record.redraw.as_ref())
    }

    /// Puts `row`'s text on row `line`, and blanks the rest of it.
    ///
    /// The cells are the ones [`tokenize_row`] says the text puts there, which
    /// is the point of there being one tokenizer: the same function decides
    /// what a row may carry when it is written to a terminal and what this grid
    /// believes a terminal is showing.
    ///
    /// And remembers the text the cells were laid from ([`Laid`]).
    pub(crate) fn place_row(&mut self, line: u16, row: &str, geometry: &Geometry) {
        self.put_row(line, row, geometry);
        let (cols, screen) = (self.cols, geometry.cols);
        if let Some(slot) = self.slot(line) {
            slot.laid = Some(Laid {
                text: std::sync::Arc::from(row),
                cols,
                screen,
            });
        }
    }

    /// [`Self::place_row`]'s cells, and nothing remembered about them: the
    /// scratch row a comparison or a drawing is placed on, which nothing
    /// asks what it was laid from.
    fn put_row(&mut self, line: u16, row: &str, geometry: &Geometry) {
        self.erase_row(line);
        let Some(span) = self.span(line) else {
            return;
        };
        let cols = self.cols;
        let cells = &mut self.cells[span];
        tokenize_row(row, cols, geometry, |placed| match placed {
            Placed::Lead {
                column,
                cluster,
                width,
                sgr,
            } => {
                cells[column] = Cell::Lead {
                    grapheme: cluster.to_string(),
                    width: u8::try_from(width).unwrap_or(u8::MAX),
                    sgr: sgr.clone(),
                };
                for offset in 1..width {
                    cells[column + offset] = Cell::Continuation;
                }
            }
            Placed::Join { column, cluster } => {
                if let Cell::Lead { grapheme, .. } = &mut cells[column] {
                    grapheme.push_str(cluster);
                }
            }
        });
    }

    /// Whether row `line` already holds what placing `row` on it would leave
    /// behind, without writing anything.
    ///
    /// `scratch` is caller-owned so a loop over many settled rows reuses one
    /// row's own backing storage instead of allocating a fresh one per row --
    /// see [`super::frame::Band`]'s field of the same name. Narrower than
    /// "allocates nothing": [`Self::put_row`] still allocates a `String`
    /// per grapheme cluster it writes ([`Cell::Lead`]), every call that reaches
    /// it, whether the row it renders ends up matching or not -- what is reused here is
    /// only the row's own cell-vector storage, not each cell's content. It is
    /// resized here whenever it is not already one row of this grid's own
    /// width, and then filled through [`Self::put_row`]: the **one**
    /// tokenizer, so this cannot agree where a real placement would
    /// disagree, or the reverse.
    ///
    /// **Not placed again when the row's cells were laid from `row`** at this
    /// grid's width and the geometry's ([`Laid`]): those cells are what the
    /// placement leaves, and comparing the text is one pass over its bytes
    /// where placing it reads it cluster by cluster.
    ///
    /// `false` on anything this grid cannot make a real comparison from --
    /// `line` outside it, no columns, or a geometry whose columns disagree
    /// with its own -- rather than a slice equality that would hold
    /// vacuously on a row with nothing in it.
    pub(crate) fn row_matches(
        &self,
        line: u16,
        row: &str,
        geometry: &Geometry,
        scratch: &mut Grid,
    ) -> bool {
        if self.cols == 0 || self.cols != geometry.cols {
            return false;
        }
        let Some(span) = self.span(line) else {
            return false;
        };
        let laid = self.records[usize::from(line - 1)].laid.as_ref();
        if laid.is_some_and(|laid| {
            laid.cols == self.cols && laid.screen == geometry.cols && *laid.text == *row
        }) {
            return true;
        }
        if scratch.rows != 1 || scratch.cols != self.cols {
            scratch.resize(1, self.cols);
        }
        scratch.put_row(1, row, geometry);
        let Some(scratch_span) = scratch.span(1) else {
            return false;
        };
        self.cells[span] == scratch.cells[scratch_span]
    }

    /// Repaints the document's own cells on rows `1..=last` in `palette`, and
    /// says how many of them moved.
    ///
    /// **A candidate, and never the shadow.** The caller builds this out of a
    /// copy of what the terminal is holding and adopts it only if the bytes
    /// land ([`super::frame::Band`]), which is the same discipline every other
    /// target grid is under.
    ///
    /// A row is repainted one of two ways, by what its writer recorded:
    ///
    /// * **Drawn again**, when the writer recorded how it draws the row in
    ///   each mode ([`Redraw`] -- a tool group's rows, which upstream projects
    ///   again in the new theme rather than recolouring,
    ///   `tool_group_projection.zig:558-640@481104fd`). Each cell takes the
    ///   attributes the drawing in `palette`'s mode gives it, whole -- a
    ///   fresh paint of the drawing, which for a tool row is its foreground
    ///   alone -- and only when the drawing is the text the row holds, cell
    ///   for cell ([`Self::redraw_row`]); a row that is not is left as it is.
    /// * **Recoloured**, every other row: a cell's colour is asked of the
    ///   palette within the kind of row it was recorded as.
    ///
    /// Three things make it narrow enough to be safe on rows nobody repaints:
    ///
    /// * **Only a [`Cell::Lead`] this crate drew.** Recoloured, the colour is
    ///   asked of the cell, within the kind of row it was recorded on when
    ///   it was written -- an answered question's answer shares one dark grey
    ///   with a turn's summary and they part ways on a light screen
    ///   ([`DocumentRow`]) -- and [`Palette::document_retint_in`] answers
    ///   `None` for every one it did not paint; drawn again, only a row whose
    ///   writer recorded the drawing, and only while the row still holds its
    ///   text. So an [`Cell::Empty`] cell, a continuation, the user's own
    ///   echo and whatever the terminal was holding before xfx ran are all
    ///   left exactly as they are. Nothing here can *create* a cell, which is
    ///   what stops a repaint reconstructing history.
    /// * **Only the attributes.** Recoloured, [`SgrState::observe`] replaces
    ///   the one slot the sequence names and leaves the rest of them, in the
    ///   order they were opened, so a bold or underlined answer row stays bold
    ///   and underlined; drawn again, the cell's attributes are the drawing's.
    /// * **Nothing else about the cell.** The grapheme, its width and the
    ///   continuation behind it are untouched, so the count this returns is a
    ///   count of attribute changes and the row is the same shape it was.
    ///
    /// A cell whose attribute slot cannot be read is skipped rather than
    /// guessed at: it is one the checker would refuse the whole vector for
    /// (`super::check::CellState::of`), and one this crate did not write.
    pub(crate) fn retint_document(
        &mut self,
        last: u16,
        palette: &Palette,
        geometry: &Geometry,
    ) -> usize {
        let mut moved = 0usize;
        let mut scratch: Option<Grid> = None;
        for line in 1..=last {
            let Some(span) = self.span(line) else {
                continue;
            };
            let mut here = 0usize;
            if let Some(redraw) = self.redraw(line).cloned() {
                let cols = self.cols;
                let scratch = scratch.get_or_insert_with(|| Grid::blank(1, cols));
                here = self.redraw_row(line, redraw.drawn(palette.mode), geometry, scratch);
            } else {
                // What its writer recorded the row as ([`DocumentRow`]), not
                // what its first cell looks like: an answer row and a summary
                // open with the same painted blank.
                let row = self.kind(line);
                for cell in &mut self.cells[span] {
                    let Cell::Lead { sgr, .. } = cell else {
                        continue;
                    };
                    let Ok(colour) = sgr.color() else {
                        continue;
                    };
                    let Some(wanted) = palette.document_retint_in(row, colour) else {
                        continue;
                    };
                    sgr.observe(wanted);
                    here += 1;
                }
            }
            // A cell moved, so the row is no longer what its text lays.
            if here > 0 {
                if let Some(slot) = self.slot(line) {
                    slot.laid = None;
                }
            }
            moved += here;
        }
        moved
    }

    /// Row `line` with each of its cells given the attributes `drawing` puts
    /// on it, when `drawing` is the text the row holds -- every cell the same
    /// grapheme at the same width, every empty cell and continuation where
    /// the drawing has one -- and how many cells that changed. Zero, and the
    /// row as it was, when it is not: a drawing is a claim about what its
    /// writer put on the row, and a row that holds something else is not one
    /// to draw a claim onto.
    ///
    /// `scratch` is one row of this grid's width, reused across rows: the
    /// drawing is placed there by the one tokenizer ([`Self::put_row`]),
    /// so what a cell is given is what a fresh paint of the drawing would
    /// leave in it.
    fn redraw_row(
        &mut self,
        line: u16,
        drawing: &str,
        geometry: &Geometry,
        scratch: &mut Grid,
    ) -> usize {
        let (Some(span), Some(drawn)) = (self.span(line), scratch.span(1)) else {
            return 0;
        };
        if scratch.cols != self.cols {
            return 0;
        }
        scratch.put_row(1, drawing, geometry);
        let same_text = self.cells[span.clone()]
            .iter()
            .zip(&scratch.cells[drawn.clone()])
            .all(|pair| match pair {
                (Cell::Empty, Cell::Empty) | (Cell::Continuation, Cell::Continuation) => true,
                (
                    Cell::Lead {
                        grapheme, width, ..
                    },
                    Cell::Lead {
                        grapheme: drawn_grapheme,
                        width: drawn_width,
                        ..
                    },
                ) => grapheme == drawn_grapheme && width == drawn_width,
                _ => false,
            });
        if !same_text {
            return 0;
        }
        let mut moved = 0usize;
        for (cell, fresh) in self.cells[span].iter_mut().zip(&scratch.cells[drawn]) {
            if let (Cell::Lead { sgr, .. }, Cell::Lead { sgr: fresh, .. }) = (cell, fresh) {
                if sgr.reopen() != fresh.reopen() {
                    sgr.clone_from(fresh);
                    moved += 1;
                }
            }
        }
        moved
    }

    /// The rows of `1..=last` a switch to collapsed tool groups takes out of
    /// the screen (`formatGroupBlock`'s `collapse_tool_calls`,
    /// `tool_group_projection.zig:561-565`, re-projected over the visible
    /// transcript by `markTranscriptStructureDirty`, `app_commands.zig:4120-4133`),
    /// ascending: every call row of a group whose header is on the screen, and
    /// each cancelled call's own line with the blank row its writer put under
    /// it. The header and the group's blank row stay -- what a collapsed group
    /// is (`super::toolgroup`).
    ///
    /// **Read off what the writers recorded, never guessed.** A row is a group's
    /// when its writer said so ([`DocumentRow::ToolGroup`],
    /// [`DocumentRow::ToolStopped`]), and its header is the one of them that
    /// opens with the group's mark. Call rows at the top of the screen whose
    /// header has already left it are a group's too -- the rows above the
    /// first one are the same group's, in scrollback -- and go as well:
    /// upstream's re-render has that group above the screen, header and all
    /// (`fx-settings-collapse-open-40x120-collapse-on`, row 1).
    pub(crate) fn collapsible_rows(&self, last: u16) -> Vec<u16> {
        let last = last.min(self.rows);
        let mut removed = Vec::new();
        let mut in_group = true;
        let mut line = 1;
        while line <= last {
            match self.kind(line) {
                DocumentRow::ToolGroup if self.opens_with(line, GROUP_MARK) => in_group = true,
                DocumentRow::ToolGroup if in_group => removed.push(line),
                DocumentRow::ToolStopped if in_group => {
                    removed.push(line);
                    if line < last && self.is_blank(line + 1) {
                        line += 1;
                        removed.push(line);
                    }
                }
                // The group's own blank row, after which its cancelled calls'
                // lines still belong to it.
                DocumentRow::Plain if self.is_blank(line) => {}
                _ => in_group = false,
            }
            line += 1;
        }
        removed
    }

    /// Takes `removed` -- rows of `1..=last`, ascending -- off the screen:
    /// every row of `1..=last` under one moves up past it, records and all,
    /// and the rows that frees at the bottom of that range are blank. Nothing
    /// below `last` moves, and nothing above the first removed row changes.
    pub(crate) fn remove_rows(&mut self, removed: &[u16], last: u16) {
        let last = last.min(self.rows);
        let width = usize::from(self.cols);
        let end = usize::from(last) * width;
        let mut cells = Vec::with_capacity(end);
        let mut records = Vec::with_capacity(usize::from(last));
        for line in 1..=last {
            if removed.binary_search(&line).is_ok() {
                continue;
            }
            if let Some(span) = self.span(line) {
                cells.extend_from_slice(&self.cells[span]);
                records.push(self.records[usize::from(line - 1)].clone());
            }
        }
        cells.resize(end, Cell::Empty);
        records.resize(usize::from(last), Record::default());
        self.cells.splice(..end, cells);
        self.records.splice(..usize::from(last), records);
    }

    /// The rows whose cells are not what placing the text they were laid from
    /// ([`Laid`]) leaves, at the widths it was laid at: none while every
    /// change to a row's cells forgets its text. For the tests that hold a
    /// whole session's frames to it (`super::event_loop`'s).
    #[cfg(test)]
    pub(crate) fn stale_laid_rows(&self, geometry: &Geometry) -> Vec<u16> {
        (1..=self.rows)
            .filter(|&line| {
                let Some(laid) = self.records[usize::from(line - 1)].laid.as_ref() else {
                    return false;
                };
                let mut screen = *geometry;
                screen.cols = laid.screen;
                let mut fresh = Grid::blank(1, laid.cols);
                fresh.put_row(1, &laid.text, &screen);
                laid.cols != self.cols
                    || self
                        .span(line)
                        .is_some_and(|span| self.cells[span] != fresh.cells[..])
            })
            .collect()
    }

    /// The longest text a row of this grid was laid from and still holds, in
    /// bytes: what says a case's rows really went through the remembered
    /// answer ([`Grid::row_matches`]).
    #[cfg(test)]
    pub(crate) fn longest_laid(&self) -> usize {
        self.records
            .iter()
            .filter_map(|record| record.laid.as_ref())
            .map(|laid| laid.text.len())
            .max()
            .unwrap_or(0)
    }

    /// Whether row `line`'s first cell is `grapheme`.
    fn opens_with(&self, line: u16, grapheme: &str) -> bool {
        matches!(self.cell(line, 1), Some(Cell::Lead { grapheme: first, .. }) if first == grapheme)
    }

    /// Whether row `line` holds nothing a terminal would draw.
    fn is_blank(&self, line: u16) -> bool {
        self.span(line).is_some_and(|span| {
            self.cells[span].iter().all(|cell| match cell {
                Cell::Empty => true,
                Cell::Lead { grapheme, .. } => grapheme == " ",
                Cell::Continuation => false,
            })
        })
    }

    /// The band's own rows, and the erase in front of them.
    ///
    /// Exactly what one Phase-1 frame does to a screen: `CUP` to the band's top
    /// row, erase from there to the bottom, and place the rows. **Nothing above
    /// the band's top row is touched** -- that is the terminal's own document,
    /// and the rows a *shrinking* band gave back are the caller's to erase
    /// before this runs, because only the caller knows where the band used to
    /// be.
    pub(crate) fn paint_band(&mut self, rows: &[String], geometry: &Geometry) {
        for line in geometry.band_top()..=self.rows {
            self.erase_row(line);
        }
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = geometry.band_top().saturating_add(offset);
            if line > geometry.hint {
                // More rows than the band owns, dropped rather than written
                // below the screen's last row -- which a terminal would answer
                // by scrolling the document.
                break;
            }
            self.place_row(line, row, geometry);
        }
    }

    /// Moves every row up by `rows`, and blanks what that frees at the bottom.
    ///
    /// What a linefeed on the bottom row does, and therefore what a document
    /// append does: the band's own rows go up with everything else, which is
    /// why a shadow that did not scroll would leave the band a row above where
    /// it belongs for the rest of the session.
    pub(crate) fn scroll_up(&mut self, rows: usize) {
        let height = usize::from(self.rows);
        let moved = rows.min(height);
        if moved == 0 {
            return;
        }
        let width = usize::from(self.cols);
        self.cells.drain(..moved * width);
        self.cells.resize(height * width, Cell::Empty);
        self.records.drain(..moved.min(self.records.len()));
        self.records.resize(height, Record::default());
    }

    /// Writes the bytes that turn this grid into `target`, and says how many
    /// rows they touch.
    ///
    /// Zero rows means zero bytes, and that is the whole of the no-op skip: a
    /// diff that emitted a `CUP` for an unchanged band would make the skip
    /// above it unreachable.
    ///
    /// Three decisions are load-bearing:
    ///
    /// * **The run starts at the lead of whatever it lands in.** A `CUP` onto a
    ///   [`Cell::Continuation`] would put the caret inside a character.
    /// * **It stops at the end of the target's own text**, and `EL` says the
    ///   rest. That is what erases a tail that got shorter, and it is the only
    ///   thing that erases the second column of a wide cluster an `x` was
    ///   written over.
    /// * **The attribute state is tracked across the whole frame and closed at
    ///   the end of it.** So a frame begins with a terminal in the default
    ///   state -- the frame before it left one -- and a run that needs no
    ///   attribute writes no bytes for one.
    ///
    /// `geometry` bounds the comparison rather than describing it: a grid a
    /// size the geometry has outgrown is one nothing can be claimed about past
    /// the overlap, and the caller repaints it whole
    /// ([`super::frame::Band::invalidate`]).
    pub(crate) fn diff(&self, target: &Grid, geometry: &Geometry, out: &mut Vec<u8>) -> usize {
        self.diff_from(1, target, geometry, out)
    }

    /// The same diff, over the rows from `first_row` **down**.
    ///
    /// The rows above `first_row` are not compared and nothing is written for
    /// them: this says "the caller already knows those rows are equal", not
    /// "leave them alone". A caller that cannot prove the prefix equal calls
    /// [`Self::diff`], because a difference above `first_row` here is silently
    /// dropped -- the screen keeps what it is holding and no later frame has
    /// anything to say about it.
    ///
    /// The one caller that can prove it is [`super::frame::Band::commit`], out
    /// of how its target is built: [`super::frame::Band::plan`] clones the
    /// shadow and then touches only the rows the band gave back and the band's
    /// own ([`Grid::paint_band`]), so every row above the top of what the frame
    /// is about to write is the shadow's own cell, cloned, and comparing a cell
    /// with itself can neither find a difference nor emit a byte.
    ///
    /// What that buys is the whole of why it exists. A session whose answer is
    /// settled and whose composer takes one row asks for a frame twice a second
    /// while a turn runs, and the document above the band is most of the screen:
    /// comparing every cell of it, every frame, to conclude what the clone
    /// already proves is work that grows with the terminal rather than with what
    /// changed.
    ///
    /// `first_row` below row one is read as row one, so a caller's saturating
    /// arithmetic cannot turn a narrowing into a skipped row.
    pub(crate) fn diff_from(
        &self,
        first_row: u16,
        target: &Grid,
        geometry: &Geometry,
        out: &mut Vec<u8>,
    ) -> usize {
        let rows = self.rows.min(target.rows).min(geometry.rows);
        let cols = usize::from(self.cols.min(target.cols).min(geometry.cols));
        // The terminal's right margin, which is the screen's rather than the
        // overlap's: what [`put_unit`] guards a unit at.
        let margin = usize::from(geometry.cols);
        let mut touched = 0usize;
        // What the terminal has switched on, as this frame left it. It starts
        // closed whatever row the run begins on, and that is what the prefix
        // being *equal* rather than merely unwritten buys: a row the full diff
        // would have found nothing to say about opens nothing either, so the
        // narrowed run meets the first row it does compare in the same state.
        let mut open = Pen::default();
        for line in first_row.max(1)..=rows {
            let (Some(before), Some(after)) = (self.span(line), target.span(line)) else {
                continue;
            };
            let old = &self.cells[before.start..before.start + cols];
            let new = &target.cells[after.start..after.start + cols];
            #[cfg(test)]
            note_row_compared();
            let Some(first) = (0..cols).find(|&at| old[at] != new[at]) else {
                continue;
            };
            let last = (0..cols)
                .rev()
                .find(|&at| old[at] != new[at])
                .unwrap_or(first);
            let old_end = filled(old);
            let new_end = filled(new);
            // Back onto the lead of whatever the first change lands in --
            // **in either grid**, and both halves are needed. A continuation in
            // the *target* means the replacement text would begin in the middle
            // of a cluster; a continuation in the grid the terminal is holding
            // means the `CUP` would put the caret on the right-hand column of a
            // character that is on the screen right now, and writing there
            // leaves its left half behind as an orphan nothing will erase.
            let mut start = first;
            while start > 0
                && (matches!(new[start], Cell::Continuation)
                    || matches!(old[start], Cell::Continuation))
            {
                start -= 1;
            }
            // Forward over the columns a wide cluster at the end of the run
            // owns, and never past the target's own text: what is beyond it is
            // `EL`'s to say.
            let mut stop = (last + 1).min(new_end);
            while stop < new_end && matches!(new[stop], Cell::Continuation) {
                stop += 1;
            }
            // Never before the start. The clamp above is to the *target* row's
            // own text and the start is the first changed column, so on a
            // screen holding something out beyond where that text stops the two
            // cross -- and an inverted slice is a panic on the thread that owes
            // the terminal a restore. Nothing `place_row` builds can do it (it
            // leaves one contiguous run), so this costs a comparison to make a
            // whole class of grid unable to crash the painter; the `EL` below
            // is what says the row, and it is already owed whenever this fires.
            let stop = stop.max(start);
            // An erase follows the run when the row got shorter, which is
            // something written after its last cell ([`put_unit`]'s `more`).
            let erases = old_end > new_end;
            cup(out, line, column_of(start));
            for (index, cell) in new.iter().enumerate().take(stop).skip(start) {
                match cell {
                    // Carried by the lead in front of it.
                    Cell::Continuation => {}
                    Cell::Empty => {
                        reopen(out, &mut open, "", None);
                        out.push(b' ');
                    }
                    Cell::Lead {
                        grapheme,
                        width,
                        sgr,
                    } => {
                        reopen(out, &mut open, &sgr.attributes(), sgr.link());
                        let width = usize::from(*width);
                        let more = index + width < stop || erases;
                        let pen = &open.attributes;
                        put_unit(out, grapheme, index, width, margin, more, |out| {
                            write_cell(out, grapheme, width, pen);
                        });
                    }
                }
            }
            if erases {
                // The row got shorter. The caret is at `new_end` -- every cell
                // written above advanced it by exactly the columns it covered --
                // so this erases precisely the tail that is no longer there,
                // including the orphaned second column of a wide cluster.
                //
                // With the attributes closed first, because an erase paints the
                // *background* of what it clears on a terminal that has one
                // switched on -- and the file picker's marked row is one
                // (`super::theme::Palette::selected`), so a row that ended on
                // it would otherwise have the bar painted to the right edge.
                reopen(out, &mut open, "", None);
                out.extend_from_slice(ERASE_LINE.as_bytes());
            }
            // A link ends with the row it is on, before the `CUP` that starts
            // the next run: upstream closes one there for the reason it gives
            // -- tmux ends an active hyperlink at an automatic wrap
            // (`engine.zig:2400-2406`) -- and a link carried across a `CUP`
            // would take in whatever the next run paints first. The colours
            // carry on as they did: a pen survives a `CUP` on every terminal.
            close_link(out, &mut open);
            touched += 1;
        }
        // Whatever the last run opened, closed: the next frame assumes a
        // terminal in the default state, and this is what makes that true.
        reopen(out, &mut open, "", None);
        touched
    }
}

#[cfg(test)]
thread_local! {
    /// How many rows the diffs on **this thread** have compared cells on, since
    /// [`rows_compared`] last reset it.
    ///
    /// Per-thread rather than a process-wide counter because the test binary
    /// runs its cases in parallel: a shared count would be every other case's
    /// diffs as well, and the measurement would pass or fail on the scheduler.
    static ROWS_COMPARED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One more row whose cells a diff really read.
///
/// Counted where the per-row work is -- the column scan below it -- rather than
/// at the head of the loop, because that scan is what a narrowed diff exists to
/// not do: it is `cols` comparisons per row, and everything above it is the loop
/// variable.
#[cfg(test)]
fn note_row_compared() {
    ROWS_COMPARED.with(|counted| counted.set(counted.get() + 1));
}

/// What `run` answered, and how many rows the diffs inside it compared cells on.
///
/// A test utility, and deliberately not a method on [`Grid`]: no released
/// binary counts anything, and nothing in this crate reads the count.
#[cfg(test)]
pub(crate) fn rows_compared<T>(run: impl FnOnce() -> T) -> (T, usize) {
    ROWS_COMPARED.with(|counted| counted.set(0));
    let answered = run();
    (answered, ROWS_COMPARED.with(std::cell::Cell::get))
}

#[cfg(test)]
thread_local! {
    /// How many bytes of row text [`tokenize_row`] was handed on **this
    /// thread**, since [`bytes_tokenized`] last reset it: per-thread for the
    /// reason [`ROWS_COMPARED`] is.
    static BYTES_TOKENIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A row of `bytes` more was read cluster by cluster.
#[cfg(test)]
fn note_tokenized(bytes: usize) {
    BYTES_TOKENIZED.with(|counted| counted.set(counted.get() + bytes));
}

/// What `run` answered, and how many bytes of row text the tokenizer read
/// inside it: the grid's placements and comparisons, and the output check's
/// replay of a declared row, all read a row through [`tokenize_row`].
#[cfg(test)]
pub(crate) fn bytes_tokenized<T>(run: impl FnOnce() -> T) -> (T, usize) {
    BYTES_TOKENIZED.with(|counted| counted.set(0));
    let answered = run();
    (answered, BYTES_TOKENIZED.with(std::cell::Cell::get))
}

/// How many columns of `row` have been written on.
fn filled(row: &[Cell]) -> usize {
    row.iter()
        .rposition(|cell| !matches!(cell, Cell::Empty))
        .map_or(0, |at| at + 1)
}

/// A zero-based column as the terminal's one-based column, clamped.
///
/// The **one** narrowing in this module, and it is a coordinate rather than a
/// count: everything above is `usize`, and a screen wider than `u16::MAX`
/// columns is not one a `CUP` could address anyway.
fn column_of(index: usize) -> u16 {
    u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX)
}

/// What a diff has left switched on, so far into its frame.
///
/// The attributes and the link apart, because they are closed apart: a reset
/// turns every attribute off and leaves a hyperlink open.
#[derive(Default)]
struct Pen {
    attributes: String,
    link: Option<std::sync::Arc<str>>,
}

/// Puts the terminal into `attributes` under `link`, if it is not there
/// already -- the attributes first and the link second, upstream's order for
/// one cell (`engine.zig:2421-2428`).
///
/// A reset in front of any attributes that are not a pure addition, because
/// [`SgrState`] replays a *state* rather than a delta: replaying it onto a
/// terminal that still has the previous run's attributes on would leave
/// whatever the new run does not mention switched on. A link needs no close in
/// front of another one -- an open replaces the link that was open, which is
/// what OSC 8 says and what upstream writes -- and a cell with none closes the
/// one that was.
fn reopen(out: &mut Vec<u8>, open: &mut Pen, attributes: &str, link: Option<&std::sync::Arc<str>>) {
    if open.attributes != attributes {
        if !open.attributes.is_empty() {
            out.extend_from_slice(RESET.as_bytes());
        }
        out.extend_from_slice(attributes.as_bytes());
        open.attributes.clear();
        open.attributes.push_str(attributes);
    }
    if open.link.as_ref() != link {
        match link {
            Some(link) => out.extend_from_slice(link.as_bytes()),
            None => out.extend_from_slice(super::hyperlink::CLOSE.as_bytes()),
        }
        open.link = link.cloned();
    }
}

/// Closes the link the pen has open, and nothing else.
fn close_link(out: &mut Vec<u8>, open: &mut Pen) {
    if open.link.take().is_some() {
        out.extend_from_slice(super::hyperlink::CLOSE.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tui::layout::Geometry;

    /// A 256-colour foreground, and the reset that ends it. Spelled out rather
    /// than imported from [`crate::tui::theme`]: a test that read the palette
    /// it is checking would pass for whatever that module happened to declare.
    const COLOUR: &str = "\u{1b}[38;5;250m";
    const RESET: &str = "\u{1b}[0m";

    /// A three-person ZWJ family: five scalars, one grapheme, two cells.
    const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";

    fn geometry() -> Geometry {
        crate::tui::layout::solve(24, 80, 1).expect("a band")
    }

    /// A blank grid with `text` on row `line`, and nothing else.
    fn painted(line: u16, text: &str) -> Grid {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(line, text, &geometry);
        grid
    }

    /// A blank grid with each `(line, text)` on it, and nothing else.
    fn painted_rows(rows: &[(u16, &str)]) -> Grid {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        for (line, text) in rows {
            grid.place_row(*line, text, &geometry);
        }
        grid
    }

    /// The bytes that turn `before` into `after`.
    fn diffed(before: &Grid, after: &Grid) -> String {
        let mut out = Vec::new();
        before.diff(after, &geometry(), &mut out);
        String::from_utf8(out).expect("a diff is text and escapes")
    }

    #[test]
    fn one_changed_grapheme_emits_only_its_span() {
        // The whole of item 11 in one assertion: a keystroke that changed one
        // cell must cost one `CUP` and one character, not a repaint of the
        // band on a link that may be a serial line.
        let before = painted(10, "abc");
        let after = painted(10, "aXc");
        assert_eq!(diffed(&before, &after), "\u{1b}[10;2HX");
    }

    #[test]
    fn shrinking_a_row_erases_the_old_tail() {
        // A row that got shorter leaves the old tail on the screen unless
        // something erases it, and the tail may be wide: a stale continuation
        // cell is half a character nothing will ever overwrite.
        let before = painted(10, "hi\u{d55c}\u{d55c}");
        let after = painted(10, "hi");
        assert_eq!(diffed(&before, &after), "\u{1b}[10;3H\u{1b}[K");
    }

    #[test]
    fn wide_to_narrow_overwrite_clears_the_second_cell() {
        // The narrow case of the same rule, and the one a diff gets wrong by
        // default: writing `x` over a two-cell family leaves the family's
        // second cell holding the second half of an emoji.
        let before = painted(10, FAMILY);
        let after = painted(10, "x");
        assert_eq!(diffed(&before, &after), "\u{1b}[10;1Hx\u{1b}[K");
    }

    #[test]
    fn a_zwj_family_is_one_cell_two_columns_wide() {
        // The column arithmetic every later cell on the row depends on.
        // `unicode-width` gives a ZWJ sequence two columns, so the character
        // behind one is at column three; a grid that measured the scalars would
        // put it at column seven, aim every `CUP` on that row at the wrong
        // place, and disagree with `super::wrap::width` about where the caret
        // is -- which is the same disagreement `scripts/smoke-tui.sh`'s oracle
        // is falsified for on the other side of the wire.
        let before = painted(10, &format!("{FAMILY}x"));
        let after = painted(10, &format!("{FAMILY}y"));
        assert_eq!(diffed(&before, &after), "\u{1b}[10;3Hy");
    }

    #[test]
    fn a_run_that_lands_inside_a_wide_cluster_starts_at_its_lead() {
        // A `CUP` onto a continuation cell puts the caret inside a character,
        // and the replacement text then paints from the wrong column.
        //
        // **Unreachable through `place_row`**, and built here by hand for that
        // reason: a continuation is only ever written directly behind its lead,
        // so a first difference that fell on one would need the lead in front
        // of it to be equal in both grids -- and an equal wide lead writes an
        // equal continuation behind it. The guard is for the day something else
        // fills a grid (Phase 2 item 12 reflows one), and it is pinned here so
        // that day cannot remove it silently.
        let geometry = geometry();
        let mut before = Grid::blank(geometry.rows, geometry.cols);
        let mut after = Grid::blank(geometry.rows, geometry.cols);
        before.place_row(10, "\u{d55c}\u{d55c}", &geometry);
        after.place_row(10, "\u{d55c}\u{d55c}", &geometry);
        // The lead is left equal in both and only the cell behind it is
        // disturbed, which is the shape `place_row` cannot produce.
        let span = after.span(10).expect("a row");
        after.cells[span.start + 1] = Cell::Empty;
        let mut out = Vec::new();
        before.diff(&after, &geometry, &mut out);
        let text = String::from_utf8(out).expect("utf-8");
        assert!(
            text.starts_with("\u{1b}[10;1H"),
            "the run began inside a wide cluster: {text:?}"
        );
    }

    #[test]
    fn a_change_beyond_the_targets_own_text_is_erased_rather_than_sliced() {
        // The run's start is the first changed column and its end is clamped to
        // the *target* row's own text. On a screen holding something out beyond
        // where the target's text stops, the two cross -- `start` past `stop` --
        // and `&new[start..stop]` is an inverted slice: a panic on the UI
        // thread, which is the one thread that owes the terminal a restore.
        //
        // `place_row` cannot build the screen that does it: it fills from
        // column one and leaves one contiguous run, so anything it writes is
        // reachable before the target's text ends. A grid filled by anything
        // else can, which is why the guard is here and why this builds one by
        // hand -- Phase 2 item 12 reflows a grid, and this is the shape it must
        // not be able to crash on.
        let geometry = geometry();
        let mut before = Grid::blank(geometry.rows, geometry.cols);
        let mut after = Grid::blank(geometry.rows, geometry.cols);
        before.place_row(10, "ab", &geometry);
        after.place_row(10, "ab", &geometry);
        // A cell on the screen out past the end of everything either row's
        // contiguous text covers, and nothing between.
        let span = before.span(10).expect("a row");
        before.cells[span.start + 5] = Cell::Lead {
            grapheme: "z".to_string(),
            width: 1,
            sgr: SgrState::default(),
        };

        let mut out = Vec::new();
        before.diff(&after, &geometry, &mut out);
        let text = String::from_utf8(out).expect("utf-8");
        assert_eq!(
            text,
            format!("\u{1b}[10;6H{ERASE_LINE}"),
            "the cell beyond the target's text was not erased"
        );
    }

    #[test]
    fn sgr_is_reopened_at_the_first_changed_cell() {
        // A diff starts writing in the middle of a row, so whatever colour the
        // untouched prefix opened is not on the terminal any more: the
        // attribute has to be re-opened at the first cell that is written, or
        // the suffix is painted in the wrong colour.
        let before = painted(10, "abcdef");
        let after = painted(10, &format!("abc{COLOUR}def{RESET}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;4H{COLOUR}def{RESET}")
        );
    }

    #[test]
    fn a_grid_that_did_not_change_costs_nothing() {
        // The no-op skip, at the level it is decided: a diff that emitted a
        // `CUP` for an unchanged band would make the skip above it unreachable.
        let before = painted(10, "abc");
        let after = painted(10, "abc");
        let mut out = Vec::new();
        assert_eq!(before.diff(&after, &geometry(), &mut out), 0);
        assert!(out.is_empty(), "an unchanged grid wrote {out:?}");
    }

    #[test]
    fn a_scroll_moves_every_row_up_and_blanks_the_bottom() {
        // What a document append does to the screen, and therefore what it has
        // to do to the shadow: a linefeed on the bottom row moves the band's
        // own rows up with everything else, and the row it frees is blank.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(10, "up", &geometry);
        grid.place_row(geometry.rows, "bottom", &geometry);
        grid.scroll_up(1);

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(9, "up", &geometry);
        expected.place_row(geometry.rows - 1, "bottom", &geometry);
        assert_eq!(
            diffed(&grid, &expected),
            "",
            "the scroll did not move every row up by one and blank the bottom"
        );
    }

    #[test]
    fn a_combining_mark_belongs_to_the_grapheme_it_marks() {
        // The grid is cells, and a mark is not one: an `e` and its acute are
        // one cell, so a grid that gave the mark a cell of its own would
        // disagree with `wrap::width` about where every later column is -- and
        // the `x` behind it would be rewritten for nothing.
        assert_eq!(
            diffed(&painted(10, "ex"), &painted(10, "e\u{301}x")),
            "\u{1b}[10;1He\u{301}"
        );
    }

    #[test]
    fn a_band_is_painted_from_its_top_row_and_nothing_above_it_is_touched() {
        // `Band::render`'s erase starts at the band's top row, so the target
        // grid's must too: a `paint_band` that blanked the screen would make
        // the diff rewrite the terminal's own document.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(1, "the document", &geometry);
        grid.place_row(geometry.band_top(), "stale", &geometry);
        grid.paint_band(&["--".to_string()], &geometry);

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(1, "the document", &geometry);
        expected.place_row(geometry.band_top(), "--", &geometry);
        assert_eq!(diffed(&grid, &expected), "");
    }

    // -- Grid::diff_from (the rows a caller has already proved equal) --

    #[test]
    fn a_narrow_diff_reads_no_cell_on_a_row_above_the_one_it_starts_at() {
        // The whole of what the narrowing is for, measured where the cost is:
        // the per-row column scan. A `diff_from` that still walked every row --
        // or one that walked them and filtered the *output* instead -- would
        // leave a settled session paying for the whole screen twice a second,
        // which is the work this exists to not do.
        let geometry = geometry();
        let before = painted(10, "abc");
        let after = painted(10, "aXc");
        let mut out = Vec::new();
        let (touched, rows) = rows_compared(|| before.diff_from(10, &after, &geometry, &mut out));
        assert_eq!(touched, 1, "the changed row was not written");
        // Rows ten to twenty-four: fifteen, and never the nine above them.
        assert_eq!(
            rows, 15,
            "the diff read cells on rows it was told begin equal"
        );
    }

    #[test]
    fn a_narrow_diff_writes_nothing_for_a_row_above_the_one_it_starts_at() {
        // The other half of the same claim, in bytes rather than in a count: a
        // row above the start is not compared, so a difference on one is not
        // written. That is the contract the caller carries -- and the reason
        // `diff` itself still starts at row one.
        let geometry = geometry();
        let before = painted_rows(&[(3, "a settled answer"), (10, "abc")]);
        let after = painted_rows(&[(3, "rewritten"), (10, "aXc")]);
        let mut out = Vec::new();
        before.diff_from(10, &after, &geometry, &mut out);
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            "\u{1b}[10;2HX",
            "the diff wrote a row it was told to leave to the caller"
        );
    }

    #[test]
    fn a_narrow_diff_writes_the_row_it_starts_at() {
        // The boundary, off by one in the direction that loses a row silently:
        // a start of `first_row + 1` drops the band's own top row -- the
        // divider, or the activity line a turn puts above it -- and nothing
        // downstream would ever write it again.
        let geometry = geometry();
        let before = painted(10, "abc");
        let after = painted(10, "aXc");
        let mut out = Vec::new();
        assert_eq!(before.diff_from(10, &after, &geometry, &mut out), 1);
        assert_eq!(String::from_utf8(out).expect("utf-8"), "\u{1b}[10;2HX");
    }

    #[test]
    fn a_narrow_diff_writes_what_a_full_one_does_when_the_rows_above_it_are_equal() {
        // The equivalence the caller's invariant buys, on the rows most likely
        // to break it: a wide cluster and a colour on the row above the start,
        // equal in both grids, and a colour that runs into the row below. The
        // attribute state is threaded across the *whole* frame, so a prefix
        // that emitted nothing is also a prefix that opened nothing -- and the
        // narrowed run must begin in the same state the full one reaches it in.
        let geometry = geometry();
        let above = format!("{COLOUR}{FAMILY} settled{RESET}");
        let before = painted_rows(&[
            (9, &above),
            (10, &format!("{COLOUR}{FAMILY}b{RESET}")),
            (11, "tail"),
        ]);
        let after = painted_rows(&[
            (9, &above),
            (10, &format!("{COLOUR}{FAMILY}c{RESET}")),
            (11, "tai"),
        ]);

        let mut full = Vec::new();
        let full_touched = before.diff(&after, &geometry, &mut full);
        let mut narrow = Vec::new();
        let narrow_touched = before.diff_from(10, &after, &geometry, &mut narrow);

        // Hand-derived: the family is two columns, so the letter behind it is
        // at column three and is repainted under a re-opened colour; row eleven
        // got one column shorter, so its run is empty and the pen is closed
        // before the erase that says the tail.
        assert_eq!(
            String::from_utf8(narrow.clone()).expect("utf-8"),
            format!("\u{1b}[10;3H{COLOUR}c\u{1b}[11;4H{RESET}\u{1b}[K")
        );
        assert_eq!(narrow, full, "the narrowed run wrote different bytes");
        assert_eq!(
            narrow_touched, full_touched,
            "the narrowed run counted different rows"
        );
    }

    // -- Grid::retint_document (P3-THEME, the document's visible cells) --

    /// The two palettes a retint case moves between.
    fn dark() -> Palette {
        Palette {
            mode: crate::tui::theme::Mode::Dark,
        }
    }

    fn light() -> Palette {
        Palette {
            mode: crate::tui::theme::Mode::Light,
        }
    }

    /// The document's two owned greys, dark and light, spelled out.
    const BODY_DARK: &str = "\u{1b}[38;5;255m";
    const BODY_LIGHT: &str = "\u{1b}[38;5;235m";
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";
    const SUMMARY_DARK: &str = "\u{1b}[38;5;245m";
    const SUMMARY_LIGHT: &str = "\u{1b}[38;5;247m";
    const ANSWER_LIGHT: &str = "\u{1b}[38;5;241m";
    const LABEL_DARK: &str = "\u{1b}[38;5;252m";
    const LABEL_LIGHT: &str = "\u{1b}[38;5;238m";

    #[test]
    fn a_retinted_row_is_the_row_a_fresh_paint_in_the_new_palette_would_leave() {
        // Both roles, and the whole claim in one comparison: what the cells
        // hold afterwards is what placing the same text in the other palette
        // would have put there -- so the text, its columns and everything but
        // the foreground are untouched.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{BODY_DARK}an answer{RESET}"), &geometry);
        grid.place_row(4, &format!("{NOTICE_DARK}[tool] ran{RESET}"), &geometry);
        assert_eq!(
            grid.retint_document(10, &light(), &geometry),
            "an answer[tool] ran".len()
        );

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("{BODY_LIGHT}an answer{RESET}"), &geometry);
        expected.place_row(4, &format!("{NOTICE_LIGHT}[tool] ran{RESET}"), &geometry);
        assert_eq!(
            diffed(&grid, &expected),
            "",
            "the retinted rows are not the rows the other palette would paint"
        );
    }

    #[test]
    fn a_row_is_retinted_as_what_its_writer_recorded_it_as() {
        // An answered question's answer and a turn's summary are the same dark
        // grey, 245, and part on a light screen -- 241 and 247 -- and both open
        // with a painted blank, so only the record the writer left with the
        // row can tell them apart.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{SUMMARY_DARK}     SQLite{RESET}"), &geometry);
        grid.set_kind(3, DocumentRow::Answer);
        grid.place_row(4, &format!("{SUMMARY_DARK}  5s (3 4){RESET}"), &geometry);
        // A question's `▪ Cancelled`: the mark in the outcome grey is not a
        // tool group's stopped call, whatever its first cell looks like.
        grid.place_row(
            5,
            &format!("{LABEL_DARK}\u{25aa}{RESET} Cancelled {SUMMARY_DARK}x{RESET}"),
            &geometry,
        );
        assert_eq!(grid.kind(3), DocumentRow::Answer);
        assert_eq!(grid.kind(5), DocumentRow::Plain);
        grid.retint_document(10, &light(), &geometry);

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("{ANSWER_LIGHT}     SQLite{RESET}"), &geometry);
        expected.place_row(4, &format!("{SUMMARY_LIGHT}  5s (3 4){RESET}"), &geometry);
        expected.place_row(
            5,
            &format!("{LABEL_LIGHT}\u{25aa}{RESET} Cancelled {SUMMARY_LIGHT}x{RESET}"),
            &geometry,
        );
        assert_eq!(diffed(&grid, &expected), "");
    }

    #[test]
    fn a_code_blocks_comment_is_retinted_as_a_comment_and_a_summary_as_a_summary() {
        // A code block's comment and a turn's summary are the same dark grey,
        // 245, and part on a light screen -- 243 and 247 -- because fx v0.0.16
        // draws the block again in the light palette rather than mapping its
        // bytes (`transcript_blocks.zig:2023-2029`). The writer's record of
        // the row is what tells them apart, both ways.
        const COMMENT_DARK: &str = "\u{1b}[38;5;245m";
        const COMMENT_LIGHT: &str = "\u{1b}[38;5;243m";
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("  {COMMENT_DARK}# build it{RESET}"), &geometry);
        grid.set_kind(3, DocumentRow::Code);
        grid.place_row(
            4,
            &format!("  {LABEL_DARK}zig{RESET} build {NOTICE_DARK}--summary{RESET}"),
            &geometry,
        );
        grid.set_kind(4, DocumentRow::Code);
        grid.place_row(5, &format!("{SUMMARY_DARK}  5s (3 4){RESET}"), &geometry);
        grid.retint_document(10, &light(), &geometry);

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("  {COMMENT_LIGHT}# build it{RESET}"), &geometry);
        expected.place_row(
            4,
            &format!("  {LABEL_LIGHT}zig{RESET} build {NOTICE_LIGHT}--summary{RESET}"),
            &geometry,
        );
        expected.place_row(5, &format!("{SUMMARY_LIGHT}  5s (3 4){RESET}"), &geometry);
        assert_eq!(diffed(&grid, &expected), "");

        grid.retint_document(10, &dark(), &geometry);
        let mut back = Grid::blank(geometry.rows, geometry.cols);
        back.place_row(3, &format!("  {COMMENT_DARK}# build it{RESET}"), &geometry);
        back.place_row(
            4,
            &format!("  {LABEL_DARK}zig{RESET} build {NOTICE_DARK}--summary{RESET}"),
            &geometry,
        );
        back.place_row(5, &format!("{SUMMARY_DARK}  5s (3 4){RESET}"), &geometry);
        assert_eq!(diffed(&grid, &back), "");
    }

    /// A grid holding `rows` from row 1 down, each with its writer's record.
    fn document(rows: &[(&str, DocumentRow)]) -> Grid {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        for (index, (text, kind)) in rows.iter().enumerate() {
            let line = u16::try_from(index + 1).expect("a row");
            grid.place_row(line, text, &geometry);
            grid.set_kind(line, *kind);
        }
        grid
    }

    /// Row `line`'s text, without the blank cells after it.
    fn text_of(grid: &Grid, line: u16) -> String {
        (1..=grid.cols())
            .filter_map(|column| match grid.cell(line, column) {
                Some(Cell::Lead { grapheme, .. }) => Some(grapheme.as_str()),
                Some(Cell::Empty) => Some(" "),
                _ => None,
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn a_collapse_takes_each_groups_calls_and_cancelled_lines_and_keeps_its_header() {
        use DocumentRow::{Plain, ToolGroup, ToolStopped};
        let mut grid = document(&[
            // A group whose header is in scrollback: its calls go too.
            ("\u{251c} Read old.txt", ToolGroup),
            ("\u{2514} Read older.txt", ToolGroup),
            ("", Plain),
            ("\u{2503} look", Plain),
            ("", Plain),
            ("\u{25cf} 2 tool calls \u{b7} 2 read", ToolGroup),
            ("\u{251c} Read a.txt", ToolGroup),
            ("\u{2514} Read b.txt", ToolGroup),
            ("", Plain),
            ("  Read both.", Plain),
            ("", Plain),
            ("\u{25cf} 1 tool call \u{b7} 1 command", ToolGroup),
            ("\u{2514} Cancelled sleep 4", ToolGroup),
            ("", Plain),
            ("\u{25aa} Cancelled sleep 4", ToolStopped),
            ("", Plain),
            ("  after", Plain),
        ]);
        let removed = grid.collapsible_rows(17);
        assert_eq!(removed, [1, 2, 7, 8, 13, 15, 16]);
        // And nothing at or past a row it was not given.
        assert_eq!(grid.collapsible_rows(7), [1, 2, 7]);

        grid.remove_rows(&removed, 17);
        let texts: Vec<String> = (1..=17).map(|line| text_of(&grid, line)).collect();
        assert_eq!(
            texts,
            [
                "",
                "\u{2503} look",
                "",
                "\u{25cf} 2 tool calls \u{b7} 2 read",
                "",
                "  Read both.",
                "",
                "\u{25cf} 1 tool call \u{b7} 1 command",
                "",
                "  after",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
            ]
        );
        // The records went with their rows, and the freed rows have none.
        assert_eq!(grid.kind(4), ToolGroup);
        assert_eq!(grid.kind(8), ToolGroup);
        assert_eq!(grid.kind(11), Plain);
        assert_eq!(
            grid.collapsible_rows(17),
            [] as [u16; 0],
            "a second pass took more"
        );
    }

    #[test]
    fn rows_past_the_range_a_removal_was_given_do_not_move() {
        use DocumentRow::{Plain, ToolGroup};
        let mut grid = document(&[
            ("\u{25cf} 1 tool call \u{b7} 1 read", ToolGroup),
            ("\u{2514} Read a.txt", ToolGroup),
            ("", Plain),
            ("band", Plain),
        ]);
        grid.remove_rows(&[2], 3);
        assert_eq!(text_of(&grid, 2), "");
        assert_eq!(text_of(&grid, 3), "");
        assert_eq!(text_of(&grid, 4), "band");
    }

    #[test]
    fn a_rows_record_moves_with_it_and_goes_with_its_cells() {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, "answer", &geometry);
        grid.set_kind(3, DocumentRow::Answer);
        grid.scroll_up(1);
        assert_eq!(
            grid.kind(2),
            DocumentRow::Answer,
            "the record stayed behind"
        );
        assert_eq!(grid.kind(3), DocumentRow::Plain);
        assert_eq!(grid.kind(geometry.rows), DocumentRow::Plain);
        // A row written over or erased is a new row, with no record until its
        // writer gives it one.
        grid.place_row(2, "something else", &geometry);
        assert_eq!(grid.kind(2), DocumentRow::Plain);
        grid.set_kind(2, DocumentRow::ToolGroup);
        grid.erase_row(2);
        assert_eq!(grid.kind(2), DocumentRow::Plain);
        grid.set_kind(2, DocumentRow::Answer);
        grid.resize(geometry.rows, geometry.cols);
        assert_eq!(grid.kind(2), DocumentRow::Plain);
        // Nothing to record outside the grid, and nothing to read there.
        grid.set_kind(0, DocumentRow::Answer);
        grid.set_kind(geometry.rows + 1, DocumentRow::Answer);
        assert_eq!(grid.kind(0), DocumentRow::Plain);
    }

    /// A tool group's call row as its writer draws it in each mode
    /// (`super::super::toolgroup`, `p5-toolgroup-retint`): the statusline
    /// grey, a word in the highlighter's, a string, and a comment.
    fn call_row() -> Redraw {
        let drawn = |statusline: u8, word: u8, string: u8, comment: u8| {
            format!(
                "\u{1b}[38;5;{statusline}m\u{2514} Ran \u{1b}[38;5;{word}mecho\u{1b}[39m\
                 \u{1b}[38;5;{statusline}m \u{1b}[38;5;{string}m\"hi\"\u{1b}[39m\
                 \u{1b}[38;5;{statusline}m \u{1b}[38;5;{comment}m# note\u{1b}[39m{RESET}"
            )
        };
        Redraw::new(drawn(245, 252, 250, 245), drawn(241, 238, 241, 243))
    }

    #[test]
    fn a_row_drawn_again_is_what_a_fresh_paint_of_its_drawing_leaves() {
        // Upstream projects a tool group again in the new theme
        // (`tool_group_projection.zig:558-640,742-784@481104fd`), so its rows
        // are put back as their writer draws them in the mode reported -- the
        // cells' attributes whole, as a paint of the drawing leaves them --
        // not recoloured: the light string's 241 and the dark comment's 245
        // are the statusline's greys in that mode, and come back as the
        // highlighter's 250 and 243, which no colour rule could give them.
        let geometry = geometry();
        let redraw = call_row();
        for (from, to) in [(dark(), light()), (light(), dark())] {
            let mut grid = Grid::blank(geometry.rows, geometry.cols);
            grid.place_row(3, redraw.drawn(from.mode), &geometry);
            grid.record(3, DocumentRow::ToolGroup, Some(redraw.clone()));
            assert!(grid.retint_document(10, &to, &geometry) > 0);
            let mut fresh = Grid::blank(geometry.rows, geometry.cols);
            fresh.place_row(3, redraw.drawn(to.mode), &geometry);
            for column in 1..=geometry.cols {
                assert!(
                    grid.cell(3, column) == fresh.cell(3, column),
                    "{:?} -> {:?}, column {column}: {:?} against {:?}",
                    from.mode,
                    to.mode,
                    grid.cell(3, column),
                    fresh.cell(3, column)
                );
            }
            // The mode it is already in moves nothing.
            assert_eq!(grid.retint_document(10, &to, &geometry), 0);
        }
    }

    #[test]
    fn a_drawing_that_is_not_the_rows_text_is_not_put_on_it() {
        // A drawing is its writer's claim about what it put on the row. A row
        // that holds anything else -- another text, one cell more or less --
        // is not drawn over, nor recoloured: a tool group's row answers no
        // colour ([`Palette::document_retint_in`]), so it is left exactly as
        // it is, and nothing is created on it.
        let geometry = geometry();
        let redraw = call_row();
        for held in [
            format!("{BODY_DARK}\u{2514} Ran echo \"ho\" # note{RESET}"),
            format!("{BODY_DARK}\u{2514} Ran echo \"hi\" # note!{RESET}"),
            format!("{BODY_DARK}\u{2514} Ran echo \"hi\" # not{RESET}"),
        ] {
            let mut grid = Grid::blank(geometry.rows, geometry.cols);
            grid.place_row(3, &held, &geometry);
            grid.record(3, DocumentRow::ToolGroup, Some(redraw.clone()));
            let before = grid.clone();
            assert_eq!(grid.retint_document(10, &light(), &geometry), 0, "{held:?}");
            for column in 1..=geometry.cols {
                assert!(grid.cell(3, column) == before.cell(3, column), "{held:?}");
            }
        }
    }

    #[test]
    fn a_rows_drawings_move_with_it_and_go_with_its_cells() {
        // The same rule as its kind: one record per row, so a scroll and a
        // collapse carry the drawings with the cells, and a row written over,
        // erased or resized away has none.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, "header", &geometry);
        grid.record(3, DocumentRow::ToolGroup, Some(call_row()));
        grid.place_row(4, "call", &geometry);
        grid.record(4, DocumentRow::ToolGroup, Some(call_row()));
        grid.scroll_up(1);
        assert_eq!(
            grid.redraw(2),
            Some(&call_row()),
            "the drawing stayed behind"
        );
        assert_eq!(grid.redraw(4), None);
        grid.remove_rows(&[2], 4);
        assert_eq!(
            grid.redraw(2),
            Some(&call_row()),
            "a collapse left it behind"
        );
        assert_eq!(grid.kind(2), DocumentRow::ToolGroup);
        assert_eq!(grid.redraw(3), None);
        grid.set_kind(2, DocumentRow::ToolGroup);
        assert_eq!(grid.redraw(2), None, "a kind alone keeps an old drawing");
        grid.record(2, DocumentRow::ToolGroup, Some(call_row()));
        grid.place_row(2, "something else", &geometry);
        assert_eq!(grid.redraw(2), None);
        grid.record(2, DocumentRow::ToolGroup, Some(call_row()));
        grid.erase_row(2);
        assert_eq!(grid.redraw(2), None);
        grid.record(2, DocumentRow::ToolGroup, Some(call_row()));
        grid.resize(geometry.rows, geometry.cols);
        assert_eq!(grid.redraw(2), None);
    }

    #[test]
    fn a_retint_reaches_no_row_past_the_one_it_was_given() {
        // The band's own rows are the frame's to paint, and this emitter may
        // not write at or below its top row: a row past `last` keeps its
        // colour whatever it is painted in.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{BODY_DARK}above{RESET}"), &geometry);
        grid.place_row(5, &format!("{BODY_DARK}below{RESET}"), &geometry);
        assert_eq!(grid.retint_document(4, &light(), &geometry), "above".len());

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("{BODY_LIGHT}above{RESET}"), &geometry);
        expected.place_row(5, &format!("{BODY_DARK}below{RESET}"), &geometry);
        assert_eq!(diffed(&grid, &expected), "");
    }

    #[test]
    fn the_same_palette_twice_moves_no_cell_at_all() {
        // What a session reported dark, then light, then dark again before any
        // of it reached the terminal must cost: nothing. The mapping is onto
        // the mode in force rather than from the last one, so the second
        // report finds every cell already where it belongs.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{BODY_DARK}an answer{RESET}"), &geometry);
        let untouched = grid.clone();
        assert_eq!(grid.retint_document(10, &dark(), &geometry), 0);
        assert_eq!(diffed(&untouched, &grid), "", "a no-op retint wrote cells");
    }

    #[test]
    fn a_colour_this_crate_did_not_paint_survives_a_retint() {
        // The row the user typed carries no colour at all, and a row the
        // terminal was already holding carries somebody else's: both are left
        // exactly as they are, and neither is counted.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, "hello", &geometry);
        grid.place_row(4, "\u{1b}[38;5;31mnot ours\u{1b}[0m", &geometry);
        // The band's divider grey, which this emitter may not touch either.
        grid.place_row(5, "\u{1b}[38;5;240mrule\u{1b}[0m", &geometry);
        let untouched = grid.clone();

        assert_eq!(grid.retint_document(10, &light(), &geometry), 0);
        assert_eq!(
            diffed(&untouched, &grid),
            "",
            "a retint rewrote a colour this crate did not paint"
        );
    }

    #[test]
    fn a_retint_keeps_the_other_attributes_a_cell_was_painted_under() {
        // `SgrState` replays a whole state, so a retint that rebuilt the slot
        // list would drop whatever else the row had open -- and the answer
        // would come back in the new grey with its emphasis gone.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("\u{1b}[1m{BODY_DARK}bold{RESET}"), &geometry);
        assert_eq!(grid.retint_document(10, &light(), &geometry), "bold".len());

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("\u{1b}[1m{BODY_LIGHT}bold{RESET}"), &geometry);
        assert_eq!(
            diffed(&grid, &expected),
            "",
            "the retint took the row's other attributes with it"
        );
    }

    /// Upstream's link around `docs` (`inline_render.zig:535-560`).
    const DOCS_OPEN: &str = "\u{1b}]8;id=fx-1;https://example.com/docs\u{1b}\\";
    const LINK_CLOSE: &str = "\u{1b}]8;;\u{1b}\\";

    #[test]
    fn a_linked_run_opens_its_link_after_its_attributes_and_closes_it_with_the_row() {
        // Upstream's order for one cell (`engine.zig:2421-2428`): the SGR
        // transition, then the hyperlink transition -- and the link closed when
        // a cell outside it comes, with the reset in front of it.
        let after = painted(
            10,
            &format!("a{DOCS_OPEN}\u{1b}[4mbc\u{1b}[24m{LINK_CLOSE}d"),
        );
        assert_eq!(
            diffed(&Grid::blank(24, 80), &after),
            format!("\u{1b}[10;1Ha\u{1b}[4m{DOCS_OPEN}bc{RESET}{LINK_CLOSE}d")
        );
    }

    #[test]
    fn a_link_on_two_rows_is_closed_before_the_second_rows_cup_and_opened_again() {
        // tmux ends an active hyperlink at an automatic wrap
        // (`engine.zig:2400-2406`), and a link carried across a `CUP` would
        // take in whatever the next run paints first.
        let after = painted_rows(&[
            (10, &format!("{DOCS_OPEN}ab{LINK_CLOSE}")),
            (11, &format!("{DOCS_OPEN}cd{LINK_CLOSE}")),
        ]);
        assert_eq!(
            diffed(&Grid::blank(24, 80), &after),
            format!("\u{1b}[10;1H{DOCS_OPEN}ab{LINK_CLOSE}\u{1b}[11;1H{DOCS_OPEN}cd{LINK_CLOSE}")
        );
    }

    #[test]
    fn two_badges_that_name_one_file_are_one_run_and_a_second_file_switches_without_a_close() {
        // `emitHyperlinkTransition` (`engine.zig:3245-3260`): an open replaces
        // the link that was open.
        let one = "\u{1b}]8;;file:///a.png\u{1b}\\";
        let two = "\u{1b}]8;;file:///b.png\u{1b}\\";
        let after = painted(1, &format!("{one}[1]{one}[2]{two}[3]{LINK_CLOSE}"));
        assert_eq!(
            diffed(&Grid::blank(24, 80), &after),
            format!("\u{1b}[1;1H{one}[1][2]{two}[3]{LINK_CLOSE}")
        );
    }

    #[test]
    fn a_cell_that_changed_only_its_link_is_repainted() {
        let before = painted(10, &format!("{DOCS_OPEN}docs{LINK_CLOSE}"));
        let other = "\u{1b}]8;id=fx-2;https://example.com/docs\u{1b}\\";
        let after = painted(10, &format!("{other}docs{LINK_CLOSE}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H{other}docs{LINK_CLOSE}")
        );
        // A frame starts outside every link -- the one before it closed what
        // it opened -- so the cells that lost theirs are written plain.
        let unlinked = painted(10, "docs");
        assert_eq!(diffed(&before, &unlinked), "\u{1b}[10;1Hdocs");
    }

    #[test]
    fn a_retint_keeps_the_link_a_cell_was_painted_under() {
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(
            3,
            &format!("{BODY_DARK}see {DOCS_OPEN}docs{LINK_CLOSE}{RESET}"),
            &geometry,
        );
        assert_eq!(
            grid.retint_document(10, &light(), &geometry),
            "see docs".len()
        );

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(
            3,
            &format!("{BODY_LIGHT}see {DOCS_OPEN}docs{LINK_CLOSE}{RESET}"),
            &geometry,
        );
        assert_eq!(
            diffed(&grid, &expected),
            "",
            "the retint took the row's link with it"
        );
    }

    #[test]
    fn a_retint_moves_no_grapheme_and_no_column() {
        // Wide clusters, a combining mark and the continuation behind a family
        // in one row: the retint is a colour and nothing else, so every column
        // is still the column it was.
        let geometry = geometry();
        let text = format!("{FAMILY}\u{d55c}e\u{301}x");
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{BODY_DARK}{text}{RESET}"), &geometry);
        assert_eq!(
            grid.retint_document(10, &light(), &geometry),
            4,
            "a continuation cell was counted as a cell of its own"
        );

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("{BODY_LIGHT}{text}{RESET}"), &geometry);
        assert_eq!(diffed(&grid, &expected), "");
    }

    // -- Grid::row_matches (P3-WRAP settled-row reuse) --

    #[test]
    fn row_matches_true_only_for_the_text_that_would_render_identically() {
        let geometry = geometry();
        let grid = painted(10, "abc");
        let mut scratch = Grid::blank(0, 0);
        assert!(grid.row_matches(10, "abc", &geometry, &mut scratch));
        assert!(!grid.row_matches(10, "abd", &geometry, &mut scratch));
    }

    #[test]
    fn row_matches_is_false_for_a_row_this_grid_does_not_have() {
        let geometry = geometry();
        let grid = painted(10, "abc");
        let mut scratch = Grid::blank(0, 0);
        assert!(!grid.row_matches(0, "abc", &geometry, &mut scratch));
        assert!(!grid.row_matches(geometry.rows + 1, "abc", &geometry, &mut scratch));
    }

    #[test]
    fn row_matches_is_false_when_the_geometrys_width_disagrees_with_this_grids() {
        // A grid sized for a screen this geometry does not describe cannot be
        // trusted about any of its cells -- the width this comparison would
        // clip `row` to is not the width the grid's own cells were painted
        // at, so an equal slice would say nothing real.
        let narrower = crate::tui::layout::solve(24, 40, 1).expect("a narrower band");
        let grid = painted(10, "abc");
        let mut scratch = Grid::blank(0, 0);
        assert!(!grid.row_matches(10, "abc", &narrower, &mut scratch));
    }

    #[test]
    fn row_matches_is_false_when_only_the_attribute_state_differs() {
        let geometry = geometry();
        let grid = painted(10, "abc");
        let mut scratch = Grid::blank(0, 0);
        assert!(!grid.row_matches(10, &format!("{COLOUR}abc{RESET}"), &geometry, &mut scratch));
    }

    #[test]
    fn row_matches_is_false_when_a_wide_cluster_replaces_a_narrow_one() {
        let geometry = geometry();
        let grid = painted(10, "ab");
        let mut scratch = Grid::blank(0, 0);
        assert!(!grid.row_matches(10, &format!("{FAMILY}b"), &geometry, &mut scratch));
    }

    #[test]
    fn row_matches_is_false_when_a_combining_mark_is_added() {
        let geometry = geometry();
        let grid = painted(10, "ex");
        let mut scratch = Grid::blank(0, 0);
        assert!(!grid.row_matches(10, "e\u{301}x", &geometry, &mut scratch));
    }

    #[test]
    fn row_matches_agrees_with_place_row_on_a_row_clipped_to_the_screen() {
        // Both sides go through the one tokenizer at the same width, so a row
        // wider than the screen is clipped identically on both -- a match
        // means what `place_row` would really paint, not what the unclipped
        // text says, and a change inside the clip window still disagrees.
        let geometry = crate::tui::layout::solve(24, crate::tui::layout::MIN_COLS, 1)
            .expect("the narrowest band");
        let long = "abcdefghijklmnopqrstuvwxyz";
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(10, long, &geometry);
        let mut scratch = Grid::blank(0, 0);
        assert!(grid.row_matches(10, long, &geometry, &mut scratch));
        // Same length, and only the first character past the clip's own
        // width changed -- a comparison that clipped one side and not the
        // other would call this a match.
        let changed_past_the_clip = format!(
            "{}X{}",
            &long[..usize::from(geometry.cols)],
            &long[usize::from(geometry.cols) + 1..]
        );
        assert!(grid.row_matches(10, &changed_past_the_clip, &geometry, &mut scratch));
        // Changed *inside* the clip window disagrees.
        let mut inside = long.to_string();
        inside.replace_range(2..3, "X");
        assert!(!grid.row_matches(10, &inside, &geometry, &mut scratch));
    }

    /// A row of bytes that paint one cell: one letter under 64 Ki combining
    /// marks, then `tail` (T-U4 round 3's `heavy` case, smaller). Its
    /// placement and its comparison read every byte of it, cluster by
    /// cluster.
    fn heavy() -> String {
        format!("a{}tail", "\u{301}".repeat(64 * 1024))
    }

    #[test]
    fn a_row_this_grid_placed_is_matched_again_without_reading_it() {
        // GRID-ROW-COST (T-U4 round 3's residual): an append that leaves a
        // settled row where it is asks this whether the row already holds it,
        // and answered by placing the row again on a scratch row, cluster by
        // cluster -- 133-171 ms a frame for 8 MiB of marks
        // (`.prd/tui-phase5/receipts/cancel-marker/round3/probe.txt`).
        let geometry = geometry();
        let row = heavy();
        let grid = painted(10, &row);
        let mut scratch = Grid::blank(0, 0);
        let (matched, read) =
            bytes_tokenized(|| grid.row_matches(10, &row, &geometry, &mut scratch));
        assert!(matched, "the row it placed is not the row it holds");
        assert_eq!(read, 0, "the row was read again to say it had not changed");
    }

    /// What [`Grid::row_matches`] answers with nothing remembered about the
    /// row: `row` placed on a blank row of the grid's width and compared cell
    /// for cell -- the oracle the remembered answer is held to.
    fn freshly_matches(grid: &Grid, line: u16, row: &str, geometry: &Geometry) -> bool {
        if grid.cols != geometry.cols {
            return false;
        }
        let Some(span) = grid.span(line) else {
            return false;
        };
        let mut fresh = Grid::blank(1, grid.cols);
        fresh.put_row(1, row, geometry);
        grid.cells[span] == fresh.cells[..]
    }

    /// [`Grid::row_matches`]'s answer, held to [`freshly_matches`]'s.
    fn matches(grid: &Grid, line: u16, row: &str, geometry: &Geometry) -> bool {
        let answered = grid.row_matches(line, row, geometry, &mut Grid::blank(0, 0));
        assert_eq!(
            answered,
            freshly_matches(grid, line, row, geometry),
            "row {line}: the remembered answer is not a fresh placement's"
        );
        answered
    }

    #[test]
    fn a_rows_text_moves_with_its_cells_and_goes_with_any_change_to_them() {
        // What a row was laid from is a claim about its cells, so it holds
        // exactly as long as they do: a scroll and a removal carry it with
        // them, and an erase, a placement and a resize -- every change but a
        // move -- leave a row that answers as a fresh placement does.
        let geometry = geometry();
        let row = heavy();
        let mut grid = painted(10, &row);
        assert!(matches(&grid, 10, &row, &geometry));

        grid.scroll_up(1);
        assert!(
            matches(&grid, 9, &row, &geometry),
            "scrolled with its cells"
        );
        assert!(!matches(&grid, 10, &row, &geometry));

        grid.place_row(1, "above it", &geometry);
        grid.remove_rows(&[1], 12);
        assert!(
            matches(&grid, 8, &row, &geometry),
            "moved up with its cells"
        );
        assert!(!matches(&grid, 9, &row, &geometry));

        // The writer's record of what the row is says nothing about its cells.
        grid.record(8, DocumentRow::ToolGroup, Some(call_row()));
        let (matched, read) =
            bytes_tokenized(|| grid.row_matches(8, &row, &geometry, &mut Grid::blank(0, 0)));
        assert!(
            matched && read == 0,
            "a record forgot what the row was laid from"
        );

        grid.place_row(8, "another text", &geometry);
        assert!(!matches(&grid, 8, &row, &geometry));
        assert!(matches(&grid, 8, "another text", &geometry));

        grid.place_row(8, &row, &geometry);
        grid.erase_row(8);
        assert!(!matches(&grid, 8, &row, &geometry), "an erased row");
        assert!(matches(&grid, 8, "", &geometry));

        grid.place_row(8, &row, &geometry);
        grid.resize(geometry.rows, geometry.cols);
        assert!(!matches(&grid, 8, &row, &geometry), "a resized grid");

        // And a band painted over it.
        grid.place_row(geometry.band_top(), &row, &geometry);
        grid.paint_band(&["> ".to_string()], &geometry);
        assert!(!matches(&grid, geometry.band_top(), &row, &geometry));
    }

    #[test]
    fn a_retint_leaves_a_row_that_answers_as_a_fresh_placement() {
        // A theme report moves a row's colours without placing it: recoloured
        // or drawn again, the row holds other cells than its text lays, and
        // asking about the text must say so.
        let geometry = geometry();
        let marks = "\u{301}".repeat(64 * 1024);
        let notice = format!("{NOTICE_DARK}[tool] a{marks}{RESET}");
        let mut grid = painted(3, &notice);
        let drawing = call_row();
        let drawn = format!("{}{marks}", drawing.drawn(crate::tui::theme::Mode::Dark));
        let drawing = Redraw::new(
            drawn.clone(),
            format!("{}{marks}", drawing.drawn(crate::tui::theme::Mode::Light)),
        );
        grid.place_row(4, &drawn, &geometry);
        grid.record(4, DocumentRow::ToolGroup, Some(drawing));
        // Nothing moved: what the rows were laid from still holds.
        assert_eq!(grid.retint_document(10, &dark(), &geometry), 0);
        assert!(matches(&grid, 3, &notice, &geometry));
        assert!(matches(&grid, 4, &drawn, &geometry));

        assert!(grid.retint_document(10, &light(), &geometry) > 0);
        assert!(!matches(&grid, 3, &notice, &geometry), "recoloured");
        assert!(!matches(&grid, 4, &drawn, &geometry), "drawn again");
    }

    #[test]
    fn a_row_laid_for_a_narrower_screen_is_not_the_row_this_screen_lays() {
        // The tokenizer cuts a row at the narrower of the grid and the screen,
        // so the screen's width is part of what a row was laid from: the same
        // text laid for 40 columns on this 80-column grid holds 40 columns of
        // it, and the whole 80 is what this screen's placement leaves.
        let geometry = geometry();
        let narrower = crate::tui::layout::solve(24, 40, 1).expect("a narrower band");
        let row = format!("{}{}", "x".repeat(70), heavy());
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(10, &row, &narrower);
        assert!(!matches(&grid, 10, &row, &geometry));
        grid.place_row(10, &row, &geometry);
        assert!(matches(&grid, 10, &row, &geometry));
    }

    // -- a variation selector, and the cells after it ------------------------
    //
    // Upstream's frame writer, `src/core/terminal/engine.zig:2548-2589@481104fd`
    // (`95e6c8fd`): a display unit whose suffix holds U+FE0E or U+FE0F has both
    // of its cells cleared first when it is two wide, and the cursor is put back
    // on the column after it when anything else is written on the row; a
    // two-wide unit with a suffix that ends at the right margin is written with
    // autowrap on for its length.

    /// The text-presentation lightning bolt of upstream's own test: two cells,
    /// as upstream and this crate both count it.
    const BOLT: &str = "\u{26a1}\u{fe0e}";
    /// A sun with the emoji selector: two cells.
    const SUN_EMOJI: &str = "\u{2600}\u{fe0f}";
    /// A sun with the text selector: one cell.
    const SUN_TEXT: &str = "\u{2600}\u{fe0e}";

    #[test]
    fn a_wide_glyph_with_a_text_selector_is_cleared_first_and_the_cursor_put_back_after_it() {
        // Upstream's test, `engine.zig:4056-4085@481104fd`: `\x1b[2X` and the
        // glyph, then `\x1b[5G` -- the column after it, one-based.
        let before = painted(10, "");
        let after = painted(10, &format!("a {BOLT} b"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1Ha \u{1b}[2X{BOLT}\u{1b}[5G b")
        );
    }

    #[test]
    fn a_wide_glyph_with_an_emoji_selector_is_cleared_first_and_the_cursor_put_back_after_it() {
        let before = painted(10, "");
        let after = painted(10, &format!("a {SUN_EMOJI} b"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1Ha \u{1b}[2X{SUN_EMOJI}\u{1b}[5G b")
        );
    }

    #[test]
    fn a_narrow_glyph_with_a_selector_clears_nothing_and_still_puts_the_cursor_back() {
        // One cell: upstream clears only a two-cell unit, and anchors any.
        let before = painted(10, "");
        let after = painted(10, &format!("a {SUN_TEXT} b"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1Ha {SUN_TEXT}\u{1b}[4G b")
        );
    }

    #[test]
    fn a_glyph_without_a_selector_needs_no_anchor() {
        let before = painted(10, "");
        let after = painted(10, &format!("a \u{26a1} b {FAMILY} c"));
        let text = diffed(&before, &after);
        assert!(!text.contains('G') && !text.contains('X'), "{text:?}");
    }

    #[test]
    fn a_selector_glyph_with_nothing_after_it_is_not_anchored() {
        // Upstream's `col <= last`: nothing else is written on the row.
        let before = painted(10, "");
        let after = painted(10, &format!("a {BOLT}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1Ha \u{1b}[2X{BOLT}")
        );
    }

    #[test]
    fn the_erase_after_a_selector_glyph_starts_where_the_grid_puts_the_glyphs_end() {
        // The row got shorter, so an `EL` follows the glyph. A terminal that
        // draws it one cell wider than this grid counts would start the erase a
        // cell late and leave a stale cell; the anchor in front of the `EL`
        // puts it where the grid's row ends.
        let before = painted(10, &format!("a {SUN_TEXT}bcd"));
        let after = painted(10, &format!("a {BOLT}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;3H\u{1b}[2X{BOLT}\u{1b}[5G{ERASE_LINE}")
        );
    }

    #[test]
    fn a_wide_selector_glyph_at_the_margin_is_written_with_autowrap_on_and_not_anchored() {
        // `guard_margin_suffix`: autowrap is off for the session
        // (`super::super::term::MODE_SET`'s `?7l`), and a terminal can drop the
        // suffix of a unit whose base ends at the margin. Upstream's order:
        // `?7h`, the clear, the glyph and its suffix, `?7l`.
        let geometry = geometry();
        let lead = "x".repeat(usize::from(geometry.cols) - 2);
        let before = painted(10, "");
        let after = painted(10, &format!("{lead}{BOLT}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H{lead}\u{1b}[?7h\u{1b}[2X{BOLT}\u{1b}[?7l")
        );
    }

    #[test]
    fn a_wide_unit_with_any_suffix_at_the_margin_is_written_with_autowrap_on() {
        // Upstream asks for a suffix, not a selector: a ZWJ family too.
        let geometry = geometry();
        let lead = "x".repeat(usize::from(geometry.cols) - 2);
        let before = painted(10, "");
        let after = painted(10, &format!("{lead}{FAMILY}"));
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H{lead}\u{1b}[?7h{FAMILY}\u{1b}[?7l")
        );
    }

    #[test]
    fn a_wide_unit_without_a_suffix_or_short_of_the_margin_is_not_guarded() {
        let geometry = geometry();
        let cols = usize::from(geometry.cols);
        let before = painted(10, "");
        for row in [
            format!("{}\u{d55c}", "x".repeat(cols - 2)),
            format!("{}{FAMILY}", "x".repeat(cols - 3)),
        ] {
            let text = diffed(&before, &painted(10, &row));
            assert!(!text.contains("\u{1b}[?7"), "{row:?}: {text:?}");
        }
    }

    #[test]
    fn a_one_cell_glyph_and_a_selector_a_colour_split_off_are_written_split() {
        // `place_row` joins the selector onto the one-cell glyph and keeps its
        // one cell; the pair written together reads as one cluster two cells
        // wide. So the diff writes the glyph, the pen as it is (a reset with
        // nothing on, the attributes otherwise), the selector, then the
        // anchor -- a selector glyph.
        let before = painted(10, "");
        let after = painted(10, "\u{2600}\u{1b}[1m\u{fe0f} x");
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H\u{2600}{RESET}\u{fe0f}\u{1b}[2G\u{1b}[1m x{RESET}")
        );
        // Under a colour: the colour again, which changes nothing; and the
        // longest part that is still one cell, the rest after it.
        let after = painted(
            10,
            &format!("{COLOUR}\u{2600}\u{1b}[1m\u{fe0f}\u{301}{RESET} x"),
        );
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H{COLOUR}\u{2600}{COLOUR}\u{fe0f}\u{301}\u{1b}[2G{RESET} x")
        );
        // A mark between the two keeps the pair one cell written together
        // (`super::wrap::width`), so it is written together.
        let after = painted(
            10,
            &format!("{COLOUR}\u{2600}\u{1b}[1m\u{301}\u{fe0f}{RESET} x"),
        );
        assert_eq!(
            diffed(&before, &after),
            format!("\u{1b}[10;1H{COLOUR}\u{2600}\u{301}\u{fe0f}\u{1b}[2G{RESET} x")
        );
        // Written together already, as one cluster: written together.
        let after = painted(10, &format!("{SUN_EMOJI} x"));
        assert!(diffed(&before, &after).contains(SUN_EMOJI));
    }

    #[test]
    fn a_row_written_whole_keeps_a_split_cluster_behind_its_colour() {
        let mut out = Vec::new();
        write_row_text(
            &mut out,
            "\u{d55c}\u{1b}[39m\u{301}\u{2600}\u{1b}[1m\u{fe0f}x",
            80,
        );
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            "\u{d55c}\u{1b}[39m\u{301}\u{2600}\u{1b}[1m\u{fe0f}\u{1b}[4Gx"
        );
        // A selector a colour split off a two-cell glyph is no cell's
        // (`place_row` drops it), so the glyph is no selector glyph: no clear
        // and no anchor, as the diff writes none for the `⚡` the grid holds.
        let row = "\u{26a1}\u{1b}[1m\u{fe0e}x";
        let mut out = Vec::new();
        write_row_text(&mut out, row, 80);
        assert_eq!(String::from_utf8(out).expect("utf-8"), row);
        let text = diffed(&painted(10, ""), &painted(10, row));
        assert!(
            !text.contains("\u{1b}[2X") && !text.contains('G'),
            "{text:?}"
        );
    }
}
