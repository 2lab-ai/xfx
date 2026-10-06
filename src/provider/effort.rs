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

use super::model::CatalogEntry;

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
