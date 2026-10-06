//! The inline menu that completes a slash command.
//!
//! It is the band's third elastic block, and it is deliberately **not** a
//! second approval panel. A question owns the focus, because the session is
//! waiting to be told what to do and every keystroke is an answer to it; a
//! completion menu is a *view of what the composer already holds*, so the
//! caret stays where the text is being typed and only the five keys the menu
//! binds are taken out of the stream ([`PickerAction::of`]). The two can
//! therefore never both be up: `super::shell`'s `ask` dismisses a menu before
//! it installs a question, and the band paints one block, not two.
//!
//! # What it looks like
//!
//! Upstream's slash popup (`vercel-labs/fx@c1db9191`,
//! `src/ui/footer/picker_presentation.zig:721-768` and `:1194-1410`), row for
//! row, between the two rules `super::shell` draws round it: a header saying
//! how many commands match and which of them are in view, a blank row, then one
//! row per command in the window -- two columns of indent, the name padded to
//! the widest name **in view**, the summary, and the category in a column that
//! ends one cell short of the right-hand edge. Rows the window has no command
//! for are blank, so the menu keeps its height while the query narrows it. The
//! mark is brightness alone: the marked row is bold in the hint grey, every
//! other row is dim, and there is no caret glyph.
//!
//! # How tall it is
//!
//! Upstream's budget (`inlinePickerRowBudget`, `:751-768`), not a count of the
//! matches: the rows a screen can spare after the band's fixed rows and five
//! rows of document, at most six commands plus the header and its blank row.
//! A budget of two rows or fewer has no header -- the commands are what the
//! rows are for -- and a budget is never below one row, which is what makes the
//! menu *unrefusable*: on the shortest screen a band fits on
//! ([`super::layout::MIN_ROWS`]) it is one row, and `super::layout::solve_band`
//! has an answer for that band even with a turn running.
//!
//! # What opens it, and what closes it
//!
//! A **trigger** is a property of the whole composer rather than of the last
//! keystroke: the draft is one word beginning with `/` and nothing else
//! ([`Trigger::of`]). That is the same shape `crate::interactive::classify`
//! calls a command, which is what keeps the menu from offering a completion for
//! a line that would not be run as one.
//!
//! Escape closes it, and the closing has to *stay* closed while the user keeps
//! typing the same word -- otherwise the next keystroke re-opens the thing they
//! just dismissed. What is remembered is the **trigger kind** and not the query
//! ([`Dismissed`]): `/he` dismissed and grown to `/hel` is still dismissed, and
//! a composer that stops being a slash word at all forgets the dismissal, so
//! typing `/` again opens a menu.
//!
//! # Ranking
//!
//! Upstream's three tiers (`src/core/slash_commands/command_specs.zig:520-566`),
//! in this order: a name that **is** the query, a name that **begins** with it,
//! then a name that merely **contains** it after the slash. Each command is
//! ranked by the best of its own name and its aliases, and the row shows the
//! one that matched -- `/exit` for the letters `ex`, not `/quit`. Within a tier
//! the order is `crate::interactive::SLASH_REGISTRY`'s, which is `/help`'s --
//! so the list is a pure function of the query, and two sessions that typed the
//! same letters see the same rows in the same order. The match is exact and
//! case-sensitive for the reason the parser's is: a surface that guessed would
//! be a surface whose answer depends on how it was feeling.

use crate::interactive::{SlashSpec, SLASH_REGISTRY};

use super::theme::Palette;

/// The most commands the menu shows at once (`list_window.zig:3`,
/// `default_max_picker_rows`).
///
/// Also the window the mark scrolls against when it moves
/// ([`Picker::apply`]), exactly as upstream's `advanceSelection` does; the
/// layout re-clamps that against the rows the screen really has.
const MOST_COMMAND_ROWS: u16 = 6;

/// The header and the blank row under it.
const HEADER_ROWS: u16 = 2;

/// The band's rows the menu cannot have (`inlinePickerRowBudgetCapped`'s
/// `fixed_rows`, `picker_presentation.zig:765`).
const FIXED_ROWS: u16 = 5;

/// The document rows the menu leaves on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:766`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// What every row is written into, so the names line up under each other.
///
/// Fixed, and the same for the marked row: the mark is brightness, so there is
/// no caret for the indent to make room for (`composeSlashMenuRow`'s
/// `marker_width`).
const INDENT: &str = "  ";

/// The columns between the name and the summary, and between the summary and
/// the category.
const COLUMN_GAP: usize = 3;

/// The least summary a row keeps before it gives up showing the category.
const MIN_SUMMARY_CELLS: usize = 24;

/// What the hint row says while the menu is up, widest first
/// (`src/ui/footer/input_presentation.zig:862-883`, `composeSlashMenuHintRow`).
///
/// The first one that fits the screen is the one shown, and the last is shown
/// -- cut by the painter -- when none does.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     enter use     esc close",
    "\u{2191}\u{2193} navigate  enter use  esc close",
    "\u{2191}\u{2193} move  enter  esc",
    "enter use  esc close",
    "enter esc",
];

/// What a cut row ends in.
const ELLIPSIS: &str = "\u{2026}";

