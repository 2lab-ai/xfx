//! What the band is a picture of.
//!
//! The event loop reads bytes and writes frames; everything between the two --
//! what the band's rows say, where the caret is, and whether the session is
//! leaving -- is here, so that "what would the band look like now" is a
//! question about a value rather than about a terminal.
//!
//! In this phase that value is small. The band is a divider, a composer showing
//! its prompt marker, and a hint row the session owns and leaves empty until
//! the phase that fills it; there is no transcript, no turn, and no editor yet,
//! and the tasks that add each of them add it here. What is already true is the
//! shape: the rows are produced top-down from the geometry, and the caret is
//! reported in the composer's own coordinates rather than derived a second time
//! by whatever draws it.

use super::layout::Geometry;
use super::render_request::{Reason, RenderRequest};
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
