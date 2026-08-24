//! What the band is a picture of.
//!
//! The event loop reads bytes and writes frames; everything between the two --
//! what the band's rows say, where the caret is, and whether the session is
//! leaving -- is here, so that "what would the band look like now" is a
//! question about a value rather than about a terminal.
//!
//! In this phase that value is small. The band is a divider, the composer, and
//! a hint row the session owns and leaves empty until the phase that fills it;
//! there is no turn yet, and the task that adds one adds it here. What is
//! already true is the shape: the rows are produced top-down from the geometry,
//! and the caret is reported in the composer's own coordinates rather than
//! derived a second time by whatever draws it.
//!
//! # The composer, its gutter, and the band's own height
//!
//! The text belongs to [`Editor`]; what this module owns is where it goes. Two
//! decisions carry that:
//!
//! * **The prompt marker is a gutter, not a prefix.** `> ` is two cells the
//!   text never uses, so the composer is measured against `cols - 2` and every
//!   one of its rows is written into that same two-cell indent. The
//!   alternative -- wrapping at the screen's width and putting the marker in
//!   front of the first row -- makes that row two cells too long, which the
//!   painter clips and the caret does not: the caret would be reported past the
//!   last column of a row whose end is not on the screen. The marker itself
//!   stays on the composer's **first** row, and a composer scrolled past that
//!   row shows none, because the marker means *this is where what you are
//!   writing begins* and there is exactly one such row.
//! * **The band grows and shrinks with the text, up to the cap.** A composer of
//!   *n* rows is a band with *n* composer rows, re-solved through
//!   [`super::layout::solve`] so the divider, the hint row and the content area
//!   stay one derivation rather than three. Past
//!   [`super::layout::input_row_limit`] it stops growing and scrolls inside the
//!   rows it has ([`editor::window`]), which is what keeps a long draft from
//!   eating the transcript.
//!
//! # What the keystrokes mean here
//!
//! [`Shell::route_bytes`] owns the [`Decoder`], so the deferred bytes the
//! launch probe read and the bytes every later read produces go through one
//! machine in arrival order -- which is the whole of what that decoder asks of
//! its caller. What comes out is routed in exactly three ways: the composer's
//! own actions go to the editor, `Submit` and `Ctrl-D` are the session's, and
//! everything else is a keystroke this phase has no binding for. `Ctrl-D` is
//! the one worth stating: it leaves **only** on an empty composer, and with
//! text under the caret it is a forward delete, which is what it means in every
//! shell that has both.
//!
//! The transcript is the one thing here that is **not** a picture of the band.
//! Nothing above the divider belongs to xfx: a row that goes there goes into
//! the terminal's own document and is never repainted, so what the shell holds
//! for it is not a state to draw but a *queue of writes it owes* -- one
//! [`Append`] per push, drained by the loop before the frame, because an append
//! scrolls the screen and a band painted first would be carried up with it.

use std::time::Instant;

use super::editor::{self, Editor};
use super::input::{Action, Decoder, Input};
use super::layout::{self, Geometry};
use super::render_request::{Reason, RenderRequest};
use super::transcript::{Append, Transcript};
use crate::config::RuntimeConfig;

/// The divider's rule, one cell wide, repeated across the screen.
const RULE: char = '\u{2500}';

/// What the composer puts in front of what is typed.
const PROMPT: &str = "> ";

/// What its continuation rows are written into instead: the same two cells,
/// blank, so a wrapped row starts under the row above it rather than two
/// columns to the left of it.
const GUTTER: &str = "  ";

/// How many cells [`PROMPT`] occupies, and therefore where an empty composer's
/// caret sits.
const PROMPT_CELLS: u16 = 2;

