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

use serde::{Deserialize, Serialize};

/// Which wire *and which authority* produced a piece of replayable state.
///
/// The authority is part of the name because the credential's issuer is part of
/// the replay contract and not a detail: Codex and Grok share a serialization
/// but a Codex `encrypted_content` item is opaque state sealed by OpenAI, and
/// handing it to xAI is handing one provider's sealed blob to another. Folding
/// them into one `openai_responses` value would leave that as a future decision;
/// naming them apart makes it a non-question.
///
/// The two Responses values are unreachable in this build -- Codex and Grok are
/// deferred rows -- and they are here anyway because this type is a **reader**
/// as much as a writer: a record produced by a later binary must be understood
/// well enough to be refused, and refusing it correctly means knowing what it
/// says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Wire {
    /// Anthropic Messages, as llmux speaks it.
    AnthropicMessages,
    /// The Vercel AI Gateway's own wire, which has no replay contract.
    VercelGateway,
    /// OpenAI Responses, authorized by a ChatGPT subscription.
    CodexResponses,
    /// OpenAI Responses, authorized by an xAI subscription.
    GrokResponses,
    /// A wire a later binary recorded and this one does not know.
    ///
    /// Kept verbatim rather than collapsed: it never equals an active wire, so
    /// it always drops with a notice, and it is written back unchanged so a
    /// binary that *does* know it can still replay the state after this one has
    /// read the record.
    Unrecognized(String),
}

impl Wire {
    pub fn label(&self) -> &str {
        match self {
            Self::AnthropicMessages => "anthropic_messages",
            Self::VercelGateway => "vercel_gateway",
            Self::CodexResponses => "codex_responses",
            Self::GrokResponses => "grok_responses",
            Self::Unrecognized(raw) => raw,
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "anthropic_messages" => Self::AnthropicMessages,
            "vercel_gateway" => Self::VercelGateway,
            "codex_responses" => Self::CodexResponses,
            "grok_responses" => Self::GrokResponses,
            other => Self::Unrecognized(other.to_string()),
        }
    }
}

impl From<String> for Wire {
    fn from(raw: String) -> Self {
        Self::parse(&raw)
    }
}

