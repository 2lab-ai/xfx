//! The band writer: one buffer, one `write_all`, one flush per frame.
//!
//! Everything the TUI puts on the screen goes through here, and that is the
//! point rather than tidiness. The band shares the screen with the terminal's
//! own document, so "what is on those rows is what this module last wrote" is
//! the only thing that makes the band's state knowable at all -- and it stops
//! being true the moment a second writer, or a second `write(2)` inside one
//! frame, can interleave with it.
//!
//! A frame is wrapped in synchronized output (`?2026h` ... `?2026l`), so a
//! terminal that supports it presents the whole band at once instead of
//! painting it row by row, and the cursor is hidden across the paint for the
//! terminals that do not. Every row is placed with `CUP` and clipped to the
//! screen's width: autowrap is off (`?7l` is in the mode set), so a row that
//! ran past the last column would be truncated by the terminal anyway, and
//! clipping here is what keeps the byte count honest.
//!
//! **A frame is a diff.** The band keeps a [`Grid`] of what the terminal is
//! holding and builds a second one of what it should be holding; what goes on
//! the wire is the difference, and a frame whose facts did not change goes
//! nowhere at all ([`Commit::NoChange`]). The shadow may only be advanced by
//! bytes that were really delivered -- a shadow updated from a refused write
//! believes a band is on a screen that never got it, and never paints it again.
//!
//! The Phase-1 painter is still here, and only in builds that ask for it: it is
//! the *reference* scenario 13 compares the diff against on a real terminal,
//! behind the compile-time `fault-injection` seam, so no released binary has a
//! way to select it.
//!
//! A **title** is frame metadata rather than a cell. Nothing on the grid moves
//! when the window title changes, so a skip that consulted the cells alone
//! would drop it; it travels inside the same synchronized frame as everything
//! else.

use std::borrow::Cow;
use std::io::{self, Write};

use unicode_segmentation::UnicodeSegmentation;

use super::grid::Grid;
use super::layout::Geometry;

/// Begins a frame: synchronized output on, cursor hidden.
const BEGIN_FRAME: &str = "\x1b[?2026h\x1b[?25l";

/// Ends one: cursor shown, synchronized output off.
const END_FRAME: &str = "\x1b[?2026l\x1b[?25h";

/// Erase from the cursor to the end of the screen.
///
/// Written by the Phase-1 painter on every frame, and by the diff on the one
/// frame that follows external damage ([`Band::invalidate`]): a shadow that has
/// been forgotten cannot say what is on those rows, and a diff against a blank
/// one would rewrite the band's own columns and leave whatever the shell put
/// beside them.
const ERASE_BELOW: &str = "\x1b[J";

/// Erase from the cursor to the end of the row it is on.
const ERASE_LINE: &str = "\x1b[K";

/// The band, and the buffer it is built in.
pub(crate) struct Band {
    /// Kept across frames so building one allocates nothing after the first.
    buffer: Vec<u8>,
    /// The band's top row as of the last thing this band began writing, or
    /// `None` while it has written nothing.
    ///
    /// It is what the exit clears from ([`super::term::shutdown`]), and the
    /// distinction it carries is load-bearing: a session that drew no band has
    /// no row to clear from, and clearing from the screen's first row instead
    /// would erase output the shell wrote before xfx ran.
    ///
    /// It is also what a **shrinking** band gives back. The composer grows and
    /// shrinks with what is typed into it (`super::shell`), so the divider
    /// moves; the rows above a divider that moved *down* were the band's a
    /// moment ago and are the document's now, and nothing else would ever
    /// rewrite them -- Phase 1 repaints no transcript. So they are erased, once,
    /// by whatever this band writes next ([`Band::release`]).
    ///
    /// Which is why this field moves in **two directions on two different
    /// clocks**. It is lowered *before* a write, because a frame that failed
    /// halfway still put bytes on the rows it had begun painting and the exit
    /// has to clear from the top of them. It is raised -- to a divider that has
    /// moved down -- only *after* a write that landed, because until those
    /// erasures are really on the screen the old rows are still on it: a band
    /// that recorded the new top from bytes it never delivered would erase them
    /// never, and the exit would clear from below them.
    painted: Option<u16>,
    /// What the terminal is holding, as far as bytes that were really
    /// delivered can say. `0x0` until the first frame sizes it.
    shadow: Grid,
    /// What it will be holding once the frame being built lands. Kept across
    /// frames so building one allocates nothing after the first, and swapped
    /// with [`shadow`](Self::shadow) only by a write that succeeded.
    target: Grid,
    /// The window title this session wants, and `None` for a session that has
    /// not asked for one -- which is every session until [`Band::set_title`] is
    /// called, and therefore every test below.
    title: Option<String>,
    /// The one the terminal was last *told*, so a title that did not change
    /// costs no bytes and a title that did cannot be skipped.
    shown_title: Option<String>,
    /// Where the last delivered frame left the caret.
    ///
    /// Frame metadata like the title, and for the same reason: the caret is the
    /// terminal's own cursor rather than a cell, so a keystroke that only moved
    /// it changes nothing on the grid -- and a skip that consulted the cells
    /// alone would leave the caret where the previous frame put it.
    caret: Option<(u16, u16)>,
    /// Whether the next frame must be a **whole** one.
    ///
    /// Raised by [`Band::invalidate`] and lowered by the write that answers it.
    /// It is a separate fact from "the shadow is blank", and the difference is
    /// the whole of what it buys: a blank shadow is a *claim* that those cells
    /// are empty, and a diff believes it -- so the band rewrites its own
    /// columns and finds nothing to erase beyond them, leaving the shell's text
    /// on the rest of every band row. This says the opposite: nothing about
    /// those rows is known, so the frame erases them before it paints.
    damaged: bool,
}

/// What one frame cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Commit {
    /// Bytes went out.
    Painted,
    /// Nothing on the screen, in its title or under its caret was different, so
    /// nothing was written at all.
    NoChange,
}

impl Band {
    pub(crate) fn new() -> Self {
        Self {
            buffer: Vec::new(),
            painted: None,
            // Sized by the first frame, from the geometry it is handed: a band
            // has no screen of its own to ask.
            shadow: Grid::blank(0, 0),
            target: Grid::blank(0, 0),
            title: None,
            shown_title: None,
            caret: None,
            // A band that has painted nothing knows nothing, and the first
            // frame is a whole one for the same reason every damaged frame is.
            damaged: true,
        }
    }