/// The UI's state: what the band shows, and what it owes the screen.
pub(crate) struct Shell {
    pub(crate) geometry: Geometry,
    pub(crate) render: RenderRequest,
    /// The model a turn will talk to.
    ///
    /// Read from the configuration once, at startup, rather than consulted per
    /// frame: the hint row renders a compact form of it and a `/model` change
    /// replaces it, and both want one field rather than a borrow of the whole
    /// configuration.
    // Task 16's hint row is its first reader.
    #[allow(dead_code)]
    model: String,
    /// The text being composed, and where the caret is in it.
    editor: Editor,
    /// The one input machine of the session.
    ///
    /// Held here rather than in the loop because it is *state a keystroke can
    /// be half-way through*: a `CSI` split across two reads, a scalar split
    /// across two, a paste that spans a hundred. A decoder made per read would
    /// begin each of them again.
    decoder: Decoder,
    /// The answer text that has not ended its line yet, and the rows it has put
    /// on the screen.
    transcript: Transcript,
    /// The document writes this session owes, oldest first.
    ///
    /// A `Vec` rather than one `Append`, because two pushes can land between
    /// two frames and their scrolls do not merge: the second one's rows are
    /// measured against a screen the first one already moved.
    pending: Vec<Append>,
    leaving: bool,
}

impl Shell {
    pub(crate) fn new(config: &RuntimeConfig, geometry: Geometry) -> Self {
        Self {
            geometry,
            // A session that has drawn nothing owes a frame. Requesting it here
            // rather than in the loop is what keeps "the band appears" a
            // property of having a shell at all.
            render: {
                let mut render = RenderRequest::default();
                render.request(Reason::FirstFrame);
                render
            },
            model: config.model.clone(),
            editor: Editor::new(),
            decoder: Decoder::new(),
            // Wrapped to the screen the band was solved for: the document rows
            // and the band rows share a terminal, and a transcript measured
            // against a different width would wrap where the screen does not.
            transcript: Transcript::new(geometry.cols),
            pending: Vec::new(),
            leaving: false,
        }
    }

    /// The band's rows, top first, starting at the divider.
    ///
    /// Exactly as many rows as the band owns: the writer places them by
    /// counting down from the divider, so a row missing here would shift every
    /// row below it up by one.
    pub(crate) fn band_rows(&self) -> Vec<String> {
        let mut rows = Vec::with_capacity(usize::from(self.geometry.band_rows()));
        rows.push(std::iter::repeat_n(RULE, usize::from(self.geometry.cols)).collect());
        // The composer's own rows, as many of them as the window shows, each in
        // the gutter the marker owns.
        let composer = self.editor.rows(self.text_cols());
        for index in self.window(composer.len()) {
            let marker = if index == 0 { PROMPT } else { GUTTER };
            rows.push(format!("{marker}{}", composer[index]));
        }
        // A composer shorter than the band it is in. The remaining rows are the
        // band's, so they are written -- blank -- rather than left out: a row
        // the frame does not place is a row the last frame's text stays on.
        while rows.len() <= usize::from(self.geometry.input_rows()) {
            rows.push(String::new());
        }
        // The hint row: owned, cleared with the rest of the band, and empty
        // until the task that fills it.
        rows.push(String::new());
        rows
    }

    /// Where the caret goes: the terminal's own row, and the number of cells to
    /// the left of it on that row.
    pub(crate) fn cursor(&self) -> (u16, u16) {
        let (row, column) = self.editor.point(self.text_cols());
        let window = self.window(self.editor.rows(self.text_cols()).len());
        // Here, and nowhere before it, is where a row becomes a **terminal
        // coordinate**: the composer's rows are counted in `usize` all the way
        // down (`Editor::point`), because a draft can have more rows than a
        // `u16` can name, and a count saturated earlier would leave the window
        // following a row the caret is not on. The clamp comes first and the
        // narrowing second, so the conversion cannot fail and its answer, if it
        // ever could, is the band's own last composer row rather than a row
        // outside the band -- the shape `transcript`'s `shown` uses for the
        // same reason.
        let last = self.geometry.input_rows().saturating_sub(1);
        let offset =
            u16::try_from(row.saturating_sub(window.start).min(usize::from(last))).unwrap_or(last);
        (
            self.geometry.input_first.saturating_add(offset),
            PROMPT_CELLS.saturating_add(column),
        )
    }