/// How well a command answers a query.
///
/// Ordered by declaration, which is the ranking: `derive(PartialOrd, Ord)` on a
/// fieldless enum is its variant order, and the sort below is what turns that
/// into rows on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rank {
    /// The name -- the command's own or an alias -- is the query.
    Exact,
    /// It begins with the query.
    Prefix,
    /// It contains the query's letters somewhere after its slash.
    Substring,
}

/// What kind of completion the composer is asking for.
///
/// One variant, and it is an `enum` rather than a `bool` because
/// [`Dismissed`] remembers *which* kind was dismissed: a file or an at-mention
/// trigger added later must not inherit the dismissal of a slash one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// The draft is one word beginning with `/`.
    Slash,
}

impl Trigger {
    /// The completion `text` is asking for, if it is asking for one.
    ///
    /// The **whole** draft, not the word under the caret: a slash command is a
    /// line whose first character is `/` and whose first word is the name, so
    /// `say /help` is a prompt here exactly as it is to
    /// `crate::interactive::classify`, and `/model acme` has stopped being a
    /// name and become a name with an argument.
    pub(crate) fn of(text: &str) -> Option<Self> {
        (text.starts_with('/') && !text.contains(char::is_whitespace)).then_some(Self::Slash)
    }
}

/// The trigger whose menu the user closed, while it is still the trigger.
///
/// Held by the shell across keystrokes. The whole of its job is that a menu
/// dismissed by Escape does not come back on the next letter of the same word,
/// and that it *does* come back once the word it belonged to is gone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Dismissed(Option<Trigger>);

impl Dismissed {
    /// Records that the menu for `trigger` was closed by the user.
    pub(crate) fn dismiss(&mut self, trigger: Trigger) {
        self.0 = Some(trigger);
    }

    /// Whether a menu may be open for `trigger` now.
    ///
    /// **Reconciling rather than asking**, which is why it takes `&mut self`:
    /// a dismissal outlives only the trigger it was made against, so a call
    /// that finds a different trigger -- or none -- forgets it here rather than
    /// leaving a stale `Some` for some later keystroke to be confused by.
    pub(crate) fn admits(&mut self, trigger: Option<Trigger>) -> bool {
        if self.0.is_some() && self.0 == trigger {
            return false;
        }
        self.0 = None;
        trigger.is_some()
    }
}

/// One command the query found, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Match {
    spec: &'static SlashSpec,
    rank: Rank,
    /// The name the query matched: the command's own, or the alias that
    /// matched it better.
    ///
    /// What the row shows and what Tab and Enter put in the composer, because a
    /// menu that listed `/quit` for the letters `ex` without saying why would
    /// look like a bug -- and the alias says why by being the row.
    label: &'static str,
}

/// What one keystroke means to an open menu.
///
/// The menu's own vocabulary rather than [`super::input::Action`], for the
/// reason `super::approval::Action` is one: the shell translates, so a key the
/// menu does not bind cannot be swallowed by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerAction {
    /// The previous match.
    Up,
    /// The next one.
    Down,
    /// Put the marked name in the composer. Tab.
    Complete,
    /// Run the marked command. Enter.
    Submit,
    /// Close it and leave the draft alone. Escape.
    Escape,
}

impl PickerAction {
    /// What an ordinary decoded action means to a menu, and `None` for the
    /// keystrokes that go on meaning what they mean.
    pub(crate) fn of(action: super::input::Action) -> Option<Self> {
        match action {
            super::input::Action::Up => Some(Self::Up),
            super::input::Action::Down => Some(Self::Down),
            super::input::Action::Tab => Some(Self::Complete),
            super::input::Action::Submit => Some(Self::Submit),
            super::input::Action::Escape => Some(Self::Escape),
            _ => None,
        }
    }
}

/// What one keystroke did to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerOutcome {
    /// The marked row moved. A frame, and nothing else.
    Changed,
    /// This name is what the composer should hold, for the user to go on
    /// typing after it.
    Complete(&'static str),
    /// This name is the line Enter runs (`input_submit_runtime.zig:1087-1089`
    /// and `:1806-1811`): the menu is gone, and **the keystroke goes on** to
    /// submit it, so one Return runs the marked command.
    Run(&'static str),
    /// The menu is gone and the draft is untouched. Escape.
    Dismiss,
}

/// Which matches are in view: an index range into them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    start: usize,
    end: usize,
}

/// The menu's shape on one screen (`SlashMenuLayout`,
/// `picker_presentation.zig:721-749`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    /// Every row the menu takes, header included.
    rows: u16,
    /// The rows commands may be shown on.
    selectable: u16,
    /// Whether the header and its blank row are drawn.
    header: bool,
    /// The matches in view.
    window: Window,
}

/// How wide the name and category columns are, over the window.
#[derive(Debug, Clone, Copy)]
struct Columns {
    label: usize,
    category: usize,
}

/// The matches for one query, and which of them is marked.
#[derive(Debug)]
pub(crate) struct Picker {
    /// What produced [`Self::matches`], so a keystroke that did not change the
    /// draft's text does not rebuild the list and move the mark back to the
    /// top.
    query: String,
    /// The trigger this menu belongs to, for the dismissal it may become.
    trigger: Trigger,
    /// Never empty: [`Self::over`] is the only constructor and it refuses a
    /// query nothing matches, which is what makes the arithmetic below total.
    matches: Vec<Match>,
    /// An index into [`Self::matches`].
    selected: usize,
    /// The first match in view, as the last move left it.
    ///
    /// **State, not derived from the mark**, because upstream's window scrolls
    /// only when the mark reaches an edge (`list_window.zig:50-62`): walking
    /// down past the last row and back up one moves the mark and leaves the
    /// window where it was, rather than snapping the window back to the top.
    window_start: usize,
}

