# fx auth/provider subsystem — evidence base for xfx provider port

Upstream: vercel-labs/fx (Zig), local clone `fx-src`, HEAD `ef1d0d0`. All `file:line` are relative to
`fx-src/`. `[추정]` marks inference; everything else is a read line.

## 0. Shape of the system (high level)

fx has THREE providers behind one `ProviderId` enum (`gateway`, `codex`, `grok`) — `src/core/config/model_provider.zig:4`.
Each provider is a `provider_set.Bundle` (`src/core/gateway/provider_set.zig:14`) wiring together: an auth strategy,
an agent stream transport, a model-catalog fetcher, a CLI-catalog fetcher, and a permission reviewer. The native set
is assembled statically in `src/builtins/providers.zig:11` (`native = Set{ .gateway, .codex, .grok }`).

Two axes that are kept *separate*:
- **ProviderId** (which transport/route) — `gateway | codex | grok`.
- **CredentialSource** (which credential) — `vercel_oidc_token, ai_gateway_api_key, fx_login, stored_key,
  chatgpt_subscription, grok_subscription` (enum in `src/core/shared/types.zig`, used everywhere).
  Mapping rule: `model_provider.authorizesCredential(provider, source)` — gateway accepts anything that is NOT a
  subscription; codex requires `chatgpt_subscription`; grok requires `grok_subscription`
  (`src/core/config/model_provider.zig:22-29`).

OAuth machinery is shared: `oauth_transport.Provider` is a single injected HTTP function
(`src/core/auth/oauth_transport.zig:54`, method enum `get|post_form|post_json`), so Codex/Grok/Vercel all issue
token calls through the same seam (native impl = `executeOAuthRequest` in `src/builtins/gateway.zig`, ~line 730).

---

## 1. OAuth flows (implementable protocol specs)

Both Codex and Grok use a **browser + loopback authorization-code + PKCE(S256)** flow. This reuses the *device-flow*
runtime (`login_flow.SignInRuntime`) but with `device_code`/`user_code` set to empty strings and `verification_uri`
carrying the full authorization URL — the poll callback actually listens on a loopback socket, it does not device-poll
(`src/core/auth/chatgpt_oauth.zig:155-172`, `:214-271`).

### 1a. Codex (ChatGPT subscription) — `src/core/auth/chatgpt_oauth.zig`

Constants (`:14-24`):
- `client_id = "app_EMoamEEZ73f0CkXaXp7hrann"`
- issuer `https://auth.openai.com`; authorize `{issuer}/oauth/authorize` (`:142`); token `https://auth.openai.com/oauth/token`
- scope = `"openid profile email offline_access api.connectors.read api.connectors.invoke"`
- loopback callback ports tried in order: **1455, then 1457** (`:21`, `bindBrowserCallback` `:186`); redirect_uri =
  `http://localhost:{port}/auth/callback` (`:109-113`)
- login timeout 5 min; callback poll 100ms; socket rcv/snd timeout 30s.

**Authorization URL** (`buildBrowserAuthorizationUrl` `:761-783`), query params:
`response_type=code`, `client_id`, `redirect_uri`, `scope`, `code_challenge` (base64url-nopad SHA256 of a 32-byte
random verifier, `:205-212`), `code_challenge_method=S256`, `id_token_add_organizations=true`,
`codex_cli_simplified_flow=true`, `state` (32-byte random), `originator=fx`.

**Callback**: local HTTP server reads the `GET /auth/callback?...` line (`:292-315`), requires exact path prefix and
`state` match (`:785-801`), extracts `code`.

**Token exchange** (`exchangeAuthorizationCodeForRedirectWithBounds` `:572-590`): POST form
(`application/x-www-form-urlencoded`) to token endpoint with `grant_type=authorization_code`, `client_id`, `code`,
`code_verifier`, `redirect_uri`. Response must be JSON with `access_token`, `refresh_token`, `expires_in>0`
(`:611-623`). `token_type` forced to `"Bearer"`, `scope` stored empty.

