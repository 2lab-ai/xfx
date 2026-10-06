//! The band writer: one buffer, one counted emit per frame.
//!
//! Everything the TUI puts on the screen goes through here, and that is the
//! point rather than tidiness. The band shares the screen with the terminal's
//! own document, so "what is on those rows is what this module last wrote" is
//! the only thing that makes the band's state knowable at all -- and it stops
//! being true the moment a second writer, or a second vector inside one
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

use super::check;
use super::deliver::{Emit, Sink};
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

/// Erase the whole screen.
///
/// Written by the frame that takes the alternate buffer and by nothing else: a
/// terminal hands out that buffer holding whatever was last on it, and the rows
/// this band is about to place are the only ones it knows about.
const ERASE_SCREEN: &str = "\x1b[2J";

/// Take the terminal's alternate screen buffer, saving the cursor.
///
/// The one sequence the main surface never writes ([`super::term::RESTORE`]
/// carries no leave for exactly that reason). It is written only for a question
/// whose change the band cannot show, and only for as long as that question is
/// up.
const ENTER_ALTERNATE: &str = "\x1b[?1049h";

/// Give it back, restoring the normal buffer and the cursor with it.
const LEAVE_ALTERNATE: &str = "\x1b[?1049l";

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
    /// One row of scratch space [`Grid::row_matches`] renders a candidate
    /// settled row into, reused across a whole append so only this row's own
    /// cell-vector storage is (re)allocated once, not once per row compared
    /// -- narrower than "the check allocates nothing": [`Grid::place_row`]
    /// still allocates a `String` per grapheme cluster it writes into
    /// `scratch`, on every row it renders, matched or not. Disposable:
    /// nothing ever reads it as a claim about the screen, only as the last
    /// thing [`Grid::row_matches`] rendered into it.
    scratch: Grid,
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
    /// The lowest row this band has itself placed a **document** row on, and
    /// has not since scrolled off the top of the screen.
    ///
    /// `None` for a session that has written no document row, and for one whose
    /// screen the band can no longer describe ([`Self::invalidate`]).
    ///
    /// It is what makes [`Self::carry_document`] narrow. The band shares the
    /// screen with the terminal's own document and Phase 1's model is that rows
    /// the band covers are covered -- a composer that wraps grows over whatever
    /// the terminal happens to hold, and that is the accepted trade. What is
    /// **not** acceptable is the band growing over a row *xfx itself wrote and
    /// nothing will ever repaint*: that row is in neither the screen nor the
    /// terminal's scrollback afterwards. This is the only row number that tells
    /// the two apart.
    document_bottom: Option<u16>,
    /// Which of the terminal's two buffers this band's bytes are landing on, as
    /// far as bytes that were **really delivered** can say.
    ///
    /// A record of the wire rather than of an intention, and that is the whole
    /// of what it is for. `super::shell::ScreenOwner` says whose screen the
    /// session *wants* to be composing for; this says which one the terminal is
    /// actually showing, and the two differ for exactly one write -- the one
    /// that changes it. Everything that has to be balanced is balanced against
    /// this: an enter is written only from `Primary`, a leave only from
    /// `Approval`, and the exit asks it rather than the shell
    /// ([`super::term::shutdown`]) because by then the shell may have released a
    /// plane whose bytes are still on the terminal.
    showing: super::shell::ScreenOwner,
    /// What the alternate buffer is holding, while this band is on it.
    ///
    /// The other plane's shadow, and it is a whole-screen one rather than a
    /// [`Grid`] because that plane is repainted whole ([`Self::paint_alternate`]).
    /// It exists for the same reason [`Commit::NoChange`] does: an approval
    /// screen is up for as long as a person takes to read a change, and the
    /// band's animation asks for a frame twice a second the whole time -- so a
    /// repaint that did not check would write a full screen, unchanged, twice a
    /// second, forever, on whatever link the session is on.
    ///
    /// Advanced by [`Self::frame_landed`] out of the frame that landed, and by
    /// nothing else -- for the same reason the shadow is: a surface that was
    /// only built is on no screen.
    alternate: Option<(Vec<String>, (u16, u16))>,
    /// The screen [`Self::painted`] is a row of: the geometry it was recorded
    /// from, kept beside it so the exit cannot measure one against the other.
    painted_on: Option<(u16, u16)>,
    /// A test's hand on the vector, between the moment it is built and the
    /// moment it is checked and written.
    ///
    /// Compiled into test builds only, so no released binary has a way to
    /// reach it. It exists because "a vector whose effect disagrees with what
    /// this band intended is refused **before** the write" is a claim about a
    /// vector that disagrees, and every vector these emitters build agrees by
    /// construction: without a way to damage one on its way to the writer, the
    /// check could only be tested by asserting that correct frames pass, which
    /// is what a check that did nothing would also do.
    #[cfg(test)]
    tamper: Option<fn(&mut Vec<u8>)>,
    /// A test's hand on the **target grid**, between the moment
    /// [`Self::plan`] builds it and the moment the diff and the check read it.
    ///
    /// [`Self::tamper`]'s reason, one layer in. A frame writes bytes for the
    /// rows its diff compares, and the check compares the *whole* screen
    /// against this grid -- so "a target that disagrees with the terminal on a
    /// row no byte of this frame addresses is refused" is a claim about a
    /// target no emitter here can build: `plan` clones the shadow, and a clone
    /// agrees with what it was cloned from everywhere. Without a way to make
    /// one disagree, the only thing that could be asserted is that correct
    /// frames pass, which is what a check of nothing would also do.
    ///
    /// Test builds only, like the hook above it, so no released binary has a
    /// way to reach a grid the band did not plan.
    #[cfg(test)]
    taint: Option<fn(&mut Grid, &Geometry)>,
}

/// One frame, and the plane it belongs to.
///
/// The pair is a type rather than two values because they are one fact: bytes
/// that take, hold or give back a plane are only meaningful together with which
/// plane they leave the terminal on. A caller emits [`Self::bytes`] as one
/// vector and only then records [`Self::owner`], which is the ordering the
/// whole restoration matrix rests on.
pub(crate) struct ScreenFrame {
    owner: super::shell::ScreenOwner,
    bytes: Vec<u8>,
    /// What the alternate plane is holding **once these bytes land** -- `None`
    /// for a frame that gives the plane back, and for a repaint the screen
    /// already holds. Adopted by [`Band::frame_landed`] and by nothing else,
    /// for the reason the shadow is: it is a claim about bytes that were
    /// delivered.
    surface: Option<(Vec<String>, (u16, u16))>,
}

impl ScreenFrame {
    /// Which plane the terminal is on once these bytes have landed.
    pub(crate) fn owner(&self) -> super::shell::ScreenOwner {
        self.owner
    }

