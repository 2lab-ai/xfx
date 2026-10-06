//! What a permission question looks like, row by row.
//!
//! Upstream's question (`vercel-labs/fx@c1db919`) is one layout on two
//! surfaces: the footer for a command (`src/ui/footer/approval_ui.zig`) and a
//! review of the change above the same chrome for a file
//! (`src/ui/approval_screen.zig`). xfx keeps its own two surfaces -- the band's
//! panel ([`super::approval::Panel`]) and the alternate plane
//! ([`super::approval_screen::ApprovalScreen`]) -- and this module is what makes
//! them say the same thing in the same rows: the review of a change, the
//! header, the question, the three choices, the hint.
//!
//! **Everything here is a pure function of the request, the width and the
//! styles.** The surfaces decide which rows go where and what they disclosed;
//! nothing in this module measures a screen's height, so a row cannot be
//! composed one way for the paint and another way for the disclosure.
//!
//! # Every string is made row-safe here
//!
//! `super::bridge::inert` turns the controls in an event into spaces, and the
//! permission boundary escaped the diff (`crate::permission::bounded_diff_side`)
//! keeping its line breaks. Neither is trusted: a raw `\n` moves the cursor in
//! the middle of a layout placed by row number, and a bidirectional override
//! reorders the glyphs of a change a reviewer is reading. So every quoted
//! string goes through [`safe_rows`] (or [`tamed`] for one line) before it is
//! wrapped.

use crate::permission::{ApprovalDiff, ApprovalRequest, ApprovalSubject};

use super::approval_amendment::Block;
use super::frame::clip;
use super::theme::Palette;

/// The title of a question about a file's contents (`approval_ui.zig:835-837`).
const FILE_TITLE: &str = "Permission needed · Review change";

/// The title of every other question (`approval_ui.zig:1514`).
const CHOOSE_TITLE: &str = "Permission needed · Choose one";

/// What a file question asks, after the path (`approval_ui.zig:645-651`).
const FILE_QUESTION: &str = "Apply this change?";

/// The separator between the path and that question.
const PATH_SEPARATOR: &str = "  ·  ";

/// What a command question asks (`approval_ui.zig:1865`).
const COMMAND_QUESTION: &str = "Would you like to run the following command?";

/// What any other question asks (`approval_ui.zig:1854-1871`'s fallback).
const OTHER_QUESTION: &str = "Would you like to allow this action?";

/// How many unchanged lines the review shows on either side of the change
/// (`diff.zig:497`, `review_context_lines`).
pub(crate) const CONTEXT_LINES: usize = 5;

/// The narrowest a review's line-number column is (`approval_ui.zig:390`).
const NUMBER_WIDTH: usize = 5;

/// The cells every indented row starts with (`approval_ui.zig:387`).
const INSET: usize = 2;

/// The cells a header keeps clear at the right edge (`approval_ui.zig:388`).
const RIGHT_MARGIN: usize = 2;

/// What a review shows for a change with no before and after at all.
///
/// Unreachable through the tools, which give every content change a pair; a
/// fixture or a future asker could build one, and a review with a hole in it
/// is a worse failure than one that says what it does not have.
const NO_DIFF: &str = "(this change has no before and after to show)";

/// What a review shows for a change whose text would not change.
const NO_CHANGE: &str = "(the text of this file would not change)";

/// The hint rows, widest first, each keeping the Enter and Escape controls so
/// a narrow terminal never loses them (`approval_ui.zig:20-45`).
///
/// `ctrl+p/n scroll` stands where upstream says `wheel scroll`: xfx asks for
/// no mouse reports, and the keys that walk a review on the plane are `C-p`
/// and `C-n` (`super::shell::Shell::decide`). A hint advertising a wheel would
/// advertise a gesture that does nothing.
const AMEND_HINTS: [&str; 3] = [
    "1–3 choose now    ↑↓ options    tab amend    enter confirm    esc cancel",
    "1–3 choose now    enter confirm    esc cancel",
    "enter confirm    esc cancel",
];
const NAVIGATE_HINTS: [&str; 3] = [
    "1–3 choose now    ↑↓ or tab options    enter confirm    esc cancel",
    "1–3 choose now    enter confirm    esc cancel",
    "enter confirm    esc cancel",
];
const AMEND_SCROLL_HINTS: [&str; 5] = [
    "1–3 choose now    ↑↓ options    tab amend    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose now    ↑↓ options    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose now    enter confirm    esc cancel",
    "enter confirm    esc cancel",
];
const NAVIGATE_SCROLL_HINTS: [&str; 5] = [
    "1–3 choose now    ↑↓ or tab options    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose now    ↑↓ options    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose    ctrl+p/n scroll    enter confirm    esc cancel",
    "1–3 choose now    enter confirm    esc cancel",
    "enter confirm    esc cancel",
];

// ---------------------------------------------------------------------------
// styles
// ---------------------------------------------------------------------------

