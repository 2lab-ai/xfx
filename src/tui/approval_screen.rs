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
//! the text than either of them answered.
//!
//! One of the two deliberately hands this module something to do: the diff's
//! **line breaks are kept** at the permission boundary, because they are the
//! change's own structure and this is the surface that can show it. Turning
//! them into rows is this module's half of that contract, and it is why the
//! split below comes first.
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

use super::approval::{self, Action, Composition};
use super::approval_readiness::Disclosure;
use super::frame::clip;

/// What every row but the title is written into.
const INDENT: &str = "  ";

/// How many cells [`INDENT`] costs.
const INDENT_CELLS: u16 = 2;

/// What marks the choice Enter would take.
const MARKER: &str = "> ";

/// How many rows the status line may take.
///
/// Bounded, because it is the one block on this screen that is **not** the
/// question: a notice allowed to grow would push the change out of the viewport
/// in order to explain why the change could not be approved.
const STATUS_ROWS: usize = 2;

/// Which row of the choices block the last control is on.
///
/// The block opens with a blank row, so the three answers are rows one, two and
/// three of it -- and the last of them is the row a readiness disclosure reports
/// ([`super::approval_readiness::Disclosure::last_control_row`]). Named rather
/// than written as `3` at each of its three uses, because the three are the same
/// fact about one layout.
const CONTROL_ROWS: usize = 3;

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
/// Reached only for what is left **inside** a line: [`safe_rows`] splits on real
/// breaks first, so the `\n` arm here is for a break that arrived from
/// somewhere this module does not trust rather than for the ones the permission
/// boundary keeps. The other two whitespace controls keep the names that
/// boundary gives them (`crate::permission::bounded_diff_side`), so a diff that
/// arrived already escaped is unchanged by passing through here again -- and a
/// backslash is passed through untouched for the same reason: the escaping was
/// done once, where the change was known, and doing it twice would show a
/// reader an escape the payload never carried.
///
/// Everything else -- every other control, and every reordering character --
/// is named by its **code point** (`crate::permission::scalar_token`), the same
/// spelling the permission boundary spends on a control. Not one replacement
/// character for all of them: a screen that painted `ESC` and `BEL` the same
/// way, or one bidi override the same as another, would show two payloads a
/// model can swap as one screen. The bidi characters are this pass's own
/// business rather than the boundary's, because they are neither controls nor
/// text and the boundary passes them through as the printable characters they
/// technically are.
fn tamed(character: char) -> Option<String> {
    match character {
        '\n' => Some("\\n".to_string()),
        '\r' => Some("\\r".to_string()),
        '\t' => Some("\\t".to_string()),
        _ if character.is_control() || reorders(character) => {
            Some(crate::permission::scalar_token(character))
        }
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
                Some(replacement) => safe.push_str(&replacement),
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

/// Whether the painter's cut takes nothing but trailing blanks off `row`.
///
/// The wrap breaks **at** a space and keeps it ([`safe_rows`] slices the range
/// the wrap returned), so a row can be one cell wider than the screen while
/// carrying no character the screen does not show -- and the painter's clip then
/// removes exactly that space. A disclosure is a claim about text the user was
/// not shown, and a space is not text: comparing the raw strings would report
/// every wrapped sentence as cut, which is how a screen that discloses
/// everything comes to be ungrantable.
fn uncut(row: &str, cols: u16) -> bool {
    clip(row, cols).trim_end() == row.trim_end()
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
    /// What the screen has to tell the user about their last keystroke.
    ///
    /// **This plane's own status line**, and it exists because this plane owns
    /// the whole terminal while it is up: `super::shell::Shell::say` writes into
    /// the document, the document is on the *primary* buffer, and a refusal
    /// written there while a question holds the alternate buffer is a refusal
    /// nobody can see. A user whose `1` did nothing and said nothing presses it
    /// again, harder, which is the one response this gate must not train.
    ///
    /// Part of the composition rather than a write of its own, so it is placed,
    /// measured and reconciled with every other row of the screen -- and so that
    /// a screen carrying a notice is a *different* screen, which is exactly what
    /// a readiness receipt should be about.
    status: Option<String>,
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
    /// What this composition really put in front of the user, decided **here**
    /// rather than by reading the rows back: every cut this screen makes is
    /// silent, so the only reader that can answer honestly is the one that made
    /// them ([`super::approval_readiness`]).
    disclosure: Disclosure,
}

impl ApprovalScreen {
    pub(crate) fn new(request: ApprovalRequest) -> Self {
        Self {
            request,
            scroll: 0,
            // Yes-once, the same default the band's panel starts on: an Enter
            // pressed without reading grants one call rather than the session.
            selected: 0,
            status: None,
        }
    }

    /// Puts a sentence about the last keystroke on this plane's status line.
    ///
    /// The alternate half of `super::shell::Shell::say`: while this screen is up
    /// it owns every row the user can see, so a refusal has to be painted here
    /// or it is not painted at all.
    pub(crate) fn set_status(&mut self, status: String) {
        self.status = Some(status);
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

    /// The same screen, with what it really disclosed beside it.
    ///
    /// A **projection** of [`Self::compose`] rather than a second layout: the
    /// caret and the viewport stay private to this module, and the rows a
    /// readiness receipt is about are byte for byte the rows [`Self::rows`]
    /// hands the painter.
    pub(crate) fn composition(&self, cols: u16, terminal_rows: u16) -> Composition {
        let composed = self.compose(cols, terminal_rows);
        Composition {
            rows: composed.rows,
            disclosure: composed.disclosure,
        }
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

    /// Whether this screen can put **all three** answers in front of the user
    /// at this size.
    ///
    /// Asked of the composition itself rather than of a remembered minimum, and
    /// that is the whole of why it is honest: [`Self::compose`] is what drops
    /// rows when the screen runs out of them, so the only way to know what a
    /// screen loses is to compose it and look. Two ways to lose an answer, and
    /// both are checked here -- a row truncated off the bottom, and a row
    /// clipped so far that nothing of the label survives beside its marker.
    ///
    /// A `false` is a screen on which xfx cannot ask, and
    /// [`super::shell::Shell::ask`] refuses on the user's behalf rather than
    /// leaving a session waiting for a keystroke about a question with no
    /// visible answers -- the same rule [`super::layout::fits_panel`] applies
    /// to the band's own panel. Every screen a session can really run on is
    /// above this bound (`super::layout::MIN_ROWS`/`MIN_COLS`), which is
    /// pinned in `super::shell`; the guard is the hazard written down where
    /// the next edit reads it.
    pub(crate) fn presents_choices(&self, cols: u16, terminal_rows: u16) -> bool {
        let painted = self.compose(cols, terminal_rows).rows;
        approval::labels(self.request.tool)
            .iter()
            .enumerate()
            .all(|(index, label)| {
                let marker = if index == self.selected {
                    MARKER
                } else {
                    INDENT
                };
                let whole = format!("{marker}{label}");
                let shown = clip(&whole, cols);
                shown.len() > marker.len() && painted.iter().any(|row| row == shown)
            })
    }

    /// The rows above the change: what is being asked, and about what.
    fn heading(&self, cols: u16) -> Vec<String> {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        let mut rows = vec![approval::TITLE.to_string()];
        // Two rows for the subject, because a target is a path and a path is
        // routinely longer than a row: one row would show `edit_file` and stop
        // at the word boundary in front of the thing being edited.
        // **As many rows as the path really needs**, with [`SUBJECT_ROWS`] as a
        // floor rather than a ceiling: a workspace path is routinely longer than
        // two rows of an eighty-column screen, and a `.take(2)` here cut it
        // silently -- which made the target something the user was never shown
        // and the request something no frame could disclose.
        rows.extend(
            safe_rows(
                &format!("{} {}", self.request.tool, self.request.target),
                budget,
            )
            .into_iter()
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
        // **Every row the scope needs.** Each one `crate::permission` builds
        // carries an unconditional suffix -- the resume-id of a saved session,
        // or the note that this turn is not being recorded -- so a real scope is
        // three wrapped rows at eighty columns and the `.take(1)` this replaced
        // showed a third of it. What "always" buys is half the question.
        rows.extend(
            safe_rows(&format!("2 = {}", self.request.always_scope), budget)
                .into_iter()
                .map(|row| format!("{INDENT}{row}")),
        );
        // Last, so that a screen too short to hold everything loses the notice
        // before it loses the scope, and the scope before it loses an answer.
        if let Some(status) = self.status.as_ref() {
            rows.push(String::new());
            rows.extend(
                safe_rows(status, budget)
                    .into_iter()
                    .take(STATUS_ROWS)
                    .map(|row| format!("{INDENT}{row}")),
            );
        }
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

    /// How many rows the tool-and-target line really wants.
    ///
    /// The **source** measurement the disclosure is about. [`Self::heading`]
    /// now emits exactly this many, so the only way to lose one is the
    /// truncation [`Self::compose`] applies when the screen has no room -- which
    /// is a fact about the screen and is checked there.
    fn subject_rows(&self, cols: u16) -> usize {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        safe_rows(
            &format!("{} {}", self.request.tool, self.request.target),
            budget,
        )
        .len()
    }

    /// How many rows what "always" would buy really wants.
    fn scope_rows(&self, cols: u16) -> usize {
        let budget = cols.saturating_sub(INDENT_CELLS).max(1);
        safe_rows(&format!("2 = {}", self.request.always_scope), budget).len()
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

        // What the composition really disclosed, decided **here** because here
        // is the only place that knows what was cut: both truncations above are
        // silent, so a reader given only `rows` could not tell a subject that
        // ends at row two from one that was stopped there.
        //
        // The subject: every row of it survived `heading.truncate` and the clip
        // below changes nothing about it. The rows are `heading[1..=wanted]`,
        // since the title is first.
        let wanted = self.subject_rows(cols);
        let subject_whole =
            heading.len() > wanted && heading[1..=wanted].iter().all(|row| uncut(row, cols));
        // The controls: all three rows survived `choices.truncate` -- the blank
        // row the block opens with is `choices[0]` -- and each is unclipped.
        // **Unclipped, not merely non-empty**: see [`Self::presents_choices`],
        // which asks the weaker question because it is answering a different
        // one.
        let mut controls_whole = choices.len() > CONTROL_ROWS;
        for (index, label) in approval::labels(self.request.tool).iter().enumerate() {
            let marker = if index == self.selected {
                MARKER
            } else {
                INDENT
            };
            let whole = format!("{marker}{label}");
            controls_whole &= uncut(&whole, cols);
        }
        // The always-scope: [`Self::choices`] emits every row of it, so the only
        // way to lose one is `choices.truncate` above -- a screen with no room.
        // The rows sit directly after the three answers.
        let scope_wanted = self.scope_rows(cols);
        let scope_end = CONTROL_ROWS + 1 + scope_wanted;
        let scope_whole = choices.len() >= scope_end
            && choices[CONTROL_ROWS + 1..scope_end]
                .iter()
                .all(|row| uncut(row, cols));

        let change = self.change_rows(cols);
        // At least one row of the change is inside the window. Both halves
        // matter: a screen with no room left for it shows none, and a viewport
        // walked to the far side of a short change would show none either.
        let change_visible = viewport > 0 && self.scroll < change.len();
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
        // The plane owns every row of the screen, so a row of the composition
        // **is** a terminal row -- one-based, which is the only correction.
        let last_control = rows.len() + CONTROL_ROWS;
        rows.extend(choices);
        let disclosure = Disclosure {
            last_control_row: u16::try_from(last_control + 1).unwrap_or(u16::MAX),
            controls_whole,
            scope_whole,
            subject_whole,
            change_visible,
        };
        Composed {
            rows: rows.iter().map(|row| clip(row, cols).to_string()).collect(),
            caret: caret.min(terminal_rows).max(1),
            viewport,
            disclosure,
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
    fn a_screen_that_cannot_show_all_three_answers_says_so() {
        // The other half of the case above, and the one the shell acts on: a
        // screen the question cannot be *asked* on is refused rather than
        // painted with an answer missing. Two ways to lose one, and the bound
        // is different for each -- a row that never survived the truncation,
        // and a row clipped down to its own marker.
        let screen = screen();
        for rows in [4u16, 5, 6, 8, 10, 24, 40] {
            assert!(
                screen.presents_choices(80, rows),
                "a {rows}-row screen was refused a question it can show"
            );
        }
        for rows in [1u16, 2, 3] {
            assert!(
                !screen.presents_choices(80, rows),
                "a {rows}-row screen claimed to show three answers it has no rows for"
            );
        }
        assert!(
            !screen.presents_choices(2, 24),
            "a screen two cells wide claimed to show answers clipped to their markers"
        );
        // And the narrowest screen a band -- and therefore a session -- exists
        // on is comfortably above that bound.
        assert!(screen.presents_choices(super::super::layout::MIN_COLS, 24));
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
    fn a_change_built_at_the_permission_boundary_keeps_its_lines_and_its_literals_apart() {
        // **End to end through the boundary, not by hand.** The pair a reviewer
        // found: a payload whose line breaks are real, and one that merely
        // contains a backslash and an `n`. They are two different files, and a
        // screen that showed them the same way would let a model replace a
        // hundred-line file with one line of literal escapes and call it a
        // no-op. What the screen owes is not merely two different strings --
        // it is two different **shapes**: rows where the breaks are real, and a
        // visible escape where they are spelled out.
        let real = "A\n".repeat(3);
        let literal = "A\\n".repeat(3);
        let screen = ApprovalScreen::new(ApprovalRequest {
            tool: "edit_file",
            target: "notes.txt".to_string(),
            summary: "edit `notes.txt`".to_string(),
            always_scope: "allow every future edit_file to `notes.txt`".to_string(),
            diff: Some(crate::permission::ApprovalDiff::of(&real, &literal)),
        });
        let painted = screen.rows(80, 40);
        let at = |needle: &str| {
            painted
                .iter()
                .position(|row| row.trim() == needle)
                .unwrap_or_else(|| panic!("no row is exactly {needle:?}: {painted:?}"))
        };
        let before = at(BEFORE_HEADER);
        let after = at(AFTER_HEADER);

        // The side whose breaks are real is **rows**: one per line, each
        // carrying the line and nothing else.
        let rows: Vec<&String> = painted[before + 1..after].iter().collect();
        assert_eq!(
            rows.iter().filter(|row| row.trim() == "A").count(),
            3,
            "the file's three lines are not three rows: {rows:?}"
        );
        for row in &rows {
            assert!(
                !row.contains("\\n"),
                "a real line break was spelled out instead of ending a row: {row:?}"
            );
        }

        // The side that only *spells* a break is one row, and the escape is
        // visible on it -- doubled, because the payload's own backslash is
        // escaped too, which is what keeps the two apart.
        let shown: String = painted[after + 1..].join("");
        assert!(
            shown.contains("A\\\\nA\\\\nA\\\\n"),
            "the literal backslash-n was not shown as a literal: {shown:?}"
        );
        assert!(
            !painted[after + 1..]
                .iter()
                .any(|row| row.trim() == "A" && !row.contains('\\')),
            "the spelled-out breaks were rendered as rows, so the two sides read alike"
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
    fn two_characters_a_row_may_not_carry_are_not_shown_as_one() {
        // The screen's half of the same rule the permission boundary keeps: a
        // character that may not reach a row is **named** rather than blanked,
        // so two of them that a payload could swap are two different screens.
        // Both classes, because they are two different failures -- a control is
        // a sequence the terminal executes, a bidi override is a display
        // instruction it obeys -- and neither is `char::is_control` alone.
        let shown =
            |payload: &str| ApprovalScreen::new(asked_about(payload, "unchanged")).rows(80, 40);
        for (one, other) in [
            ('\u{1b}', '\u{7}'),
            ('\u{9b}', '\u{9d}'),
            ('\u{202e}', '\u{202d}'),
            ('\u{2066}', '\u{2069}'),
        ] {
            let first = shown(&one.to_string().repeat(4));
            let second = shown(&other.to_string().repeat(4));
            assert_ne!(
                first, second,
                "{one:?} and {other:?} are painted as the same screen"
            );
            // Row by row, because the rows are what the terminal is given: a
            // join would put a line break between them and then measure the
            // separator this test wrote rather than the screen.
            for painted in [&first, &second] {
                for row in painted {
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
            // And what stands in for them names which one it was.
            assert!(
                first
                    .join("")
                    .contains(&crate::permission::scalar_token(one)),
                "{one:?} is not named on the screen: {first:?}"
            );
        }
    }

    #[test]
    fn a_change_already_escaped_at_the_permission_boundary_is_unchanged_here() {
        // The diff arrives having been escaped once
        // (`crate::permission::bounded_diff_side`), and escaping an escaped
        // string has to be a no-op or the second pass would double every
        // backslash and the reader would be shown a change nobody is making.
        //
        // A tab and a CR, not a line break: the boundary **keeps** real breaks
        // for this surface to turn into rows, and
        // `a_change_built_at_the_permission_boundary_keeps_its_lines_and_its_literals_apart`
        // is where that half is pinned.
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
    fn a_long_target_is_shown_whole_where_there_is_room_and_refused_where_there_is_not() {
        // The truncation that dropped the tail of a path was silent -- no
        // ellipsis, no marker, just a path that stops -- so a rule reading the
        // painted rows back could not tell it from a path that ends there.
        //
        // **A changed coordinate.** The fix is not to keep detecting the cut at
        // an ordinary size; it is to stop cutting. The subject now takes the
        // rows it needs, so a twenty-four-row screen shows this target whole.
        let mut request = asked_about("one", "two");
        request.target = "a/".repeat(120) + "notes.txt";
        let screen = ApprovalScreen::new(request);
        let roomy = screen.composition(80, 24);
        assert!(
            roomy.disclosure.subject_whole,
            "the target was still dropped: {:?}",
            roomy.rows
        );
        assert!(
            roomy.disclosure.controls_whole,
            "the choices are still whole"
        );
        // The **tail** of the path, which is precisely what the old `.take(2)`
        // dropped: a subject reported whole whose last component is missing
        // would be the original defect wearing a `true`.
        assert!(
            roomy.rows.iter().any(|row| row.contains("notes.txt")),
            "the end of the path is not on the screen it was reported disclosed on: {:?}",
            roomy.rows
        );

        // And where the screen really cannot carry it, `compose` truncates and
        // the disclosure says so rather than pretending -- while every answer
        // stays on the screen, because a question that cannot be granted must
        // still be refusable.
        let cramped = screen.composition(80, 8);
        assert!(
            !cramped.disclosure.subject_whole,
            "a screen with no room for the path claimed to have shown it: {:?}",
            cramped.rows
        );
        assert!(
            cramped.disclosure.controls_whole,
            "a short screen dropped an answer: {:?}",
            cramped.rows
        );
    }

    #[test]
    fn a_screen_too_short_for_the_heading_discloses_no_subject() {
        // Six rows: the choices block takes what it may never lose, and the
        // heading is truncated into whatever that left -- room enough for the
        // title and nothing else, so the subject rows never reach the screen at
        // all.
        let composed = ApprovalScreen::new(asked_about("one", "two")).composition(80, 6);
        assert_eq!(
            composed.rows.len(),
            6,
            "the composition still fills the plane"
        );
        assert!(
            !composed.disclosure.subject_whole,
            "the heading was truncated into the choices' room"
        );
    }

    #[test]
    fn the_last_control_row_is_the_row_the_refusal_is_really_painted_on() {
        // The plane owns every row of the screen, so a local index **is** a
        // terminal row -- one-based, which is the only thing that has to be
        // added.
        for rows in [10u16, 24, 40] {
            let composed = ApprovalScreen::new(asked_about("one", "two")).composition(80, rows);
            let refusal = composed
                .rows
                .iter()
                .rposition(|row| row.contains("3. No"))
                .expect("the refusal is on the screen");
            assert_eq!(
                usize::from(composed.disclosure.last_control_row),
                refusal + 1,
                "{rows}-row screen: {:?}",
                composed.rows
            );
        }
    }

    #[test]
    fn a_status_line_is_painted_on_this_plane_because_the_document_is_behind_it() {
        // While this screen is up it owns every row the user can see, so a
        // refusal written into the document (`super::super::shell::Shell::say`)
        // is a refusal on the *primary* buffer, behind the plane, that nobody
        // can read. A gate whose explanation is invisible is a key that appears
        // to do nothing.
        let mut screen = screen();
        let quiet = screen.composition(80, 24);
        screen.set_status("the question is not on the screen yet".to_string());
        let noticed = screen.composition(80, 24);
        assert!(
            noticed
                .rows
                .iter()
                .any(|row| row.contains("the question is not on the screen yet")),
            "the notice never reached the plane that owns the screen: {:?}",
            noticed.rows
        );
        // The answers survive it, and so does what "always" would buy: the
        // notice is the first thing this screen gives up, never the last.
        assert!(noticed.disclosure.controls_whole && noticed.disclosure.scope_whole);
        assert_eq!(
            noticed.rows.len(),
            usize::from(24u16),
            "the notice changed the height of a screen that is the terminal"
        );
        // And a screen carrying a notice is a *different* screen, which is
        // exactly what a readiness receipt must be about: the disclosure moved,
        // so the receipt earned before it cannot answer for it.
        assert_ne!(quiet.disclosure, noticed.disclosure);
    }

    #[test]
    fn a_status_line_never_costs_the_question_its_answers() {
        // Bounded, and last in the block that may not be dropped. A notice
        // allowed to grow -- or placed above the choices -- would push an answer
        // off a short screen in order to explain why the question could not be
        // approved, which is the one trade this screen may never make.
        let mut screen = screen();
        screen.set_status("a ".repeat(400));
        for rows in [6u16, 8, 10, 24] {
            let composed = screen.composition(80, rows);
            assert_eq!(composed.rows.len(), usize::from(rows), "{rows}");
            let painted = composed.rows.join("\n");
            for choice in ["1. Yes", "2. Yes, and", "3. No (esc)"] {
                assert!(
                    painted.contains(choice),
                    "a {rows}-row screen dropped {choice:?} to make room for a notice: {painted:?}"
                );
            }
        }
    }

    #[test]
    fn a_change_scrolled_out_of_the_viewport_is_no_longer_visible() {
        // The half of the clause `super::approval_readiness::Readiness`'s `seen`
        // remembers: what is on the screen **now** is a different fact from what
        // this request has ever shown, and only the composition can answer the
        // first.
        let mut screen = screen();
        assert!(screen.composition(80, 24).disclosure.change_visible);
        screen.scroll_by(1_000_000, 80, 24);
        assert!(
            screen.composition(80, 24).disclosure.change_visible,
            "the walk stops at the end of the change, so rows are still shown"
        );
        // A screen with no room left for the change at all.
        assert!(!screen.composition(80, 6).disclosure.change_visible);
    }

    #[test]
    fn a_side_with_nothing_in_it_says_so_rather_than_showing_a_blank() {
        let screen = ApprovalScreen::new(asked_about("", "beta"));
        let painted = screen.rows(80, 24).join("\n");
        assert!(painted.contains(EMPTY_SIDE), "{painted:?}");
    }
}