    /// The whole frame, to be written in exactly one call.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
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

/// What one recolour of the document's visible cells did.
///
/// Two answers rather than one, because "no bytes went out" is two different
/// facts to the caller holding the debt. [`Self::Settled`] is a decision made:
/// the cells were repainted, or the screen already held them, or this session
/// owns no document row on that screen -- either way nothing is owed any more.
/// [`Self::Deferred`] is no decision at all: the band cannot describe the
/// screen right now, so the debt stands and the ordinary frame is what makes
/// the next tick able to answer it. Collapsing the two either drops a recolour
/// the user is owed or leaves one owed for ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Retint {
    Settled(Commit),
    Deferred,
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
            scratch: Grid::blank(0, 0),
            title: None,
            shown_title: None,
            caret: None,
            // A band that has painted nothing knows nothing, and the first
            // frame is a whole one for the same reason every damaged frame is.
            damaged: true,
            document_bottom: None,
            // The normal buffer, which is the one xfx is launched on and the one
            // it comes back to.
            showing: super::shell::ScreenOwner::Primary,
            alternate: None,
            painted_on: None,
            #[cfg(test)]
            tamper: None,
            #[cfg(test)]
            taint: None,
        }
    }

    /// Damages every vector this band builds from here on, for the tests that
    /// have to see a wrong one refused.
    #[cfg(test)]
    fn tamper_with(&mut self, tamper: fn(&mut Vec<u8>)) {
        self.tamper = Some(tamper);
    }

    /// Damages the grid every frame from here on aims at, for the tests that
    /// have to see the check catch a disagreement no byte of the frame is
    /// near.
    #[cfg(test)]
    fn taint_target_with(&mut self, taint: fn(&mut Grid, &Geometry)) {
        self.taint = Some(taint);
    }

    /// Whether the terminal is showing the alternate buffer this band took.
    ///
    /// Asked by the exit ([`super::term::shutdown`]) and by the loop that owns
    /// the transitions, and answered from bytes that were delivered rather than
    /// from what the session wants.
    pub(crate) fn on_alternate(&self) -> bool {
        matches!(self.showing, super::shell::ScreenOwner::Approval)
    }

    /// Builds the frame that **takes** the alternate buffer: the mode set, an
    /// erase, and the whole surface painted onto it.
    ///
    /// One frame rather than two, for the reason the restore below is one: a
    /// `1049h` on its own hands the user whatever the terminal's other buffer
    /// was holding -- the last `less`, the last `vim` -- until the next tick
    /// gets round to painting something.
    ///
    /// **It records nothing about the primary plane, and that is the load-
    /// bearing half.** [`Self::painted`] is the normal buffer's top row and is
    /// what the exit clears from; [`Self::shadow`] is a claim about the normal
    /// buffer's cells. Neither is true of the screen these bytes land on, and a
    /// frame that updated either would leave the exit erasing from a row number
    /// that means nothing on the plane the user is left looking at -- taking the
    /// shell's own output above the band with it.
    pub(crate) fn enter_alternate(
        &mut self,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> io::Result<ScreenFrame> {
        // The plane this vector is written *from*: the band is on the normal
        // buffer, and the `1049h` inside the vector is what moves it.
        let seed = self.seed(check::PlaneKind::Primary, geometry)?;
        let mut bytes = Vec::with_capacity(ENTER_ALTERNATE.len());
        bytes.extend_from_slice(ENTER_ALTERNATE.as_bytes());
        self.paint_alternate(&mut bytes, rows, geometry, cursor);
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut bytes);
        }
        Self::check_surface(
            &seed,
            &bytes,
            rows,
            geometry,
            cursor,
            check::PlaneMove::Take,
        )?;
        Ok(ScreenFrame {
            owner: super::shell::ScreenOwner::Approval,
            bytes,
            // The buffer the terminal is about to hand out is blank, so what
            // these bytes put on it is the whole of what is on it -- once they
            // land, which is what carrying it in the frame says.
            surface: Some((rows.to_vec(), cursor)),
        })
    }

    /// The same surface again, on a plane this band is already on.
    ///
    /// What a resize asks for, and what a marker that moved asks for. No
    /// `1049h`: the plane is taken, and taking it twice is a save of the normal
    /// buffer over the save that is holding the user's own screen.
    ///
    /// **Empty bytes when the screen already holds this**, which is
    /// [`Commit::NoChange`] on the other plane and is not an optimization: the
    /// band asks for a frame twice a second while a turn is running
    /// (`super::render_request`), and a person reading a change takes minutes,
    /// so a repaint that did not check would write a full unchanged screen for
    /// as long as they read it.
    pub(crate) fn repaint_alternate(
        &mut self,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> io::Result<ScreenFrame> {
        // The plane the band is already on, holding what its own cache says is
        // up: the skip below writes no bytes, and a vector of none is checked
        // against that surface rather than against a blank one.
        let seed = match &self.alternate {
            Some((held, cursor)) => self.seed(check::PlaneKind::Alternate, geometry)?.holding(
                &Self::surface(held, geometry),
                (cursor.0, cursor.1.saturating_add(1)),
            )?,
            None => self.seed(check::PlaneKind::Alternate, geometry)?,
        };
        let mut bytes = Vec::new();
        let mut surface = None;
        if self.alternate.as_ref() != Some(&(rows.to_vec(), cursor)) {
            self.paint_alternate(&mut bytes, rows, geometry, cursor);
            surface = Some((rows.to_vec(), cursor));
        }
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut bytes);
        }
        // The unchanged repaint is checked too, and against the same intent:
        // "the screen already holds this" is a claim, and a vector of no bytes
        // makes it good only if the model already holds the surface.
        Self::check_surface(
            &seed,
            &bytes,
            rows,
            geometry,
            cursor,
            check::PlaneMove::Stay,
        )?;
        Ok(ScreenFrame {
            owner: super::shell::ScreenOwner::Approval,
            bytes,
            // Nothing for the frame that wrote nothing: an empty frame that
            // landed would otherwise re-assert a cache the band may have
            // dropped in between ([`Self::invalidate`]).
            surface,
        })
    }

    /// What an alternate-plane frame says the screen will hold: the rows it was
    /// handed, on a blank plane, placed exactly where [`Self::paint_alternate`]
    /// puts them.
    ///
    /// Built from the same rows the vector is built from -- never from the
    /// vector -- so the two can disagree.
    fn surface(rows: &[String], geometry: &Geometry) -> Grid {
        let mut grid = Grid::blank(geometry.rows, geometry.cols);
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = offset.saturating_add(1);
            if line > geometry.rows {
                break;
            }
            grid.place_row(line, row, geometry);
        }
        grid
    }

    /// One alternate-plane vector against the surface it says it paints.
    fn check_surface(
        seed: &check::TerminalModel,
        bytes: &[u8],
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
        moves: check::PlaneMove,
    ) -> io::Result<()> {
        let surface = Self::surface(rows, geometry);
        check::preflight(
            seed,
            bytes,
            &check::Declared::new(
                check::Intent::Alternate {
                    grid: &surface,
                    caret: (cursor.0, cursor.1.saturating_add(1)),
                    // A repaint the screen already holds writes no bytes, and a
                    // vector of none says nothing about the cursor.
                    cursor_visible: if bytes.is_empty() { None } else { Some(true) },
                },
                // The whole of the borrowed plane, and **no cell of the normal
                // one** -- which is a claim about the buffer an effect landed
                // on rather than about its row number, because the two planes
                // share one row-number space. The transition is declared too:
                // a vector that took the plane twice would have saved a screen
                // of its own over the user's.
                check::Footprint::new(
                    check::PlaneKind::Alternate,
                    vec![
                        check::Seg::Erase(1..=geometry.rows),
                        check::Seg::Place(1..=geometry.rows),
                    ],
                )
                .moving(moves),
            ),
        )?;
        Ok(())
    }

    /// One whole-screen paint of the other plane.
    ///
    /// Whole rather than a difference, and it is not an optimization left
    /// undone: the diff exists because the band shares its rows with a document
    /// nothing else repaints, and this surface shares its screen with nothing at
    /// all. A shadow of it would be a second grid to keep honest across a plane
    /// change that a terminal, not this process, performs.
    ///
    /// Its own buffer rather than [`Self::buffer`], so that a frame on this
    /// plane cannot leave the primary plane's reusable buffer holding rows that
    /// belong to the other one.
    fn paint_alternate(
        &self,
        bytes: &mut Vec<u8>,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) {
        bytes.extend_from_slice(BEGIN_FRAME.as_bytes());
        cup(bytes, 1, 1);
        bytes.extend_from_slice(ERASE_SCREEN.as_bytes());
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = offset.saturating_add(1);
            if line > geometry.rows {
                // More rows than the screen has. Dropped rather than written
                // onto the row below the last, which a terminal answers by
                // scrolling -- on a buffer whose top row is gone for good.
                break;
            }
            cup(bytes, line, 1);
            bytes.extend_from_slice(row_text(row, geometry.cols).as_bytes());
        }
        cup(bytes, cursor.0, cursor.1.saturating_add(1));
        bytes.extend_from_slice(END_FRAME.as_bytes());
    }

    /// Builds the one frame that gives the primary plane back: the leave, the
    /// hidden cursor, and the **whole** band repainted, in one vector.
    ///
    /// Three things and one write, because each of the three alone is a state a
    /// terminal can be caught in:
    ///
    /// * `1049l` alone shows the user the buffer the terminal saved, with the
    ///   band as it was before the question -- stale by however long the
    ///   question was up.
    /// * a repaint alone paints the band onto the plane being left.
    /// * two writes are two presentations on any terminal without synchronized
    ///   output, and the one in between is the blank grid this ordering exists
    ///   to make unreachable.
    ///
    /// **Whole rather than a difference.** The terminal has been showing another
    /// buffer, and what it restores is its own saved copy of this one; the
    /// shadow describes the cells this band last wrote there, which is a claim
    /// about a screen a `1049` pair has since saved and restored. So the band's
    /// own rows are all written, and everything above them is the terminal's to
    /// give back -- which is what a `1049` pair is *for*, and why the document
    /// is not repainted here.
    ///
    /// Nothing is recorded: these bytes are owed until they are delivered, and
    /// [`Self::frame_landed`] is what the caller calls once they are.
    pub(crate) fn restore_primary(
        &mut self,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> io::Result<ScreenFrame> {
        // **Before `plan`, before `shown_title` is cleared and before `painted`
        // moves**: all three are mutations this vector makes while building
        // itself, and a model seeded after them would be measured against the
        // state the vector had already moved.
        let seed = self.seed(check::PlaneKind::Alternate, geometry)?;
        let released = self.top(geometry);
        // The band's top as this vector began, which is what `release` below
        // erases from and what `painted` stops being a few lines later.
        let painted_before = self.painted;
        // What the terminal is showing on the title bar as this vector begins,
        // which is what it still shows if this band wants no title of its own:
        // `shown_title` below is cleared to force a re-emit, and clearing a
        // record takes nothing off a window.
        let shown = self.shown_title.clone();
        // What the screen will hold once this lands: the shadow it held before
        // the excursion -- which is what the terminal restores with the plane --
        // with this band painted over it. Built before the bytes, and adopted
        // only by `frame_landed`, for the reason `commit` builds it there.
        self.plan(rows, geometry);
        self.buffer.clear();
        self.buffer.extend_from_slice(LEAVE_ALTERNATE.as_bytes());
        self.buffer.extend_from_slice(BEGIN_FRAME.as_bytes());
        // The window title, which the terminal may have given back with the
        // plane. Asked for again on the frame that is a whole repaint anyway.
        self.shown_title = None;
        self.retitle();
        // The rows a band that shrank while the question was up no longer owns,
        // then its own rows and everything below them.
        self.release(geometry);
        cup(&mut self.buffer, geometry.band_top(), 1);
        self.buffer.extend_from_slice(ERASE_BELOW.as_bytes());
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = geometry.band_top().saturating_add(offset);
            if line > geometry.hint {
                break;
            }
            cup(&mut self.buffer, line, 1);
            self.buffer
                .extend_from_slice(row_text(row, geometry.cols).as_bytes());
        }
        cup(&mut self.buffer, cursor.0, cursor.1.saturating_add(1));
        self.buffer.extend_from_slice(END_FRAME.as_bytes());
        // The top of what this frame is about to write, recorded before the
        // write for the reason `render` records it there: a frame that failed
        // halfway has still written some of it.
        self.record_painted(self.top(geometry), geometry);
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut self.buffer);
        }
        // What this frame says it leaves on the rows it writes: the window a
        // shrinking band gave back and everything from the band's top row down,
        // blanked, with the band's own rows placed from the text this function
        // was handed. Rows rather than a screen, and for the same reason the
        // document emitters declare rows: the target this frame planned is the
        // size of a shadow that may be older than the screen the question was
        // answered on. Everything above the band is the terminal's own to give
        // back with the plane, and the preservation sweep holds it to the seed.
        let mut script = check::Script::new(geometry);
        if let Some(top) = painted_before {
            for line in top..geometry.band_top() {
                script.erase(line);
            }
        }
        for line in geometry.band_top()..=geometry.rows {
            script.erase(line);
        }
        for (offset, row) in rows.iter().enumerate() {
            let Ok(offset) = u16::try_from(offset) else {
                break;
            };
            let line = geometry.band_top().saturating_add(offset);
            if line > geometry.hint {
                break;
            }
            script.place(line, row);
        }
        // The `1049l` at its head gives the plane back, so this vector is
        // decoded from the borrowed plane and must end on the normal one --
        // by exactly that one declared transition, holding the band it repaints
        // whole over the screen the terminal restores with the buffer.
        check::preflight(
            &seed,
            &self.buffer,
            &check::Declared::new(
                check::Intent::Document {
                    script: &script,
                    caret: Some((cursor.0, cursor.1.saturating_add(1))),
                    cursor_visible: Some(true),
                    title: self.title.as_deref().or(shown.as_deref()),
                },
                self.band_footprint(released, geometry)
                    .moving(check::PlaneMove::Give),
            ),
        )?;
        Ok(ScreenFrame {
            owner: super::shell::ScreenOwner::Primary,
            bytes: self.buffer.clone(),
            // The plane is being given back, so there is no surface on it to
            // claim: `frame_landed` clears the cache on this branch instead.
            surface: None,
        })
    }

    /// Records that a [`ScreenFrame`] reached the terminal.
    ///
    /// One recorder for all three of them, and it reads the plane out of the
    /// **frame** rather than out of the branch that chose it: which buffer the
    /// terminal is on is a fact about bytes that were delivered, so it is taken
    /// from the thing that was delivered.
    ///
    /// A restore is additionally a *whole* repaint of the band, so once it has
    /// landed the band knows exactly what is on its own rows again and the next
    /// ordinary frame is a difference from it. The two alternate frames record
    /// the plane they painted ([`ScreenFrame::surface`]) and nothing else:
    /// nothing they wrote is on the normal buffer.
    pub(crate) fn frame_landed(
        &mut self,
        frame: &ScreenFrame,
        geometry: &Geometry,
        cursor: (u16, u16),
    ) {
        self.showing = frame.owner();
        if matches!(frame.owner(), super::shell::ScreenOwner::Primary) {
            // The terminal takes its alternate buffer back when the plane is
            // given up, so what this band believed was on it is a claim about a
            // screen that no longer exists.
            self.alternate = None;
            self.landed(geometry, cursor);
        } else if let Some(surface) = &frame.surface {
            self.alternate = Some(surface.clone());
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
        // The screen that row is a row of is this one now: `invalidate` is the
        // one place the band accepts a new size for its own record.
        self.painted_on = self.painted.map(|_| (rows, cols));
        // A row number on a screen that has been re-wrapped, erased or handed
        // back and taken again is not a row number any more.
        self.document_bottom = None;
        // Nor is a claim about what the *other* plane is holding. The skip in
        // [`Self::repaint_alternate`] is an equality against exactly this, so a
        // cache that outlived the damage would answer "the terminal already
        // holds this" about a screen this band has just said it cannot
        // describe -- and would suppress the repaint the damage asked for.
        self.alternate = None;
        self.caret = None;
        self.damaged = true;
        // What the terminal was told, rather than what this band wants: see the
        // title paragraph above.
        self.shown_title = None;
    }

    /// Forces the next [`commit`](Self::commit) to be a real write, and
    /// nothing else [`invalidate`](Self::invalidate) also resets.
    ///
    /// The one caller ([`super::event_loop::commit_band`]) is a tick right
    /// behind a recovered tear: [`recover_primary`](Self::recover_primary)
    /// already rebuilt this band correctly, in the same call the tear
    /// happened in, so `document_bottom`, the alternate cache, `caret` and
    /// `shown_title` are exactly right and `invalidate` would only be
    /// throwing away work recovery already did. What that tick still owes is
    /// proof the screen is healthy again, and a diff against an
    /// already-correct shadow cannot give it one -- `commit`'s own fast path
    /// (guarded by `!self.damaged`) would call an identical frame
    /// [`Commit::NoChange`] and write nothing, which settles nothing about
    /// whether the terminal is still there.
    ///
    /// Raising `damaged` alone is not enough, and that is what the row loop
    /// below is for. [`Band::commit`]'s erase preamble (the `if self.damaged`
    /// branch just before [`Grid::diff`](super::grid::Grid::diff)) is gated on
    /// `damaged`, but the *repaint* bytes that are supposed to follow it come
    /// from `diff` alone, which is a pure content comparison against
    /// `shadow` and knows nothing about `damaged`. A same-shape tick right
    /// behind a recovery has a `shadow` that is already byte-identical to
    /// what this tick's `target` will plan to -- so with `damaged` set and
    /// nothing else changed, `diff` would find zero touched cells and the
    /// emitted vector would erase the band's rows and never repaint them, a
    /// frame [`check::preflight`](super::check::preflight) rightly refuses.
    /// [`recover_primary`](Self::recover_primary) avoids exactly this by
    /// blanking its own rows in `shadow` before its own `commit` call; this
    /// does the same, on the same rows, so this tick's `diff` has the real
    /// content to write that the erase preamble promises. Only the band's
    /// own rows: `document_bottom`, the alternate cache, `caret` and
    /// `shown_title` are left exactly as recovery set them, which is what
    /// keeps this narrower than [`invalidate`](Self::invalidate).
    pub(crate) fn force_redraw(&mut self, geometry: &Geometry) {
        let top = self.top(geometry);
        for line in top..=geometry.rows {
            self.shadow.erase_row(line);
        }
        self.damaged = true;
    }

    /// Forces the next [`repaint_alternate`](Self::repaint_alternate) to be a
    /// real, whole write: [`force_redraw`](Self::force_redraw)'s counterpart on
    /// the borrowed plane, for the tick right behind a recovered tear there
    /// ([`recover_alternate`](Self::recover_alternate)).
    ///
    /// The cache is the one thing that can make a repaint write nothing, and
    /// right behind a recovery it is exactly right -- the rebuild landed and
    /// was adopted -- so an unforced tick would find "the screen already holds
    /// this" and write no byte. That is the one frame the tick cannot settle
    /// for: an empty repaint proves nothing about a screen that just tore, and
    /// it may keep an approval receipt but never mint one, so a question whose
    /// receipt the tear took would stay unanswerable for as long as nothing on
    /// it moved. Dropping the cache asks for no more than that: the plane, the
    /// title and every record of the normal buffer are left as they are, which
    /// is what keeps this narrower than [`invalidate`](Self::invalidate).
    pub(crate) fn force_alternate_repaint(&mut self) {
        self.alternate = None;
    }

    /// The band's top row, if this band has painted one.
    pub(crate) fn painted_top(&self) -> Option<u16> {
        self.painted
    }

    /// The screen [`Self::painted_top`] is a row of.
    ///
    /// **The geometry that row was recorded from**, kept beside it rather than
    /// derived from the shadow, because the two move on different clocks: an
    /// append records `painted` from the geometry it was handed, while the
    /// shadow is resized only by the frame path -- so after a screen grows,
    /// a document write leaves a `painted` the shadow's size cannot describe.
    /// The exit reads both, and reading them from one place is what stops it
    /// measuring a row of one screen against the size of another.
    ///
    /// `0x0` for a band that has painted nothing, which is the same session
    /// whose top row is `None` and whose exit therefore erases nothing.
    pub(crate) fn screen_size(&self) -> (u16, u16) {
        self.painted_on
            .unwrap_or((self.shadow.rows(), self.shadow.cols()))
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
        self.record_painted(self.top(geometry), geometry);
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
    /// it should be holding, in exactly one emit.
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
        out: &mut impl Sink,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> Result<Commit, Emit> {
        // A band that has never painted, or one whose screen has changed size,
        // knows nothing about what is on those rows.
        if self.shadow.rows() != geometry.rows || self.shadow.cols() != geometry.cols {
            self.invalidate(geometry.rows, geometry.cols);
        }

        // **Before `plan`, and before the `painted` below moves.** Both are
        // mutations this frame makes while building itself, so a model seeded
        // afterwards would be compared against state the vector it is checking
        // had already moved -- and would agree with it for that reason rather
        // than on the merits.
        let seed = self
            .seed(check::PlaneKind::Primary, geometry)
            .map_err(Emit::rejected)?;
        let released = self.top(geometry);

        // The matrix row scenario 13 compares the diff against: the Phase-1
        // painter, on the same facts, on a real terminal. Compile-time only --
        // a released binary contains neither this branch nor the painter --
        // and checked like any other vector, because the reference a scenario
        // compares against has to be one this band would stand behind.
        #[cfg(feature = "fault-injection")]
        if super::fault::injected(super::fault::Fault::FullPaintReference) {
            let frame = self.render(rows, geometry, cursor);
            // The shadow is kept honest even here, so the two builds differ in
            // the bytes they write and in nothing else.
            self.plan(rows, geometry);
            check::preflight(
                &seed,
                &frame,
                &check::Declared::new(
                    check::Intent::Primary {
                        grid: &self.target,
                        caret: Some((cursor.0, cursor.1.saturating_add(1))),
                        cursor_visible: Some(true),
                        title: self.title.as_deref().or(self.shown_title.as_deref()),
                    },
                    self.band_footprint(released, geometry),
                ),
            )
            .map_err(Emit::rejected)?;
            out.emit(&frame)?;
            self.landed(geometry, cursor);
            return Ok(Commit::Painted);
        }

        self.plan(rows, geometry);
        #[cfg(test)]
        if let Some(taint) = self.taint {
            taint(&mut self.target, geometry);
        }
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
        // The rows above `released` are the shadow's own cells, cloned: `plan`
        // copies the shadow and then touches only the rows the band gave back
        // and the band's own ([`Grid::paint_band`]), and `released` is the
        // higher of those two tops -- captured above, before `plan` ran and
        // before `painted` moved. So comparing them can neither find a
        // difference nor emit a byte, and the diff is told to begin there
        // ([`Grid::diff_from`]).
        //
        // The clone is the whole argument. The conditions below only decide
        // whether it is being relied on, and they are two different kinds:
        //
        // * `damaged` is **reached**, on every tick behind a `/clear`, a
        //   Ctrl-L, a resize or a [`Self::force_redraw`]. Such a frame is a
        //   whole repaint, so it takes the full screen -- conservative rather
        //   than necessary, since `plan` still touches nothing above
        //   `released`.
        // * The size and plane checks are redundant today: the invalidate above
        //   makes the sizes agree and `commit` is the primary plane's emitter.
        //   They are kept so that the day either stops holding, the diff falls
        //   back instead of skipping rows whose equality nothing proves.
        let first_row = if !self.damaged
            && matches!(self.showing, super::shell::ScreenOwner::Primary)
            && self.shadow.rows() == geometry.rows
            && self.shadow.cols() == geometry.cols
        {
            released
        } else {
            1
        };
        let touched = self
            .shadow
            .diff_from(first_row, &self.target, geometry, &mut self.buffer);
        if touched == 0 && !retitled && !moved && !self.damaged {
            // **The skip is checked too, against no bytes at all.** "Nothing
            // needs to be written" is the claim that the screen already holds
            // this frame, and a vector of zero bytes satisfies it only if the
            // model seeded from the shadow already equals the target. It is
            // screen identity, not request identity: two different requests
            // whose bands are byte-identical both pass, and neither is an
            // approval-readiness proof.
            check::preflight(
                &seed,
                &[],
                &check::Declared::new(
                    check::Intent::Primary {
                        grid: &self.target,
                        caret: Some((cursor.0, cursor.1.saturating_add(1))),
                        // Zero bytes say nothing about the cursor, and this
                        // declares that rather than leaving a check out: the
                        // cells, the caret and the title are all still
                        // compared, which is the whole of what "the screen
                        // already holds this" claims.
                        cursor_visible: None,
                        title: self.title.as_deref().or(self.shown_title.as_deref()),
                    },
                    check::Footprint::none(check::PlaneKind::Primary),
                ),
            )
            .map_err(Emit::rejected)?;
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
        self.record_painted(self.top(geometry), geometry);
        cup(&mut self.buffer, cursor.0, cursor.1.saturating_add(1));
        self.buffer.extend_from_slice(END_FRAME.as_bytes());
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut self.buffer);
        }
        // What these bytes do to a terminal, decoded, against what this frame
        // says it wants -- and the same slice is what goes out below. A
        // disagreement is returned as an error and counted by the caller's
        // existing budget, exactly as a refused write is.
        check::preflight(
            &seed,
            &self.buffer,
            &check::Declared::new(
                check::Intent::Primary {
                    grid: &self.target,
                    caret: Some((cursor.0, cursor.1.saturating_add(1))),
                    cursor_visible: Some(true),
                    title: self.title.as_deref().or(self.shown_title.as_deref()),
                },
                self.band_footprint(released, geometry),
            ),
        )
        .map_err(Emit::rejected)?;
        out.emit(&self.buffer)?;
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
    fn render_append<'rows>(
        &mut self,
        scroll: usize,
        rows: &'rows [String],
        geometry: &'rows Geometry,
    ) -> (Vec<u8>, check::Footprint, check::Script<'rows>) {
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
            return (
                self.buffer.clone(),
                check::Footprint::none(check::PlaneKind::Primary),
                check::Script::new(geometry),
            );
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
        // A settled row is reused -- neither emitted nor re-placed onto
        // `target` -- only when every fact this frame carries about the
        // screen says the row is already sitting where it is about to be
        // placed again: undamaged, the same terminal size and the same
        // band top as the frame that landed last, and no extra blank scroll
        // ahead of it (`scroll != fresh` means `Transcript` handed back more
        // scroll than rows, which is not a screen this reasoning covers).
        // Row-by-row it still costs a real rendered-cell comparison
        // ([`Grid::row_matches`]) rather than trusting the frame-level facts
        // alone -- those rule out *why* a row could be stale, not whether
        // this particular one is.
        let reuse_eligible = !self.damaged
            && self.painted_on == Some((geometry.rows, geometry.cols))
            && self.painted == Some(geometry.band_top())
            && scroll == fresh;
        for (offset, row) in rows[settled - usize::from(shown)..settled]
            .iter()
            .enumerate()
        {
            // A row number too, bounded by `shown` a line above it.
            let offset = u16::try_from(offset).unwrap_or(shown);
            let line = first.saturating_add(offset);
            let reused = reuse_eligible
                && self
                    .target
                    .row_matches(line, row, geometry, &mut self.scratch);
            if !reused {
                place(&mut self.buffer, line, row, geometry);
                self.target.place_row(line, row, geometry);
            }
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
        // Every row these bytes may reach, and every scroll they may make,
        // named from the counts above rather than from what was emitted.
        //
        // The erase window is the widest of the three that ride in front of the
        // scrolls -- the rows a shrinking band gave back, and the rows the
        // settled block vacated -- and it stops one row above the band, because
        // an append may never write at or below the band's top row. The
        // placements are the settled block and the new rows the scrolls free,
        // all anchored at `document_bottom = band_top - 1`.
        let bottom = geometry.band_top().saturating_sub(1);
        let released = self.painted.unwrap_or(geometry.band_top());
        let erase_top = first.saturating_sub(vacated).min(released);
        let footprint = check::Footprint::new(
            check::PlaneKind::Primary,
            vec![
                check::Seg::Erase(erase_top..=bottom),
                check::Seg::Place(first.min(bottom)..=bottom),
                check::Seg::Scroll {
                    rows: u32::try_from(scroll).unwrap_or(u32::MAX),
                },
            ],
        );
        // And the ordered edits themselves, in the coordinates they are made at
        // rather than the ones they end on -- because the rows this append
        // delivers past the height of the document area **have** no final
        // coordinate. Each is painted on the bottom document row and carried off
        // the top by the scrolls behind it, into the terminal's own scrollback,
        // which this phase never repaints and cannot take back. A declaration
        // that only described the last screen would leave every one of them
        // uncompared.
        //
        // Built from the counts above and the row text this function was
        // handed, in the same order the buffer was written in, and never from
        // the buffer: it is an expectation the bytes can disagree with.
        let mut script = check::Script::new(geometry);
        if let Some(top) = self.painted {
            for line in top..geometry.band_top() {
                script.erase(line);
            }
        }
        if shown > 0 {
            for line in first.saturating_sub(vacated)..first {
                script.erase(line);
            }
        }
        for _ in 0..scroll - fresh {
            script.scroll();
        }
        for (offset, row) in rows[settled - usize::from(shown)..settled]
            .iter()
            .enumerate()
        {
            let offset = u16::try_from(offset).unwrap_or(shown);
            script.place(first.saturating_add(offset), row);
        }
        for row in &rows[settled..] {
            script.scroll();
            script.place(geometry.band_top().saturating_sub(1), row);
        }
        (self.buffer.clone(), footprint, script)
    }

    /// Records that something which is **not a frame** gave the alternate plane
    /// back.
    ///
    /// There is exactly one such writer, and it is the reason this is not
    /// [`Self::frame_landed`]: the stop handler. A `SIGTSTP` at a question runs
    /// `super::signals`'s `stop_for_job_control`, which writes
    /// [`super::term::abnormal_restore`] -- `1049l` first, unconditionally,
    /// because an exit that does not know what is on the screen may not ask --
    /// and stops the process with the user's own buffer back. The `SIGCONT`
    /// that follows re-announces the mode set (`super::resume`), and the mode
    /// set carries **no** `1049h`.
    ///
    /// So the terminal is on the normal buffer and this band's record says
    /// otherwise, and the record is what every transition is decided from
    /// ([`Self::on_alternate`]). Left uncorrected the next tick asks for a
    /// *repaint* of a plane the terminal is not showing: either nothing at all,
    /// because the cache says the screen already holds it -- a session with no
    /// band and no question on it -- or a full-screen erase and repaint of the
    /// approval surface **onto the buffer the user was just given back**.
    ///
    /// Told, the band is where it started: the next tick finds the session
    /// still owning the question, takes the plane again with `1049h` and paints
    /// the whole surface onto it ([`Self::enter_alternate`]).
    pub(crate) fn plane_given_back(&mut self) {
        self.showing = super::shell::ScreenOwner::Primary;
        // And nothing is known about the buffer that was handed back either: it
        // is the terminal's own again, and whatever this band last painted on
        // it went with it.
        self.alternate = None;
    }

    /// How many rows the band has taken from the document since the last frame
    /// that landed.
    ///
    /// The mirror of the window [`Self::release`] gives back. `painted` is the
    /// band's top as of the last delivered write, so a `painted` **below** the
    /// band's top now means the band has grown into rows that were the
    /// document's -- one row when a turn starts and its activity row appears,
    /// one when the composer wraps onto a second line, several when a question
    /// opens a panel.
    ///
    /// Zero on a band that knows nothing about the screen. A resize, a
    /// `/clear`, a Ctrl-L and a resume all reach here with a `painted` from the
    /// screen that *was*, and the terminal has since re-wrapped or erased its
    /// own document by rules this module does not model -- so those rows are
    /// not this band's to move. A resize to a shorter screen is the case that
    /// would otherwise look exactly like a band that grew.
    fn grown(&self, geometry: &Geometry) -> u16 {
        if self.damaged {
            return 0;
        }
        // Only rows this band wrote the document onto, and only once the band
        // has actually reached them. A band that grew over rows the *terminal*
        // holds is Phase 1's accepted trade -- the composer wrapping as a draft
        // is typed grows over whatever is above it, and scrolling the screen on
        // every keystroke that wrapped would be a worse answer than covering a
        // row nothing here wrote.
        let bottom = match self.document_bottom {
            Some(bottom) => bottom,
            None => return 0,
        };
        let top = geometry.band_top();
        if bottom < top {
            return 0;
        }
        bottom.saturating_sub(top).saturating_add(1)
    }

    /// Carries the document up out of the way of a band that has grown, in one
    /// write and one flush.
    ///
    /// **The counterpart [`Self::release`] never had.** That function handles a
    /// band that *shrank*: the rows it gave back were its own, so it erases
    /// them. This handles the other direction, and the difference is whose text
    /// is on the rows: a band that grows takes rows the **document** is holding,
    /// and the only thing a terminal can do with a row it has no room for is
    /// scroll it -- off the top of the screen and into its own scrollback,
    /// where it is still the user's.
    ///
    /// Without it those rows are destroyed rather than moved, and it is not a
    /// corner: a submitted line is echoed into the document the instant it is
    /// submitted, on the bottom document row, and the turn it started is
    /// announced by the runtime a moment later. Whether the band grows onto that
    /// row before or after the echo lands on it is the scheduler's to choose,
    /// so the loss is a **flake** -- which is how it was found, as a
    /// disagreement between two builds of `scripts/smoke-tui.sh`'s scenario 13
    /// that had each raced differently.
    ///
    /// A real linefeed from the bottom margin ([`scroll_one`]), which is the
    /// same instrument [`Self::render_append`] makes room with and the only one
    /// that feeds native scrollback -- so the two writers move the document by
    /// the same means and a row carried here is a row an append would have
    /// carried the same way.
    ///
    /// **Nothing is recorded until the write lands**, exactly as
    /// [`Self::append_document`] records nothing: the shadow is swapped and the
    /// band's top is raised only by a write that succeeded, so a screen that
    /// refused this is owed it again.
    pub(crate) fn carry_document(
        &mut self,
        out: &mut impl Sink,
        geometry: &Geometry,
    ) -> Result<(), Emit> {
        let grown = self.grown(geometry);
        if grown == 0 {
            return Ok(());
        }
        let seed = self
            .seed(check::PlaneKind::Primary, geometry)
            .map_err(Emit::rejected)?;
        // One scroll per row the band grew by, and no row edited at all: every
        // row that leaves the top of the screen on the way is compared as it
        // goes, which is the only moment it can be.
        let mut script = check::Script::new(geometry);
        self.buffer.clear();
        self.target.clone_from(&self.shadow);
        for _ in 0..grown {
            scroll_one(&mut self.buffer, geometry);
            self.target.scroll_up(1);
            script.scroll();
        }
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut self.buffer);
        }
        // Exactly `grown` rows and not one more: what leaves the top of the
        // screen is in the terminal's own scrollback for good, so a carry that
        // scrolled one row too many cannot be taken back by any later frame.
        // The pairing is checked by decoding rather than by pattern: a linefeed
        // whose `CUP` to the bottom row went missing walks the caret down and
        // scrolls nothing, and the count then disagrees.
        check::preflight(
            &seed,
            &self.buffer,
            &check::Declared::new(
                // A carry touches no row: it declares the scroll and the caret,
                // and every cell on the screen is then held to the seed,
                // displaced by exactly that many rows. Nothing here is compared
                // against the band's own target, which is the size of a shadow
                // that may be older than the screen being scrolled.
                check::Intent::Document {
                    script: &script,
                    caret: Some((geometry.rows, 1)),
                    // A carry writes linefeeds and nothing else: it says
                    // nothing about the cursor, and declares as much.
                    cursor_visible: None,
                    title: self.shown_title.as_deref(),
                },
                check::Footprint::new(
                    check::PlaneKind::Primary,
                    vec![check::Seg::Scroll {
                        rows: u32::from(grown),
                    }],
                ),
            ),
        )
        .map_err(Emit::rejected)?;
        out.emit(&self.buffer)?;
        std::mem::swap(&mut self.shadow, &mut self.target);
        // The rows moved up with the screen. A bottom that has reached the top
        // of the screen has left it, and what leaves the top is in the
        // terminal's own scrollback -- there is nothing left here to protect.
        self.document_bottom = self
            .document_bottom
            .and_then(|bottom| bottom.checked_sub(grown))
            .filter(|bottom| *bottom > 0);
        // The band's top is now its own again, so the frame that follows erases
        // from a row the document has been carried off.
        self.delivered(geometry);
        // The scroll left the caret on the bottom row, which is not where the
        // last frame left it: the next frame owes a `CUP`.
        self.caret = None;
        Ok(())
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

    /// The model this band's next vector is decoded against.
    ///
    /// Everything in it is state the band already keeps and already adopts only
    /// from a write that landed -- the shadow, the caret a frame left, the
    /// title the terminal was last told -- so the check needs no adoption
    /// discipline of its own and makes no standing claim about the screen.
    fn seed(
        &self,
        plane: check::PlaneKind,
        geometry: &Geometry,
    ) -> io::Result<check::TerminalModel> {
        Ok(check::TerminalModel::seed_primary(
            &self.shadow,
            // The screen this vector is being built for, which a shadow that
            // has not been sized yet -- or was sized for a screen that has
            // since changed -- does not describe: what it does not reach is
            // seeded foreign rather than blank.
            geometry.rows,
            geometry.cols,
            // The band counts a caret's column as the cells to its **left**;
            // a terminal counts columns from one, and `cup` converts at the
            // one place a frame writes it.
            self.caret
                .map(|(row, cells)| (row, cells.saturating_add(1))),
            self.shown_title.as_deref(),
            plane,
        )?)
    }

    /// Every row one band frame may reach.
    ///
    /// `released` is the top of what the frame is about to write -- the old top
    /// while a shrinking band's erasures are still owed -- and the erase runs
    /// from there to the screen's **last** row rather than to the hint row,
    /// because that is what [`Grid::paint_band`] does (`grid.rs:240-243`).
    /// Placement stops at the hint row, because that is where `paint_band`
    /// breaks. Nothing above `released` may be touched: that is the terminal's
    /// own document, and this phase never repaints one.
    fn band_footprint(&self, released: u16, geometry: &Geometry) -> check::Footprint {
        check::Footprint::new(
            check::PlaneKind::Primary,
            vec![
                check::Seg::Erase(released..=geometry.rows),
                check::Seg::Place(geometry.band_top()..=geometry.hint),
            ],
        )
    }

    /// Records that everything the band built reached the screen, so the rows
    /// it gave back are blank and its top really is its top row.
    fn delivered(&mut self, geometry: &Geometry) {
        self.record_painted(geometry.band_top(), geometry);
    }

    /// Records the band's top row **and the screen it is a row of**, together,
    /// because a row number from one geometry and a size from another describe
    /// no screen at all.
    fn record_painted(&mut self, top: u16, geometry: &Geometry) {
        self.painted = Some(top);
        self.painted_on = Some((geometry.rows, geometry.cols));
    }

    /// [`render_append`](Self::render_append) plus exactly one emit, for the
    /// same reason [`commit`](Self::commit) is one.
    /// Whether the **primary** plane is holding bytes this band has not framed
    /// since.
    ///
    /// The caret answers it, and it is not a coincidence that it does: every
    /// way the band's rows stop being where the last frame put them --- an
    /// append or a carry, which scroll the screen, and
    /// [`invalidate`](Self::invalidate), which says the rows are not knowable
    /// at all --- leaves the caret somewhere no frame placed it, and only a
    /// landed frame puts it back. So "the caret is not where a frame left it"
    /// and "the band is not where a frame left it" are the same fact.
    ///
    /// Asked by the transition barrier before the terminal is handed to a
    /// question ([`super::event_loop`]): `1049h` **saves this buffer**, and a
    /// buffer whose band is a row above where it belongs is the one the user is
    /// given back when the question is answered.
    pub(crate) fn owes_primary_frame(&self) -> bool {
        self.caret.is_none()
    }

    /// Recovers from a primary-band frame the screen tore: a fixed cleanup
    /// vector, then the same frame rebuilt from an erased shadow, in this
    /// same call -- never deferred to a later tick, and never a second
    /// attempt.
    ///
    /// **`rows`, `geometry` and `cursor` are the caller's torn attempt,
    /// unchanged.** Recovery rebuilds exactly what the session already meant
    /// to show; it is not a place a caller composes a different frame.
    ///
    /// Only the band's own rows are treated as unknown. `top(geometry)..=
    /// geometry.rows` in [`shadow`](Self::shadow) is reset to
    /// [`Cell::Empty`](super::grid::Cell::Empty) and [`damaged`](Self::damaged)
    /// is raised, which is narrower than [`invalidate`](Self::invalidate):
    /// `document_bottom`, `painted`/`painted_on` and every cell above the
    /// band are left exactly as they were, because nothing about them was
    /// touched by a torn *band* frame. `caret` and `shown_title` are forced
    /// unknown, because a terminal that was mid-sequence for the tear may
    /// have consumed some of what either believed it already had.
    ///
    /// **What licenses the cleanup vector is narrower than the normal
    /// alphabet.** [`check::preflight_recovery_cleanup`] accepts exactly one
    /// fixed vector and nothing a full [`check::preflight`] would also
    /// accept from a genuine partial-parser state -- that broader claim is
    /// not made here, and does not come from this checker: it rests on an
    /// independent terminal experiment kept outside this crate. Silently
    /// treating an untried screen as known is exactly what this function
    /// must not do, which is why the erase below runs only *after* the
    /// cleanup vector has actually landed.
    ///
    /// Any failure -- the cleanup vector refused or unlanded, or the rebuild
    /// itself failing the same way the original frame did -- is returned
    /// as-is, immediately: there is no second recovery attempt, and the
    /// caller treats every error from this function as fatal.
    pub(crate) fn recover_primary(
        &mut self,
        out: &mut impl Sink,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> Result<Commit, Emit> {
        if !matches!(self.showing, super::shell::ScreenOwner::Primary) {
            return Err(Emit::rejected(io::Error::other(
                "recover_primary was called while the terminal is not on the primary plane",
            )));
        }
        check::preflight_recovery_cleanup(check::RECOVERY_CLEANUP.as_bytes())
            .map_err(Emit::rejected)?;
        out.emit(check::RECOVERY_CLEANUP.as_bytes())?;

        // The cleanup vector is on the terminal: only the band's own rows are
        // unknown now, not what is above them or who owns which row.
        let top = self.top(geometry);
        for line in top..=geometry.rows {
            self.shadow.erase_row(line);
        }
        self.damaged = true;
        self.caret = None;
        self.shown_title = None;

        // Same captured rows, geometry and cursor; one more emit. Any error
        // here propagates as-is -- this function makes exactly one attempt.
        self.commit(out, rows, geometry, cursor)
    }

    /// Recovers from a repaint of the **alternate** plane the screen tore: the
    /// same fixed cleanup vector [`Self::recover_primary`] writes, then the same
    /// surface repainted whole, in this same call -- never deferred to a later
    /// tick, and never a second attempt.
    ///
    /// The one other vector whose whole intended content is in hand when it
    /// tears. A repaint of a plane this band already holds
    /// ([`Self::repaint_alternate`]) takes no plane, gives none back, scrolls
    /// nothing into a scrollback the terminal keeps, and paints every row of a
    /// surface that shares its screen with nothing -- so a rebuild from the rows it was
    /// built from is the whole of what the torn attempt meant, and not an
    /// approximation of it. The two transitions are deliberately **not**
    /// recoverable this way: a `1049h` or `1049l` the terminal took part of
    /// leaves which buffer it is showing unknown, and no rebuild is right on
    /// both.
    ///
    /// **`rows`, `geometry` and `cursor` are the caller's torn attempt,
    /// unchanged**, for the reason they are in `recover_primary`.
    ///
    /// **The whole alternate surface is unknown afterwards, and only it.** The
    /// cache of what that plane holds is dropped -- it is the borrowed plane's
    /// shadow, its damage and its caret in one, since a repaint is whole and
    /// ends on an explicit `CUP` -- so the rebuild below is a full repaint by
    /// construction rather than an equality against a screen nobody can vouch
    /// for. The title the terminal was told is forgotten too: it is one window
    /// title for both buffers, and forgetting it costs only the `OSC 2` the
    /// restore re-asserts anyway. What is **not** touched is every record of the
    /// normal buffer -- its shadow, `damaged`, `painted` and the caret the last
    /// primary frame left. The terminal saved that buffer and its cursor at
    /// `1049h` and gives both back at `1049l`, and neither the torn bytes nor
    /// the cleanup are a plane transition; forgetting the caret in particular
    /// would read, at the transition barrier, as a primary plane owed a frame
    /// it is not owed ([`Self::owes_primary_frame`]).
    ///
    /// What licenses the cleanup vector, and what it does not establish about
    /// the terminal's own parser, is exactly as `recover_primary` says; the
    /// vector is plane-neutral -- it carries no `?1049`
    /// (`check::tests::the_recovery_cleanup_takes_and_gives_back_no_plane`) --
    /// so it lands on the buffer the tear did.
    ///
    /// Any failure is returned as-is and is fatal to the caller; the cache is
    /// adopted from the rebuild only once it has landed, as everywhere else.
    pub(crate) fn recover_alternate(
        &mut self,
        out: &mut impl Sink,
        rows: &[String],
        geometry: &Geometry,
        cursor: (u16, u16),
    ) -> Result<(), Emit> {
        if !self.on_alternate() {
            return Err(Emit::rejected(io::Error::other(
                "recover_alternate was called while the terminal is not on the alternate plane",
            )));
        }
        check::preflight_recovery_cleanup(check::RECOVERY_CLEANUP.as_bytes())
            .map_err(Emit::rejected)?;
        out.emit(check::RECOVERY_CLEANUP.as_bytes())?;

        // The cleanup vector is on the terminal: what the borrowed plane holds
        // is unknown now, and nothing about the buffer the terminal saved is.
        self.alternate = None;
        self.shown_title = None;

        // Same captured rows, geometry and cursor; one more emit, checked like
        // every repaint before it is written. Any error here propagates as-is
        // -- this function makes exactly one attempt.
        let frame = self
            .repaint_alternate(rows, geometry, cursor)
            .map_err(Emit::rejected)?;
        out.emit(frame.bytes())?;
        self.frame_landed(&frame, geometry, cursor);
        Ok(())
    }

    /// The last document row this band can still say anything about, or `None`
    /// when there is no such row.
    ///
    /// Two bounds and both are needed. [`Self::top`] is the band's own top --
    /// the old one while a shrinking band's erasures are still owed -- and
    /// nothing at or below it is the document's. [`Self::document_bottom`] is
    /// the lowest row this band itself put a document row on and has not since
    /// scrolled off the screen, which is the one number that tells "a row xfx
    /// wrote" from "a row the terminal happened to be holding"
    /// ([`Self::grown`] reads it for the same reason). A band that has written
    /// no document row, or whose rows have all left the top of the screen, has
    /// nothing here to repaint.
    fn document_last(&self, geometry: &Geometry) -> Option<u16> {
        let above_band = self.top(geometry).checked_sub(1).filter(|row| *row > 0)?;
        Some(above_band.min(self.document_bottom?))
    }

    /// Repaints the document rows already on the screen in the palette a theme
    /// report has moved the session to.
    ///
    /// **The one emitter that writes above the band's top row**, and everything
    /// about it is narrowed to make that safe:
    ///
    /// * It writes **no scroll and no erase of its own** -- a `CUP`, the cells
    ///   the palette moved, and the pen closed behind them ([`Grid::diff`]).
    ///   Nothing it does can put a row into native scrollback, which is the one
    ///   thing on this surface that cannot be taken back.
    /// * It repaints **cells, never rows**: the candidate is a copy of the
    ///   shadow with one colour slot rewritten per recognised cell
    ///   ([`Grid::retint_document`]), so it cannot *create* a cell and
    ///   therefore cannot reconstruct history the screen no longer has.
    /// * It runs only where the shadow is a claim about **this** screen. A
    ///   damaged band, a shadow of another size and a plane this band is not on
    ///   are each [`Retint::Deferred`] with no bytes -- what is not knowable is
    ///   not repainted, and nothing is manufactured to fill the gap -- while a
    ///   session that owns no document row on a screen it *can* describe is
    ///   [`Retint::Settled`], because there is nothing there to owe.
    ///
    /// The caret is put back where the last frame left it, explicitly, because
    /// this vector is not a frame and owes the band nothing. A band that is
    /// already owed a `CUP` (`caret == None`, after an append or a carry) gets
    /// no promise it did not have: the vector declares no caret, leaves it
    /// wherever the last cell put it, and the frame that was already owed still
    /// owes it.
    ///
    /// **Nothing is recorded until the write lands**, as everywhere else here:
    /// a refused vector leaves the shadow exactly as it was and the caller
    /// still owes the retint.
    pub(crate) fn retint_document(
        &mut self,
        out: &mut impl Sink,
        palette: &super::theme::Palette,
        geometry: &Geometry,
    ) -> Result<Retint, Emit> {
        if self.damaged
            || !matches!(self.showing, super::shell::ScreenOwner::Primary)
            || self.shadow.rows() != geometry.rows
            || self.shadow.cols() != geometry.cols
        {
            return Ok(Retint::Deferred);
        }
        let Some(last) = self.document_last(geometry) else {
            return Ok(Retint::Settled(Commit::NoChange));
        };
        // Before the candidate is built, for the reason `commit`'s is: a model
        // seeded from state this call has already moved would agree with the
        // vector because it was built from it.
        let seed = self
            .seed(check::PlaneKind::Primary, geometry)
            .map_err(Emit::rejected)?;
        self.target.clone_from(&self.shadow);
        if self.target.retint_document(last, palette) == 0 {
            return Ok(Retint::Settled(Commit::NoChange));
        }
        self.buffer.clear();
        if self.shadow.diff(&self.target, geometry, &mut self.buffer) == 0 {
            // Unreachable through [`Grid::retint_document`], which answers
            // nothing for a cell already in this palette's own grey -- a guard
            // rather than a case, because a zero-byte vector declared as a
            // paint is the one shape the check below cannot be handed.
            return Ok(Retint::Settled(Commit::NoChange));
        }
        let caret = self
            .caret
            .map(|(row, cells)| (row, cells.saturating_add(1)));
        if let Some((row, column)) = caret {
            cup(&mut self.buffer, row, column);
        }
        #[cfg(test)]
        if let Some(tamper) = self.tamper {
            tamper(&mut self.buffer);
        }
        check::preflight(
            &seed,
            &self.buffer,
            &check::Declared::new(
                // The whole screen, against the candidate: the band's own rows
                // are in it unchanged, so a vector that strayed below the
                // document is refused by the cells rather than only by the
                // footprint. No `?25` and no title, exactly as the append and
                // the carry declare.
                check::Intent::Primary {
                    grid: &self.target,
                    caret,
                    cursor_visible: None,
                    title: self.shown_title.as_deref(),
                },
                check::Footprint::new(check::PlaneKind::Primary, vec![check::Seg::Place(1..=last)]),
            ),
        )
        .map_err(Emit::rejected)?;
        out.emit(&self.buffer)?;
        std::mem::swap(&mut self.shadow, &mut self.target);
        Ok(Retint::Settled(Commit::Painted))
    }

    pub(crate) fn append_document(
        &mut self,
        out: &mut impl Sink,
        scroll: usize,
        rows: &[String],
        geometry: &Geometry,
    ) -> Result<(), Emit> {
        let seed = self
            .seed(check::PlaneKind::Primary, geometry)
            .map_err(Emit::rejected)?;
        let (appended, footprint, script) = self.render_append(scroll, rows, geometry);
        if appended.is_empty() {
            return Ok(());
        }
        #[cfg(test)]
        let appended = {
            let mut appended = appended;
            if let Some(tamper) = self.tamper {
                tamper(&mut appended);
            }
            appended
        };
        // The caret is declared as unconstrained, and that is this emitter's
        // own record rather than a gap: an append leaves the caret wherever its
        // last placement put it, which is why it sets `caret = None` below and
        // the next frame owes a `CUP`. Everything else is asserted -- the cells,
        // the rows that may be reached, and the exact number of scrolls, since
        // a row carried off the top of the screen is in native scrollback for
        // good.
        check::preflight(
            &seed,
            &appended,
            &check::Declared::new(
                // **Rows rather than a screen.** The band's own target grid is
                // the size of its shadow, and the shadow is resized by the
                // frame path alone -- while the document is written *before*
                // the band on every tick. So on the one emitter whose mistakes
                // cannot be taken back, a comparison against that grid stops
                // describing exactly the rows a screen that grew has just
                // added. What is compared instead is the text this function was
                // handed, on the rows it computed, with every other cell held
                // to the seed.
                check::Intent::Document {
                    script: &script,
                    caret: None,
                    // As with the carry above: an append carries no `?25`.
                    cursor_visible: None,
                    title: self.shown_title.as_deref(),
                },
                footprint,
            ),
        )
        .map_err(Emit::rejected)?;
        out.emit(&appended)?;
        // The release rode along at the head of those bytes, so the same rule
        // applies: delivered, and only then is the band's top its divider.
        std::mem::swap(&mut self.shadow, &mut self.target);
        // The document's newest row is on the row above the band, wherever the
        // append put it: every row this function places is anchored there, and
        // it is the row a band that grows next takes first
        // ([`Self::carry_document`]).
        self.document_bottom = Some(geometry.band_top().saturating_sub(1)).filter(|row| *row > 0);
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
        let Some(cluster) = super::wrap::first_cluster(rest) else {
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

    use super::super::deliver::RawWrite;

    fn geometry() -> Geometry {
        crate::tui::layout::solve(24, 80, 1).expect("a band")
    }

    fn band_rows() -> Vec<String> {
        vec!["--".to_string(), "> ".to_string(), "hint".to_string()]
    }

    /// The same band with the row a running turn puts above the divider.
    fn running_rows() -> Vec<String> {
        vec![
            "\u{2022} Thinking  0s".to_string(),
            "--".to_string(),
            "> ".to_string(),
            "hint".to_string(),
        ]
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

        /// What row `line` -- one-based, as the terminal counts -- says.
        fn row(&self, line: u16) -> String {
            self.lines[usize::from(line) - 1].clone()
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
        let bytes = band
            .render_append(1, &["answered".to_string()], &geometry())
            .0;
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
        let text = String::from_utf8(band.render_append(0, &rows, &geometry).0).expect("utf-8");
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
        let text = String::from_utf8(band.render_append(1, &["fresh".to_string()], &geometry).0)
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

        let text = String::from_utf8(band.render_append(1, &["fresh".to_string()], &geometry).0)
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
        let bytes = band
            .render_append(
                2,
                &["first\r\nsecond".to_string(), "third\n".to_string()],
                &geometry(),
            )
            .0;
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
        assert!(band.render_append(0, &[], &geometry()).0.is_empty());
    }

    #[test]
    fn an_append_that_scrolls_nothing_rewrites_the_row_where_it_already_is() {
        // A delta that only lengthened the last row of an answer. Scrolling
        // here would push a blank row into the document for every few
        // characters the model streams, and the answer would come out
        // double-spaced.
        let mut band = Band::new();
        let bytes = band
            .render_append(0, &["answered".to_string()], &geometry())
            .0;
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
        let bytes = band
            .render_append(1, &["abcd".to_string(), "efgh".to_string()], &geometry())
            .0;
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
        screen.feed(&band.render_append(5, &rows, &geometry).0);

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
            let bytes = band.render_append(count, &rows, &geometry).0;
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
        let bytes = band.render_append(3, &rows, &geometry).0;
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
        screen.feed(&band.render_append(1, &["ok".to_string()], &geometry).0);
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
            screen.feed(&band.render_append(append.scroll, &append.rows, &geometry).0);
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

    /// A screen that remembers how many vectors it was offered.
    ///
    /// "One vector per frame" is the property that makes what is on the
    /// terminal knowable at all -- a second vector inside one frame is a window
    /// another writer can interleave in -- so it is counted rather than
    /// assumed. It counts **emits**, which is the unit the band decides; how
    /// many syscalls one emit costs is the kernel's business and is asserted
    /// where that lives (`super::super::deliver`).
    #[derive(Default)]
    struct Counted {
        writes: usize,
        written: Vec<u8>,
    }

    impl Sink for Counted {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            self.writes += 1;
            self.written.extend_from_slice(bytes);
            Ok(())
        }
    }

    /// A screen that refuses everything, for ever, and takes nothing on the way.
    struct Refuses;

    impl RawWrite for Refuses {
        fn write_once(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "gone"))
        }
    }

    impl Sink for Refuses {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            super::super::deliver::emit_counted(self, bytes)
        }
    }

    /// A screen that refuses `refusals` vectors -- taking nothing of them --
    /// and then takes them whole.
    struct Fussy {
        refusals: usize,
        written: Vec<u8>,
    }

    impl RawWrite for Fussy {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.refusals > 0 {
                self.refusals -= 1;
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "not now"));
            }
            self.written.extend_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    impl Sink for Fussy {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            super::super::deliver::emit_counted(self, bytes)
        }
    }

    /// A screen that takes `prefix` bytes of the vector it is offered, fails
    /// once, and takes everything after that.
    ///
    /// The failure is deliberately not permanent: what a later offer carries
    /// **lands**, so the bytes past the prefix are the band's own answer to
    /// "what do you still believe the terminal is holding?".
    struct HalfDeaf {
        prefix: usize,
        failed: bool,
        written: Vec<u8>,
    }

    impl HalfDeaf {
        fn taking(prefix: usize) -> Self {
            Self {
                prefix,
                failed: false,
                written: Vec::new(),
            }
        }
    }

    impl RawWrite for HalfDeaf {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.failed {
                self.written.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            if self.prefix > 0 {
                let taken = self.prefix.min(bytes.len());
                self.prefix -= taken;
                self.written.extend_from_slice(&bytes[..taken]);
                return Ok(taken);
            }
            self.failed = true;
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the screen stopped taking bytes",
            ))
        }
    }

    impl Sink for HalfDeaf {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            super::super::deliver::emit_counted(self, bytes)
        }
    }

    /// A screen that takes `successes` vectors whole, then refuses every one
    /// after that -- taking nothing of them.
    ///
    /// The mirror image of [`Fussy`]: that one models a screen recovering
    /// *after* a failure, this one models a screen that fails on the
    /// **second** vector of a call that writes more than one -- exactly the
    /// shape [`Band::recover_primary`]'s rebuild write needs, once its
    /// cleanup vector has already landed.
    struct WorksThenRefuses {
        successes: usize,
        written: Vec<u8>,
    }

    impl RawWrite for WorksThenRefuses {
        fn write_once(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.successes > 0 {
                self.successes -= 1;
                self.written.extend_from_slice(bytes);
                return Ok(bytes.len());
            }
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "gone after the vectors it agreed to take",
            ))
        }
    }

    impl Sink for WorksThenRefuses {
        fn emit(&mut self, bytes: &[u8]) -> Result<(), Emit> {
            super::super::deliver::emit_counted(self, bytes)
        }
    }

    #[test]
    fn recovering_a_torn_primary_band_writes_the_fixed_cleanup_then_the_rebuilt_frame() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame lands so recovery has something torn to rebuild from");

        screen.written.clear();
        let commit = band
            .recover_primary(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a cleanup and a rebuild that both land is a successful recovery");

        assert!(
            matches!(commit, Commit::Painted),
            "a rebuild that changed the screen was not reported as painted: {commit:?}"
        );
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            text.starts_with(check::RECOVERY_CLEANUP),
            "the recovery did not lead with the fixed cleanup vector: {text:?}"
        );
        for row in band_rows() {
            assert!(
                text.contains(&row),
                "the recovery did not rebuild the row {row:?}: {text:?}"
            );
        }
        assert_eq!(
            band.caret,
            Some((23, 2)),
            "a landed rebuild did not record where the frame left the cursor"
        );
        assert!(
            !band.damaged,
            "a landed rebuild left the band claiming it still owed a repaint"
        );
    }

    #[test]
    fn recovery_rebuilds_only_the_bands_own_rows_and_leaves_the_document_above_it_alone() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame lands");
        band.append_document(&mut screen, 0, &["above the band".to_string()], &geometry)
            .expect("a document row settles above the band");

        // A snapshot of every cell above the band, taken by hand rather than
        // by a helper this test does not control -- the claim is only that
        // recovery leaves these cells exactly as it found them, whatever they
        // are.
        let snapshot = |band: &Band| -> Vec<String> {
            (1..geometry.band_top())
                .flat_map(|line| {
                    (0..geometry.cols)
                        .map(move |column| format!("{:?}", band.shadow.cell(line, column)))
                })
                .collect()
        };
        let document_before = snapshot(&band);
        let document_bottom_before = band.document_bottom;

        let commit = band
            .recover_primary(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a cleanup and a rebuild that both land is a successful recovery");
        assert!(matches!(commit, Commit::Painted));

        assert_eq!(
            band.document_bottom, document_bottom_before,
            "a band-only recovery moved the boundary of the terminal's own document"
        );
        assert_eq!(
            document_before,
            snapshot(&band),
            "a band-only recovery touched a cell above the band"
        );
    }

    #[test]
    fn recover_primary_rejects_a_call_while_the_terminal_is_not_on_the_primary_plane() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        let entered = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("entering the alternate plane");
        screen.emit(&entered.bytes).expect("the enter lands");
        band.frame_landed(&entered, &geometry, (7, 0));

        let failure = band
            .recover_primary(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect_err("recovery must refuse a screen that is not on the primary plane");
        assert!(
            matches!(failure, Emit::Rejected(_)),
            "the wrong-plane guard was not reported as a rejection: {failure:?}"
        );
    }

    #[test]
    fn recover_primary_is_fatal_when_the_cleanup_vector_is_refused() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame lands");
        let shadow_before = band.shadow.clone();

        let mut refusing = Refuses;
        let failure = band
            .recover_primary(&mut refusing, &band_rows(), &geometry, (23, 2))
            .expect_err("a refused cleanup vector must not be treated as recovered");
        assert!(
            matches!(failure, Emit::ZeroProgress(_)),
            "a wholly refused cleanup vector was not reported as zero progress: {failure:?}"
        );
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&shadow_before, &geometry, &mut owed),
            0,
            "a cleanup vector that never landed still erased the band's shadow rows"
        );
    }

    #[test]
    fn recover_primary_is_fatal_when_the_rebuild_is_refused_after_the_cleanup_lands() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame lands");

        // One success: the fixed cleanup vector. The rebuild that follows in
        // the same call is refused outright.
        let mut screen = WorksThenRefuses {
            successes: 1,
            written: Vec::new(),
        };
        let failure = band
            .recover_primary(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect_err("a rebuild the screen refused must not be treated as recovered");
        assert!(
            matches!(failure, Emit::ZeroProgress(_)),
            "a wholly refused rebuild was not reported as zero progress: {failure:?}"
        );
        assert!(
            String::from_utf8_lossy(&screen.written).starts_with(check::RECOVERY_CLEANUP),
            "the one vector that did land was not the fixed cleanup vector"
        );
    }

    /// A band that has painted its primary frame and then taken the alternate
    /// plane with `rows` on it, every byte of both landed, and the screen it
    /// did it on.
    fn on_the_alternate_plane(rows: &[String]) -> (Band, Geometry) {
        let (mut band, geometry) = painted_primary();
        let entered = band
            .enter_alternate(rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        (band, geometry)
    }

    /// The other plane's surface with its marker moved: what a repaint of a
    /// plane the band already holds is asked for.
    fn moved_marker(rows: u16) -> Vec<String> {
        let mut rows = screen_rows(rows);
        rows[2] = "> moved marker".to_string();
        rows
    }

    #[test]
    fn recovering_a_torn_alternate_repaint_writes_the_fixed_cleanup_then_the_whole_surface_again() {
        let (mut band, geometry) = on_the_alternate_plane(&screen_rows(24));
        let caret_before = band.caret;
        let painted_before = band.painted_top();
        let shadow_before = band.shadow.clone();
        let wanted = moved_marker(geometry.rows);

        let mut screen = Counted::default();
        band.recover_alternate(&mut screen, &wanted, &geometry, (3, 2))
            .expect("a cleanup and a rebuild that both land is a successful recovery");

        assert_eq!(
            screen.writes, 2,
            "recovery is the cleanup and one rebuild, each one vector"
        );
        let text = String::from_utf8(screen.written).expect("utf-8");
        let rebuilt = text
            .strip_prefix(check::RECOVERY_CLEANUP)
            .unwrap_or_else(|| panic!("the recovery did not lead with the cleanup: {text:?}"));
        assert!(
            rebuilt.starts_with(BEGIN_FRAME) && rebuilt.ends_with(END_FRAME),
            "the rebuild is not one whole frame: {rebuilt:?}"
        );
        assert!(
            rebuilt.contains(&format!("\u{1b}[1;1H{ERASE_SCREEN}")),
            "the rebuild did not erase the plane before painting it: {rebuilt:?}"
        );
        for (offset, row) in wanted.iter().enumerate() {
            assert!(
                rebuilt.contains(&format!("\u{1b}[{};1H{row}", offset + 1)),
                "the rebuild did not repaint row {} of the surface: {rebuilt:?}",
                offset + 1
            );
        }
        assert!(
            !text.contains("\u{1b}[?1049"),
            "recovering a repaint moved the terminal between planes: {text:?}"
        );
        assert!(band.on_alternate(), "recovery lost the plane it was on");
        assert_eq!(
            band.alternate,
            Some((wanted.clone(), (3, 2))),
            "the rebuild that landed was not adopted as what the plane holds"
        );
        // Nothing about the buffer the terminal saved was touched by a vector
        // that never left the borrowed one.
        assert_eq!(
            band.caret, caret_before,
            "recovery forgot the primary caret"
        );
        assert_eq!(
            band.painted_top(),
            painted_before,
            "recovery moved the primary band's top"
        );
        assert!(!band.damaged, "recovery damaged the primary plane");
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&shadow_before, &geometry, &mut owed),
            0,
            "recovery changed the primary plane's shadow"
        );
    }

    #[test]
    fn a_surface_the_cache_already_claimed_is_still_rebuilt_whole_by_recovery() {
        // The tear can be of a repaint of exactly the surface the cache holds
        // -- the forced one behind an earlier recovery is that repaint -- and a
        // rebuild that consulted the cache would find "the screen already holds
        // this" and write nothing after the cleanup.
        let rows = screen_rows(24);
        let (mut band, geometry) = on_the_alternate_plane(&rows);

        let mut screen = Counted::default();
        band.recover_alternate(&mut screen, &rows, &geometry, (7, 0))
            .expect("a successful recovery");
        let text = String::from_utf8(screen.written).expect("utf-8");
        let rebuilt = text
            .strip_prefix(check::RECOVERY_CLEANUP)
            .expect("the cleanup first");
        assert!(
            rebuilt.contains(ERASE_SCREEN) && rebuilt.contains("screen row 23"),
            "recovery trusted a cache the tear made worthless: {rebuilt:?}"
        );
    }

    #[test]
    fn recover_alternate_rejects_a_call_while_the_terminal_is_not_on_the_alternate_plane() {
        let (mut band, geometry) = painted_primary();
        let mut screen = Counted::default();
        let failure = band
            .recover_alternate(&mut screen, &screen_rows(geometry.rows), &geometry, (7, 0))
            .expect_err("recovery must refuse a screen that is not on the alternate plane");
        assert!(
            matches!(failure, Emit::Rejected(_)),
            "the wrong-plane guard was not reported as a rejection: {failure:?}"
        );
        assert_eq!(screen.writes, 0, "the wrong-plane guard wrote something");
    }

    #[test]
    fn recover_alternate_is_fatal_when_the_cleanup_vector_is_refused() {
        let rows = screen_rows(24);
        let (mut band, geometry) = on_the_alternate_plane(&rows);

        let failure = band
            .recover_alternate(
                &mut Refuses,
                &moved_marker(geometry.rows),
                &geometry,
                (3, 2),
            )
            .expect_err("a refused cleanup vector must not be treated as recovered");
        assert!(
            matches!(failure, Emit::ZeroProgress(_)),
            "a wholly refused cleanup vector was not reported as zero progress: {failure:?}"
        );
        // Nothing landed, so nothing this band believes changed either.
        assert_eq!(
            band.alternate,
            Some((rows, (7, 0))),
            "a cleanup vector that never landed still changed what the plane is believed to hold"
        );
    }

    #[test]
    fn recover_alternate_is_fatal_when_the_rebuild_is_refused_after_the_cleanup_lands() {
        let (mut band, geometry) = on_the_alternate_plane(&screen_rows(24));

        let mut screen = WorksThenRefuses {
            successes: 1,
            written: Vec::new(),
        };
        let failure = band
            .recover_alternate(&mut screen, &moved_marker(geometry.rows), &geometry, (3, 2))
            .expect_err("a rebuild the screen refused must not be treated as recovered");
        assert!(
            matches!(failure, Emit::ZeroProgress(_)),
            "a wholly refused rebuild was not reported as zero progress: {failure:?}"
        );
        assert_eq!(
            screen.written,
            check::RECOVERY_CLEANUP.as_bytes(),
            "the one vector that did land was not the fixed cleanup vector"
        );
        assert_eq!(
            band.alternate, None,
            "a rebuild that never landed was adopted as what the plane holds"
        );
    }

    #[test]
    fn a_forced_alternate_repaint_writes_the_surface_the_cache_says_is_already_there() {
        // The tick behind a recovered tear: the cache is right, and a repaint
        // that consulted it would be empty -- which proves nothing about a
        // screen that just tore. Forced, the same surface is written whole,
        // and nothing about the primary plane moves.
        let rows = screen_rows(24);
        let (mut band, geometry) = on_the_alternate_plane(&rows);
        assert!(
            band.repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "the cache did not make an unchanged repaint empty, so this case proves nothing"
        );
        let caret_before = band.caret;

        band.force_alternate_repaint();
        let frame = band
            .repaint_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        let text = String::from_utf8(frame.bytes().to_vec()).expect("utf-8");
        assert!(
            text.contains(ERASE_SCREEN) && text.contains("screen row 23"),
            "a forced repaint was not a whole one: {text:?}"
        );
        assert!(band.on_alternate(), "forcing a repaint gave the plane back");
        assert_eq!(
            band.caret, caret_before,
            "forcing a repaint forgot the primary caret"
        );
    }

    #[test]
    fn a_frame_the_screen_took_part_of_is_not_adopted_as_what_the_screen_holds() {
        // The adoption rule, on the failure it was written for. A prefix is not
        // a delivery: the shadow, the caret and the title are claims about a
        // frame the terminal has, and a band that adopted them here would
        // compute its next difference against a screen that got twelve bytes.
        // What the *loop* does about it is a separate decision and lives in
        // `super::event_loop`; this is only the band's own state.
        let mut band = Band::new();
        let geometry = geometry();
        let mut screen = HalfDeaf::taking(12);

        let failure = band
            .commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect_err("the screen stopped taking bytes part way through");

        assert!(
            matches!(failure, Emit::Partial { delivered: 12, .. }),
            "twelve accepted bytes were not reported as a prefix: {failure:?}"
        );
        let mut later = Vec::new();
        band.commit(&mut later, &band_rows(), &geometry, (23, 2))
            .expect("a vector takes everything");
        let text = String::from_utf8(later).expect("utf-8");
        for row in band_rows() {
            assert!(
                text.contains(&row),
                "the band adopted a frame the screen took twelve bytes of, so \
                 {row:?} was never offered again: {text:?}"
            );
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

    // -- The rows a frame's diff reads (P3-RETENTION) --

    /// The band with a turn running: the activity row moves the band's top row
    /// up by one, and giving it back moves it down again.
    fn running() -> Geometry {
        crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn")
    }

    #[test]
    fn a_frame_reads_no_cell_on_the_document_rows_above_the_band() {
        // The cost of a settled session: the band asks for a frame twice a
        // second while a turn runs, and the document above it is most of the
        // screen. Those rows are the shadow's own cells, cloned into the target
        // by `plan` and not touched again, so a frame that compared them is
        // paying for the whole terminal to re-derive what the clone proves.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        // A real document row above the band, so the rows the frame skips hold
        // something: a blank prefix would be skipped cheaply either way and the
        // measurement would say nothing.
        band.append_document(&mut screen, 0, &["a settled answer".to_string()], &geometry)
            .expect("a document row settles above the band");

        let typed = vec!["--".to_string(), "> hi".to_string(), "hint".to_string()];
        let (commit, rows) = crate::tui::grid::rows_compared(|| {
            band.commit(&mut screen, &typed, &geometry, (23, 4))
        });
        commit.expect("the frame that shows the keystroke");
        // The band's own three rows -- divider, composer, hint -- and not one
        // of the twenty-one above them.
        assert_eq!(
            rows, 3,
            "the frame read cells on the terminal's own document rows"
        );
    }

    #[test]
    fn a_frame_reads_the_rows_a_shrinking_band_gave_back() {
        // The boundary, in the direction that loses the user's screen: a turn
        // that ends gives the activity row back to the document, and the only
        // thing that ever erases it is this frame's diff. A window that began
        // at the band's *new* top row would leave `Thinking` on the screen for
        // the rest of the session -- and after the exit.
        let running = running();
        let idle = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &running_rows(), &running, (23, 2))
            .expect("a frame while a turn runs");
        assert_eq!(band.painted_top(), Some(21));

        screen.written.clear();
        let (commit, rows) = crate::tui::grid::rows_compared(|| {
            band.commit(&mut screen, &band_rows(), &idle, (23, 2))
        });
        commit.expect("the frame that ends the turn");
        assert_eq!(
            rows, 4,
            "the frame did not read exactly the row it gave back and the band's own three"
        );
        assert!(
            String::from_utf8(screen.written)
                .expect("utf-8")
                .contains(&released(21)),
            "the row the band gave back was never erased"
        );
    }

    #[test]
    fn a_frame_reads_the_rows_a_growing_band_took() {
        // The same boundary the other way: a turn that starts takes the row
        // above the divider, and a window that began at the row the *last*
        // frame painted from would never write it. The activity row would be
        // missing from a band that says it is thinking.
        let idle = geometry();
        let running = running();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &idle, (23, 2))
            .expect("an idle frame");
        assert_eq!(band.painted_top(), Some(22));

        screen.written.clear();
        let (commit, rows) = crate::tui::grid::rows_compared(|| {
            band.commit(&mut screen, &running_rows(), &running, (23, 2))
        });
        commit.expect("the frame that starts the turn");
        assert_eq!(
            rows, 4,
            "the frame did not read exactly the row it took and the band's own three"
        );
        assert!(
            String::from_utf8(screen.written)
                .expect("utf-8")
                .contains("Thinking"),
            "the row the band took was never painted"
        );
    }

    #[test]
    fn a_damaged_frame_reads_the_whole_screen() {
        // The fallback is a branch a session really takes -- a `/clear`, a
        // Ctrl-L, a resize, or the tick behind a recovered tear
        // ([`Band::force_redraw`]) -- not a defensive `else` nothing reaches.
        // A damaged frame knows nothing about the rows it is about to write
        // over, so it pays for the whole screen rather than trusting a window.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a settled first frame");

        band.force_redraw(&geometry);
        let (commit, rows) = crate::tui::grid::rows_compared(|| {
            band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
        });
        commit.expect("the frame that answers the damage");
        assert_eq!(
            rows,
            usize::from(geometry.rows),
            "a damaged frame narrowed its diff to rows it has no claim about"
        );
    }

    #[test]
    fn a_target_that_disagrees_with_the_screen_above_the_diffs_window_is_refused() {
        // The blind spot a narrowed diff would have if the check were narrowed
        // with it. The diff no longer reads the document rows; the check still
        // compares **every** cell of the screen against the grid the frame
        // declares, so a target that claims a row no byte of this frame goes
        // near is refused before the write rather than adopted as what the
        // terminal holds -- the diff's window and the check's are two different
        // things, and this is the one that says so.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;

        band.taint_target_with(|target, geometry| target.place_row(2, "a ghost", geometry));
        let typed = vec!["--".to_string(), "> hi".to_string(), "hint".to_string()];
        let refused = band
            .commit(&mut screen, &typed, &geometry, (23, 4))
            .expect_err("a frame whose target claims a document row it never wrote");
        assert!(
            matches!(refused, Emit::Rejected(_)),
            "the disagreement above the window was not reported as a rejection: {refused:?}"
        );
        assert_eq!(
            screen.writes, writes,
            "the frame reached the screen before the check caught it"
        );
    }

    #[test]
    fn a_skipped_frame_whose_target_disagrees_with_the_screen_is_refused() {
        // The same blind spot on the path that writes **no bytes at all**.
        // "The screen already holds this frame" is a claim about every cell,
        // and zero bytes make it good only if the model seeded from the shadow
        // already equals the target -- so the skip is checked too, and a target
        // that disagrees anywhere fails it.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");

        // The untainted shape first, pinned rather than assumed: the same rows,
        // the same cursor and the same title really do take the skip. Without
        // this the rejection below could be a frame that was never a skip at
        // all, and the case would be testing the ordinary write path.
        let skipped = band
            .commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("an identical frame");
        assert_eq!(
            skipped,
            Commit::NoChange,
            "the identical frame was not the skip this case is about"
        );
        let writes = screen.writes;

        band.taint_target_with(|target, geometry| target.place_row(2, "a ghost", geometry));
        // The same frame again, and now the target claims a row the screen
        // never got.
        let refused = band
            .commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect_err("a skip whose target claims a document row the screen never got");
        assert!(
            matches!(refused, Emit::Rejected(_)),
            "the skipped frame's disagreement was not reported as a rejection: {refused:?}"
        );
        assert_eq!(
            screen.writes, writes,
            "a skip that should have been refused wrote to the screen"
        );
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

        screen.feed(&band.render_append(1, &["ok".to_string()], &short).0);
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
        // One emit, and the flush that used to be asserted beside it is gone
        // rather than retargeted: there is no buffer to flush any more, which
        // is the whole of what this unit changed about transport.
        assert_eq!(screen.writes, 1, "a frame is one vector: {text:?}");
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

    #[test]
    fn a_band_that_grows_carries_the_document_up_instead_of_being_painted_over_it() {
        // The mirror of `Band::release`, and the case that had no counterpart.
        //
        // A prompt echo is written to the document the instant it is submitted,
        // on the bottom document row -- `band_top - 1`, with `band_top` the
        // **idle** band's. The turn it started is announced by the runtime a
        // moment later (`UiEvent::TurnStarted`), the band grows by its activity
        // row, and `band_top` becomes the row that echo is on. Every frame from
        // then on opens with `CUP(band_top,1)` + `ED`, so the row the user just
        // submitted is erased -- gone from the screen, and never in scrollback,
        // because this phase never repaints a document row.
        //
        // Which of the two happens first is the scheduler's to choose, so the
        // loss is a flake: `scripts/smoke-tui.sh`'s scenario 13 found it as a
        // disagreement between two builds that had each raced differently.
        let idle = geometry();
        let running =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        assert_eq!(
            running.band_top() + 1,
            idle.band_top(),
            "the turn's row did not grow the band, so this case proves nothing"
        );

        let mut band = Band::new();
        let mut screen = Screen::under_a_painted_band(&idle);
        let mut sink = Vec::new();
        band.commit(&mut Vec::new(), &band_rows(), &idle, (23, 2))
            .expect("the idle band");
        band.append_document(&mut sink, 1, &["say hello".to_string()], &idle)
            .expect("the prompt echo");
        screen.feed(&sink);
        sink.clear();
        assert_eq!(
            screen.row(idle.band_top() - 1),
            "say hello",
            "the echo was not written on the bottom document row"
        );

        // The turn is announced: the band grows, and the document is carried up
        // out of its way **before** anything is painted at the new top.
        band.carry_document(&mut sink, &running)
            .expect("the document is carried up");
        screen.feed(&sink);

        assert_eq!(
            screen.row(running.band_top() - 1),
            "say hello",
            "the echo is not on the bottom row of the document the band left"
        );
        assert!(
            screen.document().contains(&"say hello".to_string()),
            "the row the user submitted was lost: {:?}",
            screen.document()
        );
        // And the row the frame is about to erase from holds nothing of the
        // document's, which is the whole of what the carry buys.
        assert_ne!(
            screen.row(running.band_top()),
            "say hello",
            "the echo is still on the row every frame erases from"
        );

        // The row moved, so the band knows where it is now: a second growth
        // carries it again, and a band that recorded nothing would either carry
        // the same row twice or stop protecting it.
        let panelled = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let taller = crate::tui::layout::solve_with(24, 80, 3, true)
            .expect("a band with a three-row composer and a turn in it");
        assert!(
            taller.band_top() < running.band_top(),
            "the band did not grow again, so this case proves nothing"
        );
        let _ = panelled;
        sink.clear();
        band.carry_document(&mut sink, &taller)
            .expect("the document is carried up again");
        screen.feed(&sink);
        assert!(
            screen.document().contains(&"say hello".to_string()),
            "the row was lost on the second growth: {:?}",
            screen.document()
        );
        assert_eq!(
            screen.row(taller.band_top() - 1),
            "say hello",
            "the second carry did not put the row clear of the band"
        );
    }

    #[test]
    fn a_band_that_grows_over_rows_this_band_never_wrote_carries_nothing() {
        // Phase 1's accepted trade, and the bound that keeps the carry narrow.
        // The band shares the screen with the terminal's own document: a
        // composer that wraps as a draft is typed grows over whatever is above
        // it, and scrolling the user's screen on every keystroke that wrapped
        // would be a worse answer than covering a row this session never wrote.
        // Only a row **xfx placed and nothing will ever repaint** is carried.
        let idle = geometry();
        let grown = crate::tui::layout::solve(24, 80, 5).expect("a five-row composer");
        assert!(
            grown.band_top() < idle.band_top(),
            "the composer did not grow the band, so this case proves nothing"
        );
        let mut band = Band::new();
        let mut written = Vec::new();
        band.commit(&mut Vec::new(), &band_rows(), &idle, (23, 2))
            .expect("the idle band");

        band.carry_document(&mut written, &grown)
            .expect("a band that grew over the terminal's own document");
        assert!(
            written.is_empty(),
            "a band that grew over rows nothing here wrote scrolled the screen: {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    #[test]
    fn a_band_that_did_not_grow_carries_nothing_and_writes_nothing() {
        // The bound. A scroll is not reversible -- what leaves the top of the
        // screen is in the terminal's own scrollback for good -- so a carry on
        // a band that did not take a row would push the document up for nothing,
        // once per frame, forever.
        let idle = geometry();
        let mut band = Band::new();
        let mut written = Vec::new();
        band.commit(&mut Vec::new(), &band_rows(), &idle, (23, 2))
            .expect("the idle band");

        band.carry_document(&mut written, &idle)
            .expect("a band that did not grow");
        assert!(
            written.is_empty(),
            "a band that took no row scrolled the document anyway: {:?}",
            String::from_utf8_lossy(&written)
        );

        // And a band that *shrank* carries nothing either: those rows are
        // `release`'s, and it erases them rather than moving anything.
        let running =
            crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn in it");
        band.commit(&mut Vec::new(), &running_rows(), &running, (22, 2))
            .expect("the running band");
        band.carry_document(&mut written, &idle)
            .expect("a band that shrank");
        assert!(
            written.is_empty(),
            "a band that gave a row back scrolled the document: {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    #[test]
    fn a_screen_the_band_knows_nothing_about_is_not_carried() {
        // A resize, a `/clear`, a Ctrl-L or a resume: the terminal has re-wrapped
        // or erased its own document by rules this module does not model, so the
        // rows above the band are not this band's to move. `damaged` is the one
        // fact that says so, and a resize to a shorter screen is exactly the case
        // that would otherwise look like a band that grew.
        let tall = crate::tui::layout::solve(24, 80, 1).expect("a band");
        let short = crate::tui::layout::solve(12, 80, 1).expect("a shorter band");
        let mut band = Band::new();
        let mut written = Vec::new();
        band.commit(&mut Vec::new(), &band_rows(), &tall, (23, 2))
            .expect("the tall band");
        // A document row this band really wrote, so the absence below is the
        // damage rule biting rather than there being nothing to carry.
        band.append_document(&mut written, 1, &["say hello".to_string()], &tall)
            .expect("the prompt echo");
        written.clear();
        assert!(
            band.painted_top().is_some_and(|top| top > short.band_top()),
            "the screen did not shrink past the band's top, so this proves nothing"
        );

        band.invalidate(short.rows, short.cols);
        band.carry_document(&mut written, &short)
            .expect("a screen the band knows nothing about");
        assert!(
            written.is_empty(),
            "a resized screen's document was scrolled by a band that cannot describe it: {:?}",
            String::from_utf8_lossy(&written)
        );

        // **And still nothing once the whole repaint has landed.** `damaged` is
        // cleared by the frame that answers it, and what must not survive that
        // frame is the *row number*: it was a row on the screen that was, and on
        // this one it names a row the band never wrote. A band that kept it
        // would scroll the user's document by the difference between two
        // screens the moment anything grew.
        band.commit(&mut Vec::new(), &band_rows(), &short, (short.rows, 2))
            .expect("the whole repaint the resize asked for");
        let taller = crate::tui::layout::solve(12, 80, 3).expect("a three-row composer");
        assert!(
            taller.band_top() < short.band_top(),
            "the band did not grow on the new screen, so this case proves nothing"
        );
        band.carry_document(&mut written, &taller)
            .expect("a band that grew on a screen it has only just learned");
        assert!(
            written.is_empty(),
            "a row number from the screen that was outlived the repaint: {:?}",
            String::from_utf8_lossy(&written)
        );
    }

    // -----------------------------------------------------------------------
    // the other plane
    // -----------------------------------------------------------------------

    /// `CSI ? 1049 h` and `CSI ? 1049 l`, spelled here rather than imported for
    /// the reason every needle in this module's tests is: a test that read the
    /// constant it is checking would pass for whatever the module declared.
    const ENTERS_ALTERNATE: &str = "\u{1b}[?1049h";
    const LEAVES_ALTERNATE: &str = "\u{1b}[?1049l";

    /// A full-screen approval surface, as `super::super::approval_screen` builds
    /// one: as many rows as the terminal has.
    fn screen_rows(rows: u16) -> Vec<String> {
        (0..rows).map(|row| format!("screen row {row}")).collect()
    }

    /// A band that has painted one ordinary frame on the primary plane, and the
    /// screen it painted it on.
    fn painted_primary() -> (Band, Geometry) {
        let geometry = geometry();
        let mut band = Band::new();
        band.commit(&mut Vec::new(), &band_rows(), &geometry, (23, 2))
            .expect("a frame the screen took");
        (band, geometry)
    }

    #[test]
    fn entering_the_alternate_screen_asks_for_it_and_paints_the_whole_of_it() {
        // The enter is one frame: the mode set that takes the plane, and then a
        // screen painted from its first row. A `1049h` on its own would show
        // the user whatever the terminal's alternate buffer happened to be
        // holding until the next tick got round to painting one.
        let (mut band, geometry) = painted_primary();
        let frame = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("a vector the check accepted");
        let text = String::from_utf8(frame.bytes().to_vec()).expect("utf-8");

        assert_eq!(
            frame.owner(),
            super::super::shell::ScreenOwner::Approval,
            "the frame that takes the plane does not say whose it is"
        );
        assert!(
            text.starts_with(ENTERS_ALTERNATE),
            "the plane was painted before it was taken: {text:?}"
        );
        assert_eq!(
            text.matches(ENTERS_ALTERNATE).count(),
            1,
            "the alternate screen was entered more than once in one frame: {text:?}"
        );
        // And the buffer it was handed is **erased** before anything is placed
        // on it: a terminal hands out its alternate screen holding whatever was
        // last on it, and rows this band never wrote are rows it cannot
        // describe.
        assert!(
            text.contains(&format!("\u{1b}[1;1H{ERASE_SCREEN}")),
            "the plane was painted without being cleared first: {text:?}"
        );
        for row in 1..=geometry.rows {
            let painted = format!("\u{1b}[{row};1Hscreen row {}", row - 1);
            assert!(
                text.contains(&painted),
                "row {row} of the alternate screen was not painted: {text:?}"
            );
        }
    }

    #[test]
    fn alternate_frames_never_update_band_painted() {
        // `Band::painted` is the **normal** buffer's top row, and it is what
        // `super::super::term::shutdown` clears from. A frame on the other
        // plane that moved it would make the exit erase from a row number that
        // means nothing on the screen the user is left looking at -- taking the
        // shell's own output with it.
        let (mut band, geometry) = painted_primary();
        let before = band.painted_top();
        assert_eq!(before, Some(geometry.band_top()));

        let entered = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        assert_eq!(band.painted_top(), before, "the enter moved the band's top");

        band.repaint_alternate(&screen_rows(geometry.rows), &geometry, (8, 0))
            .expect("a vector the check accepted");
        assert_eq!(
            band.painted_top(),
            before,
            "a repaint on the other plane moved the band's top"
        );
    }

    #[test]
    fn a_repaint_the_other_plane_already_holds_costs_nothing() {
        // `Commit::NoChange` on the other plane. The band asks for a frame
        // twice a second while a turn is running, and a question is up for as
        // long as a person takes to read a change -- so a repaint that did not
        // check would write a whole unchanged screen, forever, on whatever link
        // the session is on.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);
        let entered = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        assert!(!entered.bytes().is_empty(), "the enter wrote nothing");

        assert!(
            band.repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "a screen the terminal already holds was written again"
        );
        // And a marker that moved is a frame, so the skip is a skip rather than
        // a surface that stopped painting.
        assert!(
            !band
                .repaint_alternate(&rows, &geometry, (8, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "the caret moved and nothing was written"
        );
        let mut moved = rows.clone();
        moved[3] = "> 2. Yes, and".to_string();
        assert!(
            !band
                .repaint_alternate(&moved, &geometry, (8, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "a row changed and nothing was written"
        );
    }

    #[test]
    fn a_repaint_after_damage_is_never_suppressed_by_what_the_plane_used_to_hold() {
        // [`Band::invalidate`] is this band saying it knows nothing about the
        // screen -- a resize, a `/clear`, a Ctrl-L, or the terminal being handed
        // back to the shell by a stop and taken again. What it must not leave
        // behind is a claim about the *other* plane's cells: the skip in
        // `repaint_alternate` is an equality against that claim, so a cache that
        // survived the damage would answer "the terminal already holds this"
        // about a screen the band has just said it cannot describe -- and the
        // repaint the damage asked for would be suppressed.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);
        let entered = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        assert!(
            band.repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "the screen did not already hold this, so the case below proves nothing"
        );

        band.invalidate(geometry.rows, geometry.cols);

        assert!(
            !band
                .repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "a damaged plane was left believing what it used to hold, so the \
             repaint the damage asked for was skipped"
        );
    }

    #[test]
    fn a_surface_the_band_only_built_is_not_recorded_as_the_screens() {
        // The cache is a claim about what the *terminal* is holding, so a frame
        // no writer has seen may not make it. `super::super::event_loop`'s
        // alternate paths record nothing on `Err`, and a build that recorded
        // itself would leave the band believing a surface that never left the
        // process was up -- and the retry the failure asked for skipped.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);

        let entering = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        assert!(!entering.bytes().is_empty(), "the enter wrote nothing");
        // No `frame_landed`: exactly what the loop does on a refused write.

        assert!(
            !band
                .repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "the build-time cache made the band believe an undelivered surface \
             was up"
        );
    }

    #[test]
    fn a_repaint_the_screen_refused_is_written_again_rather_than_skipped() {
        // The same defect on the repaint, where it costs the frame *and* the
        // failure budget: the loop turns empty bytes into a succeeded frame, so
        // a repaint recorded at build time and then refused is skipped on the
        // next tick and reported as a frame the session never wrote.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);

        let entering = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        let mut screen = Counted::default();
        screen.emit(entering.bytes()).expect("the enter landed");
        band.frame_landed(&entering, &geometry, (7, 0));

        let mut moved = rows.clone();
        moved[0] = "the marker moved".to_string();
        let refused = band
            .repaint_alternate(&moved, &geometry, (7, 0))
            .expect("a vector the check accepted");
        assert!(
            !refused.bytes().is_empty(),
            "the moved marker was not a frame"
        );
        Refuses
            .emit(refused.bytes())
            .expect_err("the screen took it");

        assert!(
            !band
                .repaint_alternate(&moved, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "the refused surface was cached as delivered, so the retry was \
             skipped and the session reported a frame it never wrote"
        );
    }

    #[test]
    fn a_surface_the_screen_took_is_not_written_a_second_time() {
        // The other half of the same rule, and the one the 2 Hz animation rests
        // on: a frame that *did* land is the screen's, and an empty frame that
        // lands after it re-asserts nothing -- it has no surface of its own to
        // adopt, so what the band already believes is left alone.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);

        let entering = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        let mut screen = Counted::default();
        screen.emit(entering.bytes()).expect("the enter landed");
        band.frame_landed(&entering, &geometry, (7, 0));

        let idle = band
            .repaint_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        assert!(
            idle.bytes().is_empty(),
            "an unchanged surface was repainted"
        );

        band.frame_landed(&idle, &geometry, (7, 0));
        assert!(
            band.repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "landing an empty frame disturbed the cache"
        );
    }

    #[test]
    fn a_repaint_the_screen_took_is_not_written_a_second_time_either() {
        // The skip has to survive a *repaint* landing, not only the enter: the
        // marker moves several times while a question is up, and each move is a
        // repaint whose surface becomes what the screen holds. A repaint that
        // painted and adopted nothing would leave the band comparing against
        // the enter's surface for ever -- a full screen written twice a second
        // for as long as the person reads the change, which is the cost
        // `repaint_alternate` exists to avoid.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);

        let entering = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        let mut screen = Counted::default();
        screen.emit(entering.bytes()).expect("the enter landed");
        band.frame_landed(&entering, &geometry, (7, 0));

        let mut moved = rows.clone();
        moved[3] = "> 2. Yes, and".to_string();
        let painted = band
            .repaint_alternate(&moved, &geometry, (7, 0))
            .expect("a vector the check accepted");
        assert!(!painted.bytes().is_empty(), "the moved row was not a frame");
        screen.emit(painted.bytes()).expect("the repaint landed");
        band.frame_landed(&painted, &geometry, (7, 0));

        assert!(
            band.repaint_alternate(&moved, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "a surface the screen took as a repaint was written again"
        );
    }

    #[test]
    fn an_empty_frame_that_lands_after_damage_does_not_bring_the_cache_back() {
        // Why an empty repaint carries no surface rather than the rows it was
        // handed: the band can be damaged between the build and the landing
        // (`Band::invalidate` -- a resize, a `/clear`), and a no-op that adopted
        // on the way in would answer "the terminal already holds this" about a
        // screen the band has just said it cannot describe.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);
        let entering = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entering, &geometry, (7, 0));

        let idle = band
            .repaint_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        assert!(
            idle.bytes().is_empty(),
            "the screen did not already hold this, so the case below proves nothing"
        );
        band.invalidate(geometry.rows, geometry.cols);
        band.frame_landed(&idle, &geometry, (7, 0));

        assert!(
            !band
                .repaint_alternate(&rows, &geometry, (7, 0))
                .expect("a vector the check accepted")
                .bytes()
                .is_empty(),
            "a frame that wrote nothing brought back a cache the damage cleared"
        );
    }

    #[test]
    fn a_plane_the_terminal_gave_back_behind_the_bands_back_is_taken_again() {
        // The stop handler is the one writer that is not a frame. `SIGTSTP`
        // lands, `super::super::signals`'s `stop_for_job_control` writes the
        // abnormal restore -- which leads with `1049l` -- and the process stops
        // with the user's own screen back. `super::super::resume` re-announces
        // the mode set and it carries no `1049h`, so the terminal is on the
        // normal buffer while this band still believes it is on the other one.
        //
        // Told, the band takes the plane again from scratch; not told, its next
        // frame is either nothing at all (the cache says the screen holds this
        // already) or a full-screen erase and repaint **on the user's own
        // buffer**.
        let (mut band, geometry) = painted_primary();
        let rows = screen_rows(geometry.rows);
        let entered = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        assert!(band.on_alternate());

        band.plane_given_back();

        assert!(
            !band.on_alternate(),
            "the band still believes it is on a plane the handler gave back"
        );
        let retaken = band
            .enter_alternate(&rows, &geometry, (7, 0))
            .expect("a vector the check accepted");
        let text = String::from_utf8(retaken.bytes().to_vec()).expect("utf-8");
        assert!(
            text.starts_with(ENTERS_ALTERNATE),
            "the plane was not taken again: {text:?}"
        );
        assert!(
            text.contains("screen row 0"),
            "the plane was taken again and nothing was painted on it: {text:?}"
        );
    }

    #[test]
    fn restore_primary_composes_1049l_hidden_cursor_and_a_complete_repaint_in_one_screen_frame() {
        // One vector, in this order, or the user sees the terminal's own
        // restored buffer -- or a blank one -- for as long as it takes the next
        // write to arrive.
        let (mut band, geometry) = painted_primary();
        let entered = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));

        let frame = band
            .restore_primary(&band_rows(), &geometry, (23, 2))
            .expect("a vector the check accepted");
        let text = String::from_utf8(frame.bytes().to_vec()).expect("utf-8");

        assert_eq!(
            frame.owner(),
            super::super::shell::ScreenOwner::Primary,
            "the frame that gives the plane back does not say whose it is"
        );
        let leaves = text.find(LEAVES_ALTERNATE).expect("the leave");
        assert_eq!(
            leaves, 0,
            "something was written before the leave: {text:?}"
        );
        let hidden = text.find("\u{1b}[?25l").expect("the cursor is hidden");
        assert!(
            leaves < hidden,
            "the cursor was hidden on the plane being left: {text:?}"
        );
        // The **whole** band, not a difference from a shadow: the terminal has
        // been showing another plane, and a diff against what it held before it
        // was taken is a claim nothing on this path can make.
        for (offset, row) in band_rows().iter().enumerate() {
            let line = geometry.band_top() + u16::try_from(offset).expect("a band row");
            let painted = format!("\u{1b}[{line};1H{row}");
            assert!(
                text.contains(&painted),
                "row {line} of the band was not repainted by the restore: {text:?}"
            );
            assert!(
                text.find(&painted).expect("the row") > hidden,
                "the band was repainted before the cursor was hidden: {text:?}"
            );
        }
        assert!(
            text.ends_with(END_FRAME),
            "the restore did not close its frame: {text:?}"
        );
        assert_eq!(
            text.matches(LEAVES_ALTERNATE).count(),
            1,
            "the restore left the alternate screen more than once: {text:?}"
        );
    }

    #[test]
    fn a_restore_names_the_row_the_exit_clears_from_before_it_is_written() {
        // The rule every frame in this module keeps, on the one path that grew
        // a second writer. `Band::painted` is lowered **before** the bytes go
        // out, because an emit can fail with some of them delivered: a
        // restore that recorded nothing until it landed would leave the exit
        // clearing from a row *below* the rows it had already painted, and the
        // top of the band would survive the exit on the user's screen.
        //
        // Driven through a band that **grew** while the question was up -- the
        // composer took a second row -- because that is the direction in which
        // the two answers differ: the new top is above the old one.
        let short = crate::tui::layout::solve(24, 80, 1).expect("a one-row composer");
        let tall = crate::tui::layout::solve(24, 80, 2).expect("a two-row composer");
        let mut band = Band::new();
        band.commit(&mut Vec::new(), &band_rows(), &short, (23, 2))
            .expect("a frame the screen took");
        assert_eq!(band.painted_top(), Some(short.band_top()));

        let entered = band
            .enter_alternate(&screen_rows(short.rows), &short, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &short, (7, 0));
        let grown = vec![
            "--".to_string(),
            "> ".to_string(),
            "second".to_string(),
            "hint".to_string(),
        ];
        band.restore_primary(&grown, &tall, (23, 2))
            .expect("a vector the check accepted");

        assert_eq!(
            band.painted_top(),
            Some(tall.band_top()),
            "the restore painted rows the exit would not have cleared from"
        );
        assert!(
            tall.band_top() < short.band_top(),
            "the band did not grow, so this case proves nothing"
        );
    }

    #[test]
    fn restoration_never_shows_an_intermediate_blank_grid() {
        // Every state a terminal can be in between the leave and the repaint is
        // a state this vector puts it in, because there is nothing else on the
        // wire until the whole of it has been written. So the property is
        // checked the only way it is checkable: no prefix of the restore ends
        // with the plane given back and the band not yet on it.
        let (mut band, geometry) = painted_primary();
        let entered = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        let frame = band
            .restore_primary(&band_rows(), &geometry, (23, 2))
            .expect("a vector the check accepted");

        // The frame is wrapped in synchronized output, which is what makes the
        // whole of it one presentation on a terminal that supports it -- and
        // the `1049l` is inside that wrapper rather than in front of it.
        let text = String::from_utf8(frame.bytes().to_vec()).expect("utf-8");
        assert!(
            text.contains(BEGIN_FRAME),
            "the restore was not presented as one frame: {text:?}"
        );
        let opened = text.find(BEGIN_FRAME).expect("the frame opens");
        let closed = text.rfind(END_FRAME).expect("the frame closes");
        for row in band_rows() {
            let at = text.find(&row).expect("a band row");
            assert!(
                opened < at && at < closed,
                "{row:?} was painted outside the frame that presents it: {text:?}"
            );
        }
    }

    #[test]
    fn the_band_the_restore_repaints_is_the_one_the_exit_then_clears_from() {
        // The restore is a **whole** frame, so once it has landed the band knows
        // exactly what is on those rows again: the shadow is what it painted,
        // the top is its own top row, and the next ordinary frame is a
        // difference from that rather than a second whole repaint.
        let (mut band, geometry) = painted_primary();
        let entered = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .expect("a vector the check accepted");
        band.frame_landed(&entered, &geometry, (7, 0));
        assert!(band.on_alternate(), "the band forgot which plane it is on");

        let restored = band
            .restore_primary(&band_rows(), &geometry, (23, 2))
            .expect("a vector the check accepted");
        band.frame_landed(&restored, &geometry, (23, 2));
        assert!(
            !band.on_alternate(),
            "the band still believes it is on the plane it just gave back"
        );
        assert_eq!(band.painted_top(), Some(geometry.band_top()));

        let mut screen = Vec::new();
        assert_eq!(
            band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
                .expect("a frame the screen took"),
            Commit::NoChange,
            "the restore did not leave the band knowing what it had painted"
        );
    }

    #[test]
    fn an_untampered_band_writes_exactly_the_frame_it_always_did() {
        // The seam's own falsifier: with no hand on the vector, the bytes a
        // frame writes are the bytes the whole-band painter builds for the same
        // facts. Without this the tests below could pass against a band whose
        // ordinary output the seam had quietly changed.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a frame the screen took");

        let mut reference = Band::new();
        let expected = reference.render(&band_rows(), &geometry, (23, 2));
        assert_eq!(screen.written, expected);
        assert_eq!(screen.writes, 1, "one write per frame");
    }

    #[test]
    fn a_frame_that_would_write_on_a_document_row_is_refused_before_the_write() {
        // The failure this whole check exists to prevent: a vector that
        // addresses a row above the band's top writes over the terminal's own
        // document, and Phase 1 never repaints one -- so the row is gone the
        // instant the bytes land. The refusal must therefore happen **before**
        // the write, not be noticed after it.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[3;1Hx"));
        let refused = band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .expect_err("a frame that wandered into the document");
        assert!(
            refused.to_string().contains("outside the footprint"),
            "the refusal did not name the footprint: {refused}"
        );
        assert_eq!(
            screen.writes, 1,
            "the frame that wandered into the document reached the screen"
        );
    }

    #[test]
    fn a_frame_whose_cursor_show_went_missing_is_refused_before_the_write() {
        // `?25h` is welded into `END_FRAME`, and a constant is exactly where a
        // regression hides: nothing else in the frame would notice a terminal
        // left with a hidden cursor for the rest of the session.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        band.tamper_with(|bytes| {
            let end = bytes.len() - "\u{1b}[?25h".len();
            bytes.truncate(end);
        });
        let refused = band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .expect_err("a frame that left the cursor hidden");
        assert!(
            refused.to_string().contains("cursor visibility"),
            "the refusal did not name the cursor: {refused}"
        );
        assert_eq!(screen.writes, 1, "the frame reached the screen anyway");
    }

    #[test]
    fn the_bytes_a_frame_is_checked_against_are_the_bytes_it_writes() {
        // The property a check of a locally reconstructed copy cannot have.
        // The tamper hook runs *before* the check, so a band that checked one
        // vector and wrote another would accept this frame and put the damaged
        // bytes on the screen.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[3;1Hx"));
        assert!(band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .is_err());
        assert!(
            !screen.written.ends_with(b"\x1b[3;1Hx"),
            "the damaged vector was written while a clean copy was checked"
        );
    }

    #[test]
    fn a_carry_that_scrolled_one_row_too_many_is_refused_before_the_write() {
        // A row that leaves the top of the screen is in the terminal's own
        // scrollback for good: there is no later frame that can take it back,
        // which is why the count is checked before the bytes go out.
        let running = crate::tui::layout::solve_with(24, 80, 1, true).expect("a band with a turn");
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        // The band grows by the row a starting turn takes, and the document row
        // under it is one this band wrote: exactly the case a carry exists for.
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        band.append_document(&mut screen, 1, &["answered".to_string()], &geometry)
            .expect("a document row");
        let writes = screen.writes;
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[24;1H\n"));
        let refused = band
            .carry_document(&mut screen, &running)
            .expect_err("a carry that scrolled one row too many");
        assert!(
            refused.to_string().contains("scroll"),
            "the refusal did not name the scroll: {refused}"
        );
        assert_eq!(screen.writes, writes, "the extra scroll reached the screen");
    }

    #[test]
    fn an_append_that_scrolled_one_row_too_many_is_refused_before_the_write() {
        // The same harm from the other writer: an append is a scroll, and a
        // linefeed nobody declared carries a document row off the top.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[24;1H\n"));
        let refused = band
            .append_document(&mut screen, 1, &["answered".to_string()], &geometry)
            .expect_err("an append that scrolled one row too many");
        assert!(
            refused.to_string().contains("scroll"),
            "the refusal did not name the scroll: {refused}"
        );
        assert_eq!(screen.writes, writes, "the extra scroll reached the screen");
    }

    #[test]
    fn an_alternate_frame_that_paints_something_else_is_refused() {
        // The surface is declared from the rows the caller handed over, so a
        // vector that painted a different screen -- one row short, one row
        // extra, or the same rows somewhere else -- disagrees with it.
        let geometry = geometry();
        let mut band = Band::new();
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[2;1Hlater"));
        let refused = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .err()
            .expect("a surface with something else on it");
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the cell: {refused}"
        );
    }

    #[test]
    fn an_alternate_frame_that_lost_its_cursor_show_is_refused() {
        let geometry = geometry();
        let mut band = Band::new();
        band.tamper_with(|bytes| {
            let end = bytes.len() - "\u{1b}[?25h".len();
            bytes.truncate(end);
        });
        let refused = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .err()
            .expect("a surface that left the cursor hidden");
        assert!(
            refused.to_string().contains("cursor visibility"),
            "the refusal did not name the cursor: {refused}"
        );
    }

    #[test]
    fn a_frame_carrying_an_unclaimed_scrollback_erase_is_refused_before_the_write() {
        // `ED 3` moves no cell, so no footprint and no cell comparison can see
        // it -- and what it destroys is the only copy of every document row this
        // phase has ever carried off the top of the screen. A frame declares a
        // band, a caret and a title; it does not declare this.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[3J"));
        let refused = band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .expect_err("a frame that erased the user's scrollback");
        assert!(
            refused.to_string().contains("scrollback"),
            "the refusal did not name the scrollback: {refused}"
        );
        assert_eq!(
            screen.writes, writes,
            "the scrollback erase reached the screen"
        );
    }

    #[test]
    fn a_frame_that_turns_a_terminal_mode_back_on_is_refused_before_the_write() {
        // Autowrap back on is what makes a band row written at the last column
        // scroll the terminal's own document. A frame declares no mode at all.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[?7h"));
        let refused = band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .expect_err("a frame that put autowrap back on");
        assert!(
            refused.to_string().contains("mode"),
            "the refusal did not name the mode: {refused}"
        );
        assert_eq!(screen.writes, writes);
    }

    #[test]
    fn a_frame_carrying_a_query_is_refused_before_the_write() {
        // A terminal's answer arrives on standard input, where the session
        // parses it as the user's typing: a frame that asked a question nobody
        // is waiting for puts an escape sequence into the composer.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;
        band.tamper_with(|bytes| bytes.extend_from_slice(b"\x1b[6n"));
        let refused = band
            .commit(&mut screen, &running_rows(), &geometry, (23, 2))
            .expect_err("a frame that asked a question");
        assert!(
            refused.to_string().contains("quer"),
            "the refusal did not name the query: {refused}"
        );
        assert_eq!(screen.writes, writes);
    }

    #[test]
    fn an_alternate_frame_that_writes_on_the_normal_buffer_first_is_refused() {
        // The `1049h` is the first thing in the vector, so a cell written in
        // front of it lands on the buffer the terminal is about to **save** --
        // and hands it back, with the stray cell on it, when the question is
        // answered. The band repaints its own rows there and nothing repaints
        // the document, so the cell stays.
        let geometry = geometry();
        let mut band = Band::new();
        band.tamper_with(|bytes| {
            let mut stray = b"\x1b[1;1HX".to_vec();
            stray.extend_from_slice(bytes);
            *bytes = stray;
        });
        let refused = band
            .enter_alternate(&screen_rows(geometry.rows), &geometry, (7, 0))
            .err()
            .expect("a frame that wrote on the buffer it was about to save");
        assert!(
            refused.to_string().contains("buffer") || refused.to_string().contains("plane"),
            "the refusal did not name the buffer: {refused}"
        );
    }

    #[test]
    fn an_append_onto_a_screen_the_band_has_not_framed_yet_is_still_compared() {
        // The shadow is `0x0` until the first frame sizes it, and an append
        // runs before the band's own frame in the tick. Comparing what the
        // shadow can describe compares nothing at all here -- on the one
        // emitter whose mistakes cannot be taken back, because its rows leave
        // the top of the screen into native scrollback.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            *bytes = text.replace("answered", "ANSWERED").into_bytes();
        });
        let refused = band
            .append_document(&mut screen, 1, &["answered".to_string()], &geometry)
            .expect_err("an append whose row is not the row it was handed");
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the cell: {refused}"
        );
        assert_eq!(screen.writes, 0, "the wrong row reached the document");

        // And the same append, untampered, is accepted and written.
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.append_document(&mut screen, 1, &["answered".to_string()], &geometry)
            .expect("the row it was handed");
        assert_eq!(screen.writes, 1);
    }

    #[test]
    fn an_append_onto_rows_a_grown_screen_added_is_still_compared() {
        // The shadow is resized by `invalidate`, which only `commit` calls --
        // and the document is written **before** the band in every tick. So a
        // terminal that grew between two frames leaves the append placing rows
        // the band's own target has no cells for.
        let short = geometry();
        let tall = crate::tui::layout::solve(40, 80, 1).expect("a taller band");
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &short, (23, 2))
            .expect("a first frame on the smaller screen");
        let writes = screen.writes;
        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            *bytes = text.replace("answered", "ANSWERED").into_bytes();
        });
        let refused = band
            .append_document(&mut screen, 1, &["answered".to_string()], &tall)
            .expect_err("an append onto rows the target cannot describe");
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the cell: {refused}"
        );
        assert_eq!(screen.writes, writes, "the wrong row reached the document");
    }

    #[test]
    fn the_screen_the_exit_clears_against_is_the_one_the_band_last_painted_for() {
        // `painted` is recorded from the geometry the write was built for, so
        // the screen it is a row **of** has to be that same geometry. Taking it
        // from the shadow instead disagrees exactly when an append has moved
        // `painted` onto a screen the shadow has not been resized to yet -- and
        // the exit's own cleanup line is then refused for addressing a row that
        // is really there.
        let short = geometry();
        let tall = crate::tui::layout::solve(40, 80, 1).expect("a taller band");
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &short, (23, 2))
            .expect("a first frame on the smaller screen");
        band.append_document(&mut screen, 1, &["answered".to_string()], &tall)
            .expect("a document row on the taller screen");
        assert_eq!(band.painted_top(), Some(tall.band_top()));
        assert_eq!(
            band.screen_size(),
            (tall.rows, tall.cols),
            "the exit would be measured against a screen the band has left"
        );
    }

    /// Thirty distinct document rows, so that a substitution in any one of them
    /// is visible and the nine that reach native scrollback are told apart.
    fn numbered_rows(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("row-{index:02}")).collect()
    }

    #[test]
    fn a_row_written_on_its_way_into_native_scrollback_is_still_compared() {
        // **The one direction nothing can take back.** An append that delivers
        // more rows than the document area holds paints each of them on the
        // bottom document row and scrolls; the first nine leave the top of the
        // screen during the very vector that wrote them. They are in the
        // terminal's own scrollback afterwards, where no later frame reaches,
        // so a row substituted on the way out is a row the user keeps for good.
        //
        // The final screen and the scroll count are identical either way, which
        // is the point: a check that compared only what is left at the end, or
        // only how far the screen moved, passes this.
        let geometry = geometry();
        assert_eq!(geometry.band_top(), 22, "the repro's own geometry");
        let rows = numbered_rows(30);
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;

        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            // An early row, which the scrolls that follow it carry off the top.
            assert!(text.contains("row-03"), "the repro's own row is not there");
            *bytes = text.replacen("row-03", "ROW-03", 1).into_bytes();
        });
        let refused = band
            .append_document(&mut screen, 30, &rows, &geometry)
            .expect_err("a row substituted on its way into scrollback");
        assert!(
            refused.to_string().contains("did not leave as intended")
                || refused.to_string().contains("carried off"),
            "the refusal did not name the row: {refused}"
        );
        assert_eq!(
            screen.writes, writes,
            "the substituted row reached the terminal's own scrollback"
        );
    }

    #[test]
    fn the_same_thirty_rows_untampered_are_accepted_and_written() {
        // The other half of the pair: the guarantee must not be bought by
        // refusing the long appends the product really makes.
        let geometry = geometry();
        let rows = numbered_rows(30);
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;
        band.append_document(&mut screen, 30, &rows, &geometry)
            .expect("thirty rows the emitter really wrote");
        assert_eq!(screen.writes, writes + 1, "the append was refused");
        let text = String::from_utf8_lossy(&screen.written).into_owned();
        assert!(
            text.contains("row-29") && text.contains("row-00"),
            "the append did not carry every row it was handed"
        );
    }

    #[test]
    fn a_row_harmed_before_it_is_evicted_is_refused_even_though_the_last_screen_matches() {
        // Ordering, not final state. The tamper writes on the bottom document
        // row **before** the first scroll -- inside the rows this append is
        // allowed to place on, so the footprint admits it -- and the twenty
        // scrolls behind it carry that row off the top. What is left on the
        // screen at the end is byte-for-byte what the untampered append leaves,
        // and the scroll count is unchanged; only the content of a row at the
        // moment it was evicted differs.
        let geometry = geometry();
        let rows = numbered_rows(30);
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame");
        let writes = screen.writes;

        band.tamper_with(|bytes| {
            let mut harmed = format!("\u{1b}[{};1HHARM", 21).into_bytes();
            harmed.extend_from_slice(bytes);
            *bytes = harmed;
        });
        let refused = band
            .append_document(&mut screen, 30, &rows, &geometry)
            .expect_err("a row harmed before the scroll that evicted it");
        assert!(
            refused.to_string().contains("did not leave as intended")
                || refused.to_string().contains("carried off"),
            "the refusal did not name the row: {refused}"
        );
        assert_eq!(screen.writes, writes, "the harmed row reached scrollback");
    }

    // -- The document's visible cells, recoloured (P3-THEME). --
    //
    // A theme report moves the palette, and this phase never repaints a
    // document row: without these bytes every answer already on the screen
    // stays in the greys of the terminal the user *had*. What the emitter may
    // do is narrow -- colours, above the band, no scroll, nothing created --
    // and these say so at the level the bytes are decided.

    /// The document's two owned greys, dark and light, at the depth these
    /// cases paint in. Spelled out rather than asked of `theme`: a test built
    /// from the accessor it is checking passes for whatever that accessor says.
    const BODY_DARK: &str = "\u{1b}[38;5;255m";
    const BODY_LIGHT: &str = "\u{1b}[38;5;235m";
    const NOTICE_DARK: &str = "\u{1b}[38;5;250m";
    const NOTICE_LIGHT: &str = "\u{1b}[38;5;241m";
    /// What ends either of them.
    const ENDED: &str = "\u{1b}[0m";

    fn dark_palette() -> super::super::theme::Palette {
        super::super::theme::Palette {
            mode: super::super::theme::Mode::Dark,
            depth: super::super::theme::Depth::Ansi256,
        }
    }

    fn light_palette() -> super::super::theme::Palette {
        super::super::theme::Palette {
            mode: super::super::theme::Mode::Light,
            depth: super::super::theme::Depth::Ansi256,
        }
    }

    /// A band whose shadow holds one answer row, one of xfx's own lines and
    /// the echo of a prompt, with the caret where the last frame left it.
    ///
    /// The frame comes first because it is what sizes the shadow: rows
    /// appended before a session has painted anything are on the screen
    /// without the band ever having recorded them.
    fn a_document_this_band_painted(geometry: &Geometry) -> (Band, Counted) {
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), geometry, (23, 2))
            .expect("a first frame sizes the shadow");
        band.append_document(
            &mut screen,
            3,
            &[
                "hello".to_string(),
                format!("{BODY_DARK}an answer{ENDED}"),
                format!("{NOTICE_DARK}[tool] ran{ENDED}"),
            ],
            geometry,
        )
        .expect("the document lands");
        band.commit(&mut screen, &band_rows(), geometry, (23, 2))
            .expect("the frame the append owed");
        screen.written.clear();
        (band, screen)
    }

    #[test]
    fn a_retint_writes_the_documents_own_cells_and_puts_the_caret_back() {
        // The whole vector, byte for byte: a `CUP` and the cells of each row
        // the palette moved, the pen closed behind them, and the caret back
        // where the last frame left it. **No linefeed** -- a scroll here would
        // put a row into native scrollback that nothing can take back -- and
        // nothing on the row the user's own echo is on.
        let geometry = geometry();
        let (mut band, mut screen) = a_document_this_band_painted(&geometry);
        let writes = screen.writes;

        assert_eq!(
            band.retint_document(&mut screen, &light_palette(), &geometry)
                .expect("a screen that takes everything"),
            Retint::Settled(Commit::Painted)
        );

        let written = String::from_utf8(screen.written.clone()).expect("text and escapes");
        assert_eq!(
            written,
            format!(
                // The reset in the middle is [`Grid::diff`]'s: it replays a
                // state rather than a delta, so one colour is closed before
                // the next is opened -- and the last one is closed at the end,
                // which is what leaves the terminal in the default state the
                // next frame assumes.
                "\u{1b}[20;1H{BODY_LIGHT}an answer\
                 \u{1b}[21;1H{ENDED}{NOTICE_LIGHT}[tool] ran{ENDED}\u{1b}[23;3H"
            ),
            "the recolour is not the cells, the pen and the caret"
        );
        assert_eq!(
            screen.writes - writes,
            1,
            "a recolour cost more than one vector"
        );
    }

    #[test]
    fn a_second_report_finds_the_screen_already_holding_what_it_asks_for() {
        // The way back costs nothing, and it is the mapping that makes it so:
        // both greys map onto the mode in force, so a session reported light
        // and then dark again is a session whose cells are already dark.
        let geometry = geometry();
        let (mut band, mut screen) = a_document_this_band_painted(&geometry);
        band.retint_document(&mut screen, &light_palette(), &geometry)
            .expect("the way out");
        screen.written.clear();

        assert_eq!(
            band.retint_document(&mut screen, &dark_palette(), &geometry)
                .expect("the way back"),
            Retint::Settled(Commit::Painted)
        );
        screen.written.clear();
        assert_eq!(
            band.retint_document(&mut screen, &dark_palette(), &geometry)
                .expect("the same palette again"),
            Retint::Settled(Commit::NoChange),
            "a report that said what the screen already shows wrote bytes"
        );
        assert!(screen.written.is_empty());
    }

    #[test]
    fn a_recolour_the_screen_refused_is_owed_again_rather_than_recorded_as_done() {
        // The rule every emitter here is under: nothing is recorded until the
        // write lands. A shadow advanced by a refused vector believes the
        // document is in a palette it never reached, and never writes it again.
        let geometry = geometry();
        let (mut band, _) = a_document_this_band_painted(&geometry);

        band.retint_document(&mut Refuses, &light_palette(), &geometry)
            .expect_err("a screen that refuses everything");

        let mut screen = Counted::default();
        assert_eq!(
            band.retint_document(&mut screen, &light_palette(), &geometry)
                .expect("the retry"),
            Retint::Settled(Commit::Painted),
            "the refused recolour was recorded as if it had landed"
        );
        assert!(
            String::from_utf8_lossy(&screen.written).contains(&format!("{BODY_LIGHT}an answer")),
            "the retry wrote something other than the recolour it still owed"
        );
    }

    #[test]
    fn a_recolour_that_would_write_on_the_bands_own_rows_is_refused_before_the_write() {
        // The check is not vacuous. Every vector this emitter builds stays
        // above the band by construction, so the claim "it may not reach the
        // band's rows" is only testable against one that does -- and a
        // placement on the divider is refused before a byte goes out.
        let geometry = geometry();
        let (mut band, mut screen) = a_document_this_band_painted(&geometry);
        let writes = screen.writes;
        band.tamper_with(|bytes| {
            let mut damaged = format!("\u{1b}[{};1Hx", 22).into_bytes();
            damaged.append(bytes);
            *bytes = damaged;
        });

        let refused = band
            .retint_document(&mut screen, &light_palette(), &geometry)
            .expect_err("a recolour that strayed onto the band");
        assert!(
            refused.to_string().contains("outside the footprint")
                || refused.to_string().contains("did not declare"),
            "the refusal did not name the stray row: {refused}"
        );
        assert_eq!(screen.writes, writes, "the refused vector reached the wire");
    }

    #[test]
    fn a_band_that_cannot_describe_the_screen_defers_and_one_with_no_document_settles() {
        // Writing nothing is two different answers and the caller must be able
        // to tell them apart. **Deferred**: the shadow is not a claim about
        // this screen -- a `/clear`, a Ctrl-L, a resume or a resize got here
        // first -- so the recolour is still owed and the frame that repairs the
        // screen is what makes the next tick able to answer it. **Settled**:
        // the band can describe the screen and owns no document row on it, so
        // there is nothing to owe. A single "nothing happened" would either
        // drop a recolour the user is owed or leave one owed for ever.
        let geometry = geometry();
        let (mut band, mut screen) = a_document_this_band_painted(&geometry);
        band.invalidate(geometry.rows, geometry.cols);
        assert_eq!(
            band.retint_document(&mut screen, &light_palette(), &geometry)
                .expect("a damaged band"),
            Retint::Deferred
        );

        // A band that has painted nothing at all: its shadow is not this
        // screen's size, so it cannot say anything about these rows either.
        let mut fresh = Band::new();
        assert_eq!(
            fresh
                .retint_document(&mut screen, &light_palette(), &geometry)
                .expect("a band that has written nothing"),
            Retint::Deferred
        );

        // And a band whose frames have landed but which has never put a
        // document row on the screen: the rows above it are the terminal's
        // own, this phase does not own a shell's output, and nothing is owed.
        let mut painted = Band::new();
        painted
            .commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a frame");
        screen.written.clear();
        assert_eq!(
            painted
                .retint_document(&mut screen, &light_palette(), &geometry)
                .expect("a band with no document of its own"),
            Retint::Settled(Commit::NoChange)
        );
        assert!(
            screen.written.is_empty(),
            "a band that knows nothing about the document wrote: {:?}",
            String::from_utf8_lossy(&screen.written)
        );
    }

    // -- Settled-row reuse (P3-WRAP: frame.rs/grid.rs bounded unit). --
    //
    // A settled row is reused -- placed once, then trusted across appends
    // that repeat it verbatim -- only when nothing this frame knows about
    // the screen has moved since the row was last verified there. These
    // cover the emitted bytes (a match is skipped, a real change is not),
    // the state a fully-reused append must leave untouched, the facts that
    // withdraw the trust, and the independent checker's refusal to accept
    // a claim about a row -- reused or not -- that the real bytes disagree
    // with.

    #[test]
    fn a_settled_row_unchanged_since_the_last_append_is_reused_and_a_changed_one_is_repainted() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");

        let first = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "marker-C".to_string(),
        ];
        band.append_document(&mut screen, 0, &first, &geometry)
            .expect("the first settled block lands");

        screen.written.clear();
        let second = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "CHANGED".to_string(),
        ];
        band.append_document(&mut screen, 0, &second, &geometry)
            .expect("the reused settled block lands");

        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            !text.contains("marker-A") && !text.contains("marker-B"),
            "an unchanged settled row was re-emitted though nothing about it \
             differed: {text:?}"
        );
        assert!(
            text.contains("CHANGED"),
            "the row that really changed was not repainted: {text:?}"
        );

        // The screen this leaves is still the one three literal placements
        // would leave: reuse skips the *bytes*, never the cell they stand
        // for. `reference` is `band.shadow` everywhere reuse cannot have
        // touched, and a hand-placed literal on the three rows under test.
        let mut reference = band.shadow.clone();
        reference.place_row(19, "marker-A", &geometry);
        reference.place_row(20, "marker-B", &geometry);
        reference.place_row(21, "CHANGED", &geometry);
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&reference, &geometry, &mut owed),
            0,
            "the reused row's cells were not what a real placement would have \
             left: owed {:?}",
            String::from_utf8_lossy(&owed)
        );
    }

    #[test]
    fn an_append_whose_settled_rows_all_match_writes_nothing_and_leaves_band_state_untouched() {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");

        let rows = vec!["marker-A".to_string(), "marker-B".to_string()];
        band.append_document(&mut screen, 0, &rows, &geometry)
            .expect("the settled block lands");

        let shadow_before = band.shadow.clone();
        let painted_before = band.painted_top();
        let document_bottom_before = band.document_bottom;
        let caret_before = band.caret;

        screen.written.clear();
        let writes_before = screen.writes;
        band.append_document(&mut screen, 0, &rows, &geometry)
            .expect("a fully-reused append is still accepted");

        assert_eq!(
            screen.writes, writes_before,
            "an append every one of whose rows already matched still wrote a \
             vector"
        );
        assert!(screen.written.is_empty());
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&shadow_before, &geometry, &mut owed),
            0,
            "a no-op append changed what the shadow believes the screen holds"
        );
        assert_eq!(band.painted_top(), painted_before);
        assert_eq!(band.document_bottom, document_bottom_before);
        assert_eq!(band.caret, caret_before);
    }

    #[test]
    fn invalidating_between_appends_disables_reuse_and_repaints_every_settled_row() {
        // `invalidate` also blanks the shadow unconditionally (`Grid::resize`),
        // so this exercises the combined effect a Ctrl-L or a stop's resume
        // really leaves behind -- `damaged` alone is not separable from that
        // blanking through this public path, and this is the path production
        // takes.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");

        let rows = vec!["marker-A".to_string(), "marker-B".to_string()];
        band.append_document(&mut screen, 0, &rows, &geometry)
            .expect("the settled block lands");

        band.invalidate(geometry.rows, geometry.cols);

        screen.written.clear();
        band.append_document(&mut screen, 0, &rows, &geometry)
            .expect("the append after invalidation lands");
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            text.contains("marker-A") && text.contains("marker-B"),
            "a row identical to the pre-invalidation text was reused across an \
             invalidation, though nothing here still knows that to be true: \
             {text:?}"
        );
    }

    #[test]
    fn a_band_top_that_moved_since_the_last_append_disables_reuse() {
        let base = geometry();
        let taller = crate::tui::layout::solve(24, 80, 3).expect("a taller composer");
        assert_ne!(
            taller.band_top(),
            base.band_top(),
            "the repro's own composer did not move the band top"
        );

        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &base, (23, 2))
            .expect("a first frame clears `damaged`");
        let rows = vec!["marker-A".to_string(), "marker-B".to_string()];
        band.append_document(&mut screen, 0, &rows, &base)
            .expect("the settled block lands at the smaller composer's band top");

        // The composer grows -- more input rows -- without any invalidation:
        // the document's own cells the first append wrote are untouched.
        band.commit(&mut screen, &band_rows(), &taller, (23, 2))
            .expect("the taller composer's own frame");

        screen.written.clear();
        band.append_document(&mut screen, 0, &rows, &taller)
            .expect("the append at the new band top lands");
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            text.contains("marker-A") && text.contains("marker-B"),
            "a settled row was reused across a band top that moved: {text:?}"
        );
    }

    #[test]
    fn a_changed_row_whose_bytes_were_stripped_is_still_caught_by_the_independent_script() {
        // The reuse decision never touches this row -- it differs, so the
        // emitter places it same as before the feature existed. The strip is
        // a fault this unit does not cause, and the independent script has
        // to catch it regardless.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");
        let first = vec!["marker-A".to_string(), "marker-B".to_string()];
        band.append_document(&mut screen, 0, &first, &geometry)
            .expect("the first settled block lands");

        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            let placement = format!("\u{1b}[21;1HCHANGED{ERASE_LINE}");
            assert!(
                text.contains(&placement),
                "the repro's own changed-row placement is not there: {text:?}"
            );
            *bytes = text.replacen(&placement, "", 1).into_bytes();
        });
        let second = vec!["marker-A".to_string(), "CHANGED".to_string()];
        let refused = band
            .append_document(&mut screen, 0, &second, &geometry)
            .expect_err("a row the script still declares must be caught even stripped");
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the missing cell: {refused}"
        );
    }

    #[test]
    fn a_reused_row_the_emitted_bytes_corrupt_is_still_caught_by_the_independent_script() {
        // `marker-A` is reused this call -- the emitter writes nothing for
        // it -- so the bytes the tamper adds here are a fault an external
        // layer injected onto a row this feature trusted without a write,
        // not a placement this emitter forgot.
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");
        let first = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "marker-C".to_string(),
        ];
        band.append_document(&mut screen, 0, &first, &geometry)
            .expect("the first settled block lands");

        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            assert!(
                !text.contains("marker-A"),
                "the repro's own reused row was placed by the real emitter: {text:?}"
            );
            let mut corrupted = format!("\u{1b}[19;1HRUINED{ERASE_LINE}").into_bytes();
            corrupted.extend_from_slice(bytes);
            *bytes = corrupted;
        });
        let second = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "CHANGED".to_string(),
        ];
        let refused = band
            .append_document(&mut screen, 0, &second, &geometry)
            .expect_err(
                "a corrupted reused row must still be caught even though it was \
                 never written",
            );
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the corrupted cell: {refused}"
        );
    }

    /// Regression characterization, not TDD-first: `render_append` already
    /// rebuilds `target` from `shadow` at the top of every call
    /// (`frame.rs:1085`), so a refused attempt's speculative `target` writes
    /// cannot leak into a retry -- this test only makes that existing
    /// behavior observable, no production line changed to make it pass.
    ///
    /// The tamper strips the changed row's own placement, which
    /// `check::preflight` must still catch (`frame.rs:1593-1616`, strictly
    /// before `out.emit` at `frame.rs:1617`): the sink is never called and
    /// `shadow` is never swapped in. Clearing the tamper through the
    /// existing seam (`Band.tamper`, `#[cfg(test)]`, a private field `mod
    /// tests` can reach directly) and retrying the identical append must
    /// then repaint only the row that changed -- the two unchanged rows stay
    /// reused, omitted from what the sink receives.
    #[test]
    fn a_preflight_refusal_leaves_shadow_untouched_and_a_cleared_retry_lands_only_the_changed_row()
    {
        let geometry = geometry();
        let mut band = Band::new();
        let mut screen = Counted::default();
        band.commit(&mut screen, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");
        let first = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "marker-C".to_string(),
        ];
        band.append_document(&mut screen, 0, &first, &geometry)
            .expect("the first settled block lands");

        let shadow_before = band.shadow.clone();
        screen.written.clear();
        let writes_before = screen.writes;

        band.tamper_with(|bytes| {
            let text = String::from_utf8(bytes.clone()).expect("an append is text and escapes");
            let placement = format!("\u{1b}[21;1HCHANGED{ERASE_LINE}");
            assert!(
                text.contains(&placement),
                "the repro's own changed-row placement is not there: {text:?}"
            );
            *bytes = text.replacen(&placement, "", 1).into_bytes();
        });
        let second = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "CHANGED".to_string(),
        ];
        let refused = band
            .append_document(&mut screen, 0, &second, &geometry)
            .expect_err("a stripped changed-row placement must still be caught");
        assert!(
            refused.to_string().contains("did not leave as intended"),
            "the refusal did not name the missing cell: {refused}"
        );
        assert_eq!(
            screen.writes, writes_before,
            "a refused vector must never reach the sink"
        );
        assert!(
            screen.written.is_empty(),
            "a refused vector must never reach the sink"
        );
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&shadow_before, &geometry, &mut owed),
            0,
            "a refused append must not move what the shadow believes the screen \
             holds: owed {:?}",
            String::from_utf8_lossy(&owed)
        );

        band.tamper = None;
        band.append_document(&mut screen, 0, &second, &geometry)
            .expect("the retry, with the tamper cleared, must land");
        let text = String::from_utf8(screen.written).expect("utf-8");
        assert!(
            text.contains("CHANGED"),
            "the retry did not repaint the row that changed: {text:?}"
        );
        assert!(
            !text.contains("marker-A") && !text.contains("marker-B"),
            "the retry re-emitted a row reuse should still have omitted: {text:?}"
        );
    }

    /// Companion to the preflight-level refusal above, at the sink layer
    /// instead: `Fussy{refusals: 1, ..}`'s first `write_once` takes zero
    /// bytes, which `deliver::classify` reports as `Emit::ZeroProgress`
    /// (`deliver.rs:190-196`) -- a delivery failure `out.emit` returns
    /// strictly *after* `check::preflight` already accepted the vector and
    /// strictly *before* the `shadow`/`target` swap (`frame.rs:1617-1620`).
    /// Regression characterization, not TDD-first; no production line
    /// changed to make this pass.
    #[test]
    fn a_sink_zero_progress_refusal_leaves_shadow_untouched_and_a_retry_lands_only_the_changed_row()
    {
        let geometry = geometry();
        let mut band = Band::new();
        let mut warmup = Counted::default();
        band.commit(&mut warmup, &band_rows(), &geometry, (23, 2))
            .expect("a first frame clears `damaged`");
        let first = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "marker-C".to_string(),
        ];
        band.append_document(&mut warmup, 0, &first, &geometry)
            .expect("the first settled block lands");

        let shadow_before = band.shadow.clone();
        let second = vec![
            "marker-A".to_string(),
            "marker-B".to_string(),
            "CHANGED".to_string(),
        ];

        let mut sink = Fussy {
            refusals: 1,
            written: Vec::new(),
        };
        let refused = band
            .append_document(&mut sink, 0, &second, &geometry)
            .expect_err("a sink taking zero bytes of its first write must not count as delivered");
        assert!(
            matches!(refused, Emit::ZeroProgress(_)),
            "a write that took zero bytes must classify as `ZeroProgress`, not {refused:?}"
        );
        assert!(
            sink.written.is_empty(),
            "the refused write still left bytes on the sink"
        );
        let mut owed = Vec::new();
        assert_eq!(
            band.shadow.diff(&shadow_before, &geometry, &mut owed),
            0,
            "a refused write must not move what the shadow believes the screen \
             holds: owed {:?}",
            String::from_utf8_lossy(&owed)
        );

        band.append_document(&mut sink, 0, &second, &geometry)
            .expect("the retry through the now-willing sink must land");
        let text = String::from_utf8(sink.written).expect("utf-8");
        assert!(
            text.contains("CHANGED"),
            "the retry did not repaint the row that changed: {text:?}"
        );
        assert!(
            !text.contains("marker-A") && !text.contains("marker-B"),
            "the retry re-emitted a row reuse should still have omitted: {text:?}"
        );
    }

    /// P3-WRAP diagnostic (not a gate): splits the `append` bucket already
    /// isolated by `event_loop::tests`'s near-cap breakdown (controller:
    /// 300x200 median 23.09ms) into `append_document`'s own three calls --
    /// `seed` (`frame.rs:1456`), `render_append` (`frame.rs:1063-1228`:
    /// clone shadow, place, clip, script) and `check::preflight`
    /// (`frame.rs:1533-1585`) -- plus the sink write and the drop of what
    /// they built. Two runs: `whole` calls the real, unmodified
    /// `append_document`; `substeps` repeats its calls inline with its own
    /// separately measured `whole` span -- neither assumes the substeps sum
    /// to either `whole` number.
    #[test]
    #[ignore = "P3-WRAP diagnostic: release only, see xfx-append-breakdown.log"]
    fn append_document_costs_split_into_seed_render_append_preflight_and_sink() {
        use std::time::{Duration, Instant};

        const WARMUP: usize = 5;
        const SAMPLES: usize = 20;

        fn report(cols: u16, rows: u16, label: &str, mut samples: Vec<Duration>) {
            samples.sort();
            let last = samples.len() - 1;
            println!(
                "append-breakdown {cols}x{rows} {label}: {SAMPLES} samples \
                 min={:?} median={:?} max={:?}",
                samples[0],
                samples[last / 2],
                samples[last]
            );
        }

        // Per-tick shape, matched to `event_loop.rs`'s real near-cap
        // breakdown (`run_streaming_near_cap`, `NEAR_CAP_ROWS = 250`): 249
        // settled rows at the screen's own width (`cols` ASCII characters,
        // distinct prefix + `x` padding -- not `numbered_rows`'s 6-7-char
        // fixture, which measured ~1.46ms at 300x200, nowhere near the
        // ~23.09ms `append` bucket it was meant to attribute, because it
        // never made `render_append`'s clone/place/clip or `preflight`'s
        // comparisons touch anything close to a real row's width) plus one
        // short fresh row shaped like the real per-tick delta text.
        let scroll = 1; // one new row a tick: the near-cap steady state.

        for (screen_rows, screen_cols) in [(24u16, 80u16), (200u16, 300u16)] {
            let geometry = crate::tui::layout::solve(screen_rows, screen_cols, 1).expect("a band");
            let area = usize::from(geometry.band_top().saturating_sub(1));
            let cursor = (geometry.hint, 1);
            let cols = usize::from(screen_cols);

            let settled_rows: Vec<String> = (0..249)
                .map(|index| {
                    let prefix = format!("row-{index:04}-");
                    let pad = cols.saturating_sub(prefix.len());
                    format!("{prefix}{}", "x".repeat(pad))
                })
                .collect();
            let new_row =
                " tail-0250-more unbroken text keeps the retained tail near its cap ".to_string();
            let mut rows = settled_rows;
            rows.push(new_row.clone());
            for row in &rows[..rows.len() - 1] {
                assert_eq!(row.len(), cols, "a settled row was not exactly cols wide");
            }
            assert!(
                new_row.len() < cols,
                "the fresh row should be short, like the real per-tick delta text"
            );
            let total_bytes: usize = rows.iter().map(String::len).sum();
            println!(
                "append-breakdown {screen_cols}x{screen_rows} fixture: {} rows, {total_bytes} \
                 bytes, settled width={cols} x{}, fresh width={}",
                rows.len(),
                rows.len() - 1,
                new_row.len()
            );

            // Correctness on the bytes each sample actually emitted: the new
            // row reached the sink, and the scroll/place counts match
            // `render_append`'s own proven formula for a band already at its
            // top (`an_append_with_more_rows_than_a_u16_scrolls_and_places_every_one_of_them`).
            let verify = |chunk: &[u8], i: usize| {
                assert!(
                    chunk
                        .windows(new_row.len())
                        .any(|w| w == new_row.as_bytes()),
                    "sample {i}: the new row never reached the emitted bytes"
                );
                assert_eq!(
                    scrolls_and_placements(chunk, &geometry),
                    (scroll, area + scroll),
                    "sample {i}: the append touched a different number of rows/scrolls"
                );
            };
            let prime = || {
                let mut band = Band::new();
                let mut sink = Counted::default();
                band.commit(&mut sink, &band_rows(), &geometry, cursor)
                    .expect("a priming frame sizes the shadow to this screen");
                // Untimed dense priming: one full near-cap-shaped append
                // before the timed loop starts, so the shadow every timed
                // sample clones/diffs already holds a full page of
                // full-width rows -- not the few short `band_rows()` lines
                // `commit` alone would leave it holding.
                let before = sink.written.len();
                band.append_document(&mut sink, scroll, &rows, &geometry)
                    .expect("an untimed dense append primes the shadow with near-cap content");
                let placed = scrolls_and_placements(&sink.written[before..], &geometry);
                assert_eq!(
                    placed,
                    (scroll, area + scroll),
                    "the untimed priming append did not place a full page of dense rows"
                );
                println!(
                    "append-breakdown {screen_cols}x{screen_rows} priming placed: scrolls={} \
                     erases={}",
                    placed.0, placed.1
                );
                (band, sink)
            };

            // -- whole: the real, unmodified `append_document`. --
            let (mut band, mut sink) = prime();
            let mut whole = Vec::with_capacity(SAMPLES);
            for i in 0..WARMUP + SAMPLES {
                let before = sink.written.len();
                let started = Instant::now();
                band.append_document(&mut sink, scroll, &rows, &geometry)
                    .expect("the near-cap append lands");
                let elapsed = started.elapsed();
                verify(&sink.written[before..], i);
                if i >= WARMUP {
                    whole.push(elapsed);
                }
            }
            report(
                screen_cols,
                screen_rows,
                "whole (append_document, unmodified)",
                whole,
            );

            // -- substeps: the same three calls, timed apart. --
            let (mut band, mut sink) = prime();
            let mut by_stage: [Vec<Duration>; 6] = Default::default();
            for i in 0..WARMUP + SAMPLES {
                let before = sink.written.len();
                let whole_started = Instant::now();

                let t = Instant::now();
                let seed = band
                    .seed(check::PlaneKind::Primary, &geometry)
                    .expect("a seed");
                let seed_elapsed = t.elapsed();

                let t = Instant::now();
                let (appended, footprint, script) = band.render_append(scroll, &rows, &geometry);
                let render_elapsed = t.elapsed();

                // Cloned rather than borrowed from `band`: `declared` must
                // outlive the sink write below, and `band` is mutated
                // (`delivered`, `caret`) before this iteration's teardown
                // drops it, so a borrow of `band` here would hold `band`
                // frozen across that mutation.
                let shown_title = band.shown_title.clone();
                let declared = check::Declared::new(
                    check::Intent::Document {
                        script: &script,
                        caret: None,
                        cursor_visible: None,
                        title: shown_title.as_deref(),
                    },
                    footprint,
                );
                let t = Instant::now();
                check::preflight(&seed, &appended, &declared).expect("the append is accepted");
                let preflight_elapsed = t.elapsed();

                let t = Instant::now();
                sink.emit(&appended).expect("the sink takes the append");
                let sink_elapsed = t.elapsed();

                // The same tail `append_document` runs after a landed emit
                // (`frame.rs:1587-1598`), untimed here as it is there.
                std::mem::swap(&mut band.shadow, &mut band.target);
                band.document_bottom =
                    Some(geometry.band_top().saturating_sub(1)).filter(|row| *row > 0);
                band.delivered(&geometry);
                band.caret = None;

                // What building `appended`/`script`/`seed` costs to free,
                // reported rather than dropped silently outside any timer.
                // `declared` first: it borrows `script`.
                let t = Instant::now();
                drop(declared);
                drop((script, seed, appended));
                let teardown_elapsed = t.elapsed();
                let whole_elapsed = whole_started.elapsed();

                verify(&sink.written[before..], i);
                if i >= WARMUP {
                    for (bucket, elapsed) in by_stage.iter_mut().zip([
                        seed_elapsed,
                        render_elapsed,
                        preflight_elapsed,
                        sink_elapsed,
                        teardown_elapsed,
                        whole_elapsed,
                    ]) {
                        bucket.push(elapsed);
                    }
                }
            }
            for (label, samples) in [
                "seed (Band::seed)",
                "render_append (whole call)",
                "preflight (check::preflight)",
                "sink (Vec write)",
                "teardown (drop)",
                "whole (substeps span)",
            ]
            .into_iter()
            .zip(by_stage)
            {
                report(screen_cols, screen_rows, label, samples);
            }
        }
    }
}
