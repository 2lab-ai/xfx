//! The skills menu: every skill the session can see, by source, in the band
//! under the composer -- opened by `/skills` and by a `$` the user types.
//!
//! Upstream's (`vercel-labs/fx@c1db9191`): the state machine `SkillMenu`
//! (`src/core/skills/skill_runtime.zig:1426-1561`), its ordering
//! (`SkillMenuView`, `:1330-1395`) and its inline presentation
//! (`src/ui/footer/skills_menu_presentation.zig`), in the slot the `/help`
//! catalog uses and with its own key help on the hint row:
//!
//! ```text
//! Skills 2  [All]  xfx  Workspace
//!
//!   demo             xfx · Workspace
//!   release-notes    xfx · Workspace
//! ```
//!
//! A bold title, the source tabs (the active one bracketed), a blank row, and
//! one row per skill: its name, and from the widest name plus four its scope.
//! The marked row is bold, the rest dim, and nothing marks it but that.
//!
//! # What it is a view of
//!
//! Like the catalog it is a **view of the composer**, never a focus, and what
//! the composer holds means one of two things ([`Origin`]):
//!
//! * opened by `/skills` (or `/skills show`), the whole composer is the
//!   search, and Enter replaces it with the chosen skill's token;
//! * opened by a `$` -- typed, or in a paste -- the word after the `$` is the
//!   search, and Enter replaces the word with the token. The menu shows only
//!   while some skill matches that word (`menuVisible`, `:2639-2644`); a space
//!   ends it.
//!
//! # What is xfx's
//!
//! The tabs are upstream's families that xfx reads: `All`, `xfx` and
//! `Workspace`. Upstream also has `Claude`, `Codex`, `Agents`, `OpenCode` and
//! `Claw` tabs, for the other agents' roots it reads; xfx reads none of them
//! ([`crate::skills::discovery`]), so a tab for one would be a list that is
//! always empty and a claim that it is looked at.

use std::ops::Range;

use crate::skills::{Skill, Source};

use super::picker::{cells, edge_start, ellipsized};
use super::theme::Palette;

/// The header and the blank row under it (`header_rows`, `roomy_top_gap_rows`).
const HEADER_ROWS: u16 = 2;

/// The most rows the menu is given: six skills under a header and a gap
/// (`inlinePickerRowBudget`, `picker_presentation.zig:751-758`, over
/// `max_model_picker_rows + 2`).
const MOST_ROWS: u16 = 8;

/// The band's rows the menu cannot have (`inlinePickerRowBudgetCapped`'s
/// `fixed_rows`, `picker_presentation.zig:766`), before the composer's extra
/// rows are added to them.
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them.
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// Every skill row's indent, on a screen wider than four.
const INDENT: usize = 2;

/// The columns between the widest name and the scopes
/// (`inline_picker_column_gap_width`, `picker_presentation.zig:17`).
const COLUMN_GAP: usize = 4;

/// What separates the title and the tabs.
const TAB_GAP: &str = "  ";

/// What the hint row says while the menu is up, widest first
/// (`source_variants`, `input_presentation.zig:820-826`).
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     tab source     enter use     esc close",
    "\u{2191}\u{2193} navigate  tab source  enter use  esc close",
    "\u{2191}\u{2193} move  tab source  enter  esc",
    "enter use  esc close",
    "enter esc",
];

/// How the menu was opened, which is what the composer means to it
/// (`SkillMenuOrigin`, `skill_runtime.zig:240-254`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// `/skills`: the whole composer is the search.
    Command,
    /// A typed `$`: the word after it is.
    Dollar,
    /// A `$word` that arrived in a paste.
    Paste,
}

impl Origin {
    /// Whether the search is one `$` word rather than the composer
    /// (`isMention`).
    pub(crate) fn is_mention(self) -> bool {
        matches!(self, Self::Dollar | Self::Paste)
    }
}

/// One source tab (`SkillMenuSourceFilter`, cut to xfx's roots).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    All,
    Xfx,
    Workspace,
}