/// The sequences a question's rows are painted in.
///
/// A value rather than a [`Palette`] so that the same construction can be
/// asked for **plain** rows ([`Self::PLAIN`]): the height a band solves for,
/// the disclosure a receipt is about and the rows a test reads are all the
/// painted rows with nothing in front of each run, and an escape costs no cells
/// ([`super::wrap::width`]), so the two cannot measure differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Styles {
    /// The title and the path (`bold_style`).
    pub bold: &'static str,
    /// The marked choice and the command (`tag_style`).
    pub strong: &'static str,
    /// Line numbers, unchanged lines, the kind on the right of the header, an
    /// unmarked digit and the hint (`statusline_style`).
    pub quiet: &'static str,
    /// The always-scope and a command question's hint (`dim_style`).
    pub dim: &'static str,
    /// A changed line and the `+N  -N` counts (`diff_added_style`).
    pub diff: &'static str,
    /// The dotted rule above the header (`divider_style`).
    pub rule: &'static str,
    /// What ends a run.
    pub reset: &'static str,
}

impl Styles {
    /// No sequences at all: the rows as text.
    pub(crate) const PLAIN: Self = Self {
        bold: "",
        strong: "",
        quiet: "",
        dim: "",
        diff: "",
        rule: "",
        reset: "",
    };

    /// The palette's sequences for each role.
    pub(crate) fn of(palette: &Palette) -> Self {
        Self {
            bold: palette.bold(),
            strong: palette.strong(),
            quiet: palette.statusline(),
            dim: palette.dim(),
            diff: palette.diff(),
            rule: palette.divider(),
            reset: palette.reset(),
        }
    }

    /// `text` in `style`, ended; nothing around an empty run.
    fn run(&self, style: &str, text: &str) -> String {
        if style.is_empty() || text.is_empty() {
            return text.to_string();
        }
        format!("{style}{text}{}", self.reset)
    }

    /// A row cut to `cols` by the painter's own rule, with the run that may
    /// have been open at the cut ended **after** it.
    ///
    /// The cut drops everything past the last cell that fits, the closing
    /// reset included; a row that lost it would leave its colour open over the
    /// next row of somebody else's document. An escape costs no cells, so a
    /// reset appended after the cut is always inside the budget.
    fn finish(&self, row: &str, cols: u16) -> String {
        let cut = clip(row, cols);
        if cut.len() == row.len() || self.reset.is_empty() || !cut.contains('\u{1b}') {
            return cut.to_string();
        }
        format!("{cut}{}", self.reset)
    }
}

// ---------------------------------------------------------------------------
// what kind of question it is
// ---------------------------------------------------------------------------

/// Which of upstream's question layouts a request takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `edit_file`: a review headed `Edit · +N  -N`.
    Edit,
    /// `write_file`: a review headed `Write · +N  -N`.
    Write,
    /// `terminal`: `Would you like to run the following command?`.
    Command,
    /// Anything else -- today `create_folder` -- in upstream's generic words.
    Other,
}

impl Kind {
    pub(crate) fn of(request: &ApprovalRequest) -> Self {
        match request.tool {
            "edit_file" => Self::Edit,
            "write_file" => Self::Write,
            "terminal" => Self::Command,
            _ => Self::Other,
        }
    }

    /// Whether this question is a review of a file's contents.
    pub(crate) fn reviews_a_file(self) -> bool {
        matches!(self, Self::Edit | Self::Write)
    }

    /// What the right of the header calls it (`approval_ui.zig:626-633`,
    /// `:1842-1852`).
    fn action(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Write => "Write",
            Self::Command => "Command",
            Self::Other => "Tool",
        }
    }
}

/// What choice `index` is called, before any amendment.
///
/// A file's wording is upstream's (`approval_ui.zig:1195-1222`) with the one
/// change honesty asks for: upstream's second choice grants **workspace file
/// access**, and xfx's `Always` grants this tool on this file
/// (`crate::permission::Grant`), so the label says what xfx's grant is. The
/// session-durable half of it is the scope row's ([`scope_rows`]). A command's
/// and every other question's are upstream's exactly (`:1966-1972`).
pub(crate) fn label(kind: Kind, index: usize) -> &'static str {
    match (kind, index) {
        (Kind::Edit | Kind::Write, 0) => "Apply once",
        (Kind::Edit, 1) => "Apply + allow future edits to this file for this session",
        (Kind::Write, 1) => "Apply + allow future writes to this file for this session",
        (Kind::Edit | Kind::Write, _) => "Don't apply",
        (_, 0) => "1. Yes",
        (Kind::Command, 1) => "2. Yes, and don't ask again for this exact command",
        (_, 1) => "2. Yes, and don't ask again for this request",
        (_, _) => "3. No",
    }
}

/// What a choice is called while it carries an amendment: the words the draft
/// follows (`approval_ui.zig:1227-1260`, `:1727-1733`).
fn amended_head(kind: Kind, index: usize) -> &'static str {
    match (kind.reviews_a_file(), index) {
        (true, 0) => "Apply once, ",
        (true, _) => "Don't apply, ",
        (false, 0) => "1. Yes, ",
        (false, _) => "3. No, ",
    }
}

