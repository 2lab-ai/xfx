//! The `/provider` picker: upstream's provider column, over xfx's providers.
//!
//! Upstream's `/provider` (alias `/setup`) is a columnar picker in the band
//! under the composer: the provider, then for the Gateway how it signs in, the
//! key it uses or the Vercel team (`vercel-labs/fx@c1db9191
//! src/core/app/provider_picker_runtime.zig:1-11`). xfx has the **first column
//! only**. Its rows are [`ProviderId::ALL`], and every later column is upstream
//! sign-in -- an fx login, a Vercel team, a keychain key, a Codex or Grok
//! subscription -- which is gated (zbrain open-loop #88) and therefore shows
//! nothing here rather than a row that leads nowhere. With no column after it,
//! choosing a provider is a whole choice: it commits, exactly as upstream's
//! subscription rows (which have no method column either) commit at once
//! (`provider_picker_runtime.zig:260-265`).
//!
//! # What it looks like
//!
//! `fx-provider-40x120-open`, row for row: the composer reseeded to
//! `/provider `, and between the two rules one row per provider **anchored
//! under the argument** -- column 12 for `/provider `, column 9 for `/setup `
//! (`app_render_runtime.zig:599-628`, `input_presentation.zig:347-372`). The
//! marked row is bold in the hint grey and every other row dim; the active
//! provider carries ` · current` in the dim grey whatever its own brightness
//! (`composePickerOptionRowAnnotated`, `picker_presentation.zig:785-825`). The
//! body is upstream's list-picker budget, at most six rows and padded blank
//! below the last provider so the band keeps its height while the query
//! narrows the list (`activeListPickerReservedRows`, `:634-639`). The hint row
//! under it stays the statusline: every upstream capture of this picker shows
//! `auto · model-a` there, not key help.
//!
//! # What the composer means to it
//!
//! It is a **view of the composer**, like the completion menu: what follows
//! the prefix is the query, typing filters live, and the caret stays in the
//! composer. The prefix is `/provider ` or `/setup `, matched without regard
//! to case after leading blanks and kept as typed (`picker_state.zig:50-57`,
//! `:574-581`); `/login ` is upstream's third spelling and is not one here,
//! because `/login` is the gated command. The query is every byte after the
//! prefix, trimmed of spaces and tabs, and a row survives when its label
//! contains it, case aside (`filterAnnotatedLabels`, `picker_state.zig:491-509`)
//! -- so `/provider llmux x` names no provider and shows
//! `no matching providers`, which is upstream's own row for a provider column
//! with nothing in it (`picker_presentation.zig:892`).
//!
//! The marked row is the first one whenever the text changes, as upstream's is
//! (`resetActiveCompletionIndex`, `picker_state.zig:311-319`) -- not the
//! current provider: upstream re-selects a previous choice only when stepping
//! back into a column (`syncProviderSelection`), and xfx has no column to step
//! back from.

use crate::provider::ProviderId;

use super::picker::{cells, edge_start};
use super::theme::Palette;

/// The canonical prefix, which a bare `/provider` or `/setup` reseeds the
/// composer with (`picker_state.provider_prefix`, `picker_state.zig:54`).
pub(crate) const PROVIDER_PREFIX: &str = "/provider ";

/// Every prefix that opens the picker: upstream's `provider_picker_prefixes`
/// (`picker_state.zig:57`) less `/login `, the gated command.
const PREFIXES: [&str; 2] = [PROVIDER_PREFIX, "/setup "];

/// The most rows the column shows (`list_window.default_max_picker_rows`,
/// `list_window.zig:3`).
const MOST_ROWS: u16 = 6;

/// The band's rows a list picker cannot have (`activeListPickerReservedRows`'
/// `fixed_rows`, `picker_presentation.zig:635`).
const FIXED_ROWS: u16 = 5;

/// What the column says when the query names no provider
/// (`picker_presentation.zig:892`).
const NO_MATCH: &str = "no matching providers";

/// What joins a row's label to its annotation (`picker_annotation_separator`,
/// `picker_presentation.zig:783`).
const SEPARATOR: &str = " \u{b7} ";

/// The annotation the active, credentialed provider carries
/// (`provider_picker_runtime.zig:76-78`).
const CURRENT: &str = "current";

/// Whether `text` is the picker's: a prefix it answers to, after leading
/// blanks.
pub(crate) fn is_query(text: &str) -> bool {
    query_of(text).is_some()
}

/// The prefix as typed, the byte the query starts at, and the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Query<'a> {
    prefix: &'a str,
    token_start: usize,
    query: &'a str,
}