impl Picker {
    /// The menu `query` deserves, or `None` when it names nothing.
    pub(crate) fn open(query: &str) -> Option<Self> {
        Self::over(SLASH_REGISTRY, query)
    }

    /// The same, over a registry named by the caller.
    ///
    /// The seam the ranking is tested through: the live registry has no
    /// command whose name begins with an alias's first letters, so the whole
    /// three-tier order cannot be produced from it.
    fn over(registry: &'static [SlashSpec], query: &str) -> Option<Self> {
        let trigger = Trigger::of(query)?;
        let mut matches: Vec<Match> = registry
            .iter()
            .filter_map(|spec| found(spec, query))
            .collect();
        // Stable, so the tie inside a rank is the registry's order rather than
        // whatever the sort happened to do with it.
        matches.sort_by_key(|found| found.rank);
        (!matches.is_empty()).then(|| Self {
            query: query.to_string(),
            trigger,
            matches,
            selected: 0,
            window_start: 0,
        })
    }

    /// What this menu was built for.
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The trigger it belongs to.
    pub(crate) fn trigger(&self) -> Trigger {
        self.trigger
    }

    /// The menu's shape on a screen `terminal_rows` tall
    /// (`slashMenuLayoutForCounts`).
    ///
    /// Upstream's budget takes two more inputs, and both are zero here for a
    /// reason that is a proof rather than a guess:
    ///
    /// * `input_extra` is the composer's rows past its first, and a draft that
    ///   opens a menu is one row. A query that matches is no longer than the
    ///   name or alias it matches (every tier is that name, a prefix of it, or
    ///   a run of its letters), the longest is `/version`'s eight cells, and
    ///   the narrowest screen a band fits on gives the composer eighteen
    ///   ([`super::layout::MIN_COLS`] less the rail).
    /// * `banner_rows` is upstream's steering banner, which xfx does not have.
    fn layout(&self, terminal_rows: u16) -> Layout {
        let budget = row_budget(terminal_rows);
        let header = budget > HEADER_ROWS;
        let selectable = if header { budget - HEADER_ROWS } else { budget };
        let count = self.matches.len();
        let start = edge_start(self.window_start, count, self.selected, selectable);
        Layout {
            rows: budget,
            selectable,
            header,
            window: window_from(count, start, selectable),
        }
    }

    /// The menu's rows, top first, exactly as many as [`Self::height`] says.
    ///
    /// **One layout for both**, the same rule the approval panel is built on:
    /// the height the band solved its geometry from is this list's length, so a
    /// row painted without a row of geometry to put it on -- or a row of
    /// geometry with nothing painted into it -- is not a state this can reach.
    /// The two rules round it are the band's, not the menu's.
    pub(crate) fn rows(&self, cols: u16, terminal_rows: u16, palette: Palette) -> Vec<String> {
        let layout = self.layout(terminal_rows);
        let mut rows = Vec::with_capacity(usize::from(layout.rows));
        if layout.header {
            rows.push(self.header(&layout, cols, palette));
            rows.push(String::new());
        }
        let in_view = &self.matches[layout.window.start..layout.window.end];
        // Measured over the window, not over every match: the name column is
        // as wide as the names **on the screen**, so it moves as the window
        // scrolls and as the query narrows it.
        let columns = Columns {
            label: widest(in_view.iter().map(|found| found.label)),
            category: widest(in_view.iter().map(|found| found.spec.category)),
        };
        for (offset, found) in in_view.iter().enumerate() {
            let marked = layout.window.start + offset == self.selected;
            rows.push(item_row(found, marked, columns, cols, palette));
        }
        // The rows the window has no command for: blank, so a query that
        // narrowed the list leaves the menu the height it was.
        rows.resize(usize::from(layout.rows), String::new());
        rows
    }

    /// How many rows the band has to give it.
    pub(crate) fn height(&self, terminal_rows: u16) -> u16 {
        self.layout(terminal_rows).rows
    }

    /// What one keystroke does to it.
    pub(crate) fn apply(&mut self, action: PickerAction) -> PickerOutcome {
        match action {
            PickerAction::Up => {
                self.advance(false);
                PickerOutcome::Changed
            }
            PickerAction::Down => {
                self.advance(true);
                PickerOutcome::Changed
            }
            PickerAction::Complete => PickerOutcome::Complete(self.matches[self.selected].label),
            PickerAction::Submit => PickerOutcome::Run(self.matches[self.selected].label),
            PickerAction::Escape => PickerOutcome::Dismiss,
        }
    }

    /// Moves the mark one row, wrapping at both ends, and scrolls the window
    /// only as far as keeps the mark in view (`list_window.zig:40-48`,
    /// `advanceSelection`).
    fn advance(&mut self, down: bool) {
        let count = self.matches.len();
        self.selected = if down {
            (self.selected + 1) % count
        } else {
            (self.selected + count - 1) % count
        };
        self.window_start = edge_start(self.window_start, count, self.selected, MOST_COMMAND_ROWS);
    }

