//! The session's reasoning effort: one value, for every model.
//!
//! Upstream's `ReasoningEffort` (`vercel-labs/fx@c1db9191
//! src/core/shared/types.zig:2431-2510`): the default, which asks the provider
//! for nothing, or a name the provider publishes. **Global, not per model** --
//! upstream carries it across a model switch (`fx-model-40x120-arg-opus`
//! switched from astra `low` to opus `low`) and stores it under one settings
//! key, `effort`.
//!
//! Whether a model *takes* the effort in force is the catalog's answer
//! ([`Effort::supported_by`]), and it is asked twice: by the picker, which
//! offers only what the row lists, and before each turn, which sends the
//! effort only to a model that lists it (`resolveProviderOptionsForCapabilities`,
//! `model_capabilities.zig:194-206`). A model the catalog does not describe
//! takes none, which is upstream's fallback too (`capabilitiesForModel`,
//! `:117-119`, knows no efforts).
//!
//! # And the speed
//!
//! The session's two speed switches live here beside it, for the same reasons:
//! Fast and Ultra are one setting each for the whole session (`fast_mode`,
//! `ultrafast_mode`), a model *takes* one only when its catalog row says so
//! (`supportsFastMode`, `supportsUltrafastMode`, `gateway.zig:2918-2937`), and
//! they are what the `/model` picker's third column chooses ([`Speed`]). The
//! two are exclusive -- turning either on turns the other off
//! (`applyFastMode`, `applyUltrafastMode`, `session_commands.zig:753-859`) --
//! and **both go off whenever the model changes** (`setResolvedModel`,
//! `:1168-1190`). [`TurnSettings`] is the three of them together, as a turn is
//! run with them and as a session records them.

use super::model::CatalogEntry;

/// The session's effort and its two speed switches, as one value: what a turn
/// is run with, what a session records ([`crate::session::SessionEvent`]'s
/// `PreferencesChanged`), and what a resume puts back
/// (`restoreRuntimePreferences`, `app_session_runtime.zig:6126-6175`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnSettings {
    pub effort: Effort,
    /// Fast mode (`fast_mode`).
    pub fast: bool,
    /// Ultra mode (`ultrafast_mode`). Never on together with [`Self::fast`].
    pub ultrafast: bool,
}

impl TurnSettings {
    /// The speed these settings ask for.
    pub fn speed(&self) -> Speed {
        Speed::of(self.fast, self.ultrafast)
    }

    /// After `/model <query>` (`setResolvedModel`, `session_commands.zig:
    /// 1168-1190`): the effort stands, and both switches go off when the model
    /// changed.
    pub fn after_query(&self, changed: bool) -> Self {
        if !changed {
            return self.clone();
        }
        Self {
            effort: self.effort.clone(),
            fast: false,
            ultrafast: false,
        }
    }

    /// After a model chosen in a picker or by the explicit form
    /// (`selectModelFromPicker`, `:886-915`): the effort chosen with it, and
    /// the speed asked for -- or, with no speed column, the Fast mode the
    /// session is in and no Ultra -- **as far as `entry` offers it**. A model
    /// the catalog does not describe offers neither.
    pub fn after_pick(
        &self,
        effort: Option<&Effort>,
        speed: Option<Speed>,
        entry: Option<&CatalogEntry>,
    ) -> Self {
        let (fast, ultrafast) = match speed {
            Some(speed) => (speed == Speed::Fast, speed == Speed::Ultrafast),
            None => (self.fast, false),
        };
        let offers_fast = entry.is_some_and(|entry| entry.facts.fast);
        let offers_ultrafast = entry.is_some_and(|entry| entry.facts.ultrafast);
        let ultrafast = offers_ultrafast && ultrafast;
        Self {
            effort: effort.cloned().unwrap_or_else(|| self.effort.clone()),
            fast: offers_fast && fast && !ultrafast,
            ultrafast,
        }
    }

    /// `/fast` (`toggleFastForModel`, `:733-751`): off when it is on; on --
    /// and Ultra off -- when the model offers it; `None`, nothing changed,
    /// when it does not.
    pub fn after_fast(&self, entry: Option<&CatalogEntry>) -> Option<Self> {
        if self.fast {
            return Some(Self {
                fast: false,
                ..self.clone()
            });
        }
        entry.filter(|entry| entry.facts.fast).map(|_| Self {
            fast: true,
            ultrafast: false,
            ..self.clone()
        })
    }