/// Where the picker's argument is in `text`, if `text` is the picker's
/// (`rawProviderPickerQuery`, `picker_state.zig:270-283`).
///
/// Spaces and tabs before the command are skipped, as upstream's
/// `leadingWhitespaceLen` skips them, and counted into `token_start`, because
/// the column anchors under the argument where it really is on the row.
fn query_of(text: &str) -> Option<Query<'_>> {
    let trimmed = text.trim_start_matches([' ', '\t']);
    let lead = text.len() - trimmed.len();
    PREFIXES.iter().find_map(|prefix| {
        // `get` rather than a slice: a prefix-length cut can land inside a
        // multi-byte character of something that is not a prefix at all.
        let head = trimmed.get(..prefix.len())?;
        head.eq_ignore_ascii_case(prefix).then(|| Query {
            prefix: head,
            token_start: lead + prefix.len(),
            query: &trimmed[prefix.len()..],
        })
    })
}

/// One provider's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    id: ProviderId,
    /// The active provider, with a credential that authorizes it.
    current: bool,
}

/// The provider column for one composer text, and which row is marked.
#[derive(Debug)]
pub(crate) struct ProviderPicker {
    /// The composer text this was built for, so a keystroke that did not
    /// change the text does not rebuild it and move the mark back to the top.
    text: String,
    /// The prefix as typed, which Tab writes back (`autocomplete`,
    /// `provider_picker_runtime.zig:207`).
    prefix: String,
    /// Where the argument starts in [`Self::text`], in bytes: what the column
    /// is anchored under.
    token_start: usize,
    /// What was typed after the prefix, trimmed of spaces and tabs.
    query: String,
    /// The providers the query matched, in [`ProviderId::ALL`]'s order.
    /// Empty is a state of its own: the `no matching providers` row.
    rows: Vec<Row>,
    /// An index into [`Self::rows`].
    selected: usize,
    /// The first row in view, as the last move left it (`list_window.zig`'s
    /// window, which scrolls only at an edge).
    window_start: usize,
}

impl ProviderPicker {
    /// The column `text` asks for, or `None` when `text` is not the picker's.
    ///
    /// `current` is the provider that earns ` · current`: the active one, and
    /// only while a credential authorizes it -- `None` for a session whose
    /// provider has nothing to authenticate with, because upstream's row says
    /// `current` only about a provider that would really answer
    /// (`provider_picker_runtime.zig:76-78`).
    pub(crate) fn open(text: &str, current: Option<ProviderId>) -> Option<Self> {
        let query = query_of(text)?;
        let needle = query.query.trim_matches([' ', '\t']).to_ascii_lowercase();
        let rows = ProviderId::ALL
            .iter()
            .filter(|id| needle.is_empty() || id.label().to_ascii_lowercase().contains(&needle))
            .map(|&id| Row {
                id,
                current: Some(id) == current,
            })
            .collect();
        Some(Self {
            text: text.to_string(),
            prefix: query.prefix.to_string(),
            token_start: query.token_start,
            query: needle,
            rows,
            selected: 0,
            window_start: 0,
        })
    }

    /// The composer text this column was built for.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The byte of [`Self::text`] the column is anchored under.
    pub(crate) fn token_start(&self) -> usize {
        self.token_start
    }

    /// The rows a screen `terminal_rows` tall gives the column, with
    /// `reserved` rows of the band taken by something other than the five
    /// every band has (`activeListPickerReservedRows`,
    /// `picker_presentation.zig:634-639`).
    ///
    /// Upstream's `reserved` is the composer's rows past its first; xfx adds
    /// the running turn's row to it, because that row is the band's here and
    /// above the footer upstream -- and a column that took it too would ask
    /// for a band the screen does not have. **Never none**: a screen with no
    /// row to spare still shows one, as upstream's does.
    pub(crate) fn height(terminal_rows: u16, reserved: u16) -> u16 {
        let available = terminal_rows.saturating_sub(FIXED_ROWS.saturating_add(reserved));
        if available == 0 {
            1
        } else {
            available.min(MOST_ROWS)
        }
    }

    /// The column's rows, top first, exactly `height` of them, on a screen
    /// `cols` wide with the column anchored at cell `anchor`.
    ///
    /// `height` is the band's answer rather than this column's own, so the
    /// rows painted and the rows the geometry was solved for are one number.
    pub(crate) fn rows(
        &self,
        cols: u16,
        anchor: u16,
        height: u16,
        palette: Palette,
    ) -> Vec<String> {
        let available = usize::from(cols).saturating_sub(usize::from(anchor));
        let mut painted = Vec::with_capacity(usize::from(height));
        if self.rows.is_empty() {
            painted.push(status_row(NO_MATCH, anchor, available, palette));
        } else {
            let count = self.rows.len();
            let start = edge_start(self.window_start, count, self.selected, height);
            let end = (start + usize::from(height)).min(count);
            for (offset, row) in self.rows[start..end].iter().enumerate() {
                let marked = start + offset == self.selected;
                painted.push(option_row(*row, marked, anchor, available, palette));
            }
        }
        // The rows the column has no provider for: blank, so a query that
        // narrowed it leaves the band the height it was.
        painted.resize(usize::from(height), String::new());
        painted
    }