/// The tabs, in the order upstream lists the ones xfx has
/// (`skill_menu_source_filters`, `:229-238`).
const FILTERS: [Filter; 3] = [Filter::All, Filter::Xfx, Filter::Workspace];

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Xfx => "xfx",
            Self::Workspace => "Workspace",
        }
    }

    /// The tab a source is under (`skillMenuFilterForSource`, `:1187-1198`).
    fn of(source: Source) -> Self {
        match source {
            Source::Managed | Source::WorkspaceXfx => Self::Xfx,
            Source::WorkspaceShared => Self::Workspace,
        }
    }

    fn admits(self, source: Source) -> bool {
        self == Self::All || Self::of(source) == self
    }
}

/// The scope a row says beside its name (`skillSourceScopeLabel`,
/// `skills_menu_presentation.zig:422-438`).
fn scope(source: Source) -> &'static str {
    match source {
        Source::Managed => "xfx \u{b7} Global",
        Source::WorkspaceXfx | Source::WorkspaceShared => "xfx \u{b7} Workspace",
    }
}

/// How well a skill answers a search: its name starts with it, has it, or
/// its description, source or path has it (`skillMatchRank`, `:1378-1395`).
fn rank(skill: &Skill, query: &str) -> Option<usize> {
    let query = query.trim_matches([' ', '\t', '\r', '\n', '/']);
    if query.is_empty() {
        return Some(0);
    }
    let lower = query.to_ascii_lowercase();
    let name = skill.name.to_ascii_lowercase();
    if name.starts_with(&lower) {
        return Some(0);
    }
    if name.contains(&lower) {
        return Some(1);
    }
    let elsewhere = [
        skill.description.as_str(),
        skill.source.short_label(),
        &skill.path.to_string_lossy(),
    ]
    .iter()
    .any(|text| text.to_ascii_lowercase().contains(&lower));
    elsewhere.then_some(2)
}

/// The skills a tab and a search list, as indexes into `items`: by how well
/// they match, then managed installs before workspace skills, then in
/// discovery order (`SkillMenuView.next`, `:1349-1376`).
fn listed(items: &[Skill], filter: Filter, query: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for match_rank in 0..3 {
        for group in 0..2 {
            for (index, skill) in items.iter().enumerate() {
                if rank(skill, query) == Some(match_rank)
                    && skill.source.group().0 == group
                    && filter.admits(skill.source)
                {
                    out.push(index);
                }
            }
        }
    }
    out
}

/// Every skill a search finds on `All`, in the menu's order: what the slash
/// popup lists after its commands (`fillSkillMenuRangeAtQuery` with `.all`,
/// `picker_presentation.zig:1123-1131`).
pub(crate) fn matching(items: &[Skill], query: &str) -> Vec<Skill> {
    listed(items, Filter::All, query)
        .into_iter()
        .map(|index| items[index].clone())
        .collect()
}

/// The menu's shape in one row budget (`SkillsMenuLayout.buildInline`,
/// `skills_menu_presentation.zig:84-117`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    rows: u16,
    header: bool,
    first_item_row: u16,
    visible: usize,
}

fn layout(count: usize, budget: u16) -> Layout {
    if budget == 0 {
        return Layout::default();
    }
    if count == 0 {
        if budget < HEADER_ROWS + 1 {
            return Layout {
                rows: 1,
                ..Layout::default()
            };
        }
        return Layout {
            rows: HEADER_ROWS + 1,
            header: true,
            first_item_row: HEADER_ROWS,
            visible: 0,
        };
    }
    if budget <= 2 {
        let visible = count.min(usize::from(budget));
        return Layout {
            rows: u16::try_from(visible).unwrap_or(budget),
            header: false,
            first_item_row: 0,
            visible,
        };
    }
    let visible = count.min(usize::from(budget - HEADER_ROWS));
    Layout {
        rows: HEADER_ROWS + u16::try_from(visible).unwrap_or(budget - HEADER_ROWS),
        header: true,
        first_item_row: HEADER_ROWS,
        visible,
    }
}