/// What an open draft with nothing in it says (`approval_ui.zig:1719-1725`),
/// with xfx's own name in it.
fn placeholder(index: usize) -> &'static str {
    if index == 0 {
        "and tell xfx what to do next"
    } else {
        "and tell xfx what to do differently"
    }
}

/// The cells in front of a choice's words: the inset, the marker, and for a
/// file the digit and its gap (`approval_ui.zig:389`).
pub(crate) fn choice_prefix_cells(kind: Kind) -> u16 {
    if kind.reviews_a_file() {
        7
    } else {
        4
    }
}

/// The cells in front of choice `index`'s draft text: everything its row
/// paints before the first character the user typed.
pub(crate) fn draft_column(kind: Kind, index: usize) -> u16 {
    choice_prefix_cells(kind).saturating_add(super::wrap::width(amended_head(kind, index)))
}

/// How wide choice `index`'s draft is on a screen `cols` wide.
///
/// **One function for the three consumers of a draft** -- the editor that
/// applies a key at a width, the rows painted, and the caret placed -- for the
/// reason `super::approval`'s module says: a caret computed at one width and a
/// wrap computed at another is a caret standing on the wrong cell.
pub(crate) fn draft_cols(kind: Kind, index: usize, cols: u16) -> u16 {
    cols.saturating_sub(draft_column(kind, index)).max(1)
}

// ---------------------------------------------------------------------------
// the review of a change
// ---------------------------------------------------------------------------

/// One line of a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Line {
    /// A line the change leaves alone, by its number after the change.
    Context(usize, String),
    /// A line the change adds, by its number after the change.
    Added(usize, String),
    /// A line the change removes, by its number before the change.
    Removed(usize, String),
    /// A run of unchanged lines left out of the review (`diff.zig:620-632`).
    Elided(usize),
    /// A sentence standing in for a review there is nothing of.
    Notice(&'static str),
}

impl Line {
    /// Whether this line is the change itself rather than its surroundings --
    /// what a review has to have shown before it can be granted
    /// (`approval_ui.zig:605-610`, `fileApprovalPreviewLineQualifies`).
    pub(crate) fn changed(&self) -> bool {
        matches!(self, Self::Added(..) | Self::Removed(..) | Self::Notice(_))
    }

    fn number(&self) -> Option<usize> {
        match self {
            Self::Context(number, _) | Self::Added(number, _) | Self::Removed(number, _) => {
                Some(*number)
            }
            Self::Elided(_) | Self::Notice(_) => None,
        }
    }
}

/// A change as upstream reviews it: the changed lines numbered, the unchanged
/// lines around them, and the counts the header sums them up with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Review {
    pub lines: Vec<Line>,
    pub additions: usize,
    pub deletions: usize,
}

/// The lines of one side, the way a file has them: a trailing break ends the
/// last line rather than starting an empty one, and an empty side has none.
fn lines_of(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n').collect()
}

impl Review {
    /// The review of a request's change, or a notice when it has none.
    pub(crate) fn of(request: &ApprovalRequest) -> Self {
        match request.diff.as_ref() {
            Some(diff) => Self::of_diff(diff),
            None => Self {
                lines: vec![Line::Notice(NO_DIFF)],
                additions: 0,
                deletions: 0,
            },
        }
    }

    /// The common lines at either end, and the middle as removed then added.
    ///
    /// Every change xfx asks about is **one** region: an edit replaces the one
    /// place its `old_string` matched, and a write replaces the file. So the
    /// common prefix and suffix are exactly the unchanged lines, and what is
    /// between them is the change -- the shape upstream's own fallback review
    /// takes (`diff.zig:655-720`), without the line-matching pass a many-hunk
    /// diff needs. Five unchanged lines are kept on either side and the rest
    /// counted on an elision row (`diff.zig:580-618`).
    pub(crate) fn of_diff(diff: &ApprovalDiff) -> Self {
        let before = lines_of(&diff.before);
        let after = lines_of(&diff.after);
        let prefix = before
            .iter()
            .zip(&after)
            .take_while(|(old, new)| old == new)
            .count();
        let room = before.len().min(after.len()) - prefix;
        let suffix = before
            .iter()
            .rev()
            .zip(after.iter().rev())
            .take(room)
            .take_while(|(old, new)| old == new)
            .count();
        let removed = &before[prefix..before.len() - suffix];
        let added = &after[prefix..after.len() - suffix];
        if removed.is_empty() && added.is_empty() {
            return Self {
                lines: vec![Line::Notice(NO_CHANGE)],
                additions: 0,
                deletions: 0,
            };
        }
        let mut lines = Vec::new();
        let lead = prefix.saturating_sub(CONTEXT_LINES);
        if lead > 0 {
            lines.push(Line::Elided(lead));
        }
        for (index, text) in after.iter().enumerate().take(prefix).skip(lead) {
            lines.push(Line::Context(index + 1, (*text).to_string()));
        }
        for (offset, text) in removed.iter().enumerate() {
            lines.push(Line::Removed(prefix + offset + 1, (*text).to_string()));
        }
        for (offset, text) in added.iter().enumerate() {
            lines.push(Line::Added(prefix + offset + 1, (*text).to_string()));
        }
        let tail = after.len() - suffix;
        let shown = (tail + CONTEXT_LINES).min(after.len());
        for (index, text) in after.iter().enumerate().take(shown).skip(tail) {
            lines.push(Line::Context(index + 1, (*text).to_string()));
        }
        if after.len() > shown {
            lines.push(Line::Elided(after.len() - shown));
        }
        Self {
            lines,
            additions: added.len(),
            deletions: removed.len(),
        }
    }

