//! `/settings`: every preference this session can change, by category, in the
//! band under the composer, each changed in place with the arrow keys.
//!
//! Upstream's settings menu (`vercel-labs/fx@c1db9191`, the catalog in
//! `src/core/config/settings_catalog.zig`, the rows in
//! `src/ui/footer/settings_menu_presentation.zig`), and it lives in the slot the
//! `/help` catalog does ([`super::help_menu`]): between the rule under the
//! composer and the bottom rule, with its own key help where the hint row was.
//! Like the catalog it is a **view of the composer** rather than a focus -- the
//! composer row stays and what is typed is its search -- but unlike the
//! catalog its Enter runs nothing and its Left and Right are its own: they
//! change the marked row's value, and the caret never moves
//! (`app_input_runtime.zig:1113-1120`, ahead of every composer binding).
//!
//! # What a change does
//!
//! **The same thing the row's slash command does, and it says so the same
//! way** (`applySettingsCatalogChange`, `app_commands.zig:4110-4210`): a status
//! line row is `/statusline <item>` with both of its notices, the permission
//! row is `/permissions <mode>`, the sound row is `/sound <level>`, and
//! `Fast mode` and `Ultra mode` are `/fast` and `/ultrafast on|off` when they
//! differ from the session. The four
//! preferences that have no command of their own are written to the profile
//! and announce the write (`persistUserPreferences`, `app_commands.zig:167-192`).
//! This module only decides *which* value a keystroke means
//! ([`SettingsMenu::change`]); the shell applies it, so a row can never do
//! something its command does not. `Startup scrollback` is
//! `/settings startup-scrollback <value>`, which changes the next launch and
//! not this one. Enter does nothing except on `Model`
//! (`submitSettingsMenuSelection`, `app_input_runtime.zig:2531-2552`) -- the
//! statusline picker is the one whose Enter flips a value, not this one.
//!
//! # The Model row's list
//!
//! Right or Enter on `Model` opens the catalog `/model` lists **under the
//! row**, in the menu's own rows (`changeSettingsMenuSelection`,
//! `app_input_runtime.zig:2489-2512`; `inlineModelRowCount` and
//! `composeModelRow`, `settings_menu_presentation.zig:15-19`, `:277-310`): up to
//! six model ids at the value column, the marked one bold, or one dim line
//! four columns in while there is nothing to list. The arrows move its mark,
//! Left and Enter use the marked model and close it, Tab closes it with the
//! tab change. It is `/model`'s menu, held by the shell
//! ([`super::model_menu::ModelMenu::inline`]); this module only lays it out.
//!
//! # What it lists
//!
//! Upstream's fourteen rows, in its order. `Session titles` switches the
//! title a model is asked for after a fresh session's first prompt
//! (`super::titles`). `Fast mode` and `Ultra
//! mode` are `/fast` and `/ultrafast on|off`, with choices only for a model
//! whose catalog row offers the speed, or while it is on
//! (`optionCount`, `settings_catalog.zig:314-315`). `Sound level` is `off  on`:
//! upstream's `max` drives chimes xfx does not play (`crate::sound`).
//! `Reasoning effort` is the session's effort (`crate::provider::effort`), its
//! choices `default` and the levels the catalog lists for the model in force --
//! none, and the value alone, for a model it lists none for or a catalog not
//! yet read, unless the effort in force is not the default
//! (`optionCount`, `settings_catalog.zig:308-318`).
//!
//! # What it looks like
//!
//! A header -- `Settings N`, then `All` and the four categories, the active one
//! in brackets -- a blank row, then one row per setting in view: two columns of
//! indent, the label, and at a column four past the widest label **in the
//! filtered list** the setting's choices with the one in force bold, or its
//! value alone in bold when it has no choices to cycle (`Model`). The marked
//! row's label is the bold hint grey and every other label is dim; there is no
//! caret glyph.

use crate::config::{Notifications, PermissionMode, StatusLine, StatusLineItem};

use super::input::Action;
use super::model_menu::{Inline, ModelMenu};
use super::picker::{advanced, cells, edge_start, ellipsized, pad_to};
use super::theme::Palette;

/// The header and the blank row under it (`roomy_header_rows`,
/// `settings_menu_presentation.zig:12`).
const HEADER_ROWS: u16 = 2;

/// The band's rows the menu cannot have, before the composer's extra rows are
/// added to them (`inlinePickerRowBudgetCapped`'s `fixed_rows`,
/// `picker_presentation.zig:766`).
const FIXED_ROWS: u16 = 5;

/// The document rows left on any screen tall enough to have them
/// (`minimum_transcript_rows`, `picker_presentation.zig:767`).
const MINIMUM_TRANSCRIPT_ROWS: u16 = 5;

/// The most rows the menu is given: the header, its blank row and one row per
/// setting (`max_inline_rows`, `settings_menu_presentation.zig:13`), which is
/// upstream's cap counted over the settings this menu really has.
const MOST_ROWS: u16 = HEADER_ROWS + SPECS.len() as u16;

/// What every setting row starts with.
const INDENT: usize = 2;

/// The columns between the widest label and the values
/// (`inline_picker_column_gap_width`, `picker_presentation.zig:17`).
const COLUMN_GAP: usize = 4;

/// What separates the title and the tabs from each other, and one choice from
/// the next.
const GAP: &str = "  ";

/// What a list with nothing in it says (`composeEmptyRow`, `:322-324`).
const EMPTY: &str = "No settings found.";

/// The most models the Model row's list shows at once (`inlineModelRowCount`,
/// `settings_menu_presentation.zig:15-19`).
const MOST_MODELS: usize = 6;

/// Where the list's one line sits while it has no model to show
/// (`composeStyledText(..., 4)`, `:283-290`).
const MODEL_STATE_INDENT: usize = 4;

/// The list's line while the catalog is on its way, while nothing matches,
/// and when it could not be read (`:284-288`).
const MODELS_LOADING: &str = "Loading models\u{2026}";
const MODELS_NONE: &str = "No models found";
const MODELS_FAILED: &str = "Models unavailable";

/// What the hint row says while the menu is up, widest first
/// (`composeSettingsMenuHintRow`, `src/ui/footer/input_presentation.zig:729-765`).
///
/// The first one that fits the screen is the one shown, and the last is shown
/// -- cut by the painter -- when none does.
const KEY_HELP: [&str; 5] = [
    "\u{2191}\u{2193} navigate     tab category     \u{2190}\u{2192} change     esc close",
    "\u{2191}\u{2193} navigate  tab category  \u{2190}\u{2192} change  esc close",
    "\u{2191}\u{2193} move  tab category  \u{2190}\u{2192} change  esc",
    "tab category  \u{2190}\u{2192} change  esc",
    "tab \u{2190}\u{2192} esc",
];

/// The topic a `Slash menu categories` change is announced under: upstream's
/// label for it (`persistUserPreferences(app, "slash menu categories", ...)`,
/// `app_commands.zig:4135-4148`).
pub(crate) const SLASH_MENU_CATEGORIES_TOPIC: &str = "slash menu categories";

/// The topic a `Collapse tool calls` change is announced under (`:4120-4134`).
pub(crate) const COLLAPSE_TOOL_CALLS_TOPIC: &str = "collapse tool calls";

/// The topic a `Session titles` change is announced under (`:4149-4166`).
pub(crate) const SESSION_TITLES_TOPIC: &str = "session titles";

/// The topic a `Reasoning effort` change is announced under (`applyEffort`'s
/// `persistPreferenceTargets(app, ..., "effort", ...)`,
/// `session_commands.zig:867-874`).
pub(crate) const EFFORT_TOPIC: &str = "effort";

