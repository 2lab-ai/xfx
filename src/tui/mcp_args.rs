//! `/mcp `'s argument completions: the verbs, then each server's lines,
//! under the argument.
//!
//! fx v0.0.16 completes what follows `/mcp ` from a list the app hands its
//! slash registry (`ArgumentCompletion`, `src/core/mods/registry.zig:23`):
//! the eight verbs with what each does, then `/mcp show NAME`, `remove`,
//! `login` and `logout` for a remote server, `approve` and `reject` for a
//! project one, each with the server's status (`buildCompletion`,
//! `src/core/app/app_mcp_runtime.zig:459-515@481104fd`; the list is
//! [`crate::mcp::command::completions`]). A verb row matches while the
//! argument has no space, a `VERB NAME` row once it has, by the start of the
//! row ignoring case (`runtimeArgMatches`, `command_specs.zig:892-897`).
//!
//! # What it looks like
//!
//! `fx-p5-mcp-verbs-40x120-complete-args` and `-complete-show`: no header,
//! one row per completion **anchored under the argument** (column 7 for
//! `┃ /mcp `, `argCompletionAnchor`, `command_specs.zig:636-644`), the text
//! after `/mcp ` padded to the widest in view and two more, then the
//! description in the dim grey. The marked row's text is bold in the hint
//! grey, every other row's dim. At most six rows, padded blank below the last
//! so the band keeps its height while the argument narrows the list; the hint
//! row under it stays the status line. The list is rebuilt from the servers
//! at most twice a second, as fx's is (`refreshCompletion`, `:470-484`).
//!
//! # Keys
//!
//! Up and Down move the mark and wrap; Tab writes the marked line into the
//! composer; **Enter submits the marked line** in place of what was typed,
//! as fx's slash completion does (`resolveSlashSubmission`,
//! `src/core/app/input_submit_runtime.zig:1819-1824@481104fd`) -- so
//! `/mcp remove web` with only `web2` configured runs `/mcp remove web2`
//! (`p5-mcp-verbs`); Escape closes it until the draft stops being a `/mcp `
//! line. With nothing matching it is not up, and Enter submits the text.

use std::time::{Duration, Instant};

use super::picker::{advanced, cells, edge_start};
use super::theme::Palette;

/// The command the completions are for (`runtimeArgCommand`).
const COMMAND: &str = "/mcp";

/// The most rows it shows (`list_window.default_max_picker_rows`,
/// `list_window.zig:3`).
const MOST_ROWS: u16 = 6;

/// The band's rows a list picker cannot have (`activeListPickerReservedRows`'
/// `fixed_rows`, `picker_presentation.zig:635`).
const FIXED_ROWS: u16 = 5;

/// How often the list is rebuilt while it is up (`refreshCompletion`'s
/// 500 ms, `app_mcp_runtime.zig:472-474`).
const REFRESH: Duration = Duration::from_millis(500);

/// Where the argument starts in `text`, in bytes, and the argument as the
/// matching reads it, if `text` is a `/mcp ` line: blanks before the command
/// skipped (`slashCompletionPrefix`, `command_specs.zig:568-576`), `/mcp`
/// then a space, and the argument with its leading spaces trimmed
/// (`runtimeArgQuery`, `:883-890`). The anchor is the byte after that space
/// whatever follows (`slashRawAnchor`, `input_presentation.zig:287-292`).
pub(crate) fn query_of(text: &str) -> Option<(usize, &str)> {
    let trimmed = text.trim_start_matches([' ', '\t', '\r', '\n']);
    let lead = text.len() - trimmed.len();
    let rest = trimmed.strip_prefix(COMMAND)?.strip_prefix(' ')?;
    Some((lead + COMMAND.len() + 1, rest.trim_start_matches(' ')))
}

/// Whether the completion `full` answers `query` (`runtimeArgMatches`).
fn answers(full: &str, query: &str) -> bool {
    let Some(rest) = full
        .strip_prefix(COMMAND)
        .and_then(|rest| rest.strip_prefix(' '))
    else {
        return false;
    };
    if query.contains(' ') != rest.contains(' ') {
        return false;
    }
    rest.get(..query.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(query))
}

/// The completions for one `/mcp ` line, and which is marked.
#[derive(Debug)]
pub(crate) struct McpArgs {
    /// The composer text this was built for.
    text: String,
    /// Where the argument starts in [`Self::text`]: what the rows hang under.
    token_start: usize,
    /// Every completion, the whole line and its description, as last read.
    entries: Vec<(String, String)>,
    /// The ones [`Self::text`] matches, as indices into [`Self::entries`].
    matching: Vec<usize>,
    /// When [`Self::entries`] was read.
    read_at: Instant,
    /// An index into [`Self::matching`].
    selected: usize,
    /// The first row in view, as the last move left it.
    window_start: usize,
    /// The marked line went away in a fresh read: closed until the text
    /// changes, so Enter submits what was typed rather than a line nobody
    /// marked.
    lost: bool,
}