    /// `/ultrafast on|off` (`applyUltrafastMode`, `:802-859`): on -- and Fast
    /// off -- only for a model that offers it, `None` otherwise; off always.
    pub fn after_ultrafast(&self, on: bool, entry: Option<&CatalogEntry>) -> Option<Self> {
        if on && !entry.is_some_and(|entry| entry.facts.ultrafast) {
            return None;
        }
        Some(Self {
            ultrafast: on,
            fast: !on && self.fast,
            ..self.clone()
        })
    }
}

/// One row of the `/model` picker's speed column, and the speed a model change
/// makes (`speedPickerLabels`, `input_completion_runtime.zig:1377-1394`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Speed {
    /// Neither switch.
    #[default]
    Normal,
    /// Fast mode.
    Fast,
    /// Ultra mode.
    Ultrafast,
}

impl Speed {
    /// The three, in the column's order (`model_picker_speed_options`,
    /// `picker_state.zig:48`): what a marked speed is an index into, whatever
    /// a model offers.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Fast, Self::Ultrafast];

    /// Where this speed is in [`Self::ALL`].
    pub fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Fast => 1,
            Self::Ultrafast => 2,
        }
    }

    /// The word the column and the explicit form use.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Fast => "fast",
            Self::Ultrafast => "ultrafast",
        }
    }

    /// The speed `word` names, any case (`speedSelection`).
    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|speed| word.eq_ignore_ascii_case(speed.label()))
    }

    /// The speed a pair of switches makes; Ultra outranks Fast, as turning it
    /// on turns Fast off.
    pub fn of(fast: bool, ultrafast: bool) -> Self {
        match (fast, ultrafast) {
            (_, true) => Self::Ultrafast,
            (true, false) => Self::Fast,
            (false, false) => Self::Normal,
        }
    }

    /// What the column offers for `entry`, in order: `normal`, then `fast`
    /// and `ultrafast` where the row offers them.
    pub fn options(entry: &CatalogEntry) -> Vec<Self> {
        let mut options = vec![Self::Normal];
        if entry.facts.fast {
            options.push(Self::Fast);
        }
        if entry.facts.ultrafast {
            options.push(Self::Ultrafast);
        }
        options
    }

    /// Whether `entry` offers a speed to choose at all -- which is what puts
    /// a speed column after the model (`beginModelPickerOptions`).
    pub fn chosen_for(entry: &CatalogEntry) -> bool {
        entry.facts.fast || entry.facts.ultrafast
    }
}

/// Whether `model` is a fast one by its name (`capabilitiesForModel`,
/// `model_capabilities.zig:117-119`): its id ends `-fast`. Such a model is
/// announced at `speed: fast` and marked on the footer whatever the switch
/// says, and it is not a switch -- nothing is sent for it.
pub fn intrinsic_fast(model: &str) -> bool {
    model.ends_with("-fast")
}

/// The most bytes a named effort may have (`ReasoningEffort.max_name_bytes`).
const MAX_NAME_BYTES: usize = 64;

/// A reasoning effort: the provider's default, or one it names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Effort {
    /// Ask for nothing, and let the provider decide. What a session starts in.
    #[default]
    Auto,
    /// A level the catalog publishes, spelled exactly as it publishes it.
    Named(String),
}

impl Effort {
    /// Reads a typed or stored effort (`ReasoningEffort.parse`).
    ///
    /// `auto`, `adaptive` and `default` -- any case -- are the default; anything
    /// else is a name of letters, digits, `-`, `_` and `.`, up to 64 bytes,
    /// **kept as written**: upstream compares names byte for byte
    /// (`model_capabilities.zig:162-168`), so `High` is not `high` and a picker
    /// that lowercased would accept an effort the catalog never offered.
    pub fn parse(raw: &str) -> Option<Self> {
        if ["auto", "adaptive", "default"]
            .iter()
            .any(|default| raw.eq_ignore_ascii_case(default))
        {
            return Some(Self::Auto);
        }
        let usable = !raw.is_empty()
            && raw.len() <= MAX_NAME_BYTES
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        usable.then(|| Self::Named(raw.to_string()))
    }