/// The menu: what it lists from, which tab is up, which skill is marked, and
/// what the search is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillsMenu {
    /// The skills discovered when it opened.
    items: Vec<Skill>,
    filter: Filter,
    /// An index into the listed skills. Moves stop at either end rather than
    /// wrapping (`moveVisibleRowsCount`, `:1488-1505`).
    selected: usize,
    /// The first listed skill in view, as the last move left it.
    window_start: usize,
    origin: Origin,
    /// The `$` word a mention menu replaces, `$` included.
    target: Option<Range<usize>>,
    query: String,
}

impl SkillsMenu {
    /// A menu on `All`, the first skill marked (`openWithQuery`, `:1438-1449`).
    pub(crate) fn open(
        items: Vec<Skill>,
        origin: Origin,
        target: Option<Range<usize>>,
        query: &str,
    ) -> Self {
        Self {
            items,
            filter: Filter::All,
            selected: 0,
            window_start: 0,
            origin,
            target,
            query: query.to_string(),
        }
    }

    /// `/skills show <name>`: the menu on the skill's own tab with it marked,
    /// or `None` when no one skill has that name (`openMenuFocusedByName`,
    /// `:2553-2576`).
    pub(crate) fn focused(items: Vec<Skill>, name: &str) -> Option<Self> {
        let mut named = items
            .iter()
            .enumerate()
            .filter(|(_, skill)| skill.name == name);
        let (index, skill) = named.next()?;
        if named.next().is_some() {
            return None;
        }
        let filter = Filter::of(skill.source);
        let selected = listed(&items, filter, "")
            .iter()
            .position(|listed| *listed == index)?;
        Some(Self {
            items,
            filter,
            selected,
            window_start: 0,
            origin: Origin::Command,
            target: None,
            query: String::new(),
        })
    }

    pub(crate) fn origin(&self) -> Origin {
        self.origin
    }

    /// Reads a list that landed after it opened, keeping its tab, search and
    /// mark -- the mark held to what is listed -- as upstream's menu reads the
    /// runtime's list each frame (`SkillMenuView`, over `app.skills.items`).
    pub(crate) fn replace_items(&mut self, items: Vec<Skill>) {
        self.items = items;
        let count = self.count();
        if count == 0 {
            self.selected = 0;
            self.window_start = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }

    /// The `$` word a choice replaces, for a mention menu.
    pub(crate) fn target(&self) -> Option<Range<usize>> {
        self.target.clone()
    }

    /// Reads the composer again after an edit (`syncSkillsMenu`,
    /// `app_input_runtime.zig:2940-2958`): the whole of it is the search of a
    /// command menu; a mention menu follows its `$` word, and is over --
    /// `false` -- once the `$` it opened on is gone.
    pub(crate) fn sync(&mut self, text: &str) -> bool {
        if !self.origin.is_mention() {
            self.set_query(text);
            return true;
        }
        let Some(target) = self.target.clone() else {
            return false;
        };
        let bytes = text.as_bytes();
        if target.start >= bytes.len() || bytes[target.start] != b'$' {
            return false;
        }
        let end = word_end(bytes, target.start + 1);
        self.target = Some(target.start..end);
        let query = text[target.start + 1..end].to_string();
        self.set_query(&query);
        true
    }

    /// A new search: the mark stays where it was, held to what is listed
    /// (`setMenuQuery`, `:2601-2623`).
    fn set_query(&mut self, query: &str) {
        if self.query == query {
            return;
        }
        self.query = query.to_string();
        let count = self.count();
        if count == 0 {
            self.selected = 0;
            self.window_start = 0;
        } else if self.selected >= count {
            self.selected = count - 1;
        }
    }

    /// Whether the band shows it: always for a command menu, and for a mention
    /// menu only while some skill matches its word on any tab
    /// (`menuVisible`, `:2639-2644`).
    pub(crate) fn visible(&self) -> bool {
        !self.origin.is_mention() || !listed(&self.items, Filter::All, &self.query).is_empty()
    }

    fn count(&self) -> usize {
        listed(&self.items, self.filter, &self.query).len()
    }

    /// The mark `delta` skills on, stopping at either end, and the window
    /// scrolled only as far as keeps it in view of `visible` rows.
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16) -> bool {
        let count = self.count();
        if count == 0 {
            return false;
        }
        let last = i64::try_from(count - 1).unwrap_or(i64::MAX);
        let current = i64::try_from(self.selected % count).unwrap_or(0);
        self.selected = usize::try_from(current.saturating_add(delta).clamp(0, last)).unwrap_or(0);
        self.window_start = edge_start(self.window_start, count, self.selected, visible.max(1));
        true
    }