    /// How wide the composer's *text* is: the screen without the gutter.
    fn text_cols(&self) -> u16 {
        self.geometry.cols.saturating_sub(PROMPT_CELLS).max(1)
    }

    /// Which of a `rows`-row composer's rows the band is showing.
    fn window(&self, rows: usize) -> std::ops::Range<usize> {
        editor::window(
            rows,
            self.editor.point(self.text_cols()).0,
            self.geometry.input_rows(),
        )
    }

    /// Adds answer text to the transcript.
    ///
    /// Nothing is written here. What the text costs the terminal is queued and
    /// a frame is asked for, because the append and the frame that follows it
    /// are one turn's worth of work and the loop is the only thing that owns
    /// the screen.
    // The composer's submit is the first caller -- it echoes the line the user
    // sent so the loop is visibly closed -- and Task 12's deltas are the next.
    pub(crate) fn write_transcript(&mut self, text: &str) {
        let append = self.transcript.push(text);
        self.owe(append);
    }

    /// Ends the transcript's current line, leaving it in the document.
    // The composer's submit is the first caller, and Task 12's end-of-turn is
    // the next: a turn ends whether or not the last delta carried a newline.
    pub(crate) fn end_transcript_line(&mut self) {
        let append = self.transcript.end_line();
        self.owe(append);
    }

    /// Records a document write, unless it is one that would write nothing.
    ///
    /// The guard is not tidiness: an append that scrolls nothing and writes no
    /// rows would still cost the loop a frame, and a frame the band did not
    /// need is a repaint of the whole band on a link that may be a serial line.
    fn owe(&mut self, append: Append) {
        if append.scroll == 0 && append.rows.is_empty() {
            return;
        }
        self.pending.push(append);
        // An append scrolls the screen out from under the band, so the frame
        // that follows it is not optional.
        self.render.request(Reason::Transcript);
    }

    /// Takes the document writes this session owes, oldest first.
    pub(crate) fn take_pending(&mut self) -> Vec<Append> {
        std::mem::take(&mut self.pending)
    }

    /// Whether the session is on its way out.
    pub(crate) fn leaving(&self) -> bool {
        self.leaving
    }

    /// Ends the session at the end of this turn of the loop.
    pub(crate) fn leave(&mut self) {
        self.leaving = true;
    }

    /// Decodes `bytes` -- in the order the terminal delivered them -- and does
    /// what they mean.
    ///
    /// One [`Instant`] for the whole read rather than one per byte: the bytes
    /// of a single read arrived together, and the only thing the decoder times
    /// is how long a bare `ESC` has been alone, which a burst's own bytes must
    /// not be able to expire.
    pub(crate) fn route_bytes(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        let mut events = Vec::new();
        for byte in bytes {
            self.decoder.feed(*byte, now, &mut events);
        }
        self.consume(events);
    }

    /// Resolves what only the passage of time resolves: a bare `ESC` that has
    /// gone quiet is the Escape key.
    ///
    /// Called once a turn, which is what makes [`super::input::Decoder`]'s
    /// timeout mean 50 ms rather than "until the next keystroke".
    pub(crate) fn settle_input(&mut self, now: Instant) {
        let mut events = Vec::new();
        self.decoder.flush(now, &mut events);
        self.consume(events);
    }

    /// Applies decoded events in order.
    fn consume(&mut self, events: Vec<Input>) {
        for event in events {
            match event {
                Input::Text(character) => self.type_character(character),
                Input::Action(action) => self.act(action),
                // Task 18's `paste` module is what turns these into text: a
                // pasted byte is not a keystroke, and until there is something
                // that filters it, letting one into the composer would put a
                // control the terminal obeys into the transcript on submit.
                Input::PasteByte(_) => {}
            }
        }
    }

    /// One typed character.
    ///
    /// A character the byte budget refuses changes nothing and says nothing:
    /// upstream flashes the composer (`input_limit_rejection.zig:4-23`) and
    /// this phase does not, which `docs/parity.md` records.
    fn type_character(&mut self, character: char) {
        let mut encoded = [0u8; 4];
        if self.editor.insert(character.encode_utf8(&mut encoded)) {
            self.edited();
        }
    }