/// The topic a `Prompt history` change, and a history file that could not be
/// written, are announced under (`handleHistory`, `session_commands.zig:443-509`;
/// `recordAcceptedInput`, `input_submit_runtime.zig:1270-1274`).
pub(crate) const HISTORY_TOPIC: &str = "history";

/// The topic `/settings startup-scrollback`'s answer is filed under
/// (`writeDomainNotice(.{ .topic = "settings" ...})`,
/// `session_commands.zig:1119-1121`).
pub(crate) const SETTINGS_TOPIC: &str = "settings";

/// The argument `/settings` takes, and the topic a write of it that failed is
/// filed under (`reportUserSettingsFailure(app, "startup-scrollback", ...)`,
/// `session_commands.zig:1083-1090`).
pub(crate) const STARTUP_SCROLLBACK_TOPIC: &str = "startup-scrollback";

/// What a saved `startup_scrollback` says: the value, and that it is the next
/// launch's (`saveStartupScrollbackSetting`, `session_commands.zig:1102-1118`,
/// for the profile as its source).
pub(crate) fn startup_scrollback_saved(on: bool) -> String {
    format!(
        "startup_scrollback: {} (applies on next launch)",
        on_off(on)
    )
}

/// The two words every on/off setting cycles through, in the order drawn.
const ON_OFF: [&str; 2] = ["off", "on"];

/// The permission modes, as the row draws them (`permission_options`,
/// `settings_catalog.zig:279`): the person's words, not the wire's.
const PERMISSION_MODES: [&str; 3] = ["ask", "auto", "full access"];

/// A tab of the menu (`Category`, `settings_catalog.zig:5-21`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    All,
    Interface,
    Agent,
    Notifications,
    Advanced,
}

impl Category {
    /// Every tab, in the order Tab walks them.
    const ALL: [Self; 5] = [
        Self::All,
        Self::Interface,
        Self::Agent,
        Self::Notifications,
        Self::Advanced,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Interface => "Interface",
            Self::Agent => "Agent",
            Self::Notifications => "Notifications",
            Self::Advanced => "Advanced",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|category| *category == self)
            .unwrap_or(0)
    }
}

/// One setting the menu lists (`SettingId`, `settings_catalog.zig:25-40`, the
/// rows xfx has something behind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Setting {
    /// One of the footer's three optional segments: `/statusline <item>`.
    Statusline(StatusLineItem),
    /// Whether the slash popup shows each command's category.
    SlashMenuCategories,
    /// Whether a tool group is drawn as its header alone.
    CollapseToolCalls,
    /// The model a turn talks to: `/model`'s menu.
    Model,
    /// The session's reasoning effort, written as `effort`.
    Effort,
    /// `/fast`: Fast mode for the model in force.
    FastMode,
    /// `/ultrafast on|off`: Ultra mode for the model in force.
    UltraMode,
    /// `/permissions <mode>`.
    PermissionMode,
    /// Whether a fresh session's first prompt asks a model for its title
    /// (`super::titles`).
    SessionTitles,
    /// `/sound <level>`.
    SoundLevel,
    /// `/settings startup-scrollback <value>`: whether the next launch pushes
    /// the shell's output into scrollback.
    StartupScrollback,
    /// Whether submitted lines are kept for the composer's recall, here and in
    /// the next session.
    PromptHistory,
}

impl Setting {
    /// Upstream's name for it (`@tagName`), which the search also reads
    /// (`tokenMatches`, `settings_catalog.zig:391-396`).
    fn tag(self) -> &'static str {
        match self {
            Self::Statusline(StatusLineItem::Context) => "statusline_context",
            Self::Statusline(StatusLineItem::Session) => "statusline_session",
            Self::Statusline(StatusLineItem::Workspace) => "statusline_workspace",
            Self::SlashMenuCategories => "slash_menu_categories",
            Self::CollapseToolCalls => "collapse_tool_calls",
            Self::Model => "model",
            Self::Effort => "effort",
            Self::FastMode => "fast_mode",
            Self::UltraMode => "ultrafast_mode",
            Self::PermissionMode => "permission_mode",
            Self::SessionTitles => "session_titles",
            Self::SoundLevel => "sound_level",
            Self::StartupScrollback => "startup_scrollback",
            Self::PromptHistory => "prompt_history",
        }
    }
}

/// A setting's row in the catalog.
#[derive(Debug, Clone, Copy)]
struct Spec {
    id: Setting,
    category: Category,
    label: &'static str,
    /// Never drawn: what the search reads besides the label.
    description: &'static str,
}

/// The settings, in upstream's order (`specs`, `settings_catalog.zig:261-276`)
/// and upstream's words, with this product's name where upstream says its own
/// and without `max` in the sound row's, which xfx's switch does not have.
const SPECS: [Spec; 14] = [
    Spec {
        id: Setting::Statusline(StatusLineItem::Context),
        category: Category::Interface,
        label: "Status line context",
        description: "Show context usage in the status line",
    },
    Spec {
        id: Setting::Statusline(StatusLineItem::Session),
        category: Category::Interface,
        label: "Status line session",
        description: "Show the session title in the status line",
    },
    Spec {
        id: Setting::Statusline(StatusLineItem::Workspace),
        category: Category::Interface,
        label: "Status line workspace",
        description: "Show the workspace path and Git branch in the status line",
    },
    Spec {
        id: Setting::SlashMenuCategories,
        category: Category::Interface,
        label: "Slash menu categories",
        description: "Show categories and skill sources in slash-command results",
    },
    Spec {
        id: Setting::CollapseToolCalls,
        category: Category::Interface,
        label: "Collapse tool calls",
        description: "Show only a summary for each group of tool calls",
    },
    Spec {
        id: Setting::Model,
        category: Category::Agent,
        label: "Model",
        description: "Choose the model used for new turns",
    },
    Spec {
        id: Setting::Effort,
        category: Category::Agent,
        label: "Reasoning effort",
        description: "Control how much reasoning the model applies",
    },
    Spec {
        id: Setting::FastMode,
        category: Category::Agent,
        label: "Fast mode",
        description: "Use faster inference when the model supports it",
    },
    Spec {
        id: Setting::UltraMode,
        category: Category::Agent,
        label: "Ultra mode",
        description: "Request the highest-speed mode when the model supports it",
    },
    Spec {
        id: Setting::PermissionMode,
        category: Category::Agent,
        label: "Permission mode",
        description: "Choose when xfx asks before taking actions",
    },
    Spec {
        id: Setting::SessionTitles,
        category: Category::Agent,
        label: "Session titles",
        description: "Generate a short session title from the first prompt",
    },
    Spec {
        id: Setting::SoundLevel,
        category: Category::Notifications,
        label: "Sound level",
        description: "Choose off or on for sounds and terminal bells",
    },
    Spec {
        id: Setting::StartupScrollback,
        category: Category::Advanced,
        label: "Startup scrollback",
        description: "Restore terminal output when xfx starts",
    },
    Spec {
        id: Setting::PromptHistory,
        category: Category::Advanced,
        label: "Prompt history",
        description: "Save accepted prompts and slash commands for composer history",
    },
];

