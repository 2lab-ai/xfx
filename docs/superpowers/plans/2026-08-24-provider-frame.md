# Provider Frame Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give xfx a provider *identity* — a `ProviderId` with a bundle per provider, Gateway and llmux as the first two, per-provider model catalogs, a `provider` + `models{}` profile with a read-repair migration off `backend`, and a wire-provenance session record — so that switching providers is a first-class, recorded act instead of an untyped `backend` string.

**Architecture:** A new `src/provider/` module owns the two axes upstream keeps separate: **which transport** (`ProviderId`) and **which credential** (`CredentialSource`), joined by one `authorizes` rule. `gateway::Provider` stays the transport seam and becomes the `stream` field of a `Bundle` selected once per invocation. The profile grows `provider` and `models{}` alongside the shipped `backend`/`model`, read by a read-repair that never rewrites on read and written by one staged-`0600`-plus-rename merge that keeps the legacy pair in sync. The session log gains provenance — `wire` and `responses_state` — so a replayed assistant turn is keyed by the **authority** that produced it rather than by the shape of its blocks.

**Tech Stack:** Rust 2021, `rust-version = 1.96`. `serde`/`serde_json`, `clap`, `reqwest` (rustls only), `tokio` (current-thread), `async-trait`. No new dependency is introduced by this plan.

**Spec:** [`.prd/04-providers.md`](../../../.prd/04-providers.md) — authoritative, five-round reviewed. Companions: [`.prd/02-architecture.md`](../../../.prd/02-architecture.md), [`.prd/05-upstream-delta.md`](../../../.prd/05-upstream-delta.md) §B2/§I2, [`.prd/research/auth-providers.md`](../../../.prd/research/auth-providers.md), [`docs/parity.md`](../../parity.md), [`CONTRIBUTING.md`](../../../CONTRIBUTING.md).

## Scope

This plan is **MVS steps 1–2 only** (`04-providers.md` §"MVS order"): the provider frame with Gateway and llmux, `/setup`-based provider switching, per-provider catalogs. **No OAuth**: policy option D is in force — *"D is the default unless the decision below says otherwise"* — so Codex and Grok stay `deferred` rows, absent from the binary, and no sign-in entry is added to any surface. The `Wire` enum and the session-log replay rules for `codex_responses`/`grok_responses` **do** land now, because they are what a *reader* must already know: a Phase-1 binary can meet a record written by a later binary, and it must degrade rather than mis-wire.

A **parallel plan owns the TUI**. This plan ships engine APIs and the two line surfaces that already exist (`xfx setup`, the shell's `/model`). It creates no picker, no menu, no alternate screen, and touches `src/interactive.rs` only where Task 4 and Task 9 say. Task 9 names the produces-interface the TUI plan consumes.

## Global Constraints

Every task's requirements implicitly include this section.

- Upstream evidence is pinned to `580a0c5da9386317251968c09c1cee69e763487a`; cite it as ``vercel-labs/fx@580a0c5d path/to/file.zig:LINE``. `04-providers.md`'s own `file:line` citations are read against upstream HEAD `ef1d0d0` and must be quoted as such when copied.
- **Advertisement is a promise.** A name in `xfx --help`, in `/help`, or in a tool schema has a handler, an acceptance test, and an `implemented` row in `docs/parity.md` **in the same change**.
- **No stubs.** No `todo!`, `unimplemented!`, placeholder success, or canned output in production code. **Deferred means absent** from the binary.
- **Never claim parity.** No "full parity", "feature complete", or a version number implying either.
- **xfx's own Gateway credential never enters output** — no snapshot, session event, or log line.
- **Profile-only key rule (verbatim, `04-providers.md`):** "Layer order is untouched: project → profile → exact-workspace → environment, and `provider`, `models`, `backend`, `llmux_url`, `model` are **all profile-only**, so a cloned repository still cannot choose the endpoint."
- **Loopback-service policy for `llmux_url` (verbatim, `docs/parity.md`):** "a loopback **address literal** (`127.0.0.1` or `[::1]` -- the name `localhost` is refused, because it resolves wherever the resolver says) with an explicit port and no path, under either scheme, and never userinfo. Neither llmux client honours a proxy or follows a redirect". `04-providers.md` adds: "`llmux_url` is unchanged and stays profile-only. It is a **provider parameter**, not a credential, and it keeps its own loopback-service policy."
- **No-network `status`/`doctor` (verbatim, `04-providers.md`):** "**Credential *presence* is a configuration fact; *reachability* is a network fact, and they must not merge.** `status` performs no I/O today and must keep performing none"; `doctor` "does no network I/O ... and never probes the daemon". Reachability surfaces only where a network call already legitimately happens: `xfx setup`'s ping and catalog proof, and a `/model` catalog load.
- **Endpoint-selection safety property (verbatim, `04-providers.md`):** "*a prompt is never sent to an endpoint the operator did not choose, and an unreadable choice is never replaced by a default.*" It must hold across the migration: every new key is profile-only; an unreadable `provider` is refused exactly like an unreadable `backend` rather than defaulted; `llmux_url` keeps the loopback-service rule; the rollback path degrades to a **previously operator-chosen** value, never to a built-in one.
- **Migration is read-repair, never a rewrite on read.** "The file is not rewritten by a read; `status` and `doctor` must stay side-effect-free, and a diagnostic command that edits the profile it is describing is the opposite of the contract."
- **Parity-ledger discipline.** `docs/parity.md` row updates travel in the same task — and the same commit — as the code they describe. `scripts/check-no-stubs.sh` and `tests/parity.rs` reconcile both directions.
- **Comments explain why, and especially why not.** Do not narrate the next line. Errors say what happened and what to do; diagnostics on stderr, answers on stdout.
- **The gate** (from `CONTRIBUTING.md`), all of it, before any task is reported done:

  ```bash
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked --all-targets
  cargo build --locked --release
  ./scripts/check-no-stubs.sh
  ./scripts/check-no-secrets.sh
  ./scripts/check-xfx-identity.sh
  ./scripts/check-preview-contract.sh
  ./scripts/smoke.sh target/release/xfx
  ```

- **Known blind spot:** parts of `tests/cli.rs` are compiled out on macOS, so a CLI-surface change can be locally green and fail on `check (ubuntu-latest)`. A grammar change is not done until that job is read.
- Commit subjects are plain imperative sentences. Every commit ends with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

## Decisions this plan makes, and why

These are deviations from, or refinements of, `04-providers.md`. They are recorded here so a reviewer can reject them individually.

1. **`ProviderId` replaces `config::Backend` outright** rather than sitting beside it. With Codex and Grok deferred, the two enums would be *identical* types with two label tables that could drift. The settings **key** `backend` survives as a legacy key that is still read and still written (see 5).
2. **The reported axis is renamed `backend` → `provider`** in `status`/`doctor` (`provider`, `provider_url`, `provider_rejected`, and the `provider` doctor check). Two vocabularies for one fact is exactly what the ledger exists to prevent, and `doctor` now has to name **both keys** on a desync, which only reads correctly if the axis has one name.
3. **`Wire` carries an `Unrecognized(String)` variant.** `04` specifies a closed enum; a closed enum makes an unknown value a *deserialization error*, which would refuse a whole session recorded by a later binary. Mapping an unknown wire to `None` would be worse — `None` means "legacy Anthropic" in the replay table, so an unknown wire would be **mis-replayed onto the Messages wire**, the exact hazard §Provenance exists to prevent. `Unrecognized` never matches an active wire, so it always drops with notice.
4. **`responses_state` lands on the session record and on `TurnStep`, not on `Completion`.** `Completion` gains `wire` (the decoder is the authority on which wire produced it) but not a payload field nothing can populate until a Responses decoder exists. The forward-compatible thing is the **record**, and the load-bearing thing now is the **reader**.
5. **The writer keeps `backend` and `model` in sync** for as long as the provider is legacy-representable, which both of today's are. The non-representable branch is implemented as `ProviderId::legacy_backend() -> Option<&'static str>` and is exercised by a unit test that passes `None` to the pure merge helper, so the rollback rule is live code rather than a comment.
6. **The Gateway advertises no model catalog.** `04` names llmux's `GET /models` and Codex/Grok fetchers; no Vercel Gateway catalog endpoint is researched anywhere in `.prd/` or in `src/`, and inventing a URL would be a guess. `Bundle`'s catalog is therefore `Option`al at the type level: `None` means *this provider advertises no catalog*, which is a fact, not a stub. The `models` command row stays `deferred`.
7. **The catalog is not a field of `Bundle`.** `/model` must work in a shell opened on a machine with **no credential** — that is exactly the machine whose user needs it — and a bundle carries a built transport. Catalog access is `provider::model::catalog_for(config)`; the bundle carries identity and transport.
8. **`xfx setup gateway` records a selection; it does not onboard a credential.** No key is read from a prompt, none is written. A target with no resolvable credential is **recorded with a warning**, not refused: the profile is machine state and the environment is shell state, and refusing a durable write because of an ephemeral shell is the same defect `setup llmux` already fixed once for `XFX_MODEL`.
9. **`CatalogEntry` carries `name`, `efforts` and `max_context`** because llmux really publishes them — `{id, aliases, name, efforts, max_context, group}`, evidence `2lab-ai/llmux@79f66748656b src/catalog.rs:44-63` — which is what makes `05-upstream-delta.md` §I2 ("`/model` shows the catalog — provider-advertised models, context windows, effort levels") reachable without OAuth.

## File Structure

**Created**

- `src/provider/mod.rs` — `ProviderId`, `ProviderEntry`, `PROVIDERS`, `Wire`, `Bundle`, `ProviderCredential`, `authorizes`. The identity axis and the dispatch type. Re-exports the submodules.
- `src/provider/model.rs` — `CatalogEntry`, `ModelCatalog` trait, `LlmuxCatalog`, `catalog_for`, and the `/model` engine seam (`ModelSelector`, `ModelRequest`, `ModelOutcome`, `CatalogState`, `CatalogError`).
- `src/provider/profile.rs` — the one profile writer: staged `0600` file + rename + `fsync`, merging `provider`, `models{}` and the legacy pair while preserving every unrelated key.
- `src/provider/setup.rs` — `SetupReport`, `SetupError`, the `xfx setup <provider>` transaction, and the Gateway arm.

**Modified**

- `src/lib.rs` — declare `pub mod provider;` and its doc bullet.
- `src/config.rs` — delete `Backend`; `RuntimeConfig.provider`/`provider_rejected`/`models`; the `provider`+`models{}` read-repair; `CredentialSource::LlmuxLoopback`; `Diagnostic.note`; `SettingSource: Ord`.
- `src/output.rs` — the reported axis rename; `AuthSnapshot` over the provider-scoped resolver; `SetupSnapshot` gains `provider`, loses the mandatory `url`/`models`.
- `src/app.rs` — `Bundle::select` replaces `build_provider`; the `provider` doctor check; `setup` dispatch by target.
- `src/cli.rs` — `setup` takes a provider target.
- `src/gateway/protocol.rs`, `src/gateway/sse.rs`, `src/llmux/sse.rs` — `Completion.wire`.
- `src/agent/machine.rs` — record provenance with the assistant message.
- `src/session/event.rs`, `src/session/store.rs` — `responses_state` + `wire` on the record and on `TurnStep`; authority-keyed replay returning notices.
- `src/llmux/setup.rs` — catalog parse widened; writing delegated to `provider::profile`; report types moved to `provider::setup`.
- `src/interactive.rs` — **only**: the replay-notice line (Task 4) and `apply_model` delegating to `ModelSelector` (Task 9).
- `docs/parity.md`, `CHANGELOG.md`, `docs/architecture.md`.
- `tests/cli.rs`, `tests/llmux.rs`, `tests/gateway.rs`, `tests/sessions.rs`, `tests/tool_loop.rs`, `tests/permissions.rs`, `tests/parity.rs`, `tests/support/fake_llmux.rs`.

---

### Task 1: The provider identity axis

`ProviderId` replaces `config::Backend` crate-wide and the reported axis is renamed. Behavior is otherwise unchanged: same two providers, same labels, same refusals.

**Files:**
- Create: `src/provider/mod.rs`
- Modify: `src/lib.rs`, `src/config.rs`, `src/output.rs`, `src/app.rs`, `src/llmux/setup.rs`
- Modify: `docs/parity.md`, `CHANGELOG.md`
- Test: `src/provider/mod.rs` (unit), `tests/cli.rs`, `tests/llmux.rs`, `tests/gateway.rs`

**Interfaces:**
- Produces: `xfx::provider::ProviderId { Gateway, Llmux }` with `ALL: &[ProviderId]`, `label(self) -> &'static str`, `parse(&str) -> Option<Self>`, `entry(self) -> &'static ProviderEntry`, `legacy_backend(self) -> Option<&'static str>`, `Default = Gateway`.
- Produces: `xfx::provider::ProviderEntry { id: ProviderId, slug: &'static str, name: &'static str, description: &'static str, subscription: bool }` and `xfx::provider::PROVIDERS: &[ProviderEntry]`.
- Produces: `xfx::provider::LLMUX_LOOPBACK_LABEL: &str = "llmux-keyless-loopback"`.
- Produces: `config::RuntimeConfig { provider: ProviderId, provider_rejected: Option<ProviderRejection>, .. }`, `config::ProviderRejection { key: &'static str, value: String }`, `config::Sources.provider`.
- Produces: `output::REJECTED_PROVIDER_LABEL`, `output::rejected_provider_help(&ProviderRejection) -> String`.

- [ ] **Step 1: Write the failing unit tests for the identity axis**

Create `src/provider/mod.rs` containing only this test module plus the `use super::*;` it needs (the types come in Step 3):

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib provider::`
Expected: FAIL — `error[E0433]: failed to resolve: use of undeclared crate or module 'provider'` (the module is not declared in `src/lib.rs` yet), or once declared, `cannot find type 'ProviderId' in this scope`.

- [ ] **Step 3: Write the identity axis**

In `src/provider/mod.rs`, above the test module:

```rust
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
```

Declare the module in `src/lib.rs`: add `pub mod provider;` in alphabetical position (after `pub mod permission;`) and a doc bullet `//! - [`provider`]: provider identity, the selected bundle, and model catalogs` after the `permission` bullet.

- [ ] **Step 4: Run the unit tests to verify they pass**

Run: `cargo test --lib provider::`
Expected: PASS, 6 tests.

- [ ] **Step 5: Replace `config::Backend` with `ProviderId`**

In `src/config.rs`:

- Delete the `Backend` enum and its `impl`s entirely.
- Add `use crate::provider::ProviderId;`.
- Add the rejection type next to `Diagnostic`:

```rust
/// A provider selection xfx could not read, and which key wrote it.
///
/// Kept rather than discarded, because it must **poison the turn**. Falling back
/// to the compiled default would send the prompt and the Gateway credential to a
/// remote paid endpoint because a settings value was mistyped. The key travels
/// with the value because the refusal has to name the key the operator actually
/// wrote -- there are two that select a provider, and quoting the wrong one
/// sends them to edit a line that is fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRejection {
    pub key: &'static str,
    pub value: String,
}
```

- `Settings`: rename `backend: Option<Backend>` to `backend: Option<ProviderId>` and `backend_rejected: Option<String>` stays a `String` (the raw value); `merge` is unchanged apart from the type.
- `Sources`: rename the field `backend` to `provider`, keep the doc comment, keep `llmux_url` as its own field.
- `RuntimeConfig`: rename `backend: Backend` to `provider: ProviderId` and `backend_rejected: Option<String>` to `provider_rejected: Option<ProviderRejection>`. In `load_with`, build it as `settings.backend_rejected.map(|value| ProviderRejection { key: "backend", value })`.
- `parse_layer`'s `"backend"` arm is otherwise unchanged: it still reads the key `backend`, using `ProviderId::parse`.

In `src/output.rs`:

- `REJECTED_BACKEND_LABEL` → `REJECTED_PROVIDER_LABEL` (same value `"rejected"`, same doc).
- `LLMUX_AUTH_LABEL` becomes `pub const LLMUX_AUTH_LABEL: &str = crate::provider::LLMUX_LOOPBACK_LABEL;`, keeping its doc comment. One value, one definition, and `provider` does not depend on the renderer.
- `rejected_backend_help(rejected: &str)` → `rejected_provider_help(rejected: &ProviderRejection)`, whose message names the key: ``the `{key}` setting is `{value}`, which xfx cannot read; set it to `gateway` or `llmux`.`` — keep the rest of the sentence as it is.
- `StatusSnapshot`/`DoctorSnapshot` fields `backend`/`backend_url`/`backend_rejected` → `provider`/`provider_url`/`provider_rejected`. The text renderer emits exactly three renamed keys, in the same positions and the same order they hold today: `line("provider", &self.provider)`, `line("provider_url", url)`, `line("provider_rejected", rejected)`. The helpers `backend_label`/`backend_url` become `provider_label`/`provider_url`.
- `AuthSnapshot::for_config` switches on `config.provider` and `config.provider_rejected`.
- `SetupSnapshot::new` uses `crate::provider::ProviderId::Llmux.label()`; leave its field named `backend` for now — Task 8 renames it with the rest of the setup surface.

In `src/app.rs`: `backend_check` → `provider_check`, its `DoctorCheck::new("backend", ..)` → `("provider", ..)`, `unreadable_backend_message` → `unreadable_provider_message`, and `build_provider` matches on `config.provider`. Its doc comments keep their arguments; replace the word "backend" with "provider" where it names the axis, not where it names the legacy key.

In `src/llmux/setup.rs`: `overriding_layers` reads `config.sources.provider`.

- [ ] **Step 6: Update the tests that name the axis**

`tests/cli.rs`: change the import to `use xfx::provider::ProviderId;` (drop `Backend` from the `xfx::config` import list), and rename in place:

- `the_backend_defaults_to_the_gateway_and_the_profile_may_select_llmux` → `the_provider_defaults_to_the_gateway_and_the_profile_may_select_llmux`; `config.backend` → `config.provider`, `Backend::Gateway` → `ProviderId::Gateway`, `config.sources.backend` → `config.sources.provider`.
- `an_exact_workspace_entry_may_choose_a_different_backend` → `..._provider`, same substitutions.
- `project_settings_cannot_choose_the_backend_or_point_at_an_endpoint` → `..._the_provider_...`; the ignored-key assertion still asserts the **key** `"backend"`, because that is the key a project file wrote.
- `a_backend_name_is_read_without_regard_to_case_or_padding` → `a_provider_name_...`.
- `an_unreadable_backend_poisons_turns_rather_than_falling_back_to_the_gateway` → `an_unreadable_provider_selection_poisons_turns_rather_than_falling_back_to_the_gateway`; the assertion becomes:

```rust
    let rejection = config
        .provider_rejected
        .as_ref()
        .expect("an unreadable selection is kept, never defaulted");
    assert_eq!(rejection.key, "backend");
    assert_eq!(rejection.value, "definitely-not-a-backend");
```

  and the binary-level assertion `run.stdout.contains("backend")` becomes:

```rust
    // The refusal names the key the operator wrote and quotes what they wrote.
    assert!(run.stdout.contains("provider=rejected"), "{}", run.stdout);
    assert!(run.stdout.contains("`backend`"), "{}", run.stdout);
    assert!(run.stdout.contains("definitely-not-a-backend"), "{}", run.stdout);
```

`tests/llmux.rs`: change `use xfx::config::{Backend, Environment, RuntimeConfig};` to `use xfx::config::{Environment, RuntimeConfig};` plus `use xfx::provider::ProviderId;`, and substitute `Backend::` → `ProviderId::`, `config.backend` → `config.provider`. Every JSON assertion on `document["backend"]`/`["backend_url"]`/`["backend_rejected"]` becomes `["provider"]`/`["provider_url"]`/`["provider_rejected"]`, and every `[status] backend…` text-line assertion becomes `[status] provider…`. The tests holding these are `status_reports_a_llmux_backend_as_keyless_rather_than_unauthenticated`, `doctor_reports_the_backend_and_adds_no_network_call`, `status_carries_the_refusal_when_llmux_has_no_endpoint`, and the rejected-value test above them (which also asserts the help names the setting — it still does, and the setting it names is still `backend`, because that is the key the fixture wrote). Rename those three test functions to say `provider` where they say `backend` about the axis. The `[setup]` assertions are untouched in this task.

`tests/gateway.rs`: the single `backend` occurrence is a `status` field assertion; rename it to `provider`.

- [ ] **Step 7: Update the ledger and the changelog**

`docs/parity.md`:

- In the `status` row, replace the sentence listing fields so it reads "Model, provider, credential source, permission mode, sandbox, workspace, history turns, step limit."
- In the `doctor` row, replace every occurrence of the check name `backend` with `provider` and keep the rest of the sentence intact — including "The `provider` check appears only when the configured provider cannot run".
- In the `backend selection` persistence row, change the surface cell to ``provider selection `backend` + `llmux_url` `` and add, at the end of the notes: "The runtime axis and every rendered field are called `provider`; `backend` is the settings key that selects it."
- In both `status/doctor` UI rows, rename `backend`/`backend_url`/`backend_rejected` to `provider`/`provider_url`/`provider_rejected` and `backend=rejected` to `provider=rejected`.

`CHANGELOG.md`, under `## [Unreleased]`, add a `### Changed` section **above** the existing `### Fixed`:

```markdown
### Changed

- **`status` and `doctor` report `provider` where they reported `backend`.**
  The fields are `provider`, `provider_url` and `provider_rejected`, and the
  `doctor` check is named `provider`. The settings key that selects it is still
  `backend` and is read exactly as before; what changed is that the product now
  has one word for the axis instead of two, which is what lets a later release
  report a disagreement between two keys without inventing a third name for the
  thing they disagree about.
```

- [ ] **Step 8: Run the full gate**

Run:
```bash
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh && ./scripts/check-no-secrets.sh && ./scripts/check-xfx-identity.sh
```
Expected: all exit 0. `cargo test` reports the same test count as before plus the 6 new `provider::` unit tests.

- [ ] **Step 9: Commit**

```bash
git add src/provider/mod.rs src/lib.rs src/config.rs src/output.rs src/app.rs src/llmux/setup.rs docs/parity.md CHANGELOG.md tests/cli.rs tests/llmux.rs tests/gateway.rs
git commit -m $'give the transport axis one name and one type\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 2: The `provider` and `models{}` profile keys

The read side of the migration: four rules, coexistence precedence, and a reported disagreement. Nothing writes the new keys yet.

**Files:**
- Modify: `src/config.rs`, `src/output.rs`
- Modify: `docs/parity.md`
- Test: `src/config.rs` (unit), `tests/cli.rs`

**Interfaces:**
- Consumes: `provider::ProviderId`, `config::ProviderRejection` (Task 1).
- Produces: `config::RuntimeConfig.models: BTreeMap<String, String>` — the per-provider model preferences as merged, keyed by `ProviderId::label`.
- Produces: `config::DiagnosticCause::ConflictingProviderSelection`, `config::Diagnostic.note: Option<String>`.
- Produces: `SettingSource: PartialOrd + Ord`, ranked in precedence order.
- Produces: `config::PROVIDER_KEY: &str = "provider"`, `config::MODELS_KEY: &str = "models"`, `config::BACKEND_KEY: &str = "backend"`.

- [ ] **Step 1: Write the failing tests for the read-repair**

In `tests/cli.rs`, after the existing provider precedence tests:

```rust
#[test]
fn a_provider_key_is_read_and_a_flat_model_seeds_that_providers_entry() {
    // Rule 1 and rule 2: the derived provider is the one named, and a flat
    // `model` seeds `models[<provider>]` in memory only.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"model\":\"fable\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);
    assert_eq!(config.model, "fable");
    assert_eq!(config.models.get("llmux").map(String::as_str), Some("fable"));
}

#[test]
fn a_models_entry_outranks_a_flat_model_inside_one_layer() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\
          \"model\":\"flat\",\"models\":{\"llmux\":\"chosen\",\"gateway\":\"other\"}}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, "chosen", "the newer key is the deliberate one");
    // The other provider's preference is kept, not discarded: switching back
    // must not lose what was chosen for it.
    assert_eq!(config.models.get("gateway").map(String::as_str), Some("other"));
}

