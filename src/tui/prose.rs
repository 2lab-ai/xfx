//! The answer's soft-wrap: upstream's assistant wrap
//! (`vercel-labs/fx@c1db9191 src/ui/render_engine/assistant_wrap.zig`), rule
//! for rule.
//!
//! [`super::wrap::wrap`] is the composer's rule (`visual_layout.zig`), and it
//! stays the rule of every surface upstream lays out with something other than
//! its assistant wrap. Upstream gives assistant text a function of its own:
//! every line of a turn's answer -- prose, the operational lines a turn says
//! ahead of it, headings, list items, quotes, definitions and footnotes
//! (`transcript_blocks.zig:1894`, `wrapTranscriptAssistantTextWithFinality…`)
//! -- and the literal detail Ctrl-O shows under a tool call
//! (`wrapLiteralToolOutput`, `full_transcript_screen.zig:6384`). This is that
//! function. Tables (`wrapTableCell`, `transcript_blocks.zig:1443`) and code
//! panels (`prefixStructuralRows`) are laid out by their own renderers and do
//! not come here.
//!
//! What it does that the composer's wrap does not, each of them upstream's:
//!
//! * **No orphan.** When the word that overflows a row is the **last word of
//!   its line**, the word before it goes down with it, so a paragraph never
//!   ends on a row holding one word (`:219-267` when the overflow is the space
//!   in front of that word, `:269-298` when it is the word itself, and only
//!   when two words stay behind). Only when the pair fits the next row
//!   (`wordBreakFits`, `reflowAtWordBreak`); a word that is not the line's
//!   last breaks as any word does.
//! * **Continuation rows line up under the text.** A list item's, a quote's, a
//!   definition's, a footnote's and an indented paragraph's rows after the
//!   first begin where the text after the marker began, and a quote's repeat
//!   its rules (`AssistantContinuation`, `:48-54`, `:682-727`, `:894-1154`).
//!   The marker is read off the rendered line (the presenter's bytes, which
//!   are upstream's), and the lead is dropped on a row it would not leave room
//!   on.
//! * **The space a row breaks at is the break.** A space that would cross the
//!   margin ends the row and is not carried down (`:248-266`); the spaces of a
//!   run that fit stay on the row they end, and a space after the one that
//!   crossed begins the next row. Upstream's row of a reflowed break ends
//!   before the break's spaces.
//! * **A tab stops every eight columns of the screen** (`:163-174`,
//!   `display_width.zig:290-300`), counted from the screen's left edge, gutter
//!   included -- not the composer's four cells.
//!
//! A row is still a **range of the text** ([`super::wrap::Row`]) and the rows
//! still tile it, which is what the transcript's freeze and resize rest on: a
//! dropped space hangs at the end of the row it ended, past the margin, where
//! the painter's cut leaves it (`super::transcript`'s `drawn`), and the spaces
//! of a reflowed break hang inside it, after the row's colour is closed -- a
//! blank cell either way, as upstream's. What is not in the text is the
//! **lead** a continuation row begins with, so it travels beside the range
//! ([`ProseRow::lead`]) and the transcript writes it in front.
//!
//! A cell is measured as the painter measures it ([`super::wrap::width`]),
//! one grapheme cluster at a time, so a row this wraps is a row the painter
//! cuts where this said it ends.
//!
//! # What is not ported, and why
//!
//! * **A wide unit in a one-column content area becomes `?`** (`:216`), and
//!   **a footnote or definition marker that cannot fit beside the next unit
//!   is dropped** (`infeasibleMarkerEnd`, `:807-819`). Both need a content
//!   area a few cells wide; the screen is never narrower than `MIN_COLS`
//!   (`super::layout`), which leaves the answer eighteen, and a marker that
//!   cannot fit there is a footnote numbered with fourteen digits. Below a
//!   width the layout accepts, a unit wider than the row takes a row of its
//!   own rather than leave an empty one, as [`super::wrap::wrap`]'s does.
//! * **The pen and an open link carried across a line break** (`:144-148`,
//!   `:364-384`). The transcript starts each line from a fresh pen; the
//!   presenter renders a line at a time and leaves none open at its end, which
//!   `the_presenter_leaves_no_pen_or_link_open_across_a_line_break` pins.
//! * **The bytes around a row's start.** Upstream does not copy the sequences
//!   in front of a guttered row's first cell and writes the pen it reached
//!   instead (`:139`, `:378-383`); the transcript writes the line's own bytes.
//!   The cells are the same.
//! * **The retention source map** (`RetentionSourceMap`, `:459-522`): upstream
//!   maps rendered offsets back to the source to retain a re-wrapped answer;
//!   a [`Row`] is already a range of the source.

use std::borrow::Cow;

use super::wrap::{self, Row};

/// How many bytes of zero-width units one literal row takes before it starts
/// another (`literal_command_zero_width_row_byte_limit`, `:9-12`): a
/// pathological run of combining marks is rowed rather than left on one row
/// of unbounded size.
const ZERO_WIDTH_ROW_BYTES: usize = 256;

const DIM_OPEN: &str = "\u{1b}[2m";
const DIM_CLOSE: &str = "\u{1b}[22m";
const RESET: &str = "\u{1b}[0m";
/// What upstream's pacer writes between two characters of a dim marker it
/// paced apart (`skipPacerDimReassertions`, `:1149-1154`). xfx's presenter
/// renders a line whole and writes none, but a marker that carries them is
/// still the marker.
const REASSERT: &str = "\u{1b}[0m\u{1b}[2m";
const BULLET: &str = "\u{2022}";
const RULE: &str = "\u{2502}";
const PENDING: &str = "\u{2610}";
const COMPLETED: &str = "\u{2713}";
/// The completed task marker's colours: the dark and the light built-in
/// theme's (`crate::markdown::theme`), which are upstream's two built-ins
/// (`:1123`). Upstream also takes a custom theme's; xfx has none.
const COMPLETED_OPENS: [&str; 2] = ["\u{1b}[38;5;252m", "\u{1b}[38;5;238m"];
const COMPLETED_CLOSE: &str = "\u{1b}[39m";

/// What the rows after a line's first one begin with
/// (`AssistantContinuation`, `:48-54`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Continuation {
    /// Blank columns: under a list item's, a definition's or a footnote's
    /// text, or an indented paragraph's own indent.
    Indent(usize),
    /// A quote's: its own indent, then its rule and a space for each level,
    /// dim, as the first row has them.
    Quote { indent: usize, depth: usize },
}

impl Continuation {
    /// The cells the lead takes (`continuationWidth`, `:885-890`).
    pub(crate) fn width(self) -> usize {
        match self {
            Continuation::Indent(indent) => indent,
            Continuation::Quote { indent, depth } => indent + 2 * depth,
        }
    }

    /// The lead as it is written (`appendAssistantContinuation`, `:703-716`):
    /// self-contained, so the row's own attributes are reopened after it.
    pub(crate) fn lead(self) -> String {
        match self {
            Continuation::Indent(indent) => " ".repeat(indent),
            Continuation::Quote { indent, depth } => {
                let mut lead = " ".repeat(indent);
                for _ in 0..depth {
                    lead.push_str(DIM_OPEN);
                    lead.push_str(RULE);
                    lead.push(' ');
                    lead.push_str(DIM_CLOSE);
                }
                lead
            }
        }
    }
}

/// One row: the range of the text it shows and the cells that range takes
/// ([`Row`]), and the lead written in front of it.
///
/// `lead` is `None` on a line's first row -- its marker is in the text -- and
/// on a continuation row the lead would leave no room on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProseRow {
    pub(crate) row: Row,
    pub(crate) lead: Option<Continuation>,
}

impl ProseRow {
    /// The cells the lead takes.
    pub(crate) fn lead_width(&self) -> u16 {
        self.lead
            .map_or(0, |lead| u16::try_from(lead.width()).unwrap_or(u16::MAX))
    }
}

/// Where a text begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opening {
    /// At the start of its line, so its continuation is read off its marker.
    Line,
    /// On a continuation row of a line whose first rows are no longer held --
    /// the transcript's freeze -- with the continuation that line set, if it
    /// set one. Its marker is gone, and text that happens to begin with
    /// blanks is not an indent.
    Continued(Option<Continuation>),
}