/// What is in force for each setting, as the shell holds it: the values a row
/// shows and a change starts from (`Snapshot`, `settings_catalog.zig:49-86`).
///
/// Handed in to paint and to change rather than held here, so a row can never
/// show a value the session is not using -- the same reason
/// [`super::statusline::Menu`] holds none.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Snapshot<'a> {
    pub(crate) statusline: StatusLine,
    pub(crate) slash_menu_categories: bool,
    pub(crate) collapse_tool_calls: bool,
    /// The model id in force, provider prefix and all.
    pub(crate) model: &'a str,
    /// The reasoning effort in force, as a person reads it: `default` or a
    /// level's name (`crate::provider::effort::Effort::display`).
    pub(crate) effort: &'a str,
    /// The levels the catalog lists for the model in force, empty when it
    /// lists none or has not been read.
    pub(crate) efforts: &'a [String],
    /// Fast mode and Ultra mode, as the session asks for them now.
    pub(crate) fast: bool,
    pub(crate) ultrafast: bool,
    /// Whether the model in force's catalog row offers each speed: false
    /// while the catalog has not been read.
    pub(crate) offers_fast: bool,
    pub(crate) offers_ultrafast: bool,
    pub(crate) permission_mode: PermissionMode,
    pub(crate) session_titles: bool,
    pub(crate) sound: Notifications,
    /// What the profile says the next launch does, which is what the row and
    /// its command change (`openWithStartupScrollback`,
    /// `app_commands.zig:2022-2041`) -- not what this launch did.
    pub(crate) startup_scrollback: bool,
    pub(crate) prompt_history: bool,
    /// `/model`'s menu, while the Model row has it open under itself.
    pub(crate) models: Option<&'a ModelMenu>,
}

impl<'a> Snapshot<'a> {
    /// The setting's value as its row spells it (`Snapshot.value`).
    ///
    /// Sound is `custom` when a profile set its two switches apart -- the one
    /// value no choice on the row is (`notificationLevel`,
    /// `settings_catalog.zig:402-406`), so no choice is bold and a change
    /// starts from the first.
    fn value(&self, id: Setting) -> &'a str {
        match id {
            Setting::Statusline(item) => on_off(self.statusline.get(item)),
            Setting::SlashMenuCategories => on_off(self.slash_menu_categories),
            Setting::CollapseToolCalls => on_off(self.collapse_tool_calls),
            Setting::Model => self.model,
            Setting::Effort => self.effort,
            Setting::FastMode => on_off(self.fast),
            Setting::UltraMode => on_off(self.ultrafast),
            Setting::PermissionMode => self.permission_mode.display_label(),
            Setting::SessionTitles => on_off(self.session_titles),
            Setting::SoundLevel => match (self.sound.turn_end, self.sound.attention_required) {
                (false, false) => "off",
                (true, true) => "on",
                _ => "custom",
            },
            Setting::StartupScrollback => on_off(self.startup_scrollback),
            Setting::PromptHistory => on_off(self.prompt_history),
        }
    }

    /// The rows the Model row's list takes under it: none while it is shut,
    /// one line while it has no model to show, and up to [`MOST_MODELS`]
    /// otherwise (`inlineModelRowCount`, `:15-19`).
    fn model_rows(&self) -> u16 {
        match self.models.map(ModelMenu::inline) {
            None => 0,
            Some(Inline::Listed { ids, .. }) if !ids.is_empty() => {
                u16::try_from(ids.len().min(MOST_MODELS)).unwrap_or(1)
            }
            Some(_) => 1,
        }
    }
}

/// `on` or `off`.
fn on_off(on: bool) -> &'static str {
    ON_OFF[usize::from(on)]
}

/// The choices a setting's row cycles through, in the order drawn
/// (`optionCount`/`optionAt`/`staticOptionsFor`, `settings_catalog.zig:308-377`).
/// None for the model, whose row is its value and whose Enter is `/model`'s
/// menu. The effort's are `default` and the model's levels -- none at all for
/// a model with no levels while the effort is the default, and `default`
/// alone when it is not (a level carried over from another model), so the one
/// way off it is back to the default.
fn options<'a>(id: Setting, snapshot: &Snapshot<'a>) -> Vec<&'a str> {
    match id {
        Setting::Model => Vec::new(),
        Setting::Effort => {
            if snapshot.efforts.is_empty() && snapshot.effort.eq_ignore_ascii_case(DEFAULT_EFFORT) {
                return Vec::new();
            }
            std::iter::once(DEFAULT_EFFORT)
                .chain(snapshot.efforts.iter().map(String::as_str))
                .collect()
        }
        Setting::FastMode if snapshot.offers_fast || snapshot.fast => ON_OFF.to_vec(),
        Setting::UltraMode if snapshot.offers_ultrafast || snapshot.ultrafast => ON_OFF.to_vec(),
        Setting::FastMode | Setting::UltraMode => Vec::new(),
        Setting::PermissionMode => PERMISSION_MODES.to_vec(),
        Setting::Statusline(_)
        | Setting::SlashMenuCategories
        | Setting::CollapseToolCalls
        | Setting::SessionTitles
        | Setting::SoundLevel
        | Setting::StartupScrollback
        | Setting::PromptHistory => ON_OFF.to_vec(),
    }
}

/// The effort row's first choice, the provider's own default
/// (`optionAt`'s index 0, `settings_catalog.zig:323`).
const DEFAULT_EFFORT: &str = "default";

/// One change a keystroke asked for: the setting, and the choice it is to have
/// -- a word of [`options`], which is what the setting's own command reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub(crate) setting: Setting,
    pub(crate) value: String,
}

/// What one keystroke means to the menu.
///
/// Its own vocabulary rather than [`Action`], for the reason
/// [`super::picker::PickerAction`] is one: the shell translates, so a key the
/// menu does not bind goes on to the composer -- which is what makes typing a
/// search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsAction {
    /// The previous setting, wrapping to the last. Up.
    Up,
    /// The next one, wrapping to the first. Down.
    Down,
    /// One row down (`true`, `C-j`) or up (`C-k`), as Down and Up -- but
    /// spent only when there was a row to move to: upstream's control-byte
    /// route goes on to the composer when the menu answers `false`
    /// (`routePickerControlByte`, `app_input_runtime.zig:1093-1108@481104fd`;
    /// `Menu.move` is `false` over an empty list, `settings_catalog.zig:227`).
    Row(bool),
    /// A page down (`true`, PgDn) or up (PgUp): the composer's row cap of
    /// settings, to the other end when it would run past one
    /// (`routeComposerVertical` with `page`,
    /// `input_completion_runtime.zig:222-233@481104fd`, into
    /// `routeSettingsMenuMove`, `:431-463`). Spent, like [`Self::Row`], only
    /// when there was a row to move to.
    Page(bool),
    /// The next tab, wrapping to `All`. Tab.
    NextCategory,
    /// The previous one. Shift-Tab (`toggle_permission_mode`'s first claimant,
    /// `app_input_runtime.zig:1175-1185`).
    PreviousCategory,
    /// The marked setting's previous choice, wrapping. Left.
    Previous,
    /// Its next choice, wrapping. Right.
    Next,
    /// Enter: the list under the `Model` row -- or, with it open, the marked
    /// model -- and nothing on any other row.
    Open,
    /// Close it and empty the search (`cancelSettingsMenu`, `:3292-3294`).
    /// Escape.
    Close,
}

impl SettingsAction {
    /// What an ordinary decoded action means to the menu, and `None` for the
    /// keystrokes that go on to the composer.
    pub(crate) fn of(action: Action) -> Option<Self> {
        match action {
            Action::Up => Some(Self::Up),
            Action::Down => Some(Self::Down),
            Action::ControlJ => Some(Self::Row(true)),
            Action::ControlK => Some(Self::Row(false)),
            Action::PageDown => Some(Self::Page(true)),
            Action::PageUp => Some(Self::Page(false)),
            Action::Tab => Some(Self::NextCategory),
            Action::BackTab => Some(Self::PreviousCategory),
            Action::Left => Some(Self::Previous),
            Action::Right => Some(Self::Next),
            Action::Submit => Some(Self::Open),
            Action::Escape => Some(Self::Close),
            _ => None,
        }
    }
}