#[test]
fn a_higher_layer_flat_model_still_outranks_a_lower_layer_models_entry() {
    // Layer order is untouched by the new key. A workspace entry that pins this
    // directory's model must not stop working because the global file grew a
    // `models` object.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(&format!(
        "{{\"provider\":\"gateway\",\"models\":{{\"gateway\":\"global\"}},\
          \"workspaces\":{{{}:{{\"model\":\"pinned\"}}}}}}",
        serde_json::to_string(&sandbox.workspace.display().to_string()).unwrap()
    ));
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, "pinned");
    assert_eq!(config.sources.model, SettingSource::UserWorkspace);
}

#[test]
fn an_environment_model_still_outranks_every_layer_including_models() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"provider\":\"gateway\",\"models\":{\"gateway\":\"file\"}}");
    let config = RuntimeConfig::load_with(
        &Environment::new(
            Some(sandbox.home.clone()),
            [("XFX_MODEL".to_string(), "shell".to_string())]
                .into_iter()
                .collect(),
        ),
        &sandbox.workspace,
    )
    .expect("load config");
    assert_eq!(config.model, "shell");
    assert_eq!(config.sources.model, SettingSource::ProcessOverride);
}

#[test]
fn a_provider_key_outranks_a_legacy_backend_key_and_the_disagreement_is_reported() {
    // Coexistence: the newer key is the one a newer binary wrote deliberately.
    // A machine whose profile says two different things about where prompts go
    // is exactly the machine an operator should be told about.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"backend\":\"gateway\",\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Llmux);

    let details: Vec<String> = config.diagnostics.iter().map(|d| d.detail()).collect();
    let conflict = details
        .iter()
        .find(|detail| detail.contains("conflicting_provider_selection"))
        .unwrap_or_else(|| panic!("no conflict reported: {details:?}"));
    assert!(conflict.contains("provider=llmux"), "{conflict}");
    assert!(conflict.contains("backend=gateway"), "{conflict}");
}

#[test]
fn two_keys_that_agree_are_not_reported_as_a_disagreement() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"backend\":\"llmux\",\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert!(
        !config
            .diagnostics
            .iter()
            .any(|d| d.cause == DiagnosticCause::ConflictingProviderSelection),
        "agreement is not a problem to report"
    );
}

#[test]
fn an_unreadable_provider_poisons_the_turn_even_when_backend_is_readable() {
    // The operator's most recent word is what they meant. Quietly using the
    // older key would be exactly the fallback this mechanism exists to refuse.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"gateway\",\"provider\":\"not-a-provider\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config.provider_rejected.expect("kept, never defaulted");
    assert_eq!(rejection.key, "provider");
    assert_eq!(rejection.value, "not-a-provider");

    let run = sandbox.run(&["ask", "--no-save", "hello"]);
    assert_eq!(run.code, Some(1));
    assert!(run.stderr.contains("`provider`"), "{}", run.stderr);
    assert!(run.stderr.contains("not-a-provider"), "{}", run.stderr);
}

#[test]
fn an_unreadable_backend_still_poisons_the_turn_when_no_provider_key_exists() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"definitely-not-a-backend\"}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    let rejection = config.provider_rejected.expect("kept, never defaulted");
    assert_eq!(rejection.key, "backend");
}

#[test]
fn a_project_file_cannot_choose_the_provider_or_a_models_entry() {
    let sandbox = Sandbox::new();
    sandbox.write_project_settings(
        "{\"provider\":\"llmux\",\"models\":{\"llmux\":\"whatever\"}}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.provider, ProviderId::Gateway);
    assert!(config.models.is_empty());

    let ignored: Vec<&str> = config
        .diagnostics
        .iter()
        .filter_map(|d| d.ignored_setting_key())
        .collect();
    assert!(ignored.contains(&"provider"), "got {ignored:?}");
    assert!(ignored.contains(&"models"), "got {ignored:?}");
}

#[test]
fn reading_a_profile_never_rewrites_it() {
    // A diagnostic command that edits the profile it is describing is the
    // opposite of the contract.
    let sandbox = Sandbox::new();
    let body = "{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"model\":\"fable\"}";
    sandbox.write_user_settings(body);
    let path = sandbox.profile_dir().join("settings.json");
    let before = fs::metadata(&path).unwrap().modified().unwrap();

    assert_eq!(sandbox.run(&["status", "--json"]).code, Some(0));
    assert_eq!(sandbox.run(&["doctor", "--json"]).code, Some(0));

    assert_eq!(fs::read_to_string(&path).unwrap(), body);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
}

#[test]
fn a_models_value_that_is_not_a_string_is_a_diagnostic_and_not_a_model() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"provider\":\"gateway\",\"models\":{\"gateway\":7}}");
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert_eq!(config.model, xfx::config::DEFAULT_MODEL);
    assert!(config
        .diagnostics
        .iter()
        .any(|d| d.setting_key.as_deref() == Some("models")));
}
```

Add `use xfx::config::DiagnosticCause;` to the imports. Every one of these builds its config through the free `environment(&sandbox.home, &[..])` helper the file already has (`tests/cli.rs:910`) and `RuntimeConfig::load_with`, which is how every other configuration test in this file already does it — do not add a `Sandbox::config` method: `tests/llmux.rs` has one of those, and two spellings of the same thing is how they drift.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test cli provider -- --nocapture` then `cargo test --test cli models`
Expected: FAIL to compile — `no field 'models' on type 'RuntimeConfig'`, `no variant 'ConflictingProviderSelection'`.