    /// How many commands match, and which of them are in view
    /// (`composeSlashMenuHeaderRow`, `picker_presentation.zig:1194-1232`).
    ///
    /// `· type to filter` while the query is a bare slash; the range, in
    /// upstream's en dash, only while there are more matches than rows, and
    /// ending one cell short of the right-hand edge like every row of the menu.
    fn header(&self, layout: &Layout, cols: u16, palette: Palette) -> String {
        let count = self.matches.len();
        let left = if self.query == "/" {
            format!("Commands {count} \u{b7} type to filter")
        } else {
            format!("Commands {count}")
        };
        let range = if count > usize::from(layout.selectable) {
            format!("{}\u{2013}{}", layout.window.start + 1, layout.window.end)
        } else {
            String::new()
        };
        let content = usize::from(cols).saturating_sub(1);
        let range_cells = cells(&range);
        let room = range_cells > 0 && content > range_cells + 2;
        let left_cells = if room {
            content - range_cells - 2
        } else {
            content
        };
        let mut row = String::from(palette.dim());
        row.push_str(&ellipsized(&left, left_cells));
        if room {
            pad_to(&mut row, content - range_cells);
            row.push_str(&range);
        }
        row.push_str(palette.reset());
        row
    }
}

/// One command's row (`composeSlashMenuRow`, `picker_presentation.zig:1351-1404`).
///
/// The name in the row's own style -- bold hint grey when marked, dim
/// otherwise -- padded to the name column, then the summary and the category
/// in the dim grey. The marked row's bold carries through the summary and the
/// category, because only the colour is switched for them.
///
/// The category is the first thing a narrow screen gives up: it is shown only
/// while the summary keeps [`MIN_SUMMARY_CELLS`] beside it, and a summary too
/// long for its room is cut with an ellipsis rather than run into it.
fn item_row(found: &Match, marked: bool, columns: Columns, cols: u16, palette: Palette) -> String {
    let style = if marked {
        // Upstream's `selected_completion_style`, which is the same bold hint
        // grey as the welcome line's brand in both modes.
        palette.subtitle()
    } else {
        palette.dim()
    };
    let mut row = format!("{style}{INDENT}");
    let content = usize::from(cols).saturating_sub(1);
    let indent = INDENT.len();
    if content <= indent {
        row.push_str(palette.reset());
        return row;
    }
    row.push_str(&ellipsized(found.label, content - indent));

    let summary_start = content.min(indent + columns.label.max(cells(found.label)) + COLUMN_GAP);
    pad_to(&mut row, summary_start);

    let summary = found.spec.summary;
    let category_start = content.saturating_sub(columns.category);
    let category_fits =
        !found.spec.category.is_empty() && columns.category > 0 && columns.category <= content;
    let category_shown = if summary.is_empty() {
        category_fits && category_start >= summary_start + COLUMN_GAP
    } else {
        category_fits && category_start >= summary_start + MIN_SUMMARY_CELLS + COLUMN_GAP
    };
    let summary_cells = if category_shown {
        category_start - summary_start - COLUMN_GAP
    } else {
        content - summary_start
    };
    if !summary.is_empty() && summary_cells > 0 {
        row.push_str(palette.dim());
        row.push_str(&ellipsized(summary, summary_cells));
    }
    if category_shown {
        pad_to(&mut row, category_start);
        row.push_str(palette.dim());
        row.push_str(found.spec.category);
    }
    row.push_str(palette.reset());
    row
}

/// What the hint row says while a menu is up, on a screen `cols` wide.
///
/// The widest of [`KEY_HELP`] that fits, and the narrowest when none does.
pub(crate) fn key_help(cols: u16) -> &'static str {
    KEY_HELP
        .iter()
        .copied()
        .find(|candidate| cells(candidate) <= usize::from(cols))
        .unwrap_or(KEY_HELP[KEY_HELP.len() - 1])
}

/// The rows a screen `terminal_rows` tall gives the menu
/// (`inlinePickerRowBudget`, `picker_presentation.zig:751-768`).
///
/// What is left after the band's fixed rows and [`MINIMUM_TRANSCRIPT_ROWS`] of
/// document, at most [`MOST_COMMAND_ROWS`] plus the header, and **never
/// none**: a screen too short for the reserve still gets one row, because a
/// completion can always show fewer matches -- it never has to be refused.
fn row_budget(terminal_rows: u16) -> u16 {
    terminal_rows
        .saturating_sub(FIXED_ROWS)
        .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
        .clamp(1, MOST_COMMAND_ROWS + HEADER_ROWS)
}

/// Where a window of `rows` over `count` matches starts once `selected` is
/// kept in it, moving from `current` only as far as it has to
/// (`list_window.zig:50-62`, `updateEdgeStart`).
pub(super) fn edge_start(current: usize, count: usize, selected: usize, rows: u16) -> usize {
    let rows = usize::from(rows);
    if count == 0 || rows == 0 || count <= rows {
        return 0;
    }
    let selected = selected.min(count - 1);
    let start = current.min(count - rows);
    if selected < start {
        selected
    } else if selected >= start + rows {
        selected + 1 - rows
    } else {
        start
    }
}