    /// The review as rows of a `cols`-wide screen, each beside whether it is
    /// part of the change.
    ///
    /// A line too long for its row **wraps** under its own text rather than
    /// being cut: a reviewer approving a line whose end was clipped is
    /// approving bytes they were not shown. The number column is as wide as the
    /// widest number in the review, and never narrower than five.
    pub(crate) fn rows(&self, cols: u16, styles: &Styles) -> Vec<(String, bool)> {
        let digits = self
            .lines
            .iter()
            .filter_map(Line::number)
            .max()
            .map_or(1, |widest| widest.to_string().len());
        let number_width = digits.max(NUMBER_WIDTH);
        // Inset, numbers, a space, the sign, a space.
        let prefix = INSET + number_width + 3;
        let budget = u16::try_from(usize::from(cols).saturating_sub(prefix))
            .unwrap_or(u16::MAX)
            .max(1);
        let mut rows = Vec::new();
        for line in &self.lines {
            let (number, sign, style, text) = match line {
                Line::Context(number, text) => (Some(*number), " ", styles.quiet, text.clone()),
                Line::Added(number, text) => (Some(*number), "+", styles.diff, text.clone()),
                Line::Removed(number, text) => (Some(*number), "-", styles.diff, text.clone()),
                Line::Elided(count) => (
                    None,
                    "⋯",
                    styles.quiet,
                    format!("{count} unchanged lines ⋯"),
                ),
                Line::Notice(text) => {
                    let notice = format!(
                        "{}{}",
                        " ".repeat(INSET + number_width + 2),
                        styles.run(styles.quiet, text)
                    );
                    rows.push((styles.finish(&notice, cols), true));
                    continue;
                }
            };
            let segments = safe_rows(&text, budget);
            for (index, segment) in segments.iter().enumerate() {
                let row = if index == 0 {
                    let number = number.map(|number| number.to_string()).unwrap_or_default();
                    format!(
                        "{}{} {} {}",
                        " ".repeat(INSET + number_width - number.len()),
                        styles.run(styles.quiet, &number),
                        styles.run(style, sign),
                        styles.run(style, segment)
                    )
                } else {
                    format!("{}{}", " ".repeat(prefix), styles.run(style, segment))
                };
                rows.push((styles.finish(&row, cols), line.changed()));
            }
        }
        rows
    }
}

// ---------------------------------------------------------------------------
// the chrome
// ---------------------------------------------------------------------------

/// The header: the title on the left, the kind and the counts on the right
/// (`approval_ui.zig:825-890`, `:1770-1806`).
///
/// The right half is dropped whole, never cut, when the two do not fit with a
/// cell between them -- upstream's rule, and the reason the counts can never
/// read as a different number.
pub(crate) fn header(kind: Kind, review: Option<&Review>, cols: u16, styles: &Styles) -> String {
    let title = if kind.reviews_a_file() {
        FILE_TITLE
    } else {
        CHOOSE_TITLE
    };
    let inner = usize::from(cols).saturating_sub(INSET + RIGHT_MARGIN);
    let mut right = styles.run(styles.quiet, kind.action());
    let mut right_cells = super::wrap::width(kind.action()) as usize;
    if let Some(review) = review.filter(|review| review.additions + review.deletions > 0) {
        let added = format!("+{}", review.additions);
        let removed = format!("-{}", review.deletions);
        right_cells += 3 + added.len() + 2 + removed.len();
        right = format!(
            "{right} · {}  {}",
            styles.run(styles.diff, &added),
            styles.run(styles.diff, &removed)
        );
    }
    let title_cells = super::wrap::width(title) as usize;
    let mut row = format!("{}{}", " ".repeat(INSET), styles.run(styles.bold, title));
    if title_cells + right_cells < inner {
        row.push_str(&" ".repeat(inner - title_cells - right_cells));
        row.push_str(&right);
    }
    styles.finish(&row, cols)
}

/// The dotted rule a file review's chrome starts with (`approval_screen.zig:35`).
pub(crate) fn dotted_rule(cols: u16, styles: &Styles) -> String {
    styles.run(styles.rule, &"┄".repeat(usize::from(cols)))
}

/// The solid rule above the hint row, on the plane that paints its own
/// (`approval_screen.zig:484-490`).
pub(crate) fn solid_rule(cols: u16, styles: &Styles) -> String {
    styles.run(styles.rule, &"─".repeat(usize::from(cols)))
}