/// The menu's shape for one row budget (`Layout`,
/// `settings_menu_presentation.zig:30-38`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Layout {
    /// Every row it takes, header included.
    rows: u16,
    /// Whether the header and its blank row are drawn.
    header: bool,
    /// The first setting in view, as an index into the filtered list.
    first: usize,
    /// How many settings are in view.
    visible: usize,
    /// The marked setting, as an index into the filtered list.
    selected: usize,
    /// One body row and the Model row's list open: the row is the list's
    /// first (`model_only`, `:118-131`).
    model_only: bool,
}

/// One row under the header (`BodyRow`, `settings_menu_presentation.zig:21-28`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyRow {
    /// A setting, as an index into the filtered list.
    Item(usize),
    /// The Model row's list's row this many from its first in view.
    Model(usize),
}

/// The menu: which tab is up, which setting is marked, and what the search is.
///
/// The search is the composer's text, held here as of the last edit
/// ([`Self::set_query`]), so that the rows and the height are a function of
/// this value and a [`Snapshot`] alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettingsMenu {
    category: Category,
    /// An index into the filtered list, kept in range by the readers rather
    /// than by every writer, as upstream's is (`selected_index % count`).
    selected: usize,
    /// The first setting in view, as the last move left it.
    window_start: usize,
    /// The composer's text.
    query: String,
}

impl SettingsMenu {
    /// A menu on `All`, the first setting marked, searching for `query`
    /// (`Menu.open`, `settings_catalog.zig:196-202`).
    pub(crate) fn open(query: &str) -> Self {
        Self {
            category: Category::All,
            selected: 0,
            window_start: 0,
            query: query.to_string(),
        }
    }