- [ ] **Step 3: Implement the read-repair**

In `src/config.rs`:

```rust
/// The settings key that names the provider directly.
pub const PROVIDER_KEY: &str = "provider";
/// The settings key holding one model preference per provider tag.
pub const MODELS_KEY: &str = "models";
/// The v0.1.0 key that selected a backend, still read and still written.
pub const BACKEND_KEY: &str = "backend";
```

Add both new keys to `PROFILE_ONLY_KEYS` (the list becomes `backend`, `credential_source`, `llmux_url`, `model`, `models`, `permission_mode`, `provider`, `workspaces`).

Give `SettingSource` an order — the variants are already declared in precedence order, so:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingSource { .. }
```

with a doc line: *"Ordered: a later variant outranks an earlier one. The order is the layer order, which is what lets two keys competing for one setting be compared by where each came from rather than by which was parsed last."*

Add the cause and the note:

```rust
pub enum DiagnosticCause {
    ..
    /// Two keys select a provider and they disagree.
    ConflictingProviderSelection,
}
```
with label `"conflicting_provider_selection"`, and on `Diagnostic`:

```rust
pub struct Diagnostic {
    pub layer: ConfigLayer,
    pub cause: DiagnosticCause,
    pub setting_key: Option<String>,
    /// The facts a reader needs that the cause alone does not carry -- today,
    /// the two values that disagree. Rendered into `detail`, so a `doctor`
    /// `config` check names them rather than telling an operator that something
    /// somewhere conflicts.
    pub note: Option<String>,
}
```

`Diagnostic::new`/`with_key` set `note: None`; add `fn with_note(layer, cause, key: &str, note: String)`. `detail()` appends `" "` and the note when there is one.

`Settings` gains:

```rust
    /// A layer's `provider` value, when it wrote a readable one.
    provider: Option<ProviderId>,
    /// The raw `provider` value a layer wrote that could not be read.
    provider_rejected: Option<String>,
    /// Model preferences by provider tag, merged per entry.
    models: BTreeMap<String, String>,
    /// Where each surviving `models` entry came from.
    models_sources: BTreeMap<String, SettingSource>,
```

`merge` handles `provider`/`provider_rejected` exactly as `backend`/`backend_rejected` already are (a later unreadable value replaces a readable one and clears it), tracking `sources.provider`, and merges `models` **per entry**:

```rust
        // Per entry rather than wholesale: a profile that names a model for one
        // provider and a workspace entry that names one for another are two
        // different settings, and replacing the map would silently drop the one
        // the operator is not currently using.
        for (tag, model) in incoming.models {
            self.models.insert(tag.clone(), model);
            self.models_sources.insert(tag, source);
        }
```

`parse_layer`, under `LayerKind::Profile`, gains:

```rust
        if let Some(value) = object.get(PROVIDER_KEY) {
            match value.as_str().and_then(ProviderId::parse) {
                Some(provider) => settings.provider = Some(provider),
                None => {
                    settings.provider_rejected =
                        Some(value.as_str().unwrap_or_default().trim().to_string());
                    diagnostics.push(Diagnostic::with_key(
                        layer,
                        DiagnosticCause::InvalidValue,
                        PROVIDER_KEY,
                    ));
                }
            }
        }
        if let Some(value) = object.get(MODELS_KEY) {
            match value.as_object() {
                Some(entries) => {
                    for (tag, model) in entries {
                        // An entry keyed by a provider this build cannot reach is
                        // kept, not dropped: it belongs to a provider a newer
                        // binary selects, and deleting other people's settings is
                        // not this reader's job. It is simply never selected.
                        match model.as_str().map(str::trim) {
                            Some(model) if !model.is_empty() => {
                                settings.models.insert(tag.clone(), model.to_string());
                            }
                            _ => diagnostics.push(Diagnostic::with_key(
                                layer,
                                DiagnosticCause::InvalidValue,
                                MODELS_KEY,
                            )),
                        }
                    }
                }
                None => diagnostics.push(Diagnostic::with_key(
                    layer,
                    DiagnosticCause::InvalidValue,
                    MODELS_KEY,
                )),
            }
        }
```

In `load_with`, after the merges, resolve the two facts:

```rust
        // Rule 1 and the coexistence rule, in one place: `provider` wins over
        // `backend`, an unreadable value of whichever key is newest poisons the
        // turn rather than being replaced by a default, and a disagreement is
        // reported rather than resolved silently.
        let (provider, provider_rejected) = match (
            settings.provider,
            settings.provider_rejected.as_ref(),
            settings.backend,
            settings.backend_rejected.as_ref(),
        ) {
            (_, Some(raw), _, _) => (
                ProviderId::default(),
                Some(ProviderRejection {
                    key: PROVIDER_KEY,
                    value: raw.clone(),
                }),
            ),
            (Some(provider), None, _, _) => (provider, None),
            (None, None, _, Some(raw)) => (
                ProviderId::default(),
                Some(ProviderRejection {
                    key: BACKEND_KEY,
                    value: raw.clone(),
                }),
            ),
            (None, None, Some(backend), None) => (backend, None),
            (None, None, None, None) => (ProviderId::default(), None),
        };
        if let (Some(chosen), Some(legacy)) = (settings.provider, settings.backend) {
            if chosen != legacy {
                diagnostics.push(Diagnostic::with_note(
                    ConfigLayer::User,
                    DiagnosticCause::ConflictingProviderSelection,
                    PROVIDER_KEY,
                    format!(
                        "provider={} backend={}; `provider` wins, and an older binary \
                         reading this profile would use `backend`",
                        chosen.label(),
                        legacy.label()
                    ),
                ));
            }
        }
```

Note on the poisoned case: `provider` is still set to the compiled default so every renderer has a value to print, and **`provider_rejected` being `Some` is what refuses the turn** — the shipped `unreadable_provider_message` path is unchanged, it just reads the new field. Both keys are profile-only, so the layer is always `ConfigLayer::User`; say so in a comment.

Then the model:

```rust
        // The per-provider preference wins inside one layer, because the newer
        // key is the one a newer binary wrote deliberately; across layers the
        // layer order still decides, because a workspace entry that pins this
        // directory's model must not stop working when the global file grows a
        // `models` object. Comparing the two sources is the whole rule.
        let tag = provider.label();
        let flat_source = sources.model;
        let entry = settings
            .models
            .get(tag)
            .zip(settings.models_sources.get(tag).copied())
            // `>=` and not `>`: inside one layer the two sources are equal and
            // the per-provider key is the deliberate one. When no layer wrote a
            // flat `model`, `flat_source` is `CompiledDefault`, which every real
            // layer outranks -- so this one comparison covers that case too and
            // there is no second condition to keep in step with it.
            .filter(|(_, entry_source)| *entry_source >= flat_source);
        let model = match entry {
            Some((model, entry_source)) => {
                sources.model = entry_source;
                model.clone()
            }
            None => settings
                .model
                .clone()
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        };
```

`RuntimeConfig` gains `pub models: BTreeMap<String, String>` (documented as *"Model preferences by provider tag, as merged. Exposed so a writer can preserve the entries for providers this invocation is not using."*), and its construction uses `provider`, `provider_rejected`, `model`, `models: settings.models`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test cli && cargo test --lib config::`
Expected: PASS, including the 11 new tests.

- [ ] **Step 5: Update the ledger**

`docs/parity.md`, the `provider selection` persistence row: change the surface cell to ``provider selection `provider` + `models` + `backend` + `llmux_url` `` and add these sentences to the notes:

> `provider` and `models` are read as of this release and are **profile-only** like the rest, so a cloned repository still cannot choose the endpoint. `provider` names a provider tag directly; `models` maps a provider tag to that provider's model, and `models[<active provider>]` outranks the flat `model` **inside one layer** while the layer order still decides between layers. When `provider` is absent it is derived from `backend`; when both are present `provider` wins and `doctor` reports the disagreement as a `config` check naming both values, because a machine whose profile says two different things about where prompts go is exactly the machine an operator should be told about. An unreadable value of **whichever key is newest** is kept and refuses every turn, quoting the key and the value; no default is substituted. Reading never rewrites: `status` and `doctor` leave the file byte-identical, and the new keys appear only when something that already writes the profile writes them.

- [ ] **Step 6: Run the full gate**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh`
Expected: all exit 0.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/output.rs docs/parity.md tests/cli.rs
git commit -m $'read a provider and per-provider models out of the profile\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 3: Wire provenance on a completion

`Wire` names the wire **and its authority** together. A decoder stamps it, because the decoder is the only thing that knows.

**Files:**
- Modify: `src/provider/mod.rs`, `src/gateway/protocol.rs`, `src/gateway/sse.rs`, `src/llmux/sse.rs`
- Test: `src/provider/mod.rs` (unit), `tests/llmux.rs`, `tests/gateway.rs`

**Interfaces:**
- Consumes: `provider::ProviderId` (Task 1).
- Produces: `provider::Wire { AnthropicMessages, VercelGateway, CodexResponses, GrokResponses, Unrecognized(String) }` with `label(&self) -> &str`, `parse(&str) -> Wire`, `Serialize`/`Deserialize` as a snake_case string, `PartialEq`, `Clone`.
- Produces: `ProviderId::wire(self) -> Wire`.
- Produces: `gateway::protocol::Completion.wire: Wire`.

- [ ] **Step 1: Write the failing unit tests**

In `src/provider/mod.rs`'s test module:

```rust
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
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib provider::`
Expected: FAIL — `cannot find type 'Wire' in this scope`.

- [ ] **Step 3: Implement `Wire`**

In `src/provider/mod.rs`:

```rust
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
```

and on `ProviderId`:

```rust
    /// The wire this provider speaks, which is also the authority its replayable
    /// state is sealed by.
    pub fn wire(self) -> Wire {
        match self {
            Self::Gateway => Wire::VercelGateway,
            Self::Llmux => Wire::AnthropicMessages,
        }
    }
```

- [ ] **Step 4: Stamp it on the completion**

`src/gateway/protocol.rs`, on `Completion`:

```rust
    /// Which wire produced this completion, and therefore which authority
    /// sealed anything replayable in it.
    ///
    /// Set by the decoder rather than by the caller: the caller knows which
    /// provider it *asked*, the decoder knows which wire actually answered, and
    /// the second is the one a replay contract is written against.
    pub wire: Wire,
```

with `use crate::provider::Wire;` at the top. Set it at the two construction sites: `src/gateway/sse.rs:252` gets `wire: Wire::VercelGateway`, `src/llmux/sse.rs:237` gets `wire: Wire::AnthropicMessages`.

Every other `Completion { .. }` literal is in tests. Fix them all — `tests/gateway.rs:987,1297,1320,1345` take `Wire::VercelGateway`; `tests/tool_loop.rs:1266,1280` and `tests/permissions.rs:2524,2535` are fake-provider completions and take `Wire::VercelGateway` (they drive the Gateway path). Add `use xfx::provider::Wire;` to each file.

Add one assertion per decoder, next to the existing decode tests:

```rust
// tests/llmux.rs, in `the_live_daemon_stream_decodes_into_one_completion`
assert_eq!(
    completion.wire,
    Wire::AnthropicMessages,
    "the decoder stamps the wire that answered"
);
```

```rust
// tests/gateway.rs, in the equivalent single-completion decode test
assert_eq!(completion.wire, Wire::VercelGateway);
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --lib provider:: && cargo test --test llmux && cargo test --test gateway && cargo test --test tool_loop && cargo test --test permissions`
Expected: PASS.

- [ ] **Step 6: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets`
Expected: exit 0.

```bash
git add src/provider/mod.rs src/gateway/protocol.rs src/gateway/sse.rs src/llmux/sse.rs tests/gateway.rs tests/llmux.rs tests/tool_loop.rs tests/permissions.rs
git commit -m $'let a completion say which wire produced it\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 4: Session provenance and authority-keyed replay

`raw_content` is redefined as Anthropic-Messages blocks only; `responses_state` and `wire` join it on the record; replay is decided by authority, and a drop is never silent.

**Files:**
- Modify: `src/session/event.rs`, `src/session/store.rs`, `src/agent/machine.rs`, `src/app.rs`, `src/interactive.rs`
- Modify: `docs/parity.md`, `docs/architecture.md`
- Test: `tests/sessions.rs`, `src/session/store.rs` (unit)

**Interfaces:**
- Consumes: `provider::Wire`, `ProviderId::wire()` (Task 3), `config::RuntimeConfig.provider` (Task 1).
- Produces: `SessionEvent::AssistantMessage { text, tool_calls, raw_content, responses_state, wire }` where the last two are `#[serde(default, skip_serializing_if = ..)]`.
- Produces: `session::store::TurnStep::Assistant { text, tool_calls, raw_content, responses_state, wire }`.
- Produces: `session::store::ReplayedHistory { messages: Vec<Message>, notices: Vec<String> }` and `DurableState::history_messages(&self, active: Wire) -> ReplayedHistory` (replaces the no-argument form).

- [ ] **Step 1: Write the failing replay tests**

In `tests/sessions.rs`:

```rust
/// One recorded turn whose assistant step carries the provenance a test names.
///
/// Hand-built rather than produced by a turn, because the cases that matter are
/// exactly the ones this binary cannot produce: a record from a version that
/// speaks a wire this one does not.
fn provenance_events(
    raw_content: Vec<Value>,
    responses_state: Vec<Value>,
    wire: Option<Wire>,
) -> Vec<SessionEvent> {
    vec![
        SessionEvent::UserMessage {
            text: "ask".to_string(),
        },
        SessionEvent::AssistantMessage {
            text: "answered".to_string(),
            tool_calls: Vec::new(),
            raw_content,
            responses_state,
            wire,
        },
    ]
}

/// Records `events` into a fresh session and hands back its replayable state.
fn recorded_state(profile: &Profile, workspace: &Workspace, events: Vec<SessionEvent>) -> DurableState {
    let store = profile.store();
    let mut session = store
        .create(id("provenance"), new_session(workspace.path()))
        .expect("create the session");
    for event in events {
        store.append(&mut session, event).expect("append");
        store.publish(&mut session).expect("publish");
    }
    session.state().clone()
}

fn thinking_block() -> Value {
    json!({"type": "thinking", "signature": "sig", "thinking": "…"})
}

fn reasoning_item() -> Value {
    json!({"type": "reasoning", "encrypted_content": "sealed"})
}

#[test]
fn a_legacy_record_with_blocks_and_no_wire_replays_on_the_messages_wire() {
    // The only wire that ever wrote `raw_content` is Anthropic Messages, so a
    // record that has it and names no wire can only have come from there.
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let state = recorded_state(
        &profile,
        &workspace,
        provenance_events(vec![thinking_block()], Vec::new(), None),
    );

    let replay = state.history_messages(Wire::AnthropicMessages);
    assert!(replay.notices.is_empty(), "{:?}", replay.notices);
    assert_eq!(
        replay.messages[1].raw_blocks().expect("blocks replayed")[0]["signature"],
        "sig"
    );
}

#[test]
fn a_legacy_record_is_dropped_with_a_notice_on_any_other_wire() {
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let state = recorded_state(
        &profile,
        &workspace,
        provenance_events(vec![thinking_block()], Vec::new(), None),
    );

    let replay = state.history_messages(Wire::VercelGateway);
    assert!(
        replay.messages[1].raw_blocks().is_none(),
        "a Gateway turn has no replay contract to satisfy"
    );
    // The turn still replays as text and tool calls: dropped state degrades a
    // request, it never deletes a turn.
    assert_eq!(replay.messages[1].text(), "answered");
    assert_eq!(replay.notices.len(), 1);
    let notice = &replay.notices[0];
    assert!(notice.contains("anthropic_messages"), "{notice}");
    assert!(notice.contains("vercel_gateway"), "{notice}");
}

#[test]
fn responses_state_recorded_under_one_authority_is_never_sent_to_another() {
    // Codex and Grok share a serialization and not its issuer. This binary can
    // reach neither, which is exactly why the rule is written now: the record it
    // is reading was written by a binary that could.
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let state = recorded_state(
        &profile,
        &workspace,
        provenance_events(Vec::new(), vec![reasoning_item()], Some(Wire::CodexResponses)),
    );

    for active in [
        Wire::GrokResponses,
        Wire::AnthropicMessages,
        Wire::VercelGateway,
    ] {
        let replay = state.history_messages(active.clone());
        assert!(replay.messages[1].raw_blocks().is_none(), "for {active:?}");
        assert_eq!(replay.notices.len(), 1, "for {active:?}");
        assert!(
            replay.notices[0].contains("codex_responses"),
            "for {active:?}: {:?}",
            replay.notices
        );
    }
}

#[test]
fn a_wire_this_version_does_not_know_drops_rather_than_guessing() {
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let state = recorded_state(
        &profile,
        &workspace,
        provenance_events(
            Vec::new(),
            vec![reasoning_item()],
            Some(Wire::Unrecognized("some_future_wire".to_string())),
        ),
    );

    let replay = state.history_messages(Wire::AnthropicMessages);
    assert!(replay.messages[1].raw_blocks().is_none());
    assert!(
        replay.notices[0].contains("some_future_wire"),
        "{:?}",
        replay.notices
    );
}

#[test]
fn a_dropped_state_is_not_deleted_from_the_log() {
    // Dropping shapes a request; it never mutates a record. A later resume back
    // onto the original authority replays it -- from a different store object,
    // as a new process would.
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let state = recorded_state(
        &profile,
        &workspace,
        provenance_events(vec![thinking_block()], Vec::new(), Some(Wire::AnthropicMessages)),
    );
    let _ = state.history_messages(Wire::VercelGateway);

    let reread = profile
        .read_only_store()
        .detail(&Selector::Id(id("provenance")), workspace.path())
        .expect("read the session back");
    let replay = reread.state.history_messages(Wire::AnthropicMessages);
    assert!(replay.notices.is_empty());
    assert_eq!(
        replay.messages[1].raw_blocks().expect("still on disk")[0]["signature"],
        "sig"
    );
}

#[test]
fn a_record_this_version_wrote_carries_no_new_field_when_there_is_nothing_to_replay() {
    // Byte-compatibility with an older reader is only free if the fields stay
    // absent when they are empty.
    let (profile, workspace) = (Profile::new(), Workspace::new());
    let _ = recorded_state(
        &profile,
        &workspace,
        provenance_events(Vec::new(), Vec::new(), None),
    );

    let log = fs::read_to_string(
        profile
            .sessions_dir()
            .join("provenance")
            .join(EVENTS_FILE),
    )
    .expect("read the log");
    assert!(log.contains("assistant_message"), "{log}");
    assert!(!log.contains("responses_state"), "{log}");
    assert!(!log.contains("\"wire\""), "{log}");
    assert!(!log.contains("raw_content"), "{log}");
}
```

Add `use xfx::provider::Wire;` and `use xfx::session::store::DurableState;` to the file's imports (`Profile`, `Workspace`, `new_session`, `id`, `Selector` and `EVENTS_FILE` are already there). `DurableState` must be `Clone` for `recorded_state` to hand it back — it derives `Clone` already; if it does not, take the state by reference from a `detail()` read instead of cloning rather than adding a derive for a test's convenience.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --test sessions`
Expected: FAIL to compile — `struct variant 'SessionEvent::AssistantMessage' has no field named 'responses_state'`.

- [ ] **Step 3: Extend the record**

`src/session/event.rs`, on `AssistantMessage`, replacing the current `raw_content` doc with the redefinition and adding the two fields:

```rust
    AssistantMessage {
        text: String,
        tool_calls: Vec<RecordedToolCall>,
        /// Anthropic Messages content blocks, verbatim, in arrival order.
        ///
        /// **Only ever written by the `anthropic_messages` wire** -- which is
        /// why an older record that has it can only have come from that wire,
        /// and why a second wire's replay state does not go here. Anthropic
        /// signs its reasoning blocks and verifies the signature when they come
        /// back in a continuation, so a rebuilt-from-text assistant turn is a
        /// 400 at the next step: xfx cannot reconstruct a signature it did not
        /// keep. Never displayed by any renderer; it exists to go back on the
        /// wire, and it can be large.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        raw_content: Vec<serde_json::Value>,
        /// OpenAI Responses replay items (`reasoning` with `encrypted_content`),
        /// verbatim, in arrival order.
        ///
        /// Disjoint from `raw_content` by construction: a turn writes at most
        /// one of the two. A separate field rather than a tagged shared one,
        /// because a tag's compatibility argument is **syntactic only** -- an
        /// older binary ignores an unknown tag, finds a non-empty `raw_content`,
        /// concludes "Anthropic blocks", and replays Responses items onto the
        /// Messages wire. Separated storage makes that binary see nothing at all
        /// and rebuild from text: degraded, never mis-wired.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        responses_state: Vec<serde_json::Value>,
        /// Which wire *and which authority* produced the state above.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wire: Option<crate::provider::Wire>,
    },
