//! `/model`'s inline column picker: the model, then its reasoning effort,
//! chosen under the composer as they are typed.
//!
//! Upstream's (`vercel-labs/fx@c1db9191
//! src/core/app/input_completion_runtime.zig:1029-1360`, the stage machine in
//! `src/core/input/picker_state.zig:205-487`): typing `/model ` opens a column
//! of the catalog's models **under the argument** (`fx-model-40x120-space`),
//! the text after it filters them, Enter on a model opens a second column
//! under the next token -- `default`, then the levels the catalog lists for
//! that model, in its order (`fx-model-40x120-gpt-enter`) -- and Enter there
//! opens a third, the speed -- `normal`, then `fast` and `ultrafast` where the
//! model offers them -- for a model that offers Fast, or makes the change.
//! Left steps back a column; Tab writes the marked row into the composer;
//! Escape puts the column away until the text stops being a `/model` line.
//!
//! **A view of the composer, not a second place to type.** Every stage is
//! read back out of the text ([`Column::query`]): the model stage is
//! everything after `/model `, and the effort stage exists only while the text
//! still spells the model it was opened for. What the column remembers between
//! keystrokes is the little the text cannot say -- which stage, which model is
//! pending, which row is marked -- and an edit that is not typing at the end
//! of a later stage throws the stage away ([`Column::edited`]), as upstream's
//! picker policy does (`shouldPreserveModelPickerInsert`, `:1308-1316`).
//!
//! # The speed column, and when there is one
//!
//! Upstream's rules, which are not symmetric (`submitModelPicker`,
//! `beginModelPickerOptions`, `advanceModelPickerOnSpace`, `:1123-1258`):
//!
//! - **Enter on a level opens it only for a model that offers Fast**
//!   (`:1183`), with `fast` marked. A model that offers Ultra and not Fast
//!   commits from its effort column, keeping the session's Fast as far as the
//!   model offers it -- which is not at all -- so Ultra cannot be chosen there.
//! - A model with no levels and a speed opens it straight from the model
//!   column, the composer reading `/model <id> auto `.
//! - A space after a level spelled exactly opens it for a model that offers
//!   either, `fast` marked only where Fast is offered.
//! - A model with neither commits from the model column at once.
//!
//! What is marked is a speed, not a row: an index into all three
//! ([`Speed::ALL`], `model_picker_fast_index`), shown on the row that names
//! it, or the first row when the filter hid it (`speedPickerIndex`,
//! `:1396-1404`). The footer previews it from the moment the effort column
//! opens, which is the `⚡︎` a model with Fast shows before its speed column
//! does (`fx-model-column-40x120-gpt-enter`).
//!
//! # `/effort `, the effort column on its own
//!
//! fx v0.0.16 (`vercel-labs/fx@481104fd`, commit `04a5e3af`) opens the effort
//! column for an `/effort ` line too ([`Command::Effort`];
//! `rawModelPickerQuery`, `picker_state.zig:298-307@481104fd`): the levels of
//! **the model in force**, whatever a `/model` flow has pending
//! (`effortPickerModel`, `input_completion_runtime.zig:1449-1459`;
//! `app_render_runtime.zig:542-546`), and the same levels `/effort`'s menu and
//! argument take ([`super::effort_menu::offered`]). It is one column with no
//! other: Left is the caret's (`stepBackModelPicker`, `:1385`), a space goes
//! nowhere (`advanceModelPickerOnSpace`, `:1221`), Tab writes
//! `/effort <level>` (`:1168-1175`), and Enter applies the level -- or, with
//! none to apply, leaves the line to the `/effort` command, which says why
//! (`submitEffortPicker`, `:1301-1310`). A model with no levels has no column
//! at all, though the line is still the column's (`app_render_runtime.zig:750`).
//! A space after a bare `/effort` opens it on its first row, as any edit puts
//! the mark there; Tab on a bare `/effort` opens it on the level in use
//! (`openCurrentEffortPicker`, `:1312-1328`).

use crate::provider::effort::{Effort, Speed};
use crate::provider::model::CatalogEntry;

use super::effort_menu;
use super::picker::{edge_start, ellipsized};
use super::theme::Palette;

/// What a `/model` line begins with once it has an argument
/// (`rawModelPickerQuery`, `picker_state.zig:244-249`): matched ignoring case,
/// after any leading blanks.
const PREFIX: &str = "/model ";

/// What an `/effort` line begins with once it has an argument
/// (`effort_prefix`, `picker_state.zig:52@481104fd`), read the same way --
/// and first (`:302`).
pub(crate) const EFFORT_PREFIX: &str = "/effort ";

/// The most rows the column shows at once (`default_max_picker_rows`,
/// `list_window.zig:3`).
pub(crate) const MOST_ROWS: u16 = 6;

/// The most models one filter lists (`modelPickerCompletions` fills a
/// 32-slot buffer, `input_completion_runtime.zig:1029`).
const MOST_MODELS: usize = 32;

/// Which column is open.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Stage {
    /// The models, filtered by everything after `/model `.
    #[default]
    Model,
    /// The pending model's reasoning levels, filtered by the next token.
    Effort,
    /// The pending model's speeds, filtered by the token after the level.
    Speed,
}

/// The command a column's line belongs to (`ModelPickerCommand`,
/// `picker_state.zig:63-65@481104fd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command<'a> {
    /// A `/model` line: the models, then the pending one's later columns.
    Model,
    /// An `/effort` line: the levels of `model`, the one in force.
    Effort {
        /// The model in force, as the session names it.
        model: &'a str,
    },
}

/// The composer, read as the column reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Query<'a> {
    pub(crate) stage: Stage,
    /// Which command's line it is.
    pub(crate) command: Command<'a>,
    /// The text the open column is filtered by.
    pub(crate) query: &'a str,
    /// Where that text starts in the composer, in bytes: the column is drawn
    /// under it.
    pub(crate) token_start: usize,
}

impl Query<'_> {
    /// Whether it is an `/effort` line's.
    pub(crate) fn effort_command(&self) -> bool {
        matches!(self.command, Command::Effort { .. })
    }
}

/// What the band knows of the catalog, as far as the column cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Known<'a> {
    /// Asked for, or about to be: `loading models...`.
    Loading,
    /// The runtime could not read it: `unable to load models`.
    Failed,
    /// The rows the runtime sent, in the menu's order -- possibly none.
    Rows(&'a [CatalogEntry]),
}

impl<'a> Known<'a> {
    fn rows(self) -> &'a [CatalogEntry] {
        match self {
            Self::Rows(rows) => rows,
            Self::Loading | Self::Failed => &[],
        }
    }

    /// The catalog's row for `id`, spelled exactly.
    pub(crate) fn entry(self, id: &str) -> Option<&'a CatalogEntry> {
        self.rows().iter().find(|entry| entry.id == id)
    }

    /// The catalog's row for `model` as the session names it -- its id or
    /// one of its aliases -- which is how `/effort` finds the model in force
    /// (`Shell::catalog_row`).
    pub(crate) fn row_for(self, model: &str) -> Option<&'a CatalogEntry> {
        self.rows().iter().find(|entry| entry.matches(model))
    }
}

/// What the last change to the composer was, as far as a later stage cares
/// (`PickerPolicy`, `composer_insertion.zig:165-195`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Edit {
    /// A character typed.
    Typed,
    /// A Backspace.
    Backspace,
    /// The column wrote the text itself, and has already set its own state.
    Written,
    /// Anything else: a paste, a kill, an undo, a recall.
    #[default]
    Other,
}

/// What a model chosen in the first column leads to (`beginModelPickerOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Chosen {
    /// The model has levels to choose from: the composer becomes this, and the
    /// effort column is open under its last token.
    Efforts(String),
    /// The model has no levels and a speed to choose: the composer becomes
    /// this -- `/model <id> auto ` -- and the speed column is open under its
    /// last token.
    Speeds(String),
    /// The model has neither: the change is made at once, with the session's
    /// effort and speed as they stand.
    Commit(String),
}

/// What the footer previews while a later column is open
/// (`app_render_runtime.zig:653-668`, `:712-714`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Preview {
    /// The pending model.
    pub(crate) model: String,
    /// The level the text spells or the mark is on.
    pub(crate) effort: Effort,
    /// The speed the text spells or the mark is on -- as asked for, before the
    /// model's row is asked whether it offers it.
    pub(crate) speed: Speed,
}

