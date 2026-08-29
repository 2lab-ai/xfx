//! The screen a change too large for the band is reviewed on.
//!
//! The band's question quotes 160 bytes of a change
//! (`crate::permission::authority::MAX_EXCERPT_BYTES`) inside a sentence, which
//! is the right disclosure for replacing one word and no disclosure at all for
//! replacing a file. So a question whose change outruns that sentence is asked
//! somewhere else: the terminal's **alternate** buffer, for the length of the
//! question and not one frame longer ([`super::frame::Band::restore_primary`]).
//!
//! What is here is only the surface. Which questions come to it is
//! [`super::approval::ApprovalSurface`]'s (the change decides, never the
//! terminal's height); whose screen it is at any instant is
//! [`super::shell::ScreenOwner`]'s; the bytes that take the plane and give it
//! back are [`super::frame`]'s.
//!
//! # Every string on this screen is made row-safe **here**
//!
//! `super::bridge::inert` already turns the controls in a `UiEvent` into spaces
//! at the channel, and `crate::permission::bounded_diff_side` already escaped
//! the diff at the permission boundary. Neither is trusted, and the reason is
//! not defensiveness -- it is that this surface asks a **different** question of
//! the text than either of them answered:
//!
//! * A raw `\n` is inert to a terminal's *state* and is not inert to a **row**:
//!   it moves the cursor down one line in the middle of a screen whose rows this
//!   module places by number, so one arriving here paints the rest of the diff
//!   one row lower than the layout believes, and the choices last.
//! * A bidirectional override (`U+202A`-`U+202E`, `U+2066`-`U+2069`, `U+200E`,
//!   `U+200F`, `U+061C`) is neither a control nor a state change: it is a
//!   *display* instruction that reorders the glyphs after it. On a screen whose
//!   whole purpose is showing a person what a file is about to become, a
//!   reordering that survives is a change that reads as something other than
//!   what will be written.
//!
//! So every string this module paints goes through [`safe_rows`], which splits
//! on real line breaks, replaces every control and every bidi format character,
//! and only then wraps. A caller that hands this an already-escaped string pays
//! nothing: escaping an escaped string is a no-op, and the alternative is
//! trusting a producer this file cannot see.

use crate::permission::{ApprovalAnswer, ApprovalRequest};

use super::approval::{self, Action};
use super::frame::clip;

/// What every row but the title is written into.
const INDENT: &str = "  ";

/// How many cells [`INDENT`] costs.
const INDENT_CELLS: u16 = 2;

/// What marks the choice Enter would take.
const MARKER: &str = "> ";

/// How many rows the tool-and-target line may take.
const SUBJECT_ROWS: usize = 2;

/// How many rows of the summary the screen shows.
///
/// Two, and the rest of the disclosure is the diff below it: the summary is a
/// sentence *about* the change and the change itself is what this screen exists
/// for, so a summary given more rows would be taking them from the thing the
/// user came here to read.
const SUMMARY_ROWS: usize = 2;

/// What the two halves of the change are called on this screen.
const BEFORE_HEADER: &str = "before";
const AFTER_HEADER: &str = "after";

/// What a side with nothing in it says, so an empty half reads as a fact rather
/// than as a rendering fault.
const EMPTY_SIDE: &str = "(nothing)";

/// What a question with no diff at all shows in the viewport.
///
/// Unreachable through [`super::approval::ApprovalSurface::for_request`], which
/// sends nothing here without a diff -- and written down rather than
/// `unreachable!()`, because a screen with a hole in it is a worse failure than
/// a screen that says what it does not have.
const NO_DIFF: &str = "(this change has no before and after to show)";