    /// The word a composer and a settings file hold: `auto` for the default
    /// (`label`), which is what `/model <id> auto ` reads and what `effort`
    /// is written as.
    pub fn label(&self) -> &str {
        match self {
            Self::Auto => "auto",
            Self::Named(name) => name,
        }
    }

    /// The word a person is shown: `default` for the default
    /// (`displayLabel`), which is what the announcement, the picker's first
    /// row and the footer say.
    pub fn display(&self) -> &str {
        match self {
            Self::Auto => "default",
            Self::Named(name) => name,
        }
    }

    /// Whether this is the provider's own default.
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Whether `entry` takes this effort (`reasoningEffortSupported`): the
    /// default always, a name only when the row lists it.
    pub fn supported_by(&self, entry: &CatalogEntry) -> bool {
        match self {
            Self::Auto => true,
            Self::Named(name) => entry.efforts.iter().any(|offered| offered == name),
        }
    }

    /// What a picker offers for `entry`, in order: the default, then the
    /// catalog's levels as it lists them -- or nothing at all for a model that
    /// takes no effort (`reasoningEffortOptionCount`, `:189-191`), which is
    /// what skips the effort column.
    pub fn options(entry: &CatalogEntry) -> Vec<Self> {
        if entry.efforts.is_empty() {
            return Vec::new();
        }
        std::iter::once(Self::Auto)
            .chain(entry.efforts.iter().cloned().map(Self::Named))
            .collect()
    }

