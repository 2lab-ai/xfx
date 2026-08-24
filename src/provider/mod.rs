//! Provider identity: which transport a turn talks to, and how that provider
//! presents itself.
//!
//! Upstream keeps two axes apart and this module copies that separation:
//! **which transport and route** (`ProviderId`, `vercel-labs/fx@580a0c5d
//! src/core/config/model_provider.zig:4`) and **which credential**
//! (`CredentialSource`, `src/core/shared/types.zig`). Collapsing them is how a
//! credential ends up being asked of a provider that cannot use it.
//!
//! xfx has two providers today. Codex and Grok are `deferred` rows in
//! `docs/parity.md` and are absent from this enum: a name that parses is a
//! provider a profile can select, and selecting one with no transport behind it
//! would be a promise the binary cannot keep.

/// What `status` calls the arrangement llmux answers a loopback request under.
///
/// It names an arrangement rather than a credential source, because on this
/// provider there is no source: llmux accepts a keyless loopback request and
/// xfx sends nothing. `missing` would be the wrong word -- nothing is absent.
pub const LLMUX_LOOPBACK_LABEL: &str = "llmux-keyless-loopback";

/// Which provider a turn talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderId {
    /// The Vercel AI Gateway over its own wire, authenticated by a bearer token.
    Gateway,
    /// A local llmux daemon over the Anthropic Messages wire, keyless.
    Llmux,
}

impl ProviderId {
    /// Every provider this build can actually reach, in presentation order.
    pub const ALL: &'static [ProviderId] = &[Self::Gateway, Self::Llmux];

    /// The stable tag every renderer, settings file and `models{}` key uses.
    pub fn label(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::Llmux => "llmux",
        }
    }

    /// The inverse of [`Self::label`], tolerating padding and any casing.
    ///
    /// Case-insensitive because the consequence of *not* recognizing a name was
    /// out of all proportion to the typo: `"Llmux"` used to resolve to nothing,
    /// fall back to the compiled default, and send the prompt and the Vercel
    /// credential to a remote paid endpoint. Recognizing the name the operator
    /// obviously meant is the cheap half of fixing that; refusing to run on a
    /// name nobody can mean is the other half.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|id| raw.eq_ignore_ascii_case(id.label()))
    }

    /// How this provider presents itself.
    pub fn entry(self) -> &'static ProviderEntry {
        PROVIDERS
            .iter()
            .find(|entry| entry.id == self)
            .expect("every id has exactly one entry, which a unit test pins")
    }

    /// The value of the legacy `backend` key an older binary would read for this
    /// provider, when there is one.
    ///
    /// `None` means *no older binary can reach this provider*. The writer's rule
    /// for that case is to leave `backend` at its previous value rather than
    /// invent one, so a v0.1.0 binary keeps talking to the backend it was last
    /// told about instead of to a provider it cannot authenticate. Both of
    /// today's providers are representable; the `Option` is what forces the
    /// decision to be made rather than defaulted when a third one is added.
    pub fn legacy_backend(self) -> Option<&'static str> {
        match self {
            Self::Gateway => Some("gateway"),
            Self::Llmux => Some("llmux"),
        }
    }
}

impl Default for ProviderId {
    /// The Gateway, which is what xfx has always talked to.
    fn default() -> Self {
        Self::Gateway
    }
}

/// How a provider names and describes itself.
///
/// Upstream's entry also carries aliases (`provider_catalog.zig:4-40`); neither
/// of xfx's two providers has one, and an always-empty field would be a slot
/// nobody can fill. It arrives with the first provider that has an alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderEntry {
    pub id: ProviderId,
    /// The tag in the profile and on the wire of every renderer. Equal to
    /// [`ProviderId::label`], which a unit test pins.
    pub slug: &'static str,
    /// What a person calls it.
    pub name: &'static str,
    /// One line, for `xfx setup` and `/model`.
    pub description: &'static str,
    /// Whether this provider is reached by a subscription credential. Both of
    /// today's are `false`; the field exists because the guard that joins the
    /// two axes reads it.
    pub subscription: bool,
}

/// The closed presentation set, assembled statically like upstream's
/// (`vercel-labs/fx@580a0c5d src/builtins/providers.zig:11`).
pub const PROVIDERS: &[ProviderEntry] = &[
    ProviderEntry {
        id: ProviderId::Gateway,
        slug: "gateway",
        name: "Vercel AI Gateway",
        description: "remote gateway, bearer credential from the environment",
        subscription: false,
    },
    ProviderEntry {
        id: ProviderId::Llmux,
        slug: "llmux",
        name: "llmux",
        description: "local keyless daemon",
        subscription: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_id_round_trips_through_its_label() {
        for id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.label()), Some(*id));
        }
    }

    #[test]
    fn a_provider_name_is_read_without_regard_to_case_or_padding() {
        // The consequence of not recognizing a name is out of all proportion to
        // the typo: an unread name refuses every turn on the machine.
        assert_eq!(ProviderId::parse("  LLMUX \n"), Some(ProviderId::Llmux));
        assert_eq!(ProviderId::parse("Gateway"), Some(ProviderId::Gateway));
    }

    #[test]
    fn a_provider_this_build_cannot_reach_does_not_parse() {
        // Codex and Grok are deferred rows. Parsing their names would let a
        // profile select a provider the binary has no transport for.
        for absent in ["codex", "grok", "chatgpt", "openai-codex", ""] {
            assert_eq!(ProviderId::parse(absent), None, "for {absent:?}");
        }
    }

    #[test]
    fn the_presentation_set_covers_every_id_exactly_once() {
        assert_eq!(PROVIDERS.len(), ProviderId::ALL.len());
        for id in ProviderId::ALL {
            let entries: Vec<&ProviderEntry> =
                PROVIDERS.iter().filter(|entry| entry.id == *id).collect();
            assert_eq!(entries.len(), 1, "for {}", id.label());
            assert_eq!(entries[0].slug, id.label());
            assert_eq!(id.entry().slug, id.label());
        }
    }

    #[test]
    fn no_provider_in_this_build_is_a_subscription() {
        // Subscriptions arrive with OAuth, which this epic does not ship. A
        // `true` here with no login flow behind it would advertise a sign-in
        // xfx cannot perform.
        assert!(PROVIDERS.iter().all(|entry| !entry.subscription));
    }

    #[test]
    fn both_providers_have_a_legacy_backend_value_an_older_binary_can_reach() {
        assert_eq!(ProviderId::Gateway.legacy_backend(), Some("gateway"));
        assert_eq!(ProviderId::Llmux.legacy_backend(), Some("llmux"));
    }
}
