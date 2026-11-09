//! The screen a change too large for the band is reviewed on.
//!
//! Upstream reviews every file change on a screen of its own: the review of the
//! change fills the rows above the question's chrome, its tail against the
//! controls, and the wheel walks it (`vercel-labs/fx@c1db919
//! src/ui/approval_screen.zig:135-206`; capture `fx-approvals-big-40x120-write-big`).
//! xfx asks a change the band can show in the band
//! ([`super::approval::ApprovalSurface`]) and brings the rest here: the
//! terminal's **alternate** buffer, for the length of the question and not one
//! frame longer ([`super::frame::Band::restore_primary`]).
//!
//! What is here is only the surface. Which questions come to it is
//! [`super::approval::ApprovalSurface`]'s and [`super::shell::Shell::ask`]'s;
//! whose screen it is at any instant is [`super::shell::ScreenOwner`]'s; the
//! bytes that take the plane and give it back are [`super::frame`]'s; what each
//! row says is [`super::approval_view`]'s, the same rows the band paints.
//!
//! # What differs from upstream's screen, and why
//!
//! * Upstream's screen is one document -- the transcript, then the review,
//!   then the chrome -- so a short change is shown under the conversation.
//!   xfx's document is the terminal's own scrollback, which this plane cannot
//!   read back, so the rows above a short review are blank.
//! * Upstream walks the review with the wheel; xfx asks for no mouse reports,
//!   and `C-p` and `C-n` walk it ([`super::shell::Shell::decide`]). The hint row
//!   says which.
//! * Under the choices, what `2` would buy, in the permission layer's words
//!   ([`super::approval_view::scope_rows`]); and, while the user is owed one, a
//!   status line, because this plane hides the document a refusal would
//!   otherwise be written into.

use crate::permission::{ApprovalAnswer, ApprovalRequest};

use super::approval::{self, Action, Composition, Reply, CHOICE_COUNT};
use super::approval_amendment::Drafts;
use super::approval_readiness::Disclosure;
use super::approval_view::{
    blank, choice_prefix_cells, choice_rows, dotted_rule, draft_cols, draft_column, header, hint,
    safe_rows, scope_rows, solid_rule, subject_rows, Kind, Review, Styles,
};
use super::frame::clip;
use super::theme::Palette;

/// How many rows the status line may take.
///
/// Bounded, because it is the one block on this screen that is **not** the
/// question: a notice allowed to grow would push the change out of the viewport
/// in order to explain why the change could not be approved.
const STATUS_ROWS: usize = 2;

/// The screen height at which the chrome spends its blank rows
/// (`approval_screen.zig:25`, `file_screen_spaced_controls_min_rows`).
const SPACED_AT: u16 = 15;

/// What one row of the chrome is, in the order the screen gives them up when
/// it is too short for all of them: the first is dropped first.
///
/// **The choices are given up last, and the scope just before them**: a
/// question that cannot be granted on a screen this short must still be
/// refusable, and a choice whose grant is not on the screen is a choice the
/// readiness gate refuses rather than one the user can be tricked into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Blank,
    Status,
    Hint,
    Rule,
    Dotted,
    Header,
    Draft,
    Subject,
    Scope,
    Choice,
}

/// A question about a change the band cannot show, and the screen it is shown
/// on.
///
/// It owns the request rather than borrowing it, because it outlives the event
/// the request arrived on and because the alternate plane is the only thing
/// holding that question while it is up: there is no panel behind it.
pub(crate) struct ApprovalScreen {
    request: ApprovalRequest,
    kind: Kind,
    /// The review of the change, built once: a pure function of the request.
    review: Review,
    /// How many rows the window is above the review's **tail**.
    ///
    /// The tail is where upstream's screen opens (`approval_screen.zig:150-155`,
    /// `visibleWindow`), right against the controls; `C-p` walks back toward
    /// the head and `C-n` forward again ([`Self::scroll_by`]).
    scroll: usize,
    /// Which of the three answers is marked, as an index into the choices.
    selected: usize,
    /// What the screen has to tell the user about their last keystroke.
    ///
    /// **This plane's own status line**, and it exists because this plane owns
    /// the whole terminal while it is up: `super::shell::Shell::say` writes into
    /// the document, the document is on the *primary* buffer, and a refusal
    /// written there while a question holds the alternate buffer is a refusal
    /// nobody can see. Part of the composition rather than a write of its own,
    /// so that a screen carrying a notice is a *different* screen, which is
    /// exactly what a readiness receipt should be about.
    status: Option<String>,
    /// The two amendments this question may carry.
    ///
    /// **Both surfaces get drafts**, from the same module and with the same
    /// keys: which surface a question landed on is a fact about how big the
    /// change was, and a user should not lose the ability to say something
    /// because the diff was long ([`super::approval_amendment`]).
    drafts: Drafts,
}