/// A blank row.
pub(crate) fn blank() -> String {
    String::new()
}

/// What the question is about, as the rows that name it.
///
/// A file's is upstream's `a.txt  ·  Apply this change?` with the path bold
/// (`approval_ui.zig:892-931`). Where that does not fit on one row the path is
/// **wrapped whole** onto rows of its own and the question follows: upstream
/// clips the path from the left and requires only its last component, and
/// xfx's readiness rule requires the whole target ([`super::approval_readiness`]).
///
/// A command's is the bold question, an empty reason row (upstream's
/// `approvalReasonLine` has nothing to say about a command without a URL), and
/// the command under `$`, opened by a comment naming the tool and where it
/// runs -- upstream's `# shell.run profile=user shell=…` line, in xfx's terms.
pub(crate) fn subject_rows(
    kind: Kind,
    request: &ApprovalRequest,
    cols: u16,
    styles: &Styles,
) -> Vec<String> {
    let budget = cols.saturating_sub(INSET as u16).max(1);
    let inset = " ".repeat(INSET);
    match kind {
        Kind::Edit | Kind::Write => {
            let path = match &request.subject {
                ApprovalSubject::Path(path) => tamed(path),
                ApprovalSubject::Command { .. } => tamed(&request.target),
            };
            let whole = INSET
                + super::wrap::width(&path) as usize
                + PATH_SEPARATOR.len()
                + super::wrap::width(FILE_QUESTION) as usize;
            if whole <= usize::from(cols) {
                return vec![styles.finish(
                    &format!(
                        "{inset}{}{PATH_SEPARATOR}{FILE_QUESTION}",
                        styles.run(styles.bold, &path)
                    ),
                    cols,
                )];
            }
            let mut rows: Vec<String> = safe_rows(&path, budget)
                .iter()
                .map(|segment| {
                    styles.finish(
                        &format!("{inset}{}", styles.run(styles.bold, segment)),
                        cols,
                    )
                })
                .collect();
            rows.push(styles.finish(&format!("{inset}{FILE_QUESTION}"), cols));
            rows
        }
        Kind::Command => {
            let (command, cwd) = match &request.subject {
                ApprovalSubject::Command { command, cwd } => (command.as_str(), cwd.as_str()),
                ApprovalSubject::Path(_) => (request.target.as_str(), "."),
            };
            let mut rows = vec![
                styles.finish(
                    &format!("{inset}{}", styles.run(styles.bold, COMMAND_QUESTION)),
                    cols,
                ),
                blank(),
            ];
            let text = format!("# terminal.exec cwd={cwd}\n{command}");
            let segments = safe_rows(&text, cols.saturating_sub(4).max(1));
            for (index, segment) in segments.iter().enumerate() {
                let lead = if index == 0 { "  $ " } else { "    " };
                rows.push(styles.finish(
                    &styles.run(styles.strong, &format!("{lead}{segment}")),
                    cols,
                ));
            }
            rows
        }
        Kind::Other => {
            let path = match &request.subject {
                ApprovalSubject::Path(path) => path.as_str(),
                ApprovalSubject::Command { command, .. } => command.as_str(),
            };
            let mut rows = vec![
                styles.finish(
                    &format!("{inset}{}", styles.run(styles.bold, OTHER_QUESTION)),
                    cols,
                ),
                blank(),
            ];
            rows.extend(
                safe_rows(&format!("{} {path}", request.tool), budget)
                    .iter()
                    .map(|segment| styles.finish(&format!("{inset}{segment}"), cols)),
            );
            rows
        }
    }
}

/// The rows of one choice: the choice itself, and the rest of its draft.
///
/// The draft sits **on its choice's row**, after the choice's words, as
/// upstream's does (`approval_ui.zig:1559-1592`): `Apply once, use capitals`.
/// Its later rows continue under its first character. `continuation` caps how
/// many of those later rows are painted: the panel pays for the draft last,
/// out of what the screen has left.
///
/// Also answers whether the **choice** reached the row whole -- the words
/// that say which answer it is, not the draft after them -- because that is
/// what a readiness receipt is about.
pub(crate) fn choice_rows(
    kind: Kind,
    index: usize,
    selected: bool,
    draft: Option<&Block>,
    continuation: usize,
    cols: u16,
    styles: &Styles,
) -> (Vec<String>, bool) {
    let file = kind.reviews_a_file();
    let digit = (b'1' + index as u8) as char;
    // The words, and the draft's first row with what it is painted in.
    let (words, first, first_style) = match draft {
        Some(block) => {
            let empty = block.rows.iter().all(String::is_empty);
            let head = amended_head(kind, index);
            if empty && block.caret.is_some() {
                (head, placeholder(index).to_string(), styles.quiet)
            } else {
                (
                    head,
                    block.rows.first().cloned().unwrap_or_default(),
                    if selected { styles.strong } else { "" },
                )
            }
        }
        None => (label(kind, index), String::new(), ""),
    };
    let words_cells = usize::from(choice_prefix_cells(kind)) + super::wrap::width(words) as usize;
    let whole = words_cells <= usize::from(cols);
    let row = match (file, selected) {
        (true, true) => format!(
            "{}{}{}",
            " ".repeat(INSET),
            styles.run(styles.strong, &format!("❯ {digit}  {words}")),
            styles.run(first_style, &first)
        ),
        (true, false) => format!(
            "{}  {}  {words}{}",
            " ".repeat(INSET),
            styles.run(styles.quiet, &digit.to_string()),
            styles.run(first_style, &first)
        ),
        (false, true) => format!(
            "{}❯ {}{}",
            " ".repeat(INSET),
            styles.run(styles.strong, words),
            styles.run(first_style, &first)
        ),
        (false, false) => format!(
            "{}  {words}{}",
            " ".repeat(INSET),
            styles.run(first_style, &first)
        ),
    };
    let mut rows = vec![styles.finish(&row, cols)];
    if let Some(block) = draft {
        let column = " ".repeat(usize::from(draft_column(kind, index)));
        let style = if selected { styles.strong } else { "" };
        rows.extend(block.rows.iter().skip(1).take(continuation).map(|segment| {
            styles.finish(&format!("{column}{}", styles.run(style, segment)), cols)
        }));
    }
    (rows, whole)
}