/// The bidirectional format characters, which are neither controls nor
/// printable text: they reorder the glyphs around them.
///
/// The embeddings and overrides (`U+202A`-`U+202E`), the isolates
/// (`U+2066`-`U+2069`), the two marks (`U+200E`, `U+200F`) and the Arabic letter
/// mark (`U+061C`). Replaced rather than dropped, for the reason a control is:
/// the reader is shown that the payload carried one.
fn reorders(character: char) -> bool {
    matches!(
        character,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

/// What a character a terminal would obey becomes on this screen.
///
/// The three whitespace controls keep their names, exactly as the permission
/// boundary spells them (`crate::permission::bounded_diff_side`), so a diff that
/// arrived already escaped is unchanged by passing through here again. Every
/// other control, and every reordering character, becomes `U+FFFD`: visible,
/// one cell wide, and inert.
fn tamed(character: char) -> Option<&'static str> {
    match character {
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        _ if character.is_control() || reorders(character) => Some("\u{fffd}"),
        _ => None,
    }
}

/// `text` as rows of a `budget`-wide screen, with nothing left in it a terminal
/// would obey or reorder.
///
/// The **split comes first**: a real line break in the payload is a row break
/// here, so a diff that carries newlines reads as the file does. What is then
/// escaped is what is left inside each line, and the wrap runs last -- on text
/// whose every character is one the wrap can measure.
fn safe_rows(text: &str, budget: u16) -> Vec<String> {
    let budget = budget.max(1);
    let mut out = Vec::new();
    for line in text.split(['\n', '\r']) {
        let mut safe = String::with_capacity(line.len());
        for character in line.chars() {
            match tamed(character) {
                Some(replacement) => safe.push_str(replacement),
                None => safe.push(character),
            }
        }
        if safe.is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(
            super::wrap::wrap(&safe, budget)
                .iter()
                .map(|row| safe[row.start..row.end].to_string()),
        );
    }
    out
}

/// A question about a change the band cannot show, and the screen it is shown
/// on.
///
/// It owns the request rather than borrowing it, because it outlives the event
/// the request arrived on and because the alternate plane is the only thing
/// holding that question while it is up: there is no panel behind it.
pub(crate) struct ApprovalScreen {
    request: ApprovalRequest,
    /// The first row of the change this screen is showing.
    ///
    /// A bounded diff is up to 128 KiB (`crate::permission::ApprovalDiff`) and
    /// a screen is a few dozen rows, so the viewport is a window onto it. It is
    /// moved by [`Self::scroll_by`], which is what the keys that walk the change
    /// bind to.
    scroll: usize,
    /// Which of the three answers is marked, as an index into the choices.
    selected: usize,
}

/// One composed screen: its rows, and the row the caret belongs on.
///
/// Both come out of one construction, for the reason
/// `super::approval::Panel::rows` gives: the height, the caret's row and the
/// paint are three readings of one layout, and three constructions are three
/// chances to disagree.
struct Composed {
    rows: Vec<String>,
    /// The caret's row, one-based, as the terminal counts.
    caret: u16,
    /// How many rows of the change the viewport is showing.
    viewport: usize,
}

impl ApprovalScreen {
    pub(crate) fn new(request: ApprovalRequest) -> Self {
        Self {
            request,
            scroll: 0,
            // Yes-once, the same default the band's panel starts on: an Enter
            // pressed without reading grants one call rather than the session.
            selected: 0,
        }
    }

    /// What one keystroke does. `Some` is an answer; `None` moved something.
    ///
    /// Routed through the same function the band's panel answers with
    /// (`super::approval::answered`), so "which key means which answer" is one
    /// fact on both surfaces rather than two that can drift.
    pub(crate) fn apply(&mut self, action: Action) -> Option<ApprovalAnswer> {
        approval::answered(action, &mut self.selected)
    }

    /// Moves the viewport `delta` rows, bounded by what there is to show.
    ///
    /// `shown` is how many rows of the change the screen is currently giving the
    /// viewport, so the last row of a diff can always be reached and the window
    /// can never be walked past it into blank rows -- a scroll that ran off the
    /// end would read as a change that had ended when it had not.
    pub(crate) fn scroll_by(&mut self, delta: isize, cols: u16, terminal_rows: u16) {
        let composed = self.compose(cols, terminal_rows);
        let total = self.change_rows(cols).len();
        let last = total.saturating_sub(composed.viewport);
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta.unsigned_abs())
        }
        .min(last);
    }

    /// The whole screen, top first: exactly `terminal_rows` rows.
    ///
    /// A **full** screen rather than a band's worth, because that is what the
    /// alternate buffer is: every row of it is this module's, and a row left
    /// unwritten is whatever the terminal's other buffer happened to hold.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        self.compose(cols, terminal_rows).rows
    }

    /// Where the caret goes: the terminal's own row, and the cells to the left
    /// of it on that row.
    ///
    /// On the marked choice, for the reason the panel's is: the caret is what
    /// the terminal says the next keystroke goes to, and this screen has the
    /// focus while it is up.
    pub(crate) fn caret(&self, cols: u16, terminal_rows: u16) -> (u16, u16) {
        (self.compose(cols, terminal_rows).caret, 0)
    }

    /// The rows above the change: what is being asked, and about what.
    fn heading(&self, cols: u16) -> Vec<String> {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        let mut rows = vec![approval::TITLE.to_string()];
        // Two rows for the subject, because a target is a path and a path is
        // routinely longer than a row: one row would show `edit_file` and stop
        // at the word boundary in front of the thing being edited.
        rows.extend(
            safe_rows(
                &format!("{} {}", self.request.tool, self.request.target),
                budget,
            )
            .into_iter()
            .take(SUBJECT_ROWS)
            .map(|row| format!("{INDENT}{row}")),
        );
        rows.push(String::new());
        let summary = safe_rows(&self.request.summary, budget);
        rows.extend(
            summary
                .into_iter()
                .take(SUMMARY_ROWS)
                .map(|row| format!("{INDENT}{row}")),
        );
        rows.push(String::new());
        rows
    }

    /// The rows below it: the answers, and what the second one grants.
    ///
    /// **What a question may never lose.** It is placed before the change is,
    /// and the change gets what is left -- so a screen too short for everything
    /// is a screen with less of the diff on it, never one with the choices
    /// below its last row.
    fn choices(&self, cols: u16) -> Vec<String> {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        let mut rows = vec![String::new()];
        for (index, label) in approval::labels(self.request.tool).iter().enumerate() {
            let marker = if index == self.selected {
                MARKER
            } else {
                INDENT
            };
            rows.push(format!("{marker}{label}"));
        }
        rows.extend(
            safe_rows(&format!("2 = {}", self.request.always_scope), budget)
                .into_iter()
                .take(1)
                .map(|row| format!("{INDENT}{row}")),
        );
        rows
    }

    /// The change itself, as rows, before any window is taken of it.
    fn change_rows(&self, cols: u16) -> Vec<String> {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        let Some(diff) = self.request.diff.as_ref() else {
            return vec![NO_DIFF.to_string()];
        };
        let mut rows = Vec::new();
        for (header, side) in [(BEFORE_HEADER, &diff.before), (AFTER_HEADER, &diff.after)] {
            if !rows.is_empty() {
                rows.push(String::new());
            }
            rows.push(header.to_string());
            let side = safe_rows(side, budget);
            if side.iter().all(String::is_empty) {
                rows.push(format!("{INDENT}{EMPTY_SIDE}"));
                continue;
            }
            rows.extend(side.into_iter().map(|row| format!("{INDENT}{row}")));
        }
        rows
    }

    /// One screen: the heading, as much of the change as fits, and the choices.
    fn compose(&self, cols: u16, terminal_rows: u16) -> Composed {
        let height = usize::from(terminal_rows);
        // The choices first, because they are the part that may not be dropped.
        let mut choices = self.choices(cols);
        choices.truncate(height);
        let room = height - choices.len();
        let mut heading = self.heading(cols);
        heading.truncate(room);
        let viewport = room - heading.len();

        let change = self.change_rows(cols);
        let mut rows = heading;
        rows.extend(
            change
                .into_iter()
                .skip(self.scroll)
                .take(viewport)
                .map(|row| format!("{INDENT}{row}")),
        );
        rows.resize(height - choices.len(), String::new());
        // The first choice sits one row below the blank row the block opens
        // with, and the caret is one-based.
        let caret = u16::try_from(rows.len() + 2 + self.selected).unwrap_or(terminal_rows);
        rows.extend(choices);
        Composed {
            rows: rows.iter().map(|row| clip(row, cols).to_string()).collect(),
            caret: caret.min(terminal_rows).max(1),
            viewport,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::permission::ApprovalDiff;

    fn asked_about(before: &str, after: &str) -> ApprovalRequest {
        ApprovalRequest {
            tool: "edit_file",
            target: "notes.txt".to_string(),
            summary: "edit `notes.txt`: replace the whole of it".to_string(),
            always_scope:
                "allow every future edit_file to `notes.txt` for the rest of this session"
                    .to_string(),
            diff: Some(ApprovalDiff {
                before: before.to_string(),
                after: after.to_string(),
            }),
        }
    }

    fn screen() -> ApprovalScreen {
        ApprovalScreen::new(asked_about(&"alpha ".repeat(200), &"beta ".repeat(200)))
    }

    #[test]
    fn the_screen_is_exactly_as_tall_as_the_terminal_and_no_wider() {
        // Every row of the alternate buffer is this module's: one it did not
        // write is one the terminal's other buffer is still showing.
        for (rows, cols) in [(24u16, 80u16), (40, 120), (10, 20), (6, 20)] {
            let painted = screen().rows(cols, rows);
            assert_eq!(painted.len(), usize::from(rows), "{rows}x{cols}");
            for row in painted {
                assert!(
                    super::super::wrap::width(&row) <= cols,
                    "{row:?} is wider than {cols} cells"
                );
            }
        }
    }

    #[test]
    fn a_question_never_loses_its_answers_however_short_the_screen_is() {
        // The one thing a screen too short may not drop. A change is a
        // disclosure and the choices are the question; a screen with the
        // change on it and the answers below its last row leaves a session
        // waiting for a keystroke about something nobody can act on.
        for rows in [6u16, 8, 10, 24, 40] {
            let painted = screen().rows(80, rows).join("\n");
            for choice in ["1. Yes", "2. Yes, and", "3. No (esc)"] {
                assert!(
                    painted.contains(choice),
                    "a {rows}-row screen dropped {choice:?}: {painted:?}"
                );
            }
        }
    }

    #[test]
    fn the_caret_sits_on_the_marked_choice_and_walks_with_it() {
        let mut screen = screen();
        let marked = |screen: &ApprovalScreen| {
            let (row, column) = screen.caret(80, 24);
            assert_eq!(column, 0, "the caret left the first column");
            screen.rows(80, 24)[usize::from(row) - 1].clone()
        };
        assert_eq!(marked(&screen), "> 1. Yes");
        assert_eq!(screen.apply(Action::Down), None);
        assert!(marked(&screen).starts_with("> 2. Yes, and"));
        assert_eq!(screen.apply(Action::Up), None);
        assert_eq!(marked(&screen), "> 1. Yes");
    }

    #[test]
    fn every_answer_the_band_gives_is_the_answer_this_screen_gives() {
        // One vocabulary on both surfaces. A screen that answered `2` with
        // something else would make the surface -- which the *change* chooses,
        // not the user -- decide what a keystroke means.
        for (action, answer) in [
            (Action::Text('1'), ApprovalAnswer::Once),
            (Action::Text('2'), ApprovalAnswer::Always),
            (Action::Text('3'), ApprovalAnswer::Deny),
            (Action::Escape, ApprovalAnswer::Deny),
            (Action::Cancel, ApprovalAnswer::Deny),
            (Action::Submit, ApprovalAnswer::Once),
        ] {
            assert_eq!(screen().apply(action), Some(answer), "{action:?}");
        }
        assert_eq!(screen().apply(Action::Text('9')), None);
    }

    #[test]
    fn the_change_is_on_the_screen_and_both_of_its_halves_are_named() {
        let painted = screen().rows(80, 40).join("\n");
        assert!(painted.contains("before"), "{painted:?}");
        assert!(painted.contains("after"), "{painted:?}");
        assert!(painted.contains("alpha"), "{painted:?}");
        assert!(
            painted.contains("Permission needed"),
            "the screen does not name itself: {painted:?}"
        );
        assert!(
            painted.contains("notes.txt"),
            "the screen does not say what the change is to: {painted:?}"
        );
    }

    #[test]
    fn a_raw_newline_in_a_change_is_a_row_break_and_never_a_cursor_move() {
        // The row-safety rule this module exists to keep. A `\n` written into
        // the middle of a screen whose rows are placed by number moves the
        // terminal's cursor, and every row after it lands one row low -- with
        // the choices falling off the bottom.
        let screen = ApprovalScreen::new(asked_about("first\nsecond", "third"));
        let painted = screen.rows(80, 24);
        // On **two rows**, and that is the whole claim: a break escaped into
        // `\\n` and left in the middle of one row is inert to the terminal and
        // is not the file, and a break passed through unescaped moves the
        // cursor. Splitting is the only answer that is both.
        let first = painted
            .iter()
            .position(|row| row.contains("first"))
            .unwrap_or_else(|| panic!("{painted:?}"));
        let second = painted
            .iter()
            .position(|row| row.contains("second"))
            .unwrap_or_else(|| panic!("{painted:?}"));
        assert_eq!(
            second,
            first + 1,
            "the line break was escaped into a row instead of ending one: {painted:?}"
        );
        for row in &painted {
            assert!(
                !row.contains('\n') && !row.contains('\r'),
                "a line break survived into a row: {row:?}"
            );
        }
        assert!(
            painted.iter().any(|row| row.starts_with("> 1. Yes")),
            "the line break pushed the choices off the screen: {painted:?}"
        );
    }

    #[test]
    fn nothing_a_terminal_would_obey_or_reorder_reaches_a_row() {
        // Both classes, because they are different failures. A control is a
        // sequence the terminal *executes*; a bidi override is a display
        // instruction it *obeys* -- and neither is `char::is_control` alone.
        let hostile = "\u{1b}[2J\u{9b}31m\u{9d}0;pwned\u{7}\u{0}\u{202e}drawkcab\u{2069}";
        let screen = ApprovalScreen::new(asked_about(hostile, hostile));
        for row in screen.rows(80, 40) {
            for character in row.chars() {
                assert!(
                    !character.is_control(),
                    "a control character reached a row: {row:?}"
                );
                assert!(
                    !reorders(character),
                    "a reordering character reached a row: {row:?}"
                );
            }
        }
    }

    #[test]
    fn a_change_already_escaped_at_the_permission_boundary_is_unchanged_here() {
        // The diff arrives having been escaped once
        // (`crate::permission::bounded_diff_side`), and escaping an escaped
        // string has to be a no-op or the second pass would double every
        // backslash and the reader would be shown a change nobody is making.
        let escaped = "alpha\\nbeta\\tgamma";
        let screen = ApprovalScreen::new(asked_about(escaped, escaped));
        let painted = screen.rows(80, 40).join("\n");
        assert!(painted.contains("alpha\\nbeta\\tgamma"), "{painted:?}");

        // And a **raw** tab is given the same name the permission boundary
        // gives it, rather than being expanded or replaced: a tab painted as
        // cells would put the rest of its row somewhere the layout did not
        // measure, and one replaced by `U+FFFD` would read as a byte the
        // payload never carried.
        let raw = ApprovalScreen::new(asked_about("alpha\tbeta", "gamma"));
        let painted = raw.rows(80, 40).join("\n");
        assert!(
            painted.contains("alpha\\tbeta"),
            "a raw tab was not given its name: {painted:?}"
        );
    }

    #[test]
    fn the_viewport_walks_the_change_and_stops_at_its_end() {
        // A window onto up to 128 KiB, and a window that could be walked past
        // the end would read as a change that had stopped when it had not.
        let mut screen = screen();
        let first = screen.rows(80, 24).join("\n");
        screen.scroll_by(5, 80, 24);
        let moved = screen.rows(80, 24).join("\n");
        assert_ne!(first, moved, "the viewport did not move");

        screen.scroll_by(1_000_000, 80, 24);
        let end = screen.rows(80, 24);
        assert!(
            end.iter().any(|row| row.starts_with("> 1. Yes")),
            "the walk to the end lost the choices: {end:?}"
        );
        let same = end.join("\n");
        screen.scroll_by(1, 80, 24);
        assert_eq!(
            screen.rows(80, 24).join("\n"),
            same,
            "the viewport was walked past the end of the change"
        );

        screen.scroll_by(-1_000_000, 80, 24);
        assert_eq!(
            screen.rows(80, 24).join("\n"),
            first,
            "the viewport did not come back to the top"
        );
    }

    #[test]
    fn a_side_with_nothing_in_it_says_so_rather_than_showing_a_blank() {
        let screen = ApprovalScreen::new(asked_about("", "beta"));
        let painted = screen.rows(80, 24).join("\n");
        assert!(painted.contains(EMPTY_SIDE), "{painted:?}");
    }
}