/// The column's memory between keystrokes.
#[derive(Debug, Clone, Default)]
pub(crate) struct Column {
    stage: Stage,
    /// The model the effort column is for.
    pending: Option<String>,
    model_index: usize,
    model_window: usize,
    effort_index: usize,
    effort_window: usize,
    /// The marked speed, as an index into [`Speed::ALL`] whatever the model
    /// offers (`model_picker_fast_index`).
    speed_index: usize,
    speed_window: usize,
    /// Escape put the column away; it stays away until the text stops being a
    /// `/model` line (`dismissed_until_trigger_change`, `picker_state.zig:156-159`).
    dismissed: bool,
}

impl Column {
    /// The composer read as a column's query, whether or not the column was
    /// put away (`rawModelPickerQuery`, `picker_state.zig:244-277`).
    ///
    /// The effort stage needs the pending model spelled **exactly** after the
    /// prefix, and its query starts after the blanks that follow it; text that
    /// no longer spells it is not an effort query at all.
    ///
    /// An `/effort ` line is read first, whatever is pending: everything after
    /// the prefix, the blanks included, is its query, and `current` -- the
    /// model in force -- is the one whose levels it lists.
    pub(crate) fn query<'t>(&self, text: &'t str, current: &'t str) -> Option<Query<'t>> {
        let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
        let trimmed = &text[lead..];
        if starts_with_ignoring_case(trimmed, EFFORT_PREFIX) {
            return Some(Query {
                stage: Stage::Effort,
                command: Command::Effort { model: current },
                query: &trimmed[EFFORT_PREFIX.len()..],
                token_start: lead + EFFORT_PREFIX.len(),
            });
        }
        if !starts_with_ignoring_case(trimmed, PREFIX) {
            return None;
        }
        match self.stage {
            Stage::Model => Some(Query {
                stage: Stage::Model,
                command: Command::Model,
                query: &trimmed[PREFIX.len()..],
                token_start: lead + PREFIX.len(),
            }),
            Stage::Effort | Stage::Speed => {
                let model = self.pending.as_deref()?;
                let after = trimmed[PREFIX.len()..].strip_prefix(model)?;
                let mut start = PREFIX.len() + model.len() + blanks(after);
                // The speed's query starts after the level's token and the
                // blanks after it, and there is none until that token is
                // there (`modelPickerTokenStart`, `picker_state.zig:556-573`).
                if self.stage == Stage::Speed {
                    let level = &trimmed[start..];
                    if level.is_empty() {
                        return None;
                    }
                    let word = level.find([' ', '\t']).unwrap_or(level.len());
                    start += word + blanks(&level[word..]);
                }
                Some(Query {
                    stage: self.stage,
                    command: Command::Model,
                    query: &trimmed[start..],
                    token_start: lead + start,
                })
            }
        }
    }

    /// [`Self::query`], unless Escape put the column away.
    pub(crate) fn active<'t>(&self, text: &'t str, current: &'t str) -> Option<Query<'t>> {
        if self.dismissed {
            return None;
        }
        self.query(text, current)
    }

    /// The model a later column is open for, if one is.
    pub(crate) fn pending(&self) -> Option<&str> {
        match self.stage {
            Stage::Effort | Stage::Speed => self.pending.as_deref(),
            Stage::Model => None,
        }
    }

    /// Escape: the column goes, and the text stays.
    pub(crate) fn dismiss(&mut self) {
        self.dismissed = true;
    }

    /// Whether there is nothing here an edit could change: the model column,
    /// nothing pending, nothing marked, nothing put away.
    pub(crate) fn at_rest(&self) -> bool {
        self.stage == Stage::Model
            && self.pending.is_none()
            && self.model_index == 0
            && self.model_window == 0
            && !self.dismissed
    }

    /// Takes one change to the composer, `before` to `after`.
    ///
    /// Upstream keeps a later stage only for an insert at the end of the text,
    /// and for a Backspace at the end that leaves the stage's query something
    /// to lose (`shouldPreserveModelPickerInsert`/`Backspace`,
    /// `input_completion_runtime.zig:1308-1316`); **every other edit -- a
    /// paste included -- puts the column back to the models**
    /// (`abandonModelPickerOptions`). Either way the open column's mark goes
    /// back to its first row, because the list under it changed
    /// (`resetActiveCompletionIndex`). A put-away column comes back once the
    /// text is not a `/model` line any more.
    pub(crate) fn edited(&mut self, before: &str, after: &str, edit: Edit, at_end: bool) {
        if before == after || edit == Edit::Written {
            if !shaped(after.trim_start_matches([' ', '\t'])) {
                self.dismissed = false;
            }
            return;
        }
        let kept = self.stage != Stage::Model
            && at_end
            && match edit {
                Edit::Typed => after.starts_with(before),
                // Which model an `/effort ` line lists does not change how
                // it reads, so none is named.
                Edit::Backspace => {
                    before.starts_with(after)
                        && self
                            .query(before, "")
                            .is_some_and(|query| !query.query.is_empty())
                }
                Edit::Written | Edit::Other => false,
            };
        if kept {
            // `resetActiveModelPickerIndex`: the open column's mark only -- a
            // level typed at goes back to `default` with the speed it was
            // offered with, and a speed typed at goes back to `normal`.
            match self.stage {
                Stage::Effort => {
                    self.effort_index = 0;
                    self.effort_window = 0;
                }
                Stage::Speed => {
                    self.speed_index = 0;
                    self.speed_window = 0;
                }
                Stage::Model => {}
            }
        } else {
            self.reset();
        }
        if !shaped(after.trim_start_matches([' ', '\t'])) {
            self.dismissed = false;
        }
    }

    /// Back to the model column, nothing pending (`clearModelPickerFlow`).
    pub(crate) fn reset(&mut self) {
        let dismissed = self.dismissed;
        *self = Self::default();
        self.dismissed = dismissed;
    }