```

Both new fields go **inside the variant** and never on `EventEnvelope`, because `SessionEvent` has no `deny_unknown_fields` (`src/session/event.rs:88`) while `EventEnvelope` does (`:178`) — that asymmetry is exactly what makes an older binary skip them instead of refusing the frame. Neither field needs a `schema_version` bump, for the same reason `raw_content` needed none.

`src/session/store.rs`: `TurnStep::Assistant` gains the same two fields with the same serde attributes, the `apply` arm copies them through, and:

```rust
/// A history rebuilt for one active wire, and what the user has to be told
/// about what did not survive the rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedHistory {
    pub messages: Vec<Message>,
    /// One line per assistant turn whose recorded state was not carried over.
    ///
    /// A drop is never silent: the user is entitled to know that the model
    /// resumed with less context than the log holds.
    pub notices: Vec<String>,
}
```

`history_messages` becomes:

```rust
    /// The durable history, as the messages a next request would carry on
    /// `active`.
    ///
    /// Replay is keyed by **authority, not by shape**. Two wires can serialize
    /// state identically and still not be interchangeable, because the state is
    /// sealed by whoever issued the credential -- so the question is never "does
    /// this look like something I could send", it is "did the provider I am
    /// about to talk to produce it".
    ///
    /// Dropping shapes a *request*; it never mutates a record. The items stay on
    /// disk and a later resume back onto the original authority replays them.
    pub fn history_messages(&self, active: Wire) -> ReplayedHistory {
```

with the per-step decision:

```rust
                        // The replay table of `.prd/04-providers.md` §Provenance,
                        // in one expression. `None` with blocks is legacy
                        // Anthropic, because that is the only wire that ever
                        // wrote the field; everything else must match exactly.
                        let recorded = wire.clone().unwrap_or_else(|| {
                            if raw_content.is_empty() {
                                active.clone()
                            } else {
                                Wire::AnthropicMessages
                            }
                        });
                        let state: &[serde_json::Value] = match (&recorded, &active) {
                            (Wire::AnthropicMessages, Wire::AnthropicMessages) => raw_content,
                            (Wire::CodexResponses, Wire::CodexResponses)
                            | (Wire::GrokResponses, Wire::GrokResponses) => responses_state,
                            _ => &[],
                        };
                        if state.is_empty() && !(raw_content.is_empty() && responses_state.is_empty())
                        {
                            notices.push(format!(
                                "xfx: this session recorded reasoning on the {} wire and is \
                                 resuming on {}, so that reasoning was not carried over",
                                recorded.label(),
                                active.label()
                            ));
                        }
                        pending.push(if state.is_empty() {
                            Message::assistant(Some(text), calls)
                        } else {
                            Message::assistant_raw(state.to_vec(), calls)
                        });
```

Keep the existing "nothing at all is not a step" guard ahead of this, extended to `responses_state`.

`src/agent/machine.rs`: `record_assistant` writes the provenance it was given —

```rust
    journal.record(SessionEvent::AssistantMessage {
        text: completion.text.clone(),
        tool_calls: ..,
        // A terminal step has no continuation to satisfy, but a resumed
        // conversation replays it too, and the authority is checked there.
        raw_content: completion.raw_content.clone(),
        // Nothing on either of today's wires produces Responses items. The field
        // is written empty rather than omitted from the code, so the day a
        // Responses decoder exists there is one line to change and no second
        // recording path to discover.
        responses_state: Vec::new(),
        wire: (!completion.raw_content.is_empty()).then(|| completion.wire.clone()),
    });
```

The `wire` is written **only when there is state to replay**: a turn with nothing to carry gains no field, which is what keeps an older reader's view of those records byte-identical.

- [ ] **Step 4: Feed the active wire in at the two call sites**

`src/app.rs`, in `open_session`: the resume arm becomes

```rust
            let replay = state.history_messages(config.provider.wire());
```

and `OpenedSession` gains `notices: Vec<String>` carried from `replay.notices`; `run_ask` prints each to **stderr** before the turn starts (the answer stays on stdout, and a `--json` caller's stdout stays exactly the event stream).

`src/interactive.rs:525`: `history: conversation.recorder.state().history_messages(config.provider.wire())` becomes the two-field form, and the notices are printed with `writeln!(io::stderr(), "{notice}")?` where the shell already reports turn facts. **This and Task 9 are the only edits this plan makes to `src/interactive.rs`.**

Update the remaining `history_messages()` callers: `src/session/store.rs:2322,2360` (unit tests) and `tests/sessions.rs:281,1197` take `Wire::AnthropicMessages` and read `.messages`.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --test sessions && cargo test --lib session::`
Expected: PASS, including the 6 new tests.

- [ ] **Step 6: Update the ledger and the architecture note**

`docs/parity.md`, the `session event log` row: replace the `raw_content` sentence with

> An `assistant_message` records the provider's own replayable state under **two disjoint fields**: `raw_content` is Anthropic Messages blocks and is only ever written by that wire, and `responses_state` is OpenAI Responses items for the wires that produce them; a turn writes at most one. `wire` names the wire **and its authority** that produced them, and is written only when there is state to replay, so a record with nothing to carry is byte-identical to one an older binary wrote. Replay is keyed by authority, not by shape: a session recorded under one wire and resumed under another **drops** the state -- with a one-line notice on stderr naming both wires -- and replays text and tool calls as normal. The stored items are not deleted, so a later resume back onto the original authority replays them. A record naming a wire this binary does not know drops for the same reason rather than being guessed at. Both fields are additive and absent on older records; neither needs a schema version bump.

`docs/architecture.md`, §"Session log design", the "Replay fidelity" bullet: extend it with the authority rule and the notice, in the same voice.

- [ ] **Step 7: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh`
Expected: exit 0.

```bash
git add src/session/event.rs src/session/store.rs src/agent/machine.rs src/app.rs src/interactive.rs docs/parity.md docs/architecture.md tests/sessions.rs
git commit -m $'replay recorded reasoning by its authority, and say so when it is dropped\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 5: The credential axis and the rule that joins it to providers

One resolver per provider, no I/O, and the guard that says which credential a provider will accept.

**Files:**
- Modify: `src/provider/mod.rs`, `src/config.rs`, `src/output.rs`
- Modify: `docs/parity.md`
- Test: `src/provider/mod.rs` (unit), `tests/cli.rs`, `tests/llmux.rs`

**Interfaces:**
- Consumes: `ProviderId`, `RuntimeConfig.provider`, `RuntimeConfig.llmux_url`, `RuntimeConfig.credential`.
- Produces: `config::CredentialSource::LlmuxLoopback` with `label()` = `provider::LLMUX_LOOPBACK_LABEL`.
- Produces: `provider::ProviderCredential { Bearer(Credential), KeylessLoopback }` with `source(&self) -> CredentialSource`.
- Produces: `provider::resolve_credential_for(provider: ProviderId, config: &RuntimeConfig) -> Option<ProviderCredential>`.
- Produces: `provider::authorizes(provider: ProviderId, source: CredentialSource) -> bool`.

- [ ] **Step 1: Write the failing tests**

In `src/provider/mod.rs`'s test module:

```rust
    #[test]
    fn the_gateway_accepts_any_source_that_is_not_a_subscription() {
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
        assert!(authorizes(ProviderId::Llmux, CredentialSource::LlmuxLoopback));
        for source in [
            CredentialSource::VercelOidcToken,
            CredentialSource::AiGatewayApiKey,
        ] {
            assert!(!authorizes(ProviderId::Llmux, source), "for {source:?}");
        }
    }
```

In `tests/cli.rs`:

```rust
#[test]
fn a_provider_scoped_resolution_never_falls_back_to_another_providers_credential() {
    // Provider-scoped resolution bypasses precedence entirely. There is no
    // fallback from one provider's arrangement down to another's key; switching
    // providers is the only path.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"provider\":\"llmux\"}");
    let config = RuntimeConfig::load_with(
        &environment(&sandbox.home, &[("AI_GATEWAY_API_KEY", "not-a-real-key")]),
        &sandbox.workspace,
    )
    .unwrap();

    assert!(
        resolve_credential_for(ProviderId::Llmux, &config).is_none(),
        "no llmux_url is configured, so llmux has no credential -- and the \
         Gateway key is not a substitute for one"
    );
    assert!(matches!(
        resolve_credential_for(ProviderId::Gateway, &config),
        Some(ProviderCredential::Bearer(_))
    ));
}

#[test]
fn a_configured_llmux_url_is_a_present_credential_without_anything_being_probed() {
    // Credential *presence* is a configuration fact; *reachability* is a network
    // fact, and they must not merge. Nothing is listening on this port.
    let sandbox = Sandbox::new();
    sandbox.write_user_settings(
        "{\"provider\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:1\"}",
    );
    let config =
        RuntimeConfig::load_with(&environment(&sandbox.home, &[]), &sandbox.workspace).unwrap();
    assert!(matches!(
        resolve_credential_for(ProviderId::Llmux, &config),
        Some(ProviderCredential::KeylessLoopback)
    ));

    let status = sandbox.run(&["status", "--json"]).json();
    assert_eq!(status["auth"], "llmux-keyless-loopback");
    assert_eq!(status["auth_refreshable"], false);
}
```

Both use the free `environment(&sandbox.home, &[..])` helper `tests/cli.rs` already has (`tests/cli.rs:910`), and the file imports `use xfx::provider::{resolve_credential_for, ProviderCredential, ProviderId};`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib provider:: && cargo test --test cli credential`
Expected: FAIL — `cannot find function 'authorizes'`, `no variant 'LlmuxLoopback'`.

- [ ] **Step 3: Implement the axis**

`src/config.rs`: `CredentialSource` gains

```rust
    /// llmux's keyless loopback arrangement: authenticated-but-tokenless.
    ///
    /// Not "the daemon is up" -- that is a reachability probe wearing a
    /// credential's clothes -- and not an empty secret passed through a guard
    /// written for a non-empty one, which is how this becomes a confusing 401
    /// later. It resolves from configuration alone.
    LlmuxLoopback,
```

with `label()` returning `crate::provider::LLMUX_LOOPBACK_LABEL`.

`src/provider/mod.rs`:

```rust
/// A credential resolved for one provider.
///
/// `fx` threads a bearer string into every request and into catalog access;
/// llmux is keyless, so this is a sum type rather than an `Option<String>`.
#[derive(Clone)]
pub enum ProviderCredential {
    /// A bearer credential from the environment.
    Bearer(Credential),
    /// A loopback endpoint that answers without one.
    KeylessLoopback,
}

impl ProviderCredential {
    pub fn source(&self) -> CredentialSource {
        match self {
            Self::Bearer(credential) => credential.source(),
            Self::KeylessLoopback => CredentialSource::LlmuxLoopback,
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
    config: &RuntimeConfig,
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
pub fn authorizes(provider: ProviderId, source: CredentialSource) -> bool {
    match provider {
        ProviderId::Gateway => !matches!(source, CredentialSource::LlmuxLoopback),
        ProviderId::Llmux => matches!(source, CredentialSource::LlmuxLoopback),
    }
}
```

`src/output.rs`: `AuthSnapshot::for_config` is rewritten over the resolver, keeping every shipped string:

```rust
    pub fn for_config(config: &RuntimeConfig) -> Self {
        if let Some(rejected) = &config.provider_rejected {
            return Self {
                source: MISSING_AUTH_LABEL.to_string(),
                refreshable: false,
                help: Some(rejected_provider_help(rejected)),
            };
        }
        match crate::provider::resolve_credential_for(config.provider, config) {
            Some(credential) => Self {
                source: credential.source().label().to_string(),
                // No refreshable source exists in this build: no login is
                // implemented, so nothing here can be refreshed. Landing one
                // flips this field and rewrites `UPSTREAM.md` deviation #5 in
                // the same change -- the ledger is the contract, not this line.
                refreshable: false,
                help: None,
            },
            None => Self {
                source: MISSING_AUTH_LABEL.to_string(),
                refreshable: false,
                help: Some(match config.provider {
                    ProviderId::Gateway => MISSING_AUTH_HELP.to_string(),
                    ProviderId::Llmux => crate::llmux::MISSING_URL_HELP.to_string(),
                }),
            },
        }
    }
```

This changes one shipped behavior deliberately: `auth` on the llmux provider **with no usable url** now reports `missing` with the setup help instead of `llmux-keyless-loopback` with the same help. That is the honest reading — there is no arrangement when there is no endpoint — and the help line is unchanged. Pin it by adding one line to `tests/llmux.rs`'s `status_carries_the_refusal_when_llmux_has_no_endpoint`:

```rust
    // There is no keyless arrangement when there is no endpoint to be keyless
    // toward; the help line is what says how to get one.
    assert_eq!(document["auth"], "missing");
```

and say so in the ledger (Step 5).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test --lib provider:: && cargo test --test cli && cargo test --test llmux`
Expected: PASS.

- [ ] **Step 5: Update the ledger**

`docs/parity.md`:

- Add a provider row after the `AI_GATEWAY_API_KEY` row:

  ```markdown
  | `llmux` keyless loopback credential | provider | implemented | Resolved from **configuration alone**: an `llmux_url` that is present and passed the loopback-service policy. Not "the daemon answered" -- resolving a credential does no I/O, or `status` and `doctor` stop being safe to run. Reported as `llmux-keyless-loopback`, never as a secret, and `auth_refreshable` is false. Provider-scoped: it is not a Gateway credential and a Gateway key is not a substitute for it, so a provider with no credential is a refusal rather than a fallback to another provider's. |
  ```

- In the `status/doctor JSON renderer` row, replace the `auth` sentence with: "`auth` names the credential the **active provider** resolved -- an environment variable's name on the Gateway, `llmux-keyless-loopback` on llmux -- or `missing` when that provider has none, in which case `auth_help` carries the refusal: the Gateway's two variables, `xfx setup llmux` when no endpoint resolved, or the unreadable provider setting and its value when nothing validly chose one."

- [ ] **Step 6: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh`
Expected: exit 0.

```bash
git add src/provider/mod.rs src/config.rs src/output.rs docs/parity.md tests/cli.rs tests/llmux.rs
git commit -m $'resolve a credential per provider, and never across providers\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 6: Model catalogs and the selected bundle

The `ModelCatalog` trait, llmux's `GET /models` behind it, and one selection point that turns a configured provider into a transport.

**Files:**
- Create: `src/provider/model.rs`
- Modify: `src/provider/mod.rs`, `src/llmux/setup.rs`, `src/app.rs`, `src/interactive.rs`
- Test: `src/provider/model.rs` (unit), `tests/llmux.rs`

**Interfaces:**
- Consumes: `ProviderId`, `Wire`, `resolve_credential_for`, `authorizes` (Tasks 1, 3, 5).
- Produces: `provider::model::CatalogEntry { id: String, aliases: Vec<String>, name: Option<String>, efforts: Vec<String>, max_context: Option<u64> }` with `matches(&self, name: &str) -> bool` and `preferred_name(&self) -> &str`.
- Produces: `provider::model::ModelCatalog` trait: `async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError>`.
- Produces: `provider::model::CatalogError { Unavailable { detail: String }, Empty, Malformed { detail: String } }`.
- Produces: `provider::model::catalog_for(config: &RuntimeConfig) -> Option<Box<dyn ModelCatalog>>` — `None` means *this provider advertises no catalog*.
- Produces: `provider::model::parse_llmux_catalog(body: &str) -> Option<Vec<CatalogEntry>>`.
- Produces: `provider::Bundle { id: ProviderId, stream: Box<dyn Provider> }` with `entry()`, `wire()`, and `Bundle::select(config: &RuntimeConfig, cancel: &CancelToken) -> Result<Bundle, String>`, replacing `app::build_provider`.

- [ ] **Step 1: Write the failing catalog tests**

In `src/provider/model.rs`'s test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The document a real daemon answers, reduced to the keys xfx reads
    /// (`2lab-ai/llmux@79f66748656b src/catalog.rs:44-63`).
    const REAL_SHAPE: &str = r#"{"models":[
        {"id":"claude-fable-5[1m]","aliases":["fable"],"name":"Claude Fable 5",
         "efforts":["low","medium","high","xhigh","max"],"max_context":1000000,"group":"claude"},
        {"id":"grok-4.5","aliases":["grok"],"name":"Grok 4.5",
         "efforts":["low","medium","high"],"max_context":500000,"group":"grok"}]}"#;

    #[test]
    fn a_catalog_row_carries_everything_the_daemon_publishes_about_a_model() {
        let entries = parse_llmux_catalog(REAL_SHAPE).expect("a catalog document");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "claude-fable-5[1m]");
        assert_eq!(entries[0].aliases, ["fable"]);
        assert_eq!(entries[0].name.as_deref(), Some("Claude Fable 5"));
        assert_eq!(entries[0].efforts, ["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(entries[0].max_context, Some(1_000_000));
        assert_eq!(entries[0].preferred_name(), "fable", "the short name it publishes");
        assert!(entries[0].matches("fable") && entries[0].matches("claude-fable-5[1m]"));
    }

    #[test]
    fn a_row_without_a_usable_id_is_skipped_rather_than_invented() {
        // A model xfx cannot name is a model it cannot ask for.
        let entries = parse_llmux_catalog(
            r#"{"models":[{"aliases":["x"]},{"id":""},{"id":"real"}]}"#,
        )
        .expect("a catalog document");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "real");
    }

    #[test]
    fn optional_metadata_is_absent_rather_than_defaulted() {
        // A context window nobody published is unknown, not zero: `/model` would
        // otherwise print a window the provider never promised.
        let entries = parse_llmux_catalog(r#"{"models":[{"id":"bare"}]}"#).expect("document");
        assert_eq!(entries[0].max_context, None);
        assert!(entries[0].efforts.is_empty());
        assert_eq!(entries[0].name, None);
        assert_eq!(entries[0].preferred_name(), "bare");
    }

    #[test]
    fn a_body_that_is_not_a_catalog_is_not_an_empty_catalog() {
        for body in ["", "llmux", "[]", r#"{"data":[]}"#] {
            assert!(parse_llmux_catalog(body).is_none(), "for {body:?}");
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib provider::model`
Expected: FAIL — `file not found for module 'model'`.

- [ ] **Step 3: Write the catalog module**

Create `src/provider/model.rs` with the doc header, `CatalogEntry`, `CatalogError`, the `ModelCatalog` trait, `parse_llmux_catalog`, `LlmuxCatalog`, and `catalog_for`:

```rust
//! What a provider says it can run.
//!
//! Two catalog concepts, kept apart the way upstream keeps them: the **static
//! identity** of a provider (`ProviderEntry`, which drives labels) and the
//! **fetched model catalog** below, which is what `/model` renders. Only the
//! second one needs a socket, which is why it is a trait: a provider that
//! advertises no catalog has `None`, not an object that answers nothing.
```

```rust
/// One model a provider advertises.
///
/// The optional fields are optional because the daemon really may omit them
/// (`2lab-ai/llmux@79f66748656b src/catalog.rs:44-63`: `max_context` is
/// `Option<u64>`, `efforts` is empty when the model takes no reasoning field).
/// Defaulting either would print a promise the provider did not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub aliases: Vec<String>,
    pub name: Option<String>,
    /// Accepted reasoning-effort values, low to high; empty when the model takes
    /// no reasoning field.
    pub efforts: Vec<String>,
    /// Context window in tokens, or `None` when the provider did not publish one.
    pub max_context: Option<u64>,
}

impl CatalogEntry {
    /// Whether `name` selects this entry, as an id or as an alias.
    pub fn matches(&self, name: &str) -> bool {
        self.id == name || self.aliases.iter().any(|alias| alias == name)
    }

    /// How xfx will ask for this model: the first alias when there is one,
    /// because that is the short name the provider publishes and the one an
    /// operator recognizes; the id otherwise. Either resolves at the provider.
    pub fn preferred_name(&self) -> &str {
        self.aliases.first().map(String::as_str).unwrap_or(&self.id)
    }
}

/// Why a catalog could not be read.
#[derive(Debug)]
pub enum CatalogError {
    /// Nothing answered, or what answered was not usable as a catalog source.
    Unavailable { detail: String },
    /// The provider answered a catalog with no models in it.
    Empty,
    /// The body is not a catalog document at all.
    Malformed { detail: String },
}
```
(with `Display`/`Error` impls in the file's house style: each says what happened and what to do.)

```rust
/// A source of a provider's model catalog.
///
/// One call is one attempt, like [`crate::gateway::Provider`]: the caller
/// decides whether a failure is worth repeating.
#[async_trait::async_trait(?Send)]
pub trait ModelCatalog {
    async fn fetch(&self) -> Result<Vec<CatalogEntry>, CatalogError>;
}
```

`parse_llmux_catalog` is `llmux::setup::parse_catalog` widened to the three extra keys (`name`, `efforts`, `max_context`) with the same skip-a-row-without-an-id rule. `LlmuxCatalog { endpoint: Endpoint }`, constructed by `LlmuxCatalog::new(endpoint: Endpoint) -> Self`, implements the trait by reusing `llmux::setup`'s bounded probe: move `probe_client`, `fetch` and `MAX_PROBE_BODY_BYTES` behavior into a shared `pub(crate) async fn fetch_catalog(url: &str) -> Result<Vec<CatalogEntry>, CatalogError>` in this module, and have `llmux::setup::probe` call it so there is exactly one `GET /models` reader. The client keeps `no_proxy` and `Policy::none` and the same timeouts, for the reasons already documented at `src/llmux/setup.rs:355-380`.

```rust
/// The catalog for the configured provider, when it advertises one.
///
/// `None` is a fact about the provider, not a missing feature: the Vercel
/// Gateway publishes no catalog endpoint this port has evidence for, and xfx
/// does not invent a URL to ask. `/model` on that provider therefore reports the
/// model and its source and says there is nothing to browse.
pub fn catalog_for(config: &RuntimeConfig) -> Option<Box<dyn ModelCatalog>> {
    match config.provider {
        ProviderId::Gateway => None,
        ProviderId::Llmux => {
            let url = config.llmux_url.as_deref()?;
            // Re-checked rather than trusted: the rule that decides what may
            // receive a keyless request belongs to the module that made the
            // promise.
            let endpoint = crate::llmux::endpoint(url, crate::llmux::URL_KEY).ok()?;
            Some(Box::new(LlmuxCatalog::new(endpoint)))
        }
    }
}
```

- [ ] **Step 4: Add the bundle and route every dispatch through it**

In `src/provider/mod.rs`:

```rust
/// A selected provider and the transport that talks to it.
///
/// Upstream's bundle also carries the catalog fetcher
/// (`vercel-labs/fx@ef1d0d0 src/core/config/provider_set.zig:14-45`). xfx keeps
/// the catalog out of it on purpose: `/model` has to work in a shell opened on a
/// machine with **no credential** -- that is exactly the machine whose user
/// needs it -- and a bundle carries a built transport, which that machine cannot
/// build. Catalog access is [`model::catalog_for`], which needs only the
/// configuration.
pub struct Bundle {
    pub id: ProviderId,
    pub stream: Box<dyn Provider>,
}

impl Bundle {
    pub fn entry(&self) -> &'static ProviderEntry {
        self.id.entry()
    }

    pub fn wire(&self) -> Wire {
        self.id.wire()
    }

    /// The one place a configured provider becomes a socket.
    ///
    /// `ask` and the shell both come through here, so "which provider am I
    /// talking to" is decided once rather than in two dispatch sites that could
    /// disagree -- and a third caller that forgot one of them could not exist.
    pub fn select(config: &RuntimeConfig, cancel: &CancelToken) -> Result<Self, String> {
        ..
    }
}
```

`select` is `app::build_provider` moved verbatim — including the unreadable-selection refusal, the missing-credential and missing-url messages, and the endpoint re-check — with two additions: it resolves through `resolve_credential_for` and refuses when `authorizes` says no, quoting the provider and the source. Delete `app::build_provider` and update its two callers: `src/app.rs` (`ask`) and `src/interactive.rs:421,580-583` (`provider: Option<Bundle>`, `ensure_provider` returning `&Bundle`, and `one_turn(bundle.stream.as_ref(), ..)`).

- [ ] **Step 5: Prove the catalog against the fake daemon**

In `tests/llmux.rs`:

```rust
#[test]
fn the_catalog_reader_sees_what_the_daemon_publishes() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(json!({"models": [
        {"id": "claude-fable-5[1m]", "aliases": ["fable"], "name": "Claude Fable 5",
         "efforts": ["low", "high"], "max_context": 1_000_000, "group": "claude"}
    ]}));
    let endpoint = xfx::llmux::endpoint(&daemon.url(), "test").expect("a loopback endpoint");
    let catalog = LlmuxCatalog::new(endpoint);
    let entries = block_on(catalog.fetch()).expect("the daemon answers its catalog");
    assert_eq!(entries[0].preferred_name(), "fable");
    assert_eq!(entries[0].max_context, Some(1_000_000));
    assert_eq!(entries[0].efforts, ["low", "high"]);
    assert_eq!(daemon.paths(), ["/models"], "a catalog load is not a ping");
}

#[test]
fn a_daemon_that_is_not_listening_is_a_catalog_that_is_unavailable_not_empty() {
    // "The daemon has no models" and "xfx could not ask" are different facts and
    // only one of them is a reason to change the recorded model.
    let endpoint = xfx::llmux::endpoint("http://127.0.0.1:1", "test").expect("endpoint");
    let err = block_on(LlmuxCatalog::new(endpoint).fetch()).expect_err("nothing is listening");
    assert!(matches!(err, CatalogError::Unavailable { .. }), "{err:?}");
}
```

- [ ] **Step 6: Run to verify everything passes**

Run: `cargo test --lib provider:: && cargo test --test llmux && cargo test --test cli && cargo test --test interactive`
Expected: PASS.

- [ ] **Step 7: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh && ./scripts/smoke.sh target/release/xfx`
Expected: exit 0. (`cargo build --locked --release` first, so `smoke.sh` has a binary.)

```bash
git add src/provider/mod.rs src/provider/model.rs src/llmux/setup.rs src/app.rs src/interactive.rs tests/llmux.rs
git commit -m $'give each provider a catalog and one place it becomes a socket\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 7: One writer for the profile selection

The write side of the migration: `provider` and `models{}` appear only when something already writes the file, and the legacy pair stays in sync so an older binary degrades to a previously operator-chosen value.

**Files:**
- Create: `src/provider/profile.rs`
- Modify: `src/llmux/setup.rs`, `src/provider/mod.rs`
- Modify: `docs/parity.md`
- Test: `src/provider/profile.rs` (unit), `tests/llmux.rs`

**Interfaces:**
- Consumes: `ProviderId::legacy_backend()` (Task 1), `config::{PROVIDER_KEY, MODELS_KEY, BACKEND_KEY}` (Task 2).
- Produces: `provider::profile::Selection<'a> { provider: ProviderId, model: &'a str, llmux_url: Option<&'a str> }`.
- Produces: `provider::profile::merge_selection(existing: Map<String, Value>, selection: &Selection<'_>, legacy: Option<&str>) -> Map<String, Value>` — pure, and the unit under test.
- Produces: `provider::profile::write(path: &Path, existing: Map<String, Value>, selection: &Selection<'_>) -> io::Result<()>`, plus the staged-write helpers moved out of `llmux::setup` (`read_existing`, `create_private_dir`, `replace_private_file`, `sync_directory`, `StagedFile`).

- [ ] **Step 1: Write the failing merge tests**

In `src/provider/profile.rs`'s test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn object(value: Value) -> Map<String, Value> {
        value.as_object().expect("an object").clone()
    }

    #[test]
    fn a_selection_writes_the_new_keys_and_keeps_the_legacy_pair_in_sync() {
        // Rollback is the dangerous direction and it is decided here: a v0.1.0
        // binary ignores `provider` and `models`, so the two keys it does read
        // have to still say the same thing.
        let merged = merge_selection(
            object(json!({"permission_mode": "auto"})),
            &Selection {
                provider: ProviderId::Llmux,
                model: "fable",
                llmux_url: Some("http://127.0.0.1:3456"),
            },
            ProviderId::Llmux.legacy_backend(),
        );
        assert_eq!(merged["provider"], "llmux");
        assert_eq!(merged["models"]["llmux"], "fable");
        assert_eq!(merged["backend"], "llmux");
        assert_eq!(merged["model"], "fable");
        assert_eq!(merged["llmux_url"], "http://127.0.0.1:3456");
        assert_eq!(merged["permission_mode"], "auto", "unrelated keys survive");
    }

    #[test]
    fn another_providers_model_preference_is_preserved() {
        // Switching away from a provider must not lose what was chosen for it.
        let merged = merge_selection(
            object(json!({"models": {"gateway": "zai/glm-5.2"}})),
            &Selection {
                provider: ProviderId::Llmux,
                model: "fable",
                llmux_url: None,
            },
            ProviderId::Llmux.legacy_backend(),
        );
        assert_eq!(merged["models"]["gateway"], "zai/glm-5.2");
        assert_eq!(merged["models"]["llmux"], "fable");
    }

    #[test]
    fn a_models_value_that_was_not_an_object_is_replaced_rather_than_merged_into() {
        // There is nothing to preserve in a value that was never a map, and
        // trying to merge into it would fail the write over someone else's typo.
        let merged = merge_selection(
            object(json!({"models": "nonsense"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "zai/glm-5.2",
                llmux_url: None,
            },
            ProviderId::Gateway.legacy_backend(),
        );
        assert_eq!(merged["models"], json!({"gateway": "zai/glm-5.2"}));
    }

    #[test]
    fn a_provider_no_older_binary_can_reach_leaves_the_legacy_keys_alone() {
        // The rule for a provider with no representable legacy value: leave
        // `backend` at its previous value rather than inventing one, so an old
        // binary keeps talking to the backend it was last told about instead of
        // to a provider it cannot authenticate. No such provider exists in this
        // build, which is why the rule is proven on the pure helper.
        let merged = merge_selection(
            object(json!({"backend": "llmux", "model": "fable"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "some-future-model",
                llmux_url: None,
            },
            None,
        );
        assert_eq!(merged["backend"], "llmux", "untouched");
        assert_eq!(merged["model"], "fable", "untouched");
        assert_eq!(merged["provider"], "gateway");
        assert_eq!(merged["models"]["gateway"], "some-future-model");
    }

    #[test]
    fn a_selection_without_a_url_does_not_erase_a_recorded_one() {
        let merged = merge_selection(
            object(json!({"llmux_url": "http://127.0.0.1:3456"})),
            &Selection {
                provider: ProviderId::Gateway,
                model: "zai/glm-5.2",
                llmux_url: None,
            },
            ProviderId::Gateway.legacy_backend(),
        );
        assert_eq!(merged["llmux_url"], "http://127.0.0.1:3456");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib provider::profile`
Expected: FAIL — `file not found for module 'profile'`.

- [ ] **Step 3: Write the writer**

Create `src/provider/profile.rs`. `merge_selection` is pure and does exactly what the tests pin. `write` is `llmux::setup::record` generalized: it takes the already-read document, merges, serializes pretty with a trailing newline, creates the profile home `0700` if absent, and replaces the file through a staged `0600` file plus rename plus directory `fsync`. Move `read_existing`, `create_private_dir`, `stage_path`, `StagedFile`, `replace_private_file` and `sync_directory` here verbatim, with their doc comments, and re-point `llmux::setup` at them — one writer, not two, because two would eventually disagree about the mode bits.

The module doc says why the legacy pair is written:

```rust
//! The one thing that writes `~/.xfx/settings.json`.
//!
//! Two properties make this a module rather than a function on a command:
//!
//! 1. **Read-repair never rewrites.** The new keys appear only here, because
//!    only a command the operator ran to change something is allowed to change
//!    the file. Loading, `status` and `doctor` leave it byte-identical.
//! 2. **Rollback is decided by the writer.** A v0.1.0 binary reads only the keys
//!    it knows, so it silently ignores `provider` and `models` and falls back to
//!    `backend` and `model`. That is safe **only if the writer keeps those two
//!    in sync**, which is what this does for as long as the selected provider
//!    has a value an older binary can reach. When it does not, the legacy keys
//!    are left at their previous values rather than given invented ones: an old
//!    binary then keeps talking to the backend it was last told about, which is
//!    a previously operator-chosen endpoint and never a compiled default.
```

- [ ] **Step 4: Route `setup llmux` through it and prove the file**

`src/llmux/setup.rs`'s `run` builds a `Selection` and calls `provider::profile::write`. Its model decision is unchanged in this task — it still reads the profile document it is about to write rather than the resolved config — except that the "configured" model it compares against is now `models[llmux]` when the document has one, else the flat `model`, else `DEFAULT_MODEL`, which is the same precedence the loader applies.

In `tests/llmux.rs`, extend `every_key_setup_writes_is_a_key_the_loader_reads_back` and add:

```rust
#[test]
fn setup_writes_the_new_keys_and_leaves_an_older_binarys_view_correct() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(catalog(&[("m-1", &["short"])]));
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"permission_mode\":\"auto\"}");
    assert_eq!(
        sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]).code,
        Some(0)
    );

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.settings_path()).unwrap()).unwrap();
    assert_eq!(settings["provider"], "llmux");
    assert_eq!(settings["models"]["llmux"], "short");
    // What a v0.1.0 binary reads. It ignores the two keys above, so these have
    // to say the same thing or an older binary would send the prompt somewhere
    // the operator did not choose.
    assert_eq!(settings["backend"], "llmux");
    assert_eq!(settings["model"], "short");
    assert_eq!(settings["permission_mode"], "auto");
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --lib provider:: && cargo test --test llmux`
Expected: PASS.

- [ ] **Step 6: Update the ledger**

In the `provider selection` persistence row, append: "The file gains `provider` and `models` only when something already writes it -- `xfx setup <provider>` -- through the same staged `0600` file and rename that preserves every unrelated key, and the writer keeps `backend` and `model` in sync with them for as long as the selected provider has a value an older binary can reach; when it does not, the legacy keys are left at their previous values rather than given invented ones."

In the `setup` command row, append: "It writes `provider` and `models[<target>]` alongside `backend` and `model`, so a v0.1.0 binary reading the same profile still resolves the endpoint the operator chose."

- [ ] **Step 7: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh && ./scripts/check-no-secrets.sh`
Expected: exit 0.

```bash
git add src/provider/profile.rs src/provider/mod.rs src/llmux/setup.rs docs/parity.md tests/llmux.rs
git commit -m $'write the provider selection once, and keep the legacy keys true\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 8: `xfx setup <provider>` — switching on the surface that already exists

Upstream removed `/provider` in 0.0.5 and folded switching into `/setup` (`05-upstream-delta.md` §B2). xfx does the same on the line surface it already has: `setup` takes a provider target, and the transaction is the one upstream runs.

**Files:**
- Create: `src/provider/setup.rs`
- Modify: `src/cli.rs`, `src/app.rs`, `src/output.rs`, `src/llmux/setup.rs`
- Modify: `docs/parity.md`, `CHANGELOG.md`
- Test: `tests/cli.rs`, `tests/llmux.rs`

**Interfaces:**
- Consumes: `provider::profile::{Selection, write}` (Task 7), `model::catalog_for` (Task 6), `resolve_credential_for`, `authorizes` (Task 5).
- Produces: `cli::Command::Setup { provider: ProviderId, url: Option<String>, json: bool }`.
- Produces: `provider::setup::{SetupReport, SetupError}` (moved out of `llmux::setup`, which keeps `run`, `candidates`, `discover_in` and re-exports the two types so existing importers still compile).
- Produces: `provider::setup::run(config, env, provider, explicit_url) -> Result<SetupReport, SetupError>` — the six-step transaction.
- Produces: `SetupReport { provider: ProviderId, url: Option<String>, models: Option<usize>, model: String, model_reason: String, credential: Option<CredentialSource>, settings_path: PathBuf, overridden_by: Option<String>, credential_warning: Option<String> }`. `credential` is the **source**, never a secret; `SetupSnapshot` renders it as `auth`, and `None` renders as `output::MISSING_AUTH_LABEL`.

- [ ] **Step 1: Write the failing grammar and transaction tests**

In `src/cli.rs`'s own test module — that is where `parse` and `Command` live, and where the shipped `setup_names_exactly_one_target_and_carries_only_its_two_flags` sits; **replace** that test with these two, because the surface it pins is the one that changed:

```rust
    #[test]
    fn setup_names_every_provider_it_can_configure_and_nothing_else() {
        assert!(matches!(
            parse(&["setup", "gateway"]),
            Command::Setup {
                provider: ProviderId::Gateway,
                url: None,
                json: false
            }
        ));
        assert!(matches!(
            parse(&["setup", "llmux", "--json"]),
            Command::Setup {
                provider: ProviderId::Llmux,
                json: true,
                ..
            }
        ));
        // A bare `setup` is not a menu, and a provider this build cannot reach
        // is not a target: `codex` parsing here would promise a login xfx
        // cannot perform.
        for rejected in [
            vec!["setup"],
            vec!["setup", "codex"],
            vec!["setup", "login"],
            vec!["setup", "llmux", "extra"],
            vec!["setup", "llmux", "--url"],
        ] {
            assert!(
                matches!(parse(&rejected), Command::Rejected { .. }),
                "{rejected:?} must be rejected"
            );
        }
    }

    #[test]
    fn a_url_is_refused_for_a_provider_that_has_no_endpoint_to_name() {
        // A flag that means nothing for the named target is a typo worth
        // reporting, not a value to ignore.
        let Command::Rejected { message } =
            parse(&["setup", "gateway", "--url", "http://127.0.0.1:1"])
        else {
            panic!("a url on the gateway target must be rejected");
        };
        assert!(message.contains("--url"), "{message}");
        assert!(message.contains("llmux"), "{message}");
    }
```

and in `tests/cli.rs`, the binary-level half:

```rust
#[test]
fn setup_gateway_records_the_selection_and_says_what_credential_will_be_used() {
    let sandbox = Sandbox::new();
    sandbox.write_user_settings("{\"backend\":\"llmux\",\"llmux_url\":\"http://127.0.0.1:3456\",\"model\":\"fable\"}");
    let run = sandbox.run_with_env(
        &["setup", "gateway", "--json"],
        &[("AI_GATEWAY_API_KEY", "planted-key-not-a-real-one")],
    );
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);

    let document = run.json();
    assert_eq!(document["kind"], "setup");
    assert_eq!(document["provider"], "gateway");
    assert_eq!(document["auth"], "AI_GATEWAY_API_KEY");
    assert!(document.get("url").is_none(), "the gateway has no daemon url");
    assert!(document.get("models").is_none(), "and advertises no catalog");
    // The credential is named, never printed.
    assert!(!run.stdout.contains("planted-key"), "{}", run.stdout);
    assert!(!run.stderr.contains("planted-key"), "{}", run.stderr);

    let settings: Value = serde_json::from_str(
        &fs::read_to_string(sandbox.profile_dir().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["provider"], "gateway");
    assert_eq!(settings["backend"], "gateway");
    // Every unrelated key survives, including the other provider's parameters.
    assert_eq!(settings["llmux_url"], "http://127.0.0.1:3456");
    assert_eq!(settings["models"]["llmux"], "fable");
}

#[test]
fn setup_gateway_records_the_selection_and_warns_when_no_credential_is_set() {
    // The profile is machine state and the environment is shell state: refusing
    // a durable write because of an ephemeral shell would be the same defect as
    // persisting an XFX_MODEL that outranks the file.
    let sandbox = Sandbox::new();
    let run = sandbox.run(&["setup", "gateway"]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert!(run.stdout.contains("[setup] provider=gateway"), "{}", run.stdout);
    assert!(run.stderr.contains("VERCEL_OIDC_TOKEN"), "{}", run.stderr);

    let settings: Value = serde_json::from_str(
        &fs::read_to_string(sandbox.profile_dir().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["provider"], "gateway");
}

#[test]
fn setup_does_not_offer_a_provider_this_build_cannot_reach() {
    let help = Sandbox::new().run(&["setup", "--help"]).stdout;
    for absent in ["codex", "grok", "chatgpt"] {
        assert!(!help.contains(absent), "`{absent}` is deferred: {help}");
    }
}
```

In `tests/llmux.rs`, one test that the switch is complete end to end:

```rust
#[test]
fn switching_to_the_gateway_and_back_keeps_each_providers_model() {
    let daemon = FakeLlmux::start(vec![Reply::Sse(anthropic_answer(&["back"]))])
        .with_catalog(catalog(&[("m-1", &["fable"])]));
    let sandbox = Sandbox::new();

    assert_eq!(sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]).code, Some(0));
    assert_eq!(sandbox.run(&["setup", "gateway"], &[]).code, Some(0));
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(sandbox.settings_path()).unwrap()).unwrap();
    assert_eq!(settings["models"]["llmux"], "fable", "kept while unused");
    assert_eq!(settings["models"]["gateway"], xfx::config::DEFAULT_MODEL);

    assert_eq!(sandbox.run(&["setup", "llmux", "--url", &daemon.url()], &[]).code, Some(0));
    let run = sandbox.run(&["ask", "--json", "--no-save", "hello"], &[]);
    assert_eq!(run.code, Some(0), "stderr={:?}", run.stderr);
    assert_eq!(daemon.only_message_request().json()["model"], "fable");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib cli:: && cargo test --test cli setup && cargo test --test llmux switching`
Expected: FAIL — `struct variant 'Command::Setup' has no field named 'provider'`, and at the binary level `setup gateway` is still `Rejected`.

- [ ] **Step 3: Widen the grammar**

`src/cli.rs`: `Command::Setup` gains `provider: ProviderId`; the raw `target` is parsed with `ProviderId::parse`, and `SETUP_USAGE` becomes

```rust
/// What `xfx setup` says when it was not given a target it can perform.
///
/// It lists the providers this build can actually configure. A target for a
/// provider whose login xfx cannot perform would advertise a surface that does
/// not exist -- which is why `codex` and `grok` are not here and are not
/// mentioned as coming.
const SETUP_USAGE: &str = "xfx setup: name the provider to set up -- `gateway` or `llmux` \
     (usage: xfx setup <gateway|llmux> [--url URL] [--json]; --url is llmux only)";
```

A `--url` with a non-llmux target is `Command::Rejected` naming both the flag and the one target it belongs to. The clap `Setup` variant's `target` help becomes `What to set up: gateway or llmux`.

- [ ] **Step 4: Write the transaction**

Create `src/provider/setup.rs` holding `SetupReport`, `SetupError` (moved from `llmux::setup`, which re-exports them) and:

```rust
/// The provider switch, as a transaction.
///
/// The six steps are upstream's `switchProvider`
/// (`vercel-labs/fx@ef1d0d0 src/app/app_auth_runtime.zig` ~760-960), minus the
/// two that need a login xfx does not implement:
///
/// 1. resolve the target's credential -- and **only** the target's;
/// 2. check that the target will accept it (`authorizes`);
/// 3. fetch the target's catalog, when it advertises one, and require it to be
///    non-empty and valid;
/// 4. pick the model: the profile's preference for this provider when the
///    catalog has it, else the catalog's first entry, else the profile's
///    preference unchanged;
/// 5. publish atomically -- selection, model and provider parameters in one
///    staged write;
/// 6. report what still outranks the file, and what the machine is missing.
///
/// Step 2 does **not** refuse a target whose credential is absent. The profile
/// is machine state and the environment is shell state, and refusing a durable
/// write because of an ephemeral shell is the defect this command already fixed
/// once for `XFX_MODEL`. It warns instead, on stderr, in both output modes.
pub async fn run(
    config: &RuntimeConfig,
    env: &Environment,
    provider: ProviderId,
    explicit_url: Option<&str>,
) -> Result<SetupReport, SetupError>
```

The llmux arm delegates to `llmux::setup::run`'s discovery and catalog proof (unchanged: `GET /` must answer exactly `llmux` **and** `GET /models` must answer a non-empty catalog, each read through a bounded stream, and no completion request is sent). The Gateway arm does **no I/O at all**: it has no daemon to ping and advertises no catalog, so it resolves the credential, keeps the profile's model for that provider, and publishes. `credential_warning` is `Some(MISSING_AUTH_HELP)` for a Gateway with no credential and `Some(llmux::MISSING_URL_HELP)` for llmux — the latter is unreachable after a successful probe and is written as the same expression so the two arms cannot drift.

`src/output.rs`: `SetupSnapshot` gains `pub provider: &'static str` (replacing `backend`), `pub auth: String`, and makes `url` and `models` `Option` with `skip_serializing_if`; `render_text` prints `provider`, then `url` and `models` when present, then `model`, `model_reason`, `auth`, `settings_path`, and `overridden_by`. `override_warning` is unchanged and joined by a second `credential_warning()` that returns the report's warning; `src/app.rs` writes both to stderr in both modes.

`src/app.rs`: `setup_llmux` becomes `setup_provider(config, env, provider, url, json, stdout, stderr)`, dispatching to `provider::setup::run`. Failure reporting is unchanged.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --lib cli:: && cargo test --test cli && cargo test --test llmux && cargo test --test parity`
Expected: PASS. `tests/parity.rs` is the one that fails if the ledger and the parser disagree about what `setup` accepts, so read its output rather than only its status.

- [ ] **Step 6: Update the ledger and the changelog**

`docs/parity.md`:

- `setup` command row: change the grammar to ``<gateway|llmux> [--url URL] [--json]`` and open the notes with: "Selects the provider a turn will talk to and records it. This is xfx's provider-switching surface, in the same place upstream put it when it removed `/provider` in 0.0.5. `llmux` additionally discovers and proves a daemon" — keeping the whole existing llmux paragraph — and close with: "`gateway` performs **no network I/O**: it has no daemon to probe and advertises no catalog, so it records the selection, keeps the profile's model for that provider, and names the credential the environment supplies. **It is not credential onboarding**: no key is read from a prompt and none is written, and a target with no resolvable credential is recorded with a warning naming the two variables rather than refused, because the profile is machine state and the environment is shell state. Upstream's interactive Gateway onboarding remains absent."
- `provider` command row: append "Provider *switching* is implemented as `xfx setup <provider>`, which is where upstream moved it in 0.0.5; the standalone `provider` command name is not advertised."
- `setup renderers` UI row: replace the field list with "provider, url and catalog size when the provider has them, model, why that model, the credential source, settings path, and `overridden_by`", and note that the credential warning goes to stderr in both modes for the same reason the override warning does.

`CHANGELOG.md`, under `## [Unreleased]`, add a `### Added` section:

```markdown
### Added

- **`xfx setup <gateway|llmux>` switches providers.** `setup` now names the
  provider it is configuring, and the profile records `provider` plus a
  `models` object keyed by provider, so each provider keeps its own model across
  a switch. The keys a v0.1.0 binary reads -- `backend` and `model` -- are
  written alongside them and kept in sync, so an older binary reading the same
  profile still resolves the endpoint the operator chose. `setup gateway` is a
  selection, not credential onboarding: it reads no key and writes none.
```

- [ ] **Step 7: Run the full gate and commit**

Run: `cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && ./scripts/check-no-stubs.sh && ./scripts/check-no-secrets.sh && cargo build --locked --release && ./scripts/smoke.sh target/release/xfx`
Expected: exit 0. Read the `check (ubuntu-latest)` job before believing this one: `tests/cli.rs` is a grammar test and parts of it are compiled out on macOS.

```bash
git add src/provider/setup.rs src/provider/mod.rs src/cli.rs src/app.rs src/output.rs src/llmux/setup.rs docs/parity.md CHANGELOG.md tests/cli.rs tests/llmux.rs
git commit -m $'switch providers from the setup command\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

### Task 9: The `/model` engine seam, and the shell on top of it

The rules of `/model` move into the engine so two front ends can share them. **This is the produces-interface the TUI plan consumes.** The shell keeps printing `[shell] key=value` lines; no picker, no menu, no alternate screen.

**Files:**
- Modify: `src/provider/model.rs`, `src/interactive.rs`
- Modify: `docs/parity.md`
- Test: `src/provider/model.rs` (unit), `tests/interactive.rs`, `tests/llmux.rs`

**Interfaces:**
- Consumes: `catalog_for`, `CatalogEntry`, `CatalogError` (Task 6), `RuntimeConfig` (Tasks 1–2).
- **Produces — the TUI seam.** All of it in `src/provider/model.rs`, exported as `xfx::provider::model::*`:

  ```rust
  pub struct ModelSelector { /* private */ }

  impl ModelSelector {
      /// The selector for the configured provider and its active model.
      /// Does no I/O: constructing one on a machine with no credential and no
      /// daemon must work, because that is the machine whose user needs `/model`.
      pub fn new(config: &RuntimeConfig) -> Self;
      /// The provider whose catalog this selector browses.
      pub fn provider(&self) -> ProviderId;
      /// The model in force right now.
      pub fn model(&self) -> &str;
      /// Which settings layer chose it.
      pub fn source(&self) -> SettingSource;
      /// The catalog as far as it is known. Never performs I/O.
      pub fn catalog(&self) -> &CatalogState;
      /// Loads the catalog once, if the provider advertises one and it has not
      /// been loaded in this process. **This is the only method that opens a
      /// socket**; call it where a network call is already legitimate.
      pub async fn ensure_catalog(&mut self) -> &CatalogState;
      /// Applies one `/model` request and returns what the caller must render.
      /// Never performs I/O: it decides against whatever `catalog()` holds.
      pub fn apply(&mut self, request: ModelRequest<'_>) -> ModelOutcome;
  }

  pub enum ModelRequest<'a> {
      /// `/model` with no argument.
      Report,
      /// `/model <id>`.
      Select(&'a str),
  }

  pub enum ModelOutcome {
      Reported { provider: ProviderId, model: String, source: SettingSource },
      Selected { provider: ProviderId, model: String, previous: String,
                 /// Set when the catalog could not be consulted, so the caller
                 /// can say the selection was accepted unverified.
                 unverified: Option<String> },
      Unchanged { model: String },
      Refused { reason: String },
  }

  pub enum CatalogState {
      /// This provider advertises no catalog.
      Unavailable,
      /// Not loaded in this process yet.
      NotLoaded,
      /// Loaded, with the entries in the order the provider published them.
      Loaded(Vec<CatalogEntry>),
      /// A load was attempted and failed. Carries the one-line reason.
      Failed(String),
  }

  /// The most catalog rows a caller renders in one go, matching `list_files`'
  /// ceiling; a caller that hits it says how many were left out.
  pub const MAX_RENDERED_MODELS: usize = 100;
  ```

- [ ] **Step 1: Write the failing seam tests**

In `src/provider/model.rs`'s test module:

```rust
    fn entry(id: &str, aliases: &[&str]) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            aliases: aliases.iter().map(|a| (*a).to_string()).collect(),
            name: None,
            efforts: Vec::new(),
            max_context: None,
        }
    }

    #[test]
    fn a_report_names_the_provider_the_model_and_the_layer_that_chose_it() {
        let selector = ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        let mut selector = selector;
        match selector.apply(ModelRequest::Report) {
            ModelOutcome::Reported { provider, model, source } => {
                assert_eq!(provider, ProviderId::Llmux);
                assert_eq!(model, "fable");
                assert_eq!(source, SettingSource::UserGlobal);
            }
            other => panic!("expected a report, got {other:?}"),
        }
    }

    #[test]
    fn a_selection_the_loaded_catalog_does_not_have_is_refused_by_name() {
        // The provider published what it can run. An id it did not publish is a
        // turn that fails at the provider, and failing here says why.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry("m-1", &["fable"])]));
        match selector.apply(ModelRequest::Select("not-published")) {
            ModelOutcome::Refused { reason } => {
                assert!(reason.contains("not-published"), "{reason}");
                assert!(reason.contains("llmux"), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(selector.model(), "fable", "a refusal changes nothing");
    }

    #[test]
    fn an_alias_selects_the_entry_it_belongs_to() {
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "other", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Loaded(vec![entry("m-1", &["fable"])]));
        match selector.apply(ModelRequest::Select("fable")) {
            ModelOutcome::Selected { model, previous, unverified, .. } => {
                assert_eq!(model, "fable");
                assert_eq!(previous, "other");
                assert_eq!(unverified, None);
            }
            other => panic!("expected a selection, got {other:?}"),
        }
    }

    #[test]
    fn a_selection_is_accepted_unverified_when_the_catalog_could_not_be_read() {
        // A daemon that is down must not stop an operator from changing a
        // preference. It stops xfx from *checking* it, which is a different
        // sentence and the one the user gets.
        let mut selector =
            ModelSelector::for_test(ProviderId::Llmux, "fable", SettingSource::UserGlobal);
        selector.set_catalog_for_test(CatalogState::Failed("the daemon did not answer".to_string()));
        match selector.apply(ModelRequest::Select("anything")) {
            ModelOutcome::Selected { model, unverified: Some(reason), .. } => {
                assert_eq!(model, "anything");
                assert!(reason.contains("did not answer"), "{reason}");
            }
            other => panic!("expected an unverified selection, got {other:?}"),
        }
    }

    #[test]
    fn a_provider_with_no_catalog_accepts_any_well_formed_id() {
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "zai/glm-5.2", SettingSource::CompiledDefault);
        assert!(matches!(selector.catalog(), CatalogState::Unavailable));
        assert!(matches!(
            selector.apply(ModelRequest::Select("openai/gpt-5")),
            ModelOutcome::Selected { unverified: None, .. }
        ));
    }

    #[test]
    fn a_model_id_is_one_bounded_printable_word_whatever_the_provider() {
        // The id becomes an HTTP header value and a durable session field.
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "zai/glm-5.2", SettingSource::CompiledDefault);
        for bad in ["", "two words", "with\u{0}control"] {
            assert!(
                matches!(selector.apply(ModelRequest::Select(bad)), ModelOutcome::Refused { .. }),
                "for {bad:?}"
            );
        }
    }

    #[test]
    fn selecting_the_model_already_in_force_changes_nothing() {
        let mut selector =
            ModelSelector::for_test(ProviderId::Gateway, "zai/glm-5.2", SettingSource::UserGlobal);
        assert!(matches!(
            selector.apply(ModelRequest::Select("zai/glm-5.2")),
            ModelOutcome::Unchanged { .. }
        ));
    }
