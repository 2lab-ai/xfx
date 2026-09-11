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
//! What the diff emits is `CUP`, replacement text, and `EL`. **No `ECH`**: it
//! is not in the subset `scripts/smoke-tui.sh`'s oracle models, and an erase to
//! the end of a row says the same thing about a row that got shorter without
//! adding a sequence the emulator -- and therefore the acceptance suite --
//! would have to be widened for.

use unicode_segmentation::UnicodeSegmentation;

use super::frame::cup;
use super::layout::Geometry;
use super::pacer::SgrState;
use super::theme::Palette;

/// Erase from the cursor to the end of the row it is on.
const ERASE_LINE: &str = "\u{1b}[K";

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

/// What a terminal is holding, or is about to be holding.
#[derive(Debug, Clone)]
pub(crate) struct Grid {
    rows: u16,
    cols: u16,
    /// Row-major, `rows * cols` long, and that length is an invariant every
    /// method below relies on: [`Grid::span`] is the only place a row becomes
    /// an index range.
    cells: Vec<Cell>,
}

impl Grid {
    /// A screen with nothing on it.
    pub(crate) fn blank(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Cell::Empty; usize::from(rows) * usize::from(cols)],
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
    }

    /// Puts `row`'s text on row `line`, and blanks the rest of it.
    ///
    /// The **one** tokenizer, and that is the point of routing it through
    /// [`super::frame::row_text`]: the same function decides what a row may
    /// carry when it is written to a terminal and what this grid believes a
    /// terminal is showing. A second reading of a row -- one that kept a
    /// sequence the painter drops, or measured a cluster differently -- would
    /// make the diff emit bytes for cells that never changed and, worse, skip
    /// cells that did.
    pub(crate) fn place_row(&mut self, line: u16, row: &str, geometry: &Geometry) {
        self.erase_row(line);
        let Some(span) = self.span(line) else {
            return;
        };
        let cols = usize::from(self.cols);
        // Clipped to the narrower of the two, because a row built for a screen
        // this grid is not the size of is a row this grid cannot hold.
        let text = super::frame::row_text(row, self.cols.min(geometry.cols));
        let mut sgr = SgrState::default();
        let mut column = 0usize;
        let mut rest = text.as_ref();
        while !rest.is_empty() {
            if let Some(len) = super::pacer::escape_at(rest) {
                // `row_text` keeps colours and removes everything else, so the
                // only sequences that reach here are the ones a cell is allowed
                // to remember. Handed to the model whole; what it does not
                // model, it drops.
                sgr.observe(&rest[..len]);
                rest = &rest[len..];
                continue;
            }
            let Some(cluster) = rest.graphemes(true).next() else {
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
                if let Some(Cell::Lead { grapheme, .. }) = column
                    .checked_sub(1)
                    .map(|at| &mut self.cells[span.start + at])
                {
                    grapheme.push_str(cluster);
                }
                continue;
            }
            if column + width > cols {
                // `row_text` clips to the same width, so this is unreachable
                // through it; a cluster that straddled the last column would be
                // drawn in a column the layout believes is empty.
                break;
            }
            self.cells[span.start + column] = Cell::Lead {
                grapheme: cluster.to_string(),
                width: u8::try_from(width).unwrap_or(u8::MAX),
                sgr: sgr.clone(),
            };
            for offset in 1..width {
                self.cells[span.start + column + offset] = Cell::Continuation;
            }
            column += width;
        }
    }

    /// Whether row `line` already holds what placing `row` on it would leave
    /// behind, without writing anything.
    ///
    /// `scratch` is caller-owned so a loop over many settled rows reuses one
    /// row's own backing storage instead of allocating a fresh one per row --
    /// see [`super::frame::Band`]'s field of the same name. Narrower than
    /// "allocates nothing": [`Self::place_row`] still allocates a `String`
    /// per grapheme cluster it writes ([`Cell::Lead`]), every call, whether
    /// the row it renders ends up matching or not -- what is reused here is
    /// only the row's own cell-vector storage, not each cell's content. It is
    /// resized here whenever it is not already one row of this grid's own
    /// width, and then filled through [`Self::place_row`]: the **one**
    /// tokenizer, so this cannot agree where a real placement would
    /// disagree, or the reverse.
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
        if scratch.rows != 1 || scratch.cols != self.cols {
            scratch.resize(1, self.cols);
        }
        scratch.place_row(1, row, geometry);
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
    /// Three things make it narrow enough to be safe on rows nobody repaints:
    ///
    /// * **Only a [`Cell::Lead`] this crate's palette recognises.** The colour
    ///   is asked of the cell rather than of the row it came from, and
    ///   [`Palette::document_retint`] answers `None` for every one it did not
    ///   paint -- so an [`Cell::Empty`] cell, a continuation, the user's own
    ///   echo and whatever the terminal was holding before xfx ran are all
    ///   left exactly as they are. Nothing here can *create* a cell, which is
    ///   what stops a repaint reconstructing history.
    /// * **Only the foreground slot.** [`SgrState::observe`] replaces the one
    ///   slot the sequence names and leaves the rest of them, in the order
    ///   they were opened, so a bold or underlined answer row stays bold and
    ///   underlined.
    /// * **Nothing else about the cell.** The grapheme, its width and the
    ///   continuation behind it are untouched, so the count this returns is a
    ///   count of colours and the row is the same shape it was.
    ///
    /// A cell whose attribute slot cannot be read is skipped rather than
    /// guessed at: it is one the checker would refuse the whole vector for
    /// (`super::check::CellState::of`), and one this crate did not write.
    pub(crate) fn retint_document(&mut self, last: u16, palette: &Palette) -> usize {
        let mut moved = 0usize;
        for line in 1..=last {
            let Some(span) = self.span(line) else {
                continue;
            };
            for cell in &mut self.cells[span] {
                let Cell::Lead { sgr, .. } = cell else {
                    continue;
                };
                let Ok(colour) = sgr.color() else {
                    continue;
                };
                let Some(wanted) = palette.document_retint(colour) else {
                    continue;
                };
                sgr.observe(wanted);
                moved += 1;
            }
        }
        moved
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
        let rows = self.rows.min(target.rows).min(geometry.rows);
        let cols = usize::from(self.cols.min(target.cols).min(geometry.cols));
        let mut touched = 0usize;
        // What the terminal has switched on, as this frame left it.
        let mut open = String::new();
        for line in 1..=rows {
            let (Some(before), Some(after)) = (self.span(line), target.span(line)) else {
                continue;
            };
            let old = &self.cells[before.start..before.start + cols];
            let new = &target.cells[after.start..after.start + cols];
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
            cup(out, line, column_of(start));
            for cell in &new[start..stop] {
                match cell {
                    // Carried by the lead in front of it.
                    Cell::Continuation => {}
                    Cell::Empty => {
                        reopen(out, &mut open, "");
                        out.push(b' ');
                    }
                    Cell::Lead { grapheme, sgr, .. } => {
                        reopen(out, &mut open, &sgr.reopen());
                        out.extend_from_slice(grapheme.as_bytes());
                    }
                }
            }
            if old_end > new_end {
                // The row got shorter. The caret is at `new_end` -- every cell
                // written above advanced it by exactly the columns it covered --
                // so this erases precisely the tail that is no longer there,
                // including the orphaned second column of a wide cluster.
                //
                // With the attributes closed first, because an erase paints the
                // *background* of what it clears on a terminal that has one
                // switched on. This palette only ever opens a foreground, so
                // today it costs nothing and prevents a class of defect that
                // would otherwise arrive with the first background colour.
                reopen(out, &mut open, "");
                out.extend_from_slice(ERASE_LINE.as_bytes());
            }
            touched += 1;
        }
        // Whatever the last run opened, closed: the next frame assumes a
        // terminal in the default state, and this is what makes that true.
        reopen(out, &mut open, "");
        touched
    }
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

