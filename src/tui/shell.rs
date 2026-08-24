//! What the band is a picture of.
//!
//! The event loop reads bytes and writes frames; everything between the two --
//! what the band's rows say, where the caret is, and whether the session is
//! leaving -- is here, so that "what would the band look like now" is a
//! question about a value rather than about a terminal.
//!
//! In this phase that value is small. The band is a divider, a composer showing
//! its prompt marker, and a hint row the session owns and leaves empty until
//! the phase that fills it; there is no turn and no editor yet, and the tasks
//! that add each of them add it here. What is already true is the shape: the
//! rows are produced top-down from the geometry, and the caret is reported in
//! the composer's own coordinates rather than derived a second time by whatever
//! draws it.
//!
//! The transcript is the one thing here that is **not** a picture of the band.
//! Nothing above the divider belongs to xfx: a row that goes there goes into
//! the terminal's own document and is never repainted, so what the shell holds
//! for it is not a state to draw but a *queue of writes it owes* -- one
//! [`Append`] per push, drained by the loop before the frame, because an append
//! scrolls the screen and a band painted first would be carried up with it.

use super::layout::Geometry;
use super::render_request::{Reason, RenderRequest};
use super::transcript::{Append, Transcript};
use crate::config::RuntimeConfig;

/// The divider's rule, one cell wide, repeated across the screen.
const RULE: char = '\u{2500}';

/// What the composer puts in front of what is typed.
const PROMPT: &str = "> ";

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
        // The composer's first row carries the prompt marker; the rows below it
        // are its continuations, and there is nothing to continue yet.
        for row in 0..self.geometry.input_rows() {
            rows.push(if row == 0 {
                PROMPT.to_string()
            } else {
                String::new()
            });
        }
        // The hint row: owned, cleared with the rest of the band, and empty
        // until the task that fills it.
        rows.push(String::new());
        rows
    }

    /// Where the caret goes: the terminal's own row, and the number of cells to
    /// the left of it on that row.
    pub(crate) fn cursor(&self) -> (u16, u16) {
        (self.geometry.input_first, PROMPT_CELLS)
    }

    /// Adds answer text to the transcript.
    ///
    /// Nothing is written here. What the text costs the terminal is queued and
    /// a frame is asked for, because the append and the frame that follows it
    /// are one turn's worth of work and the loop is the only thing that owns
    /// the screen.
    // Task 10's submit is the first caller -- it echoes the line the user sent
    // so the loop is visibly closed -- and Task 12's deltas are the next.
    #[allow(dead_code)]
    pub(crate) fn write_transcript(&mut self, text: &str) {
        let append = self.transcript.push(text);
        self.owe(append);
    }

    /// Ends the transcript's current line, leaving it in the document.
    // Task 12's end-of-turn is the first caller: a turn ends whether or not the
    // last delta happened to carry a newline.
    #[allow(dead_code)]
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

    /// What this phase does with input.
    ///
    /// Ctrl-D leaves -- the shell's own contract, and the one byte that means
    /// anything before there is a composer to type into -- and every other byte
    /// is discarded, because there is nothing yet that it could mean. Task 8's
    /// escape decoder takes this over and routes typed events instead; until it
    /// does, discarding is the honest behaviour rather than a queue nothing
    /// reads.
    pub(crate) fn route_bytes(&mut self, bytes: &[u8]) {
        if bytes.contains(&super::END_OF_TRANSMISSION) {
            self.leave();
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

    #[test]
    fn ctrl_d_leaves_and_nothing_else_does() {
        let mut shell = shell(24, 80);
        shell.route_bytes(b"hello\x1b[A");
        assert!(!shell.leaving(), "an ordinary keystroke ended the session");
        shell.route_bytes(&[0x04]);
        assert!(shell.leaving(), "Ctrl-D did not leave");
    }

    #[test]
    fn ctrl_d_leaves_from_the_middle_of_a_burst_as_well_as_from_its_own_read() {
        // A paste that ends in Ctrl-D arrives as one read, and a loop that only
        // looked at the first byte of it would keep waiting for input the
        // terminal has already delivered.
        let mut shell = shell(24, 80);
        shell.route_bytes(b"abc\x04def");
        assert!(shell.leaving());
    }
}