    /// The models `query` lists, in the catalog's order: every id containing
    /// it, ignoring case (`modelCompletions`, `model_cache_runtime.zig:681-695`).
    pub(crate) fn models<'c>(query: &str, catalog: Known<'c>) -> Vec<&'c CatalogEntry> {
        catalog
            .rows()
            .iter()
            .filter(|entry| query.is_empty() || contains_ignoring_case(&entry.id, query))
            .take(MOST_MODELS)
            .collect()
    }

    /// The model `query` names exactly, ignoring case and the blanks round it
    /// (`exactModelCompletion`, `:1149-1160`).
    pub(crate) fn exact_model<'c>(query: &str, catalog: Known<'c>) -> Option<&'c CatalogEntry> {
        let query = query.trim_matches([' ', '\t']);
        if query.is_empty() {
            return None;
        }
        Self::models(query, catalog)
            .into_iter()
            .find(|entry| entry.id.eq_ignore_ascii_case(query))
    }

    /// Every level the effort column of `query`'s line lists before a
    /// filter: the pending model's for a `/model` line, the model in force's
    /// for an `/effort` line (`effortPickerModel`,
    /// `input_completion_runtime.zig:1449-1459@481104fd`) -- the `default`
    /// first either way, and nothing for a model with no levels.
    pub(crate) fn levels(&self, query: &Query<'_>, catalog: Known<'_>) -> Vec<Effort> {
        match query.command {
            Command::Effort { model } => catalog
                .row_for(model)
                .map(effort_menu::offered)
                .unwrap_or_default(),
            Command::Model => self
                .pending
                .as_deref()
                .and_then(|id| catalog.entry(id))
                .map(Effort::options)
                .unwrap_or_default(),
        }
    }

    /// The levels the column offers that `query` matches -- the default
    /// included, matched by the word it is shown as
    /// (`effortCompletionValues`, `:1318-1338`).
    pub(crate) fn efforts(&self, query: &Query<'_>, catalog: Known<'_>) -> Vec<Effort> {
        let typed = query.query.trim_matches([' ', '\t']);
        self.levels(query, catalog)
            .into_iter()
            .filter(|effort| typed.is_empty() || contains_ignoring_case(effort.display(), typed))
            .collect()
    }

    /// The level `query` spells, when the column offers it
    /// (`exactEffortCompletion`, `:1340-1346`) -- spelled as the row spells
    /// it, or as the default in any case.
    pub(crate) fn exact_effort(&self, query: &Query<'_>, catalog: Known<'_>) -> Option<Effort> {
        let effort = Effort::parse(query.query.trim_matches([' ', '\t']))?;
        let offered = match query.command {
            // `/effort`'s own set, which has a `default` only beside a level
            // (`exactEffortCompletion` asks for one, `:1484`).
            Command::Effort { .. } => self.levels(query, catalog).contains(&effort),
            Command::Model => effort.supported_by(catalog.entry(self.pending.as_deref()?)?),
        };
        offered.then_some(effort)
    }

    /// Whether the band shows a column for `query`: always for a `/model`
    /// line, and for an `/effort` line only when the model in force has
    /// levels -- one without has no column, and Enter runs the command,
    /// which says why (`effort_command_without_levels`,
    /// `app_render_runtime.zig:549-552`, `:750@481104fd`).
    pub(crate) fn shows(&self, query: &Query<'_>, catalog: Known<'_>) -> bool {
        !query.effort_command() || !self.levels(query, catalog).is_empty()
    }

    /// The model Enter takes: the one the query spells, else the marked row
    /// (`selectedModelCompletion`, `:1212-1221`).
    pub(crate) fn chosen_model<'c>(
        &self,
        query: &Query<'_>,
        catalog: Known<'c>,
    ) -> Option<&'c CatalogEntry> {
        if let Some(exact) = Self::exact_model(query.query, catalog) {
            return Some(exact);
        }
        let models = Self::models(query.query, catalog);
        (!models.is_empty()).then(|| models[self.model_index % models.len()])
    }

    /// The level Enter takes: the one the query spells, else the marked row
    /// (`selectedEffortCompletion`, `:1348-1359`) -- which is how an
    /// `/effort HIGH` is `high`: no level is spelled `HIGH`, and of the two
    /// rows the filter keeps, ignoring case, the first is marked
    /// (`input_completion_runtime.zig:1491-1503@481104fd`).
    pub(crate) fn chosen_effort(&self, query: &Query<'_>, catalog: Known<'_>) -> Option<Effort> {
        if let Some(exact) = self.exact_effort(query, catalog) {
            return Some(exact);
        }
        self.marked_level(query, catalog)
    }

    /// The marked row among the levels the filter keeps, whatever the text
    /// spells -- what an `/effort` line's footer previews
    /// (`effort_command_preview`, `app_render_runtime.zig:546-584@481104fd`):
    /// `/effort high` with the mark moved to `xhigh` previews `xhigh`, though
    /// Enter would take `high`.
    pub(crate) fn marked_level(&self, query: &Query<'_>, catalog: Known<'_>) -> Option<Effort> {
        let efforts = self.efforts(query, catalog);
        (!efforts.is_empty()).then(|| efforts[self.effort_index % efforts.len()].clone())
    }

    /// Tab on a bare `/effort` (`openCurrentEffortPicker`,
    /// `input_completion_runtime.zig:1312-1328@481104fd`): whatever `/model`
    /// had pending is let go, the mark goes on `current` among `row`'s levels
    /// -- the first, `default`, for one it does not list -- with the window
    /// over it, and the composer becomes the text returned.
    pub(crate) fn open_levels(&mut self, row: Option<&CatalogEntry>, current: &Effort) -> String {
        self.reset();
        let levels = row.map(effort_menu::offered).unwrap_or_default();
        self.effort_index = levels
            .iter()
            .position(|level| level == current)
            .unwrap_or(0);
        self.effort_window = edge_start(0, levels.len(), self.effort_index, MOST_ROWS);
        EFFORT_PREFIX.to_string()
    }

    /// The speeds the pending model offers that `query` matches, in the
    /// column's order (`speedPickerLabels`, `:1377-1394`).
    pub(crate) fn speeds(&self, query: &str, catalog: Known<'_>) -> Vec<Speed> {
        let Some(entry) = self.pending.as_deref().and_then(|id| catalog.entry(id)) else {
            return Vec::new();
        };
        let query = query.trim_matches([' ', '\t']);
        Speed::options(entry)
            .into_iter()
            .filter(|speed| query.is_empty() || contains_ignoring_case(speed.label(), query))
            .collect()
    }

    /// Where the marked speed is among `listed`: on the row that names it, or
    /// the first when the filter hid it (`speedPickerIndex`, `:1396-1404`).
    fn speed_row(&self, listed: &[Speed]) -> usize {
        let marked = Speed::ALL[self.speed_index % Speed::ALL.len()];
        listed
            .iter()
            .position(|speed| *speed == marked)
            .unwrap_or(0)
    }

    /// The speed Enter takes (`selectedSpeedCompletion`, `:1426-1441`): with
    /// nothing listed, none; one spelled out, if it is listed, and none if it
    /// is not; else the marked one.
    pub(crate) fn chosen_speed(&self, query: &Query<'_>, catalog: Known<'_>) -> Option<Speed> {
        let typed = query.query.trim_matches([' ', '\t']);
        let listed = self.speeds(typed, catalog);
        if listed.is_empty() {
            return None;
        }
        if let Some(spelled) = Speed::parse(typed) {
            return listed.contains(&spelled).then_some(spelled);
        }
        Some(listed[self.speed_row(&listed)])
    }

    /// The level a speed column was opened with: the one at the effort
    /// column's mark (`reasoningEffortAtIndex`), `default` for a model with
    /// none.
    pub(crate) fn speed_effort(&self, catalog: Known<'_>) -> Effort {
        self.pending
            .as_deref()
            .and_then(|id| catalog.entry(id))
            .and_then(|entry| Effort::options(entry).into_iter().nth(self.effort_index))
            .unwrap_or_default()
    }

    /// Opens what comes after `entry` (`beginModelPickerOptions`,
    /// `:1227-1258`): its levels, with the default marked; or, for a model
    /// with none, its speeds; or -- for a model with neither -- nothing, and
    /// the change is made. Fast is marked for a model that offers it, which
    /// the footer previews from here on.
    pub(crate) fn choose(&mut self, entry: &CatalogEntry) -> Chosen {
        let levels = !Effort::options(entry).is_empty();
        if !levels && !Speed::chosen_for(entry) {
            self.reset();
            return Chosen::Commit(entry.id.clone());
        }
        self.open(
            entry,
            if levels { Stage::Effort } else { Stage::Speed },
            0,
            Speed::of(entry.facts.fast, false),
        );
        if levels {
            Chosen::Efforts(format!("{PREFIX}{} ", entry.id))
        } else {
            Chosen::Speeds(format!("{PREFIX}{} {} ", entry.id, Effort::Auto.label()))
        }
    }

    /// Enter on `effort` in the effort column (`submitModelPicker`,
    /// `:1174-1191`): the speed column, with `fast` marked, for a model that
    /// offers Fast -- the composer becomes the text returned -- and `None`
    /// for one that does not, whose change is made with that level.
    pub(crate) fn chose_effort(&mut self, entry: &CatalogEntry, effort: &Effort) -> Option<String> {
        if !entry.facts.fast {
            return None;
        }
        Some(self.open_speeds(entry, effort, Speed::Fast))
    }

    /// A space after a level spelled exactly (`advanceModelPickerOnSpace`,
    /// `:1133-1144`): the speed column for a model that offers either speed,
    /// with `fast` marked where it is offered, and `None` -- the space does
    /// nothing -- for one that offers neither.
    pub(crate) fn advance_effort(
        &mut self,
        entry: &CatalogEntry,
        effort: &Effort,
    ) -> Option<String> {
        if !Speed::chosen_for(entry) {
            return None;
        }
        Some(self.open_speeds(entry, effort, Speed::of(entry.facts.fast, false)))
    }

    /// Opens the speed column after `effort`, with `marked` marked.
    fn open_speeds(&mut self, entry: &CatalogEntry, effort: &Effort, marked: Speed) -> String {
        let index = level_index(entry, effort);
        self.open(entry, Stage::Speed, index, marked);
        format!("{PREFIX}{} {} ", entry.id, effort.label())
    }

    /// `beginModelPickerFlow` (`picker_state.zig:399-419`): `entry` pending
    /// at `stage`, the level at `effort_index` and `speed` marked, every
    /// window at its top.
    fn open(&mut self, entry: &CatalogEntry, stage: Stage, effort_index: usize, speed: Speed) {
        self.stage = stage;
        self.pending = Some(entry.id.clone());
        self.effort_index = effort_index;
        self.effort_window = 0;
        self.speed_index = speed.index();
        self.speed_window = 0;
    }

    /// Left: back one column (`stepBackModelPicker`, `:1263-1287`), and the
    /// text the composer goes back to. From the speeds, the levels of a model
    /// that has them -- the level it was opened with still marked, and the
    /// session's own speed marked for the footer, as upstream passes its
    /// `fast_mode` and `ultrafast_mode` there -- and the model column for one
    /// that has none; from the levels, the model column: the model spelled
    /// out, with the mark on it. `None` from the model column, where Left
    /// moves the caret.
    pub(crate) fn step_back(&mut self, catalog: Known<'_>, session: Speed) -> Option<String> {
        if self.stage == Stage::Model {
            return None;
        }
        if self.stage == Stage::Speed {
            let entry = catalog.entry(self.pending.as_deref()?)?;
            let levels = Effort::options(entry);
            if !levels.is_empty() {
                let index = self.effort_index % levels.len();
                self.open(entry, Stage::Effort, index, session);
                return Some(format!("{PREFIX}{} ", entry.id));
            }
        }
        let model = self.pending.take()?;
        self.reset();
        // `syncModelCompletionSelection`: the mark on the model the text now
        // spells, among the models it lists.
        let models = Self::models(&model, catalog);
        self.model_index = models
            .iter()
            .position(|entry| entry.id == model)
            .unwrap_or(0);
        self.model_window = edge_start(0, models.len(), self.model_index, MOST_ROWS);
        Some(format!("{PREFIX}{model}"))
    }

    /// Tab: the marked row written into the composer
    /// (`autocompleteModelPickerSelection`, `:1085-1121`) -- the model's id,
    /// with the column still on the models; or the level after the model, with
    /// the column still on the levels and the mark on it. On an `/effort`
    /// line, the level after `/effort ` as it is shown -- `default` for the
    /// default -- and, the text being replaced, the mark back on the first
    /// row the line now lists (`:1168-1175@481104fd`; `replace` clears the
    /// flow, `composer_text_replacement.zig:82`).
    pub(crate) fn complete(&mut self, query: &Query<'_>, catalog: Known<'_>) -> Option<String> {
        if query.effort_command() {
            let effort = self.chosen_effort(query, catalog)?;
            self.reset();
            return Some(format!("{EFFORT_PREFIX}{}", effort.display()));
        }
        match query.stage {
            Stage::Model => {
                let id = self.chosen_model(query, catalog)?.id.clone();
                Some(format!("{PREFIX}{id}"))
            }
            Stage::Effort => {
                let effort = self.chosen_effort(query, catalog)?;
                let entry = catalog.entry(self.pending.as_deref()?)?;
                let index = level_index(entry, &effort);
                self.open(
                    entry,
                    Stage::Effort,
                    index,
                    Speed::of(entry.facts.fast, false),
                );
                Some(format!("{PREFIX}{} {}", entry.id, effort.label()))
            }
            Stage::Speed => {
                let speed = self.chosen_speed(query, catalog)?;
                let entry = catalog.entry(self.pending.as_deref()?)?;
                let effort = self.speed_effort(catalog);
                let index = level_index(entry, &effort);
                self.open(entry, Stage::Speed, index, speed);
                Some(format!(
                    "{PREFIX}{} {} {}",
                    entry.id,
                    effort.label(),
                    speed.label()
                ))
            }
        }
    }

    /// Up or Down in the open column, wrapping (`advanceSelection`,
    /// `list_window.zig:38-47`). Says whether the mark moved.
    pub(crate) fn step(&mut self, query: &Query<'_>, catalog: Known<'_>, down: bool) -> bool {
        if query.stage == Stage::Speed {
            // Walked over the rows listed, and kept as the speed the new row
            // names (`navigateModelPicker`, `:1001-1013`).
            let listed = self.speeds(query.query, catalog);
            if listed.is_empty() {
                return false;
            }
            let mut row = self.speed_row(&listed);
            advance(&mut row, &mut self.speed_window, listed.len(), down);
            self.speed_index = listed[row].index();
            return true;
        }
        let count = match query.stage {
            Stage::Model => Self::models(query.query, catalog).len(),
            Stage::Effort | Stage::Speed => self.efforts(query, catalog).len(),
        };
        if count == 0 {
            return false;
        }
        let (index, window) = match query.stage {
            Stage::Model => (&mut self.model_index, &mut self.model_window),
            Stage::Effort | Stage::Speed => (&mut self.effort_index, &mut self.effort_window),
        };
        advance(index, window, count, down);
        true
    }

    /// What the footer previews while a later column is open
    /// (`app_render_runtime.zig:653-668`, `pendingPickerEffort` and
    /// `pendingPickerSpeed` at `:862-916`): the pending model; the level the
    /// text spells while the effort column is open, else the marked one; and
    /// the speed the text spells -- any of the three -- while the speed column
    /// is open, else the marked one, which the effort column already has.
    ///
    /// Nothing for an `/effort` line: whatever `/model` has pending, its
    /// footer is the model in force's, with only the level previewed
    /// ([`Self::marked_level`]).
    pub(crate) fn preview(&self, query: &Query<'_>, catalog: Known<'_>) -> Option<Preview> {
        if query.effort_command() {
            return None;
        }
        let model = self.pending()?;
        let marked = || {
            catalog
                .entry(model)
                .and_then(|entry| Effort::options(entry).into_iter().nth(self.effort_index))
                .unwrap_or_default()
        };
        let effort = match query.stage {
            Stage::Effort => self.exact_effort(query, catalog).unwrap_or_else(marked),
            Stage::Speed | Stage::Model => marked(),
        };
        let speed = match query.stage {
            Stage::Speed => Speed::parse(query.query.trim_matches([' ', '\t'])),
            Stage::Effort | Stage::Model => None,
        }
        .unwrap_or(Speed::ALL[self.speed_index % Speed::ALL.len()]);
        Some(Preview {
            model: model.to_string(),
            effort,
            speed,
        })
    }

    /// The column's rows, exactly `height` of them: the open column's items
    /// from `anchor` cells in, the marked one in the bold hint grey and the
    /// rest dim (`composePickerOptionRowAnnotated`,
    /// `picker_presentation.zig:785-830`), or the one dim line that says why
    /// there are none (`composePickerStatusRow`, `:850-891`), and blank rows to
    /// the height.
    pub(crate) fn rows(
        &self,
        query: &Query<'_>,
        catalog: Known<'_>,
        anchor: u16,
        cols: u16,
        height: u16,
        palette: Palette,
    ) -> Vec<String> {
        let (labels, marked, window): (Vec<String>, usize, usize) = match query.stage {
            Stage::Model => (
                Self::models(query.query, catalog)
                    .into_iter()
                    .map(|entry| entry.id.clone())
                    .collect(),
                self.model_index,
                self.model_window,
            ),
            Stage::Effort => (
                self.efforts(query, catalog)
                    .iter()
                    .map(|effort| effort.display().to_string())
                    .collect(),
                self.effort_index,
                self.effort_window,
            ),
            Stage::Speed => {
                let listed = self.speeds(query.query, catalog);
                (
                    listed
                        .iter()
                        .map(|speed| speed.label().to_string())
                        .collect(),
                    self.speed_row(&listed),
                    self.speed_window,
                )
            }
        };
        let indent = usize::from(anchor);
        let room = usize::from(cols).saturating_sub(indent);
        let mut rows = Vec::with_capacity(usize::from(height));
        if room > 0 {
            if labels.is_empty() {
                let status = match (query.stage, catalog) {
                    (Stage::Model, Known::Loading) => "loading models...",
                    (Stage::Model, Known::Failed) => "unable to load models",
                    (Stage::Model, Known::Rows(_)) => "no matching models",
                    (Stage::Effort, _) => "no matching effort",
                    (Stage::Speed, _) => "no matching mode",
                };
                rows.push(format!(
                    "{}{}{}{}",
                    " ".repeat(indent),
                    palette.dim(),
                    ellipsized(status, room),
                    palette.reset()
                ));
            } else {
                let marked = marked % labels.len();
                let start = edge_start(window, labels.len(), marked, height);
                for (offset, label) in labels
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(usize::from(height))
                {
                    rows.push(format!(
                        "{}{}{}{}",
                        " ".repeat(indent),
                        if offset == marked {
                            palette.subtitle()
                        } else {
                            palette.dim()
                        },
                        ellipsized(label, room),
                        palette.reset()
                    ));
                }
            }
        }
        rows.resize(usize::from(height), String::new());
        rows
    }
}