/// One composed screen and every fact read off it.
///
/// One construction, for the reason `super::approval::Panel`'s is: the height,
/// the caret's row and the paint are three readings of one layout, and three
/// constructions are three chances to disagree.
struct Composed {
    rows: Vec<String>,
    /// The caret's row, one-based, as the terminal counts.
    caret: u16,
    /// The cells to the left of the caret on that row.
    caret_column: u16,
    /// How many rows the review's window has.
    viewport: usize,
    /// How many rows the review has in all.
    total: usize,
    /// Whether all three choices reached the screen with something of each
    /// shown beside its marker -- the question "may xfx ask here at all".
    presents: bool,
    /// What this composition really put in front of the user, decided **here**
    /// rather than by reading the rows back: every cut this screen makes is
    /// silent, so the only reader that can answer honestly is the one that made
    /// them ([`super::approval_readiness`]).
    disclosure: Disclosure,
}

impl ApprovalScreen {
    pub(crate) fn new(request: ApprovalRequest) -> Self {
        let kind = Kind::of(&request);
        let review = Review::of(&request);
        Self {
            request,
            kind,
            review,
            scroll: 0,
            // Yes-once, the same default the band's panel starts on: an Enter
            // pressed without reading grants one call rather than the session.
            selected: 0,
            status: None,
            drafts: Drafts::default(),
        }
    }

    /// Whether an amendment has the keys.
    /// The request this question is about.
    pub(crate) fn request(&self) -> &ApprovalRequest {
        &self.request
    }

    pub(crate) fn drafting(&self) -> bool {
        self.drafts.editing()
    }