    /// What one decoded action means.
    ///
    /// Exhaustive on purpose: an action added later has to be given a home
    /// here rather than falling into a wildcard that silently ignores it.
    fn act(&mut self, action: Action) {
        match action {
            // The composer's own.
            Action::Left
            | Action::Right
            | Action::Up
            | Action::Down
            | Action::Home
            | Action::End
            | Action::WordLeft
            | Action::WordRight
            | Action::Backspace
            | Action::Delete
            | Action::DeleteWordLeft
            | Action::KillToEnd
            | Action::KillToStart
            | Action::InsertNewline => {
                self.editor.apply(action, self.text_cols());
                self.edited();
            }
            Action::Submit => self.submit(),
            // The end of the session, but only from an empty composer: with
            // text under the caret Ctrl-D is the forward delete it is in every
            // shell that has both, and leaving would throw away a draft.
            Action::Eof => {
                if self.editor.is_empty() {
                    self.leave();
                } else {
                    self.editor.apply(Action::Delete, self.text_cols());
                    self.edited();
                }
            }
            // The whole band is repainted every frame, so a redraw is a frame.
            Action::Redraw => self.render.request(Reason::ExternalDamage),
            // Not this task's: Ctrl-C and the double-Esc gesture are Task 12's,
            // the paste markers are Task 18's, and an `Ignore` is a keystroke
            // this session has no binding for -- an event rather than silence
            // precisely so that it accounts for the bytes it was decoded from.
            Action::Cancel
            | Action::Escape
            | Action::PasteStart
            | Action::PasteEnd
            | Action::Ignore => {}
        }
    }

    /// Sends what has been composed.
    ///
    /// Task 11 hands it to the worker. Until then the composer is cleared and
    /// the text is appended to the terminal's document, so that a submission is
    /// something the session visibly did rather than something that vanished.
    fn submit(&mut self) {
        let text = self.editor.take();
        self.edited();
        if text.is_empty() {
            return;
        }
        self.write_transcript(&text);
        // The line ends whether or not the last thing typed was a newline: what
        // was submitted is finished, and a tail left open would be continued by
        // whatever is written next.
        self.end_transcript_line();
    }

    /// What every change to the composer owes: a frame, and a band the right
    /// height for the text it now holds.
    fn edited(&mut self) {
        self.refit();
        self.render.request(Reason::Footer);
    }