/// How many rows the band gives the column on a screen `terminal_rows` tall:
/// six, or what is left over the five the band always needs
/// (`activeListPickerReservedRows`, `picker_presentation.zig:634-639`).
pub(crate) fn height(terminal_rows: u16) -> u16 {
    terminal_rows.saturating_sub(5).clamp(1, MOST_ROWS)
}

/// Whether the composer's `text` is a `/model` or an `/effort` line with an
/// argument position, whatever blanks lead it -- the column's, open or put
/// away, and so never an `@` word or a slash word as well
/// (`inlinePickerTriggerKind`, `picker_state.zig:287-296@481104fd`). A prefix
/// check, whatever the length of `text`.
pub(crate) fn shaped_line(text: &str) -> bool {
    shaped(text.trim_start_matches([' ', '\t']))
}

/// Whether `trimmed` -- the composer without its leading blanks -- is a
/// `/model` or an `/effort` line with an argument position.
fn shaped(trimmed: &str) -> bool {
    starts_with_ignoring_case(trimmed, PREFIX) || starts_with_ignoring_case(trimmed, EFFORT_PREFIX)
}

/// Whether the composer's `text` is a bare `/effort`, any case, after any
/// leading blanks and with nothing after it (`isBareEffortCommandAtCursor`,
/// `picker_state.zig:540-544@481104fd`, the caret's half asked by the
/// caller): what Tab opens the `/effort ` column from on the level in use.
pub(crate) fn bare_effort(text: &str) -> bool {
    text.trim_start_matches([' ', '\t'])
        .eq_ignore_ascii_case(EFFORT_PREFIX.trim_end())
}