    /// Takes the composer's text as the search.
    ///
    /// A search that changed puts the mark back on the first setting and the
    /// window back at the top (`resetForQuery`, `:208-211`, called for every
    /// edit at `app_input_runtime.zig:2930-2933`); a keystroke that left the
    /// text alone leaves both where they were.
    pub(crate) fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.selected = 0;
        self.window_start = 0;
    }

    /// The next tab, or the previous one, wrapping through `All`
    /// (`cycleCategory`, `:213-223`). The mark goes back to the top.
    pub(crate) fn cycle(&mut self, forward: bool) {
        let count = Category::ALL.len();
        let current = self.category.index();
        let next = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        self.category = Category::ALL[next];
        self.reset();
    }

    /// Moves the mark `delta` settings on -- one for Up and Down, a page for
    /// PgUp and PgDn -- and scrolls a window of `visible` settings only as far
    /// as keeps it in view (`move`, `:225-241`). Past either end it goes to
    /// the **other** end, however far past it was asked to go: `move` sets
    /// `next` to the last or the first rather than taking it round
    /// (`:229-233`), so a page from the last setting is the first. `false`,
    /// and nothing moved, when nothing is listed.
    pub(crate) fn step_by(&mut self, delta: i64, visible: u16, snapshot: &Snapshot<'_>) -> bool {
        let count = self.items(snapshot).len();
        if count == 0 {
            return false;
        }
        self.selected = advanced(self.selected, count, delta);
        self.window_start = edge_start(self.window_start, count, self.selected, visible.max(1));
        true
    }

    /// The marked setting, or `None` when nothing is listed
    /// (`selectedItem`, `:243-247`).
    pub(crate) fn selected(&self, snapshot: &Snapshot<'_>) -> Option<Setting> {
        let items = self.items(snapshot);
        let count = items.len();
        (count > 0).then(|| items[self.selected % count].id)
    }

    /// The choice one step from the marked setting's, wrapping
    /// (`changeSelectedOption` and `cycleChange`, `:249-258`, `:332-341`).
    ///
    /// `None` when nothing is listed or the marked setting has no choices --
    /// the model, whose Right opens `/model`'s menu instead, which is the
    /// shell's to do. A value that is none of the choices (sound's `custom`)
    /// counts as the first.
    pub(crate) fn change(&self, forward: bool, snapshot: &Snapshot<'_>) -> Option<Change> {
        let setting = self.selected(snapshot)?;
        let choices = options(setting, snapshot);
        if choices.is_empty() {
            return None;
        }
        let value = snapshot.value(setting);
        let current = choices
            .iter()
            .position(|choice| choice.eq_ignore_ascii_case(value))
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % choices.len()
        } else {
            (current + choices.len() - 1) % choices.len()
        };
        Some(Change {
            setting,
            value: choices[next].to_string(),
        })
    }

    /// What the menu lists: the active tab's settings the search finds, in
    /// catalog order (`itemAt`, `:290-306`).
    fn items(&self, snapshot: &Snapshot<'_>) -> Vec<&'static Spec> {
        let words: Vec<String> = self
            .query
            .split([' ', '\t', '\r', '\n'])
            .filter(|word| !word.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        SPECS
            .iter()
            .filter(|spec| self.category == Category::All || spec.category == self.category)
            .filter(|spec| found(spec, snapshot, &words))
            .collect()
    }

    /// The menu's shape in `max_rows` rows (`buildBrowseLayout`, `:97-134`):
    /// the header only when there are more than two rows, an empty list as
    /// one row under it, otherwise the window where the last move left it --
    /// from the mark itself while the Model row's list is open -- pulled back
    /// to the mark and walked down until the mark is in it.
    fn layout(&self, items: &[&Spec], snapshot: &Snapshot<'_>, max_rows: u16) -> Layout {
        if max_rows == 0 {
            return Layout::default();
        }
        let header = max_rows > HEADER_ROWS;
        let body_start = if header { HEADER_ROWS } else { 0 };
        let count = items.len();
        if count == 0 {
            return Layout {
                rows: max_rows.min(body_start + 1),
                header,
                ..Layout::default()
            };
        }
        let selected = self.selected % count;
        let budget = max_rows - body_start;
        let listing = snapshot.models.is_some();
        let mut first = if listing {
            selected
        } else {
            self.window_start.min(count - 1)
        }
        .min(selected);
        let mut body = measure(items, snapshot, first, budget);
        while selected >= first + body.0 && first < selected {
            first += 1;
            body = measure(items, snapshot, first, budget);
        }
        if body.0 == 0 {
            first = selected;
            body = measure(items, snapshot, first, budget);
        }
        let (visible, rows) = body;
        Layout {
            rows: body_start + rows,
            header,
            first,
            visible,
            selected,
            model_only: listing && budget == 1,
        }
    }

    /// The rows under the header, top first: each setting in view, and under
    /// the Model row its list while it is open (`browseBodyRowAt`,
    /// `:160-182`), as many as the layout has rows for.
    fn body(&self, layout: &Layout, items: &[&Spec], snapshot: &Snapshot<'_>) -> Vec<BodyRow> {
        let body_start = if layout.header { HEADER_ROWS } else { 0 };
        let room = usize::from(layout.rows.saturating_sub(body_start));
        if layout.model_only {
            return vec![BodyRow::Model(0)].into_iter().take(room).collect();
        }
        let mut body = Vec::with_capacity(room);
        for (index, spec) in items
            .iter()
            .enumerate()
            .skip(layout.first)
            .take(layout.visible)
        {
            body.push(BodyRow::Item(index));
            if spec.id == Setting::Model {
                body.extend((0..usize::from(snapshot.model_rows())).map(BodyRow::Model));
            }
        }
        body.truncate(room);
        body
    }

    /// How many rows the band has to give it on a screen `terminal_rows` tall
    /// whose composer is `input_extra` rows taller than one (`menuRowCount`
    /// under `inlinePickerRowBudgetCapped`, `surface_frame.zig:556-560`,
    /// `:601-606`).
    pub(crate) fn height(
        &self,
        snapshot: &Snapshot<'_>,
        terminal_rows: u16,
        input_extra: u16,
    ) -> u16 {
        self.layout(
            &self.items(snapshot),
            snapshot,
            row_budget(terminal_rows, input_extra),
        )
        .rows
    }

    /// How many settings are in view in `max_rows` rows, and never fewer than
    /// one: the window a move scrolls against
    /// (`visibleNavigationItemsForBudget`, `:44-50`).
    pub(crate) fn visible_items(&self, snapshot: &Snapshot<'_>, max_rows: u16) -> u16 {
        let visible = self
            .layout(&self.items(snapshot), snapshot, max_rows)
            .visible;
        u16::try_from(visible.max(1)).unwrap_or(u16::MAX)
    }

    /// How many of the Model row's list's rows are in view in `max_rows`
    /// rows: the window its mark scrolls against
    /// (`visibleModelItemsForBudget`, `:52-71`).
    pub(crate) fn visible_models(&self, snapshot: &Snapshot<'_>, max_rows: u16) -> u16 {
        let items = self.items(snapshot);
        let layout = self.layout(&items, snapshot, max_rows);
        let shown = self
            .body(&layout, &items, snapshot)
            .iter()
            .filter(|row| matches!(row, BodyRow::Model(_)))
            .count();
        u16::try_from(shown).unwrap_or(u16::MAX)
    }

    /// The menu's rows in `max_rows` rows of a `cols`-wide screen, top first:
    /// exactly as many as [`Self::height`] gave the band when `max_rows` is
    /// that height (`composeSettingsMenuRow`, `:73-95`).
    pub(crate) fn rows(
        &self,
        snapshot: &Snapshot<'_>,
        cols: u16,
        max_rows: u16,
        palette: Palette,
    ) -> Vec<String> {
        let items = self.items(snapshot);
        let layout = self.layout(&items, snapshot, max_rows);
        let mut rows = Vec::with_capacity(usize::from(layout.rows));
        if layout.rows == 0 {
            return rows;
        }
        if layout.header {
            rows.push(self.header(items.len(), cols, palette));
            rows.push(String::new());
        }
        if items.is_empty() {
            rows.push(format!(
                "{}{}{}",
                palette.dim(),
                ellipsized(EMPTY, usize::from(cols)),
                palette.reset()
            ));
            return rows;
        }
        let column = value_column(&items, cols);
        for row in self.body(&layout, &items, snapshot) {
            rows.push(match row {
                BodyRow::Item(index) => item_row(
                    items[index],
                    snapshot,
                    index == layout.selected,
                    column,
                    cols,
                    palette,
                ),
                BodyRow::Model(offset) => match snapshot.models {
                    Some(menu) => model_row(menu, offset, column, cols, palette),
                    None => String::new(),
                },
            });
        }
        rows
    }

    /// `Settings N` and the tabs (`composeBrowseHeader`, `:185-210`): every
    /// tab when they fit; narrower, the title and the active tab; narrower
    /// still, the active tab alone, cut at the screen's edge.
    fn header(&self, count: usize, cols: u16, palette: Palette) -> String {
        let width = usize::from(cols);
        let title = format!("{}Settings {count}{}", palette.subtitle(), palette.reset());
        let mut wide = title.clone();
        for category in Category::ALL {
            wide.push_str(GAP);
            wide.push_str(&painted_tab(category, category == self.category, palette));
        }
        if cells(&wide) <= width {
            return wide;
        }
        let active = painted_tab(self.category, true, palette);
        let compact = format!("{title}{GAP}{active}");
        if cells(&compact) <= width {
            return compact;
        }
        // Cut inside its colour, so the reset is never what the cut drops.
        format!(
            "{}{}{}",
            palette.subtitle(),
            super::frame::clip(&format!("[{}]", self.category.label()), cols),
            palette.reset()
        )
    }
}

/// How many settings from `first` fit in `budget` rows, and the rows they
/// take with the Model row's list under it (`measureBrowseBody`, `:140-158`):
/// a setting takes its row, and the Model row as many of its list's as are
/// left. With the list open and one row to give, the one row is the list's.
fn measure(items: &[&Spec], snapshot: &Snapshot<'_>, first: usize, budget: u16) -> (usize, u16) {
    let mut visible = 0;
    let mut rows: u16 = 0;
    for spec in &items[first..] {
        if rows + 1 > budget {
            break;
        }
        let list = if spec.id == Setting::Model {
            snapshot.model_rows().min(budget - rows - 1)
        } else {
            0
        };
        rows += 1 + list;
        visible += 1;
    }
    if snapshot.models.is_some() && budget == 1 && visible > 0 {
        rows = 1;
    }
    (visible, rows)
}

/// One row of the Model row's list (`composeModelRow`, `:277-310`): a model
/// id at the value column, the marked one bold and the rest dim, in a window
/// of at most [`MOST_MODELS`] kept round the mark -- or, with no model to
/// show, what the list is waiting for, dim and four columns in.
fn model_row(
    menu: &ModelMenu,
    offset: usize,
    column: usize,
    cols: u16,
    palette: Palette,
) -> String {
    let width = usize::from(cols);
    let (ids, selected, window_start) = match menu.inline() {
        Inline::Listed {
            ids,
            selected,
            window_start,
        } if !ids.is_empty() => (ids, selected, window_start),
        state => {
            let text = match state {
                Inline::Loading => MODELS_LOADING,
                Inline::Failed => MODELS_FAILED,
                Inline::Listed { .. } => MODELS_NONE,
            };
            return format!(
                "{}{}{}{}",
                " ".repeat(MODEL_STATE_INDENT.min(width)),
                palette.dim(),
                ellipsized(text, width.saturating_sub(MODEL_STATE_INDENT)),
                palette.reset()
            );
        }
    };
    let count = ids.len();
    let selected = selected % count;
    let shown = count.min(MOST_MODELS);
    let mut start = window_start.min(selected);
    if selected >= start + shown {
        start = selected + 1 - shown;
    }
    let index = start.min(count - shown) + offset;
    let Some(id) = ids.get(index) else {
        return String::new();
    };
    let mut row = String::new();
    pad_to(&mut row, column);
    row.push_str(if index == selected {
        palette.subtitle()
    } else {
        palette.dim()
    });
    row.push_str(&ellipsized(id, width.saturating_sub(column)));
    row.push_str(palette.reset());
    row
}

/// One tab: the active one bold in brackets, the rest dim
/// (`appendCategoryTab`, `:220-231`).
fn painted_tab(category: Category, active: bool, palette: Palette) -> String {
    if active {
        format!(
            "{}[{}]{}",
            palette.subtitle(),
            category.label(),
            palette.reset()
        )
    } else {
        format!("{}{}{}", palette.dim(), category.label(), palette.reset())
    }
}

/// One setting's row (`composeItemRow`, `:233-275`).
///
/// The indent, the label in the row's style and closed, uncoloured spaces out
/// to the value column; then each choice, two spaces apart, the one in force
/// bold and the rest dim -- or, for a setting with no choices, its value in
/// bold. A choice that would run past the screen is cut there and ends the
/// row.
fn item_row(
    spec: &Spec,
    snapshot: &Snapshot<'_>,
    marked: bool,
    column: usize,
    cols: u16,
    palette: Palette,
) -> String {
    let width = usize::from(cols);
    let indent = if width <= INDENT { 0 } else { INDENT };
    let mut row = " ".repeat(indent);
    row.push_str(if marked {
        palette.subtitle()
    } else {
        palette.dim()
    });
    row.push_str(&ellipsized(spec.label, column.saturating_sub(indent)));
    row.push_str(palette.reset());
    pad_to(&mut row, column);
    let value = snapshot.value(spec.id);
    let choices = options(spec.id, snapshot);
    if choices.is_empty() {
        row.push_str(palette.subtitle());
        row.push_str(&ellipsized(value, width.saturating_sub(column)));
        row.push_str(palette.reset());
        return row;
    }
    for (index, choice) in choices.iter().enumerate() {
        let used = cells(&row);
        if used >= width {
            break;
        }
        if index > 0 {
            row.push_str(&" ".repeat(GAP.len().min(width - used)));
        }
        row.push_str(if choice.eq_ignore_ascii_case(value) {
            palette.subtitle()
        } else {
            palette.dim()
        });
        row.push_str(&ellipsized(choice, width.saturating_sub(cells(&row))));
        row.push_str(palette.reset());
    }
    row
}

/// Where the values start: past the indent, the widest label **in the
/// filtered list** -- not the window -- and the gap (`valueColumn`,
/// `:312-320`). So the column moves as the search or the tab narrows the list.
fn value_column(items: &[&Spec], cols: u16) -> usize {
    let width = usize::from(cols);
    let indent = if width <= INDENT { 0 } else { INDENT };
    let widest = items
        .iter()
        .map(|spec| cells(spec.label))
        .max()
        .unwrap_or(0);
    (indent + widest + COLUMN_GAP).min(width)
}

/// Whether every word of the search is somewhere in the setting's label,
/// description, value or name, case aside (`matchesQuery`, `:383-396`).
fn found(spec: &Spec, snapshot: &Snapshot<'_>, words: &[String]) -> bool {
    words.iter().all(|word| {
        [
            spec.label,
            spec.description,
            snapshot.value(spec.id),
            spec.id.tag(),
        ]
        .into_iter()
        .any(|text| text.to_ascii_lowercase().contains(word.as_str()))
    })
}

/// The rows a screen `terminal_rows` tall gives the menu when its composer is
/// `input_extra` rows taller than one (`inlinePickerRowBudgetCapped`,
/// `picker_presentation.zig:760-770`): what is left after the band's fixed
/// rows and five rows of document, at most [`MOST_ROWS`] and never none.
fn row_budget(terminal_rows: u16, input_extra: u16) -> u16 {
    terminal_rows
        .saturating_sub(FIXED_ROWS.saturating_add(input_extra))
        .saturating_sub(MINIMUM_TRANSCRIPT_ROWS)
        .clamp(1, MOST_ROWS)
}

/// What the hint row says while the menu is up, on a screen `cols` wide.
///
/// The widest of [`KEY_HELP`] that fits, and the narrowest when none does.
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

    use super::super::theme::Mode;

    const PALETTE: Palette = Palette { mode: Mode::Dark };

    /// Upstream's `selected_completion_style`: the title, the active tab, the
    /// marked label and the value in force.
    const BRIGHT: &str = "\u{1b}[1;38;5;255m";

    /// Upstream's `dim_style`.
    const DIM: &str = "\u{1b}[38;5;245m";

    const RESET: &str = "\u{1b}[0m";

    /// What a session launched by the capture harness holds: every segment
    /// off, the categories shown, groups expanded, `auto`, sound off, history
    /// kept.
    fn launched() -> Snapshot<'static> {
        Snapshot {
            statusline: StatusLine::default(),
            slash_menu_categories: true,
            collapse_tool_calls: false,
            model: "fake/model-a",
            effort: "default",
            efforts: &[],
            fast: false,
            ultrafast: false,
            offers_fast: false,
            offers_ultrafast: false,
            permission_mode: PermissionMode::Auto,
            session_titles: true,
            sound: Notifications::both(false),
            startup_scrollback: true,
            prompt_history: true,
            models: None,
        }
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

    fn labels(menu: &SettingsMenu, snapshot: &Snapshot<'_>) -> Vec<&'static str> {
        menu.items(snapshot).iter().map(|spec| spec.label).collect()
    }

    fn on_tab(tabs: usize) -> SettingsMenu {
        let mut menu = SettingsMenu::open("");
        for _ in 0..tabs {
            menu.cycle(true);
        }
        menu
    }

    /// A choice row as upstream paints it: the label at the indent and padded
    /// to column 27 (2 + `Status line workspace` + 4), then each choice with
    /// the one in force bright.
    fn choice_row(label: &str, marked: bool, choices: &[&str], current: &str) -> String {
        let style = if marked { BRIGHT } else { DIM };
        let mut row = format!("  {style}{label}{RESET}{}", " ".repeat(25 - label.len()));
        for (index, choice) in choices.iter().enumerate() {
            if index > 0 {
                row.push_str("  ");
            }
            let style = if *choice == current { BRIGHT } else { DIM };
            row.push_str(&format!("{style}{choice}{RESET}"));
        }
        row
    }

    #[test]
    fn the_bare_menu_is_upstreams_rows_less_the_settings_xfx_has_nothing_behind() {
        // `cap/fx-settings-40x120-bare.ansi` rows 5-20, with `Sound level`
        // cycling `off  on`.
        let rows = SettingsMenu::open("").rows(&launched(), 120, 40, PALETTE);
        let tab = |label: &str| format!("  {DIM}{label}{RESET}");
        let mut expected = vec![
            format!(
                "{BRIGHT}Settings 14{RESET}  {BRIGHT}[All]{RESET}{}{}{}{}",
                tab("Interface"),
                tab("Agent"),
                tab("Notifications"),
                tab("Advanced")
            ),
            String::new(),
        ];
        expected.push(choice_row("Status line context", true, &ON_OFF, "off"));
        expected.push(choice_row("Status line session", false, &ON_OFF, "off"));
        expected.push(choice_row("Status line workspace", false, &ON_OFF, "off"));
        expected.push(choice_row("Slash menu categories", false, &ON_OFF, "on"));
        expected.push(choice_row("Collapse tool calls", false, &ON_OFF, "off"));
        expected.push(format!(
            "  {DIM}Model{RESET}{}{BRIGHT}fake/model-a{RESET}",
            " ".repeat(20)
        ));
        // A model with no levels and the default in force: the value alone.
        expected.push(format!(
            "  {DIM}Reasoning effort{RESET}{}{BRIGHT}default{RESET}",
            " ".repeat(9)
        ));
        // A model that offers no speed, with none on: the value alone.
        expected.push(format!(
            "  {DIM}Fast mode{RESET}{}{BRIGHT}off{RESET}",
            " ".repeat(16)
        ));
        expected.push(format!(
            "  {DIM}Ultra mode{RESET}{}{BRIGHT}off{RESET}",
            " ".repeat(15)
        ));
        expected.push(choice_row(
            "Permission mode",
            false,
            &PERMISSION_MODES,
            "auto",
        ));
        // Upstream's default, on (`settings_catalog.zig:63`).
        expected.push(choice_row("Session titles", false, &ON_OFF, "on"));
        expected.push(choice_row("Sound level", false, &ON_OFF, "off"));
        expected.push(choice_row("Startup scrollback", false, &ON_OFF, "on"));
        expected.push(choice_row("Prompt history", false, &ON_OFF, "on"));
        assert_eq!(rows, expected);
        assert_eq!(SettingsMenu::open("").height(&launched(), 40, 0), 16);
    }

    #[test]
    fn the_session_titles_row_flips_between_off_and_on() {
        // `settings_catalog.zig:272`: an on/off row in Agent, found by its
        // label, its description and upstream's tag.
        let snapshot = launched();
        for query in ["titles", "first prompt", "session_titles"] {
            assert_eq!(
                labels(&SettingsMenu::open(query), &snapshot),
                ["Session titles"],
                "{query}"
            );
        }
        let menu = SettingsMenu::open("titles");
        assert_eq!(
            menu.change(true, &snapshot),
            Some(Change {
                setting: Setting::SessionTitles,
                value: "off".to_string()
            })
        );
        let off = Snapshot {
            session_titles: false,
            ..launched()
        };
        assert_eq!(
            menu.change(false, &off).map(|c| c.value).as_deref(),
            Some("on")
        );
        assert_eq!(
            plain(&menu.rows(&off, 120, 40, PALETTE)[2]),
            "  Session titles    off  on"
        );
    }

    #[test]
    fn the_speed_rows_have_choices_only_for_a_model_that_offers_the_speed_or_while_it_is_on() {
        // `optionCount`, `settings_catalog.zig:314-315`: no choice for a model
        // without the speed, so Left and Right change nothing there; on and
        // off for one that has it -- and while the speed is on, even after a
        // move to a model without it, so the way back off is still a key.
        let mut snapshot = launched();
        let menu = on_tab(2);
        let mut fast = menu.clone();
        assert!(fast.step_by(1, 5, &snapshot) && fast.step_by(1, 5, &snapshot));
        assert_eq!(fast.selected(&snapshot), Some(Setting::FastMode));
        assert_eq!(fast.change(true, &snapshot), None);
        snapshot.offers_fast = true;
        assert_eq!(
            fast.change(true, &snapshot),
            Some(Change {
                setting: Setting::FastMode,
                value: "on".to_string()
            })
        );
        let rows = fast.rows(&snapshot, 120, 40, PALETTE);
        assert_eq!(plain(&rows[4]), "  Fast mode           off  on");
        snapshot.offers_fast = false;
        snapshot.fast = true;
        assert_eq!(
            fast.change(false, &snapshot),
            Some(Change {
                setting: Setting::FastMode,
                value: "off".to_string()
            })
        );
        // Ultra the same, by its own offer.
        let mut ultra = fast.clone();
        assert!(ultra.step_by(1, 5, &snapshot));
        assert_eq!(ultra.selected(&snapshot), Some(Setting::UltraMode));
        assert_eq!(ultra.change(true, &snapshot), None);
        snapshot.offers_ultrafast = true;
        assert_eq!(
            ultra.change(true, &snapshot).map(|change| change.value),
            Some("on".to_string())
        );
        // The search finds them by upstream's names and words.
        assert_eq!(
            labels(&SettingsMenu::open("ultrafast"), &snapshot),
            ["Ultra mode"]
        );
        assert_eq!(
            labels(&SettingsMenu::open("faster inference"), &snapshot),
            ["Fast mode"]
        );
    }

    #[test]
    fn tab_walks_all_and_upstreams_four_categories_and_wraps() {
        // `fx-settings2-40x120-tab1..5`: Interface, Agent, Notifications,
        // Advanced, All -- with xfx's settings on each.
        let snapshot = launched();
        assert_eq!(labels(&on_tab(0), &snapshot).len(), 14);
        assert_eq!(
            labels(&on_tab(1), &snapshot),
            [
                "Status line context",
                "Status line session",
                "Status line workspace",
                "Slash menu categories",
                "Collapse tool calls"
            ]
        );
        assert_eq!(
            labels(&on_tab(2), &snapshot),
            [
                "Model",
                "Reasoning effort",
                "Fast mode",
                "Ultra mode",
                "Permission mode",
                "Session titles"
            ]
        );
        assert_eq!(labels(&on_tab(3), &snapshot), ["Sound level"]);
        assert_eq!(
            labels(&on_tab(4), &snapshot),
            ["Startup scrollback", "Prompt history"]
        );
        assert_eq!(on_tab(5), SettingsMenu::open(""));
        // Shift-Tab is the other way round.
        let mut back = SettingsMenu::open("");
        back.cycle(false);
        assert_eq!(back.category, Category::Advanced);
        // The header counts the tab's settings and brackets the tab.
        let rows = on_tab(3).rows(&snapshot, 120, 40, PALETTE);
        assert!(plain(&rows[0]).starts_with("Settings 1  All  Interface  Agent  [Notifications]"));
    }

    #[test]
    fn the_value_column_follows_the_widest_label_on_the_tab() {
        // `fx-settings2-40x120-tab3`: Notifications alone, so its column is
        // 2 + `Sound level` + 4.
        let rows = on_tab(3).rows(&launched(), 120, 40, PALETTE);
        assert_eq!(plain(&rows[2]), "  Sound level    off  on");
        let rows = on_tab(2).rows(&launched(), 120, 40, PALETTE);
        assert_eq!(plain(&rows[2]), "  Model               fake/model-a");
    }

    #[test]
    fn a_tab_change_and_a_new_search_put_the_mark_back_on_top() {
        let snapshot = launched();
        let mut menu = SettingsMenu::open("");
        assert!(menu.step_by(1, 10, &snapshot));
        assert!(menu.step_by(1, 10, &snapshot));
        assert_eq!(
            menu.selected(&snapshot),
            Some(Setting::Statusline(StatusLineItem::Workspace))
        );
        menu.cycle(true);
        assert_eq!(menu.selected, 0);
        menu.step_by(1, 10, &snapshot);
        menu.set_query("s");
        assert_eq!(menu.selected, 0);
        // The same text again is not a new search.
        menu.step_by(1, 10, &snapshot);
        menu.set_query("s");
        assert_eq!(menu.selected, 1);
    }

    #[test]
    fn up_and_down_wrap_at_both_ends() {
        let snapshot = launched();
        let mut menu = SettingsMenu::open("");
        assert!(menu.step_by(-1, 10, &snapshot));
        assert_eq!(menu.selected(&snapshot), Some(Setting::PromptHistory));
        assert!(menu.step_by(1, 10, &snapshot));
        assert_eq!(
            menu.selected(&snapshot),
            Some(Setting::Statusline(StatusLineItem::Context))
        );
    }

    #[test]
    fn the_search_reads_labels_descriptions_values_and_upstreams_names() {
        // `fx-settings2-40x120-filter-sou`: `sou` finds `Sound level` and, by
        // its description's "skill sources", `Slash menu categories`.
        let snapshot = launched();
        assert_eq!(
            labels(&SettingsMenu::open("sou"), &snapshot),
            ["Slash menu categories", "Sound level"]
        );
        // A value: the model's id.
        assert_eq!(labels(&SettingsMenu::open("MODEL-A"), &snapshot), ["Model"]);
        // `effort` is the tag and is in the description.
        assert_eq!(
            labels(&SettingsMenu::open("effort"), &snapshot),
            ["Reasoning effort"]
        );
        // Upstream's tag, and every word has to be found.
        assert_eq!(
            labels(&SettingsMenu::open("prompt_history"), &snapshot),
            ["Prompt history"]
        );
        assert_eq!(
            labels(&SettingsMenu::open("status workspace"), &snapshot),
            ["Status line workspace"]
        );
        // `fx-settings2-40x120-filter-none`: one dim row and the header.
        let rows = SettingsMenu::open("souzzz").rows(&snapshot, 120, 40, PALETTE);
        assert_eq!(
            rows,
            [
                format!(
                    "{BRIGHT}Settings 0{RESET}  {BRIGHT}[All]{RESET}  {DIM}Interface{RESET}  \
                     {DIM}Agent{RESET}  {DIM}Notifications{RESET}  {DIM}Advanced{RESET}"
                ),
                String::new(),
                format!("{DIM}No settings found.{RESET}"),
            ]
        );
        assert_eq!(SettingsMenu::open("souzzz").height(&snapshot, 40, 0), 3);
    }

    #[test]
    fn a_change_is_the_next_or_previous_choice_wrapping() {
        let snapshot = launched();
        let menu = SettingsMenu::open("");
        assert_eq!(
            menu.change(true, &snapshot),
            Some(Change {
                setting: Setting::Statusline(StatusLineItem::Context),
                value: "on".to_string()
            })
        );
        // Two choices: either way is a flip.
        assert_eq!(
            menu.change(false, &snapshot).map(|c| c.value).as_deref(),
            Some("on")
        );
        let permission = SettingsMenu::open("permission");
        assert_eq!(
            permission
                .change(true, &snapshot)
                .map(|c| c.value)
                .as_deref(),
            Some("full access")
        );
        assert_eq!(
            permission
                .change(false, &snapshot)
                .map(|c| c.value)
                .as_deref(),
            Some("ask")
        );
        let full = Snapshot {
            permission_mode: PermissionMode::Yolo,
            ..launched()
        };
        assert_eq!(
            permission.change(true, &full).map(|c| c.value).as_deref(),
            Some("ask")
        );
        // The model has no choices: its Right is the shell's.
        assert_eq!(SettingsMenu::open("model").change(true, &snapshot), None);
        // Nothing listed, nothing changed.
        assert_eq!(SettingsMenu::open("zzz").change(true, &snapshot), None);
    }

    #[test]
    fn sound_set_apart_by_a_profile_is_custom_and_starts_from_the_first_choice() {
        // `notificationLevel`: one switch on and the other off is neither.
        let custom = Snapshot {
            sound: Notifications {
                turn_end: true,
                attention_required: false,
            },
            ..launched()
        };
        let menu = SettingsMenu::open("sound");
        let rows = menu.rows(&custom, 120, 40, PALETTE);
        assert!(
            rows[2].ends_with(&format!("{DIM}off{RESET}  {DIM}on{RESET}")),
            "{:?}",
            rows[2]
        );
        assert_eq!(
            menu.change(true, &custom).map(|c| c.value).as_deref(),
            Some("on")
        );
        assert_eq!(
            menu.change(false, &custom).map(|c| c.value).as_deref(),
            Some("on")
        );
    }

    #[test]
    fn a_narrow_screen_keeps_the_title_and_the_active_tab_then_the_tab_alone() {
        let snapshot = launched();
        let menu = on_tab(2);
        assert_eq!(
            plain(&menu.rows(&snapshot, 40, 40, PALETTE)[0]),
            "Settings 6  [Agent]"
        );
        assert_eq!(plain(&menu.rows(&snapshot, 12, 40, PALETTE)[0]), "[Agent]");
        assert_eq!(plain(&menu.rows(&snapshot, 5, 40, PALETTE)[0]), "[Agen");
    }

    #[test]
    fn a_short_screen_drops_the_header_then_shows_the_marked_row_alone() {
        let snapshot = launched();
        let mut menu = SettingsMenu::open("");
        for _ in 0..5 {
            menu.step_by(1, 1, &snapshot);
        }
        // Two rows: no header, the mark kept in view -- at the top of the
        // window its last move left (`buildBrowseLayout`'s `window_start`).
        let rows = menu.rows(&snapshot, 120, 2, PALETTE);
        assert_eq!(rows.len(), 2);
        assert!(plain(&rows[0]).starts_with("  Model"), "{rows:?}");
        assert!(
            plain(&rows[1]).starts_with("  Reasoning effort"),
            "{rows:?}"
        );
        let rows = menu.rows(&snapshot, 120, 1, PALETTE);
        assert_eq!(rows.len(), 1);
        assert!(plain(&rows[0]).starts_with("  Model"), "{rows:?}");
        // Eleven rows of screen leave the menu one row: five for the band's
        // own and five of document (`row_budget`).
        assert_eq!(menu.height(&snapshot, 11, 0), 1);
        assert_eq!(menu.height(&snapshot, 12, 0), 2);
        assert_eq!(menu.height(&snapshot, 13, 0), 3);
        // A taller composer takes its rows from the menu.
        assert_eq!(menu.height(&snapshot, 13, 1), 2);
    }

    #[test]
    fn the_effort_row_is_default_and_the_models_levels_or_its_value_alone() {
        // `fx-settings-effort-40x120-bare`: `zai/glm-5.2` lists low and high,
        // so the row is `default  low  high` with the default bold.
        let levels = ["low".to_string(), "high".to_string()];
        let glm = Snapshot {
            model: "zai/glm-5.2",
            efforts: &levels,
            ..launched()
        };
        let menu = SettingsMenu::open("reasoning");
        assert_eq!(
            menu.rows(&glm, 120, 40, PALETTE)[2],
            format!(
                "  {BRIGHT}Reasoning effort{RESET}    {BRIGHT}default{RESET}  {DIM}low{RESET}  {DIM}high{RESET}"
            )
        );
        // Right walks them and wraps (`-right1..3`), Left goes back (`-left`).
        assert_eq!(
            menu.change(true, &glm).map(|c| c.value).as_deref(),
            Some("low")
        );
        let high = Snapshot {
            effort: "high",
            ..glm
        };
        assert_eq!(
            menu.change(true, &high).map(|c| c.value).as_deref(),
            Some("default")
        );
        assert_eq!(
            menu.change(false, &glm).map(|c| c.value).as_deref(),
            Some("high")
        );
        // A model with no levels, the default in force: the value alone, bold,
        // and Right changes nothing.
        assert_eq!(menu.change(true, &launched()), None);
        // The same model with a level carried over from another
        // (`fx-settings-effort-40x120-no-levels`): `default` alone and dim, and
        // Right puts the default back (`-no-levels-right`).
        let carried = Snapshot {
            effort: "high",
            ..launched()
        };
        assert_eq!(
            menu.rows(&carried, 120, 40, PALETTE)[2],
            format!("  {BRIGHT}Reasoning effort{RESET}    {DIM}default{RESET}")
        );
        assert_eq!(
            menu.change(true, &carried).map(|c| c.value).as_deref(),
            Some("default")
        );
    }

    #[test]
    fn the_hint_row_is_upstreams_widest_key_help_that_fits() {
        assert_eq!(
            key_help(120),
            "\u{2191}\u{2193} navigate     tab category     \u{2190}\u{2192} change     esc close"
        );
        assert_eq!(
            key_help(50),
            "\u{2191}\u{2193} navigate  tab category  \u{2190}\u{2192} change  esc close"
        );
        assert_eq!(key_help(30), "tab category  \u{2190}\u{2192} change  esc");
        assert_eq!(key_help(5), "tab \u{2190}\u{2192} esc");
    }

    #[test]
    fn enter_left_and_right_are_the_menus_and_letters_go_to_the_search() {
        assert_eq!(
            SettingsAction::of(Action::Left),
            Some(SettingsAction::Previous)
        );
        assert_eq!(
            SettingsAction::of(Action::Right),
            Some(SettingsAction::Next)
        );
        assert_eq!(
            SettingsAction::of(Action::Submit),
            Some(SettingsAction::Open)
        );
        assert_eq!(
            SettingsAction::of(Action::BackTab),
            Some(SettingsAction::PreviousCategory)
        );
        assert_eq!(SettingsAction::of(Action::Backspace), None);
    }
}