/// The window of `rows` over `count` matches that begins at `start`, pulled
/// back so it does not run past the end (`list_window.zig:29-36`,
/// `edgeFromStart`).
fn window_from(count: usize, start: usize, rows: u16) -> Window {
    let rows = usize::from(rows);
    if count == 0 || rows == 0 {
        return Window { start: 0, end: 0 };
    }
    if count <= rows {
        return Window {
            start: 0,
            end: count,
        };
    }
    let start = start.min(count - rows);
    Window {
        start,
        end: start + rows,
    }
}

/// The widest of `texts`, in cells.
fn widest<'a>(texts: impl Iterator<Item = &'a str>) -> usize {
    texts.map(cells).max().unwrap_or(0)
}

/// How many cells `text` takes, by the measure the painter cuts with.
pub(super) fn cells(text: &str) -> usize {
    usize::from(super::wrap::width(text))
}

/// Spaces onto `row` until it is `target` cells wide, colours not counted
/// (`appendSpacesToVisibleWidth`, `picker_presentation.zig:1406-1409`).
pub(super) fn pad_to(row: &mut String, target: usize) {
    let mut width = cells(row);
    while width < target {
        row.push(' ');
        width += 1;
    }
}

/// `text` in at most `budget` cells, its tail swapped for an ellipsis when it
/// does not fit (`appendSingleLineEllipsized`, `row_text.zig:60-101`).
pub(super) fn ellipsized(text: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    if cells(text) <= budget {
        return text.to_string();
    }
    if budget == 1 {
        return ELLIPSIS.to_string();
    }
    // Below the text's own width here, which is itself a `u16`, so the
    // narrowing cannot fail.
    let keep = u16::try_from(budget - 1).unwrap_or(u16::MAX);
    format!("{}{ELLIPSIS}", super::frame::clip(text, keep))
}

/// Whether `spec` answers `query`, and how well (`bestCompletionCommandMatch`,
/// `command_specs.zig:528-546`).
///
/// The command's own name first and then each alias, and an alias wins only by
/// ranking **strictly** better -- so a command is on the list once however many
/// of its names the query could be read as, and under its own name whenever
/// that is as good an answer.
fn found(spec: &'static SlashSpec, query: &str) -> Option<Match> {
    let mut best = rank(spec.name, query).map(|rank| Match {
        spec,
        rank,
        label: spec.name,
    });
    for alias in spec.aliases {
        let Some(rank) = rank(alias, query) else {
            continue;
        };
        if best.is_none_or(|current| rank < current.rank) {
            best = Some(Match {
                spec,
                rank,
                label: alias,
            });
        }
    }
    best
}

/// How well one name answers `query` (`completionCommandMatchRank`,
/// `command_specs.zig:520-526`).
///
/// The slash is what makes the draft a command rather than a word, so it is
/// not part of what a *substring* is looked for in: `/e` is the letter `e`
/// somewhere after a name's slash, and a bare `/` -- which every name begins
/// with anyway -- looks for nothing.
fn rank(name: &str, query: &str) -> Option<Rank> {
    if name == query {
        return Some(Rank::Exact);
    }
    if name.starts_with(query) {
        return Some(Rank::Prefix);
    }
    let stem = query.get(1..).filter(|stem| !stem.is_empty())?;
    name.get(1..)
        .is_some_and(|rest| rest.contains(stem))
        .then_some(Rank::Substring)
}