impl McpArgs {
    /// The completions `text` asks for over `entries`, or `None` when `text`
    /// is not a `/mcp ` line. The first row is marked, as a changed text
    /// marks it (`resetActiveCompletionIndex`).
    pub(crate) fn open(text: &str, entries: Vec<(String, String)>, now: Instant) -> Option<Self> {
        let (token_start, _) = query_of(text)?;
        let mut args = Self {
            text: text.to_string(),
            token_start,
            entries,
            matching: Vec::new(),
            read_at: now,
            selected: 0,
            window_start: 0,
            lost: false,
        };
        args.rematch();
        Some(args)
    }

    fn rematch(&mut self) {
        let query = query_of(&self.text).map_or("", |(_, query)| query);
        self.matching = (0..self.entries.len())
            .filter(|&index| answers(&self.entries[index].0, query))
            .collect();
        let count = self.matching.len();
        if count > 0 {
            self.selected %= count;
            self.window_start = edge_start(self.window_start, count, self.selected, MOST_ROWS);
        } else {
            self.selected = 0;
            self.window_start = 0;
        }
    }

    /// The composer text it was built for.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The byte of [`Self::text`] the rows hang under.
    pub(crate) fn token_start(&self) -> usize {
        self.token_start
    }

    /// Whether it has a row to show: with none it is not up.
    pub(crate) fn visible(&self) -> bool {
        !self.lost && !self.matching.is_empty()
    }

    /// Whether the list is due to be read again.
    pub(crate) fn due(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.read_at) >= REFRESH
    }

    /// Takes a fresh read of the completions, and says whether what it shows
    /// changed. **The mark stays on its line**, not on its index: a server
    /// that went away above it must not put a destructive line under the mark
    /// Enter then runs. When the marked line itself went away the list
    /// closes, for as long as the text is the same.
    pub(crate) fn refresh(&mut self, entries: Vec<(String, String)>, now: Instant) -> bool {
        self.read_at = now;
        if entries == self.entries {
            return false;
        }
        let marked = self.chosen().map(str::to_string);
        self.entries = entries;
        self.rematch();
        if let Some(line) = marked {
            match self
                .matching
                .iter()
                .position(|&index| self.entries[index].0 == line)
            {
                Some(position) => {
                    self.selected = position;
                    self.window_start = edge_start(
                        self.window_start,
                        self.matching.len(),
                        self.selected,
                        MOST_ROWS,
                    );
                }
                None => self.lost = true,
            }
        }
        true
    }

    /// The rows a screen `terminal_rows` tall gives it, with `reserved` rows
    /// of the band taken by something other than the five every band has --
    /// [`super::provider_picker::ProviderPicker::height`]'s rule, a list
    /// picker's (`activeListPickerReservedRows`). Never none.
    pub(crate) fn height(terminal_rows: u16, reserved: u16) -> u16 {
        let available = terminal_rows.saturating_sub(FIXED_ROWS.saturating_add(reserved));
        if available == 0 {
            1
        } else {
            available.min(MOST_ROWS)
        }
    }

    /// Its rows, top first, exactly `height` of them, on a screen `cols` wide
    /// with the rows anchored at cell `anchor`.
    pub(crate) fn rows(
        &self,
        cols: u16,
        anchor: u16,
        height: u16,
        palette: Palette,
    ) -> Vec<String> {
        let available = usize::from(cols).saturating_sub(usize::from(anchor));
        let count = self.matching.len();
        let start = edge_start(self.window_start, count, self.selected, height);
        let end = (start + usize::from(height)).min(count);
        let in_view: Vec<(&str, &str)> = self.matching[start..end]
            .iter()
            .map(|&index| {
                let (full, description) = &self.entries[index];
                (&full[COMMAND.len() + 1..], description.as_str())
            })
            .collect();
        // The text column is as wide as the widest text **in view**.
        let widest = in_view
            .iter()
            .map(|(label, _)| cells(label))
            .max()
            .unwrap_or(0);
        let mut painted: Vec<String> = in_view
            .iter()
            .enumerate()
            .map(|(offset, (label, description))| {
                row(
                    label,
                    description,
                    start + offset == self.selected,
                    widest,
                    anchor,
                    available,
                    palette,
                )
            })
            .collect();
        painted.resize(usize::from(height), String::new());
        painted
    }

    /// Moves the mark `delta` rows on, wrapping at either end, and says
    /// whether there was a row to move to (`advanceSelection`).
    pub(crate) fn step_by(&mut self, delta: i64) -> bool {
        let count = self.matching.len();
        if count == 0 {
            return false;
        }
        self.selected = advanced(self.selected, count, delta);
        self.window_start = edge_start(self.window_start, count, self.selected, MOST_ROWS);
        true
    }

    /// The marked line, whole: what Tab writes and Enter submits.
    pub(crate) fn chosen(&self) -> Option<&str> {
        if self.lost {
            return None;
        }
        let index = *self.matching.get(self.selected)?;
        Some(self.entries[index].0.as_str())
    }
}