    /// What a request carries for this effort to `entry`'s model, if anything
    /// (`resolveProviderOptionsForCapabilities`): nothing for the default, and
    /// nothing for a level the model does not list.
    pub fn for_request(&self, entry: Option<&CatalogEntry>) -> Option<String> {
        match (self, entry) {
            (Self::Named(name), Some(entry)) if self.supported_by(entry) => Some(name.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::model::ModelFacts;

    fn entry(efforts: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: "openai/gpt-6-astra".to_string(),
            aliases: Vec::new(),
            name: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            max_context: None,
            facts: ModelFacts::default(),
        }
    }

    #[test]
    fn a_model_change_turns_the_switches_off_and_a_pick_keeps_what_the_model_offers() {
        let on = TurnSettings {
            effort: Effort::Named("low".to_string()),
            fast: true,
            ultrafast: false,
        };
        // The router form: off when the model changed, kept when it did not.
        assert!(!on.after_query(true).fast);
        assert_eq!(on.after_query(true).effort, on.effort);
        assert_eq!(on.after_query(false), on);
        // A pick with no speed column keeps Fast where the model offers it.
        let mut fast_row = entry(&["low"]);
        fast_row.facts.fast = true;
        assert!(on.after_pick(None, None, Some(&fast_row)).fast);
        assert!(!on.after_pick(None, None, Some(&entry(&[]))).fast);
        assert!(
            !on.after_pick(None, None, None).fast,
            "no row offers nothing"
        );
        // A chosen speed, as far as the row offers it.
        let mut both = fast_row.clone();
        both.facts.ultrafast = true;
        let ultra = on.after_pick(None, Some(Speed::Ultrafast), Some(&both));
        assert!(ultra.ultrafast && !ultra.fast);
        assert!(
            !on.after_pick(None, Some(Speed::Ultrafast), Some(&fast_row))
                .ultrafast
        );
        let normal = on.after_pick(Some(&Effort::Auto), Some(Speed::Normal), Some(&both));
        assert_eq!(
            (normal.fast, normal.ultrafast, normal.effort),
            (false, false, Effort::Auto)
        );
    }

    #[test]
    fn fast_and_ultra_are_exclusive_and_offered_by_the_row() {
        let mut row = entry(&[]);
        let off = TurnSettings::default();
        assert_eq!(off.after_fast(Some(&row)), None, "no fast mode");
        assert_eq!(off.after_ultrafast(true, Some(&row)), None, "no ultra mode");
        assert_eq!(off.after_ultrafast(true, None), None);
        row.facts.fast = true;
        row.facts.ultrafast = true;
        let ultra = off.after_ultrafast(true, Some(&row)).expect("offered");
        assert!(ultra.ultrafast && !ultra.fast);
        let fast = ultra.after_fast(Some(&row)).expect("offered");
        assert!(fast.fast && !fast.ultrafast, "Fast turns Ultra off");
        let toggled = fast.after_fast(None).expect("off needs no row");
        assert!(!toggled.fast);
        let ultra = fast.after_ultrafast(true, Some(&row)).expect("offered");
        assert!(ultra.ultrafast && !ultra.fast, "Ultra turns Fast off");
        // Off needs no row, and leaves Fast as it was.
        assert_eq!(fast.after_ultrafast(false, None), Some(fast.clone()));
    }

    #[test]
    fn the_speed_column_offers_normal_and_what_the_row_offers_in_that_order() {
        let mut row = entry(&[]);
        assert_eq!(Speed::options(&row), [Speed::Normal]);
        assert!(!Speed::chosen_for(&row));
        row.facts.ultrafast = true;
        assert_eq!(Speed::options(&row), [Speed::Normal, Speed::Ultrafast]);
        assert!(Speed::chosen_for(&row));
        row.facts.fast = true;
        assert_eq!(
            Speed::options(&row),
            [Speed::Normal, Speed::Fast, Speed::Ultrafast]
        );
        assert_eq!(Speed::parse("ULTRAFAST"), Some(Speed::Ultrafast));
        assert_eq!(Speed::parse("slow"), None);
        // Ultra outranks Fast: the two are never on together.
        assert_eq!(Speed::of(true, true), Speed::Ultrafast);
        assert_eq!(Speed::of(true, false), Speed::Fast);
        assert!(intrinsic_fast("zai/glm-5-fast") && !intrinsic_fast("fast/model"));
    }

    #[test]
    fn the_three_default_spellings_are_the_default_and_a_name_is_kept_as_written() {
        for default in ["auto", "Adaptive", "DEFAULT"] {
            assert_eq!(Effort::parse(default), Some(Effort::Auto), "{default}");
        }
        assert_eq!(
            Effort::parse("High"),
            Some(Effort::Named("High".to_string()))
        );
        assert_eq!(
            Effort::parse("x-high_2.0"),
            Some(Effort::Named("x-high_2.0".to_string()))
        );
        // A word upstream would not parse is not an effort, so a two-word
        // `/model` with it is not the explicit form at all.
        for unusable in ["", "hi gh", "hi/gh", "é", &"a".repeat(65)] {
            assert_eq!(Effort::parse(unusable), None, "{unusable:?}");
        }
        assert!(Effort::parse(&"a".repeat(64)).is_some());
    }

    #[test]
    fn the_default_is_auto_in_a_file_and_default_on_a_screen() {
        assert_eq!(Effort::Auto.label(), "auto");
        assert_eq!(Effort::Auto.display(), "default");
        let low = Effort::Named("low".to_string());
        assert_eq!((low.label(), low.display()), ("low", "low"));
    }

    #[test]
    fn a_model_takes_the_default_always_and_a_name_only_when_it_lists_it() {
        let astra = entry(&["low", "medium", "high", "xhigh"]);
        assert!(Effort::Auto.supported_by(&astra));
        assert!(Effort::Named("xhigh".to_string()).supported_by(&astra));
        // Byte for byte, as upstream compares.
        assert!(!Effort::Named("High".to_string()).supported_by(&astra));
        assert!(!Effort::Named("max".to_string()).supported_by(&astra));
        assert!(Effort::Auto.supported_by(&entry(&[])));
        assert!(!Effort::Named("low".to_string()).supported_by(&entry(&[])));
    }

    #[test]
    fn the_picker_offers_the_default_first_and_nothing_for_a_model_without_levels() {
        let options = Effort::options(&entry(&["low", "high"]));
        assert_eq!(
            options.iter().map(Effort::display).collect::<Vec<_>>(),
            ["default", "low", "high"]
        );
        assert!(Effort::options(&entry(&[])).is_empty());
    }

    #[test]
    fn a_request_carries_a_level_only_to_a_model_that_lists_it() {
        let low = Effort::Named("low".to_string());
        let astra = entry(&["low", "high"]);
        assert_eq!(low.for_request(Some(&astra)), Some("low".to_string()));
        assert_eq!(Effort::Auto.for_request(Some(&astra)), None);
        assert_eq!(low.for_request(Some(&entry(&[]))), None);
        // A model the catalog does not describe takes none.
        assert_eq!(low.for_request(None), None);
    }
}