    /// Asks for `title` on the terminal's title bar from the next frame on.
    ///
    /// Recorded rather than written: a title is frame metadata, and a second
    /// writer -- even one writing an `OSC` that moves no cell -- is exactly the
    /// property this module exists to keep.
    pub(crate) fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }

    /// Forgets everything this band believes about the screen.
    ///
    /// For every way the screen stops being the band's to describe -- which is
    /// what `super::render_request::Attempt::damaged` reports, and nothing
    /// else:
    ///
    /// * a `/clear`, which erases the screen and its scrollback;
    /// * a Ctrl-L, which asks for exactly this;
    /// * the **resume after a `SIGTSTP`**, where the terminal was handed back
    ///   and the shell owned it in between, so its output is on the band's own
    ///   rows;
    /// * (Phase 2 item 12) a resize, after which the terminal has re-wrapped
    ///   its own document by rules xfx does not model.
    ///
    /// The next frame is a **whole** one. `damaged` is what makes that happen,
    /// and it is a different statement from the blank shadow beside it: blank
    /// is a *claim* that those cells are empty, which a diff believes and then
    /// writes only the band's own columns over; `damaged` says nothing about
    /// them is known, so the frame opens with the Phase-1 erase.
    ///
    /// **The title the session wants is kept; what the terminal has been
    /// *told* is forgotten.** [`Band::title`] is this band's own intention and
    /// nothing here touches it. [`Band::shown_title`] is a claim about a
    /// particular terminal, and a terminal that was given back may have had the
    /// title taken back with it -- a stop's restore pops the title stack
    /// (`super::term::RESTORE`), so the window is the shell's again. Clearing
    /// it is what makes the next frame re-assert `OSC 2`.
    ///
    /// The callers that did not lose the title pay one sequence for it, on a
    /// frame that is a whole repaint anyway; none of them can grow the title
    /// *stack*, because only the mode set and the restores push and pop it.
    ///
    /// **The row this band clears from is bounded by the screen it is now on.**
    /// [`Band::painted`] is a row number in the screen the last frame was
    /// painted on, and the resize above is the one caller that can hand this a
    /// *smaller* one -- so a session that shrank and then left before its next
    /// frame landed would give `super::term::shutdown` a row below the
    /// terminal's last. A terminal answers that by clamping to its bottom row
    /// and erasing from there, which leaves the band's own rows on the screen
    /// after xfx has exited. A bound rather than a reset: forgetting the row
    /// would leave the rows a *grown* screen's band used to own unerased
    /// ([`Self::release`]), and nothing else in this phase ever repaints them.
    /// The other callers hand this the screen the band is already on, where the
    /// clamp cannot bite -- `painted` is never below the band's own top row.
    pub(crate) fn invalidate(&mut self, rows: u16, cols: u16) {
        self.shadow.resize(rows, cols);
        self.target.resize(rows, cols);
        self.painted = self.painted.map(|top| top.min(rows));
        self.caret = None;
        self.damaged = true;
        // What the terminal was told, rather than what this band wants: see the
        // title paragraph above.
        self.shown_title = None;
    }

    /// The band's top row, if this band has painted one.
    pub(crate) fn painted_top(&self) -> Option<u16> {
        self.painted
    }

    /// Builds the bytes of one **whole-band** frame: the Phase-1 painter.
    ///
    /// Kept as the reference the diff is judged against rather than as a
    /// fallback, and compiled only into builds that can ask for it -- the test
    /// harness, and the `fault-injection` binary scenario 13 drives through
    /// [`super::fault::Fault::FullPaintReference`]. A released binary has no
    /// branch that reaches it, so "the band is diffed" is a property of the
    /// artefact rather than of a default.
    ///
    /// Pure with respect to the terminal -- nothing is written -- so a frame's
    /// geometry is assertable without one, which is the only way the band's row
    /// numbers get tested at all. The buffer it is built in is reused; the copy
    /// handed back is the caller's, and [`commit`](Self::commit) writes that
    /// copy in a single call.
    ///
    /// `rows` are the band's rows, top first, starting at the band's top row
    /// ([`Geometry::band_top`]), which is the activity row while a turn is
    /// running and the divider otherwise. `cursor`
    /// is `(row, cells)`: the terminal's own one-based row, and the number of
    /// cells to the **left** of the caret on it -- a count, which is what the
    /// composer measures, converted to a one-based column here and nowhere
    /// else.
    #[cfg(any(test, feature = "fault-injection"))]
    pub(crate) fn render(
        &mut self,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> Vec<u8> {
        self.buffer.clear();
        self.buffer.extend_from_slice(BEGIN_FRAME.as_bytes());
        self.retitle();
        self.release(geometry);
        // The top of what this frame is about to write, which on a band that
        // shrank is still the *old* top: the erasures for the rows between the
        // two are in this buffer and have not been delivered. Recorded before
        // the write, because a frame that fails halfway has still written some
        // of it; raised to the divider by the write that lands
        // ([`Self::delivered`]).
        self.painted = Some(self.top(geometry));
        // The band's own rows and nothing above them: the erase starts at the
        // band's top row, so the document keeps every row it has.
        cup(&mut self.buffer, geometry.band_top(), 1);
        self.buffer.extend_from_slice(ERASE_BELOW.as_bytes());
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = geometry.band_top().saturating_add(offset);
            if line > geometry.hint {
                // More rows than the band owns. The extra ones are dropped
                // rather than written onto the row below the screen's last,
                // which a terminal would answer by scrolling the document.
                break;
            }
            cup(&mut self.buffer, line, 1);
            self.buffer
                .extend_from_slice(row_text(row, geometry.cols).as_bytes());
        }
        cup(&mut self.buffer, cursor.0, cursor.1.saturating_add(1));
        self.buffer.extend_from_slice(END_FRAME.as_bytes());
        self.buffer.clone()
    }

    /// One frame: the difference between what the terminal is holding and what
    /// it should be holding, in exactly one `write_all` and one flush.
    ///
    /// The screen is a parameter rather than `io::stdout()` for the same reason
    /// `term::shutdown_with`'s is: "the session gives up on a screen that
    /// refuses every frame" is a claim about a screen that can be made to
    /// refuse, and a function that reached for the process's own standard
    /// output could only be tested by breaking it.
    ///
    /// **Nothing is recorded until the write lands.** The shadow, the title the
    /// terminal has been told, and where the caret was left are all claims
    /// about bytes that were delivered; a refused frame leaves every one of
    /// them as it was and is owed again.
    pub(crate) fn commit(
        &mut self,
        out: &mut impl Write,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> io::Result<Commit> {
        // A band that has never painted, or one whose screen has changed size,
        // knows nothing about what is on those rows.
        if self.shadow.rows() != geometry.rows || self.shadow.cols() != geometry.cols {
            self.invalidate(geometry.rows, geometry.cols);
        }

        // The matrix row scenario 13 compares the diff against: the Phase-1
        // painter, on the same facts, on a real terminal. Compile-time only --
        // a released binary contains neither this branch nor the painter.
        #[cfg(feature = "fault-injection")]
        if super::fault::injected(super::fault::Fault::FullPaintReference) {
            let frame = self.render(rows, geometry, cursor);
            out.write_all(&frame)?;
            out.flush()?;
            // The shadow is kept honest even here, so the two builds differ in
            // the bytes they write and in nothing else.
            self.plan(rows, geometry);
            self.landed(geometry, cursor);
            return Ok(Commit::Painted);
        }

        self.plan(rows, geometry);
        let retitled = self.title != self.shown_title;
        let moved = self.caret != Some(cursor);

        self.buffer.clear();
        self.buffer.extend_from_slice(BEGIN_FRAME.as_bytes());
        self.retitle();
        if self.damaged {
            // Exactly the erase the Phase-1 painter opened every frame with,
            // and only on the frame that needs it: the rows a shrinking band
            // gave back, then the band's own rows and everything below them.
            //
            // **From the band's top row and never above it.** What is above is
            // the terminal's own document -- answers the user is still reading
            // -- and a resume that rubbed those out to be sure of its own rows
            // would be a worse defect than the one this fixes.
            self.release(geometry);
            cup(&mut self.buffer, geometry.band_top(), 1);
            self.buffer.extend_from_slice(ERASE_BELOW.as_bytes());
        }
        let touched = self.shadow.diff(&self.target, geometry, &mut self.buffer);
        if touched == 0 && !retitled && !moved && !self.damaged {
            // The screen already holds this frame. It is still *delivered* --
            // whatever a shrinking band gave back was already blank, or the
            // diff would have had something to say about it -- so the exit
            // clears from the band's top row rather than from a row above it
            // that nothing is on.
            self.delivered(geometry);
            return Ok(Commit::NoChange);
        }
        // The top of what this frame is about to write, which on a band that
        // shrank is still the *old* top: recorded before the write, because a
        // frame that fails halfway has still written some of it.
        self.painted = Some(self.top(geometry));
        cup(&mut self.buffer, cursor.0, cursor.1.saturating_add(1));
        self.buffer.extend_from_slice(END_FRAME.as_bytes());
        out.write_all(&self.buffer)?;
        out.flush()?;
        self.landed(geometry, cursor);
        Ok(Commit::Painted)
    }

    /// Builds [`target`](Self::target): what the screen will hold once this
    /// frame lands.
    ///
    /// One Phase-1 frame, applied to the shadow instead of to a terminal -- the
    /// rows a shrinking band gave back erased ([`Self::release`]'s window), and
    /// then the band's own erase and rows ([`Grid::paint_band`]). Which is what
    /// makes the diff an *optimization* rather than a second painter: the grid
    /// it aims at is the one the full painter would have produced.
    fn plan(&mut self, rows: &[String], geometry: &Geometry) {
        self.target.clone_from(&self.shadow);
        if let Some(top) = self.painted {
            for line in top..geometry.band_top() {
                self.target.erase_row(line);
            }
        }
        self.target.paint_band(rows, geometry);
    }

    /// Records that everything this frame built reached the screen.
    fn landed(&mut self, geometry: &Geometry, cursor: (u16, u16)) {
        std::mem::swap(&mut self.shadow, &mut self.target);
        // The erase reached the screen with the rest of the frame, so the band
        // knows what is on those rows again.
        self.damaged = false;
        self.delivered(geometry);
        self.shown_title.clone_from(&self.title);
        self.caret = Some(cursor);
    }

    /// Puts the window title in the frame, when it is not the one the terminal
    /// was last told.
    ///
    /// `OSC 2 ; <title> BEL`. It moves no cell, so it is written at the head of
    /// the frame where it cannot land between a `CUP` and the text that `CUP`
    /// was for.
    fn retitle(&mut self) {
        if self.title == self.shown_title {
            return;
        }
        let Some(title) = &self.title else {
            // A session that has stopped wanting a title does not take the
            // terminal's old one away: the pop of the title stack in every
            // restore sequence (`super::term::RESTORE`) is what gives that
            // back, and it is the only thing that honestly can.
            return;
        };
        self.buffer.extend_from_slice(b"\x1b]2;");
        self.buffer.extend_from_slice(title.as_bytes());
        self.buffer.push(0x07);
    }

    /// Builds the bytes that put completed rows into the terminal's own
    /// document.
    ///
    /// The screen is scrolled with literal newlines from the bottom row
    /// ([`scroll_one`]) and the rows are placed with `CUP` and an erase
    /// ([`place`]); a row's own carriage returns and linefeeds are removed
    /// rather than written, because either would move the cursor out of the row
    /// it was just placed on.
    ///
    /// `scroll` and `rows` are **different numbers**, and that difference is
    /// what makes a streamed answer paintable at all. `rows` is the whole of
    /// the transcript's unfinished line as it now stands; `scroll` is how many
    /// of its rows are new. A delta that only lengthened the last row scrolls
    /// nothing and rewrites one row; a delta that wrapped it scrolls one and
    /// rewrites both. Scrolling by `rows.len()` either way would push a blank
    /// row into the document for every delta of a stream.
    ///
    /// **Every row is painted on the screen before anything scrolls it off**,
    /// and that is why the new rows go out one at a time rather than as one
    /// burst of linefeeds followed by one burst of placements. A terminal's
    /// scrollback is fed by what *leaves the top of the screen*: a row that was
    /// never on the screen was never in the document, and a batch that scrolled
    /// further than the document area is tall would push blank rows into
    /// scrollback and then paint only the surviving suffix -- losing the
    /// beginning of a long answer permanently, since Phase 1 never repaints a
    /// transcript. So: repaint the rows already on the screen where they are,
    /// then, for each new row, scroll by one and place it on the bottom
    /// document row. The cost is one `CUP` and one linefeed per *new* row, and
    /// a streamed answer adds at most one row per delta.
    ///
    /// **Row counts are `usize` here and are never narrowed.** How many rows an
    /// append carries is a property of the text, not of the screen: one 8 MiB
    /// composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a narrow
    /// terminal is well past 65535 rows. A count that saturated at `u16::MAX`
    /// would make `rows.len() - scroll` -- the number this function treats as
    /// *already painted, and therefore already in the document* -- too large by
    /// exactly the amount the count lost, and those rows would never be
    /// painted at all. Same silent, permanent loss as a batch scroll that
    /// outruns the document area, one order of magnitude further out.
    fn render_append(&mut self, scroll: usize, rows: &[String], geometry: &Geometry) -> Vec<u8> {
        self.buffer.clear();
        // The shadow moves with the screen, step for step, and the two are
        // written side by side rather than in two functions: an append that
        // scrolled the terminal and not the grid would leave the band's own
        // rows a row above where the next diff believes they are, for the rest
        // of the session.
        self.target.clone_from(&self.shadow);
        // Before anything scrolls. The rows a shrinking band gave back are at
        // the numbers they were painted at only until the first linefeed of
        // this append moves the whole screen up, and a stale composer row that
        // scrolled into the document is a row nothing will ever repaint.
        self.release(geometry);
        if let Some(top) = self.painted {
            for line in top..geometry.band_top() {
                self.target.erase_row(line);
            }
        }
        if scroll == 0 && rows.is_empty() {
            return self.buffer.clone();
        }
        // Everything above the band's top row is the document; the band's own
        // rows belong to `render`, and nothing here may write at or below it --
        // the activity row included, which is why this is the band's top rather
        // than its divider.
        let area = geometry.band_top().saturating_sub(1);
        // The rows a previous append already put on the screen, and the ones
        // this append adds. `scroll` past the end of `rows` is not something
        // `Transcript` produces -- an append's rows are its whole tail -- but a
        // scroll is still a scroll, so it is honoured as blank rows rather than
        // silently dropped.
        let fresh = scroll.min(rows.len());
        let settled = rows.len() - fresh;
        // The settled rows, repainted where they already are. Only as many of
        // them as the screen still holds: the rest left the top of it, with
        // their text on them, when an earlier append scrolled them there.
        //
        // `shown` is the one count that becomes a `u16`, and it is a *row
        // number* rather than a row total: it is clamped to the document area
        // before the conversion, so the conversion cannot fail, and its
        // fallback is the clamp itself rather than a return that would drop the
        // whole append.
        let shown = u16::try_from(settled.min(usize::from(area))).unwrap_or(area);
        let first = geometry.band_top().saturating_sub(shown);
        // The rows the settled block **vacated**, erased before anything
        // scrolls -- the same window, and for the same reason, as
        // [`Self::release`] above.
        //
        // "Where they already are" is true of `first` only while the band's top
        // has not moved. A band that shrank -- which is every turn that ends,
        // giving back its activity row -- moves `band_top` down, so `first`
        // moves down with it and the block is repainted `vacated` rows lower
        // than it was painted. [`Self::release`] gives back the rows the
        // **band** owned; these are the ones the **document** block left, and
        // nothing else will ever write on them: Phase 1 repaints no transcript
        // row and the exit clears only from the band's top downward. Without
        // this the answer's last row stays on the screen twice -- truncated
        // where the paced release had reached when the turn ended, and complete
        // on the row below it -- and both copies reach native scrollback.
        //
        // Nothing when the band grew or held still (`saturating_sub` is zero),
        // and nothing when there is no settled block to have vacated anything,
        // since those rows are `release`'s and it has already written them.
        let vacated = self
            .painted
            .map_or(0, |top| geometry.band_top().saturating_sub(top));
        if shown > 0 {
            for line in first.saturating_sub(vacated)..first {
                cup(&mut self.buffer, line, 1);
                self.buffer.extend_from_slice(ERASE_LINE.as_bytes());
                self.target.erase_row(line);
            }
        }
        for _ in 0..scroll - fresh {
            scroll_one(&mut self.buffer, geometry);
            self.target.scroll_up(1);
        }
        for (offset, row) in rows[settled - usize::from(shown)..settled]
            .iter()
            .enumerate()
        {
            // A row number too, bounded by `shown` a line above it.
            let offset = u16::try_from(offset).unwrap_or(shown);
            let line = first.saturating_add(offset);
            place(&mut self.buffer, line, row, geometry);
            self.target.place_row(line, row, geometry);
        }
        // Each new row: one scroll, and the row painted on the row the scroll
        // freed -- so it is on the screen, and stays there until a later
        // append carries it off the top and into the terminal's own scrollback.
        for row in &rows[settled..] {
            scroll_one(&mut self.buffer, geometry);
            self.target.scroll_up(1);
            let line = geometry.band_top().saturating_sub(1);
            place(&mut self.buffer, line, row, geometry);
            self.target.place_row(line, row, geometry);
        }
        self.buffer.clone()
    }

    /// Erases the rows a band that shrank no longer owns.
    ///
    /// Nothing when the band grew or stayed where it was: growing paints over
    /// the rows it took, and what the frame itself writes covers every row at
    /// or below the band's top -- the cell diff on an ordinary frame, and
    /// [`ERASE_BELOW`] on the frame that follows external damage. (The link
    /// this sentence used to make, to the whole-band painter, is deliberately
    /// gone: that painter is compiled only into the test and `fault-injection`
    /// builds, so naming it here left a doc link that does not resolve in a
    /// release one.) It is the other direction that leaves something behind --
    /// the composer's old rows, above a divider that has moved down, in a
    /// document area no transcript will repaint.
    ///
    /// One `EL` per row rather than one `ED` from the top: an `ED` would erase
    /// the band's own rows too, and this runs *before* an append's rows are
    /// placed as often as it runs before a frame repaints them.
    ///
    /// It is **not** the whole of what a shrinking band owes. These are the
    /// rows the band gave back; an append that repaints settled transcript rows
    /// anchored to the new top also leaves the rows that block vacated, and
    /// [`Self::render_append`] erases those in the same pre-scroll window.
    ///
    /// **It records nothing.** These bytes are owed until they are delivered,
    /// so a screen that refused this write gets them again on the next one --
    /// which is what [`Self::top`] keeps true, and what makes the failure a
    /// repaint rather than a document permanently holding a dead composer row.
    fn release(&mut self, geometry: &Geometry) {
        let Some(top) = self.painted else {
            return;
        };
        for line in top..geometry.band_top() {
            cup(&mut self.buffer, line, 1);
            self.buffer.extend_from_slice(ERASE_LINE.as_bytes());
        }
    }

    /// The topmost row this band has begun writing on, given where its top row
    /// is now: the higher of the two, because a band that has moved down still
    /// owns the rows above it until the erasures land.
    fn top(&self, geometry: &Geometry) -> u16 {
        self.painted
            .map_or(geometry.band_top(), |top| top.min(geometry.band_top()))
    }

    /// Records that everything the band built reached the screen, so the rows
    /// it gave back are blank and its top really is its top row.
    fn delivered(&mut self, geometry: &Geometry) {
        self.painted = Some(geometry.band_top());
    }

    /// [`render_append`](Self::render_append) plus exactly one write and one
    /// flush, for the same reason [`commit`](Self::commit) is one of each.
    pub(crate) fn append_document(
        &mut self,
        out: &mut impl Write,
        scroll: usize,
        rows: &[String],
        geometry: &Geometry,
    ) -> io::Result<()> {
        let appended = self.render_append(scroll, rows, geometry);
        if appended.is_empty() {
            return Ok(());
        }
        out.write_all(&appended)?;
        out.flush()?;
        // The release rode along at the head of those bytes, so the same rule
        // applies: delivered, and only then is the band's top its divider.
        std::mem::swap(&mut self.shadow, &mut self.target);
        self.delivered(geometry);
        // An append leaves the caret wherever its last `place` put it, which is
        // not where the last frame left it: the next frame owes a `CUP`.
        self.caret = None;
        Ok(())
    }
}