/// The rows answer prose occupies on a screen `cols` wide behind a gutter of
/// `gutter` cells, `cols` being what the gutter leaves.
///
/// The gutter is a parameter for one reason: a tab stops at a column of the
/// **screen**, and the gutter is part of the screen.
pub(crate) fn prose(text: &str, cols: u16, gutter: u16, opening: Opening) -> Vec<ProseRow> {
    Layout::new(text, cols, gutter, false, opening).run()
}

/// The rows literal tool or command output occupies on `cols` behind a
/// `gutter`-cell rail (`wrapLiteralCommandOutput`, `wrapLiteralToolOutput`,
/// `:524-579`): the same wrap with no markdown read into it -- no
/// continuation, every row begins at the rail -- and a long run of zero-width
/// units rowed ([`ZERO_WIDTH_ROW_BYTES`]).
///
/// Only the tests read the rows alone; the screen draws through
/// [`literal_drawn`].
#[cfg(test)]
pub(crate) fn literal(text: &str, cols: u16, gutter: u16) -> Vec<Row> {
    Layout::new(text, cols, gutter, true, Opening::Line)
        .run()
        .into_iter()
        .map(|row| row.row)
        .collect()
}

/// `literal`'s rows as the text to draw: each tab written as the spaces the
/// wrap gave it where it met it, and kept so when the reflow moves it down with
/// its word.
///
/// That is what upstream draws: its wrap writes a tab into its output as the
/// spaces to the next stop the moment it measures it
/// (`assistant_wrap.zig:163-174@481104fd`), and `reflowAtWordBreak`
/// (`:729-775`) moves those spaces, as they are, to the continuation row
/// (`p5-transcript-safetab`: `aaa b<TAB>x…` at 120 columns is `aaa` over
/// `b x…`, one space). A tab measured again where it lands would be wider than
/// the row was measured for, and the row's last characters would be cut off
/// by the clip.
#[cfg(test)]
pub(crate) fn literal_drawn(text: &str, cols: u16, gutter: u16) -> Vec<String> {
    let (rows, tabs) = literal_layout(text, cols, gutter);
    rows.into_iter()
        .map(|row| literal_piece(text, row, &tabs))
        .collect()
}

/// [`literal_drawn`] before anything is drawn: the rows, as ranges of `text`,
/// and each tab's byte offset and the cells the wrap gave it -- what a
/// row's text is made from ([`literal_piece`]), at any time after.
pub(crate) fn literal_layout(
    text: &str,
    cols: u16,
    gutter: u16,
) -> (Vec<Row>, Vec<(usize, usize)>) {
    let (rows, tabs) = Layout::new(text, cols, gutter, true, Opening::Line).run_with_tabs();
    (rows.into_iter().map(|laid| laid.row).collect(), tabs)
}

/// One of [`literal_layout`]'s rows as the text to draw: `text`'s bytes in
/// `row`, each tab in it written as the spaces `tabs` gives it. `text` and
/// `tabs` are the layout's, or the same with every offset moved on by one
/// amount (a line's layout inside a longer text).
pub(crate) fn literal_piece(text: &str, row: Row, tabs: &[(usize, usize)]) -> String {
    let Row { start, end, .. } = row;
    let mut out = String::with_capacity(end - start);
    let mut written = start;
    let first = tabs.partition_point(|&(at, _)| at < start);
    for &(at, cells) in &tabs[first..] {
        if at >= end {
            break;
        }
        out.push_str(&text[written..at]);
        out.extend(std::iter::repeat_n(' ', cells));
        written = at + 1;
    }
    out.push_str(&text[written..end]);
    out
}

/// `row` -- the text of one prose row -- with every tab written as the spaces
/// to the next stop, the row's first cell being screen column `first_col`
/// (one-based) on a screen `cols` wide (`:163-174`).
///
/// The painter's half of the tab rule, which the wrap measured with the same
/// stops. Borrowed when there is no tab, which is nearly every row.
pub(crate) fn expand_tabs(row: &str, first_col: usize, cols: usize) -> Cow<'_, str> {
    if !row.contains('\t') {
        return Cow::Borrowed(row);
    }
    let mut out = String::with_capacity(row.len() + 8);
    let mut col = first_col.max(1);
    let mut written = 0usize;
    for painted in wrap::painting(row) {
        let at = painted.end - painted.cluster.len();
        out.push_str(&row[written..at]);
        written = painted.end;
        if painted.cluster == "\t" {
            let next = next_tab_stop(col, cols.max(1));
            out.extend(std::iter::repeat_n(' ', next.saturating_sub(col)));
            col = col.max(next);
        } else {
            out.push_str(painted.cluster);
            col += usize::from(wrap::cluster_width(painted.cluster));
        }
    }
    out.push_str(&row[written..]);
    Cow::Owned(out)
}

/// The next eight-column tab stop after one-based `col`, never past the last
/// column (`nextTabStopColumn`, `display_width.zig:292-300`).
pub(crate) fn next_tab_stop(col: usize, cols: usize) -> usize {
    if col >= cols {
        return cols;
    }
    (((col - 1) / 8 + 1) * 8 + 1).min(cols)
}

/// Whether a unit `w` cells wide at one-based `col` would cross the margin
/// (`shouldWrapAt`, `display_width.zig:286-288`).
fn crosses(col: usize, w: usize, cols: usize) -> bool {
    col + w - 1 > cols
}

/// One run of spaces a row may break at (`WordBreak`, `:14-21`): its bytes,
/// the column after it, and whether another break is on the row before it.
#[derive(Debug, Clone, Copy)]
struct Break {
    start: usize,
    end: usize,
    col_end: usize,
    has_preceding_word: bool,
}

/// The last two breaks on the row (`WordBreaks`, `:23-41`).
#[derive(Debug, Default, Clone, Copy)]
struct Breaks {
    last: Option<Break>,
    previous: Option<Break>,
}

impl Breaks {
    /// Extends `last` when the run is adjacent to it, and otherwise demotes it.
    fn record(&mut self, found: Break) {
        if let Some(last) = &mut self.last {
            if last.end == found.start {
                last.end = found.end;
                last.col_end = found.col_end;
                return;
            }
            self.previous = Some(*last);
        }
        self.last = Some(found);
    }
}

/// `wrapAssistantTextWithBaseGutter` (`:65-352`), in the columns the gutter
/// leaves: upstream's `col` starts at `gutter + 1` and wraps at the screen's
/// width, which is the same arithmetic as starting at 1 and wrapping at the
/// width that is left.
struct Layout<'a> {
    text: &'a str,
    cols: usize,
    /// The screen's width, gutter included, for the tab stops.
    screen: usize,
    gutter: usize,
    literal: bool,
    continuation: Option<Continuation>,
    rows: Vec<ProseRow>,
    /// The row being built: where its text begins, its lead, the column its
    /// text begins at, and the column the next unit lands in.
    start: usize,
    lead: Option<Continuation>,
    first_col: usize,
    col: usize,
    breaks: Breaks,
    has_word: bool,
    zero_run: usize,
    /// Each tab's byte offset and the cells it moved to its stop, where it was
    /// met ([`literal_drawn`]).
    tabs: Vec<(usize, usize)>,
    /// Whether runs the loop would only count are counted at once
    /// ([`Self::step_over`]): always, but for the tests, which hold the wrap
    /// that counts every unit against it.
    fast: bool,
}

impl<'a> Layout<'a> {
    fn new(text: &'a str, cols: u16, gutter: u16, literal: bool, opening: Opening) -> Self {
        let cols = usize::from(cols.max(1));
        let continuation = match (literal, opening) {
            (true, _) => None,
            (false, Opening::Line) => continuation_at(text, 0),
            (false, Opening::Continued(carried)) => carried,
        };
        let mut layout = Self {
            text,
            cols,
            screen: cols + usize::from(gutter),
            gutter: usize::from(gutter),
            literal,
            continuation,
            rows: Vec::new(),
            start: 0,
            lead: None,
            first_col: 1,
            col: 1,
            breaks: Breaks::default(),
            has_word: false,
            zero_run: 0,
            tabs: Vec::new(),
            fast: true,
        };
        if let (false, Opening::Continued(_)) = (literal, opening) {
            let next = first_visible_width(text).map_or(1, usize::from);
            (layout.lead, layout.col) = layout.opening(next);
            layout.first_col = layout.col;
        }
        layout
    }