    /// Re-solves the band for the composer's current height.
    ///
    /// The cap is measured against the screen rather than against the band, so
    /// growing the composer cannot move its own ceiling
    /// ([`layout::input_row_limit`]). A screen that holds a band holds a capped
    /// composer on it too -- the cap is never more than `rows - 2`, so there is
    /// always a document row left -- which is why the refusal below is not a
    /// case this can reach. It is `solve`'s answer being taken seriously rather
    /// than unwrapped, and a band that cannot be re-solved keeps the height it
    /// had. `the_smallest_band_still_grows_by_the_rule_rather_than_by_luck` is
    /// where that claim is checked from the smallest screen up.
    fn refit(&mut self) {
        let limit = layout::input_row_limit(self.geometry.rows);
        let rows = self.editor.rows(self.text_cols()).len();
        let wanted = u16::try_from(rows.clamp(1, usize::from(limit))).unwrap_or(limit);
        if wanted == self.geometry.input_rows() {
            return;
        }
        if let Some(geometry) = layout::solve(self.geometry.rows, self.geometry.cols, wanted) {
            self.geometry = geometry;
            // The divider moved, so every row of the band is somewhere else.
            self.render.request(Reason::Resize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use crate::config::Environment;

    /// A configuration with nothing in it, from a home and a workspace that
    /// exist and hold no settings: the shell reads one field of it and a test
    /// that hand-built the struct would stop compiling every time an unrelated
    /// key was added.
    fn config(home: &std::path::Path, workspace: &std::path::Path) -> RuntimeConfig {
        RuntimeConfig::load_with(
            &Environment::new(Some(home.to_path_buf()), BTreeMap::new()),
            workspace,
        )
        .expect("load a configuration")
    }

    fn shell(rows: u16, cols: u16) -> Shell {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        Shell::new(
            &config(home.path(), workspace.path()),
            crate::tui::layout::solve(rows, cols, 1).expect("a band"),
        )
    }

    #[test]
    fn a_session_that_has_drawn_nothing_owes_a_frame() {
        let mut shell = shell(24, 80);
        assert!(
            shell.render.begin().is_some(),
            "a fresh session did not ask for its first frame, so the band \
             would appear only once something else changed"
        );
    }

    #[test]
    fn the_band_is_a_rule_the_composer_and_a_hint_row() {
        let shell = shell(24, 80);
        let rows = shell.band_rows();
        assert_eq!(
            rows.len(),
            usize::from(shell.geometry.band_rows()),
            "the band's rows and its geometry disagree, so every row below the \
             first missing one is painted a row too high"
        );
        assert_eq!(rows[0], "\u{2500}".repeat(80), "the divider");
        assert_eq!(rows[1], "> ", "the composer's prompt marker");
        assert_eq!(rows[2], "", "the hint row is owned and empty");
    }

    #[test]
    fn the_divider_spans_the_screen_it_was_solved_for() {
        // A rule of a fixed width would leave a gap on a wide terminal and run
        // off a narrow one -- and with autowrap off, running off is silent.
        for cols in [20u16, 80, 200] {
            let shell = shell(24, cols);
            assert_eq!(shell.band_rows()[0].chars().count(), usize::from(cols));
        }
    }

    #[test]
    fn a_taller_composer_gets_one_row_each_and_the_marker_stays_on_the_first() {
        let home = tempfile::tempdir().expect("a home");
        let workspace = tempfile::tempdir().expect("a workspace");
        let shell = Shell::new(
            &config(home.path(), workspace.path()),
            crate::tui::layout::solve(24, 80, 4).expect("a four-row composer"),
        );
        let rows = shell.band_rows();
        assert_eq!(rows.len(), 6, "divider, four composer rows, hint");
        assert_eq!(rows[1], "> ");
        assert_eq!(
            &rows[2..5],
            &["".to_string(), "".to_string(), "".to_string()]
        );
    }

    #[test]
    fn the_caret_sits_after_the_prompt_marker_on_the_composers_first_row() {
        let shell = shell(24, 80);
        assert_eq!(shell.cursor(), (23, 2));
        assert_eq!(
            shell.cursor().1,
            u16::try_from(PROMPT.chars().count()).expect("the marker's width"),
            "the caret and the marker were measured apart, so they can drift"
        );
    }

    #[test]
    fn answer_text_is_owed_to_the_document_and_asks_for_the_frame_that_follows_it() {
        let mut shell = shell(24, 80);
        // Take the first frame the session owes, so what is asked for below is
        // the transcript's own request rather than that one.
        let _first = shell.render.begin().expect("the first frame");
        shell.write_transcript("answered");

        assert!(
            shell.render.begin().is_some(),
            "an append scrolls the band off its own rows and no frame was asked \
             for, so the band would stay scrolled until something else moved"
        );
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 1,
                rows: vec!["answered".to_string()]
            }]
        );
    }

    #[test]
    fn a_document_write_is_owed_once() {
        // The rows are the terminal's document after they are written, and a
        // second write of the same append would scroll a second time.
        let mut shell = shell(24, 80);
        shell.write_transcript("answered");
        assert_eq!(shell.take_pending().len(), 1);
        assert!(shell.take_pending().is_empty());
    }

    #[test]
    fn two_pushes_between_two_frames_are_two_writes_in_order() {
        // Not merged: the second append's rows are measured against a screen
        // the first one already scrolled, so replacing the pair with the later
        // one loses a row of the answer.
        let mut shell = shell(24, 80);
        shell.write_transcript("first\n");
        shell.write_transcript("second");
        assert_eq!(
            shell.take_pending(),
            vec![
                Append {
                    scroll: 2,
                    rows: vec!["first".to_string(), String::new()]
                },
                Append {
                    scroll: 0,
                    rows: vec!["second".to_string()]
                },
            ]
        );
    }