**Account id / "eligible subscription" check**: NOT a separate API call. The account id is extracted from the JWT
access_token payload claim `https://api.openai.com/auth` → `chatgpt_account_id` (`extractAccountId` `:704-726`,
constant `:19`). If the token has no such claim it errors — that is the eligibility gate. On each request the same
account id is sent as header `chatgpt-account-id` (see §4).

**Refresh** (`refreshSession` `:454-499`): POST **JSON** (not form) to token endpoint:
`{"client_id":..,"grant_type":"refresh_token","refresh_token":..}`. Accepts a response that omits `refresh_token`
(keeps old) and omits `expires_in` (then derives expiry from the new JWT's `exp` claim, `:550-570`). Refresh REJECTS
if the new access token's account id differs from the stored one → `error.ChatGptAccountChanged` (`:472`).

### 1b. Grok (xAI subscription) — `src/core/auth/grok_oauth.zig`

Constants (`:14-26`):
- `client_id = "b1a00492-073a-47ea-816f-4c329264a828"`
- issuer `https://auth.x.ai`; authorize `{issuer}/oauth2/authorize` (`:146`); token `https://auth.x.ai/oauth2/token`;
  userinfo `https://auth.x.ai/oauth2/userinfo`; revoke `https://auth.x.ai/oauth2/revoke`
- scope = `"openid profile email offline_access grok-cli:access api:access"`
- loopback callback port is **ephemeral** (bind to port 0, `bindBrowserCallback` `:185-188`); redirect_uri =
  `http://127.0.0.1:{port}/callback` (`:112-115`); callback path prefix `/callback?` (`:768`).

**Authorization URL** (`:741-761`): `response_type=code`, `client_id`, `redirect_uri`, `scope`, `code_challenge`,
`code_challenge_method=S256`, `state`, `referrer=fx` (note: `referrer`, vs Codex's `originator`; no `nonce`).

**Token exchange** (`:563-581`): POST **form** with `grant_type=authorization_code`, `client_id`, `code`,
`code_verifier`, `redirect_uri`. Requires `access_token`, `refresh_token`, `expires_in>0`.

**Account id / eligibility**: separate authenticated call to `userinfo_url` with `Authorization: Bearer {access}`,
takes `sub` (`fetchAccountId` `:652-677`). `sub` must pass `validAccountId` (non-empty, ≤1024 bytes, bytes in
0x21..0x7e — HTTP-header-safe, `grok_session.zig:22-28`). That authenticated userinfo success IS the eligibility gate.

**Refresh** (`:468-512`): POST **form** `grant_type=refresh_token&client_id=..&refresh_token=..`. Accepts omitted
rotated refresh token; REQUIRES `expires_in` (unlike Codex, errors if absent, `:496`). Re-fetches account id and
errors on mismatch (`error.GrokAccountChanged`).

**Revoke** (`revokeToken` `:679-692`): POST form `token={refresh}&client_id=..` to revoke endpoint. Called on logout.

### 1c. Vercel/Gateway login (for contrast) — `src/core/auth/login_flow.zig`, `oauth.zig`

This is a real **RFC-8628 device-authorization** flow (not loopback): OIDC discovery at
`{issuer}/.well-known/openid-configuration` (`oauth.zig:108`), `default_scope="openid offline_access"`
(`oauth.zig:11`), device-code poll. client_id from env `FX_OAUTH_CLIENT_ID` else default
`cl_zzh5hiOZbwJ9bfqEcYqPIJv3TaPaEYL0` (`oauth_session.zig:14-15`). Not needed for the subscription port but it is the
model the shared `SignInRuntime` was built for.

---

## 2. Session storage

Three separate on-disk stores under `~/.fx/` (`src/core/shared/profile_paths.zig:5-8`):
`auth.json` (Vercel fx-login), `chatgpt-auth.json` (Codex), `grok-auth.json` (Grok). Root dir `.fx`.

### Codex / Grok session files (`chatgpt_session.zig`, `grok_session.zig` — near-identical)
- JSON schema v1: `{"version":1,"access_token","refresh_token","expires_at_ms","account_id"}` (stringify `:220-231`).
  Grok additionally validates `account_id` is header-safe on both parse and stringify (`grok_session.zig:219,230`).
- **Permissions**: file must be a regular file with mode `& 0o077 == 0` (i.e. 0600-ish, no group/other bits) or it is
  rejected on load (`chatgpt_session.zig:116-119`). The `.fx` dir itself is forced to `0700`
  (`openExistingPrivateFxDir` `:184-193`).
- Writes go through a **timed advisory lock** (`chatgpt-auth.lock` / `grok-auth.lock`, 2000ms deadline, `:12-13`) and
  `durableReplaceVerified` (atomic temp+rename+fsync).
- **Refresh lifecycle**: `expires_at_ms` with a 60s safety skew (`refreshDeadlineMs` `:18-20`); `loadAccess(mode)` with
  `stored|if_needed|force` — `if_needed`/`force` take the lock, refresh if expired, save back (`chatgpt_oauth.zig:420-440`).
- **Logout**: delete the file under lock. Codex `logout()` (`chatgpt_oauth.zig:408`) returns `deleted|missing|deleted_not_durable`.
  Grok `logout()` (`grok_oauth.zig:408-426`) additionally attempts remote `revokeToken` first and reports
  `revocation_failed`.

### macOS Keychain — applies ONLY to the Vercel fx-login session, NOT to Codex/Grok
This is the load-bearing finding for the CHANGELOG 0.0.5 line "Store native fx login sessions in Keychain". The
Keychain path lives entirely in `src/core/auth/oauth_session.zig` (the Vercel session). `chatgpt_session.zig` and
`grok_session.zig` are **profile-file only on every platform**.
- Backend selection: `selectStorageBackend(os, disabled)` → `macos_keychain` iff macOS and not disabled, else
  `profile_file` (`oauth_session.zig:82-89`). Disable via `FX_DISABLE_KEYCHAIN=1|true`
  (`native_keychain.zig:38-40`).
- Keychain item = generic password via `/usr/bin/security`, service `FX_OAUTH_SESSION_V1`, account = `$USER` (or the
  OS passwd name) (`native_keychain.zig:9,104-139`). (The AI-Gateway API key uses service `FX_AI_GATEWAY_API_KEY`.)
- **Migration** state machine (`selectResolution` `:91-100`, table test `:990-1013`): a valid profile file stays
  authoritative until the Keychain copy is written AND verified (round-trip re-read equality,
  `publishAndVerifyKeychain` `:306-316`) AND the file is durably deleted (`publishAndCleanup` `:438-461`). If Keychain
  is unavailable it defers (`file_defer_migration`) and keeps using the file. Non-macOS / disabled → always file.
- Logout on macOS deletes both Keychain item and any leftover file (`delete` `:363-387`).

### WASM host
Sessions go through a `js_host_auth.SessionStore` with optimistic-concurrency revisions (`HostMutation` `:464-522`); no
filesystem. Codex/Grok return `error.*OAuthUnavailable` on WASM (`chatgpt_session.zig:133`, load returns null `:78`).

### /logout semantics (`app_commands`/`app_auth_runtime`)
`/logout [vercel|codex|grok]`; provider resolved by `auth_transition.decideLogoutProvider` (`auth_transition.zig:40-54`)
— explicit arg wins, else the active/selected provider, else the sole logged-in subscription, else gateway. After a
subscription logout the runtime reconciles the active credential back down the precedence order
(`reconcileAfterChatGptLogout`/`GrokLogout` `auth_runtime.zig:1482-1504`).

---

## 3. Provider abstraction

### The catalog TYPE (what a provider advertises)
Two catalog concepts:
1. **Static provider identity** — `provider_catalog.Entry` (`src/core/auth/provider_catalog.zig:4-40`): `{id, slug,
   aliases, name, route_name, description, subscription:bool}`. Drives `/setup` labels and the `login/logout` arg
   parser (`parse("codex")`, `parse("grok")`; note `"chatgpt"`/`"openai-codex"` deliberately do NOT parse `:64-65`).
2. **Fetched model catalog** — `model_catalog.ModelCatalogEntry` (`src/core/gateway/model_catalog.zig:277`): per-model
   `{id, model_type, has_tool_use, has_reasoning, reasoning_efforts:[]ReasoningEffort, supports_fast_mode, has_vision,
   has_file_input, has_implicit_caching, context_window, max_tokens}`. This is what `/model` renders (context window +
   effort levels come from here — CHANGELOG "Show provider-advertised models, context windows, and effort levels").

**Effort levels** are per-model `[]ReasoningEffort` inside the catalog entry, parsed from the provider's own metadata
(Codex `supported_reasoning_levels[].effort` `openai_codex_models.zig:239-255`; Grok `reasoning_efforts[].value`
`xai_grok_models.zig:293-313`, which rejects the default/duplicate efforts).

### The provider BUNDLE (how routes differ)
`provider_set.Bundle` (`provider_set.zig:14-45`) fields per provider: `auth_strategy (vercel|chatgpt|grok)`,
`agent_stream: stream_provider.Provider`, `model_catalog: model_catalog.Provider`,
`cli_model_catalog`, `permission_reviewer`, `credits`, `fx_search`, `capabilities{fx_search, vision_fallback,
deferred_usage}`, `presentation: *provider_catalog.Entry`. `Set.select(providerId)` returns the right bundle
(`:56-62`). The app dispatches through it: `agentStreamProvider()` / `creditsProvider()` /
`fetchProviderCatalog()` all do `providerSet().select(active_provider).<field>` (`src/main.zig:428-446`).

Transports differ, and that is exactly where the abstraction bottoms out into three hand-written wire modules:
- gateway → `src/builtins/gateway.zig` (Vercel AI Gateway wire, `vercel_protocol.zig`), chat url
  `https://ai-gateway.vercel.sh/v3/ai/language-model` (`gateway.zig:44`), catalog `GET {base}/coding-agent/v1/models`.
- codex → `src/gateway/openai_codex.zig` (OpenAI **Responses API** SSE).
- grok → `src/gateway/xai_grok.zig` (xAI Responses-shaped SSE via `cli-chat-proxy.grok.com`).
  Codex+Grok share the Responses serialization/reduction in `src/gateway/responses_protocol.zig`.

### Active provider+model selection & persistence
- Runtime holds `provider_selection: provider_runtime.Runtime{active_provider, model}` (`provider_runtime.zig:7-57`,
  default `.gateway`). `adoptOwned(provider, model)` is the no-fail publication boundary.
- Persisted in `~/.fx/settings.json`: top-level `provider` (tag string) + a `models` object keyed by provider tag
  (`gateway|codex|grok`) → model id (`settings_store.zig:1409`, patch `:933`). Legacy keys `model|codex_model|grok_model`
  are migrated away (`:1410`). Per-provider remembered models also held in `model_preferences.Preferences` (a
  provider-indexed array, `model_preferences.zig`).
- `credential_source` is *also* persisted separately (a remembered source that wins precedence, `credentials.zig:305`).

### What switching in /setup does (`switchProvider` `app_auth_runtime.zig` ~760-960)
1. `auth_transition.decideProviderSwitch` gate — no_change / busy (stream or queued work) / prepare (`auth_transition.zig:25-31`).
2. `credentials.resolveForProvider(target)` — for codex/grok loads the subscription credential directly; if absent AND
   `allow_login`, kicks the OAuth sign-in for that provider (`beginCodexSignInForProviderSwitch`).
3. `authorizesCredential(target, source)` guard.
4. `fetchProviderCatalog(target, access)` → must be non-empty & valid.
5. Pick model: `selectCatalogModel(catalog, current_model, preferred=saved||FX_MODEL)`.
6. Publish atomically: `model_cache.adoptOwnedCatalog`, `provider_selection.adoptOwned(target, model)`,
   `auth.adoptCredential`, then persist `{provider, model}` via `persistRuntimePreferences`.

`/setup` menu choices (`picker_presentation.zig:259-281`, `app_auth_runtime.zig:279-303`): Sign in with Vercel /
Codex / Grok, API key setup, Switch provider, Switch credential, Change team. `/provider` slash command was REMOVED in
0.0.5 (folded into `/setup`; top-level `fx provider` CLI kept — CHANGELOG "Interactive provider switching").

---

## 4. Request paths (a normal turn)

Common `stream_provider.Provider{stream_fn}` seam (`src/core/agent/stream_provider.zig`); each provider's `streamCompletion`
guards the credential source before any I/O (Codex: `error.CodexSubscriptionCredentialRequired` if source ≠
chatgpt_subscription, `openai_codex.zig:137`; Grok requires source + account id, `xai_grok.zig` streamCompletion).

### Codex request (`openai_codex.zig:46-108`)
- Endpoint `POST https://chatgpt.com/backend-api/codex/responses` (SSE).
- System prompt: all `role==.system` messages concatenated into top-level `"instructions"` (default "You are a helpful
  assistant."). Body: `{"model", "store":false, "stream":true, "instructions", "input":[…Responses items…], tools,
  "tool_choice", "parallel_tool_calls":true, "include":["reasoning.encrypted_content"], "text":{"verbosity":"low"}[,
  format], reasoning:{effort,summary:"auto"}}`. `service_tier:"priority"` when Fast mode (`:84`). "minimal" effort is
  mapped to "low" (`:100`). Deliberately omits `max_output_tokens` (ChatGPT endpoint rejects it, `:105`).
- Headers (`streamPrepared:202-217`): `Authorization: Bearer {access}`, `chatgpt-account-id: {jwt account}`,
  `originator: fx`, `OpenAI-Beta: responses=experimental`, `accept: text/event-stream`, and when a session id exists
  `session-id` + `x-client-request-id`.

### Grok request (`xai_grok.zig` buildRequest + streamPrepared)
- Endpoint `POST https://cli-chat-proxy.grok.com/v1/responses` (SSE). Same Responses body shape as Codex, but KEEPS
  `max_output_tokens` (`buildRequest` last field) and always sends `reasoning.summary:"auto"` when an effort is set.
- Headers: `Authorization: Bearer {access}`, `accept: text/event-stream`, `X-XAI-Token-Auth: xai-grok-cli`,
  `x-authenticateresponse: authenticate-response`, `x-grok-client-version: 1.0.6`, `x-grok-client-identifier: fx`,
  `x-grok-model-override: {model}`, `x-grok-user-id: {account sub}`, and `x-grok-conv-id: {session}` when present.
- Grok also honors a per-request `deadline` → `error.Timeout`.

### Per-provider limits (CHANGELOG "Reject oversized Codex and Grok catalogs, streams, tool data, replay state")
- Catalog fetch bounds: Codex `max_catalog_models=128`, `max_catalog_bytes=4MiB`, requires reviewer model
  `gpt-5.4-mini` present or rejects (`openai_codex_models.zig:10-18,109-119`); URL carries
  `client_version=0.148.0` (`:17,175-186`). Grok `max_catalog_models=128`, `max_catalog_bytes=1MiB`,
  `max_model_id_bytes=256`, joins two endpoints (`/v1/models` subscription caps + `api.x.ai/v1/language-models`
  modalities), keeps only `api_backend=="responses"` with a text output modality (`xai_grok_models.zig:11-18,251-289`).
- Stream bounds (both, `CodexLimits`/consts `openai_codex.zig:15-33`, mirrored in `xai_grok.zig:17-26`):
  `max_sse_line_bytes` (Codex 32MiB / Grok 1MiB), `max_sse_aggregate_bytes=64MiB`, `max_sse_events=100_000`,
  `max_tool_calls=128`, `max_tool_identity_bytes=1024`, `max_tool_arguments_bytes=4MiB`,
  `max_provider_state_bytes=4MiB` (encrypted reasoning replay state), `max_error_body_bytes` (Codex 1MiB/Grok 256KiB).
  Overflows are terminal typed errors, not silent truncation.

---

## 5. Credential precedence (how subscription auth coexists with Gateway key)

Single resolver `credentials.resolvePreferring` (`credentials.zig:309-355`). Order:
1. `preferred` source (the persisted `credential_source`) — explicit user choice wins, but a subscription source is
   NOT allowed as the gateway "preferred" (stripped `:301`).
2. `vercel_oidc_token` env → 3. `ai_gateway_api_key` env → 4. `fx_login` session → 5. `stored_key` (Keychain/file).
   A login that fails to load/refresh is one silent source, not fatal — falls through to the stored key.
The base `credential_source_order` incl. subscriptions is `[vercel_oidc_token, ai_gateway_api_key, fx_login,
stored_key, chatgpt_subscription, grok_subscription]` (`auth_runtime.zig:26-33`) — subscriptions sit LAST and are
never auto-selected by gateway precedence (skipped explicitly in the reselect loops `:1470`, `:1532`).

Provider-scoped resolution bypasses precedence entirely: `resolveForProvider(.codex)` loads only the ChatGPT
credential, `.grok` only the Grok credential (`credentials.zig:271-303`). So: subscription providers use ONLY their own
subscription token; gateway uses env/login/stored-key precedence. There is no fallback from a subscription to the
Gateway key within the same provider — switching providers is the only path (CHANGELOG "Credential fallback: continue
to a stored API key when saved fx login credentials cannot load" is about the gateway chain, `:331-354`).

Fallback-safe catalog access is refused for subscriptions: `publicFallbackAfterRejection` returns null for
chatgpt/grok (`credentials.zig:91-102`) — a rejected subscription catalog does NOT silently downgrade to anonymous.

---

## 6. LLMUX FIT (concrete proposal for the Rust port)

llmux today in xfx = keyless loopback daemon speaking the Anthropic Messages wire. It slots in as a **fourth
ProviderId** in the exact same architecture:

- **ProviderId**: add `Llmux` alongside `Gateway|Codex|Grok`. Add a `provider_catalog.Entry` `{slug:"llmux",
  subscription:false, name:"llmux", description:"local keyless daemon"}` so `/setup` and `/model` list it.
- **Auth story = keyless loopback**: this is llmux's "credential". Model it as a `CredentialSource::LlmuxLoopback`
  that resolves trivially (daemon reachable ⇒ present) — no OAuth, no token file. `authorizesCredential(Llmux, src)`
  returns true only for that source. It behaves like `ai_gateway_api_key` in precedence (an env-detected credential
  needing no login), so it can sit in the gateway-style precedence chain OR be provider-scoped like the subscriptions.
  Because there is no browser/loopback OAuth, you SKIP the whole `SignInRuntime`/PKCE path for it — its `/setup` entry
  is just "use local llmux" (analogous to the API-key `.setup` action, not the `.login` action,
  `app_auth_runtime.zig:279-303`).
- **Catalog from `GET /models`**: implement a `model_catalog.Provider.fetch` that hits llmux `GET /models` and maps its
  response (incl. aliases + efforts) into `ModelCatalogEntry{ id, reasoning_efforts, context_window, has_tool_use,
  has_vision,… }` — exactly what Codex/Grok catalog fetchers do (`openai_codex_models.zig:188-237`). Aliases → either
  emit one entry per alias or carry them like `provider_catalog.Entry.aliases`. Efforts → `reasoning_efforts[]`.
- **Transport**: llmux's Anthropic Messages wire becomes its own `stream_provider` impl (a 4th sibling to
  gateway/codex/grok wire modules), reusing the SSE/limit discipline. It does NOT share `responses_protocol` (that's
  OpenAI Responses shape) — it's the Anthropic Messages shape, which xfx already has.

**Impedance mismatches to flag:**
- fx's provider abstraction assumes a *bearer credential string* threaded into every request
  (`request.credential.secret`, `catalogAccessForCredential`). llmux is keyless — you'll pass an empty/sentinel
  credential and must ensure the `authorizesCredential` + `catalogAccess` guards don't reject "no credential". Cleanest
  is a `CatalogAccess` variant that is authenticated-but-tokenless, or treat "daemon up" as the credential.
- fx keys persistence (`settings.json models{}`, `credential_source`) by the provider *tag string*; adding `llmux`
  means bumping the enum everywhere it's exhaustively matched (`provider_set.Set.select`, `authorizesCredential`,
  `decideLogoutProvider`, settings validation `settings_store.zig:1654`). All are `switch` over a 3-variant enum today.
- llmux advertises Anthropic-style effort/thinking, not OpenAI `reasoning.effort`/xAI `reasoning_efforts` — the
  `ReasoningEffort` enum + its `max_options` bound must cover llmux's set, and the request shaper must translate to
  the Messages `thinking` field rather than a `reasoning` field.
- No account-id concept (Codex extracts it from JWT, Grok from userinfo). `credential_authority.derive` needs a stable
  identity for llmux — use a fixed slot like the gateway sources do (`credential_authority.zig:23-28`), not an account.

---

## 7. MINIMUM VIABLE SLICE (ranked, with risk)

Port order that gets a usable multi-provider xfx fastest:

1. **Provider enum + Bundle/Set indirection + persistence** (`ProviderId`, `provider_set.Set.select`,
   `provider_runtime.Runtime`, settings `provider`+`models{}`). Pure plumbing, no network. Unlocks everything else.
   Add `Gateway` + `Llmux` first (both keyless-ish), since xfx already has both transports — you get provider switching
   with zero OAuth.
2. **`/setup` switching + `/model` per-provider catalogs** wired to a `model_catalog.Provider` trait. For Gateway use
   the existing key; for llmux use `GET /models`. This is the visible payoff and needs no OAuth.
3. **Codex OAuth** (`chatgpt_oauth.zig` port): browser+loopback+PKCE, JSON refresh, JWT account-id, profile-file
   session (`chatgpt-auth.json`, 0600). Highest-value subscription because the flow is self-contained.
4. **Grok OAuth**: same shape, form refresh, userinfo account-id, revoke-on-logout. Cheap once Codex is done — the two
   modules are ~90% identical.
5. **DEFER macOS Keychain** — it only covers the *Vercel fx-login* session, not Codex/Grok (which are always profile
   files). If xfx doesn't do Vercel device-login, Keychain buys nothing yet. Profile-file 0600 storage is the whole
   subscription persistence story. (`oauth_session.zig` Keychain machinery is ~450 lines of migration state — skip.)

**Risk — OAuth client ids usable by a third-party port?** The client ids are hardcoded and public in the binary
(`app_EMoamEEZ73f0CkXaXp7hrann` for Codex `chatgpt_oauth.zig:14`; `b1a00492-…` for Grok `grok_oauth.zig:14`). What
identifies the client to the provider beyond the id: Codex sends `originator=fx` (authorize param + request header)
and `codex_cli_simplified_flow=true`; Grok sends `referrer=fx` + header `x-grok-client-identifier: fx` +
`X-XAI-Token-Auth: xai-grok-cli`. These are fx's identity — reusing fx's client_id means impersonating fx to OpenAI/xAI.
No client secret is used (public PKCE clients), so the port *can* technically reuse them, but:
(a) it ties xfx to fx's registered redirect URIs (fixed ports 1455/1457 for Codex; ephemeral loopback for Grok — Grok
is more forgiving), and (b) it presents as "fx" to the provider, which is an ownership/ToS concern, not a technical
blocker. `[추정]` Registering xfx's own OAuth clients with OpenAI/xAI is the correct long-term path; for a first slice,
reusing the ids works but should be flagged to the user as a policy decision, not a silent default.