    /// The next tab, or the previous one, wrapping; the mark back on the first
    /// skill (`advanceSourceFilter`, `:1512-1524`).
    pub(crate) fn cycle(&mut self, forward: bool) {
        let at = FILTERS
            .iter()
            .position(|filter| *filter == self.filter)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % FILTERS.len()
        } else {
            (at + FILTERS.len() - 1) % FILTERS.len()
        };
        self.filter = FILTERS[next];
        self.selected = 0;
        self.window_start = 0;
    }

    /// The marked skill, or `None` when nothing is listed
    /// (`selectedMenuSkill`, `:2625-2637`).
    pub(crate) fn selected(&self) -> Option<&Skill> {
        let listed = listed(&self.items, self.filter, &self.query);
        let count = listed.len();
        (count > 0).then(|| &self.items[listed[self.selected % count]])
    }

    /// How many rows the band gives it on a screen `terminal_rows` tall whose
    /// composer is `input_extra` rows taller than one.
    pub(crate) fn height(&self, terminal_rows: u16, input_extra: u16) -> u16 {
        layout(self.count(), row_budget(terminal_rows, input_extra)).rows
    }

    /// How many skills are in view in `max_rows` rows, never fewer than one:
    /// the window a move scrolls against.
    pub(crate) fn visible_items(&self, max_rows: u16) -> u16 {
        let visible = layout(self.count(), max_rows).visible.max(1);
        u16::try_from(visible).unwrap_or(u16::MAX)
    }

    /// The menu's rows in `max_rows` rows of a `cols`-wide screen
    /// (`composeSkillsMenuRow`, `skills_menu_presentation.zig:236-273`).
    pub(crate) fn rows(&self, cols: u16, max_rows: u16, palette: Palette) -> Vec<String> {
        let listed = listed(&self.items, self.filter, &self.query);
        let count = listed.len();
        let layout = layout(count, max_rows);
        let mut rows = Vec::with_capacity(usize::from(layout.rows));
        if layout.rows == 0 {
            return rows;
        }
        if layout.header {
            rows.push(self.header(count, cols, palette));
            rows.push(String::new());
        }
        if count == 0 {
            let text = if self.items.is_empty() {
                "No skills available.".to_string()
            } else if self.filter == Filter::All {
                "No skills found.".to_string()
            } else {
                format!("No {} skills found.", self.filter.label())
            };
            rows.push(format!(
                "{}{}{}",
                palette.dim(),
                ellipsized(&text, usize::from(cols)),
                palette.reset()
            ));
            return rows;
        }
        let selected = self.selected % count;
        let first = edge_start(
            self.window_start,
            count,
            selected,
            u16::try_from(layout.visible).unwrap_or(u16::MAX),
        );
        let in_view = &listed[first..(first + layout.visible).min(count)];
        let name_column = listed
            .iter()
            .map(|index| cells(&self.items[*index].name))
            .max()
            .unwrap_or(0);
        let scope_column = in_view
            .iter()
            .map(|index| cells(scope(self.items[*index].source)))
            .max()
            .unwrap_or(0);
        for (offset, index) in in_view.iter().enumerate() {
            rows.push(skill_row(
                &self.items[*index],
                first + offset == selected,
                name_column,
                scope_column,
                cols,
                palette,
            ));
        }
        rows
    }

    /// `Skills N` and the tabs, or as much of them as fits
    /// (`composeHeaderRow`, `:298-341`).
    fn header(&self, count: usize, cols: u16, palette: Palette) -> String {
        let width = usize::from(cols);
        let title = format!("{}Skills {count}{}", palette.subtitle(), palette.reset());
        let tab = |filter: Filter| painted_tab(filter.label(), filter == self.filter, palette);
        let mut wide = title.clone();
        for filter in FILTERS {
            wide.push_str(TAB_GAP);
            wide.push_str(&tab(filter));
        }
        if cells(&wide) <= width {
            return wide;
        }
        let compact = format!(
            "{title}    {}Source {}{}",
            palette.dim(),
            palette.reset(),
            tab(self.filter)
        );
        if cells(&compact) <= width {
            return compact;
        }
        let compact = format!("{title}{TAB_GAP}{}", tab(self.filter));
        if cells(&compact) <= width {
            return compact;
        }
        format!(
            "{}[{}]{}",
            palette.subtitle(),
            super::frame::clip(self.filter.label(), cols.saturating_sub(2)),
            palette.reset()
        )
    }
}