/// Scrolls the screen by one row.
///
/// The cursor goes to the **bottom** row first, because a linefeed scrolls a
/// terminal only from the bottom margin -- from anywhere else it merely walks
/// the cursor down, and the row that was supposed to enter native scrollback
/// would still be on the screen (`frame_scroll_plan.zig:8-12`,
/// `terminal_diff.zig:1348-1397`).
fn scroll_one(buffer: &mut Vec<u8>, geometry: &Geometry) {
    cup(buffer, geometry.rows, 1);
    buffer.push(b'\n');
}

/// Places one row of the document: `CUP`, the clipped text, and `EL`.
///
/// The erase is not tidiness, and it is what makes a document row knowable at
/// all. A scroll brings the band's own rows up into the document area, so the
/// row this text lands on is as likely to hold the divider's rule or the
/// composer's prompt as it is to be blank; and a re-wrap can make a row
/// *shorter* than the one it replaces, when a word moves down. Without the
/// erase, either leaves characters behind on a row nothing will ever repaint --
/// they are the terminal's document now.
fn place(buffer: &mut Vec<u8>, line: u16, row: &str, geometry: &Geometry) {
    cup(buffer, line, 1);
    buffer.extend_from_slice(row_text(row, geometry.cols).as_bytes());
    buffer.extend_from_slice(ERASE_LINE.as_bytes());
}

/// The window title a session asks for: `xfx` and the model a turn would run
/// against, in upstream's separator.
///
/// **The label is made inert here**, and that is the load-bearing half rather
/// than the formatting. A model id is configuration -- a file, an environment
/// variable, a `/model` argument -- so it is a string a user or a provider
/// chose; an `OSC` string ends at a `BEL` or an `ESC \`, so a label carrying
/// either would close the title early and leave the rest of it being *executed*
/// by the terminal. Every control goes, rather than the two that terminate this
/// particular sequence: an allowlist of terminators is a list somebody has to
/// keep correct as sequences are added, and there is no control a window title
/// has a use for.
pub(crate) fn title(model: &str) -> String {
    format!("xfx \u{b7} {}", inert_label(model))
}

/// `label` with nothing in it a terminal would act on.
fn inert_label(label: &str) -> String {
    label
        .chars()
        .filter(|character| !obeyed(*character))
        .collect()
}

/// `CUP`: place the cursor at a one-based row and column.
///
/// Visible to [`super::grid`] because the diff places its runs with the same
/// sequence this does, and two spellings of one instruction is one spelling
/// nothing tests.
pub(crate) fn cup(buffer: &mut Vec<u8>, row: u16, column: u16) {
    // Writing into a `Vec` cannot fail, and there is nothing this function
    // could do about it if it could.
    let _ = write!(buffer, "\x1b[{row};{column}H");
}

/// One row's text: clipped to the screen, carrying nothing the terminal would
/// obey except a colour.
///
/// **This is the render half of the control policy**, and the half that is
/// load-bearing rather than defensive. A row placed here is written to the
/// terminal as it stands, so a `\x1b[2J` in one erases the screen, a
/// `\x1b[?1049h` takes the alternate buffer the TUI promises never to touch,
/// and an OSC retitles the window. The text of a row is a provider's, a tool's
/// or a file's, and `super::bridge::inert` already turns every control in one
/// into a space *at the channel* -- but that is one door, and this is the room:
/// a row assembled from anything that did not come through `UiEvent` would
/// otherwise arrive here unexamined.
///
/// One shape passes: an SGR. This phase's pacer re-opens attributes into the
/// text it emits (`super::pacer::SgrState`), and Task 15's palette will put
/// them into the band's own rows, so a blanket strip here would break the
/// feature the allowlist exists to serve.
///
/// **Dropped rather than turned into a space**, which is where this differs
/// from `bridge::inert` and the difference is arithmetic rather than taste:
/// that function runs *before* the wrap, so a space it leaves is a cell the
/// wrap counts; this runs after, and a cell added here would push the row one
/// column wider than the wrap measured it.
///
/// The **tab** is the one control that is expanded rather than dropped, and it
/// is the same arithmetic read the other way: the wrap measures it at
/// `super::wrap::TAB_WIDTH` cells (item 16, because a paste can put one in the
/// composer), so dropping it here would paint a row one glyph *narrower* than
/// the caret was placed from. The composer hands its rows over already
/// expanded (`super::editor::Editor::rows`); this is the same answer for a row
/// that reached the painter by any other road.
pub(crate) fn row_text(row: &str, cols: u16) -> Cow<'_, str> {
    if row.chars().any(obeyed) {
        return Cow::Owned(clip(&tamed(row), cols).to_string());
    }
    Cow::Borrowed(clip(row, cols))
}

/// Whether a character is one the terminal would act on rather than draw.
///
/// The `ESC` is in the set even though a colour begins with one: [`tamed`] is
/// what tells the two apart, and this is only the question of whether it has to
/// look.
fn obeyed(character: char) -> bool {
    character.is_control()
}