/// Whether `text` begins with `prefix`, ASCII case ignored.
fn starts_with_ignoring_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// How many blanks `text` begins with, in bytes (`skipPickerSpaces`).
fn blanks(text: &str) -> usize {
    text.len() - text.trim_start_matches([' ', '\t']).len()
}

/// Where `effort` is among the levels `entry` offers, `default` first
/// (`reasoningEffortIndex`); the default's place for one it does not offer.
fn level_index(entry: &CatalogEntry, effort: &Effort) -> usize {
    Effort::options(entry)
        .iter()
        .position(|offered| offered == effort)
        .unwrap_or(0)
}

/// One step of a mark over `count` rows, wrapping, and the window kept over
/// it (`advanceSelection`, `list_window.zig:38-47`).
fn advance(index: &mut usize, window: &mut usize, count: usize, down: bool) {
    let current = *index % count;
    *index = match (down, current) {
        (true, last) if last + 1 == count => 0,
        (true, current) => current + 1,
        (false, 0) => count - 1,
        (false, current) => current - 1,
    };
    *window = edge_start(*window, count, *index, MOST_ROWS);
}

/// Whether the composer's `text` is an `/effort` line with an argument
/// position, whatever blanks lead it: one whose column lists the levels of
/// the model in force. A prefix check, whatever the length of `text`.
pub(crate) fn effort_line(text: &str) -> bool {
    starts_with_ignoring_case(text.trim_start_matches([' ', '\t']), EFFORT_PREFIX)
}