    fn run(self) -> Vec<ProseRow> {
        self.run_with_tabs().0
    }

    /// [`Self::run`], and where each tab stood and the cells it took.
    fn run_with_tabs(mut self) -> (Vec<ProseRow>, Vec<(usize, usize)>) {
        let text = self.text;
        let mut units = units_from(text, 0, self.literal);
        while let Some((unit, whole)) = units.next() {
            let wrap::Painted {
                start: before,
                end,
                cluster,
            } = unit;
            let at = end - cluster.len();
            // A lone carriage return breaks the line as upstream's does; the
            // transcript has already made every one a newline.
            if wrap::is_line_break(cluster) || cluster == "\r" {
                self.hard_break(end);
                continue;
            }
            if cluster == "\t" {
                // A tab never wraps: it moves to the next stop, the last column
                // at most, and whatever follows it does the wrapping.
                let next = next_tab_stop(self.gutter + self.col, self.screen);
                let was = self.col;
                self.col = self.col.max(next.saturating_sub(self.gutter));
                self.tabs.push((at, self.col - was));
                self.zero_run = 0;
                continue;
            }
            let w = usize::from(wrap::cluster_width(cluster));
            if w == 0 {
                // A control is not a zero-width unit to upstream: it is
                // written and counted nowhere (`:175-179`).
                if self.literal && !cluster.as_bytes().first().is_some_and(|byte| *byte < 0x20) {
                    if self.zero_run + cluster.len() > ZERO_WIDTH_ROW_BYTES {
                        self.wrap_to(before, self.col, before, 1);
                        self.has_word = false;
                        self.zero_run = 0;
                    }
                    self.zero_run += cluster.len();
                }
                continue;
            }
            self.zero_run = 0;

            if cluster == " " && crosses(self.col, w, self.cols) {
                if !self.orphan_at_space(at, end, w) {
                    // The space is the break: it ends this row, past the
                    // margin, and the next begins after it.
                    self.wrap_to(end, self.col + w, end, w);
                    self.has_word = false;
                    continue;
                }
                self.has_word = true;
            }

            if crosses(self.col, w, self.cols) {
                if let Some(last) = self.breaks.last {
                    let mut at_break = last;
                    if let Some(previous) = self.breaks.previous {
                        if previous.has_preceding_word
                            && is_final_word(text, at, self.fast)
                            && self.fits(self.col - previous.col_end, at)
                        {
                            at_break = previous;
                        }
                    }
                    self.reflow(at_break, w, at);
                }
            }
            // A unit wider than a whole row stays on the row it begins rather
            // than leave an empty one above it. Upstream never meets one: a
            // wide rune in a one-column content area is written as `?`
            // (`:216`), a width the screen's minimum (`super::layout`'s
            // `MIN_COLS`) never asks this wrap for.
            let fresh = before == self.start && self.col == self.first_col;
            if crosses(self.col, w, self.cols) && !fresh {
                // No break on the row, or none whose words fit the next one:
                // the unit that crosses begins the next row.
                self.wrap_to(before, self.col, before, w);
                self.has_word = false;
            }

            self.col += w;
            if cluster == " " {
                if self.has_word && !self.separates_marker(self.col - 1) {
                    let has_preceding_word = self.breaks.last.is_some();
                    self.breaks.record(Break {
                        start: at,
                        end,
                        col_end: self.col,
                        has_preceding_word,
                    });
                }
            } else {
                self.has_word = true;
                if self.fast && whole {
                    if let Some(next) = self.step_over(at, end, w) {
                        units = units_from(text, next, self.literal);
                    }
                }
            }
        }
        let width = self.col.saturating_sub(self.first_col);
        self.push(text.len(), width);
        (self.rows, self.tabs)
    }

    /// After a whole cluster at `at..end`, `w` cells, that was not a space and
    /// is on the row: steps over what follows it that the loop would only
    /// count -- a cluster that is no space, no tab, no break, no control, has
    /// cells and lands on the row without crossing the margin, for which the
    /// loop moves the column on by its cells and nothing else -- and says
    /// where the loop goes on, when it stepped over anything.
    ///
    /// Two such runs are known without segmenting: printable ASCII past the
    /// space ([`wrap::single_cells`]), and the cluster at `at` repeated
    /// ([`wrap::repeats`]) -- an argument Ctrl-O shows of megabytes is one or
    /// the other once the channel has spelled it, and so is its heading. A
    /// row of either costs about a row, not each of its characters.
    fn step_over(&mut self, at: usize, end: usize, w: usize) -> Option<usize> {
        let room = (self.cols + 1).saturating_sub(self.col);
        let run = wrap::single_cells(self.text, end, room);
        if run > 0 {
            self.col += run;
            return Some(end + run);
        }
        let copies = wrap::repeats(self.text, at, &self.text[at..end], room / w);
        if copies > 0 {
            self.col += copies * w;
            return Some(end + copies * (end - at));
        }
        None
    }

    /// The space at `at..end` crosses the margin: moves the row's last word
    /// down when the word after the space is the line's last and the two fit
    /// the next row (`:219-247`). Says whether it did.
    fn orphan_at_space(&mut self, at: usize, end: usize, w: usize) -> bool {
        let Some(last) = self.breaks.last else {
            return false;
        };
        if remaining_word_width(self.text, end, self.fast) == 0
            || !is_final_word(self.text, end, self.fast)
            || !self.fits(self.col - last.col_end, end)
        {
            return false;
        }
        let next = first_visible_width(&self.text[end..]).map_or(w, usize::from);
        self.reflow(last, next, at)
    }

    /// Whether the text after a break, `suffix` cells of it, and the rest of
    /// the word at `word` fit a continuation row (`wordBreakFits`,
    /// `:830-842`).
    fn fits(&self, suffix: usize, word: usize) -> bool {
        let indent = self.continuation.map_or(0, Continuation::width);
        if indent >= self.cols {
            return false;
        }
        suffix + remaining_word_width(self.text, word, self.fast) <= self.cols - indent
    }

    /// Breaks the row at `at_break` instead of where the margin is, carrying
    /// the text after it -- up to `now` -- onto a continuation row, when that
    /// text and the next `next_width` cells fit there
    /// (`reflowAtWordBreak`, `:729-775`). Says whether it did.
    fn reflow(&mut self, at_break: Break, next_width: usize, now: usize) -> bool {
        let suffix = self.col - at_break.col_end;
        let indent = self.continuation.map_or(0, Continuation::width);
        if indent >= self.cols || suffix + next_width > self.cols - indent {
            return false;
        }
        let first =
            first_visible_width(&self.text[at_break.end..now]).map_or(next_width, usize::from);
        self.wrap_to(at_break.end, at_break.col_end, at_break.end, first);
        self.col += suffix;
        true
    }

    /// Ends the row being built at `end`, its text having taken the columns
    /// before `col_end`, and begins a continuation row at `next` whose first
    /// unit is `next_width` cells (`appendAssistantContinuation`, `:682-727`).
    fn wrap_to(&mut self, end: usize, col_end: usize, next: usize, next_width: usize) {
        let width = col_end.saturating_sub(self.first_col);
        self.push(end, width);
        (self.lead, self.col) = self.opening(next_width);
        self.first_col = self.col;
        self.start = next;
        self.breaks = Breaks::default();
    }

    /// A line break in the text: the row ends with it, and the next line reads
    /// its own marker.
    fn hard_break(&mut self, end: usize) {
        let width = self.col - self.first_col;
        self.push(end, width);
        self.continuation = if self.literal {
            None
        } else {
            continuation_at(self.text, end)
        };
        self.start = end;
        self.lead = None;
        self.col = 1;
        self.first_col = 1;
        self.breaks = Breaks::default();
        self.has_word = false;
        self.zero_run = 0;
    }

    fn push(&mut self, end: usize, width: usize) {
        self.rows.push(ProseRow {
            row: Row {
                start: self.start,
                end,
                width: u16::try_from(width).unwrap_or(u16::MAX),
            },
            lead: self.lead,
        });
    }