    /// What the screen owes the user about their last keystroke, taken once.
    pub(crate) fn take_notice(&mut self) -> Option<&'static str> {
        self.drafts.take_notice()
    }

    /// The amendment this answer carries, and the disposal of the other draft.
    ///
    /// The band panel's rule on this plane too
    /// (`super::approval::Panel::take_feedback`): called after the readiness
    /// gate, so a refused affirmative leaves the drafts as the user typed them.
    pub(crate) fn take_feedback(
        &mut self,
        answer: ApprovalAnswer,
        amendable: bool,
    ) -> Option<String> {
        if !amendable {
            self.drafts.discard();
            return None;
        }
        self.drafts.submit(answer)
    }

    /// Puts a sentence about the last keystroke on this plane's status line.
    ///
    /// The alternate half of `super::shell::Shell::say`: while this screen is up
    /// it owns every row the user can see, so a refusal has to be painted here
    /// or it is not painted at all.
    pub(crate) fn set_status(&mut self, status: String) {
        self.status = Some(status);
    }

    /// Takes the status line down if it is one of `sentences`, and says
    /// whether it did: a sentence about a state that has ended
    /// (`super::shell::Shell`'s paste quarantine). Any other status stays --
    /// a refusal, or fx's own warning about input after a paste, is about
    /// something that happened, and is as true now as when it was said.
    pub(crate) fn clear_status_if(&mut self, sentences: &[&str]) -> bool {
        let ended = self
            .status
            .as_deref()
            .is_some_and(|status| sentences.contains(&status));
        if ended {
            self.status = None;
        }
        ended
    }

    /// What one keystroke does. Routed through the same function the band's
    /// panel answers with (`super::approval::answered`), so "which key means
    /// which answer" is one fact on both surfaces rather than two that can
    /// drift.
    pub(crate) fn apply(&mut self, action: Action, cols: u16) -> Reply {
        let kind = self.kind;
        approval::answered(action, &mut self.selected, &mut self.drafts, &|index| {
            draft_cols(kind, index, cols)
        })
    }

    /// Moves the window `delta` rows: negative toward the head of the change,
    /// positive back toward its tail, bounded by what there is to show.
    ///
    /// A window that could be walked past either end would read as a change
    /// that had stopped when it had not.
    pub(crate) fn scroll_by(&mut self, delta: isize, cols: u16, terminal_rows: u16) {
        let composed = self.compose(cols, terminal_rows, &Styles::PLAIN);
        let furthest = composed.total.saturating_sub(composed.viewport);
        self.scroll = if delta < 0 {
            self.scroll.saturating_add(delta.unsigned_abs())
        } else {
            self.scroll.saturating_sub(delta.unsigned_abs())
        }
        .min(furthest);
    }

    /// The whole screen as text, top first: exactly `terminal_rows` rows.
    ///
    /// A **full** screen rather than a band's worth, because that is what the
    /// alternate buffer is: every row of it is this module's, and a row left
    /// unwritten is whatever the terminal's other buffer happened to hold.
    /// Production paints [`Self::painted`], the same construction in colour.
    #[cfg(test)]
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16) -> Vec<String> {
        self.compose(cols, terminal_rows, &Styles::PLAIN).rows
    }

    /// The same screen in the palette's colours, for the painter.
    pub(crate) fn painted(&self, cols: u16, terminal_rows: u16, palette: &Palette) -> Vec<String> {
        self.compose(cols, terminal_rows, &Styles::of(palette)).rows
    }

    /// The same screen as text, with what it really disclosed beside it.
    pub(crate) fn composition(&self, cols: u16, terminal_rows: u16) -> Composition {
        let composed = self.compose(cols, terminal_rows, &Styles::PLAIN);
        Composition {
            rows: composed.rows,
            disclosure: composed.disclosure,
        }
    }

    /// Where the caret goes: the terminal's own row, and the cells to the left
    /// of it on that row.
    pub(crate) fn caret(&self, cols: u16, terminal_rows: u16) -> (u16, u16) {
        let composed = self.compose(cols, terminal_rows, &Styles::PLAIN);
        (composed.caret, composed.caret_column)
    }

    /// Whether this screen can put **all three** answers in front of the user
    /// at this size.
    ///
    /// Asked of the composition itself rather than of a remembered minimum:
    /// [`Self::compose`] is what drops rows when the screen runs out of them,
    /// so the only way to know what a screen loses is to compose it and look.
    /// Two ways to lose an answer, and both are checked -- a row dropped off
    /// the screen, and a row clipped so far that nothing of its words survives
    /// beside its marker.
    ///
    /// A `false` is a screen on which xfx cannot ask, and
    /// [`super::shell::Shell::ask`] refuses on the user's behalf rather than
    /// leaving a session waiting for a keystroke about a question with no
    /// visible answers.
    pub(crate) fn presents_choices(&self, cols: u16, terminal_rows: u16) -> bool {
        self.compose(cols, terminal_rows, &Styles::PLAIN).presents
    }

    /// One screen: as much of the review as fits, against the chrome.
    fn compose(&self, cols: u16, terminal_rows: u16, styles: &Styles) -> Composed {
        let height = usize::from(terminal_rows);
        let spaced = terminal_rows >= SPACED_AT;
        let mut parts: Vec<(Part, String)> = Vec::new();
        if self.kind.reviews_a_file() {
            parts.push((Part::Dotted, dotted_rule(cols, styles)));
        }
        let review_of = self.kind.reviews_a_file().then_some(&self.review);
        parts.push((Part::Header, header(self.kind, review_of, cols, styles)));
        if spaced {
            parts.push((Part::Blank, blank()));
        }
        let subject = subject_rows(self.kind, &self.request, cols, styles);
        let subject_wanted = subject.len();
        parts.extend(subject.into_iter().map(|row| (Part::Subject, row)));
        if spaced {
            parts.push((Part::Blank, blank()));
        }
        let blocks = self
            .drafts
            .blocks(|index| draft_cols(self.kind, index, cols));
        // Whether every choice's words reached its row uncut, measured on the
        // construction rather than read back off a row.
        let mut words_whole = true;
        for index in 0..CHOICE_COUNT {
            let block = blocks.iter().find(|block| block.choice == index);
            let later = block.map_or(0, |block| block.rows.len().saturating_sub(1));
            let (rows, whole) = choice_rows(
                self.kind,
                index,
                index == self.selected,
                block,
                later,
                cols,
                styles,
            );
            words_whole &= whole;
            let mut rows = rows.into_iter();
            if let Some(first) = rows.next() {
                parts.push((Part::Choice, first));
            }
            parts.extend(rows.map(|row| (Part::Draft, row)));
        }
        let scope = scope_rows(&self.request, cols, styles);
        let scope_wanted = scope.len();
        parts.extend(scope.into_iter().map(|row| (Part::Scope, row)));
        if let Some(status) = self.status.as_ref() {
            parts.push((Part::Status, blank()));
            let budget = cols.saturating_sub(2).max(1);
            parts.extend(
                safe_rows(status, budget)
                    .into_iter()
                    .take(STATUS_ROWS)
                    .map(|row| (Part::Status, clip(&format!("  {row}"), cols).to_string())),
            );
        }
        if spaced {
            parts.push((Part::Blank, blank()));
        }
        parts.push((Part::Rule, solid_rule(cols, styles)));
        // The hint says `ctrl+p/n scroll` while the review has more rows than
        // its window -- the room the chrome leaves it, hint included.
        let review = self.review.rows(cols, styles);
        let room = height.saturating_sub(parts.len() + 1);
        parts.push((
            Part::Hint,
            hint(
                self.kind,
                approval::amendable(self.selected),
                review.len() > room,
                cols,
                styles,
            ),
        ));

        // Too tall: give rows up, the least needed first and the last of each
        // kind first -- a draft's last row before its first, the third choice
        // before the first.
        while parts.len() > height {
            let Some(lowest) = parts.iter().map(|(part, _)| *part).min() else {
                break;
            };
            let Some(at) = parts.iter().rposition(|(part, _)| *part == lowest) else {
                break;
            };
            parts.remove(at);
        }
        let count = |wanted: Part| parts.iter().filter(|(part, _)| *part == wanted).count();
        let choices: Vec<usize> = parts
            .iter()
            .enumerate()
            .filter(|(_, (part, _))| *part == Part::Choice)
            .map(|(at, _)| at)
            .collect();
        let prefix = choice_prefix_cells(self.kind);
        let presents = choices.len() == CHOICE_COUNT
            && choices
                .iter()
                .all(|&at| super::wrap::width(&parts[at].1) > prefix);

        // The review, its tail against the chrome, in what the chrome left.
        let viewport = height - parts.len();
        let shown = viewport.min(review.len());
        let furthest = review.len() - shown;
        let start = furthest - self.scroll.min(furthest);
        let window = &review[start..start + shown];
        let change_visible = window.iter().any(|(_, changed)| *changed);
        let top = viewport;

        // Inside the draft while one has the keys, on the marked answer
        // otherwise; both are indices into the parts that survived, so the
        // caret and the paint are one reading of one layout.
        let marked = choices.get(self.selected).copied();
        let mut caret = (marked.map_or(height.saturating_sub(1), |at| top + at), 0u16);
        let open = self.drafts.active();
        let draft = open.and_then(|open| {
            let block = blocks.iter().find(|block| block.choice == open)?;
            Some((open, block.caret?))
        });
        if let Some((open, (within, column))) = draft {
            if let Some(&at) = choices.get(open) {
                let later = parts[at + 1..]
                    .iter()
                    .take_while(|(part, _)| *part == Part::Draft)
                    .count();
                caret = (
                    top + at + within.min(later),
                    draft_column(self.kind, open)
                        .saturating_add(column)
                        .min(cols.saturating_sub(1)),
                );
            }
        }
        let disclosure = Disclosure {
            // One-based: the plane owns every row of the screen, so a row of
            // the composition **is** a terminal row.
            last_control_row: choices
                .last()
                .and_then(|&at| u16::try_from(top + at + 1).ok())
                .unwrap_or(0),
            controls_whole: choices.len() == CHOICE_COUNT && words_whole,
            scope_whole: count(Part::Scope) == scope_wanted,
            subject_whole: count(Part::Subject) == subject_wanted,
            change_visible,
        };

        let mut rows: Vec<String> = Vec::with_capacity(height);
        rows.resize(viewport - shown, String::new());
        rows.extend(window.iter().map(|(row, _)| row.clone()));
        rows.extend(parts.into_iter().map(|(_, row)| row));
        Composed {
            rows,
            caret: u16::try_from(caret.0 + 1)
                .unwrap_or(terminal_rows)
                .clamp(1, terminal_rows.max(1)),
            caret_column: caret.1,
            viewport,
            total: review.len(),
            presents,
            disclosure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::permission::{ApprovalDiff, ApprovalSubject};

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
                first_line: 1,
                lines_below: 0,
            }),
            subject: ApprovalSubject::Path("notes.txt".to_string()),
        }
    }

    /// Eighty lines replaced: a review of a hundred and sixty rows.
    fn screen() -> ApprovalScreen {
        let before: String = (0..80).map(|line| format!("alpha {line}\n")).collect();
        let after: String = (0..80).map(|line| format!("beta {line}\n")).collect();
        ApprovalScreen::new(asked_about(&before, &after))
    }

    /// Whether all three choices are on `rows`, whole.
    fn all_three(rows: &[String]) -> bool {
        let painted = rows.join("\n");
        ["1  Apply once", "2  Apply + allow", "3  Don't apply"]
            .iter()
            .all(|choice| painted.contains(choice))
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
    fn the_review_is_upstreams_tail_against_the_chrome() {
        // `fx-approvals-big-40x120-write-big`: the review's last rows right
        // above the dotted rule, then the header, the path and its question,
        // the choices, the rule and a hint that says the review scrolls.
        let after: String = (1..=80)
            .map(|line| format!("generated line {line}\n"))
            .collect();
        let mut asked = asked_about("", &after);
        asked.tool = "write_file";
        asked.subject = ApprovalSubject::Path("big.txt".to_string());
        asked.always_scope = "allow every future write_file to `big.txt`".to_string();
        let rows = ApprovalScreen::new(asked).rows(120, 40);
        let dotted = rows
            .iter()
            .position(|row| row.starts_with('┄'))
            .expect("the dotted rule");
        assert_eq!(rows[dotted - 1], "     80 + generated line 80");
        assert_eq!(
            rows[0],
            format!(
                "     {} + generated line {}",
                80 - dotted + 1,
                80 - dotted + 1
            )
        );
        assert_eq!(
            rows[dotted + 1],
            format!(
                "  Permission needed · Review change{}Write · +80  -0",
                " ".repeat(120 - 4 - 33 - 15)
            )
        );
        assert_eq!(rows[dotted + 3], "  big.txt  ·  Apply this change?");
        assert_eq!(rows[dotted + 5], "  ❯ 1  Apply once");
        assert_eq!(rows[38], "─".repeat(120));
        assert_eq!(
            rows[39],
            "  1–3 choose now    ↑↓ options    tab amend    ctrl+p/n scroll    enter confirm    esc cancel"
        );
    }

    #[test]
    fn a_question_never_loses_its_answers_however_short_the_screen_is() {
        // The one thing a screen too short may not drop. A change is a
        // disclosure and the choices are the question; a screen with the
        // change on it and the answers below its last row leaves a session
        // waiting for a keystroke about something nobody can act on.
        for rows in [6u16, 8, 10, 24, 40] {
            let painted = screen().rows(80, rows);
            assert!(
                all_three(&painted),
                "a {rows}-row screen dropped an answer: {painted:?}"
            );
        }
    }

    #[test]
    fn a_screen_that_cannot_show_all_three_answers_says_so() {
        // The other half of the case above, and the one the shell acts on: a
        // screen the question cannot be *asked* on is refused rather than
        // painted with an answer missing. Two ways to lose one -- a row that
        // never survived the cut, and a row clipped down to its own marker.
        // (Coordinate changed: the choices block no longer opens with a blank
        // row, so three rows hold all three answers.)
        let screen = screen();
        for rows in [3u16, 4, 5, 6, 8, 10, 24, 40] {
            assert!(
                screen.presents_choices(80, rows),
                "a {rows}-row screen was refused a question it can show"
            );
        }
        for rows in [1u16, 2] {
            assert!(
                !screen.presents_choices(80, rows),
                "a {rows}-row screen claimed to show three answers it has no rows for"
            );
        }
        assert!(
            !screen.presents_choices(6, 24),
            "a screen six cells wide claimed to show answers clipped to their markers"
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
        assert_eq!(marked(&screen), "  ❯ 1  Apply once");
        assert_eq!(answered(screen.apply(Action::Down, 80)), None);
        assert!(marked(&screen).starts_with("  ❯ 2  Apply + allow"));
        assert_eq!(answered(screen.apply(Action::Up, 80)), None);
        assert_eq!(marked(&screen), "  ❯ 1  Apply once");
    }

    /// The answer a key produced, for a case that is about the decision rather
    /// than about the frame it owes (`super::super::approval::Reply`).
    fn answered(reply: Reply) -> Option<ApprovalAnswer> {
        match reply {
            Reply::Answer { answer, .. } => Some(answer),
            _ => None,
        }
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
            assert_eq!(
                answered(screen().apply(action, 80)),
                Some(answer),
                "{action:?}"
            );
        }
        assert_eq!(answered(screen().apply(Action::Text('9'), 80)), None);
    }

    #[test]
    fn the_change_is_reviewed_as_numbered_lines_and_the_screen_names_its_subject() {
        // (Claim changed: the two halves were `before` and `after` sections;
        // they are upstream's numbered `-` and `+` rows now.)
        let painted = screen().rows(80, 40).join("\n");
        assert!(painted.contains("80 + beta 79"), "{painted:?}");
        assert!(painted.contains("Permission needed"), "{painted:?}");
        assert!(
            painted.contains("notes.txt  ·  Apply this change?"),
            "the screen does not say what the change is to: {painted:?}"
        );
        let mut walked = screen();
        walked.scroll_by(-1_000_000, 80, 40);
        let head = walked.rows(80, 40).join("\n");
        assert!(head.contains("1 - alpha 0"), "{head:?}");
    }

    #[test]
    fn a_raw_newline_in_a_change_is_a_row_break_and_never_a_cursor_move() {
        // The row-safety rule this module exists to keep. A `\n` written into
        // the middle of a screen whose rows are placed by number moves the
        // terminal's cursor, and every row after it lands one row low -- with
        // the choices falling off the bottom.
        let screen = ApprovalScreen::new(asked_about("first\nsecond", "third"));
        let painted = screen.rows(80, 24);
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
            all_three(&painted),
            "the line break pushed the choices off: {painted:?}"
        );
    }

    #[test]
    fn a_change_built_at_the_permission_boundary_keeps_its_lines_and_its_literals_apart() {
        // **End to end through the boundary, not by hand.** A payload whose
        // line breaks are real and one that merely contains a backslash and an
        // `n` are two different files, and a screen that showed them the same
        // way would let a model replace a hundred-line file with one line of
        // literal escapes and call it a no-op. What the screen owes is two
        // different **shapes**: rows where the breaks are real, and a visible
        // escape where they are spelled out.
        let real = "A\n".repeat(3);
        let literal = "A\\n".repeat(3);
        let screen = ApprovalScreen::new(ApprovalRequest {
            diff: Some(crate::permission::ApprovalDiff::of(&real, &literal)),
            ..asked_about("", "")
        });
        let painted = screen.rows(80, 40);
        let removed: Vec<&String> = painted.iter().filter(|row| row.contains(" - A")).collect();
        assert_eq!(
            removed
                .iter()
                .filter(|row| row.trim_end().ends_with("- A"))
                .count(),
            3,
            "the file's three lines are not three rows: {removed:?}"
        );
        let added: Vec<&String> = painted.iter().filter(|row| row.contains(" + A")).collect();
        assert_eq!(
            added.len(),
            1,
            "the spelled-out breaks were rendered as rows: {added:?}"
        );
        assert!(
            added[0].contains("A\\\\nA\\\\nA\\\\n"),
            "the literal backslash-n was not shown as a literal: {added:?}"
        );
    }

    #[test]
    fn nothing_a_terminal_would_obey_or_reorder_reaches_a_row() {
        // Both classes, because they are different failures. A control is a
        // sequence the terminal *executes*; a bidi override is a display
        // instruction it *obeys* -- and neither is `char::is_control` alone.
        let hostile = "\u{1b}[2J\u{9b}31m\u{9d}0;pwned\u{7}\u{0}\u{202e}drawkcab\u{2069}";
        let mut asked = asked_about(hostile, hostile);
        asked.subject = ApprovalSubject::Path(hostile.to_string());
        let screen = ApprovalScreen::new(asked);
        for row in screen.rows(80, 40) {
            for character in row.chars() {
                assert!(
                    !character.is_control(),
                    "a control character reached a row: {row:?}"
                );
                assert!(
                    !matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
                    "a reordering character reached a row: {row:?}"
                );
            }
        }
    }

    #[test]
    fn two_characters_a_row_may_not_carry_are_not_shown_as_one() {
        // A character that may not reach a row is **named** rather than
        // blanked, so two of them that a payload could swap are two different
        // screens.
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
        let escaped = "alpha\\nbeta\\tgamma";
        let screen = ApprovalScreen::new(asked_about(escaped, "other"));
        let painted = screen.rows(80, 40).join("\n");
        assert!(painted.contains("alpha\\nbeta\\tgamma"), "{painted:?}");

        // And a **raw** tab is given the same name the permission boundary
        // gives it, rather than being expanded or replaced.
        let raw = ApprovalScreen::new(asked_about("alpha\tbeta", "gamma"));
        let painted = raw.rows(80, 40).join("\n");
        assert!(
            painted.contains("alpha\\tbeta"),
            "a raw tab was not given its name: {painted:?}"
        );
    }

    #[test]
    fn the_window_opens_at_the_tail_walks_to_the_head_and_stops_at_either_end() {
        // A window onto up to 128 KiB, and a window that could be walked past
        // either end would read as a change that had stopped when it had not.
        // (Claim changed: the window opened at the head and walked down; it
        // opens at the tail now, against the controls, as upstream's does.)
        let mut screen = screen();
        let tail = screen.rows(80, 24).join("\n");
        assert!(tail.contains("beta 79"), "{tail:?}");
        screen.scroll_by(5, 80, 24);
        assert_eq!(
            screen.rows(80, 24).join("\n"),
            tail,
            "the window was walked past the tail of the change"
        );
        screen.scroll_by(-5, 80, 24);
        assert_ne!(
            screen.rows(80, 24).join("\n"),
            tail,
            "the window did not move"
        );

        screen.scroll_by(-1_000_000, 80, 24);
        let head = screen.rows(80, 24);
        assert!(
            head.join("\n").contains("1 - alpha 0"),
            "the walk did not reach the head: {head:?}"
        );
        assert!(all_three(&head), "the walk lost the choices: {head:?}");
        let same = head.join("\n");
        screen.scroll_by(-1, 80, 24);
        assert_eq!(
            screen.rows(80, 24).join("\n"),
            same,
            "the window was walked past the head of the change"
        );
        screen.scroll_by(1_000_000, 80, 24);
        assert_eq!(screen.rows(80, 24).join("\n"), tail, "it did not come back");
    }

    #[test]
    fn a_long_target_is_shown_whole_where_there_is_room_and_refused_where_there_is_not() {
        // The subject takes the rows it needs, so an ordinary screen shows a
        // long target whole -- and a screen that really cannot carry it says
        // so rather than pretending, while every answer stays on the screen,
        // because a question that cannot be granted must still be refusable.
        let mut request = asked_about("one", "two");
        request.subject = ApprovalSubject::Path("a/".repeat(120) + "notes.txt");
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
        assert!(
            roomy.rows.iter().any(|row| row.contains("notes.txt")),
            "the end of the path is not on the screen it was reported disclosed on: {:?}",
            roomy.rows
        );

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
        // Four rows: the three answers and one row of what they buy, and
        // nothing of the path at all.
        let composed = ApprovalScreen::new(asked_about("one", "two")).composition(80, 4);
        assert_eq!(
            composed.rows.len(),
            4,
            "the composition still fills the plane"
        );
        assert!(
            !composed.disclosure.subject_whole,
            "the heading was cut into the choices' room"
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
                .rposition(|row| row.contains("3  Don't apply"))
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
        // refusal written into the document is a refusal on the *primary*
        // buffer, behind the plane, that nobody can read.
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
        assert!(noticed.disclosure.controls_whole && noticed.disclosure.scope_whole);
        assert_eq!(
            noticed.rows.len(),
            24,
            "the notice changed the height of a screen that is the terminal"
        );
        // And a screen carrying a notice is a *different* screen, which is
        // exactly what a readiness receipt must be about.
        assert_ne!(quiet.disclosure, noticed.disclosure);
    }

    #[test]
    fn a_status_line_never_costs_the_question_its_answers() {
        // Bounded, and the first thing given up. A notice allowed to grow would
        // push an answer off a short screen in order to explain why the
        // question could not be approved.
        let mut screen = screen();
        screen.set_status("a ".repeat(400));
        for rows in [6u16, 8, 10, 24] {
            let composed = screen.composition(80, rows);
            assert_eq!(composed.rows.len(), usize::from(rows), "{rows}");
            assert!(
                all_three(&composed.rows),
                "a {rows}-row screen dropped an answer to make room for a notice: {:?}",
                composed.rows
            );
        }
    }

    #[test]
    fn a_status_is_on_the_screen_whole_or_not_at_all_at_every_height() {
        // opus on PASTE-GAP round 5 (SHOULD 3): a screen too short for the
        // whole status gave up its rows from the last, so at one height the
        // plane painted a sentence's first row alone -- and a cut sentence
        // says something else. Given up whole: at every height, all of its
        // rows or none.
        let cols = super::super::layout::MIN_COLS;
        for status in [
            "Keys ignored until input stops.",
            "Input that arrived right after the paste was ignored.",
        ] {
            let mut screen = screen();
            screen.set_status(status.to_string());
            let rows: Vec<String> = safe_rows(status, cols - 2)
                .into_iter()
                .take(STATUS_ROWS)
                .map(|row| format!("  {row}"))
                .collect();
            let mut seen = false;
            for height in super::super::layout::MIN_ROWS..=40 {
                let composed = screen.composition(cols, height);
                let shown = rows
                    .iter()
                    .filter(|row| composed.rows.contains(row))
                    .count();
                assert!(
                    shown == 0 || shown == rows.len(),
                    "{height} rows show {shown} of {status:?}'s {}: {:?}",
                    rows.len(),
                    composed.rows
                );
                seen |= shown > 0;
            }
            assert!(seen, "{status:?} was never on the screen");
        }
    }

    #[test]
    fn a_change_scrolled_out_of_the_viewport_is_no_longer_visible() {
        // The half of the clause `super::approval_readiness::Readiness`'s `seen`
        // remembers: what is on the screen **now** is a different fact from
        // what this request has ever shown.
        let mut screen = screen();
        assert!(screen.composition(80, 24).disclosure.change_visible);
        screen.scroll_by(-1_000_000, 80, 24);
        assert!(
            screen.composition(80, 24).disclosure.change_visible,
            "the walk stops at the head of the change, so changed rows are still shown"
        );
        // A screen with no room left for the change at all.
        assert!(!screen.composition(80, 6).disclosure.change_visible);
    }

    #[test]
    fn a_file_that_does_not_exist_yet_is_reviewed_as_all_additions() {
        // (Claim changed: an empty side used to say `(nothing)`; upstream's
        // review of a new file is every line added and `-0` in the header.)
        let screen = ApprovalScreen::new(asked_about("", "beta\ngamma\n"));
        let painted = screen.rows(80, 24).join("\n");
        assert!(
            painted.contains("1 + beta") && painted.contains("2 + gamma"),
            "{painted:?}"
        );
        assert!(painted.contains("+2  -0"), "{painted:?}");
    }

    #[test]
    fn a_draft_on_this_plane_sits_on_its_choices_row_with_the_caret_after_it() {
        let mut screen = screen();
        screen.apply(Action::Tab, 80);
        for character in "use capitals".chars() {
            screen.apply(Action::Text(character), 80);
        }
        let rows = screen.rows(80, 24);
        let (row, column) = screen.caret(80, 24);
        // The caret's cell is painted, after the draft (upstream's reverse
        // video, `approval_ui.zig:1662-1672`; claim changed: it was the
        // terminal's caret alone).
        assert_eq!(
            rows[usize::from(row) - 1],
            "  ❯ 1  Apply once, use capitals "
        );
        assert_eq!(column, 19 + 12);
    }
}