/// Whether `needle` occurs in `haystack`, ASCII case ignored
/// (`text_utils.containsIgnoreCase`).
fn contains_ignoring_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::model::ModelFacts;
    use crate::tui::theme::Mode;

    const PALETTE: Palette = Palette { mode: Mode::Dark };

    fn entry(id: &str, efforts: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            max_context: None,
            facts: ModelFacts::default(),
        }
    }

    /// `entry`, offering Fast and Ultra as told.
    fn sped(id: &str, efforts: &[&str], fast: bool, ultrafast: bool) -> CatalogEntry {
        let mut entry = entry(id, efforts);
        entry.facts.fast = fast;
        entry.facts.ultrafast = ultrafast;
        entry
    }

    /// The research harness's catalog, in the menu's order
    /// (`scripts/fxcap/scenarios/models.json`, `fx-model-40x120-space`):
    /// astra offers Fast (its `priority` tier) and Ultra, opus Fast (its
    /// `fast_options` toggle), and the other two neither.
    fn catalog() -> Vec<CatalogEntry> {
        vec![
            sped(
                "openai/gpt-6-astra",
                &["low", "medium", "high", "xhigh"],
                true,
                true,
            ),
            sped(
                "anthropic/claude-opus-5",
                &["low", "medium", "high", "max"],
                true,
                false,
            ),
            entry("fake/model-a", &[]),
            entry("xai/grok-4.7", &[]),
        ]
    }

    /// The column shapes the research catalog has no row for: levels and Ultra
    /// alone, Ultra alone, Fast alone, and levels with no speed.
    fn shapes() -> Vec<CatalogEntry> {
        vec![
            sped("openai/ultra-leveled", &["low", "high"], false, true),
            sped("openai/ultra-only", &[], false, true),
            sped("acme/fast-only", &[], true, false),
            entry("zai/glm-5.2", &["low", "high"]),
        ]
    }

    /// The rows without their colours.
    fn plain(rows: &[String]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let mut out = String::new();
                let mut chars = row.chars();
                while let Some(character) = chars.next() {
                    if character == '\u{1b}' {
                        for next in chars.by_ref() {
                            if next.is_ascii_alphabetic() {
                                break;
                            }
                        }
                        continue;
                    }
                    out.push(character);
                }
                out.trim_end().to_string()
            })
            .collect()
    }

    #[test]
    fn the_model_column_is_everything_after_model_and_a_space() {
        let column = Column::default();
        assert_eq!(column.query("/model", ""), None, "no argument position yet");
        assert_eq!(
            column.query("/model ", ""),
            Some(Query {
                stage: Stage::Model,
                command: Command::Model,
                query: "",
                token_start: 7
            })
        );
        assert_eq!(
            column
                .query("  /MODEL gpt", "")
                .map(|query| (query.query, query.token_start)),
            Some(("gpt", 9))
        );
        assert_eq!(column.query("/models x", ""), None);
        assert_eq!(column.query("say /model x", ""), None);
    }

    #[test]
    fn typing_filters_the_models_by_id_ignoring_case() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let ids = |query: &str| {
            Column::models(query, known)
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("").len(), 4);
        assert_eq!(ids("GPT"), ["openai/gpt-6-astra"]);
        assert!(ids("nothing").is_empty());
        assert_eq!(
            Column::exact_model(" FAKE/model-a ", known).map(|entry| entry.id.as_str()),
            Some("fake/model-a")
        );
        assert_eq!(Column::exact_model("fake", known), None);
    }

    #[test]
    fn a_model_with_levels_opens_the_effort_column_and_one_without_commits() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        assert_eq!(
            column.choose(&rows[0]),
            Chosen::Efforts("/model openai/gpt-6-astra ".to_string())
        );
        let text = "/model openai/gpt-6-astra ";
        let query = column.query(text, "").expect("an effort query");
        assert_eq!(
            (query.stage, query.query, query.token_start),
            (Stage::Effort, "", 26)
        );
        assert_eq!(
            column
                .efforts(&query, known)
                .iter()
                .map(Effort::display)
                .collect::<Vec<_>>(),
            ["default", "low", "medium", "high", "xhigh"]
        );
        // The default is marked (`reasoningEffortIndex(.auto)`).
        assert_eq!(column.chosen_effort(&query, known), Some(Effort::Auto));
        // A level spelled out is taken whatever is marked; one the model does
        // not list is not.
        let typed = column
            .query("/model openai/gpt-6-astra high", "")
            .expect("query");
        assert_eq!(
            column.chosen_effort(&typed, known),
            Some(Effort::Named("high".to_string()))
        );
        let unknown = column
            .query("/model openai/gpt-6-astra max", "")
            .expect("query");
        assert_eq!(
            column.chosen_effort(&unknown, known),
            None,
            "no matching effort"
        );

        let mut column = Column::default();
        assert_eq!(
            column.choose(&rows[2]),
            Chosen::Commit("fake/model-a".to_string())
        );
        assert_eq!(column.pending(), None);
    }

    #[test]
    fn the_effort_column_follows_typing_at_the_end_and_nothing_else() {
        let rows = catalog();
        let mut column = Column::default();
        column.choose(&rows[0]);
        let opened = "/model openai/gpt-6-astra ";
        // Typed at the end: kept.
        column.edited(opened, "/model openai/gpt-6-astra h", Edit::Typed, true);
        assert_eq!(column.pending(), Some("openai/gpt-6-astra"));
        // A Backspace that leaves the query something to lose: kept.
        column.edited("/model openai/gpt-6-astra h", opened, Edit::Backspace, true);
        assert_eq!(column.pending(), Some("openai/gpt-6-astra"));
        // A Backspace with nothing left to lose goes back to the models.
        column.edited(opened, "/model openai/gpt-6-astra", Edit::Backspace, true);
        assert_eq!(column.pending(), None);
        // A paste at the end is not typing.
        column.choose(&rows[0]);
        column.edited(opened, "/model openai/gpt-6-astra low", Edit::Other, true);
        assert_eq!(column.pending(), None);
        // Nor is typing anywhere but the end.
        column.choose(&rows[0]);
        column.edited(opened, "/model openai/gpt-6-astra l", Edit::Typed, false);
        assert_eq!(column.pending(), None);
    }

    #[test]
    fn left_steps_back_to_the_model_with_the_mark_on_it() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        assert_eq!(
            column.step_back(known, Speed::Normal),
            None,
            "Left moves the caret here"
        );
        column.choose(&rows[1]);
        assert_eq!(
            column.step_back(known, Speed::Normal),
            Some("/model anthropic/claude-opus-5".to_string())
        );
        let query = column
            .query("/model anthropic/claude-opus-5", "")
            .expect("query");
        assert_eq!(query.stage, Stage::Model);
        assert_eq!(
            column
                .chosen_model(&query, known)
                .map(|entry| entry.id.as_str()),
            Some("anthropic/claude-opus-5")
        );
    }

    #[test]
    fn tab_writes_the_marked_row_into_the_composer() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        let query = column.query("/model gr", "").expect("query");
        assert_eq!(
            column.complete(&query, known),
            Some("/model xai/grok-4.7".to_string())
        );
        column.choose(&rows[0]);
        let query = column
            .query("/model openai/gpt-6-astra ", "")
            .expect("query");
        assert!(column.step(&query, known, true));
        assert!(column.step(&query, known, true));
        assert_eq!(
            column.complete(&query, known),
            Some("/model openai/gpt-6-astra medium".to_string())
        );
        assert_eq!(column.pending(), Some("openai/gpt-6-astra"));
    }

    #[test]
    fn the_arrows_wrap_at_both_ends() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        let query = column.query("/model ", "").expect("query");
        assert!(column.step(&query, known, false));
        assert_eq!(
            column
                .chosen_model(&query, known)
                .map(|entry| entry.id.as_str()),
            Some("xai/grok-4.7")
        );
        assert!(column.step(&query, known, true));
        assert_eq!(
            column
                .chosen_model(&query, known)
                .map(|entry| entry.id.as_str()),
            Some("openai/gpt-6-astra")
        );
        let none = column.query("/model zzz", "").expect("query");
        assert!(!column.step(&none, known, true), "nothing to move over");
    }

    #[test]
    fn escape_puts_the_column_away_until_the_line_stops_being_a_model_line() {
        let mut column = Column::default();
        column.dismiss();
        assert_eq!(column.active("/model op", ""), None);
        assert!(column.query("/model op", "").is_some());
        column.edited("/model op", "/model opu", Edit::Typed, true);
        assert_eq!(column.active("/model opu", ""), None, "still the same line");
        column.edited("/model opu", "", Edit::Other, true);
        assert!(column.active("/model ", "").is_some(), "a new /model line");
    }

    #[test]
    fn the_rows_are_drawn_under_the_argument_six_high() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let column = Column::default();
        let query = column.query("/model ", "").expect("query");
        let drawn = column.rows(&query, known, 9, 120, 6, PALETTE);
        assert_eq!(
            plain(&drawn),
            [
                "         openai/gpt-6-astra",
                "         anthropic/claude-opus-5",
                "         fake/model-a",
                "         xai/grok-4.7",
                "",
                ""
            ]
        );
        // What it says in place of rows (`picker_presentation.zig:883-891`).
        for (known, said) in [
            (Known::Loading, "loading models..."),
            (Known::Failed, "unable to load models"),
            (Known::Rows(&rows), "no matching models"),
        ] {
            let query = column.query("/model zzz", "").expect("query");
            assert_eq!(
                plain(&column.rows(&query, known, 9, 120, 6, PALETTE))[0],
                format!("         {said}")
            );
        }
        let mut efforts = Column::default();
        efforts.choose(&rows[0]);
        let query = efforts
            .query("/model openai/gpt-6-astra zzz", "")
            .expect("query");
        assert_eq!(
            plain(&efforts.rows(&query, known, 28, 120, 6, PALETTE))[0],
            format!("{}no matching effort", " ".repeat(28))
        );
    }

    #[test]
    fn the_footer_previews_the_pending_model_and_the_marked_level() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        let query = column.query("/model ", "").expect("query");
        assert_eq!(column.preview(&query, known), None, "nothing pending yet");
        column.choose(&rows[0]);
        let query = column
            .query("/model openai/gpt-6-astra ", "")
            .expect("query");
        let preview = |effort: Effort, speed: Speed| {
            Some(Preview {
                model: "openai/gpt-6-astra".to_string(),
                effort,
                speed,
            })
        };
        // Fast marked from the moment the effort column opens, for a model
        // that offers it: the `⚡︎` of `fx-model-column-40x120-gpt-enter`.
        assert_eq!(
            column.preview(&query, known),
            preview(Effort::Auto, Speed::Fast)
        );
        column.step(&query, known, true);
        assert_eq!(
            column.preview(&query, known),
            preview(Effort::Named("low".to_string()), Speed::Fast)
        );
        // Typing at the level keeps the speed marked (`resetActiveModelPickerIndex`
        // resets only the open column's mark).
        column.edited(
            "/model openai/gpt-6-astra ",
            "/model openai/gpt-6-astra hi",
            Edit::Typed,
            true,
        );
        let typed = column
            .query("/model openai/gpt-6-astra hi", "")
            .expect("query");
        assert_eq!(
            column.preview(&typed, known),
            preview(Effort::Auto, Speed::Fast)
        );
        // A model with no Fast marks `normal`.
        let mut opus = Column::default();
        opus.choose(&shapes()[0]);
        let shaped = shapes();
        let query = opus
            .query("/model openai/ultra-leveled ", "")
            .expect("query");
        assert_eq!(
            opus.preview(&query, Known::Rows(&shaped))
                .map(|preview| preview.speed),
            Some(Speed::Normal)
        );
    }

    #[test]
    fn enter_on_a_level_opens_the_speeds_only_for_a_model_that_offers_fast() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[0]);
        let high = Effort::Named("high".to_string());
        // `submitModelPicker` `:1183`: on to the speeds, `fast` marked.
        assert_eq!(
            column.chose_effort(&rows[0], &high),
            Some("/model openai/gpt-6-astra high ".to_string())
        );
        let text = "/model openai/gpt-6-astra high ";
        let query = column.query(text, "").expect("a speed query");
        assert_eq!(
            (query.stage, query.query, query.token_start),
            (Stage::Speed, "", 31)
        );
        assert_eq!(
            column.speeds(query.query, known),
            [Speed::Normal, Speed::Fast, Speed::Ultrafast]
        );
        assert_eq!(column.chosen_speed(&query, known), Some(Speed::Fast));
        assert_eq!(column.speed_effort(known), high);
        assert_eq!(column.pending(), Some("openai/gpt-6-astra"));
        // Ultra and no Fast: the level commits (`None`), so Ultra cannot be
        // chosen this way -- as upstream's cannot.
        let shaped = shapes();
        let mut ultra = Column::default();
        ultra.choose(&shaped[0]);
        assert_eq!(ultra.chose_effort(&shaped[0], &high), None);
        // But a space after a level spelled out opens it for Ultra too, with
        // `normal` marked (`advanceModelPickerOnSpace`, `:1133-1144`).
        assert_eq!(
            ultra.advance_effort(&shaped[0], &high),
            Some("/model openai/ultra-leveled high ".to_string())
        );
        let shaped_known = Known::Rows(&shaped);
        let query = ultra
            .query("/model openai/ultra-leveled high ", "")
            .expect("query");
        assert_eq!(
            ultra.speeds(query.query, shaped_known),
            [Speed::Normal, Speed::Ultrafast]
        );
        assert_eq!(
            ultra.chosen_speed(&query, shaped_known),
            Some(Speed::Normal)
        );
        // A model with levels and no speed: a space does nothing.
        let mut glm = Column::default();
        glm.choose(&shaped[3]);
        assert_eq!(glm.advance_effort(&shaped[3], &high), None);
    }

    #[test]
    fn a_model_with_no_levels_and_a_speed_opens_the_speeds_at_auto() {
        let shaped = shapes();
        let known = Known::Rows(&shaped);
        let mut column = Column::default();
        assert_eq!(
            column.choose(&shaped[1]),
            Chosen::Speeds("/model openai/ultra-only auto ".to_string())
        );
        let query = column
            .query("/model openai/ultra-only auto ", "")
            .expect("query");
        assert_eq!(query.stage, Stage::Speed);
        assert_eq!(column.speeds("", known), [Speed::Normal, Speed::Ultrafast]);
        assert_eq!(column.chosen_speed(&query, known), Some(Speed::Normal));
        assert_eq!(column.speed_effort(known), Effort::Auto);
        // Left goes back to the models: there are no levels to go back to.
        assert_eq!(
            column.step_back(known, Speed::Normal),
            Some("/model openai/ultra-only".to_string())
        );
        assert_eq!(column.pending(), None);
        // Fast alone marks `fast`.
        let mut fast = Column::default();
        fast.choose(&shaped[2]);
        let query = fast
            .query("/model acme/fast-only auto ", "")
            .expect("query");
        assert_eq!(fast.chosen_speed(&query, known), Some(Speed::Fast));
    }

    #[test]
    fn the_speed_column_filters_marks_a_speed_and_takes_one_spelled_out() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[0]);
        column.chose_effort(&rows[0], &Effort::Named("low".to_string()));
        let at = |column: &Column, text: &'static str| column.query(text, "").expect("query");
        // The token starts after the level's word, whatever it is, and the
        // blanks after it; with no level token there is no speed query.
        assert_eq!(
            column
                .query("/model openai/gpt-6-astra low   fa", "")
                .map(|q| (q.query, q.token_start)),
            Some(("fa", 32))
        );
        assert_eq!(column.query("/model openai/gpt-6-astra ", ""), None);
        assert_eq!(column.query("/model openai/gpt-6-astr", ""), None);
        // `fast` matches two rows and keeps the mark on `fast`; Down walks the
        // rows listed and keeps the speed the new row names.
        let filtered = at(&column, "/model openai/gpt-6-astra low fast");
        assert_eq!(
            column.speeds(filtered.query, known),
            [Speed::Fast, Speed::Ultrafast]
        );
        assert_eq!(column.chosen_speed(&filtered, known), Some(Speed::Fast));
        // Spelled out exactly: taken whatever is marked.
        let ultra = at(&column, "/model openai/gpt-6-astra low ULTRAFAST");
        assert_eq!(column.chosen_speed(&ultra, known), Some(Speed::Ultrafast));
        let open = at(&column, "/model openai/gpt-6-astra low ");
        assert!(column.step(&open, known, true));
        assert_eq!(column.chosen_speed(&open, known), Some(Speed::Ultrafast));
        assert!(column.step(&open, known, true));
        assert_eq!(
            column.chosen_speed(&open, known),
            Some(Speed::Normal),
            "wrapped"
        );
        // A mark the filter hides shows on the first row listed.
        let hidden = at(&column, "/model openai/gpt-6-astra low fa");
        assert_eq!(column.chosen_speed(&hidden, known), Some(Speed::Fast));
        // Nothing listed: nothing taken, and the column says so.
        let none = at(&column, "/model openai/gpt-6-astra low slow");
        assert_eq!(column.chosen_speed(&none, known), None);
        assert_eq!(
            plain(&column.rows(&none, known, 30, 120, 6, PALETTE))[0],
            format!("{}no matching mode", " ".repeat(30))
        );
        // A speed spelled out that the model does not offer is not taken.
        let opus_rows = catalog();
        let mut opus = Column::default();
        opus.choose(&opus_rows[1]);
        opus.chose_effort(&opus_rows[1], &Effort::Auto);
        let query = opus
            .query("/model anthropic/claude-opus-5 auto ultrafast", "")
            .expect("query");
        assert_eq!(opus.chosen_speed(&query, Known::Rows(&opus_rows)), None);
    }

    #[test]
    fn typing_a_speed_marks_normal_and_the_preview_follows_the_typed_word() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[0]);
        column.chose_effort(&rows[0], &Effort::Named("low".to_string()));
        let before = "/model openai/gpt-6-astra low ";
        let open = column.query(before, "").expect("query");
        assert_eq!(
            column.preview(&open, known).map(|p| p.speed),
            Some(Speed::Fast)
        );
        assert_eq!(
            column.preview(&open, known).map(|p| p.effort),
            Some(Effort::Named("low".to_string()))
        );
        // `resetActiveModelPickerIndex` at the speed stage: `normal`.
        let after = "/model openai/gpt-6-astra low u";
        column.edited(before, after, Edit::Typed, true);
        let typed = column.query(after, "").expect("query");
        assert_eq!(
            column.preview(&typed, known).map(|p| p.speed),
            Some(Speed::Normal)
        );
        // A whole word is what the footer previews, offered or not -- the
        // footer asks the row (`pendingPickerSpeed`).
        let word = "/model openai/gpt-6-astra low ultrafast";
        column.edited(after, word, Edit::Typed, true);
        let typed = column.query(word, "").expect("query");
        assert_eq!(
            column.preview(&typed, known).map(|p| p.speed),
            Some(Speed::Ultrafast)
        );
    }

    #[test]
    fn left_from_the_speeds_goes_back_to_the_levels_with_the_sessions_speed_marked() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[0]);
        column.chose_effort(&rows[0], &Effort::Named("high".to_string()));
        // `stepBackModelPicker` `:1276-1285`: the level it was opened with
        // still marked, and the session's speed -- not the model's -- marked.
        assert_eq!(
            column.step_back(known, Speed::Normal),
            Some("/model openai/gpt-6-astra ".to_string())
        );
        let query = column
            .query("/model openai/gpt-6-astra ", "")
            .expect("query");
        assert_eq!(query.stage, Stage::Effort);
        assert_eq!(
            column.chosen_effort(&query, known),
            Some(Effort::Named("high".to_string()))
        );
        assert_eq!(
            column.preview(&query, known).map(|p| p.speed),
            Some(Speed::Normal)
        );
    }

    #[test]
    fn tab_at_the_speeds_writes_all_three_tokens_and_keeps_the_column() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[0]);
        column.chose_effort(&rows[0], &Effort::Named("medium".to_string()));
        let query = column
            .query("/model openai/gpt-6-astra medium ", "")
            .expect("query");
        assert!(column.step(&query, known, true));
        assert_eq!(
            column.complete(&query, known),
            Some("/model openai/gpt-6-astra medium ultrafast".to_string())
        );
        let written = column
            .query("/model openai/gpt-6-astra medium ultrafast", "")
            .expect("still the speeds");
        assert_eq!(written.stage, Stage::Speed);
        assert_eq!(column.chosen_speed(&written, known), Some(Speed::Ultrafast));
    }

    #[test]
    fn the_column_is_six_rows_or_what_the_screen_leaves() {
        assert_eq!(height(40), 6);
        assert_eq!(height(9), 4);
        assert_eq!(height(3), 1);
    }

    const ASTRA: &str = "openai/gpt-6-astra";

    fn named(level: &str) -> Effort {
        Effort::Named(level.to_string())
    }

    /// The levels a column lists, as they are shown.
    fn shown(levels: &[Effort]) -> Vec<&str> {
        levels.iter().map(Effort::display).collect()
    }

    #[test]
    fn an_effort_line_is_everything_after_effort_and_a_space() {
        // `rawModelPickerQuery`, `picker_state.zig:298-307@481104fd`.
        let column = Column::default();
        assert_eq!(
            column.query("/effort ", ASTRA),
            Some(Query {
                stage: Stage::Effort,
                command: Command::Effort { model: ASTRA },
                query: "",
                token_start: 8
            })
        );
        assert_eq!(
            column
                .query("  /EFFORT two words", ASTRA)
                .map(|query| (query.query, query.token_start)),
            Some(("two words", 10))
        );
        assert_eq!(column.query("/effort", ASTRA), None);
        assert_eq!(column.query("/efforts x", ASTRA), None);
        assert_eq!(column.query("say /effort x", ASTRA), None);
        assert!(shaped_line("  /Effort x"));
        assert!(effort_line(" \t/EFFORT "));
        assert!(!effort_line("/effort"));
        assert!(!effort_line("/model x"));
        assert!(bare_effort("/effort"));
        assert!(bare_effort(" \t/EFFORT"));
        assert!(!bare_effort("/effort "));
        assert!(!bare_effort("/efforts"));
    }

    #[test]
    fn an_effort_line_lists_the_models_in_force_whatever_model_has_pending() {
        // `app_render_runtime.zig:542-546@481104fd`: opus pending in a
        // `/model` flow, astra in force -- the `/effort ` column is astra's,
        // and its footer is not the flow's.
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.choose(&rows[1]);
        let query = column.query("/effort ", ASTRA).expect("an effort query");
        assert_eq!(
            shown(&column.efforts(&query, known)),
            ["default", "low", "medium", "high", "xhigh"]
        );
        assert_eq!(column.preview(&query, known), None);
        assert_eq!(column.pending(), Some("anthropic/claude-opus-5"), "kept");
        // Found by an alias as well as by its id, as the command finds it.
        let mut aliased = catalog();
        aliased[0].aliases = vec!["astra".to_string()];
        let column = Column::default();
        let query = column.query("/effort ", "astra").expect("query");
        assert_eq!(column.efforts(&query, Known::Rows(&aliased)).len(), 5);
    }

    #[test]
    fn the_effort_filter_ignores_case_and_enter_takes_the_spelled_level_or_the_marked_one() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        let at = |text: &'static str| Column::default().query(text, ASTRA).expect("query");
        // No level is spelled `HIGH`; of `high` and `xhigh` the first is
        // marked (`input_completion_runtime.zig:1491-1503@481104fd`).
        let upper = at("/effort HIGH");
        assert_eq!(shown(&column.efforts(&upper, known)), ["high", "xhigh"]);
        assert_eq!(column.chosen_effort(&upper, known), Some(named("high")));
        assert_eq!(
            column.chosen_effort(&at("/effort med"), known),
            Some(named("medium"))
        );
        assert_eq!(
            column.chosen_effort(&at("/effort Default"), known),
            Some(Effort::Auto)
        );
        // The mark moved: Enter takes the spelled level, the footer the marked one.
        let typed = at("/effort high");
        assert!(column.step(&typed, known, true));
        assert_eq!(column.chosen_effort(&typed, known), Some(named("high")));
        assert_eq!(column.marked_level(&typed, known), Some(named("xhigh")));
        let partial = at("/effort hi");
        assert_eq!(column.chosen_effort(&partial, known), Some(named("xhigh")));
        // Nothing to apply: Enter goes on to the command, which says why.
        for nothing in [
            "/effort bogus",
            "/effort max",
            "/effort two words",
            "/effort zz",
        ] {
            assert_eq!(column.chosen_effort(&at(nothing), known), None, "{nothing}");
            assert_eq!(column.marked_level(&at(nothing), known), None, "{nothing}");
        }
    }

    #[test]
    fn a_space_opens_the_effort_column_on_its_first_row_and_tab_on_the_level_in_use() {
        // `fx-p5-effort-fork-effort-column-40x120-space-first` and
        // `-tab-current`: `low` in use.
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.edited("/effort", "/effort ", Edit::Typed, true);
        let open = column.query("/effort ", ASTRA).expect("query");
        assert_eq!(column.marked_level(&open, known), Some(Effort::Auto));
        // Tab on a bare `/effort`, with a `/model` flow to let go of.
        column.choose(&rows[1]);
        assert_eq!(
            column.open_levels(known.row_for(ASTRA), &named("low")),
            "/effort "
        );
        assert_eq!(column.pending(), None);
        assert_eq!(column.marked_level(&open, known), Some(named("low")));
        // A level the row does not list marks the first.
        column.open_levels(known.row_for(ASTRA), &named("max"));
        assert_eq!(column.marked_level(&open, known), Some(Effort::Auto));
        // Typing puts the mark back on the first row the filter keeps.
        column.open_levels(known.row_for(ASTRA), &named("medium"));
        column.edited("/effort ", "/effort h", Edit::Typed, true);
        let typed = column.query("/effort h", ASTRA).expect("query");
        assert_eq!(column.marked_level(&typed, known), Some(named("high")));
    }

    #[test]
    fn tab_in_the_effort_column_writes_the_level_and_marks_the_first_row() {
        // `fx-p5-effort-fork-effort-column-40x120-col-fill`, from `medium`:
        // Down to `high`, Tab writes `/effort high`, which lists `high` and
        // `xhigh` with `high` marked.
        let rows = catalog();
        let known = Known::Rows(&rows);
        let mut column = Column::default();
        column.open_levels(known.row_for(ASTRA), &named("medium"));
        let open = column.query("/effort ", ASTRA).expect("query");
        assert!(column.step(&open, known, true));
        assert_eq!(
            column.complete(&open, known),
            Some("/effort high".to_string())
        );
        let filled = column.query("/effort high", ASTRA).expect("query");
        assert_eq!(column.marked_level(&filled, known), Some(named("high")));
        // The default is written as it is shown.
        let first = column.query("/effort ", ASTRA).expect("query");
        assert_eq!(
            column.complete(&first, known),
            Some("/effort default".to_string())
        );
        assert_eq!(column.complete(&at_zz(), known), None);
    }

    fn at_zz() -> Query<'static> {
        Column::default().query("/effort zz", ASTRA).expect("query")
    }

    #[test]
    fn a_model_without_levels_has_no_effort_column() {
        let rows = catalog();
        let known = Known::Rows(&rows);
        let column = Column::default();
        let with = column.query("/effort ", ASTRA).expect("query");
        assert!(column.shows(&with, known));
        for (model, catalog) in [
            ("fake/model-a", known),
            ("acme/unlisted", known),
            (ASTRA, Known::Loading),
            (ASTRA, Known::Failed),
        ] {
            let query = column
                .query("/effort ", model)
                .expect("still the column's line");
            assert!(!column.shows(&query, catalog), "{model}");
            assert_eq!(column.chosen_effort(&query, catalog), None, "{model}");
        }
        // A `/model` line always has one.
        let model = column.query("/model zzz", ASTRA).expect("query");
        assert!(column.shows(&model, Known::Loading));
    }

    #[test]
    fn the_effort_column_is_drawn_under_the_argument() {
        // `fx-p5-effort-fork-effort-column-40x120-col-space`, `-col-nomatch`.
        let rows = catalog();
        let known = Known::Rows(&rows);
        let column = Column::default();
        let query = column.query("/effort ", ASTRA).expect("query");
        assert_eq!(
            plain(&column.rows(&query, known, 10, 120, 6, PALETTE)),
            [
                "          default",
                "          low",
                "          medium",
                "          high",
                "          xhigh",
                ""
            ]
        );
        assert_eq!(
            plain(&column.rows(&at_zz(), known, 10, 120, 6, PALETTE))[0],
            "          no matching effort"
        );
    }

    /// fx's completions case's catalog (`:2129-2134@481104fd`), and the model
    /// `cs45` is for.
    fn ranked() -> Vec<CatalogEntry> {
        [
            "openai/gpt-4.5-preview",
            "openai/gpt-5",
            "anthropic/claude-opus-4.8",
            "acme/gpt5-lite",
            "anthropic/claude-sonnet-4.5",
        ]
        .iter()
        .map(|id| entry(id, &[]))
        .collect()
    }

    fn typed(query: &str) -> Query<'_> {
        Query {
            stage: Stage::Model,
            command: Command::Model,
            query,
            token_start: PREFIX.len(),
        }
    }

    #[test]
    fn the_column_lists_its_models_ranked_as_fxs_completions_are() {
        // `model completions rank fuzzy matches below verbatim ones`
        // (`src/core/app/model_cache_runtime.zig:2125-2156@481104fd`).
        let rows = ranked();
        let known = Known::Rows(&rows);
        let ids = |query: &str| {
            Column::models(query, known)
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids("gpt5"),
            ["acme/gpt5-lite", "openai/gpt-5", "openai/gpt-4.5-preview"]
        );
        assert_eq!(ids("anthropic opus"), ["anthropic/claude-opus-4.8"]);
        assert_eq!(ids("cs45"), ["anthropic/claude-sonnet-4.5"]);
        assert_eq!(ids("").len(), 5);
        assert_eq!(ids("")[0], "openai/gpt-4.5-preview");
        // An exact id is a substring match, and a fuzzy one can rank under it
        // (`p5-input-model-rank` `c-gpt5-tab`).
        assert_eq!(
            ids("openai/gpt-5"),
            ["openai/gpt-5", "openai/gpt-4.5-preview"]
        );

        // fx fills its 32 places rank by rank (`modelCompletions`, `:897-913`),
        // so the cap keeps the best matches, not the first in the catalog.
        let mut many: Vec<CatalogEntry> = (0..MOST_MODELS)
            .map(|n| entry(&format!("acme/gpt-4.5-r{n}"), &[]))
            .collect();
        many.push(entry("acme/gpt5-last", &[]));
        let listed = Column::models("gpt5", Known::Rows(&many));
        assert_eq!(listed.len(), MOST_MODELS);
        assert_eq!(listed[0].id, "acme/gpt5-last");
        assert_eq!(listed[MOST_MODELS - 1].id, "acme/gpt-4.5-r30");
    }

    #[test]
    fn enter_and_the_arrows_take_the_ranked_rows() {
        let rows = ranked();
        let known = Known::Rows(&rows);
        let chosen = |column: &Column, query: &str| {
            column
                .chosen_model(&typed(query), known)
                .map(|entry| entry.id.clone())
        };
        // `/model cs45` and Enter: the first row the column lists.
        let mut column = Column::default();
        assert_eq!(
            chosen(&column, "cs45").as_deref(),
            Some("anthropic/claude-sonnet-4.5")
        );
        // Down from acme/gpt5-lite is openai/gpt-5, the second rank's.
        assert!(column.step(&typed("gpt5"), known, true));
        assert_eq!(chosen(&column, "gpt5").as_deref(), Some("openai/gpt-5"));
        // The id spelled out is still the one Enter takes.
        assert_eq!(
            Column::exact_model("OPENAI/gpt-5", known).map(|entry| entry.id.as_str()),
            Some("openai/gpt-5")
        );
    }
}