```

`for_test` and `set_catalog_for_test` are `#[cfg(test)]` constructors on `ModelSelector`; the production constructor is `new(config)`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test --lib provider::model`
Expected: FAIL — `cannot find type 'ModelSelector' in this scope`.

- [ ] **Step 3: Implement the seam**

In `src/provider/model.rs`, with the validation moved out of `src/interactive.rs` (`model_id_problem`, `MAX_MODEL_BYTES`) so both front ends hold ids to the same rule, and `CatalogState`/`ModelOutcome` deriving `Debug` so a test can print them.

`ModelSelector::new` calls `catalog_for(config)` **once** and keeps the boxed fetcher — `None` becomes `CatalogState::Unavailable`, `Some` becomes `CatalogState::NotLoaded` — so the selector owns everything it needs and borrows nothing from the configuration. It also copies `config.provider`, `config.model` and `config.sources.model` at construction. `ensure_catalog` runs the kept fetcher and records `Loaded(entries)` or `Failed(reason)` where `reason` is the `CatalogError`'s one-line message; it returns immediately when the state is anything other than `NotLoaded`, so a failed load is **not** retried within the process — a `/model` that reopens a dead socket every time it is typed is the shell hanging on the daemon's behalf.

- [ ] **Step 4: Put the shell on top of it**