    /// The lead a continuation row whose first unit is `next_width` cells
    /// begins with, and the column its text begins at: the line's
    /// continuation when it leaves room for that unit, nothing otherwise.
    fn opening(&self, next_width: usize) -> (Option<Continuation>, usize) {
        match self.continuation {
            Some(continuation)
                if continuation.width() < self.cols
                    && next_width <= self.cols - continuation.width() =>
            {
                (Some(continuation), continuation.width() + 1)
            }
            _ => (None, 1),
        }
    }

    /// Whether a space at `col` is the one that ends the line's marker -- a
    /// break there would leave the marker alone on its row
    /// (`isContinuationMarkerSeparator`, `:821-828`).
    fn separates_marker(&self, col: usize) -> bool {
        self.continuation
            .is_some_and(|continuation| col == continuation.width())
    }
}

/// The units of `text` that paint something or are controls, with the escape
/// sequences in front of each ([`super::wrap::painting`]).
///
/// Literal text has one more cut: a cluster longer than
/// [`ZERO_WIDTH_ROW_BYTES`] is a base and a pathological run of marks, and its
/// characters are units of their own so the run can be rowed, as upstream's
/// units -- one per rune -- are. Nothing is allocated for the cut: this runs
/// over every cluster of every answer row the transcript wraps.
///
/// From byte `from`, where a cluster begins ([`wrap::painting_from`]), and
/// each unit with whether it is a whole cluster rather than a character of
/// one cut.
fn units_from(
    text: &str,
    from: usize,
    literal: bool,
) -> impl Iterator<Item = (wrap::Painted<'_>, bool)> {
    wrap::painting_from(text, from).flat_map(move |painted| {
        let split = literal && painted.cluster.len() > ZERO_WIDTH_ROW_BYTES;
        Pieces {
            text,
            painted,
            split,
            offset: 0,
        }
        .map(move |piece| (piece, !split))
    })
}

/// One cluster as [`units`] hands it on: whole, or a character at a time.
struct Pieces<'a> {
    text: &'a str,
    painted: wrap::Painted<'a>,
    split: bool,
    /// How far into the cluster the next piece begins.
    offset: usize,
}

impl<'a> Iterator for Pieces<'a> {
    type Item = wrap::Painted<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let cluster = self.painted.cluster;
        if self.offset >= cluster.len() {
            return None;
        }
        if !self.split {
            self.offset = cluster.len();
            return Some(self.painted);
        }
        let at = self.painted.end - cluster.len();
        let character = cluster[self.offset..].chars().next()?;
        // The sequences in front of the cluster go with its first character.
        let start = if self.offset == 0 {
            self.painted.start
        } else {
            at + self.offset
        };
        let end = at + self.offset + character.len_utf8();
        self.offset += character.len_utf8();
        Some(wrap::Painted {
            start,
            end,
            cluster: &self.text[end - character.len_utf8()..end],
        })
    }
}

/// The cells from `from` to the end of the word there: up to a space or a line
/// break, sequences and controls costing nothing (`remainingWordWidth`,
/// `:844-860`).
///
/// With `fast`, a run of printable ASCII past the space -- a cell each, and
/// none of them a space or a break -- is counted at once
/// ([`wrap::single_cells`]): the last word of a line can be megabytes.
fn remaining_word_width(text: &str, from: usize, fast: bool) -> usize {
    let mut width = 0usize;
    let mut cursor = from;
    loop {
        if fast {
            let run = wrap::single_cells(text, cursor, usize::MAX);
            width += run;
            cursor += run;
        }
        let Some(painted) = wrap::painting_from(text, cursor).next() else {
            break;
        };
        let cluster = painted.cluster;
        if cluster == " " || wrap::is_line_break(cluster) || cluster.starts_with('\r') {
            break;
        }
        if cluster != "\t" {
            width += usize::from(wrap::cluster_width(cluster));
        }
        cursor = painted.end;
    }
    width
}

/// Whether the word at `from` is the last one before the line ends
/// (`isFinalWordOnLine`, `:862-883`).
///
/// With `fast`, a run of printable ASCII past the space before the first space
/// is stepped over at once ([`wrap::single_cells`]): none of it is a space or
/// a break.
fn is_final_word(text: &str, from: usize, fast: bool) -> bool {
    let mut passed = false;
    let mut cursor = from;
    loop {
        if fast && !passed {
            cursor += wrap::single_cells(text, cursor, usize::MAX);
        }
        let Some(painted) = wrap::painting_from(text, cursor).next() else {
            return true;
        };
        let cluster = painted.cluster;
        cursor = painted.end;
        if wrap::is_line_break(cluster) || cluster.starts_with('\r') {
            return true;
        }
        if cluster == " " {
            passed = true;
            continue;
        }
        if passed {
            return false;
        }
    }
}

/// The cells of the first unit of `text` that paints something, or `None`
/// when a control comes first (`firstVisibleWidth`, `:790-805`).
fn first_visible_width(text: &str) -> Option<u16> {
    for painted in wrap::painting(text) {
        if painted
            .cluster
            .as_bytes()
            .first()
            .is_some_and(|byte| *byte < 0x20)
        {
            return None;
        }
        let width = wrap::cluster_width(painted.cluster);
        if width > 0 {
            return Some(width);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// the presenter's markers (`:892-1154`)
// ---------------------------------------------------------------------------

/// The continuation of the line beginning at `line_start`, read off the
/// presenter's marker (`assistantContinuation`, `:894-918`): a list item's
/// (bullet, number, task), a definition's, a footnote's, a footnote's own
/// continuation, a quote's, or an indented paragraph's.
pub(crate) fn continuation_at(text: &str, line_start: usize) -> Option<Continuation> {
    let bytes = text.as_bytes();
    if let Some(indent) = list_indent(bytes, line_start) {
        return Some(Continuation::Indent(indent));
    }
    if definition_end(bytes, line_start).is_some() {
        return Some(Continuation::Indent(2));
    }
    if let Some(indent) = footnote_indent(bytes, line_start) {
        return Some(Continuation::Indent(indent));
    }
    if let Some(indent) = footnote_continuation(bytes, line_start) {
        return Some(Continuation::Indent(indent));
    }
    let mut i = line_start;
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    let indent = i - line_start;
    let mut depth = 0usize;
    while let Some(end) = quote_marker_end(bytes, i) {
        depth += 1;
        i = end;
    }
    if depth > 0 {
        return Some(Continuation::Quote { indent, depth });
    }
    prose_indent(text, line_start).map(Continuation::Indent)
}

/// Whether `bytes` holds `needle` at `i`.
fn holds(bytes: &[u8], i: usize, needle: &str) -> bool {
    bytes
        .get(i..)
        .is_some_and(|rest| rest.starts_with(needle.as_bytes()))
}

fn byte_is(bytes: &[u8], i: usize, wanted: u8) -> bool {
    bytes.get(i) == Some(&wanted)
}

/// Past every pacer reassertion at `i` (`skipPacerDimReassertions`).
fn skip_reassertions(bytes: &[u8], mut i: usize) -> usize {
    while holds(bytes, i, REASSERT) {
        i += REASSERT.len();
    }
    i
}

/// An indented paragraph's indent: the blanks it begins with, through colour
/// and link sequences, when text follows them (`proseIndent`, `:920-949`).
fn prose_indent(text: &str, line_start: usize) -> Option<usize> {
    let line_end = text[line_start..]
        .find(['\r', '\n'])
        .map_or(text.len(), |at| line_start + at);
    let line = &text[..line_end];
    let bytes = line.as_bytes();
    let mut i = line_start;
    let mut width = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b' ' => {
                width += 1;
                i += 1;
            }
            0x1b => {
                let len = super::pacer::escape_at(&line[i..]).unwrap_or(1);
                let sequence = &line[i..i + len];
                if sequence.len() < 3 {
                    return None;
                }
                let sgr = sequence.starts_with("\u{1b}[") && sequence.ends_with('m');
                if !sgr && !is_osc8(sequence) {
                    return None;
                }
                i += len;
            }
            0..=26 | 28..=31 | 127 => return None,
            _ => return (width > 0).then_some(width),
        }
    }
    None
}

/// Whether `sequence` is a whole OSC 8 open or close (`osc8Update`,
/// `:777-788`): `ESC ] 8 ;`, parameters, `;`, a target, and `BEL` or `ST`.
fn is_osc8(sequence: &str) -> bool {
    let Some(body) = sequence.strip_prefix("\u{1b}]8;") else {
        return false;
    };
    let payload = body
        .strip_suffix('\u{7}')
        .or_else(|| body.strip_suffix("\u{1b}\\"));
    payload.is_some_and(|payload| payload.contains(';'))
}

/// The end of a definition's dim two-blank lead (`definitionPrefixEnd`,
/// `:951-967`).
fn definition_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !holds(bytes, start, DIM_OPEN) {
        return None;
    }
    let mut i = skip_reassertions(bytes, start + DIM_OPEN.len());
    for _ in 0..2 {
        if !byte_is(bytes, i, b' ') {
            return None;
        }
        i = skip_reassertions(bytes, i + 1);
    }
    holds(bytes, i, DIM_CLOSE).then_some(i + DIM_CLOSE.len())
}