impl From<Wire> for String {
    fn from(wire: Wire) -> Self {
        wire.label().to_string()
    }
}

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

    /// The wire this provider speaks, which is also the authority its replayable
    /// state is sealed by.
    pub fn wire(self) -> Wire {
        match self {
            Self::Gateway => Wire::VercelGateway,
            Self::Llmux => Wire::AnthropicMessages,
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

/// A credential resolved for one provider.
///
/// `fx` threads a bearer string into every request and into catalog access;
/// llmux is keyless, so this is a sum type rather than an `Option<String>`.
#[derive(Clone)]
pub enum ProviderCredential {
    /// A bearer credential from the environment.
    Bearer(crate::config::Credential),
    /// A loopback endpoint that answers without one.
    KeylessLoopback,
}

impl ProviderCredential {
    pub fn source(&self) -> crate::config::CredentialSource {
        match self {
            Self::Bearer(credential) => credential.source(),
            Self::KeylessLoopback => crate::config::CredentialSource::LlmuxLoopback,
        }
    }
}

/// Resolves the credential for one provider, doing no I/O at all.
///
/// **Provider-scoped resolution bypasses precedence entirely**
/// (`vercel-labs/fx@ef1d0d0 src/core/auth/credentials.zig:271-303`): each
/// provider is asked only for its own credential, and there is no fallback from
/// one down to another's. Switching providers is the only path.
///
/// No I/O, because `status` and `doctor` call this and must stay commands that
/// are always safe to run. A daemon that is down still has a *present*
/// credential and fails at the ping, the catalog load, or the turn, each of
/// which reports in its own vocabulary.
pub fn resolve_credential_for(
    provider: ProviderId,
    config: &crate::config::RuntimeConfig,
) -> Option<ProviderCredential> {
    match provider {
        ProviderId::Gateway => config.credential.clone().map(ProviderCredential::Bearer),
        // Present when a loopback `llmux_url` is configured and passed the
        // endpoint rule on the way in -- which is a fact about the file, not
        // about the daemon.
        ProviderId::Llmux => config
            .llmux_url
            .is_some()
            .then_some(ProviderCredential::KeylessLoopback),
    }
}

/// Whether `provider` will accept a credential from `source`.
///
/// The one rule that joins the two axes
/// (`vercel-labs/fx@ef1d0d0 src/core/config/model_provider.zig:22-29`). It is
/// trivially satisfiable with two non-subscription providers and it is written
/// as a rule anyway, because it is the thing a subscription provider makes
/// load-bearing and the thing a *missing* guard makes silently wrong.
pub fn authorizes(provider: ProviderId, source: crate::config::CredentialSource) -> bool {
    match provider {
        ProviderId::Gateway => !matches!(source, crate::config::CredentialSource::LlmuxLoopback),
        ProviderId::Llmux => matches!(source, crate::config::CredentialSource::LlmuxLoopback),
    }
}

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

    #[test]
    fn every_wire_round_trips_through_its_json_string() {
        for wire in [
            Wire::AnthropicMessages,
            Wire::VercelGateway,
            Wire::CodexResponses,
            Wire::GrokResponses,
        ] {
            let json = serde_json::to_string(&wire).expect("serialize");
            assert_eq!(json, format!("\"{}\"", wire.label()));
            assert_eq!(
                serde_json::from_str::<Wire>(&json).expect("deserialize"),
                wire
            );
        }
    }

    #[test]
    fn a_wire_this_version_does_not_know_is_kept_rather_than_refused_or_forgotten() {
        // A record written by a later binary must not refuse a whole session,
        // and it must not decay into `None` either: `None` means "legacy
        // Anthropic" in the replay table, so an unknown wire read as absent
        // would be replayed onto the Messages wire -- the exact mis-wiring the
        // provenance field exists to prevent.
        let parsed: Wire = serde_json::from_str("\"openai_responses\"").expect("deserialize");
        assert_eq!(parsed, Wire::Unrecognized("openai_responses".to_string()));
        assert_eq!(parsed.label(), "openai_responses");
        assert_eq!(
            serde_json::to_string(&parsed).expect("serialize"),
            "\"openai_responses\"",
            "an unknown wire is written back exactly as it was read"
        );
    }

    #[test]
    fn each_provider_names_the_wire_it_speaks() {
        assert_eq!(ProviderId::Gateway.wire(), Wire::VercelGateway);
        assert_eq!(ProviderId::Llmux.wire(), Wire::AnthropicMessages);
    }

    #[test]
    fn the_gateway_accepts_any_source_that_is_not_a_subscription() {
        use crate::config::CredentialSource;
        for source in [
            CredentialSource::VercelOidcToken,
            CredentialSource::AiGatewayApiKey,
        ] {
            assert!(authorizes(ProviderId::Gateway, source), "for {source:?}");
        }
        // A loopback arrangement is not a Gateway credential: it authenticates
        // nothing to a remote endpoint, and passing it as one is how an empty
        // secret becomes a confusing 401 later.
        assert!(!authorizes(
            ProviderId::Gateway,
            CredentialSource::LlmuxLoopback
        ));
    }

    #[test]
    fn llmux_accepts_only_its_own_loopback_arrangement() {
        use crate::config::CredentialSource;
        assert!(authorizes(
            ProviderId::Llmux,
            CredentialSource::LlmuxLoopback
        ));
        for source in [
            CredentialSource::VercelOidcToken,
            CredentialSource::AiGatewayApiKey,
        ] {
            assert!(!authorizes(ProviderId::Llmux, source), "for {source:?}");
        }
    }
}