/// What answering `2` would buy, in the permission layer's own words, under
/// the choices.
///
/// **xfx's, not upstream's**: upstream's second choice names its whole grant in
/// its label, and xfx's grant has a half no label of upstream's shape can
/// carry -- it outlives the process, into every later `xfx ask --resume-id` of
/// a saved session (`crate::permission::PermissionSession::with_durable_session`).
/// A question has to be the question the user is actually answering, so the
/// sentence stays, dim, as upstream's own explanation rows are
/// (`approval_ui.zig:1887-1897`).
pub(crate) fn scope_rows(request: &ApprovalRequest, cols: u16, styles: &Styles) -> Vec<String> {
    let budget = cols.saturating_sub(INSET as u16).max(1);
    safe_rows(&format!("2 = {}", request.always_scope), budget)
        .iter()
        .map(|segment| {
            styles.finish(
                &format!("{}{}", " ".repeat(INSET), styles.run(styles.dim, segment)),
                cols,
            )
        })
        .collect()
}

/// The hint row (`approval_ui.zig:20-45`, `approval_screen.zig:563-598`):
/// the widest of its ladder that fits.
///
/// `amendable` is whether the marked choice carries a draft -- the first and
/// the third do, `Always` does not -- which is what makes Tab amend rather than
/// move. `scrollable` is whether the review has more rows than the screen
/// shows. A file question's hint is indented and in the statusline grey; a
/// command's starts at the first column in the dim grey: both as captured
/// (`approval_ui.zig:1342-1360`, `:1750-1761`).
pub(crate) fn hint(
    kind: Kind,
    amendable: bool,
    scrollable: bool,
    cols: u16,
    styles: &Styles,
) -> String {
    let ladder: &[&str] = match (amendable, scrollable) {
        (true, false) => &AMEND_HINTS,
        (false, false) => &NAVIGATE_HINTS,
        (true, true) => &AMEND_SCROLL_HINTS,
        (false, true) => &NAVIGATE_SCROLL_HINTS,
    };
    let indented = kind.reviews_a_file() || scrollable;
    let inset = if indented { INSET } else { 0 };
    let budget = usize::from(cols).saturating_sub(inset);
    let text = ladder
        .iter()
        .copied()
        .find(|hint| super::wrap::width(hint) as usize <= budget)
        .unwrap_or(ladder[ladder.len() - 1]);
    let style = if indented { styles.quiet } else { styles.dim };
    styles.finish(
        &format!("{}{}", " ".repeat(inset), styles.run(style, text)),
        cols,
    )
}

// ---------------------------------------------------------------------------
// making foreign text row-safe
// ---------------------------------------------------------------------------

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