/// What completing `name` puts in the composer.
///
/// A command that takes an argument is completed **with the space**, because
/// the next thing the user types is the argument and a menu that made them
/// press the space bar would have done half a job. One that takes none is
/// completed exactly, so Enter runs it. An alias takes an argument when its
/// command does (`slashCompletionHasArgs`, `command_specs.zig:674-685`).
pub(crate) fn completed(name: &str) -> String {
    if SLASH_REGISTRY
        .iter()
        .any(|spec| spec.has_args && (spec.name == name || spec.aliases.contains(&name)))
    {
        return format!("{name} ");
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interactive::{Slash, SlashSpec, SLASH_REGISTRY};

    use super::super::theme::{Depth, Mode};

    /// The palette every row below is painted in: an undecided terminal's.
    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// What the marked row opens with: upstream's `selected_completion_style`
    /// (`core/shared/theme.zig:97`).
    const MARKED: &str = "\u{1b}[1;38;5;255m";

    /// What every other row, and the header, opens with: `dim_style`
    /// (`core/shared/theme.zig:85`).
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    /// A registry that produces all three ranks for one query.
    ///
    /// The live one cannot: no canonical name is `/ex` or begins with it, so
    /// `/ex` has no exact or prefix name to rank beside `/exit`. Rather than
    /// assert a third of the order and call it the order, the ranking is asked
    /// the whole question here and the **live** alias path is asserted
    /// separately, below, against `SLASH_REGISTRY` itself.
    static THREE_TIERS: &[SlashSpec] = &[
        named("/annex"),
        SlashSpec {
            aliases: &["/exit"],
            ..named("/settle")
        },
        named("/ex"),
        named("/exercise"),
    ];

    /// A registry whose widest name scrolls out of view.
    static SCROLLING: &[SlashSpec] = &[
        named("/a"),
        named("/the-widest"),
        named("/b"),
        named("/c"),
        named("/d"),
        named("/e"),
        named("/f"),
        named("/g"),
    ];

    /// A row of a test registry: a name and nothing else worth reading.
    const fn named(name: &'static str) -> SlashSpec {
        SlashSpec {
            command: Slash::Help,
            name,
            aliases: &[],
            summary: "a summary",
            category: "General",
            has_args: false,
        }
    }

    fn labels(picker: &Picker) -> Vec<&'static str> {
        picker.matches.iter().map(|found| found.label).collect()
    }

    fn ranks(picker: &Picker) -> Vec<Rank> {
        picker.matches.iter().map(|found| found.rank).collect()
    }

    /// The text of a row with its colours taken out.
    fn plain(row: &str) -> String {
        let mut out = String::new();
        let mut rest = row;
        while !rest.is_empty() {
            if let Some(len) = super::super::pacer::escape_at(rest) {
                rest = &rest[len..];
                continue;
            }
            let mut characters = rest.chars();
            out.extend(characters.next());
            rest = characters.as_str();
        }
        out
    }

    /// The column a command row's summary starts in: past the indent, the
    /// name, and the spaces after it.
    fn summary_column(row: &str) -> usize {
        let text = plain(row);
        let after_indent = &text[INDENT.len()..];
        let name = after_indent.find(' ').expect("a gap after the name");
        let rest = &after_indent[name..];
        INDENT.len() + name + (rest.len() - rest.trim_start().len())
    }

    /// A picker on the live registry with the mark moved `downs` rows.
    fn walked(query: &str, downs: usize) -> Picker {
        let mut picker = Picker::open(query).expect("a query that matches");
        for _ in 0..downs {
            assert_eq!(picker.apply(PickerAction::Down), PickerOutcome::Changed);
        }
        picker
    }

    #[test]
    fn an_exact_name_ranks_before_a_prefix_before_a_substring() {
        let picker = Picker::over(THREE_TIERS, "/ex").expect("four matches");
        // The prefix tier is the alias and the name together, in registry
        // order: an alias is not a lesser kind of name.
        assert_eq!(labels(&picker), vec!["/ex", "/exit", "/exercise", "/annex"]);
        assert_eq!(
            ranks(&picker),
            vec![Rank::Exact, Rank::Prefix, Rank::Prefix, Rank::Substring]
        );

        // The live alias, through the same function: `/e` names no command, so
        // `/quit` is on the screen **as** `/exit` because `/exit` reaches it --
        // above every command that merely has an `e` after its slash.
        let live = Picker::open("/e").expect("the alias and the substrings");
        assert_eq!(live.matches[0].spec.name, "/quit");
        assert_eq!(live.matches[0].label, "/exit");
        assert_eq!(live.matches[0].rank, Rank::Prefix);
        assert!(
            live.matches[1..]
                .iter()
                .all(|found| found.rank == Rank::Substring),
            "{:?}",
            ranks(&live)
        );
        // Deterministic within a rank: registry order, which is `/help`'s.
        assert_eq!(
            labels(&live)[1..],
            ["/help", "/clear", "/new", "/setup", "/model", "/version"]
        );
        // The slash is not a letter of any name: a second one finds nothing.
        assert!(Picker::open("//").is_none());
    }

    #[test]
    fn dismissal_survives_query_growth_until_the_trigger_kind_changes() {
        let mut dismissed = Dismissed::default();
        assert!(dismissed.admits(Trigger::of("/he")));
        dismissed.dismiss(Trigger::Slash);
        // The query grows; the dismissal is about the trigger, not the text.
        assert!(!dismissed.admits(Trigger::of("/hel")));
        assert!(!dismissed.admits(Trigger::of("/help")));
        // The trigger goes away, and with it the dismissal.
        assert!(!dismissed.admits(Trigger::of("")));
        assert!(dismissed.admits(Trigger::of("/h")));
    }

    #[test]
    fn picker_rows_and_height_come_from_one_layout() {
        for (rows, cols) in [(24_u16, 80_u16), (7, 20), (40, 200), (10, 24), (13, 30)] {
            for query in ["/", "/e", "/q", "/exit"] {
                let picker = Picker::open(query).expect("a query that matches");
                let painted = picker.rows(cols, rows, PALETTE);
                assert_eq!(
                    usize::from(picker.height(rows)),
                    painted.len(),
                    "{query} on {rows}x{cols}"
                );
                // Every row ends a cell short of the edge, as upstream's do.
                assert!(
                    painted
                        .iter()
                        .all(|row| super::super::wrap::width(row) < cols),
                    "{query} on {rows}x{cols}: {painted:?}"
                );
            }
        }
    }

    #[test]
    fn the_budget_is_upstreams_and_never_refuses_a_menu() {
        // `inlinePickerRowBudget`: five fixed rows and five rows of document
        // off the screen, at most six commands and the header's two rows, at
        // least one row. Spelled out rather than recomputed from the
        // constants, so a constant that drifted fails here.
        for (rows, budget) in [
            (7, 1),
            (10, 1),
            (11, 1),
            (12, 2),
            (13, 3),
            (17, 7),
            (18, 8),
            (24, 8),
            (40, 8),
        ] {
            assert_eq!(row_budget(rows), budget, "{rows} rows");
        }
        let picker = Picker::open("/").expect("every command");
        assert_eq!(picker.height(40), 8, "header, blank, six commands");
        assert_eq!(picker.height(12), 2, "two commands and no header");
        assert_eq!(picker.height(7), 1, "the shortest screen still has one");
    }

    #[test]
    fn a_bare_slash_has_a_header_a_blank_row_and_six_commands() {
        let picker = Picker::open("/").expect("every command");
        let rows = picker.rows(120, 40, PALETTE);
        assert_eq!(rows.len(), 8);
        // The header: dim, the range in an en dash against the last column but
        // one, because seven commands do not fit in six rows.
        let left = format!("Commands {} \u{b7} type to filter", SLASH_REGISTRY.len());
        let range = "1\u{2013}6";
        assert_eq!(
            rows[0],
            format!(
                "{DIM}{left}{}{range}{RESET}",
                " ".repeat(119 - left.chars().count() - range.chars().count())
            )
        );
        assert_eq!(rows[1], "", "the blank row under the header");
        // The names in view are padded to `/version`, the widest of them,
        // plus the three-column gap; the categories share one column that
        // ends a cell short of the edge, each written from its left.
        let body: Vec<String> = rows[2..].iter().map(|row| plain(row)).collect();
        let expected = [
            ("/help", "show available slash commands", "General"),
            ("/clear", SLASH_REGISTRY[1].summary, "General"),
            ("/new", "start a fresh session", "Session"),
            ("/setup", SLASH_REGISTRY[3].summary, "Account"),
            ("/model", SLASH_REGISTRY[4].summary, "Model"),
            ("/version", "show the xfx version", "General"),
        ];
        for (row, (name, summary, category)) in body.iter().zip(expected) {
            assert_eq!(
                *row,
                format!(
                    "  {name:<11}{summary}{}{category}",
                    " ".repeat(112 - 13 - summary.len())
                ),
                "{row:?}"
            );
        }
    }

    #[test]
    fn the_marked_row_is_bright_and_the_rest_are_dim() {
        let picker = walked("/", 2);
        let rows = picker.rows(120, 40, PALETTE);
        // Upstream's selected row: bold hint grey for the name and its
        // padding, then only the colour switched, so the summary and the
        // category stay bold; closed by a reset.
        let summary = "start a fresh session";
        assert_eq!(
            rows[4],
            format!(
                "{MARKED}  /new{}{DIM}{summary}{}{DIM}Session{RESET}",
                " ".repeat(13 - 6),
                " ".repeat(112 - 13 - summary.len()),
            )
        );
        // Every other row is dim from its first cell to its last.
        for (index, row) in rows.iter().enumerate().skip(2) {
            if index == 4 {
                continue;
            }
            assert!(row.starts_with(DIM), "{row:?}");
            assert!(!row.contains(MARKED), "{row:?}");
            assert!(row.ends_with(RESET), "{row:?}");
        }
    }

    #[test]
    fn the_window_scrolls_at_its_edge_and_only_there() {
        // Down to the sixth row: the window has not moved.
        let picker = walked("/", 5);
        assert_eq!(picker.layout(40).window, Window { start: 0, end: 6 });
        // One more: the window moves by one, the mark on its last row.
        let mut picker = walked("/", 6);
        assert_eq!(picker.layout(40).window, Window { start: 1, end: 7 });
        let rows = picker.rows(120, 40, PALETTE);
        assert!(plain(&rows[0]).ends_with("2\u{2013}7"), "{:?}", rows[0]);
        assert!(rows[7].starts_with(MARKED), "{:?}", rows[7]);
        assert!(plain(&rows[7]).starts_with("  /quit"), "{:?}", rows[7]);
        // Back up one: the mark moves and the window stays.
        assert_eq!(picker.apply(PickerAction::Up), PickerOutcome::Changed);
        assert_eq!(picker.layout(40).window, Window { start: 1, end: 7 });
        assert!(picker.rows(120, 40, PALETTE)[6].starts_with(MARKED));
        // And past the bottom it wraps to the top, taking the window with it.
        assert_eq!(picker.apply(PickerAction::Down), PickerOutcome::Changed);
        assert_eq!(picker.apply(PickerAction::Down), PickerOutcome::Changed);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.layout(40).window, Window { start: 0, end: 6 });
    }

    #[test]
    fn the_name_column_is_as_wide_as_the_names_in_view() {
        // `/quit` alone: the summary starts past four letters and a slash,
        // plus the gap.
        let quit = Picker::open("/q").expect("one command");
        assert_eq!(summary_column(&quit.rows(120, 40, PALETTE)[2]), 2 + 5 + 3);

        // A list whose widest name is in view at the top and scrolls out of
        // it: the column follows the window, not the list.
        let mut picker = Picker::over(SCROLLING, "/").expect("every name");
        let top = picker.rows(120, 40, PALETTE);
        assert_eq!(summary_column(&top[2]), 2 + "/the-widest".len() + 3);
        for _ in 0..7 {
            assert_eq!(picker.apply(PickerAction::Down), PickerOutcome::Changed);
        }
        let scrolled = picker.rows(120, 40, PALETTE);
        assert!(plain(&scrolled[0]).ends_with("3\u{2013}8"), "{scrolled:?}");
        assert!(scrolled.iter().all(|row| !row.contains("/the-widest")));
        assert_eq!(summary_column(&scrolled[2]), 2 + 2 + 3);
    }

    #[test]
    fn a_filtered_menu_keeps_its_height_with_blank_rows() {
        let picker = Picker::open("/q").expect("one command");
        let rows = picker.rows(120, 40, PALETTE);
        assert_eq!(rows.len(), 8, "the menu shrank with the list");
        // No `type to filter` once something is typed, and no range while
        // everything fits.
        assert_eq!(rows[0], format!("{DIM}Commands 1{RESET}"));
        assert_eq!(rows[1], "");
        assert!(rows[2].starts_with(MARKED), "{:?}", rows[2]);
        assert_eq!(
            plain(&rows[2]),
            format!(
                "  /quit   exit the interactive shell{}General",
                " ".repeat(112 - 10 - "exit the interactive shell".len())
            )
        );
        assert!(rows[3..].iter().all(String::is_empty), "{rows:?}");
    }

    #[test]
    fn a_screen_with_two_rows_or_fewer_drops_the_header() {
        let picker = Picker::open("/").expect("every command");
        let rows = picker.rows(80, 12, PALETTE);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].starts_with(MARKED), "{rows:?}");
        assert!(plain(&rows[0]).starts_with("  /help"), "{rows:?}");
        assert!(plain(&rows[1]).starts_with("  /clear"), "{rows:?}");
        let rows = picker.rows(20, 7, PALETTE);
        assert_eq!(rows.len(), 1);
        assert!(plain(&rows[0]).starts_with("  /help"), "{rows:?}");
        // With three rows the header is back, over one command.
        let rows = picker.rows(80, 13, PALETTE);
        assert_eq!(rows.len(), 3);
        assert!(plain(&rows[0]).starts_with("Commands 7"), "{rows:?}");
        assert!(plain(&rows[0]).ends_with("1\u{2013}1"), "{rows:?}");
        assert_eq!(rows[1], "");
    }

    #[test]
    fn a_narrow_screen_drops_the_category_and_cuts_the_summary() {
        // 42 columns: the summary cannot keep 24 cells beside a category, so
        // the category goes and the summary is cut with an ellipsis
        // (`tui-slash-menu.test.ts`, "remains alive when descriptions clip").
        let picker = Picker::open("/mo").expect("one command");
        let rows = picker.rows(42, 18, PALETTE);
        let row = plain(&rows[2]);
        assert!(!row.contains("Model"), "{row:?}");
        assert!(row.ends_with('\u{2026}'), "{row:?}");
        assert_eq!(row.chars().count(), 41, "{row:?}");
        // The header is cut the same way rather than run off the row.
        let rows = Picker::open("/").expect("all").rows(20, 40, PALETTE);
        assert!(plain(&rows[0]).contains('\u{2026}'), "{:?}", rows[0]);
        assert!(plain(&rows[0]).ends_with("1\u{2013}6"), "{:?}", rows[0]);
        assert!(super::super::wrap::width(&rows[0]) < 20);
    }

    #[test]
    fn the_key_help_is_upstreams_and_fits_the_screen() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     enter use     esc close"
        );
        assert_eq!(
            key_help(38),
            "\u{2191}\u{2193} navigate  enter use  esc close"
        );
        assert_eq!(key_help(20), "\u{2191}\u{2193} move  enter  esc");
        assert_eq!(key_help(12), "enter esc");
        for cols in 9..60 {
            assert!(cells(key_help(cols)) <= usize::from(cols), "{cols}");
        }
    }

    #[test]
    fn a_query_that_names_nothing_opens_no_picker() {
        assert!(Picker::open("/zzz").is_none());
        assert!(Picker::open("/re").is_none(), "no xfx name has `re` in it");
        assert!(Trigger::of("say /help to me").is_none());
        assert!(Trigger::of("/model acme").is_none());
    }

    #[test]
    fn completing_a_command_that_takes_an_argument_leaves_room_for_one() {
        assert_eq!(completed("/model"), "/model ");
        assert_eq!(completed("/help"), "/help");
        assert_eq!(completed("/exit"), "/exit");
        for spec in SLASH_REGISTRY {
            assert!(completed(spec.name).starts_with(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn the_marked_row_is_the_one_tab_and_enter_take() {
        let mut picker = Picker::open("/").expect("every command");
        assert_eq!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/help")
        );
        assert_eq!(picker.apply(PickerAction::Down), PickerOutcome::Changed);
        assert_eq!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/clear")
        );
        assert_eq!(
            picker.apply(PickerAction::Submit),
            PickerOutcome::Run("/clear")
        );
        // Up from the top wraps.
        assert_eq!(picker.apply(PickerAction::Up), PickerOutcome::Changed);
        assert_eq!(picker.apply(PickerAction::Up), PickerOutcome::Changed);
        assert_eq!(
            picker.apply(PickerAction::Complete),
            PickerOutcome::Complete("/quit")
        );
        assert_eq!(picker.apply(PickerAction::Escape), PickerOutcome::Dismiss);
        // An alias row takes the alias, which is the name it is shown as.
        let mut exit = Picker::open("/ex").expect("the alias");
        assert_eq!(
            exit.apply(PickerAction::Submit),
            PickerOutcome::Run("/exit")
        );
    }
}