`src/interactive.rs`, `apply_model` only — the function keeps its signature shape and becomes a renderer:

```rust
/// Applies `/model`, returning the model in force afterwards.
///
/// The rules live in [`crate::provider::model::ModelSelector`] so the shell and
/// any other front end cannot disagree about what `/model` means; what stays
/// here is the printing, which is this surface's own business.
async fn apply_model(
    argument: &str,
    selector: &mut ModelSelector,
    conversation: Option<&mut Conversation>,
) -> io::Result<String> {
    if argument.is_empty() {
        // The one place `/model` is allowed to touch the network: the catalog
        // load is where a provider that is not answering legitimately surfaces,
        // and `status` and `doctor` are where it must not.
        selector.ensure_catalog().await;
    }
    match selector.apply(if argument.is_empty() {
        ModelRequest::Report
    } else {
        ModelRequest::Select(argument)
    }) {
        ..
    }
}
```

Rendered lines, all on stdout except refusals and the unverified warning:

- `Report`: `[shell] model=<model> provider=<slug> source=<layer>`, then either `[shell] catalog=<n> models` followed by up to `MAX_RENDERED_MODELS` lines of `[shell]   <preferred-name> context=<max_context|unknown> efforts=<a,b,c|none>` and, when it clipped, `[shell]   ... and <k> more`; or `[shell] catalog=unavailable (this provider advertises none)`; or `[shell] catalog=unread (<reason>)`.
- `Selected`: `[shell] model=<model>` and, when `unverified` is set, `xfx: <reason>, so this model was not checked against the provider's catalog` on stderr. The durable `PreferencesChanged` event is recorded exactly as today.
- `Unchanged` / `Refused`: the shipped lines.