/// A footnote marker's continuation indent, `[n] ` dim (`footnotePrefix`,
/// `:974-994`).
fn footnote_indent(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = skip_reassertions(bytes, footnote_dim_open_end(bytes, start)?);
    if !byte_is(bytes, i, b'[') {
        return None;
    }
    i = skip_reassertions(bytes, i + 1);
    let mut digits = 0usize;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        digits += 1;
        i = skip_reassertions(bytes, i + 1);
    }
    if digits == 0 || !byte_is(bytes, i, b']') {
        return None;
    }
    i = skip_reassertions(bytes, i + 1);
    if !byte_is(bytes, i, b' ') {
        return None;
    }
    footnote_boundary_end(bytes, i + 1)?;
    Some(digits + 3)
}

/// A footnote body's own continuation row: four or more dim blanks
/// (`footnoteContinuationIndent`, `:996-1009`).
fn footnote_continuation(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = footnote_dim_open_end(bytes, start)?;
    let mut indent = 0usize;
    loop {
        i = skip_reassertions(bytes, i);
        if !byte_is(bytes, i, b' ') {
            break;
        }
        indent += 1;
        i += 1;
    }
    if indent < 4 {
        return None;
    }
    footnote_boundary_end(bytes, i)?;
    Some(indent)
}

/// `footnotePrefixBoundaryEnd` (`:1011-1016`).
fn footnote_boundary_end(bytes: &[u8], start: usize) -> Option<usize> {
    if holds(bytes, start, DIM_CLOSE) {
        return Some(start + DIM_CLOSE.len());
    }
    if holds(bytes, start, RESET) {
        return Some(start + RESET.len());
    }
    holds(bytes, start, REASSERT).then_some(start + REASSERT.len())
}

/// `footnoteDimOpenEnd` (`:1018-1027`).
fn footnote_dim_open_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    if holds(bytes, i, RESET) {
        i += RESET.len();
    }
    if !holds(bytes, i, DIM_OPEN) {
        return None;
    }
    while holds(bytes, i, DIM_OPEN) {
        i += DIM_OPEN.len();
    }
    Some(i)
}

/// The end of one dim `│ ` quote rule (`blockquoteMarkerEnd`, `:1035-1052`).
fn quote_marker_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !holds(bytes, start, DIM_OPEN) {
        return None;
    }
    let mut i = skip_reassertions(bytes, start + DIM_OPEN.len());
    if !holds(bytes, i, RULE) {
        return None;
    }
    i = skip_reassertions(bytes, i + RULE.len());
    if !byte_is(bytes, i, b' ') {
        return None;
    }
    i = skip_reassertions(bytes, i + 1);
    holds(bytes, i, DIM_CLOSE).then_some(i + DIM_CLOSE.len())
}

/// A list item's continuation indent: its own indent, and the bullet, the
/// number or the task box with the space after it
/// (`listContinuationIndentWidth`, `:1055-1098`).
fn list_indent(bytes: &[u8], line_start: usize) -> Option<usize> {
    let mut i = line_start;
    while byte_is(bytes, i, b' ') {
        i += 1;
    }
    let indent = i - line_start;
    if let Some(task) = task_marker_width(bytes, i) {
        return Some(indent + task);
    }
    if !holds(bytes, i, DIM_OPEN) {
        return None;
    }
    i = skip_reassertions(bytes, i + DIM_OPEN.len());
    if let Some(task) = dim_task_marker_width(bytes, i) {
        return Some(indent + task);
    }
    if holds(bytes, i, BULLET) {
        i = skip_reassertions(bytes, i + BULLET.len());
        if !byte_is(bytes, i, b' ') {
            return None;
        }
        i = skip_reassertions(bytes, i + 1);
        return holds(bytes, i, DIM_CLOSE).then_some(indent + 2);
    }
    let mut marker = 0usize;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        marker += 1;
        i = skip_reassertions(bytes, i + 1);
    }
    if marker == 0 || !(byte_is(bytes, i, b'.') || byte_is(bytes, i, b')')) {
        return None;
    }
    marker += 1;
    i = skip_reassertions(bytes, i + 1);
    if !holds(bytes, i, DIM_CLOSE) {
        return None;
    }
    i += DIM_CLOSE.len();
    if !byte_is(bytes, i, b' ') {
        return None;
    }
    i += 1;
    if let Some(task) = task_marker_width(bytes, i) {
        return Some(indent + marker + 1 + task);
    }
    Some(indent + marker + 1)
}

/// A pending task box inside a dim run (`dimTaskMarkerWidth`, `:1100-1117`).
fn dim_task_marker_width(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = skip_reassertions(bytes, start);
    if !holds(bytes, i, PENDING) {
        return None;
    }
    i = skip_reassertions(bytes, i + PENDING.len());
    let mut width = 1usize;
    if byte_is(bytes, i, b' ') {
        width += 1;
        i = skip_reassertions(bytes, i + 1);
    }
    holds(bytes, i, DIM_CLOSE).then_some(width)
}

/// A task box, pending or completed (`taskMarkerWidth`, `:1119-1147`).
fn task_marker_width(bytes: &[u8], start: usize) -> Option<usize> {
    if holds(bytes, start, DIM_OPEN) {
        return dim_task_marker_width(bytes, start + DIM_OPEN.len());
    }
    let open = COMPLETED_OPENS
        .iter()
        .find(|open| holds(bytes, start, open))?;
    let mut i = start + open.len();
    if !holds(bytes, i, COMPLETED) {
        return None;
    }
    i += COMPLETED.len();
    if !holds(bytes, i, COMPLETED_CLOSE) {
        return None;
    }
    i += COMPLETED_CLOSE.len();
    Some(if byte_is(bytes, i, b' ') { 2 } else { 1 })
}

#[cfg(test)]
mod tests {
    //! Upstream's own cases (`assistant_wrap.zig:1156-1767`), each read as
    //! the rows it makes: a row is its lead and its text with the sequences
    //! taken out and the spaces it ends on dropped -- upstream's output drops
    //! a break's spaces and xfx hangs them, which is the same blank cells
    //! (`super::super::transcript`'s `body_rows`). The bytes the transcript
    //! writes around a break are pinned there.

    use super::*;

    /// `text` with every escape sequence taken out.
    fn bare(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(character) = rest.chars().next() {
            if let Some(len) = super::super::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            out.push(character);
            rest = &rest[character.len_utf8()..];
        }
        out
    }

    /// The rows `text` makes at `cols` with no gutter, as a reader sees them.
    fn view(text: &str, cols: u16) -> Vec<String> {
        shown(text, &prose(text, cols, 0, Opening::Line))
    }

    fn shown(text: &str, rows: &[ProseRow]) -> Vec<String> {
        rows.iter()
            .map(|laid| {
                let lead = laid.lead.map(Continuation::lead).unwrap_or_default();
                let body = &text[laid.row.start..laid.row.end];
                format!("{}{}", bare(&lead), bare(body))
                    .trim_end_matches([' ', '\n'])
                    .to_string()
            })
            .collect()
    }

    /// Upstream's expected output, read the same way.
    fn expected(output: &str) -> Vec<String> {
        output
            .split('\n')
            .map(|row| bare(row).trim_end().to_string())
            .collect()
    }