/// Where a `$` word ends: letters, digits, `_ - . :` (`skillTokenEnd`,
/// `app_input_runtime.zig:2967-2975`).
pub(crate) fn word_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b'.' | b':'))
    {
        end += 1;
    }
    end
}

/// One tab: the active one bold in brackets, the rest dim (`appendSourceTab`).
fn painted_tab(label: &str, active: bool, palette: Palette) -> String {
    if active {
        format!("{}[{label}]{}", palette.subtitle(), palette.reset())
    } else {
        format!("{}{label}{}", palette.dim(), palette.reset())
    }
}

/// One skill's row (`composeSkillTitleRow`, `:354-400`): the indent, then in
/// the row's style the name, the spaces to the scope column and the scope,
/// closed once at the end.
fn skill_row(
    skill: &Skill,
    marked: bool,
    name_column: usize,
    scope_column: usize,
    cols: u16,
    palette: Palette,
) -> String {
    let width = usize::from(cols);
    let indent = if width <= 4 { 0 } else { INDENT };
    let label = scope(skill.source);
    let scope_width = scope_column.max(cells(label));
    let content = width.saturating_sub(1);
    let show_scope = scope_width > 0 && content >= indent + name_column + COLUMN_GAP + scope_width;
    let name_budget = if show_scope {
        name_column
    } else {
        width.saturating_sub(indent)
    };
    let style = if marked {
        palette.subtitle()
    } else {
        palette.dim()
    };
    let mut row = " ".repeat(indent);
    row.push_str(style);
    row.push_str(&ellipsized(&skill.name, name_budget));
    if show_scope {
        let column = indent + name_column + COLUMN_GAP;
        let mut written = indent + cells(&ellipsized(&skill.name, name_budget));
        while written < column {
            row.push(' ');
            written += 1;
        }
        row.push_str(label);
    }
    row.push_str(palette.reset());
    row
}

/// The rows a screen `terminal_rows` tall gives the menu when its composer is
/// `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:760-770`).
fn row_budget(terminal_rows: u16, input_extra: u16) -> u16 {
    terminal_rows
        .saturating_sub(FIXED_ROWS.saturating_add(input_extra))
        .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
        .clamp(1, MOST_ROWS)
}