/// Puts the terminal into `wanted`, if it is not there already.
///
/// A reset in front of anything that is not a pure addition, because [`SgrState`]
/// replays a *state* rather than a delta: replaying it onto a terminal that
/// still has the previous run's attributes on would leave whatever the new run
/// does not mention switched on.
fn reopen(out: &mut Vec<u8>, open: &mut String, wanted: &str) {
    if open == wanted {
        return;
    }
    if !open.is_empty() {
        out.extend_from_slice(RESET.as_bytes());
    }
    out.extend_from_slice(wanted.as_bytes());
    open.clear();
    open.push_str(wanted);
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
        grid.place_row(geometry.divider, "stale", &geometry);
        grid.paint_band(&["--".to_string()], &geometry);

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(1, "the document", &geometry);
        expected.place_row(geometry.divider, "--", &geometry);
        assert_eq!(diffed(&grid, &expected), "");
    }

    // -- Grid::retint_document (P3-THEME, the document's visible cells) --

    /// The two palettes a retint case moves between, at the depth every case
    /// here paints in.
    fn dark() -> Palette {
        Palette {
            mode: crate::tui::theme::Mode::Dark,
            depth: crate::tui::theme::Depth::Ansi256,
        }
    }

    fn light() -> Palette {
        Palette {
            mode: crate::tui::theme::Mode::Light,
            depth: crate::tui::theme::Depth::Ansi256,
        }
    }

    /// The document's two owned greys, dark and light, spelled out.
    const BODY_DARK: &str = "\u{1b}[38;5;255m";
    const BODY_LIGHT: &str = "\u{1b}[38;5;235m";
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";

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
            grid.retint_document(10, &light()),
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
    fn a_retint_reaches_no_row_past_the_one_it_was_given() {
        // The band's own rows are the frame's to paint, and this emitter may
        // not write at or below its top row: a row past `last` keeps its
        // colour whatever it is painted in.
        let geometry = geometry();
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        grid.place_row(3, &format!("{BODY_DARK}above{RESET}"), &geometry);
        grid.place_row(5, &format!("{BODY_DARK}below{RESET}"), &geometry);
        assert_eq!(grid.retint_document(4, &light()), "above".len());

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
        assert_eq!(grid.retint_document(10, &dark()), 0);
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

        assert_eq!(grid.retint_document(10, &light()), 0);
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
        assert_eq!(grid.retint_document(10, &light()), "bold".len());

        let mut expected = Grid::blank(geometry.rows, geometry.cols);
        expected.place_row(3, &format!("\u{1b}[1m{BODY_LIGHT}bold{RESET}"), &geometry);
        assert_eq!(
            diffed(&grid, &expected),
            "",
            "the retint took the row's other attributes with it"
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
            grid.retint_document(10, &light()),
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
}