    fn ranges<'a>(text: &'a str, rows: &[ProseRow]) -> Vec<&'a str> {
        rows.iter()
            .map(|laid| &text[laid.row.start..laid.row.end])
            .collect()
    }

    // -- no orphan ---------------------------------------------------------

    #[test]
    fn a_line_final_word_takes_the_word_before_it_down_when_the_space_crosses() {
        // `:219-247`, upstream's "keeps paragraph words together and avoids a
        // final orphan": the row is full at `gamma`, the space after it
        // crosses, and `delta` is the line's last word -- so `gamma` goes too.
        assert_eq!(
            view("alpha beta gamma delta", 16),
            expected("alpha beta\ngamma delta")
        );
        // The composer's wrap leaves `delta` alone.
        let text = "alpha beta gamma delta";
        assert_eq!(
            wrap::wrap(text, 16)
                .iter()
                .map(|row| text[row.start..row.end].trim_end())
                .collect::<Vec<_>>(),
            ["alpha beta gamma", "delta"]
        );
    }

    #[test]
    fn a_line_final_word_takes_the_word_before_it_down_when_it_crosses() {
        // `:269-298`, "avoids an orphan after a fitting separator": `dddd`
        // crosses on its second letter and is the line's last word, and two
        // words stay behind, so the break moves back one.
        assert_eq!(
            view("aaaa bbbb cccc dddd", 16),
            expected("aaaa bbbb\ncccc dddd")
        );
        // D2c's line, at 120 columns behind the answer's two: upstream's
        // `... a standard or` / `lower tier.`.
        let ultra = "Ultrafast was requested, but Gateway did not confirm it was served; \
                     this response may have used a standard or lower tier.";
        assert_eq!(
            view(ultra, 118),
            [
                "Ultrafast was requested, but Gateway did not confirm it was served; \
                 this response may have used a standard or",
                "lower tier.",
            ]
        );
    }

    #[test]
    fn the_word_before_stays_when_the_two_would_not_fit_a_row() {
        // `wordBreakFits` (`:830-842`), at the space: `bbbbbbbbbbb cccccc`
        // is seventeen cells.
        assert_eq!(
            view("aaaa bbbbbbbbbbb cccccc", 16),
            ["aaaa bbbbbbbbbbb", "cccccc"]
        );
        // And at the word: `cccccccc dddddddd` is seventeen too.
        assert_eq!(
            view("a bb cccccccc dddddddd", 16),
            ["a bb cccccccc", "dddddddd"]
        );
        // With room for the pair, it goes.
        assert_eq!(view("a bb cccccc dddddd", 16), ["a bb", "cccccc dddddd"]);
    }

    #[test]
    fn the_overflowing_word_moves_alone_when_one_word_would_stay_behind() {
        // At the word, the break moves back only past a break that has
        // another before it (`has_preceding_word`, `:273`): `aaa` would be
        // left on a row of its own, so `ccccccc` goes down alone.
        assert_eq!(view("aaa bbbbbb ccccccc", 16), ["aaa bbbbbb", "ccccccc"]);
    }

    #[test]
    fn a_word_that_is_not_its_lines_last_breaks_as_any_word_does() {
        // Mid-paragraph the rule is not asked: at the space, and at the word.
        assert_eq!(
            view("alpha beta gamma delta epsilon", 16),
            ["alpha beta gamma", "delta epsilon"]
        );
        assert_eq!(
            view("aaaa bbbb cccc dddd eeee", 16),
            ["aaaa bbbb cccc", "dddd eeee"]
        );
        // A line break ends a line: the word before it is its last.
        assert_eq!(
            view("aaaa bbbb cccc dddd\neeee", 16),
            ["aaaa bbbb", "cccc dddd", "eeee"]
        );
    }

    // -- spaces ------------------------------------------------------------

    #[test]
    fn the_space_that_crosses_is_the_break_and_the_rest_of_its_run_is_not() {
        // `:248-266`: the space that would cross ends the row and is not
        // carried down; a space of the same run after it is the next row's
        // first cell, as upstream's is.
        let text = "abcde   fgh";
        let rows = prose(text, 6, 0, Opening::Line);
        assert_eq!(ranges(text, &rows), ["abcde  ", " fgh"]);
        // The dropped space is measured past the margin, where the painter
        // cuts it, as the composer's hung spaces are.
        assert_eq!(rows[0].row.width, 7);
        assert_eq!(rows[1].row.width, 4);
        // A row that breaks at a word keeps its spaces, inside the margin, and
        // the next row begins at the word.
        let text = "alpha bravo charlie";
        assert_eq!(
            ranges(text, &prose(text, 8, 0, Opening::Line)),
            ["alpha ", "bravo ", "charlie"]
        );
    }

    #[test]
    fn the_rows_tile_the_text_with_no_gap_and_no_overlap() {
        // What the transcript's freeze and resize rest on, as for the
        // composer's wrap: every byte in exactly one row.
        for text in [
            "",
            "a",
            "alpha bravo charlie delta",
            "alpha     bravo",
            "abcdefghij",
            "a\nb\n",
            "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} ok",
            "   leading indent that wraps onto rows",
            "trailing   ",
            "\u{1b}[2m\u{2022} \u{1b}[22malpha beta gamma delta epsilon",
            "\u{1b}[2m\u{2502} \u{1b}[22m\u{1b}[2m\u{2502} \u{1b}[22mquoted words wrap here",
            "tab\tseparated\tvalues",
        ] {
            for cols in [1u16, 2, 4, 6, 8, 17, 80] {
                let rows = prose(text, cols, 2, Opening::Line);
                assert!(!rows.is_empty(), "{text:?} at {cols}");
                assert_eq!(rows[0].row.start, 0, "{text:?} at {cols}");
                assert_eq!(rows.last().expect("a row").row.end, text.len());
                for pair in rows.windows(2) {
                    assert_eq!(pair[0].row.end, pair[1].row.start, "{text:?} at {cols}");
                    assert!(pair[0].row.start < pair[0].row.end, "{text:?} at {cols}");
                }
            }
        }
    }

    // -- continuation rows -------------------------------------------------

    #[test]
    fn list_quote_definition_and_footnote_rows_continue_under_their_text() {
        // Upstream's cases, each its own expected output.
        let cases: &[(&str, u16, &str)] = &[
            (
                "1. Heading\n   alpha beta gamma delta\n\n  alpha beta gamma delta\nalpha beta gamma delta",
                16,
                "1. Heading\n   alpha beta\n   gamma delta\n\n  alpha beta\n  gamma delta\nalpha beta\ngamma delta",
            ),
            (
                "\u{1b}[2m\u{2022} \u{1b}[22malpha beta gamma delta",
                18,
                "\u{2022} alpha beta\n  gamma delta",
            ),
            (
                "\u{1b}[2m\u{2022} \u{1b}[22mabcdefghijkl",
                7,
                "\u{2022} abcde\n  fghij\n  kl",
            ),
            (
                "\u{1b}[2m12)\u{1b}[22m abcdefghij",
                10,
                "12) abcdef\n    ghij",
            ),
            (
                "  \u{1b}[2m12.\u{1b}[22m abcdefghij",
                10,
                "  12. abcd\n      efgh\n      ij",
            ),
            (
                "  \u{1b}[2m\u{2022} \u{1b}[22mabcdefgh",
                8,
                "  \u{2022} abcd\n    efgh",
            ),
            (
                "\u{1b}[2m  \u{1b}[22malpha beta gamma delta",
                14,
                "  alpha beta\n  gamma delta",
            ),
            (
                "\u{1b}[2m[1] \u{1b}[22mabcdefghijkl",
                8,
                "[1] abcd\n    efgh\n    ijkl",
            ),
            (
                "\u{1b}[2m[1] \u{1b}[0mabcdefghijkl",
                8,
                "[1] abcd\n    efgh\n    ijkl",
            ),
            (
                "\u{1b}[2m\u{2502} \u{1b}[22m\u{1b}[2m\u{2502} \u{1b}[22m\u{1b}[1malpha beta gamma delta\u{1b}[22m",
                18,
                "\u{2502} \u{2502} alpha beta\n\u{2502} \u{2502} gamma delta",
            ),
            (
                "\u{1b}[2m\u{2610} \u{1b}[22malpha beta gamma delta",
                18,
                "\u{2610} alpha beta\n  gamma delta",
            ),
            (
                "\u{1b}[38;5;252m\u{2713}\u{1b}[39m alpha beta gamma delta",
                18,
                "\u{2713} alpha beta\n  gamma delta",
            ),
            (
                "\u{1b}[2m12.\u{1b}[22m \u{1b}[2m\u{2610} \u{1b}[22malpha beta gamma",
                20,
                "12. \u{2610} alpha\n      beta gamma",
            ),
            (
                "  \u{1b}[38;5;252m\u{2713}\u{1b}[39m abcdefghij",
                10,
                "  \u{2713} abcdef\n    ghij",
            ),
            ("   abcdef", 3, "\nabc\ndef"),
        ];
        for (input, cols, output) in cases {
            assert_eq!(view(input, *cols), expected(output), "{input:?} at {cols}");
        }
    }

    #[test]
    fn an_indent_is_blanks_before_text_and_nothing_else() {
        // `paragraph indentation ignores blank incomplete and non-prose
        // prefixes` (`:1176-1182`).
        for text in [
            "   ",
            "   \nnext",
            "   \rnext",
            "  \ttext",
            "  \u{1b}[",
            "  \u{1b}[\ntext",
            "  \u{1b}]8;;unfinished",
            "  \u{1b}[Atext",
        ] {
            assert_eq!(prose_indent(text, 0), None, "{text:?}");
        }
        assert_eq!(prose_indent("\u{1b}[2m \u{1b}[0m  text", 0), Some(3));
        assert_eq!(
            prose_indent(" \u{1b}]8;;https://example.com\u{7} text", 0),
            Some(2)
        );
    }

    #[test]
    fn a_marker_the_pacer_spaced_apart_is_still_the_marker() {
        // Upstream's pacer writes `ESC[0m ESC[2m` between the characters of a
        // dim marker it paces; xfx's presenter does not, and the reading is
        // the same either way (`skipPacerDimReassertions`).
        let cases: &[(&str, u16, &str)] = &[
            (
                "\u{1b}[2m\u{2022}\u{1b}[0m\u{1b}[2m \u{1b}[0m\u{1b}[2m\u{1b}[22mabcdefghij",
                7,
                "\u{2022} abcde\n  fghij",
            ),
            (
                "\u{1b}[2m1\u{1b}[0m\u{1b}[2m2\u{1b}[0m\u{1b}[2m.\u{1b}[0m\u{1b}[2m\u{1b}[22m abcdefgh",
                10,
                "12. abcdef\n    gh",
            ),
            (
                "\u{1b}[2m\u{2502}\u{1b}[0m\u{1b}[2m \u{1b}[0m\u{1b}[2m\u{1b}[22mabcdefghij",
                7,
                "\u{2502} abcde\n\u{2502} fghij",
            ),
            (
                "\u{1b}[2m \u{1b}[0m\u{1b}[2m \u{1b}[0m\u{1b}[2m\u{1b}[22malpha beta gamma delta",
                14,
                "  alpha beta\n  gamma delta",
            ),
            (
                "\u{1b}[2m[1] \u{1b}[0m\u{1b}[2mabcdefghijkl\u{1b}[22m",
                8,
                "[1] abcd\n    efgh\n    ijkl",
            ),
        ];
        for (input, cols, output) in cases {
            assert_eq!(view(input, *cols), expected(output), "{input:?} at {cols}");
        }
        // Two paced quote rules repeat on every continuation row, dim.
        let paced = "\u{1b}[2m\u{2502}\u{1b}[0m\u{1b}[2m \u{1b}[0m\u{1b}[2m\u{1b}[22m";
        let text = format!("{paced}{paced}abcdefghijkl");
        let rows = prose(&text, 9, 0, Opening::Line);
        assert_eq!(
            shown(&text, &rows),
            [
                "\u{2502} \u{2502} abcde",
                "\u{2502} \u{2502} fghij",
                "\u{2502} \u{2502} kl"
            ]
        );
        assert_eq!(
            rows[1].lead.map(Continuation::lead).as_deref(),
            Some("\u{1b}[2m\u{2502} \u{1b}[22m\u{1b}[2m\u{2502} \u{1b}[22m")
        );
    }

    #[test]
    fn a_footnote_marker_after_prose_still_sets_its_rows() {
        // `wrapTranscriptAssistantText retains reset-closed footnote
        // indentation after prose` and `... through character-paced markers`,
        // behind the answer's two columns.
        let text = "Forward reference.\n\n\u{1b}[2m[2] \u{1b}[0mThis definition appears \
                    before its reference and ends with FOOTNOTE_DONE.";
        let rows = shown(text, &prose(text, 30, 2, Opening::Line));
        assert!(
            rows.contains(&"    before its reference and".to_string()),
            "{rows:?}"
        );
        let paced = "Forward reference\n\n\u{1b}[2m[\u{1b}[0m\u{1b}[2m1\u{1b}[0m\u{1b}[2m]\
                     \u{1b}[0m\u{1b}[2m \u{1b}[0m\u{1b}[2m\u{1b}[22mThe deferred body wraps \
                     through a narrow terminal.";
        let rows = shown(paced, &prose(paced, 30, 2, Opening::Line));
        assert!(
            rows.contains(&"    through a narrow terminal.".to_string()),
            "{rows:?}"
        );
    }

    #[test]
    fn the_marker_space_is_not_a_place_to_break() {
        // `isContinuationMarkerSeparator` (`:821-828`): breaking after `•`
        // would leave the bullet alone; the word breaks inside instead.
        assert_eq!(
            view("\u{1b}[2m\u{2022} \u{1b}[22mabcdefghijkl", 7),
            ["\u{2022} abcde", "  fghij", "  kl"]
        );
    }

    #[test]
    fn a_lead_that_would_leave_no_room_is_dropped() {
        // `:707`: the indent goes only where the row's first unit still fits
        // after it.
        let text = "  \u{754c}\u{754c}\u{754c}";
        let rows = prose(text, 4, 0, Opening::Line);
        assert_eq!(
            shown(text, &rows),
            ["  \u{754c}", "  \u{754c}", "  \u{754c}"]
        );
        let rows = prose(text, 3, 0, Opening::Line);
        assert!(rows[1..].iter().all(|row| row.lead.is_none()), "{rows:?}");
    }

    #[test]
    fn a_line_carried_past_a_freeze_keeps_its_continuation() {
        // The transcript's freeze drops a line's first rows; what it still
        // holds begins on a continuation row and keeps the line's indent.
        let rows = prose(
            "gamma delta",
            8,
            2,
            Opening::Continued(Some(Continuation::Indent(2))),
        );
        assert_eq!(shown("gamma delta", &rows), ["  gamma", "  delta"]);
        // Text that happens to begin with blanks is not an indent there.
        let rows = prose("   gamma", 20, 2, Opening::Continued(None));
        assert_eq!(rows[0].lead, None);
    }

    #[test]
    fn the_presenter_leaves_no_pen_or_link_open_across_a_line_break() {
        // Upstream's wrap carries the pen and an open link across a **line
        // break** too: it closes them at the row's end and reopens them on
        // the next line's first row (`:144-148`, `:318-321`, `:364-384`). The
        // transcript starts every line from a fresh pen instead
        // (`super::super::transcript`'s `body_rows`), which is the same
        // screen only while no line the presenter writes leaves one open. It
        // renders a line at a time (`crate::markdown`'s `process_line`), so a
        // style or a link written across a break is two lines of literal
        // text; this pins that for the shapes that could span one.
        use crate::markdown::{self, MarkdownProcessor};
        for source in [
            "**bold\nmore** after",
            "*italic\nmore* after",
            "~~strike\nmore~~ after",
            "`code\nmore` after",
            "[text\nmore](https://example.com/x) after",
            "[ok](https://example.com/y) **open\nclosed**",
            "> quote **bold\n> more** end",
            "- item *it\n  more* done",
            "### heading **bold\nnext line",
            "Term\n: definition **bold\nmore**",
            "See [^1].\n\n[^1]: note **bold\n    more** end",
            "<https://example.com/a>\nnext",
        ] {
            let mut processor = MarkdownProcessor::with_styles(markdown::ansi::Styles::new(
                markdown::theme::builtin(false),
            ));
            let mut events = processor.feed(source);
            events.extend(processor.finish());
            for event in events {
                let markdown::Event::Text(text) = event else {
                    continue;
                };
                for line in text.split('\n') {
                    let mut state = super::super::pacer::SgrState::default();
                    state.observe(line);
                    assert_eq!(
                        state.close(),
                        "",
                        "{source:?}: {line:?} leaves its pen open"
                    );
                }
            }
        }
    }

    // -- wide units --------------------------------------------------------

    #[test]
    fn a_wide_unit_never_straddles_the_margin() {
        // `keeps Unicode display units intact at row boundaries`: every unit
        // whole, every row inside three columns.
        for unit in [
            "\u{2600}\u{FE0E}",
            "\u{1F44D}\u{1F3FD}",
            "\u{1F1FA}\u{1F1F8}",
            "#\u{FE0F}\u{20E3}",
            "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
            "\u{1F469}\u{200D}\u{1F4BB}",
        ] {
            let text = format!("a{unit}b");
            let rows = prose(&text, 3, 0, Opening::Line);
            for row in &rows {
                assert!(row.row.width <= 3, "{text:?}: {rows:?}");
            }
            assert!(ranges(&text, &rows).iter().any(|row| row.contains(unit)));
        }
        // Narrow list and quote rows stay inside the screen (`keeps narrow
        // list rows within terminal width`, `... blockquote rows ...`): what a
        // row paints -- its lead and its text, short of the spaces it ends on,
        // which the painter cuts past the margin -- fits.
        for text in [
            "\u{1b}[2m\u{2022} \u{1b}[22m\u{6f22}\u{5b57}abcdef",
            "\u{1b}[2m\u{2502} \u{1b}[22m\u{1b}[2m\u{2502} \u{1b}[22m\u{6f22}\u{5b57}abcdef",
        ] {
            for cols in 2u16..=7 {
                for row in prose(text, cols, 0, Opening::Line) {
                    let painted =
                        wrap::width(text[row.row.start..row.row.end].trim_end_matches(' '));
                    assert!(
                        usize::from(painted) + row.lead.map_or(0, Continuation::width)
                            <= usize::from(cols),
                        "{text:?} at {cols}: {row:?}"
                    );
                }
            }
        }
    }

    // -- tabs --------------------------------------------------------------

    #[test]
    fn a_literal_tab_is_drawn_in_the_cells_it_was_measured_in_where_it_moves_down() {
        // `p5-transcript-safetab`'s first line at 20 columns behind a two-cell
        // rail: `b`'s tab is one cell where it is met (screen column 8 to 9);
        // the next word crosses, `b` and the tab move down, and the tab is
        // still the one space fx moved (`reflowAtWordBreak`,
        // `assistant_wrap.zig:729-775@481104fd`), not the five it would be
        // measured again from column 4.
        assert_eq!(
            literal_drawn("aaa b\t1234567890123", 18, 2),
            ["aaa ", "b 1234567890123"]
        );
        // A tab that does not move is the spaces to its stop, as before.
        assert_eq!(literal_drawn("a\tb", 78, 2), ["a     b"]);
        assert_eq!(literal_drawn("no tab", 78, 2), ["no tab"]);
    }

    #[test]
    fn a_tab_stops_every_eight_columns_of_the_screen() {
        // `:163-174`: from screen column 4 (the gutter's two, then `a`) the
        // next stop is 9, five cells on -- and a tab never wraps, it stops at
        // the last column.
        let rows = prose("a\tb", 78, 2, Opening::Line);
        assert_eq!(rows[0].row.width, 7);
        assert_eq!(expand_tabs("a\tb", 3, 80), "a     b");
        assert_eq!(expand_tabs("no tab", 3, 80), "no tab");
        assert_eq!(next_tab_stop(79, 80), 80);
        assert_eq!(next_tab_stop(80, 80), 80);
    }

    // -- literal -----------------------------------------------------------

    #[test]
    fn literal_output_keeps_the_rule_and_reads_no_markdown() {
        // `wrapLiteralToolOutput repeats the tool detail rail on physical
        // rows`: `alpha beta` / `gamma delta` behind the three-cell rail.
        let text = "alpha beta gamma delta";
        let rows = literal(text, 13, 3);
        assert_eq!(
            rows.iter()
                .map(|row| text[row.start..row.end].trim_end())
                .collect::<Vec<_>>(),
            ["alpha beta", "gamma delta"]
        );
        // A bullet is text there, not a marker to indent under: its space is
        // a break like any other, and no row is indented.
        let text = "\u{1b}[2m\u{2022} \u{1b}[22mabcdefghijkl";
        let rows = literal(text, 7, 2);
        assert_eq!(
            rows.iter()
                .map(|row| bare(&text[row.start..row.end]))
                .collect::<Vec<_>>(),
            ["\u{2022} ", "abcdefg", "hijkl"]
        );
    }

    #[test]
    fn a_pathological_run_of_marks_is_rowed_in_literal_output() {
        // `preserves and rows a pathological zero width run`: every mark kept,
        // and more than one row for them.
        let marks = ZERO_WIDTH_ROW_BYTES + 512;
        let text = format!("a{}", "\u{301}".repeat(marks));
        let rows = literal(&text, 80, 2);
        assert!(rows.len() > 1, "{} rows", rows.len());
        assert_eq!(
            rows.iter()
                .map(|row| text[row.start..row.end].matches('\u{301}').count())
                .sum::<usize>(),
            marks
        );
        for row in &rows {
            assert!(row.end - row.start <= ZERO_WIDTH_ROW_BYTES + 1, "{row:?}");
        }
    }

    #[test]
    fn stepping_over_what_the_wrap_only_counts_changes_no_row() {
        // ARGROW-CACHE: the wrap that steps over a run of printable ASCII and a
        // repeated cluster against the wrap that counts every unit, in both
        // forms, every opening, and widths from one column.
        let openings = [
            Opening::Line,
            Opening::Continued(None),
            Opening::Continued(Some(Continuation::Indent(2))),
            Opening::Continued(Some(Continuation::Quote {
                indent: 0,
                depth: 1,
            })),
        ];
        let mut rng = wrap::corpus::Rng(0xd1b5_4a32_d192_ed03);
        for _ in 0..1500 {
            let runs = 1 + rng.below(24);
            let mut text = rng.text(runs);
            if rng.below(4) == 0 {
                // A line's shape for an argument: a name, then words and a
                // long token.
                text = {
                    let tail = rng.below(90);
                    format!("pattern: {text} {}", "q".repeat(tail))
                };
            }
            for literal in [true, false] {
                let opening = if literal {
                    Opening::Line
                } else {
                    openings[rng.below(openings.len())]
                };
                for cols in [1u16, 2, 3, 5, 8, 13, 21, 34] {
                    let gutter = rng.below(4) as u16;
                    let fast = Layout::new(&text, cols, gutter, literal, opening).run_with_tabs();
                    let mut slow = Layout::new(&text, cols, gutter, literal, opening);
                    slow.fast = false;
                    assert_eq!(
                        fast,
                        slow.run_with_tabs(),
                        "{text:?} at {cols}, gutter {gutter}, literal {literal}, {opening:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wrap_of_megabytes_of_one_token_steps_a_row_at_a_time() {
        // The shapes Ctrl-O lays out an argument of megabytes in, once the
        // channel has spelled it, against the wrap that counts every unit:
        // one token, words before it, and one wide character repeated.
        for text in [
            format!("pattern: {}", r"\u{009b}".repeat(4000)),
            format!("pattern: a b {}", r"\u{261d}".repeat(3000)),
            format!("note: {}", "\u{65e5}".repeat(5000)),
            format!("note: {} tail", "w ".repeat(3000)),
        ] {
            for cols in [17u16, 97, 117] {
                let fast = Layout::new(&text, cols, 3, true, Opening::Line).run_with_tabs();
                let mut slow = Layout::new(&text, cols, 3, true, Opening::Line);
                slow.fast = false;
                assert_eq!(fast, slow.run_with_tabs(), "{} at {cols}", &text[..20]);
            }
        }
    }
}