/// What the hint row says while the menu is up, on a screen `cols` wide.
pub(crate) fn key_help(cols: u16) -> &'static str {
    KEY_HELP
        .iter()
        .copied()
        .find(|candidate| cells(candidate) <= usize::from(cols))
        .unwrap_or(KEY_HELP[KEY_HELP.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn skill(name: &str, source: Source) -> Skill {
        Skill {
            name: name.to_string(),
            description: format!("{name} description"),
            path: PathBuf::from(format!("/root/{name}")),
            source,
        }
    }

    /// The capture's workspace: `demo` in `.xfx/skills`, `release-notes` in
    /// `skills/`.
    fn captured() -> Vec<Skill> {
        vec![
            skill("demo", Source::WorkspaceXfx),
            skill("release-notes", Source::WorkspaceShared),
        ]
    }

    const PALETTE: Palette = Palette {
        mode: super::super::theme::Mode::Dark,
    };

    /// The text of each row with its colours taken out.
    fn plain(rows: Vec<String>) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let mut out = String::new();
                let mut rest = row.as_str();
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
            })
            .collect()
    }

    #[test]
    fn the_menu_is_upstreams_rows_with_xfxs_tabs() {
        let menu = SkillsMenu::open(captured(), Origin::Command, None, "");
        assert_eq!(menu.height(40, 0), 4);
        assert_eq!(
            plain(menu.rows(120, 4, PALETTE)),
            [
                "Skills 2  [All]  xfx  Workspace",
                "",
                "  demo             xfx \u{b7} Workspace",
                "  release-notes    xfx \u{b7} Workspace",
            ]
        );
        let painted = menu.rows(120, 4, PALETTE);
        assert!(painted[2].starts_with(&format!("  {}demo", PALETTE.subtitle())));
        assert!(painted[3].starts_with(&format!("  {}release-notes", PALETTE.dim())));
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     tab source     enter use     esc close"
        );
    }

    #[test]
    fn managed_installs_list_first_and_a_search_narrows_by_name_first() {
        let mut items = captured();
        items.push(skill("foo", Source::Managed));
        let mut menu = SkillsMenu::open(items, Origin::Dollar, Some(0..1), "");
        assert!(menu.sync("$"));
        assert_eq!(menu.selected().unwrap().name, "foo");
        assert_eq!(
            plain(menu.rows(120, 8, PALETTE))[2],
            "  foo              xfx \u{b7} Global"
        );
        assert!(menu.sync("$dem"));
        assert_eq!(menu.selected().unwrap().name, "demo");
        assert_eq!(menu.target(), Some(0..4));
        assert!(menu.visible());
        assert!(menu.sync("$zzz"));
        assert!(
            !menu.visible(),
            "a word nothing matches hides a mention menu"
        );
        assert!(!menu.sync("dem"), "the `$` it opened on is gone");
    }

    #[test]
    fn show_opens_on_the_skills_own_tab_and_moves_stop_at_the_ends() {
        let menu = SkillsMenu::focused(captured(), "demo").unwrap();
        assert_eq!(
            plain(menu.rows(120, 8, PALETTE))[0],
            "Skills 1  All  [xfx]  Workspace"
        );
        assert!(SkillsMenu::focused(captured(), "nope").is_none());

        let mut menu = SkillsMenu::open(captured(), Origin::Command, None, "");
        menu.step_by(-1, 6);
        assert_eq!(menu.selected().unwrap().name, "demo");
        menu.step_by(5, 6);
        assert_eq!(menu.selected().unwrap().name, "release-notes");
        menu.cycle(true);
        assert_eq!(menu.selected().unwrap().name, "demo");
        menu.cycle(true);
        assert_eq!(menu.selected().unwrap().name, "release-notes");
        menu.cycle(true);
        menu.cycle(false);
        assert_eq!(
            plain(menu.rows(120, 8, PALETTE))[0],
            "Skills 1  All  xfx  [Workspace]"
        );
    }

    #[test]
    fn an_empty_list_says_why() {
        let menu = SkillsMenu::open(Vec::new(), Origin::Command, None, "");
        assert_eq!(
            plain(menu.rows(120, 8, PALETTE)),
            [
                "Skills 0  [All]  xfx  Workspace",
                "",
                "No skills available."
            ]
        );
        let mut menu = SkillsMenu::open(captured(), Origin::Command, None, "");
        menu.sync("zzz");
        assert_eq!(plain(menu.rows(120, 8, PALETTE))[2], "No skills found.");
        menu.cycle(true);
        assert_eq!(plain(menu.rows(120, 8, PALETTE))[2], "No xfx skills found.");
    }
}