/// What a character a terminal would obey becomes on a row.
///
/// The three whitespace controls keep the names the permission boundary gives
/// them (`crate::permission::bounded_diff_side`), so a diff that arrived
/// already escaped is unchanged by passing through here again -- and a
/// backslash is passed through untouched for the same reason. Everything else
/// a terminal would obey or reorder is named by its **code point**
/// (`crate::permission::scalar_token`): one replacement character for all of
/// them would show two payloads a model can swap as one row.
fn escaped(character: char) -> Option<String> {
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

/// One line of foreign text with nothing left in it a terminal would obey or
/// reorder.
pub(crate) fn tamed(line: &str) -> String {
    let mut safe = String::with_capacity(line.len());
    for character in line.chars() {
        match escaped(character) {
            Some(replacement) => safe.push_str(&replacement),
            None => safe.push(character),
        }
    }
    safe
}

/// `text` as rows of a `budget`-wide screen, with nothing left in it a terminal
/// would obey or reorder.
///
/// The **split comes first**: a real line break in the payload is a row break
/// here, so text that carries newlines reads as the file does. What is then
/// escaped is what is left inside each line, and the wrap runs last -- on text
/// whose every character is one the wrap can measure. The blank a wrap keeps at
/// the end of a broken row is dropped, so no row is wider than its budget.
pub(crate) fn safe_rows(text: &str, budget: u16) -> Vec<String> {
    let budget = budget.max(1);
    let mut out = Vec::new();
    for line in text.split(['\n', '\r']) {
        let safe = tamed(line);
        if safe.is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(
            super::wrap::wrap(&safe, budget)
                .iter()
                .map(|row| safe[row.start..row.end].trim_end().to_string()),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(before: &str, after: &str) -> ApprovalDiff {
        ApprovalDiff {
            before: before.to_string(),
            after: after.to_string(),
        }
    }

    fn plain_rows(review: &Review, cols: u16) -> Vec<String> {
        review
            .rows(cols, &Styles::PLAIN)
            .into_iter()
            .map(|(row, _)| row)
            .collect()
    }

    #[test]
    fn an_edit_is_reviewed_with_its_numbers_and_the_lines_around_it() {
        // The capture this was written against: `edit_file` on a three-line
        // `a.txt`, `alpha line 2` -> `ALPHA LINE TWO`
        // (`fx-approvals-40x120-edit-ask`, rows 6-9).
        let review = Review::of_diff(&diff(
            "alpha line 1\nalpha line 2\nalpha line 3\n",
            "alpha line 1\nALPHA LINE TWO\nalpha line 3\n",
        ));
        assert_eq!((review.additions, review.deletions), (1, 1));
        assert_eq!(
            plain_rows(&review, 120),
            [
                "      1   alpha line 1",
                "      2 - alpha line 2",
                "      2 + ALPHA LINE TWO",
                "      3   alpha line 3",
            ]
        );
    }

    #[test]
    fn a_new_file_is_all_additions_and_counts_nothing_removed() {
        // `fx-approvals-40x120-write-ask`: `Write · +2  -0`, two `+` rows.
        let review = Review::of_diff(&diff("", "new file\nsecond line\n"));
        assert_eq!((review.additions, review.deletions), (2, 0));
        assert_eq!(
            plain_rows(&review, 120),
            ["      1 + new file", "      2 + second line"]
        );
        assert_eq!(
            header(Kind::Write, Some(&review), 120, &Styles::PLAIN),
            format!(
                "  Permission needed · Review change{}Write · +2  -0",
                " ".repeat(120 - 4 - 33 - 14)
            ),
            "the counts are not right-aligned two cells from the edge"
        );
    }

    #[test]
    fn unchanged_lines_past_five_on_either_side_are_counted_rather_than_shown() {
        let before: Vec<String> = (1..=20).map(|line| format!("line {line}")).collect();
        let mut after = before.clone();
        after[9] = "LINE TEN".to_string();
        let review = Review::of_diff(&diff(&before.join("\n"), &after.join("\n")));
        assert_eq!(review.lines.first(), Some(&Line::Elided(4)));
        assert_eq!(review.lines.last(), Some(&Line::Elided(5)));
        let context = review
            .lines
            .iter()
            .filter(|line| matches!(line, Line::Context(..)))
            .count();
        assert_eq!(context, 2 * CONTEXT_LINES);
        let rows = plain_rows(&review, 80);
        assert_eq!(rows[0], "        ⋯ 4 unchanged lines ⋯");
        assert_eq!(rows[6], "     10 - line 10");
        assert_eq!(rows[7], "     10 + LINE TEN");
    }

    #[test]
    fn a_line_too_long_for_its_row_wraps_under_its_own_text_rather_than_being_cut() {
        let review = Review::of_diff(&diff("", &format!("{}\n", "word ".repeat(30))));
        let rows = plain_rows(&review, 40);
        assert!(rows.len() > 1, "the line was not wrapped: {rows:?}");
        for row in &rows[1..] {
            assert!(
                row.starts_with(&" ".repeat(10)) && !row[10..].starts_with(' '),
                "a continuation row is not under the text: {row:?}"
            );
        }
        for row in &rows {
            assert!(super::super::wrap::width(row) <= 40, "{row:?}");
        }
        let words: usize = rows.iter().map(|row| row.matches("word").count()).sum();
        assert_eq!(words, 30, "a word was lost to the wrap");
    }

    #[test]
    fn the_command_is_shown_whole_under_its_comment_and_a_long_one_wraps() {
        // `fx-approvals-40x120-cmd-ask` rows 15-18, in xfx's terms.
        let request = ApprovalRequest {
            tool: "terminal",
            target: "echo hi in /work".into(),
            summary: "run `echo hi` in .".into(),
            always_scope: "allow every future run of exactly `echo hi` in .".into(),
            diff: None,
            subject: ApprovalSubject::Command {
                command: "echo hi > out.txt && cat out.txt".into(),
                cwd: ".".into(),
            },
        };
        assert_eq!(
            subject_rows(Kind::Command, &request, 120, &Styles::PLAIN),
            [
                "  Would you like to run the following command?",
                "",
                "  $ # terminal.exec cwd=.",
                "    echo hi > out.txt && cat out.txt",
            ]
        );
        let mut long = request.clone();
        long.subject = ApprovalSubject::Command {
            command: "echo step ".repeat(20),
            cwd: ".".into(),
        };
        let rows = subject_rows(Kind::Command, &long, 40, &Styles::PLAIN);
        let steps: usize = rows.iter().map(|row| row.matches("step").count()).sum();
        assert_eq!(steps, 20, "the command was not shown whole: {rows:?}");
    }

    #[test]
    fn a_choice_carries_its_draft_on_its_own_row_and_the_placeholder_while_empty() {
        // `fx-approvals-40x120-edit-tab-amend` and `edit-amend-typed`, row 15.
        let open = Block {
            choice: 0,
            rows: vec![String::new()],
            caret: Some((0, 0)),
        };
        assert_eq!(
            choice_rows(Kind::Edit, 0, true, Some(&open), 3, 120, &Styles::PLAIN).0,
            ["  ❯ 1  Apply once, and tell xfx what to do next"]
        );
        let typed = Block {
            choice: 0,
            rows: vec!["use capitals".to_string()],
            caret: Some((0, 12)),
        };
        assert_eq!(
            choice_rows(Kind::Edit, 0, true, Some(&typed), 3, 120, &Styles::PLAIN).0,
            ["  ❯ 1  Apply once, use capitals"]
        );
        assert_eq!(
            choice_rows(
                Kind::Command,
                2,
                false,
                Some(&typed),
                3,
                120,
                &Styles::PLAIN
            )
            .0,
            ["    3. No, use capitals"]
        );
        assert_eq!(draft_column(Kind::Edit, 0), 19);
        assert_eq!(draft_column(Kind::Command, 0), 12);
    }

    #[test]
    fn the_rows_are_painted_in_the_captured_sequences() {
        // The SGR a dark 256-colour palette writes, against the `.ansi`
        // capture's (`fx-approvals-40x120-edit-ask.ansi`, as re-serialized by
        // tmux: `ESC[1m…ESC[0m`, `ESC[38;5;245m…ESC[39m`).
        let styles = Styles::of(&Palette {
            mode: super::super::theme::Mode::Dark,
            depth: super::super::theme::Depth::Ansi256,
        });
        let marked = choice_rows(Kind::Edit, 0, true, None, 0, 120, &styles).0;
        assert_eq!(marked, ["  \u{1b}[1;38;5;255m❯ 1  Apply once\u{1b}[0m"]);
        let other = choice_rows(Kind::Edit, 1, false, None, 0, 120, &styles).0;
        assert_eq!(
            other,
            ["    \u{1b}[38;5;245m2\u{1b}[0m  Apply + allow future edits to this file for this session"]
        );
        let review = Review::of_diff(&diff("a\n", "b\n"));
        let rows = review.rows(120, &styles);
        assert_eq!(
            rows[1].0,
            "      \u{1b}[38;5;245m1\u{1b}[0m \u{1b}[38;5;252m+\u{1b}[0m \u{1b}[38;5;252mb\u{1b}[0m"
        );
        assert!(rows[1].1, "an added line is not part of the change");
        assert_eq!(
            hint(Kind::Command, true, false, 120, &styles),
            "\u{1b}[38;5;245m1–3 choose now    ↑↓ options    tab amend    enter confirm    esc cancel\u{1b}[0m"
        );
    }

    #[test]
    fn the_hint_says_tab_amends_only_where_it_does_and_keeps_enter_and_esc_when_narrow() {
        let plain = Styles::PLAIN;
        assert_eq!(
            hint(Kind::Edit, false, false, 120, &plain),
            "  1–3 choose now    ↑↓ or tab options    enter confirm    esc cancel"
        );
        assert_eq!(
            hint(Kind::Edit, true, true, 120, &plain),
            "  1–3 choose now    ↑↓ options    tab amend    ctrl+p/n scroll    enter confirm    esc cancel"
        );
        let narrow = hint(Kind::Command, true, false, 30, &plain);
        assert_eq!(narrow, "enter confirm    esc cancel");
    }

    #[test]
    fn nothing_a_terminal_would_obey_or_reorder_reaches_a_row() {
        for hostile in ["\u{1b}[2J", "\u{7}", "\u{202e}", "\u{2066}", "\u{9b}"] {
            let rows = safe_rows(&format!("a{hostile}b"), 80);
            for row in &rows {
                assert!(
                    !row.chars().any(|c| c.is_control() || reorders(c)),
                    "{hostile:?} reached a row: {row:?}"
                );
            }
        }
        assert_eq!(safe_rows("one\ntwo", 80), ["one", "two"]);
        assert_eq!(
            safe_rows("\u{1b}x", 80),
            [format!("{}x", crate::permission::scalar_token('\u{1b}'))]
        );
    }
}