    #[test]
    fn ending_a_line_that_is_already_on_the_screen_owes_nothing_and_asks_for_nothing() {
        let mut shell = shell(24, 80);
        shell.write_transcript("answered");
        shell.take_pending();
        let _asked = shell
            .render
            .begin()
            .expect("the frame the append asked for");

        shell.end_transcript_line();
        assert!(
            shell.take_pending().is_empty(),
            "a line already on the screen was written again, which scrolls a \
             blank row into the document"
        );
        assert!(
            shell.render.begin().is_none(),
            "a whole-band repaint was asked for by a write that wrote nothing"
        );
    }

    #[test]
    fn the_transcript_wraps_to_the_screen_the_band_was_solved_for() {
        // A transcript measured against a different width wraps where the
        // terminal does not, and every row after the first is placed wrong.
        let mut shell = shell(24, 20);
        shell.write_transcript(&"x".repeat(25));
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 2,
                rows: vec!["x".repeat(20), "x".repeat(5)]
            }]
        );
    }

    // -----------------------------------------------------------------------
    // the composer
    // -----------------------------------------------------------------------

    #[test]
    fn what_is_typed_appears_in_the_composer_with_the_caret_after_it() {
        let mut shell = shell(24, 80);
        // The frame the session owes for existing, so that what is asked for
        // below is what the typing asked for.
        let _first = shell.render.begin().expect("the first frame");

        shell.route_bytes("hello \u{d55c}\u{ae00}".as_bytes());
        assert_eq!(shell.band_rows()[1], "> hello \u{d55c}\u{ae00}");
        // Two cells of gutter, six of "hello ", and two apiece for the glyphs.
        assert_eq!(shell.cursor(), (23, 12));
        assert!(
            shell.render.begin().is_some(),
            "typing asked for no frame, so the band would go on showing a \
             composer the session no longer has"
        );
    }

    #[test]
    fn a_caret_that_moved_asks_for_the_frame_that_moves_it() {
        // The band is repainted whole, and the caret is placed by the frame:
        // an arrow key that changed no text still changes where the terminal's
        // own cursor belongs.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"ab");
        let _typed = shell.render.begin().expect("the frame the typing owed");
        shell.route_bytes(b"\x1b[D");
        assert_eq!(shell.cursor(), (23, 3));
        assert!(
            shell.render.begin().is_some(),
            "the caret moved and no frame was asked for, so the terminal's \
             cursor would stay where the last frame left it"
        );
    }

    #[test]
    fn a_control_byte_that_means_nothing_here_is_not_typed_into_the_composer() {
        // The decoder's table is closed, and this is the half of that policy
        // the composer keeps: a tab, an unnamed C0 and a C1 scalar are
        // keystrokes with no binding, and a composer that took them would put a
        // control the terminal obeys into the transcript on submit.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"a\tb\x00c\xc2\x9bd");
        assert_eq!(shell.band_rows()[1], "> abcd");
    }

    #[test]
    fn ctrl_d_leaves_an_empty_composer_and_deletes_from_one_with_text_in_it() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello\x1b[A");
        assert!(!shell.leaving(), "an ordinary keystroke ended the session");

        // Home, then Ctrl-D: a forward delete, and the session stays.
        shell.route_bytes(&[0x01, 0x04]);
        assert!(
            !shell.leaving(),
            "Ctrl-D threw away a draft instead of deleting a character"
        );
        assert_eq!(shell.band_rows()[1], "> ello");

        shell.route_bytes(&[0x05, 0x15]);
        assert_eq!(shell.band_rows()[1], "> ", "Ctrl-U left text behind");
        shell.route_bytes(&[0x04]);
        assert!(shell.leaving(), "Ctrl-D did not leave an empty composer");
    }

    #[test]
    fn ctrl_d_leaves_from_the_middle_of_a_burst_as_well_as_from_its_own_read() {
        // A burst that ends a session arrives as one read, and a loop that only
        // looked at the first byte of it would keep waiting for input the
        // terminal has already delivered.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"\x04def");
        assert!(shell.leaving());
    }

    #[test]
    fn a_sequence_split_across_two_reads_is_still_one_keystroke() {
        // Why the decoder lives in the shell rather than in the loop: an arrow
        // key that arrived in two reads must not become an `ESC` and three
        // characters in the composer.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"ab\x1b");
        shell.route_bytes(b"[D");
        shell.route_bytes(b"X");
        assert_eq!(shell.band_rows()[1], "> aXb");
    }

    #[test]
    fn a_bare_escape_that_has_gone_quiet_stops_swallowing_the_next_keystroke() {
        // The turn's own flush, and the only thing that resolves an `ESC` the
        // user pressed on its own: until it does, the decoder is still waiting
        // to see whether an arrow key is arriving, and the next byte would be
        // read as the Alt-something this phase binds nothing to.
        let mut shell = shell(24, 80);
        shell.route_bytes(&[0x1b]);
        shell.settle_input(Instant::now() + Decoder::ESC_TIMEOUT);
        shell.route_bytes(b"a");
        assert_eq!(
            shell.band_rows()[1],
            "> a",
            "the keystroke after a settled Escape was eaten as a modifier"
        );
    }

    #[test]
    fn submitting_clears_the_composer_and_leaves_the_text_in_the_document() {
        let mut shell = shell(24, 80);
        let _first = shell.render.begin().expect("the first frame");
        shell.route_bytes(b"ask me\r");

        assert_eq!(shell.band_rows()[1], "> ", "the composer kept the text");
        assert_eq!(
            shell.take_pending(),
            vec![Append {
                scroll: 1,
                rows: vec!["ask me".to_string()]
            }]
        );
        assert!(
            shell.render.begin().is_some(),
            "a submission asked for no frame, so the band would keep showing \
             text the composer no longer holds"
        );
    }

    #[test]
    fn submitting_an_empty_composer_writes_nothing_at_all() {
        // Otherwise every stray Return puts a blank row in the terminal's
        // document, and a document row cannot be taken back.
        let mut shell = shell(24, 80);
        shell.route_bytes(&[0x0d]);
        assert!(shell.take_pending().is_empty());
    }

    #[test]
    fn a_newline_is_composed_rather_than_submitted() {
        // C-j is the multi-line composer's whole existence: it has to reach the
        // editor as text, not as the Return that sends what has been written.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"first\x0asecond");
        assert!(
            shell.take_pending().is_empty(),
            "a newline submitted itself"
        );
        assert_eq!(&shell.band_rows()[1..3], &["> first", "  second"]);
    }

    // -----------------------------------------------------------------------
    // the band's own height
    // -----------------------------------------------------------------------

    #[test]
    fn the_band_grows_with_the_composer_and_shrinks_back_with_it() {
        let mut shell = shell(24, 80);
        assert_eq!(shell.geometry.divider, 22);
        shell.route_bytes(b"one\x0atwo\x0athree");
        assert_eq!(shell.geometry.input_rows(), 3);
        assert_eq!(
            (shell.geometry.divider, shell.geometry.content_bottom),
            (20, 19),
            "the composer took rows from the document without moving the \
             divider, so the band and the document overlap"
        );
        assert_eq!(shell.cursor(), (23, 7), "the caret is on the last row");

        // Back to one row, and every row number back with it. A kill takes the
        // line it is on, so what empties a three-line draft is submitting it.
        shell.route_bytes(&[0x15]);
        assert_eq!(
            shell.geometry.input_rows(),
            3,
            "the kill took a whole draft"
        );
        shell.route_bytes(&[0x0d]);
        assert_eq!(shell.band_rows().len(), 3, "divider, composer, hint");
        assert_eq!(
            (shell.geometry.divider, shell.geometry.content_bottom),
            (22, 21),
            "the band kept rows the document had back"
        );
    }

    #[test]
    fn the_composer_stops_growing_at_half_the_content_area_and_scrolls_instead() {
        // input_presentation.zig:201-220. On a 12-row screen the cap is five
        // rows, and the sixth line of a draft scrolls the first one out of the
        // window rather than taking a sixth row from the transcript.
        let mut shell = shell(12, 80);
        assert_eq!(crate::tui::layout::input_row_limit(12), 5);
        shell.route_bytes(b"one\x0atwo\x0athree\x0afour\x0afive");
        assert_eq!(shell.geometry.input_rows(), 5);
        assert_eq!(shell.geometry.divider, 6);
        assert_eq!(
            shell.band_rows()[1..6],
            [
                "> one".to_string(),
                "  two".to_string(),
                "  three".to_string(),
                "  four".to_string(),
                "  five".to_string(),
            ]
        );

        shell.route_bytes(b"\x0asix");
        assert_eq!(shell.geometry.input_rows(), 5, "the cap did not hold");
        assert_eq!(
            shell.band_rows()[1..6],
            [
                "  two".to_string(),
                "  three".to_string(),
                "  four".to_string(),
                "  five".to_string(),
                "  six".to_string(),
            ],
            "the window did not follow the caret"
        );
        assert_eq!(
            shell.cursor(),
            (11, 5),
            "the caret is on the band's last composer row"
        );
    }

    #[test]
    fn a_draft_with_more_rows_than_a_u16_still_shows_its_end_and_places_the_caret() {
        // The composer's rows are counted in `usize` and narrowed only where a
        // terminal row is made. A count saturated at `u16::MAX` would leave the
        // window eleven rows short of row 65535 -- nowhere near the caret --
        // and the band would show rows the user is not typing on.
        //
        // The draft is inserted rather than typed because every keystroke
        // re-measures the whole composer (`refit`), so seventy thousand of them
        // is quadratic work for a fact about one of them. What is under test is
        // what the band makes of the text, and that is reached the same way.
        let mut shell = shell(24, 80);
        assert!(shell.editor.insert(&"x\n".repeat(70_000)));
        shell.edited();

        assert_eq!(shell.geometry.input_rows(), 11, "the cap");
        let rows = shell.band_rows();
        assert_eq!(rows[1], "  x", "the window is not showing the draft's end");
        assert_eq!(
            rows[11], "  ",
            "the last composer row is the empty one the caret is on: {rows:?}"
        );
        assert_eq!(
            shell.cursor(),
            (23, 2),
            "the caret is not on the band's last composer row"
        );
    }

    #[test]
    fn the_smallest_band_still_grows_by_the_rule_rather_than_by_luck() {
        // The cap is measured against the screen, so the row it names is one
        // `layout::solve` will really give: a screen that holds a band holds a
        // capped composer on it, and the composer never has to be clamped by
        // some second rule that could disagree with this one.
        for rows in crate::tui::layout::MIN_ROWS..=40 {
            let mut shell = shell(rows, 80);
            let limit = crate::tui::layout::input_row_limit(rows);
            shell.route_bytes("x\n".repeat(usize::from(limit) + 4).as_bytes());
            assert_eq!(
                shell.geometry.input_rows(),
                limit,
                "a {rows}-row screen grew its composer to something other than \
                 its cap"
            );
            assert!(
                shell.geometry.content_bottom >= 1,
                "a {rows}-row screen left no document above the band"
            );
            assert_eq!(
                shell.band_rows().len(),
                usize::from(shell.geometry.band_rows())
            );
        }
    }

    #[test]
    fn a_composer_wider_than_the_screen_wraps_into_the_gutter() {
        // The text is measured against the screen minus the marker, so a row is
        // never two cells too long for the terminal -- which the painter would
        // clip and the caret would not.
        let mut shell = shell(24, 20);
        shell.route_bytes(&[b'x'; 19]);
        assert_eq!(
            &shell.band_rows()[1..3],
            &["> ".to_string() + &"x".repeat(18), "  x".to_string()]
        );
        assert_eq!(shell.cursor(), (23, 3));
        for row in shell.band_rows() {
            assert!(
                crate::tui::wrap::width(&row) <= 20,
                "a band row ran past the screen: {row:?}"
            );
        }
    }
}