The call site becomes `model = apply_model(&argument, &mut selector, conversation.as_mut()).await?;`, with `let mut selector = ModelSelector::new(config);` beside `let mut model = config.model.clone();`. **These are the only edits this task makes to `src/interactive.rs`.**

- [ ] **Step 5: Prove it on a real pty and against a real catalog**

Both go in `tests/interactive.rs`, on that file's real pty harness (`Pty::open()`, `start(..)`, `session.type_line(..)`, `session.wait_for(..)`, `session.quit()`) — **never fake a terminal**, and never open a second harness when this one exists.

`tests/interactive.rs` has `command()` and `command_with(&FakeGateway)`; add the llmux sibling beside them, in the same shape:

```rust
    /// A command whose profile points at a scripted local llmux daemon.
    ///
    /// Written into the profile rather than passed as a flag, because the
    /// provider is a property of the machine and that is where a real one is
    /// configured -- and because writing it is what proves the shell reads it.
    fn command_with_llmux(&self, daemon: &FakeLlmux) -> Command {
        let dir = self.home.join(".xfx");
        fs::create_dir_all(&dir).expect("create the profile dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
                .expect("tighten the profile dir");
        }
        fs::write(
            dir.join("settings.json"),
            format!(
                "{{\"provider\":\"llmux\",\"llmux_url\":{},\"models\":{{\"llmux\":\"m-1\"}}}}",
                serde_json::to_string(&daemon.url()).expect("a url is serializable")
            ),
        )
        .expect("write the profile");
        self.command()
    }
```

with `use support::fake_llmux::FakeLlmux;` added to the file's imports.

```rust
#[test]
fn slash_model_reports_the_provider_and_says_the_gateway_has_no_catalog_to_browse() {
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command());

    session.type_line("/model");
    let text = session.wait_for("catalog=");
    assert!(text.contains("provider=gateway"), "{text}");
    // A fact about the provider, not a missing feature -- and not a network
    // call: nothing was contacted to find this out.
    assert!(text.contains("catalog=unavailable"), "{text}");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_browses_the_catalog_the_daemon_publishes() {
    let daemon = FakeLlmux::start(Vec::new()).with_catalog(json!({"models": [
        {"id": "m-1", "aliases": ["fable"], "name": "Claude Fable 5",
         "efforts": ["low", "high"], "max_context": 1_000_000}
    ]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    session.type_line("/model");
    let text = session.wait_for("catalog=");
    assert!(text.contains("provider=llmux"), "{text}");
    assert!(text.contains("catalog=1 models"), "{text}");
    assert!(text.contains("fable"), "{text}");
    assert!(text.contains("context=1000000"), "{text}");
    assert!(text.contains("efforts=low,high"), "{text}");
    assert_eq!(daemon.paths(), ["/models"], "browsing is not a ping");

    assert_eq!(session.quit().code(), Some(0));
}

#[test]
fn slash_model_refuses_an_id_the_daemon_does_not_publish() {
    let daemon = FakeLlmux::start(Vec::new())
        .with_catalog(json!({"models": [{"id": "m-1", "aliases": ["fable"]}]}));
    let sandbox = Sandbox::new();
    let pty = Pty::open();
    let mut session = start(&sandbox, &pty, sandbox.command_with_llmux(&daemon));

    // Load the catalog first, the way a user does: the refusal is only possible
    // against a catalog xfx actually read.
    session.type_line("/model");
    session.wait_for("catalog=1 models");
    session.type_line("/model not-published");
    let text = session.wait_for("not-published");
    assert!(text.contains("llmux"), "{text}");

    session.type_line("/model");
    let text = session.wait_for("model=");
    assert!(text.contains("m-1"), "a refusal changes nothing: {text}");

    assert_eq!(session.quit().code(), Some(0));
}
```

The existing `slash_model_reports_the_active_model_and_changes_it_for_later_turns` and `slash_model_refuses_an_unusable_name_rather_than_sending_it` must keep passing untouched — they run on the Gateway, which has no catalog, so nothing about their path changed.

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test --lib provider::model && cargo test --test interactive && cargo test --test llmux`
Expected: PASS.

- [ ] **Step 7: Update the ledger**

`docs/parity.md`, the `/model` slash row: replace "It does not browse a catalog -- `models` and `provider` are separate deferred rows -- and an id the Gateway rejects is reported by the Gateway." with:

> With no argument it reports the model, the active provider, and the settings layer that chose it, and browses that provider's catalog when it advertises one -- each entry's published name, context window and effort levels, bounded at 100 rows with an explicit `... and N more` line. **The catalog load is the one network call `/model` makes**, and it is where a provider that is not answering surfaces; `status` and `doctor` still perform no I/O and are still always safe to run. With an argument it uses that model from the next turn on and records a durable `preferences_changed` event. An id the loaded catalog does not publish is refused by name rather than sent; when the catalog could not be read the selection is accepted and the shell says it was not checked, because a provider that is down must not stop an operator from changing a preference. Bounded to one printable word: the id becomes an HTTP header. The standalone `models` and `provider` commands remain separate deferred rows.

- [ ] **Step 8: Run the whole gate and commit**

Run:
```bash
cargo fmt --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets && cargo build --locked --release && ./scripts/check-no-stubs.sh && ./scripts/check-no-secrets.sh && ./scripts/check-xfx-identity.sh && ./scripts/check-preview-contract.sh && ./scripts/smoke.sh target/release/xfx
```
Expected: every command exits 0. Then run the binary by hand, as `CONTRIBUTING.md` requires: `./target/release/xfx setup llmux`, `./target/release/xfx status`, and a real shell session exercising `/model` with and without an argument.

```bash
git add src/provider/model.rs src/interactive.rs docs/parity.md tests/interactive.rs tests/llmux.rs
git commit -m $'browse a provider catalog from /model, through one shared selector\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>'
```

---

## What this plan deliberately does not do

Each of these is a `deferred` row that stays exactly as it is, and none of them is hinted at in help, in `/help`, or in any menu.

- **Codex and Grok, and any OAuth at all.** Policy option D. The decision gate in `04-providers.md` §"Policy risks" must be answered — an xfx-owned registration approved, the user's explicit written acceptance of presenting as fx, or llmux mediation — before step 3 starts, and the answer is recorded in `UPSTREAM.md`. A `/setup` entry offering a sign-in xfx cannot perform would be the dishonest thing; waiting is not.
- **`/provider`, `xfx provider`, `xfx models`, `/models`, `login`, `logout`, teams, credits, usage.**
- **A Gateway model catalog.** No endpoint for one is researched anywhere in `.prd/`; `catalog_for` returns `None` for that provider, which is a fact rather than a gap to paper over.
- **Context-window and effort *rendering* beyond what a provider publishes.** llmux publishes both; the Gateway publishes no catalog; Codex/Grok metadata arrives with Codex/Grok.
- **Choosing an effort.** A catalog's `efforts` are rendered and nothing selects one: xfx has no `/effort` surface, and llmux's own wire takes adaptive thinking because pinning `thinking.type=disabled` is refused by the daemon for the default model (`docs/parity.md`, llmux provider row). A shared `ReasoningEffort` and a request shaper that translates it per wire arrive with the surface that sets it.
- **A persisted `credential_source` preference.** `04-providers.md`'s resolver puts an explicit user choice first, ahead of the environment. xfx has exactly two Gateway sources and no way to have chosen between them, so the key stays in `PROFILE_ONLY_KEYS` (where it already is) and is still read by nothing. It becomes load-bearing when a third source exists.
- **Any TUI surface.** No picker, no menu, no alternate screen, no `catalog_menu` owner. The parallel TUI plan consumes `xfx::provider::model::{ModelSelector, ModelRequest, ModelOutcome, CatalogState, CatalogEntry, MAX_RENDERED_MODELS}` and `xfx::provider::{ProviderId, PROVIDERS, ProviderEntry, Bundle}`.
- **`auth_refreshable = true`.** No refreshable source exists in this build. Landing one flips the field and rewrites `UPSTREAM.md` deviation #5 in the same change.