/// One completion's row: the text in the row's style -- bold hint grey when
/// marked, dim otherwise -- padded to `widest` and two more, then the
/// description dim, the whole cut to the cells there are.
fn row(
    label: &str,
    description: &str,
    marked: bool,
    widest: usize,
    anchor: u16,
    available: usize,
    palette: Palette,
) -> String {
    if available == 0 {
        return String::new();
    }
    let style = if marked {
        palette.subtitle()
    } else {
        palette.dim()
    };
    let mut out = " ".repeat(usize::from(anchor));
    out.push_str(style);
    out.push_str(clipped(label, available));
    out.push_str(palette.reset());
    let gap = widest.saturating_sub(cells(label)) + 2;
    let used = cells(label) + gap;
    if used < available && !description.is_empty() {
        out.push_str(&" ".repeat(gap));
        out.push_str(palette.dim());
        out.push_str(clipped(description, available - used));
        out.push_str(palette.reset());
    }
    out
}

/// `text` in at most `budget` cells, by the painter's own measure.
fn clipped(text: &str, budget: usize) -> &str {
    super::frame::clip(text, u16::try_from(budget).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::theme::Mode;

    const PALETTE: Palette = Palette { mode: Mode::Dark };
    const MARKED: &str = "\u{1b}[1;38;5;255m";
    const DIM: &str = "\u{1b}[38;5;245m";
    const RESET: &str = "\u{1b}[0m";

    fn verbs() -> Vec<(String, String)> {
        [
            ("/mcp list", "configured servers"),
            ("/mcp show", "connect to a server and list its tools"),
            ("/mcp add", "add a server"),
            ("/mcp remove", "remove a server and its login"),
            ("/mcp login", "sign in to an HTTP server"),
            ("/mcp logout", "sign out of an HTTP server"),
            ("/mcp approve", "approve a project server"),
            ("/mcp reject", "reject a project server"),
            ("/mcp show fixture", "ready, 1 tool"),
            ("/mcp remove fixture", "ready, 1 tool"),
            ("/mcp show remote", "connecting"),
            ("/mcp remove remote", "connecting"),
            ("/mcp login remote", "connecting"),
            ("/mcp logout remote", "connecting"),
            ("/mcp show proj", "rejected"),
            ("/mcp remove proj", "rejected"),
            ("/mcp approve proj", "rejected"),
            ("/mcp reject proj", "rejected"),
        ]
        .iter()
        .map(|(full, description)| (full.to_string(), description.to_string()))
        .collect()
    }

    fn open(text: &str) -> McpArgs {
        McpArgs::open(text, verbs(), Instant::now()).expect("a /mcp line")
    }

    fn plain(rows: Vec<String>) -> Vec<String> {
        rows.into_iter()
            .map(|row| row.replace(MARKED, "").replace(DIM, "").replace(RESET, ""))
            .collect()
    }

    #[test]
    fn a_mcp_line_is_one_after_a_space_and_any_leading_blanks() {
        assert_eq!(query_of("/mcp "), Some((5, "")));
        assert_eq!(query_of("/mcp show "), Some((5, "show ")));
        assert_eq!(query_of("/mcp   sh"), Some((5, "sh")));
        assert_eq!(query_of("  /mcp x"), Some((7, "x")));
        for not in [
            "/mcp",
            "/mcpx",
            "/mcp\tshow",
            "/MCP show",
            "say /mcp ",
            "/mc ",
        ] {
            assert_eq!(query_of(not), None, "{not:?}");
        }
    }

    #[test]
    fn the_rows_are_fxs_under_the_argument() {
        // `fx-p5-mcp-verbs-40x120-complete-args`: six verbs in view at column
        // 7, the text column as wide as the widest in view and two more.
        let args = open("/mcp ");
        assert_eq!(
            plain(args.rows(120, 7, 6, PALETTE)),
            [
                "       list    configured servers",
                "       show    connect to a server and list its tools",
                "       add     add a server",
                "       remove  remove a server and its login",
                "       login   sign in to an HTTP server",
                "       logout  sign out of an HTTP server",
            ]
        );
        assert_eq!(
            args.rows(120, 7, 6, PALETTE)[0],
            format!("       {MARKED}list{RESET}    {DIM}configured servers{RESET}")
        );
        assert_eq!(
            args.rows(120, 7, 6, PALETTE)[1],
            format!(
                "       {DIM}show{RESET}    {DIM}connect to a server and list its tools{RESET}"
            )
        );
        // `-complete-show`: a server's lines once the verb has its space,
        // padded blank to the height.
        assert_eq!(
            plain(open("/mcp show ").rows(120, 7, 6, PALETTE)),
            [
                "       show fixture  ready, 1 tool",
                "       show remote   connecting",
                "       show proj     rejected",
                "",
                "",
                "",
            ]
        );
    }

    #[test]
    fn a_verb_matches_before_its_space_and_a_name_after_it_ignoring_case() {
        assert_eq!(open("/mcp lo").chosen(), Some("/mcp login"));
        assert_eq!(open("/mcp LOGO").chosen(), Some("/mcp logout"));
        assert_eq!(open("/mcp show f").chosen(), Some("/mcp show fixture"));
        // `remove web` with only a server whose name begins so.
        let mut entries = verbs();
        entries.push(("/mcp remove web2".into(), "connecting".into()));
        let args = McpArgs::open("/mcp remove web", entries, Instant::now()).unwrap();
        assert_eq!(args.chosen(), Some("/mcp remove web2"));
        // Nothing to show: not up, nothing chosen.
        for none in ["/mcp add two -- x", "/mcp zzz", "/mcp show  fixture"] {
            let args = open(none);
            assert!(!args.visible(), "{none}");
            assert_eq!(args.chosen(), None, "{none}");
        }
    }

    #[test]
    fn the_mark_wraps_and_the_window_follows_it() {
        let mut args = open("/mcp ");
        assert!(args.step_by(-1));
        assert_eq!(args.chosen(), Some("/mcp reject"));
        let rows = plain(args.rows(120, 7, 6, PALETTE));
        assert_eq!(rows[5], "       reject   reject a project server");
        assert_eq!(rows[0], "       add      add a server");
        assert!(args.step_by(1));
        assert_eq!(args.chosen(), Some("/mcp list"));
    }

    #[test]
    fn a_fresh_read_keeps_the_mark_and_says_whether_anything_changed() {
        let now = Instant::now();
        let mut args = McpArgs::open("/mcp show ", verbs(), now).unwrap();
        assert!(!args.due(now));
        assert!(args.due(now + REFRESH));
        assert!(!args.refresh(verbs(), now));
        args.step_by(1);
        let mut entries = verbs();
        entries[10].1 = "retrying".into();
        assert!(args.refresh(entries, now));
        assert_eq!(args.chosen(), Some("/mcp show remote"));
        assert_eq!(
            plain(args.rows(120, 7, 6, PALETTE))[1],
            "       show remote   retrying"
        );
    }

    #[test]
    fn a_read_that_drops_an_earlier_row_keeps_the_mark_on_its_line() {
        // MCP-Vb: kept by index, the mark moved onto the next server's line,
        // and Enter would have run that.
        let now = Instant::now();
        let mut args = McpArgs::open("/mcp remove ", verbs(), now).unwrap();
        assert!(args.step_by(1));
        assert_eq!(args.chosen(), Some("/mcp remove remote"));
        let entries: Vec<(String, String)> = verbs()
            .into_iter()
            .filter(|(full, _)| !full.ends_with(" fixture"))
            .collect();
        assert!(args.refresh(entries, now));
        assert_eq!(args.chosen(), Some("/mcp remove remote"));
        // MCP-Vc: the marked line itself went away -- the mark fell to its
        // index, on a remove nobody marked. Now the list closes for this
        // text, and Enter submits what was typed.
        let entries: Vec<(String, String)> = verbs()
            .into_iter()
            .filter(|(full, _)| !full.contains("remote") && !full.ends_with(" fixture"))
            .collect();
        assert!(args.refresh(entries, now));
        assert_eq!(args.chosen(), None);
        assert!(!args.visible());
        // And stays closed while the text is the same, whatever comes back.
        assert!(args.refresh(verbs(), now));
        assert_eq!(args.chosen(), None);
        assert!(!args.visible());
    }

    #[test]
    fn a_narrow_screen_cuts_the_description_first() {
        let args = open("/mcp show ");
        let rows = plain(args.rows(30, 7, 3, PALETTE));
        assert_eq!(rows[0], "       show fixture  ready, 1 ");
        assert!(rows.iter().all(|row| cells(row) <= 30), "{rows:?}");
        assert_eq!(McpArgs::height(40, 0), 6);
        assert_eq!(McpArgs::height(8, 0), 3);
        assert_eq!(McpArgs::height(5, 0), 1);
    }
}