    /// Moves the mark one row, wrapping at both ends, and says whether there
    /// was a row to move to (`advanceSelection`, `list_window.zig:40-48`).
    pub(crate) fn step(&mut self, down: bool) -> bool {
        let count = self.rows.len();
        if count == 0 {
            return false;
        }
        let current = self.selected % count;
        self.selected = if down {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.window_start = edge_start(self.window_start, count, self.selected, MOST_ROWS);
        true
    }

    /// The provider Enter, Right and Tab take, or `None` when the column is
    /// empty (`selectedLabel`, `provider_picker_runtime.zig:490-502`).
    ///
    /// A query that **is** a label -- `llmux`, `LLMUX`, ` llmux ` -- takes that
    /// row whatever is marked (`exactLabel`, `:576-583`), so `/setup llmux`
    /// and a Return commits llmux however the mark got where it is.
    pub(crate) fn chosen(&self) -> Option<ProviderId> {
        if self.rows.is_empty() {
            return None;
        }
        self.rows
            .iter()
            .find(|row| row.id.label().eq_ignore_ascii_case(&self.query))
            .or_else(|| self.rows.get(self.selected % self.rows.len()))
            .map(|row| row.id)
    }

    /// What Tab puts in the composer: the prefix as typed and the chosen row,
    /// with no space after it -- there is no further column for one to open
    /// (`autocomplete`, `provider_picker_runtime.zig:207`).
    pub(crate) fn completion(&self) -> Option<String> {
        self.chosen()
            .map(|id| format!("{}{}", self.prefix, id.label()))
    }
}

/// One provider's row (`composePickerOptionRowAnnotated`,
/// `picker_presentation.zig:785-825`).
///
/// The label in the row's own style -- bold hint grey when marked, dim
/// otherwise -- and closed, then ` · current` in the dim grey. The annotation
/// is the first thing a narrow screen gives up, and the label is cut to what
/// is left.
fn option_row(row: Row, marked: bool, anchor: u16, available: usize, palette: Palette) -> String {
    if available == 0 {
        return String::new();
    }
    let label = row.id.label();
    let style = if marked {
        // Upstream's `selected_completion_style`, which is the welcome line's
        // brand grey in both modes ([`Palette::subtitle`]).
        palette.subtitle()
    } else {
        palette.dim()
    };
    let annotation = if row.current {
        cells(SEPARATOR) + cells(CURRENT)
    } else {
        0
    };
    let shown = annotation > 0 && available >= cells(label) + annotation;
    let label_cells = if shown {
        available - annotation
    } else {
        available
    };
    let mut out = " ".repeat(usize::from(anchor));
    out.push_str(style);
    out.push_str(clipped(label, label_cells));
    out.push_str(palette.reset());
    if shown {
        out.push_str(palette.dim());
        out.push_str(SEPARATOR);
        out.push_str(CURRENT);
        out.push_str(palette.reset());
    }
    out
}

/// A row that is a sentence rather than an option, dim
/// (`composePickerStatusRowWithProvider`, `picker_presentation.zig:855-900`).
fn status_row(text: &str, anchor: u16, available: usize, palette: Palette) -> String {
    if available == 0 {
        return String::new();
    }
    format!(
        "{}{}{}{}",
        " ".repeat(usize::from(anchor)),
        palette.dim(),
        clipped(text, available),
        palette.reset()
    )
}

/// `text` in at most `budget` cells, by the painter's own measure.
fn clipped(text: &str, budget: usize) -> &str {
    super::frame::clip(text, u16::try_from(budget).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::theme::{Depth, Mode};

    const PALETTE: Palette = Palette {
        mode: Mode::Dark,
        depth: Depth::Ansi256,
    };

    /// `selected_completion_style` (`core/shared/theme.zig:97`): the
    /// capture's `\e[1m\e[38;5;255m`, as one sequence.
    const MARKED: &str = "\u{1b}[1;38;5;255m";

    /// `dim_style` (`core/shared/theme.zig:85`): the capture's `\e[38;5;245m`.
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    fn open(text: &str, current: Option<ProviderId>) -> ProviderPicker {
        ProviderPicker::open(text, current).expect("a provider query")
    }

    fn labels(picker: &ProviderPicker) -> Vec<&'static str> {
        picker.rows.iter().map(|row| row.id.label()).collect()
    }

    #[test]
    fn both_spellings_open_it_after_leading_blanks_and_in_any_case() {
        for (text, prefix, start, query) in [
            ("/provider ", "/provider ", 10, ""),
            ("/setup ", "/setup ", 7, ""),
            ("/provider ll", "/provider ", 10, "ll"),
            ("  \t/setup llmux", "/setup ", 10, "llmux"),
            ("/PROVIDER gw", "/PROVIDER ", 10, "gw"),
            ("/Setup  ", "/Setup ", 7, " "),
        ] {
            assert_eq!(
                query_of(text),
                Some(Query {
                    prefix,
                    token_start: start,
                    query
                }),
                "{text:?}"
            );
        }
        // Not until the space: the bare name is the completion menu's, and
        // Enter on it is the command that reseeds the composer.
        for text in [
            "/provider",
            "/setup",
            "/providers ",
            "/setupx",
            "say /provider ",
            "/login ",
            "",
            "/prov\u{e9}der ",
        ] {
            assert_eq!(query_of(text), None, "{text:?}");
            assert!(!is_query(text), "{text:?}");
        }
    }

    #[test]
    fn the_open_column_is_every_provider_with_the_current_one_annotated() {
        // `fx-provider-40x120-open`: the first row marked whatever is
        // current, the annotation dim on both kinds of row, blank rows under
        // the last provider down to six.
        let picker = open("/provider ", Some(ProviderId::Gateway));
        assert_eq!(labels(&picker), ["gateway", "llmux"]);
        let rows = picker.rows(120, 12, 6, PALETTE);
        let pad = " ".repeat(12);
        assert_eq!(
            rows,
            vec![
                format!("{pad}{MARKED}gateway{RESET}{DIM} \u{b7} current{RESET}"),
                format!("{pad}{DIM}llmux{RESET}"),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]
        );

        // `current` follows the provider, not the mark: an llmux session
        // opens on the Gateway row with llmux annotated
        // (`fx-provider-40x120-down1`'s unmarked `vercel · current`).
        let llmux = open("/setup ", Some(ProviderId::Llmux));
        let pad = " ".repeat(9);
        assert_eq!(
            llmux.rows(120, 9, 6, PALETTE)[..2],
            [
                format!("{pad}{MARKED}gateway{RESET}"),
                format!("{pad}{DIM}llmux{RESET}{DIM} \u{b7} current{RESET}"),
            ]
        );

        // A provider with nothing to authenticate with is current to nobody.
        let none = open("/provider ", None);
        assert!(none
            .rows(120, 12, 6, PALETTE)
            .iter()
            .all(|row| !row.contains(CURRENT)));
    }

    #[test]
    fn typing_filters_the_column_without_regard_to_case() {
        assert_eq!(labels(&open("/provider ll", None)), ["llmux"]);
        assert_eq!(labels(&open("/provider LL", None)), ["llmux"]);
        assert_eq!(labels(&open("/provider  gate \t", None)), ["gateway"]);
        assert_eq!(labels(&open("/provider u", None)), ["llmux"]);
        assert_eq!(labels(&open("/provider a", None)), ["gateway"]);
        assert_eq!(labels(&open("/provider   ", None)), ["gateway", "llmux"]);
        // The marked row is the first match, and it is painted marked.
        let filtered = open("/provider ll", Some(ProviderId::Llmux));
        assert_eq!(
            filtered.rows(120, 12, 6, PALETTE)[0],
            format!(
                "{}{MARKED}llmux{RESET}{DIM} \u{b7} current{RESET}",
                " ".repeat(12)
            )
        );
    }

    #[test]
    fn a_query_that_names_no_provider_is_upstreams_dim_row_and_chooses_nothing() {
        // `fx-provider-40x120-arg-nomatch`, and a second word -- there is no
        // second column for it to belong to.
        for text in ["/provider zz", "/provider llmux x", "/setup gateway extra"] {
            let mut picker = open(text, Some(ProviderId::Gateway));
            assert!(picker.rows.is_empty(), "{text:?}");
            let rows = picker.rows(120, 12, 6, PALETTE);
            assert_eq!(
                rows[0],
                format!("{}{DIM}no matching providers{RESET}", " ".repeat(12))
            );
            assert!(rows[1..].iter().all(String::is_empty), "{rows:?}");
            assert_eq!(picker.chosen(), None);
            assert_eq!(picker.completion(), None);
            assert!(!picker.step(true), "a move with nowhere to go");
        }
    }

    #[test]
    fn the_arrows_wrap_at_both_ends() {
        // `fx-provider-40x120-down1` .. `down3-wrap`, on two rows.
        let mut picker = open("/provider ", None);
        assert_eq!(picker.chosen(), Some(ProviderId::Gateway));
        assert!(picker.step(true));
        assert_eq!(picker.chosen(), Some(ProviderId::Llmux));
        let rows = picker.rows(120, 12, 6, PALETTE);
        assert!(rows[0].starts_with(&format!("{}{DIM}gateway", " ".repeat(12))));
        assert!(rows[1].starts_with(&format!("{}{MARKED}llmux", " ".repeat(12))));
        assert!(picker.step(true));
        assert_eq!(picker.chosen(), Some(ProviderId::Gateway), "down wraps");
        assert!(picker.step(false));
        assert_eq!(picker.chosen(), Some(ProviderId::Llmux), "up wraps");
    }

    #[test]
    fn a_query_that_is_a_label_takes_that_row_whatever_is_marked() {
        let picker = open("/setup LLMUX", None);
        assert_eq!(picker.chosen(), Some(ProviderId::Llmux));
        assert_eq!(picker.completion().as_deref(), Some("/setup llmux"));
        let picker = open("/provider gateway ", None);
        assert_eq!(picker.chosen(), Some(ProviderId::Gateway));
    }

    #[test]
    fn tab_writes_the_prefix_as_typed_and_the_chosen_label() {
        // `fx-provider-40x120-arg-co-tab`: `/provider co` and Tab is
        // `/provider codex`, with no space after it.
        assert_eq!(
            open("/provider ll", None).completion().as_deref(),
            Some("/provider llmux")
        );
        let mut picker = open("/setup ", None);
        assert!(picker.step(true));
        assert_eq!(picker.completion().as_deref(), Some("/setup llmux"));
        assert_eq!(
            open("  /PROVIDER g", None).completion().as_deref(),
            Some("/PROVIDER gateway")
        );
    }

    #[test]
    fn the_budget_is_upstreams_list_picker_budget() {
        // `activeListPickerReservedRows`: five fixed rows and whatever else
        // the band holds off the screen, at most six, never none.
        for (rows, reserved, height) in [
            (40, 0, 6),
            (24, 0, 6),
            (11, 0, 6),
            (10, 0, 5),
            (7, 0, 2),
            (6, 0, 1),
            (5, 0, 1),
            (40, 3, 6),
            (12, 3, 4),
            (7, 1, 1),
        ] {
            assert_eq!(
                ProviderPicker::height(rows, reserved),
                height,
                "{rows} rows, {reserved} reserved"
            );
        }
        // And the rows are exactly that many, however few the providers.
        let picker = open("/provider ", None);
        for height in [1, 2, 5, 6] {
            assert_eq!(
                picker.rows(120, 12, height, PALETTE).len(),
                usize::from(height)
            );
        }
    }

    #[test]
    fn one_row_of_body_scrolls_to_keep_the_mark_in_view() {
        let mut picker = open("/provider ", None);
        assert!(picker.rows(120, 12, 1, PALETTE)[0].contains("gateway"));
        assert!(picker.step(true));
        let rows = picker.rows(120, 12, 1, PALETTE);
        assert_eq!(
            rows,
            vec![format!("{}{MARKED}llmux{RESET}", " ".repeat(12))]
        );
    }

    #[test]
    fn a_narrow_screen_gives_up_the_annotation_and_then_the_label() {
        let picker = open("/provider ", Some(ProviderId::Gateway));
        // Room for `gateway` and not for ` · current`: the label alone.
        let rows = picker.rows(24, 12, 2, PALETTE);
        assert_eq!(rows[0], format!("{}{MARKED}gateway{RESET}", " ".repeat(12)));
        // Room for five cells: the label is cut, by the painter's measure.
        let rows = picker.rows(17, 12, 2, PALETTE);
        assert_eq!(rows[0], format!("{}{MARKED}gatew{RESET}", " ".repeat(12)));
        // An anchor at or past the edge has nothing to paint.
        assert!(picker.rows(12, 12, 2, PALETTE).iter().all(String::is_empty));
        assert!(picker.rows(10, 12, 2, PALETTE).iter().all(String::is_empty));
        // Every row fits the screen it was built for.
        for cols in 1..40 {
            for row in picker.rows(cols, 12, 2, PALETTE) {
                assert!(cells(&row) <= usize::from(cols), "{cols}: {row:?}");
            }
        }
    }
}