/// `row` with every control sequence removed except the colours.
fn tamed(row: &str) -> String {
    let mut out = String::with_capacity(row.len());
    let mut rest = row;
    while !rest.is_empty() {
        if let Some(len) = super::pacer::colour_at(rest) {
            out.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        // A sequence that is not a colour goes whole, trailing bytes included:
        // leaving the `[2J` of a `\x1b[2J` behind would print `[2J` on the row,
        // and leaving a half-written one behind would have the terminal take
        // the rest of it from the row placed after this one.
        if let Some(len) = super::pacer::escape_at(rest) {
            rest = &rest[len..];
            continue;
        }
        let mut characters = rest.chars();
        let character = characters.next().unwrap_or_default();
        if character == '\t' {
            for _ in 0..super::wrap::TAB_WIDTH {
                out.push(' ');
            }
        } else if !obeyed(character) {
            out.push(character);
        }
        rest = characters.as_str();
    }
    out
}

/// As much of `row` as fits in `cols` cells, cut between grapheme clusters.
///
/// Measured in cells rather than in bytes or in `char`s, because the terminal
/// paints cells: a wide character that straddled the last column would be drawn
/// in a column the layout believes is empty. An escape sequence costs no cells
/// and is stepped over whole, by [`super::pacer::escape_at`] -- the same
/// function `super::wrap::width` measures with, so the row this cuts is cut
/// where the wrap that built it said it ends, and neither can cut inside a
/// sequence.
///
/// Visible to the rest of the TUI because a row that is *built* to a width --
/// the activity row is the first ([`super::activity`]) -- has to be cut by the
/// same function the painter cuts with, or the two disagree about what fits and
/// the shorter answer is the one on the screen.
pub(crate) fn clip(row: &str, cols: u16) -> &str {
    let budget = usize::from(cols);
    let mut used = 0usize;
    let mut end = 0usize;
    while end < row.len() {
        let rest = &row[end..];
        if let Some(len) = super::pacer::escape_at(rest) {
            end += len;
            continue;
        }
        let Some(cluster) = rest.graphemes(true).next() else {
            break;
        };
        let width = usize::from(super::wrap::width(cluster));
        if used + width > budget {
            break;
        }
        used += width;
        end += cluster.len();
    }
    &row[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> Geometry {
        crate::tui::layout::solve(24, 80, 1).expect("a band")
    }

    fn band_rows() -> Vec<String> {
        vec!["--".to_string(), "> ".to_string(), "hint".to_string()]
    }

    /// A terminal, to the extent a document append can move one.
    ///
    /// The exact-byte tests above say what goes on the wire; this says what the
    /// wire *does* to a screen, which is the claim that matters for scrollback
    /// and the one a byte string cannot make: "the row reached the terminal's
    /// document" is a fact about rows that left the top of the screen, not
    /// about escape sequences. It is the same instrument as
    /// `probe::tests::Screen` -- the launch push's model -- with text on the
    /// rows instead of marks, because that is what an append writes; Task 19's
    /// QA emulator is where the two become one.
    ///
    /// Four rules, and it refuses everything else loudly, so an append that
    /// grows a fifth cannot be silently unmodelled:
    ///
    /// * `CUP(row, 1)` places the cursor at the start of a row.
    /// * a linefeed on the bottom row scrolls, and the top row leaves for
    ///   native scrollback; anywhere else it walks the cursor down.
    /// * `EL` erases from the cursor to the end of its row.
    /// * printable text overwrites from the cursor rightwards.
    struct Screen {
        rows: u16,
        divider: u16,
        lines: Vec<String>,
        cursor_row: u16,
        cursor_column: usize,
        scrolled_off: Vec<String>,
    }

    impl Screen {
        /// A screen whose band has been painted and whose document is empty --
        /// the state every append after the first frame really meets.
        ///
        /// The band matters: a scroll carries those rows up into the document
        /// area, so a screen that started blank would let an append that never
        /// erases pass.
        fn under_a_painted_band(geometry: &Geometry) -> Self {
            let mut lines = vec![String::new(); usize::from(geometry.rows)];
            for line in &mut lines[usize::from(geometry.divider) - 1..] {
                *line = "\u{2500}".repeat(usize::from(geometry.cols));
            }
            Self {
                rows: geometry.rows,
                divider: geometry.divider,
                lines,
                cursor_row: 1,
                cursor_column: 0,
                scrolled_off: Vec::new(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut rest = std::str::from_utf8(bytes).expect("an append is text and escapes");
            while !rest.is_empty() {
                if let Some(tail) = rest.strip_prefix('\n') {
                    self.linefeed();
                    rest = tail;
                } else if let Some(tail) = rest.strip_prefix(ERASE_LINE) {
                    self.erase_to_end_of_row();
                    rest = tail;
                } else if let Some((row, tail)) = parse_cup(rest) {
                    self.cursor_row = row.clamp(1, self.rows);
                    self.cursor_column = 0;
                    rest = tail;
                } else if rest.starts_with('\u{1b}') {
                    panic!("the append wrote {rest:?}, which this screen does not model");
                } else {
                    let end = rest.find('\u{1b}').unwrap_or(rest.len());
                    let end = rest[..end].find('\n').unwrap_or(end);
                    let (text, tail) = rest.split_at(end);
                    self.print(text);
                    rest = tail;
                }
            }
        }

        fn print(&mut self, text: &str) {
            assert!(
                self.cursor_row < self.divider,
                "the append wrote {text:?} on row {} -- the band's own rows are \
                 `render`'s, and everything at or below the divider ({}) is one",
                self.cursor_row,
                self.divider
            );
            let line = &mut self.lines[usize::from(self.cursor_row) - 1];
            let mut cells: Vec<char> = line.chars().collect();
            for (offset, character) in text.chars().enumerate() {
                let at = self.cursor_column + offset;
                if at < cells.len() {
                    cells[at] = character;
                } else {
                    cells.push(character);
                }
            }
            *line = cells.into_iter().collect();
            self.cursor_column += text.chars().count();
        }

        fn erase_to_end_of_row(&mut self) {
            let line = &mut self.lines[usize::from(self.cursor_row) - 1];
            *line = line.chars().take(self.cursor_column).collect();
        }

        fn linefeed(&mut self) {
            if self.cursor_row < self.rows {
                self.cursor_row += 1;
                return;
            }
            self.scrolled_off.push(self.lines.remove(0));
            self.lines.push(String::new());
        }

        /// The document rows still on the screen, top first.
        fn visible_document(&self) -> Vec<String> {
            self.lines[..usize::from(self.divider) - 1].to_vec()
        }

        /// Everything the document holds: what has scrolled into native
        /// scrollback, then what is still on the screen -- with the blank rows
        /// above the answer dropped, since a document that has never been
        /// filled starts empty.
        fn document(&self) -> Vec<String> {
            let mut rows = self.scrolled_off.clone();
            rows.extend(self.visible_document());
            let first = rows.iter().position(|row| !row.is_empty()).unwrap_or(0);
            rows.split_off(first)
        }
    }

    /// `CSI <row> ; 1 H`, and nothing else with an escape in it.
    fn parse_cup(text: &str) -> Option<(u16, &str)> {
        let rest = text.strip_prefix("\u{1b}[")?;
        let end = rest.find('H')?;
        let (parameters, tail) = rest.split_at(end);
        let (row, column) = parameters.split_once(';')?;
        assert_eq!(
            column, "1",
            "an append left the cursor off the first column"
        );
        Some((row.parse().ok()?, &tail[1..]))
    }

    #[test]
    fn every_frame_is_wrapped_in_synchronized_output_and_hides_the_cursor() {
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 3));
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(text.starts_with("\u{1b}[?2026h\u{1b}[?25l"), "{text:?}");
        assert!(text.ends_with("\u{1b}[?2026l\u{1b}[?25h"), "{text:?}");
    }

    #[test]
    fn a_frame_positions_every_row_of_the_band_and_clears_to_the_end_of_it() {
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 3));
        let text = String::from_utf8(bytes).expect("utf-8");
        // The band's top row is the divider (22), and the paint clears from
        // there down: autowrap is off, so every row is placed with CUP.
        assert!(text.contains("\u{1b}[22;1H"), "{text:?}");
        assert!(text.contains("\u{1b}[23;1H"), "{text:?}");
        assert!(text.contains("\u{1b}[24;1H"), "{text:?}");
        assert!(
            text.contains("\u{1b}[J"),
            "the band was not cleared: {text:?}"
        );
        // and the cursor ends where the composer says it is
        assert!(text.contains("\u{1b}[23;4H"), "{text:?}");
    }

    #[test]
    fn a_document_append_scrolls_the_screen_so_the_row_enters_native_scrollback() {
        let mut band = Band::new();
        let bytes = band.render_append(1, &["answered".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        // CUP to the last row, then a literal newline: the terminal really
        // scrolls, so the row that leaves the top is in its own scrollback
        // (`frame_scroll_plan.zig:8-12`, `terminal_diff.zig:1348-1397`).
        assert!(text.contains("\u{1b}[24;1H\n"), "{text:?}");
        assert!(text.contains("answered"));
        assert!(
            !text.contains('\r'),
            "CR before LF was not normalized away: {text:?}"
        );
    }

    #[test]
    fn the_frame_is_exactly_these_bytes_in_exactly_this_order() {
        // The three assertions above are each satisfied by a frame with the
        // right pieces in the wrong order. This one is not: it spells the whole
        // frame out, so a paint that cleared *after* it drew, or placed the
        // caret before the rows, fails here.
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 2));
        assert_eq!(
            String::from_utf8(bytes).expect("utf-8"),
            "\u{1b}[?2026h\u{1b}[?25l\
             \u{1b}[22;1H\u{1b}[J\
             \u{1b}[22;1H--\
             \u{1b}[23;1H> \
             \u{1b}[24;1Hhint\
             \u{1b}[23;3H\
             \u{1b}[?2026l\u{1b}[?25h"
        );
    }

    #[test]
    fn the_erase_starts_at_the_band_and_never_above_it() {
        // The document is the terminal's, and `ED` from anywhere above the
        // divider would take rows the session never wrote. Proven by position
        // rather than by presence: the erase must follow the divider's own CUP
        // and nothing else.
        let mut band = Band::new();
        let bytes = band.render(&band_rows(), &geometry(), (23, 2));
        let text = String::from_utf8(bytes).expect("utf-8");
        let erase = text.find(ERASE_BELOW).expect("the erase");
        assert_eq!(
            &text[..erase],
            format!("{BEGIN_FRAME}\u{1b}[22;1H"),
            "the erase was not the first thing written after the divider's CUP"
        );
    }

    #[test]
    fn the_frame_is_erased_and_painted_from_the_row_the_turn_is_using() {
        // While a turn runs the band's top row is the activity row, not the
        // divider. An erase that began at the divider would leave the tail of a
        // longer row behind when a shorter one replaced it -- nothing else ever
        // rewrites that row -- and rows placed from the divider would push the
        // hint row off the bottom of the screen.
        let mut band = Band::new();
        let geometry =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        let rows = vec![
            "\u{2022} Thinking  9s".to_string(),
            "--".to_string(),
            "> ".to_string(),
            "hint".to_string(),
        ];
        let text = String::from_utf8(band.render(&rows, &geometry, (23, 2))).expect("utf-8");
        let erase = text.find(ERASE_BELOW).expect("the erase");
        assert_eq!(
            &text[..erase],
            format!("{BEGIN_FRAME}\u{1b}[21;1H"),
            "the erase did not start at the band's own top row: {text:?}"
        );
        for (line, row) in (21u16..).zip(&rows) {
            assert!(
                text.contains(&format!("\u{1b}[{line};1H{row}")),
                "{row:?} was not painted on row {line}: {text:?}"
            );
        }
    }

    #[test]
    fn an_append_leaves_the_row_the_turn_is_using_alone() {
        // The document is one row shorter while a turn runs, and an append that
        // measured it against the divider would aim its topmost row at the row
        // above the screen -- and paint over the activity row on the way.
        let mut band = Band::new();
        let geometry =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        let rows: Vec<String> = (0..40).map(|row| format!("row {row}")).collect();
        let text = String::from_utf8(band.render_append(0, &rows, &geometry)).expect("utf-8");
        assert!(
            !text.contains("\u{1b}[0;1H"),
            "the append aimed a row at the row above the screen: {text:?}"
        );
        assert!(
            text.contains("\u{1b}[1;1Hrow 20"),
            "the document's first row is not on the screen's first row: {text:?}"
        );
        assert!(
            !text.contains(&format!("\u{1b}[{};1Hrow", geometry.band_top())),
            "the append wrote on the row the turn is using: {text:?}"
        );
    }

    #[test]
    fn a_new_document_row_lands_under_the_row_the_turn_is_using() {
        // The bottom document row is one higher while a turn runs, and an
        // append that placed its new row at the divider less one would paint it
        // straight over the activity row.
        let mut band = Band::new();
        let geometry =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        let text = String::from_utf8(band.render_append(1, &["fresh".to_string()], &geometry))
            .expect("utf-8");
        assert!(
            text.contains("\u{1b}[20;1Hfresh"),
            "the new row is not on the bottom row of the document: {text:?}"
        );
        assert!(
            !text.contains(&format!("\u{1b}[{};1Hfresh", geometry.band_top())),
            "the new row was painted over the activity row: {text:?}"
        );
    }

    #[test]
    fn the_row_a_finished_turn_gave_back_is_erased_rather_than_left_in_the_document() {
        // The band shrinks by a row when the turn ends, and nothing in this
        // phase repaints a document row: a band that recorded its divider as
        // its top would never erase the activity row, and `• Thinking 12s`
        // would stay in the terminal's own document for good -- and be there
        // still after the exit, which clears from the top the band reports.
        let mut band = Band::new();
        let working =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        band.commit(
            &mut Vec::new(),
            &[
                "\u{2022} Thinking  12s".to_string(),
                "--".to_string(),
                "> ".to_string(),
                "hint".to_string(),
            ],
            &working,
            (23, 2),
        )
        .expect("a frame the screen took");
        assert_eq!(band.painted_top(), Some(working.band_top()));

        let idle = geometry();
        let text = String::from_utf8(band.render(&band_rows(), &idle, (23, 2))).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[{};1H{ERASE_LINE}", working.band_top())),
            "the row the turn gave back was left in the document: {text:?}"
        );
    }

    #[test]
    fn an_append_does_not_erase_the_row_the_turn_is_still_using() {
        // The other direction of the same bookkeeping: the band did not shrink,
        // so there is nothing to give back, and an erase aimed at the activity
        // row would blank it until whatever changes it next asks for a frame.
        let mut band = Band::new();
        let geometry =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        band.commit(
            &mut Vec::new(),
            &[
                "\u{2022} Thinking  12s".to_string(),
                "--".to_string(),
                "> ".to_string(),
                "hint".to_string(),
            ],
            &geometry,
            (23, 2),
        )
        .expect("a frame the screen took");

        let text = String::from_utf8(band.render_append(1, &["fresh".to_string()], &geometry))
            .expect("utf-8");
        assert!(
            !text.contains(&format!("\u{1b}[{};1H{ERASE_LINE}", geometry.band_top())),
            "the append erased the row the turn is using: {text:?}"
        );
    }

    #[test]
    fn a_row_wider_than_the_screen_is_cut_at_the_last_column() {
        let mut band = Band::new();
        let geometry = crate::tui::layout::solve(24, 20, 1).expect("a narrow band");
        let bytes = band.render(
            &[
                "-".repeat(40),
                format!("> {}", "x".repeat(40)),
                String::new(),
            ],
            &geometry,
            (23, 2),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[22;1H{}\u{1b}[23;1H", "-".repeat(20))),
            "the divider ran past the last column: {text:?}"
        );
        assert!(
            !text.contains(&"x".repeat(19)),
            "the composer row was not clipped: {text:?}"
        );
    }

    #[test]
    fn a_wide_character_that_would_straddle_the_last_column_is_dropped_whole() {
        // Two cells per glyph and an odd budget: the clip has to leave the last
        // column empty rather than paint half a character into it.
        let mut band = Band::new();
        let geometry = crate::tui::layout::solve(24, 21, 1).expect("a band");
        let bytes = band.render(&["\u{d55c}".repeat(20)], &geometry, (23, 2));
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[22;1H{}\u{1b}[23;", "\u{d55c}".repeat(10))),
            "the clip cut a wide character in half: {text:?}"
        );
    }

    #[test]
    fn a_row_carrying_a_line_break_is_placed_rather_than_allowed_to_move_the_cursor() {
        // A transcript row that still had a CRLF in it would scroll the screen
        // from the middle of a frame, and every row placed after it would land
        // one row too high.
        let mut band = Band::new();
        let bytes = band.render_append(
            2,
            &["first\r\nsecond".to_string(), "third\n".to_string()],
            &geometry(),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!text.contains('\r'), "a carriage return survived: {text:?}");
        assert_eq!(
            text.matches('\n').count(),
            2,
            "the only newlines in an append are the ones that scroll: {text:?}"
        );
        assert_eq!(
            text,
            "\u{1b}[24;1H\n\u{1b}[21;1Hfirstsecond\u{1b}[K\
             \u{1b}[24;1H\n\u{1b}[21;1Hthird\u{1b}[K"
        );
    }

    #[test]
    fn a_colour_costs_no_columns_and_is_never_cut_in_half() {
        // The disagreement Task 7 ledgered and this task had to close before it
        // could put an SGR in a row at all. `unicode_width` gives a lone `ESC`
        // a column of its own, so `clip` measured `\x1b[31m` at five cells
        // while `wrap::width` measured it at four -- only the `ESC` is a
        // control there, and `[31m` is four ordinary printing characters. Two
        // numbers for one row is two different rights: the wrap says the row
        // fits, the clip cuts it, and what it cuts is the middle of a `CSI`.
        // A terminal handed half a sequence takes the rest of it from whatever
        // is written next, which is the band.
        let row = "ab\u{1b}[31mcd";
        assert_eq!(
            usize::from(super::super::wrap::width(row)),
            4,
            "the colour was measured as text"
        );
        assert_eq!(clip(row, 4), row, "the clip cut inside the escape sequence");
        assert_eq!(clip(row, 3), "ab\u{1b}[31mc", "the colour cost a column");
        // and the two agree at every width, which is the property rather than
        // the example
        for cols in 0..=8u16 {
            assert_eq!(
                super::super::wrap::width(clip(row, cols)),
                cols.min(4),
                "the clip and the wrap disagree at {cols} columns"
            );
        }
    }

    #[test]
    fn a_row_may_carry_a_colour_and_nothing_else_the_terminal_obeys() {
        // The render half of the control policy. Everything above the divider
        // is written straight to the terminal, so a row carrying `\x1b[2J`,
        // `\x1b[?1049h` or an OSC title would have the terminal *execute* it.
        // The band's own palette is the one shape allowed to travel
        // (`super::pacer::colour_at`); the rest is dropped rather than turned
        // into a space, because the wrap that placed this row counted it at no
        // cells and a space is one.
        //
        // `\x1b[31m` is in the dropped set and is the interesting member of it:
        // it is a well-formed SGR that no painter here writes, and Task 15
        // narrowed the allowlist from "any attribute" to "the palette's own"
        // for exactly that reason.
        let row = "a\u{1b}[2Jb\u{1b}[?1049hc\u{1b}]0;title\u{7}d\u{1b}[31me\u{1b}[38;5;240mf\u{7}g";
        assert_eq!(row_text(row, 80), "abcde\u{1b}[38;5;240mfg");
    }

    #[test]
    fn a_tab_is_painted_as_the_cells_the_wrap_measured_it_at() {
        // The other half of item 16's tab: the wrap counts a tab at
        // `wrap::TAB_WIDTH` cells because a paste can put one in the composer,
        // so the painter has to write that many. Dropping it -- which is what
        // every other control gets -- would paint the row four columns narrower
        // than the caret was placed from.
        let cells = usize::from(super::super::wrap::TAB_WIDTH);
        let painted = row_text("a\tb", 80);
        assert_eq!(painted, format!("a{}b", " ".repeat(cells)));
        assert_eq!(
            super::super::wrap::width(&painted),
            super::super::wrap::width("a\tb"),
            "the painted row is a different width from the measured one"
        );
        // And it is still clipped in cells: a tab that crosses the margin is
        // cut with the row rather than after it.
        assert_eq!(row_text("a\tb", 3), "a  ");
    }

    /// Every row of `text` wrapped to `cols`, painted as `place` would paint
    /// it, joined back together.
    ///
    /// The composed instrument the case below needs: the wrap decides *where*
    /// a row ends and the paint decides *what* of it reaches the terminal, and
    /// the defect this catches lives in the disagreement between them rather
    /// than in either one.
    fn painted(text: &str, cols: u16) -> String {
        super::super::wrap::wrap(text, cols)
            .into_iter()
            .map(|row| row_text(&text[row.start..row.end], cols).into_owned())
            .collect()
    }

    #[test]
    fn a_sequence_a_row_may_not_keep_is_never_split_across_two_rows() {
        // The two halves of the control policy have to agree about where a
        // sequence *is*, not only about whether it may stay. They did not:
        // the wrap counted a rejected `\x1b[2J` as three printing characters
        // and so was free to break inside it, and the removal knows only how to
        // take a whole sequence out -- so at a narrow width one row ended with
        // half a `CSI` and the next one rendered the printable tail of it,
        // `2J`, as text nobody wrote. Asserted at every width rather than at
        // one, because which width breaks it depends on where the sequence sits.
        for cols in 1..=14u16 {
            assert_eq!(
                painted("ab\u{1b}[2Jcd", cols),
                "abcd",
                "an erase left a fragment on the screen at {cols} columns"
            );
            assert_eq!(
                painted("ab\u{1b}[?1049hcd", cols),
                "abcd",
                "an alternate-buffer switch left a fragment at {cols} columns"
            );
            assert_eq!(
                painted("ab\u{1b}]0;title\u{7}cd", cols),
                "abcd",
                "an OSC title left a fragment at {cols} columns"
            );
            assert_eq!(
                painted("ab\u{1b}[31mcd", cols),
                "abcd",
                "an attribute outside the palette left a fragment at {cols} columns"
            );
            // and the sequence a row *may* keep still arrives whole, at every
            // width, with its text around it
            assert_eq!(
                painted("ab\u{1b}[38;5;240mcd", cols),
                "ab\u{1b}[38;5;240mcd",
                "the colour was cut or dropped at {cols} columns"
            );
        }
    }

    #[test]
    fn an_append_of_nothing_writes_nothing() {
        let mut band = Band::new();
        assert!(band.render_append(0, &[], &geometry()).is_empty());
    }

    #[test]
    fn an_append_that_scrolls_nothing_rewrites_the_row_where_it_already_is() {
        // A delta that only lengthened the last row of an answer. Scrolling
        // here would push a blank row into the document for every few
        // characters the model streams, and the answer would come out
        // double-spaced.
        let mut band = Band::new();
        let bytes = band.render_append(0, &["answered".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        assert_eq!(
            text, "\u{1b}[21;1Hanswered\u{1b}[K",
            "a scroll of no rows still moved the screen"
        );
    }

    #[test]
    fn an_append_writes_the_whole_tail_but_scrolls_only_what_is_new() {
        // The wrapping case: one row of the answer was already on the screen,
        // so it is repainted where it is, and only the row the wrap added
        // costs a scroll.
        let mut band = Band::new();
        let bytes = band.render_append(1, &["abcd".to_string(), "efgh".to_string()], &geometry());
        let text = String::from_utf8(bytes).expect("utf-8");
        assert_eq!(
            text, "\u{1b}[21;1Habcd\u{1b}[K\u{1b}[24;1H\n\u{1b}[21;1Hefgh\u{1b}[K",
            "the append moved the screen by something other than its new rows"
        );
    }

    #[test]
    fn every_appended_row_is_on_the_screen_before_anything_scrolls_it_off() {
        // The rows of an answer taller than the document area. A batch that
        // scrolled `rows.len()` times and then painted only the surviving
        // suffix would push *blank* rows into native scrollback and lose the
        // beginning of the answer for good -- Phase 1 never repaints a
        // transcript, so what is not in scrollback is gone. Asserted against a
        // screen rather than against bytes, because "the row reached the
        // terminal's document" is a claim about what the bytes *did*.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        assert_eq!(geometry.divider, 4, "three document rows");
        let rows: Vec<String> = (1..=5).map(|row| format!("row{row}")).collect();

        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        screen.feed(&band.render_append(5, &rows, &geometry));

        assert_eq!(
            screen.document(),
            rows,
            "a row of the answer never reached the terminal's document"
        );
        assert_eq!(
            screen.visible_document(),
            vec!["row3", "row4", "row5"],
            "the screen does not end on the tail of the answer"
        );
        assert_eq!(
            screen.scrolled_off.len(),
            5,
            "the append moved the screen by something other than the rows it added"
        );
    }

    /// How many times a rendered append scrolled the screen, and how many rows
    /// it placed. Counted from the bytes rather than from the arguments,
    /// because "the row was written" is the claim, and at these sizes the
    /// `Screen` model above is the wrong instrument -- painting a hundred
    /// thousand rows through it proves nothing the counts do not.
    fn scrolls_and_placements(bytes: &[u8], geometry: &Geometry) -> (usize, usize) {
        let text = std::str::from_utf8(bytes).expect("an append is text and escapes");
        let bottom = format!("\u{1b}[{};1H\n", geometry.rows);
        // Every `place` ends in an erase and nothing else emits one, so the
        // erases are the rows -- which is what makes this count exact rather
        // than a guess from `CUP`s that also aim the scroll.
        (
            text.matches(&bottom).count(),
            text.matches(ERASE_LINE).count(),
        )
    }

    #[test]
    fn an_append_with_more_rows_than_a_u16_scrolls_and_places_every_one_of_them() {
        // A row count is a property of the *text*, not of the screen: an 8 MiB
        // composer submission (`editor::MAX_COMPOSER_BYTES`) wrapped on a
        // narrow terminal is well past 65535 rows. A count narrowed to a `u16`
        // anywhere on this path saturates, `rows.len() - scroll` then names
        // more rows as "already painted" than were ever painted, and the
        // difference is dropped -- the beginning of the answer, silently and
        // permanently, exactly as a batch scroll drops it at screen scale.
        //
        // Asserted at the boundary and past it, and by counting rather than by
        // painting: every row of a fresh append is scrolled in and placed.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let boundary = usize::from(u16::MAX);
        for count in [
            boundary - 1,
            boundary,
            boundary + 1,
            boundary + 2,
            boundary * 2 + 3,
        ] {
            let rows = vec!["x".to_string(); count];
            let mut band = Band::new();
            let bytes = band.render_append(count, &rows, &geometry);
            assert_eq!(
                scrolls_and_placements(&bytes, &geometry),
                (count, count),
                "an append of {count} rows lost some of them"
            );
        }
    }

    #[test]
    fn an_append_that_grew_past_a_u16_still_repaints_only_what_the_screen_holds() {
        // The other half of the same count. A tail that has outgrown a `u16`
        // is mostly in scrollback, so the settled rows cost nothing to skip --
        // but only the ones the screen no longer holds may be skipped, and the
        // renderer works that out from `rows.len() - scroll`, which is only
        // right if neither number saturated.
        let geometry = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let count = usize::from(u16::MAX) + 10;
        let rows = vec!["x".to_string(); count];
        let mut band = Band::new();
        let bytes = band.render_append(3, &rows, &geometry);
        // Three scrolls for the three new rows, and one placement per row the
        // screen still holds: the whole document area, plus the three that
        // scrolled in under it.
        let area = usize::from(geometry.divider) - 1;
        assert_eq!(
            scrolls_and_placements(&bytes, &geometry),
            (3, area + 3),
            "the settled rows were repainted by some number other than what \
             the screen holds"
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        // and the settled rows are repainted only as far up as the document
        // area reaches -- row 1, never row 0 and never a negative one.
        assert!(
            text.contains("\u{1b}[1;1H"),
            "the document area's first row"
        );
        assert!(!text.contains("\u{1b}[0;1H"), "a row no terminal has");
    }

    #[test]
    fn a_row_the_scroll_carried_the_band_into_is_erased_before_it_is_written() {
        // A scroll moves the band's own rows up into the document area, so the
        // row an appended line lands on holds the divider's rule. Writing a
        // shorter line over it without an erase leaves the rest of the rule
        // behind -- on a document row nothing will ever repaint.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        screen.feed(&band.render_append(1, &["ok".to_string()], &geometry));
        assert_eq!(
            screen.visible_document(),
            vec!["", "", "ok"],
            "the band's rule survived into the document"
        );
    }

    #[test]
    fn a_streamed_answer_lands_in_the_terminals_document_exactly_once() {
        // The whole path, end to end and against a screen: a transcript fed in
        // chunks the way a provider cuts them, on a screen far too short to
        // hold the answer. Every row must be in the document exactly once, in
        // order -- which is the one property that a lost scroll, a duplicated
        // repaint, and an off-by-one placement each break differently.
        let geometry = crate::tui::layout::solve(6, 20, 1).expect("the smallest band");
        let mut transcript = crate::tui::transcript::Transcript::new(geometry.cols);
        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&geometry);
        for chunk in [
            "the quick brown ",
            "fox jumps over",
            " the lazy dog\r\n",
            "and then it ",
            "rested",
        ] {
            let append = transcript.push(chunk);
            screen.feed(&band.render_append(append.scroll, &append.rows, &geometry));
        }
        assert_eq!(
            screen.document().join("\n"),
            "the quick brown fox \njumps over the lazy \ndog\nand then it rested",
            "the answer did not survive the screen it was streamed onto"
        );
    }

    /// A terminal a whole **frame** is fed to.
    ///
    /// [`Screen`] above models what a document *append* does to a screen, and
    /// refuses anything at or below the divider on purpose -- those rows are
    /// `render`'s rather than an append's. A frame is the other half of the
    /// band's output and has to write exactly there, so it needs a model of its
    /// own. Everything a frame carries and nothing else: `CUP`, `EL`, `ED`,
    /// text, a colour (which moves no cell), and the synchronized-output and
    /// cursor-visibility pair (which move no cell either). Anything else fails
    /// loudly, so a frame that grew a sequence cannot be silently unmodelled.
    ///
    /// One `char` is one cell: every row these cases paint is ASCII or the
    /// divider's `─`, all of them one column wide. The grapheme and width rules
    /// live in [`super::super::grid`] and are tested there.
    struct Terminal {
        rows: u16,
        cols: u16,
        lines: Vec<Vec<char>>,
        row: usize,
        col: usize,
    }

    impl Terminal {
        fn blank(geometry: &Geometry) -> Self {
            Self {
                rows: geometry.rows,
                cols: geometry.cols,
                lines: vec![vec![' '; usize::from(geometry.cols)]; usize::from(geometry.rows)],
                row: 0,
                col: 0,
            }
        }

        /// Text somebody who is not this band put on row `line`.
        ///
        /// The shell, while the session did not own the terminal. It is written
        /// straight onto the model rather than fed as bytes, because that is
        /// what it is: output this band never saw and cannot have recorded.
        fn write_foreign(&mut self, line: u16, text: &str) {
            let row = usize::from(line) - 1;
            for (column, character) in text.chars().enumerate() {
                if column < usize::from(self.cols) {
                    self.lines[row][column] = character;
                }
            }
        }

        fn row_text(&self, line: u16) -> String {
            self.lines[usize::from(line) - 1]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_string()
        }

        fn erase_to_end_of_row(&mut self) {
            for cell in &mut self.lines[self.row][self.col..] {
                *cell = ' ';
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            let mut rest = std::str::from_utf8(bytes).expect("a frame is text and escapes");
            while !rest.is_empty() {
                if let Some(tail) = rest
                    .strip_prefix(BEGIN_FRAME)
                    .or_else(|| rest.strip_prefix(END_FRAME))
                {
                    rest = tail;
                    continue;
                }
                if let Some(tail) = rest.strip_prefix(ERASE_BELOW) {
                    self.erase_to_end_of_row();
                    for line in self.row + 1..self.lines.len() {
                        self.lines[line] = vec![' '; usize::from(self.cols)];
                    }
                    rest = tail;
                    continue;
                }
                if let Some(tail) = rest.strip_prefix(ERASE_LINE) {
                    self.erase_to_end_of_row();
                    rest = tail;
                    continue;
                }
                if let Some((row, column, tail)) = parse_placement(rest) {
                    self.row = usize::from(row.clamp(1, self.rows)) - 1;
                    self.col = usize::from(column.clamp(1, self.cols)) - 1;
                    rest = tail;
                    continue;
                }
                if let Some(len) = crate::tui::pacer::colour_at(rest) {
                    rest = &rest[len..];
                    continue;
                }
                assert!(
                    !rest.starts_with('\u{1b}'),
                    "the frame wrote {rest:?}, which this terminal does not model"
                );
                let end = rest.find('\u{1b}').unwrap_or(rest.len());
                let (text, tail) = rest.split_at(end);
                for character in text.chars() {
                    if self.col < usize::from(self.cols) {
                        self.lines[self.row][self.col] = character;
                        self.col += 1;
                    }
                }
                rest = tail;
            }
        }
    }

    /// `CUP` with both coordinates, which is what a frame writes.
    fn parse_placement(text: &str) -> Option<(u16, u16, &str)> {
        let rest = text.strip_prefix("\u{1b}[")?;
        let end = rest.find('H')?;
        let (parameters, tail) = rest.split_at(end);
        let (row, column) = parameters.split_once(';')?;
        Some((row.parse().ok()?, column.parse().ok()?, &tail[1..]))
    }

    /// A screen that remembers how many times it was written to.
    ///
    /// "One `write_all` and one flush per frame" is the property that makes
    /// what is on the terminal knowable at all -- a second `write` inside one
    /// frame is a window another writer can interleave in -- so it is counted
    /// rather than assumed.
    #[derive(Default)]
    struct Counted {
        writes: usize,
        flushes: usize,
        written: Vec<u8>,
    }

    impl Write for Counted {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    /// A screen that refuses everything, for ever.
    struct Refuses;

    impl Write for Refuses {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
        }
    }

    /// A screen that refuses `refusals` writes and then takes them.
    struct Fussy {
        refusals: usize,
        written: Vec<u8>,
    }

    impl Write for Fussy {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.refusals > 0 {
                self.refusals -= 1;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The bytes a band gives back one row with.
    fn released(line: u16) -> String {
        format!("\u{1b}[{line};1H{ERASE_LINE}")
    }

    /// A tall band's rows, with something on every one of them.
    ///
    /// **Not blank rows.** A frame is a difference from what the terminal
    /// already holds, so a composer of empty strings gives back rows that were
    /// already blank -- there is nothing on them to erase, the erasure is
    /// correctly not written, and a test built on one would be asserting that a
    /// no-op happened.
    fn tall_rows() -> Vec<String> {
        vec!["tall".to_string(); 7]
    }

    #[test]
    fn a_band_that_shrank_erases_the_rows_it_gave_back() {
        // The composer grows and shrinks with the draft, so the divider moves.
        // Moving it *down* hands rows back to a document that nothing repaints:
        // without an erase the old composer rows stay on the screen for the
        // rest of the session, and are still there after the exit.
        let tall = crate::tui::layout::solve(12, 20, 5).expect("a five-row composer");
        let short = crate::tui::layout::solve(12, 20, 1).expect("a one-row composer");
        let mut band = Band::new();
        let mut screen = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        band.commit(&mut screen, &tall_rows(), &tall, (11, 2))
            .expect("the tall band");
        assert_eq!(band.painted_top(), Some(6));

        screen.written.clear();
        band.commit(&mut screen, &band_rows(), &short, (11, 2))
            .expect("the short band");
        let text = String::from_utf8(screen.written).expect("utf-8");
        // Rows 6 to 9, each cleared to its end, and **before** the band's own
        // rows are repainted: the erasures are the first thing in the frame.
        let mut expected = String::from(BEGIN_FRAME);
        for line in short.divider - 4..short.divider {
            expected.push_str(&released(line));
        }
        assert!(
            text.starts_with(&expected),
            "the rows the band gave back were not erased before it repainted: {text:?}"
        );
        assert_eq!(
            band.painted_top(),
            Some(10),
            "the band kept clearing from rows it has given back and blanked"
        );
    }

    #[test]
    fn erasures_the_screen_refused_are_owed_again_rather_than_recorded_as_done() {
        // The rows are given back by *bytes*, and a band that recorded the new
        // top from bytes it never delivered would never erase them again -- and
        // the exit would clear from below them. That is a document permanently
        // holding a dead composer row, which is worse than the frame that was
        // refused. So the top stays where it was until a write lands.
        let tall = crate::tui::layout::solve(12, 20, 5).expect("a five-row composer");
        let short = crate::tui::layout::solve(12, 20, 1).expect("a one-row composer");
        let mut band = Band::new();
        let mut screen = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        band.commit(&mut screen, &tall_rows(), &tall, (11, 2))
            .expect("the tall band");

        screen.refusals = 1;
        screen.written.clear();
        band.commit(&mut screen, &band_rows(), &short, (11, 2))
            .expect_err("the screen refused the frame");
        assert!(screen.written.is_empty());
        assert_eq!(
            band.painted_top(),
            Some(6),
            "the band forgot rows whose erasure never reached the screen, so \
             the exit would clear from below them"
        );

        band.commit(&mut screen, &band_rows(), &short, (11, 2))
            .expect("the next frame");
        let text = String::from_utf8(screen.written).expect("utf-8");
        for line in short.divider - 4..short.divider {
            assert!(
                text.contains(&released(line)),
                "row {line} was never erased on the frame that landed: {text:?}"
            );
        }
        assert_eq!(band.painted_top(), Some(10));
    }

    #[test]
    fn the_rows_a_shrinking_band_gave_back_are_erased_before_an_append_scrolls_them() {
        // Order, not just presence: a submission clears the composer *and*
        // writes what was submitted into the document, and the append's first
        // linefeed moves the whole screen. An erase after it would rub out a
        // row of the answer; no erase at all would scroll a stale composer row
        // into the document, where it stays forever.
        let tall = crate::tui::layout::solve(12, 20, 5).expect("a five-row composer");
        let short = crate::tui::layout::solve(12, 20, 1).expect("a one-row composer");
        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&tall);
        let _painted = band.render(&vec![String::new(); 7], &tall, (11, 2));
        // The band has given the rows back: what is on them is the document's
        // problem now, and this append is the next thing written.
        screen.divider = short.divider;

        screen.feed(&band.render_append(1, &["ok".to_string()], &short));
        assert_eq!(
            screen.visible_document(),
            vec!["", "", "", "", "", "", "", "", "ok"],
            "a row of the old composer survived into the terminal's document"
        );
    }

    #[test]
    fn a_band_that_shrank_does_not_leave_the_row_it_re_placed_behind() {
        // The row every ordinary turn ends on, and the one only a screen can
        // see. When a turn finishes, the band gives its activity row back, so
        // `band_top` moves *down* -- and [`Self::render_append`] anchors the
        // settled rows it repaints to that top, which places the transcript's
        // unfinished line one row lower than it was painted. [`Self::release`]
        // erases `painted .. band_top`, which is the rows the **band** gave
        // back; the row the **document block** vacated is not among them.
        //
        // Without an erase for it the answer's last row is on the screen twice:
        // truncated where the paced release had reached when the turn ended,
        // and complete on the row below it. Phase 1 repaints no transcript row
        // and the exit clears only from the band's top downward, so both copies
        // are permanent and both reach native scrollback.
        // Wide enough for the whole of `WHOLE`: `place` clips a row to the
        // screen, and a terminal too narrow for it would make this a test about
        // clipping with the duplicate hiding inside the ellipsis.
        let running = crate::tui::layout::solve_with(12, 40, 1, true).expect("a turn in flight");
        let ended = crate::tui::layout::solve_with(12, 40, 1, false).expect("the turn over");
        assert_eq!(
            ended.band_top() - running.band_top(),
            1,
            "the activity row is the one row the band gives back here"
        );

        let mut band = Band::new();
        let mut sink = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        let mut screen = Screen::under_a_painted_band(&running);
        // The document area ends at the band's **top**, not at its divider: the
        // activity row is the band's while a turn runs.
        screen.divider = running.band_top();

        // A committed band, so there is a recorded top to give back. Its own
        // bytes are not fed to the screen -- `render`'s frame is a different
        // instrument's subject, and this one models what an *append* does.
        band.commit(&mut sink, &band_rows(), &running, (running.divider + 1, 3))
            .expect("the band while the turn runs");
        assert_eq!(band.painted_top(), Some(running.band_top()));

        // The answer's row as the pacer had released it when the turn ended.
        sink.written.clear();
        band.append_document(&mut sink, 1, &[PARTIAL.to_string()], &running)
            .expect("the partial row");
        screen.feed(&sink.written);

        // The turn ends: the activity row goes, and the same logical row --
        // now complete -- is repainted.
        screen.divider = ended.band_top();
        sink.written.clear();
        band.append_document(&mut sink, 0, &[WHOLE.to_string()], &ended)
            .expect("the completed row");
        screen.feed(&sink.written);

        let document = screen.visible_document();
        let answers: Vec<&String> = document
            .iter()
            .filter(|row| row.starts_with("answer:"))
            .collect();
        assert_eq!(
            answers,
            vec![WHOLE],
            "the band left a stale copy of the answer on the row the document \
             block vacated: {document:?}"
        );
    }

    #[test]
    fn a_band_that_shrank_by_more_than_one_row_erases_all_of_what_it_moved() {
        // The same defect with the range's two ends told apart. A band that
        // gives back **one** row cannot distinguish "erase one row too few at
        // the top" from "erase one row too few at the bottom" from "erase
        // nothing": all three leave the same single stale row. A submission is
        // the ordinary two-row case -- a three-row draft goes, the composer
        // comes back to one -- and it is what makes the boundary a boundary.
        let tall = crate::tui::layout::solve(12, 40, 3).expect("a three-row composer");
        let short = crate::tui::layout::solve(12, 40, 1).expect("a one-row composer");
        assert_eq!(
            short.band_top() - tall.band_top(),
            2,
            "the composer gives back two rows here"
        );

        let mut band = Band::new();
        let mut sink = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        let mut screen = Screen::under_a_painted_band(&tall);
        screen.divider = tall.band_top();
        band.commit(&mut sink, &band_rows(), &tall, (tall.divider + 1, 3))
            .expect("the tall band");

        // Two rows of answer, in the document, under the tall band.
        sink.written.clear();
        band.append_document(
            &mut sink,
            2,
            &[FIRST.to_string(), PARTIAL.to_string()],
            &tall,
        )
        .expect("two rows");
        screen.feed(&sink.written);

        // The composer shrinks by two, so both rows are repainted two rows
        // lower and both of the rows they left need erasing.
        screen.divider = short.band_top();
        sink.written.clear();
        band.append_document(
            &mut sink,
            0,
            &[FIRST.to_string(), WHOLE.to_string()],
            &short,
        )
        .expect("the same two rows, one of them longer");
        screen.feed(&sink.written);

        let document = screen.visible_document();
        let answers: Vec<&String> = document
            .iter()
            .filter(|row| row.starts_with("answer:"))
            .collect();
        assert_eq!(
            answers,
            vec![FIRST, WHOLE],
            "a row the block vacated was left behind: {document:?}"
        );
    }

    /// The row above the one a turn ends on, so the two-row case has a row
    /// whose stale copy is **not** a prefix of its live one.
    const FIRST: &str = "answer: the first row of it";

    /// The answer's last row as a paced release leaves it when a turn ends, and
    /// the whole of it. The first is a **prefix** of the second on purpose:
    /// that is what makes the stale copy hard to see and worth a test, since a
    /// truncated duplicate does not contain the marker a reader would grep for.
    const PARTIAL: &str = "answer: XFXMA";
    const WHOLE: &str = "answer: XFXMARK-COMPLETE";

    #[test]
    fn a_band_that_has_painted_nothing_has_no_row_to_clear_from() {
        let band = Band::new();
        assert_eq!(
            band.painted_top(),
            None,
            "a session that drew nothing would clear a screen it never wrote on"
        );
    }

    #[test]
    fn a_frame_the_screen_refused_still_leaves_a_row_to_clear_from() {
        // The write failed, but not before the terminal saw some of it -- and
        // an exit that cleared from nothing would leave that on the screen.
        let mut band = Band::new();
        band.commit(&mut Refuses, &band_rows(), &geometry(), (23, 2))
            .expect_err("the screen refused the frame");
        assert_eq!(band.painted_top(), Some(22));

        // And from the row the *turn* is using when there is one: the frame
        // begins painting at the band's top row, so a band that reported its
        // divider would have the exit clear from below the row it had already
        // begun writing on.
        let mut band = Band::new();
        let working =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        band.commit(
            &mut Refuses,
            &[
                "\u{2022} Thinking  12s".to_string(),
                "--".to_string(),
                "> ".to_string(),
                "hint".to_string(),
            ],
            &working,
            (23, 2),
        )
        .expect_err("the screen refused the frame");
        assert_eq!(band.painted_top(), Some(working.band_top()));
    }

    #[test]
    fn the_first_frame_paints_the_whole_band_in_one_write() {
        // A band that has painted nothing knows nothing about the screen, so
        // the first frame is the whole of it -- the diff has a blank shadow to
        // work from and every cell is a change. That is what makes the diff an
        // optimization rather than a second painter with a first-run hole in
        // it: there is no "first frame" branch, only a shadow that is empty.
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("commit");
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert_eq!(screen.writes, 1, "a frame is one write: {text:?}");
        assert_eq!(screen.flushes, 1, "and one flush");
        assert!(text.starts_with(BEGIN_FRAME), "{text:?}");
        assert!(text.ends_with(END_FRAME), "{text:?}");
        for (offset, row) in band_rows().iter().enumerate() {
            let line = 22 + u16::try_from(offset).expect("three rows");
            assert!(
                text.contains(&format!("\u{1b}[{line};1H{row}")),
                "row {line} was not painted: {text:?}"
            );
        }
        assert_eq!(band.painted_top(), Some(22));
    }

    #[test]
    fn a_frame_is_the_same_screen_the_phase_one_painter_would_have_left() {
        // The claim the whole optimization rests on, and the one scenario 13
        // makes on a real terminal: the diff is judged by the screen it leaves,
        // not by the bytes it saves. Here it is judged against the reference
        // painter's own model of the band -- both are `Grid`s built through the
        // one tokenizer, so a diff that painted a different screen shows up as
        // a shadow that does not match the target it was aimed at.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Vec::new();
        let drafts = [
            vec!["--".to_string(), "> a".to_string(), "hint".to_string()],
            vec!["--".to_string(), "> ab".to_string(), "hint".to_string()],
            vec!["--".to_string(), "> a".to_string(), "hint".to_string()],
            vec![
                "--".to_string(),
                "> \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}".to_string(),
                "hint".to_string(),
            ],
            vec!["--".to_string(), "> x".to_string(), "hint".to_string()],
        ];
        for rows in &drafts {
            band.commit(&mut screen, rows, &geometry, (23, 3))
                .expect("the frame");
            let mut reference = Grid::blank(geometry.rows, geometry.cols);
            reference.paint_band(rows, &geometry);
            let mut owed = Vec::new();
            assert_eq!(
                band.shadow.diff(&reference, &geometry, &mut owed),
                0,
                "the diff left a screen the full painter would not have: {rows:?} owed {:?}",
                String::from_utf8_lossy(&owed)
            );
        }
    }

    #[test]
    fn an_idle_frame_whose_facts_did_not_change_writes_nothing() {
        // The no-op skip. A band nothing has changed is a whole-band repaint
        // every time something asks for a frame -- an animation tick, a
        // keystroke that was absorbed, a runtime event that produced no text --
        // on a link that may be a serial line.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the first frame");
        assert!(
            !screen.is_empty(),
            "the first frame wrote nothing, so this case proves nothing"
        );

        screen.clear();
        let second = band
            .commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the idle frame");
        assert!(
            matches!(second, Commit::NoChange),
            "an unchanged band reported {second:?}"
        );
        assert!(screen.is_empty(), "an idle frame wrote {screen:?}");
    }

    #[test]
    fn a_caret_that_moved_is_a_frame_even_when_no_cell_did() {
        // The caret is the terminal's own cursor rather than a cell, so a
        // keystroke that only moved it changes nothing on the grid -- and a
        // skip that consulted the cells alone would leave the caret where the
        // last frame put it, on a composer the user is walking through.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 3))
            .expect("the first frame");

        screen.clear();
        let moved = band
            .commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the frame the caret moved on");
        assert!(
            matches!(moved, Commit::Painted),
            "the caret move was skipped"
        );
        assert_eq!(
            String::from_utf8(screen).expect("utf-8"),
            format!("{BEGIN_FRAME}\u{1b}[23;3H{END_FRAME}"),
            "a caret move cost more than a `CUP`"
        );
    }

    #[test]
    fn append_scrolls_the_shadow_before_the_next_diff() {
        // A document append is a linefeed on the bottom row, so it moves the
        // **band's** rows up with everything else. A shadow that did not scroll
        // with the screen would believe the band is still where it painted it,
        // find nothing changed, and leave the band one row above where it
        // belongs for the rest of the session.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("the first frame");
        screen.clear();
        band.append_document(&mut screen, 1, &["answered".to_string()], &geometry)
            .expect("the append");

        screen.clear();
        let after = band
            .commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("the frame after the append");
        assert!(
            matches!(after, Commit::Painted),
            "the band's own facts did not change, but the rows it is painted on did"
        );
        let text = String::from_utf8(screen).expect("utf-8");
        assert!(
            text.contains(&format!("\u{1b}[{};1H--", geometry.divider)),
            "the divider was not put back on the row it belongs on: {text:?}"
        );
        assert!(
            !text.contains("answered"),
            "the frame rewrote a row that is the terminal's document now: {text:?}"
        );
    }

    #[test]
    fn the_frame_after_external_damage_erases_what_the_band_does_not_write() {
        // The band gives the terminal back on a stop and takes it again on the
        // resume, and the **shell owns the screen in between**: whatever it
        // wrote is on the band's own rows now. `Band::invalidate` says the
        // shadow is worthless, and a blank shadow is exactly where this goes
        // wrong -- blank is not "unknown", it is a *claim* that those cells are
        // empty. A diff derived from it writes only the columns the band's own
        // rows fill and finds nothing to erase beyond them, so the shell's text
        // stays on the screen to the right of every band row, for the rest of
        // the session, and rides into scrollback from there.
        //
        // Phase 1 could not have this defect: every frame began with
        // `CUP(band_top,1)` + `ED`. That is the equivalence being restored.
        let geometry = geometry();
        let mut band = Band::new();
        let mut sink = Vec::new();
        band.commit(&mut sink, &band_rows(), &geometry, (23, 2))
            .expect("the first frame");
        let mut terminal = Terminal::blank(&geometry);
        terminal.feed(&sink);

        const FOREIGN: &str = "SHELL-OUTPUT-THE-BAND-NEVER-SAW-AND-CANNOT-HAVE-RECORDED";
        for line in geometry.band_top()..=geometry.rows {
            terminal.write_foreign(line, FOREIGN);
        }
        assert!(
            terminal.row_text(geometry.rows).contains(FOREIGN),
            "the fixture never put foreign text on the band, so this proves nothing"
        );

        band.invalidate(geometry.rows, geometry.cols);
        sink.clear();
        band.commit(&mut sink, &band_rows(), &geometry, (23, 2))
            .expect("the frame after the damage");
        terminal.feed(&sink);

        for (offset, row) in band_rows().iter().enumerate() {
            let line = geometry
                .band_top()
                .saturating_add(u16::try_from(offset).expect("three rows"));
            assert_eq!(
                terminal.row_text(line),
                row.trim_end(),
                "row {line} still holds what the shell left beside the band"
            );
        }
    }

    #[test]
    fn external_damage_never_erases_the_terminals_own_document() {
        // The other half, and the one a `CUP(1,1)` + `ED` would break: the rows
        // **above** the band are the terminal's document. A resume must not
        // rub out the answers the user is still reading, and Phase 1's erase
        // started at the band's top row for exactly this reason.
        let geometry = geometry();
        let mut band = Band::new();
        let mut sink = Vec::new();
        band.commit(&mut sink, &band_rows(), &geometry, (23, 2))
            .expect("the first frame");
        let mut terminal = Terminal::blank(&geometry);
        terminal.feed(&sink);
        const ANSWER: &str = "AN-ANSWER-ALREADY-IN-THE-DOCUMENT";
        terminal.write_foreign(geometry.band_top() - 1, ANSWER);

        band.invalidate(geometry.rows, geometry.cols);
        sink.clear();
        band.commit(&mut sink, &band_rows(), &geometry, (23, 2))
            .expect("the frame after the damage");
        terminal.feed(&sink);

        assert_eq!(
            terminal.row_text(geometry.band_top() - 1),
            ANSWER,
            "the repaint erased a row of the terminal's own document"
        );
    }

    #[test]
    fn a_shadow_the_screen_refused_is_left_as_it_was() {
        // The shadow is a claim about what the terminal is holding, so it may
        // only be advanced by bytes that reached it. A shadow updated from a
        // refused write would believe the new band is on the screen and never
        // paint it again.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the first frame");

        let changed = vec!["==".to_string(), "> typed".to_string(), "hint".to_string()];
        band.commit(&mut Refuses, &changed, &geometry(), (23, 2))
            .expect_err("the screen refused the frame");

        screen.clear();
        band.commit(&mut screen, &changed, &geometry(), (23, 2))
            .expect("the frame that landed");
        let text = String::from_utf8(screen).expect("utf-8");
        assert!(
            text.contains("typed"),
            "the refused frame was recorded as delivered: {text:?}"
        );
        assert!(
            text.contains("=="),
            "the refused frame's other row was recorded as delivered: {text:?}"
        );
    }

    #[test]
    fn a_title_is_xfx_and_the_model_a_turn_would_run_against() {
        assert_eq!(title("zai/glm-5.2"), "xfx \u{b7} zai/glm-5.2");
    }

    #[test]
    fn a_model_label_cannot_close_the_title_it_is_carried_in() {
        // The label is configuration -- a file, an environment variable, a
        // `/model` argument -- so it is a string somebody else chose. An `OSC`
        // ends at a `BEL` or an `ESC \`, so a label carrying either would close
        // the title early and leave the rest of itself being *executed* by the
        // terminal: `\x1b[2J` would erase the screen, `\x1b[?1049h` would take
        // the alternate buffer this TUI promises never to touch.
        assert_eq!(
            title("evil\u{7}\u{1b}[2Jrest"),
            "xfx \u{b7} evil[2Jrest",
            "a terminator survived the label"
        );
        assert_eq!(
            title("evil\u{1b}\\\u{1b}[?1049h"),
            "xfx \u{b7} evil\\[?1049h"
        );
        for label in ["a\u{7}b", "a\u{1b}b", "a\nb", "a\rb", "a\u{0}b"] {
            let made = title(label);
            assert!(
                !made.chars().any(char::is_control),
                "a control survived {label:?}: {made:?}"
            );
        }
    }

    #[test]
    fn the_title_is_written_once_and_not_again_until_it_changes() {
        // It is one `OSC` per *change*, not per frame: a session that
        // re-announced a title the terminal already has would write bytes to
        // say nothing, on every keystroke.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.set_title(title("first/model"));
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the first frame");
        let text = String::from_utf8(screen.clone()).expect("utf-8");
        assert!(
            text.contains("\u{1b}]2;xfx \u{b7} first/model\u{7}"),
            "the title was never set: {text:?}"
        );

        screen.clear();
        let idle = band
            .commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the idle frame");
        assert!(
            matches!(idle, Commit::NoChange),
            "a title that did not change asked for a frame"
        );
    }

    #[test]
    fn a_title_only_change_is_still_one_synchronized_frame() {
        // A `/model` changes the title and no cell, so a skip that consulted
        // the grid alone would leave the window naming the model the session
        // used to be running. It travels inside the frame rather than beside
        // one, because this module is the terminal's only writer.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.set_title(title("first/model"));
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the first frame");

        screen.clear();
        band.set_title(title("second/model"));
        let retitled = band
            .commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the frame the title changed on");
        assert!(
            matches!(retitled, Commit::Painted),
            "the new title was skipped"
        );
        assert_eq!(
            String::from_utf8(screen).expect("utf-8"),
            format!("{BEGIN_FRAME}\u{1b}]2;xfx \u{b7} second/model\u{7}\u{1b}[23;3H{END_FRAME}"),
            "a title-only change cost more than the title and the caret"
        );
    }

    #[test]
    fn a_session_that_asks_for_no_title_writes_no_osc_at_all() {
        // The line-oriented shell shares this module with nothing, but the
        // *band* is also built by tests and by a future surface that may not
        // want one; a painter that wrote an empty title would take the user's
        // own away and put nothing in its place.
        let mut band = Band::new();
        let mut screen = Vec::new();
        band.commit(&mut screen, &band_rows(), &geometry(), (23, 2))
            .expect("the first frame");
        let text = String::from_utf8(screen).expect("utf-8");
        assert!(!text.contains("\u{1b}]"), "an OSC was written: {text:?}");
    }

    #[test]
    fn more_rows_than_the_band_owns_are_dropped_rather_than_scrolling_the_document() {
        // A fourth row on a three-row band would be written to row 25 of a
        // 24-row screen, which a terminal answers by scrolling everything up a
        // row -- taking a row of the user's document with it.
        let mut band = Band::new();
        let bytes = band.render(
            &[
                "--".to_string(),
                "> ".to_string(),
                "hint".to_string(),
                "overflow".to_string(),
            ],
            &geometry(),
            (23, 2),
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(!text.contains("overflow"), "{text:?}");
        assert!(!text.contains("\u{1b}[25;1H"), "{text:?}");
    }

    #[test]
    fn a_screen_that_shrank_leaves_no_top_row_below_its_last_one() {
        // `painted_top` is the row the exit clears from
        // (`super::term::shutdown`), and after a resize it is a row number in
        // the screen that *was*. A session that shrank and then left before its
        // next frame landed would clear from a row below the last one -- which
        // a terminal answers by clamping to its bottom row, so the band's own
        // rows stay on the screen after xfx has exited.
        let tall = crate::tui::layout::solve(40, 20, 1).expect("a band on a tall screen");
        let short = crate::tui::layout::solve(12, 20, 1).expect("a band on a short one");
        let mut band = Band::new();
        let mut screen = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        band.commit(&mut screen, &band_rows(), &tall, (39, 2))
            .expect("the tall band");
        assert_eq!(band.painted_top(), Some(tall.band_top()));

        band.invalidate(short.rows, short.cols);
        assert_eq!(
            band.painted_top(),
            Some(short.rows),
            "the band still claims a row the screen no longer has"
        );
    }

    #[test]
    fn a_screen_that_did_not_shrink_leaves_the_top_row_where_it_was() {
        // The clamp is a bound rather than a reset. Every other caller of
        // `invalidate` -- a `/clear`, a Ctrl-L, a resume -- hands it the screen
        // the band is already on, and a top row moved *down* by one of those
        // would be rows the band painted and now never erases.
        let tall = crate::tui::layout::solve(12, 20, 5).expect("a five-row composer");
        let mut band = Band::new();
        let mut screen = Fussy {
            refusals: 0,
            written: Vec::new(),
        };
        band.commit(&mut screen, &tall_rows(), &tall, (11, 2))
            .expect("the tall band");
        let before = band.painted_top();
        assert_eq!(before, Some(6));
        band.invalidate(tall.rows, tall.cols);
        assert_eq!(band.painted_top(), before);
    }
}
