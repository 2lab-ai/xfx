# xfx — Provider architecture

Status: **half shipped, half a target, and the line between them is MVS steps 2 and 3 below.**

**Shipped**, and therefore current-tense in `docs/parity.md` rather than a promise here: provider
*identity* (`ProviderId::{Gateway, Llmux}` with a per-provider bundle selected per turn), the
profile-only `provider` + `models{}` persistence and its migration from `backend`/`llmux_url`,
`xfx setup <provider>` and the `/setup` slash command on both front ends, the keyless-loopback
credential resolved from configuration alone, and the fetched **model catalog** with `/model`
browsing it and selecting from it — including the refusal of an id the loaded catalog does not
publish, on both front ends. That is MVS steps 1 and 2, and option **D** of §Policy risks.

**Still a target**, and absent from the binary rather than stubbed: Codex and Grok as providers of
their own. There is no ChatGPT or xAI subscription credential, no OAuth of any kind, no Responses
wire, and no `login`/`logout` — `docs/parity.md` records `login`, `logout`,
`Codex / ChatGPT subscription`, `fx login credential` and `stored API key credential` as deferred,
and they stay deferred until the registration question in §Policy risks is answered. **The current
user-facing route to a `gpt-` or `grok-` model is a llmux daemon that serves it**: those ids arrive
in llmux's own catalog and are selected like any other, which is one backend's catalog naming them
rather than a provider inside xfx.

**How to read the rest of this document.** Every section is marked below with which side of that
line it is on. The **target** sections are kept in full rather than trimmed to what shipped: they
are the design the shipped half was built against, and the next epic's specification. Nothing here
outranks `docs/parity.md`, which describes the binary.

| Section | Status |
|---|---|
| §Shape | **target**, except the two-axis separation and the `Bundle`, which shipped as `ProviderId::{Gateway, Llmux}` |
| §Credential sources and precedence | **shipped** for the Gateway's two variables and `llmux_loopback`; **target** for the two subscription rows and the persisted preference |
| §Codex, §Grok, §The response side, §Session files | **target**. No Responses wire and no OAuth exist in the binary |
| §llmux as the fourth provider | **shipped**, as the second: identity, keyless credential, `GET /models` catalog with aliases and efforts, Anthropic Messages transport |
| §`/setup` and `/model` | **shipped** for switching, the transaction, the persistence and the catalog browser; **target** for the sign-in/credential/team menu choices and `/logout` |
| §Profile migration | **shipped** |
| §MVS order | steps 1–2 **shipped**; 3–5 **target** |
| §Policy risks | **open**, and it is the gate on step 3 |

Evidence base: [`research/auth-providers.md`](research/auth-providers.md), read against upstream `fx`
at **HEAD `ef1d0d0`** — later than xfx's pin `580a0c5d`. Every `file:line` is upstream-relative and
comes from that note; `[추정]` marks are preserved. Upstream coordinates are **not** re-verified by
the shipped half: what shipped is described by `docs/parity.md` and by the code.

## Shape

> **Target**, with one part shipped: xfx has `ProviderId::{Gateway, Llmux}` and a
> `Bundle` selected per turn. `Codex` and `Grok` are not variants of that enum in the binary, and
> `CredentialSource`'s two subscription rows do not exist.

Upstream keeps **two axes separate**, and copying that separation is most of the design:

- **`ProviderId`** — which transport and route: `gateway | codex | grok`
  (`src/core/config/model_provider.zig:4`).
- **`CredentialSource`** — which credential: `vercel_oidc_token, ai_gateway_api_key, fx_login,
  stored_key, chatgpt_subscription, grok_subscription` (`src/core/shared/types.zig`).

They are joined by one rule, `authorizesCredential(provider, source)`: gateway accepts anything that
is **not** a subscription; codex requires `chatgpt_subscription`; grok requires `grok_subscription`
(`model_provider.zig:22-29`).

Each provider is a **`provider_set.Bundle`** (`provider_set.zig:14-45`) wiring an auth strategy, an
agent stream transport, a model-catalog fetcher, a CLI-catalog fetcher, a permission reviewer, a
credits surface, capability flags, and a `provider_catalog.Entry` for presentation. The native set is
assembled statically (`src/builtins/providers.zig:11`), and every dispatch is
`providerSet().select(active).<field>` (`src/main.zig:428-446`).

For xfx that is a `ProviderId` enum plus a bundle of trait objects, selected once per turn; the
existing `gateway::Provider` trait becomes the bundle's stream field, so the boundary xfx already has
does not change — it gains siblings.

```
ProviderId { Gateway, Llmux, Codex, Grok }
  └─ Bundle { auth, stream: Box<dyn Provider>,   // v0.1's gateway::Provider trait
              catalog: Box<dyn ModelCatalog>, presentation: &'static ProviderEntry }
```

Two catalog concepts, also worth keeping separate:

1. **Static identity** — `provider_catalog.Entry {id, slug, aliases, name, route_name, description,
   subscription: bool}` (`provider_catalog.zig:4-40`). Drives `/setup` labels and the `login`/`logout`
   argument parser. Note that `"chatgpt"` and `"openai-codex"` deliberately do **not** parse (`:64-65`).
2. **Fetched model catalog** — `ModelCatalogEntry {id, model_type, has_tool_use, has_reasoning,
   reasoning_efforts[], supports_fast_mode, has_vision, has_file_input, has_implicit_caching,
   context_window, max_tokens}` (`model_catalog.zig:277`). This is what `/model` renders; context
   windows and effort levels come from here, per provider metadata (Codex
   `openai_codex_models.zig:239-255`, Grok `xai_grok_models.zig:293-313`).

OAuth machinery is shared through one injected HTTP function, `oauth_transport.Provider`
(`oauth_transport.zig:54`, methods `get|post_form|post_json`) — the seam that makes both flows
testable without a browser, and what keeps Codex and Grok from duplicating a transport.

## Credential sources and precedence

> **Target** for the precedence chain and the persisted preference. What ships is the three rows of
> the table below that are not subscriptions: `VERCEL_OIDC_TOKEN`, `AI_GATEWAY_API_KEY` and
> `llmux_loopback`, resolved provider-scoped with no fallback between providers and no I/O.

One resolver, `credentials.resolvePreferring` (`credentials.zig:309-355`), in order:

1. the persisted `credential_source` preference — an explicit user choice wins, but a **subscription
   source is never allowed as the gateway preference** (stripped, `:301`);
2. `VERCEL_OIDC_TOKEN` (env) → 3. `AI_GATEWAY_API_KEY` (env) → 4. `fx_login` session → 5. `stored_key`.

A login credential that fails to load or refresh is **one silent source, not a fatal error**: it
falls through to the stored key. Subscriptions sit last in the base order
(`auth_runtime.zig:26-33`) and are explicitly skipped by the gateway reselect loops (`:1470`, `:1532`)
— they are never auto-selected.

**Provider-scoped resolution bypasses precedence entirely**: `resolveForProvider(.codex)` loads only
the ChatGPT credential, `.grok` only the Grok one (`credentials.zig:271-303`). There is no fallback
from a subscription down to the Gateway key within one provider; switching providers is the only
path. A rejected subscription catalog does **not** silently downgrade to an anonymous fetch
(`publicFallbackAfterRejection` returns null for both, `:91-102`).

xfx's mapping, keeping today's honesty properties:

| xfx source | Resolves when | `status` reports |
|---|---|---|
| `vercel_oidc_token` | env nonblank | source name only, never the secret |
| `ai_gateway_api_key` | env nonblank | source name only |
| `llmux_loopback` | a loopback `llmux_url` is configured and readable — **not** that the daemon answered | `llmux-keyless-loopback` (already shipped) |
| `chatgpt_subscription` | `~/.xfx/codex-auth.json` parses and is `0600` | source name; `auth_refreshable = true` |
| `grok_subscription` | `~/.xfx/grok-auth.json` parses and is `0600` | source name; `auth_refreshable = true` |

**Credential *presence* is a configuration fact; *reachability* is a network fact, and they must not
merge.** `status` performs no I/O today and must keep performing none, so on the llmux backend it
reports `auth=llmux-keyless-loopback` from the configuration alone. `doctor` likewise "does no network
I/O ... and never probes the daemon" (`docs/parity.md`, `doctor` row) — a daemon that is down is
therefore *not* a failing `auth` check; it surfaces where a network call already legitimately happens:
`xfx setup llmux`'s ping and catalog proof, and a `/model` catalog load. Adding subscription providers
does not change this: a session file that parses and is `0600` makes the credential *present*, and
whether the token still refreshes is discovered by the catalog fetch, not by `status`. Keeping the
split is what lets both commands stay "always safe to run".

`auth_refreshable` is `false` in v0.1.0 only because no refreshable source exists
(`UPSTREAM.md` deviation #5). Landing either subscription flips that field, and the deviation note
must be rewritten in the same change — the ledger is the contract, not the code comment.

## Codex (ChatGPT subscription) — protocol spec

> **Target. None of this is in the binary**: no OAuth, no token store, no Responses transport.

Upstream: `src/core/auth/chatgpt_oauth.zig`. Browser + loopback authorization-code + PKCE(S256). It
reuses the device-flow runtime with empty `device_code`/`user_code` and the authorization URL in
`verification_uri`; the "poll" callback actually listens on a loopback socket (`:155-172`, `:214-271`).

- **Constants** (`:14-24`): `client_id = app_EMoamEEZ73f0CkXaXp7hrann`; issuer
  `https://auth.openai.com`; authorize `{issuer}/oauth/authorize` (`:142`); token
  `https://auth.openai.com/oauth/token`; scope
  `openid profile email offline_access api.connectors.read api.connectors.invoke`; callback ports
  tried in order **1455 then 1457** (`:21`, `:186`) — the note records the registered range as
  1455–1457; redirect `http://localhost:{port}/auth/callback` (`:109-113`). Login timeout 5 min,
  callback poll 100 ms, socket send/receive timeout 30 s.
- **Authorization URL** (`:761-783`): `response_type=code`, `client_id`, `redirect_uri`, `scope`,
  `code_challenge` (base64url-nopad SHA-256 of a 32-byte random verifier, `:205-212`),
  `code_challenge_method=S256`, `id_token_add_organizations=true`, `codex_cli_simplified_flow=true`,
  `state` (32 random bytes), `originator=fx`.
- **Callback**: a local HTTP server reads the `GET /auth/callback?...` request line (`:292-315`),
  requires the exact path prefix **and** a `state` match (`:785-801`), extracts `code`.
- **Token exchange** (`:572-590`): POST `application/x-www-form-urlencoded` with
  `grant_type=authorization_code`, `client_id`, `code`, `code_verifier`, `redirect_uri`. The response
  must be JSON carrying `access_token`, `refresh_token`, `expires_in > 0` (`:611-623`); `token_type` is
  forced to `Bearer` and `scope` stored empty.
- **Eligibility is a JWT claim, not an API call.** The account id is the `chatgpt_account_id` field of
  the access token's `https://api.openai.com/auth` claim (`extractAccountId` `:704-726`). A token
  without that claim is an error — **that is the eligible-subscription gate.** The same id is sent on
  every request as `chatgpt-account-id`.
- **Refresh** (`:454-499`) is POST **JSON**, not form:
  `{"client_id":…,"grant_type":"refresh_token","refresh_token":…}`. A response may omit
  `refresh_token` (keep the old one) and may omit `expires_in` (derive expiry from the new JWT's `exp`,
  `:550-570`). Refresh **rejects** if the new token's account id differs from the stored one
  (`error.ChatGptAccountChanged`, `:472`).
- **Request path** (`openai_codex.zig:46-108`): `POST https://chatgpt.com/backend-api/codex/responses`,
  SSE. All `role == system` messages are concatenated into top-level `instructions`. Body carries
  `store:false`, `stream:true`, `input[]` in Responses shape, `tools`, `tool_choice`,
  `parallel_tool_calls:true`, `include:["reasoning.encrypted_content"]`,
  `text:{"verbosity":"low"}`, and `reasoning:{effort, summary:"auto"}`; `service_tier:"priority"` in
  Fast mode (`:84`); `"minimal"` effort maps to `"low"` (`:100`); `max_output_tokens` is deliberately
  **omitted** because the ChatGPT endpoint rejects it (`:105`). Headers (`:202-217`): bearer,
  `chatgpt-account-id`, `originator: fx`, `OpenAI-Beta: responses=experimental`,
  `accept: text/event-stream`, plus `session-id` / `x-client-request-id` when a session exists.
- **Catalog bounds** (`openai_codex_models.zig:10-18,109-119,175-186`): ≤ 128 models, ≤ 4 MiB, a URL
  carrying `client_version`, and a required reviewer model or the fetch is rejected.

## Grok (xAI subscription) — protocol spec

> **Target. None of this is in the binary**, for the same reason Codex's is not.

Upstream: `src/core/auth/grok_oauth.zig`. Same skeleton, four real differences.

- **Constants** (`:14-26`): `client_id = b1a00492-073a-47ea-816f-4c329264a828`; issuer
  `https://auth.x.ai`; authorize `{issuer}/oauth2/authorize`; token `/oauth2/token`; userinfo
  `/oauth2/userinfo`; revoke `/oauth2/revoke`; scope
  `openid profile email offline_access grok-cli:access api:access`. The callback port is
  **ephemeral** (bind port 0, `:185-188`), redirect `http://127.0.0.1:{port}/callback` (`:112-115`).
- **Authorization URL** (`:741-761`): as Codex, but `referrer=fx` instead of `originator`, and no
  `nonce`.
- **Token exchange** (`:563-581`) is POST **form**. **Refresh** (`:468-512`) is also form, and unlike
  Codex it **requires** `expires_in` (`:496`).
- **Eligibility is an authenticated call**: `GET userinfo` with the bearer, take `sub`
  (`fetchAccountId` `:652-677`); `sub` must be non-empty, ≤ 1024 bytes, and byte-range 0x21–0x7e so it
  is HTTP-header-safe (`grok_session.zig:22-28`). A refresh re-fetches it and errors on mismatch.
- **Logout revokes**: POST form `token={refresh}&client_id=…` to the revoke endpoint before deleting
  the session (`:679-692`), and reports `revocation_failed` distinctly.
- **Request path** (`xai_grok.zig`): `POST https://cli-chat-proxy.grok.com/v1/responses`, the same
  Responses body as Codex except it **keeps** `max_output_tokens`. Headers: bearer, accept,
  `X-XAI-Token-Auth`, `x-authenticateresponse`, `x-grok-client-version`, `x-grok-client-identifier`,
  `x-grok-model-override`, `x-grok-user-id` (the userinfo `sub`), and `x-grok-conv-id` per session.
- **Catalog** joins two endpoints (subscription caps + modalities), keeps only `api_backend ==
  "responses"` entries with a text output modality, bounded at 128 models / 1 MiB / 256-byte ids
  (`xai_grok_models.zig:11-18,251-289`).

## The response side — Responses SSE and its normalization

> **Target.** xfx decodes two wires today, the Gateway's and Anthropic Messages; the Responses wire
> arrives with the providers that speak it.

Codex and Grok share one decoder: `src/gateway/responses_protocol.zig`, a `Reducer` fed one decoded
SSE `data:` payload at a time (`applyJson` `:239-380`) and finalized once (`finish` `:385-443`). Each
provider owns only its framing reader and its error vocabulary. This section is the contract xfx has
to reproduce, expressed against xfx's own `DeltaSink` / `Completion` (`src/gateway/mod.rs:151`,
`src/gateway/protocol.rs:549-568`).

### Framing

`SseReader.next` (`openai_codex.zig:350-366`) reads a line at a time and: skips blank lines and
comment lines beginning `:`; skips any line that is not `data:`; trims the payload; and treats
`data: [DONE]` as **end of stream, not as completion**. Lines are bounded — Codex 32 MiB, Grok 1 MiB
(`openai_codex.zig:16`, `xai_grok.zig:18`) — and a fragment that would overflow the pending line is
`SseEventTooLarge`; a stalled read with an empty buffer is `SseReadStalled`
(`openai_codex.zig:368-390`). Grok additionally accumulates **wire** bytes across the whole stream
against a 64 MiB aggregate (`xai_grok.zig:377-384`), where Codex counts JSON payload bytes inside the
reducer (`responses_protocol.zig:249-256`, `StreamLimits.count_json_bytes`). Both cap events at
100 000.

### Event grammar (`responses_protocol.zig:263-380`)

| Event `type` | Reducer action | xfx mapping |
|---|---|---|
| `response.output_item.added` with `item.type == "function_call"` | Register a `ToolAccumulator` keyed by `output_index`, carrying `call_id` and `name`; fire `on_tool_start` **once** (a duplicate `output_index` is ignored) (`:263-277`) | Begin a `ToolCall`; the key is `output_index`, never the array position |
| `response.output_text.delta`, `response.refusal.delta` | Set `saw_content_delta`, fire `on_content`, append to captured content (`:278-284`) | `DeltaSink::text_delta`, and append to `Completion.text` — **a refusal is assistant text**, not an error |
| `response.reasoning_summary_text.delta`, `response.reasoning_text.delta` | Fire `on_reasoning` only (`:285-289`) | Not `text_delta`. v0.1 has no reasoning sink; until the TUI has one, drop it — it is never part of `Completion.text` |
| `response.reasoning_summary_part.done` | Emit `"\n\n"` on the reasoning channel (`:290-291`) | Same channel, same rule |
| `response.function_call_arguments.delta` | Append to that accumulator's arguments, bounded at 4 MiB; fire `on_tool_input` (`:292-297`) | Fragmented tool arguments are assembled by `output_index`, never concatenated blindly across calls |
| `response.function_call_arguments.done` | **Reconciliation, not append**: if the accumulated text is a prefix of the final `arguments`, append only the suffix and stream only the suffix; otherwise **discard the accumulation and take the final value whole** (`:298-310`) | Port verbatim. This is what makes a torn or re-sent argument stream converge instead of doubling |
| `response.output_item.done`, `item.type == "function_call"` | Fill arguments from the item **only if nothing was accumulated** (`:313-322`) | A late-arriving whole value never overwrites a good accumulation |
| `response.output_item.done`, `item.type == "reasoning"` with `encrypted_content` | Stringify the **entire item** and append it to a JSON array being built in `provider_state`, bounded at 4 MiB including separators (`:323-347`) | **This is `Completion.responses_state`** (a *new* field — see Provenance below; it is deliberately **not** `raw_content`, which stays Anthropic-blocks-only). Same role the Anthropic `thinking` block plays on the llmux wire: kept verbatim because it must be sent back unchanged |
| `response.output_item.done`, `item.type == "message"` **and no text delta was seen** | Emit each part's `text` (or `refusal`) as content (`:348-358`) | The non-streaming fallback: a provider that sent the message whole still produces `Completion.text` |
| `response.completed`, `response.done`, `response.incomplete` | Terminal: set `finish_reason` from `response.status`, read `usage`, keep `response.id`, **return true so the reader stops** (`:359-376`) | Sets `FinishReason`, `Usage`; the framing loop breaks here rather than reading to `[DONE]` |
| `response.failed`, `error` | `error.ResponseFailed` immediately (`:377-379`) | A provider failure **delivered inside a 200** — exactly the case xfx already handles on the Anthropic wire (`ProviderError::is_replayable`'s in-band comment, `src/gateway/mod.rs`) |
| anything else, or a non-object payload, or a missing field | Ignored, return false (`:259-261`) | Unknown events are skipped, never fatal — that is what lets the provider add events without breaking the port |

### Finish conditions (`responses_protocol.zig:385-443`)

- **A terminal event is required.** `finish` returns `error.StreamIncomplete` unless `terminal_seen`
  (`:390`). `[DONE]` alone does not complete a stream — the same rule xfx already enforces on both
  its wires ("a canonical `finish` is *required*, because `[DONE]` alone does not prove completion").
- A tool with no arguments at all finalizes as `"{}"`, never as an empty string (`:419-423`).
- `finish_reason` defaults to `tool_calls` when there are tool calls, else `stop` (`:440`).
- `finishReason` mapping (`:508-528`): `completed` → `tool_calls` if any, else `stop`; `incomplete` →
  `length` when `incomplete_details.reason == "max_output_tokens"`, `content_filter` when it is
  `content_filter`, else `provider_error`; `failed`/`cancelled` → `provider_error`; anything
  unrecognized → `tool_calls` if any, else `other`. This maps onto xfx's `FinishReason`
  (`Stop|Length|ContentFilter|ToolCalls|ProviderError|Other`) one-to-one, which is why no new variant
  is needed.
- `usage` is `input_tokens` / `output_tokens` off the terminal event's `response.usage`; a negative
  value is treated as absent (`:530-537,546-550`).
- Cancellation is checked **on entry to every event and again in `finish`** (`:248`, `:389`), which is
  where xfx's `CancelToken` poll goes.

### Provider error taxonomy

Two distinct layers, and conflating them is the mistake to avoid:

1. **HTTP status, before any stream.** A non-200 reads a bounded error body (Codex 1 MiB, Grok
   256 KiB) and returns a typed failure — `failureKind` maps 400 `invalid_request`, 401
   `unauthorized`, 403 `forbidden`, 413 `request_too_large`, 429 `rate_limited`, 500 `server_error`,
   502 `bad_gateway`, 503 `unavailable`, 504 `gateway_timeout`, everything else `provider_error`
   (`openai_codex.zig:265-278,324-338`). Over-long error bodies are replaced by a fixed sentence
   rather than truncated silently. This lands on xfx's `ProviderError::Status { retryable,
   retry_after }`.
2. **In-band failure, inside a 200.** `response.failed` / `error` → `ResponseFailed`. xfx already has
   the doctrine for this from the Anthropic wire: *the transport a failure happened to arrive over
   must not decide how many attempts it is worth.* So an in-band Codex/Grok failure must be classified
   by its own type, not made permanently fatal because it was not a 429.

Every reducer error is renamed into a provider-scoped error before it leaves the module
(`mapReducerError`, `openai_codex.zig:442-450`): `InvalidEvent`, `ResponseFailed`, `StreamIncomplete`,
`ToolCallLimitExceeded`, `ToolArgumentsTooLarge`, `ResourceLimitExceeded`. In xfx these become
`ProviderError::Protocol(SseError)` variants that **name the provider**, so a stream error can never
be reported against the wrong backend — the exact defect the llmux slice already fixed once ("a client
that failed to build on the llmux path blamed the Gateway in its error", `CHANGELOG.md` 0.1.0).

**Delivery certainty**: upstream marks the request `possiblySent` immediately before writing the body
(`openai_codex.zig:258`). xfx's equivalent is `ProviderError::is_replayable`, and the same rule holds
— once bytes are on the wire, a failure is not replayable regardless of how it presented.

### Malformed and oversized

Every limit is a **typed terminal error, never a truncation**: a JSON payload that does not parse is
`InvalidEvent` (`:257`); an event count, aggregate byte count, tool count, tool identity length, tool
argument length, or provider-state length over its bound is a `ResourceLimitExceeded` family error
(`checkedAccumulatedSize` `:483-488`, `appendTool` `:447-470`, `appendToolArguments` `:472-481`).
The one deliberate exception is **captured content**, which is clipped to a capture limit while the
deltas still stream in full (`appendCaptured` `:490-506`) — the user sees everything; only the
retained copy is bounded.

### Wire constants — the mandatory values

Every number a port has to hard-code, with its citation. Absent means **absent upstream**, not
unresearched.

| Constant | Codex | Grok | Evidence |
|---|---|---|---|
| Stream endpoint | `https://chatgpt.com/backend-api/codex/responses` | `https://cli-chat-proxy.grok.com/v1/responses` | `openai_codex.zig:13`, `xai_grok.zig` |
| Max SSE **line** bytes | 32 MiB | 1 MiB | `openai_codex.zig:16`, `xai_grok.zig:18` |
| Max SSE **aggregate** bytes | 64 MiB | 64 MiB | `openai_codex.zig:17`, `xai_grok.zig:19` |
| What "aggregate" counts | decoded **JSON payload** bytes, inside the reducer (`count_json_bytes: true`) | **wire** bytes, in the framing reader, before parsing | `responses_protocol.zig:249-256`; `xai_grok.zig:377-384` |
| Max SSE events | 100 000 | 100 000 | `openai_codex.zig:18`, `xai_grok.zig:20` |
| Max tool calls | 128 | 128 | `openai_codex.zig:19`, `xai_grok.zig:21` |
| Max tool identity bytes (`call_id`, `name`, each) | 1024 | 1024 | `openai_codex.zig:20`, `xai_grok.zig:22`; enforced with a nonzero-length check in `appendTool` `responses_protocol.zig:447-470` |
| Max tool **arguments** bytes (per call) | 4 MiB | 4 MiB | `openai_codex.zig:21`, `xai_grok.zig:23` |
| Max provider-state bytes (encrypted reasoning) | 4 MiB | 4 MiB | `openai_codex.zig:22`, `xai_grok.zig:24` |
| Max error-body bytes | 1 MiB | 256 KiB | `openai_codex.zig:15`, `xai_grok.zig:17` |
| Transfer buffer | 256 KiB | 256 KiB | `openai_codex.zig:23`, `xai_grok.zig:25` |
| Connect timeout | 30 s | 30 s | `openai_codex.zig:24`, `xai_grok.zig:26` |
| Per-request deadline | none | honoured → `error.Timeout` | `xai_grok.zig:135-149,232-257` |
| Model id validation | nonempty, ≤ 1024 bytes, no byte ≤ 0x20 or == 0x7f | same shape | `openai_codex.zig:39-44` |
| **Replay** limits (next request) | tool_calls 128 / identity 1 KiB / arguments 4 MiB / provider_state 4 MiB — the **same four constants**, reused | identical | `openai_codex.zig:117-121`, `xai_grok.zig:104-108`, `ReplayLimits` `responses_protocol.zig:7-12` |
| **Captured content** limit (upstream) | `null` = **uncapped** on the agent path | `null` = uncapped | `gateway_step.zig:272,325,400`; the parameter exists (`stream_provider.zig:190`) and is set only for the image path (`image_provider.zig:77`) |
| **Retained content** limit (xfx, deliberate deviation) | **64 MiB** | **64 MiB** | chosen to equal `max_sse_aggregate_bytes` (`openai_codex.zig:17`, `xai_grok.zig:19`) — see below |

**Retained content is bounded; streaming is not.** `appendCaptured`
(`responses_protocol.zig:490-506`) clips the *retained* copy at the limit while still emitting every
delta to the callback, so the limit bounds `Completion.text` and never what the user sees. Upstream
passes `null` for agent turns — no clip at all — and **xfx should not copy that.** An unbounded
retained string is an unbounded allocation and an unbounded `fsync`ed write into
`events.jsonl` on a path an operator cannot interrupt, in a product whose entire decoder discipline is
"bounded per frame, per completion, and by tracked-block count". A limit that exists nowhere else in
the pipeline is the one hole in it.

The concrete number is **64 MiB per completion**, equal to `max_sse_aggregate_bytes`
(`openai_codex.zig:17`, `xai_grok.zig:19`). Chosen rather than invented: the stream is already
terminal-errored above that aggregate, so a retention cap at the same value can only ever bind on a
stream that was going to be refused anyway — it adds no new refusal, it just stops the *copy* from
being the thing that exhausts memory first. It applies uniformly to every wire, including the two that
ship today.

**Truncation is marked, never silent.** On reaching the cap the retained text stops growing and gains
a single trailing sentinel line naming what happened and how many bytes were dropped — the same
pattern `read_file` already uses, where "an explicit sentinel stating how many of the file's lines
were shown" is the contract (`docs/parity.md`). The session event therefore records a *provably*
truncated answer rather than one that merely looks complete, and a resumed turn replays the marker
with it, so the model is told too. Streaming to the terminal is unaffected: every delta reaches the
user, because clipping what a user is watching would be a different and worse promise.

#### Retryability and Retry-After

| Question | Upstream answer | Evidence |
|---|---|---|
| Which HTTP statuses are retryable? | `rate_limited (429)`, `server_error (500)`, `bad_gateway (502)`, `unavailable (503)`, `gateway_timeout (504)`. Everything else — `invalid_request (400)`, `unauthorized (401)`, `forbidden (403)`, `request_too_large (413)`, and the catch-all `provider_error` — is **not** | `isRetryableModelFailure` `orchestrator.zig:1755-1760`; `FailureKind` `stream_provider.zig:223-234`; status mapping `:1764-1776` |
| Is `Retry-After` parsed on the Codex/Grok paths? | **No.** Neither module reads the header at all; only the Vercel Gateway client does | grep for `retry_after` in `openai_codex.zig` / `xai_grok.zig` is empty; `client.zig:1864-1871` |
| How does the Gateway parse it? | Header lookup case-insensitive, trimmed, **integer seconds only** — a date form yields `null`; overflow saturates to `u64::MAX` | `client.zig:1864-1871` |
| Is the server's value trusted? | No, it is **clamped**: the transport caps the delay at 5 s (`gateway_retry_after_max_ns`), and the turn-level recovery caps it at 30 s (`max_retry_after_seconds`) | `client.zig:180-182,1858-1862`; `model_response_recovery.zig:4,148-151` |
| Backoff when there is no `Retry-After` | Transport: `(attempt+1) × 150 ms`. Turn: 250 ms, then 1 s, then doubling, capped at 30 s | `client.zig:1852-1855`; `retryDelayNs` `model_response_recovery.zig:165-175` |
| Attempt budget | 10 provider attempts per turn | `default_max_provider_attempts` `model_response_recovery.zig:3` |
| Does an explicit `Retry-After` reset the backoff ladder? | Yes — pacing returns to `idle`, so a server-directed wait does not compound with implicit backoff | `RetryPacingState.afterFailure` `:54-57` |
| In-band failure classification | `response.failed` / `error` inside a 200 becomes `ResponseFailed`, and recovery classifies by **cause** (`rate_limited`, `provider_unavailable`, `transport_interrupted`, `response_interrupted`, `authentication`, `request_limit_reached`, `content_filter`), not by transport | `responses_protocol.zig:377-379`; `FailureCause` `model_response_recovery.zig:6-15` |
| Does delivery certainty gate the retry? | Yes, and it dominates: `definitely_unsent` → replay the request; anything `possibly_sent` is routed by what evidence exists (partial output → continue; tool proven unexecuted → regenerate; uncertain → reconcile) | `model_response_recovery.zig:133-145`; `markPossiblySent` `openai_codex.zig:258` |

**Two things xfx must decide rather than copy.** (1) Codex/Grok not reading `Retry-After` is a gap,
not a design: xfx already carries `retry_after: Option<Duration>` on `ProviderError::Status` and
already clamps at the turn, so parsing it on the new transports is strictly better and costs one
helper — do it, and note the deviation. (2) xfx's turn owns retries and sends no team header
(`docs/parity.md` "transport-owned retry" — deferred), so the transport-level 5 s clamp has no xfx
analogue; the 30 s turn-level clamp and the 10-attempt budget are the ones to adopt.

### Replay — the round trip that makes this load-bearing

`writeInput` (`:14-127`) re-serializes an assistant turn on the next request by writing the stored
`provider_state_json` array **items first, verbatim**, then the text, then the tool calls
(`:38-60`), under `ReplayLimits` (`:7-12`) validated by `validateReplayMessage`. A state that is not a
JSON array is `InvalidProviderState`.

This is the same *kind* of contract as `Completion.raw_content` on the Anthropic wire, and it exists
for the same reason: *"xfx cannot reconstruct a signature it did not keep."* It is **not the same
field**. Concretely, for the port:

- Codex/Grok encrypted-reasoning items are carried in a **new `Completion.responses_state`**, parallel
  to `raw_content` and disjoint from it: a turn populates at most one of the two. The two shapes are
  not interchangeable, and — decisively — putting Responses items into `raw_content` would make an
  older binary replay them onto the Anthropic wire, since it cannot know about a tag it has never
  read. §"Provenance" below is the authoritative design and carries the full argument.
- The request builder must emit those items **before** the text and tool calls of the same assistant
  turn, because that is the order upstream writes and the order the provider validates.
- A session recorded under one **authority** and resumed under another must **drop** the state rather
  than send it — including Codex↔Grok, which share this serialization but not its issuer. The existing
  degrade-to-text-and-tool-calls path for a Gateway-continued session (`CHANGELOG.md` 0.1.0) is the
  precedent; the full rule is the replay table in §"Provenance".

#### Provenance — a separate field per wire, not one polymorphic blob

`Completion.raw_content` is currently **untyped, provenance-free JSON**: the session log records
`SessionEvent::AssistantMessage { text, tool_calls, raw_content }` (`src/session/event.rs:129-134`)
with no record of which wire produced the blocks. That was safe with exactly one wire that emits them.
It stops being safe the moment a second does, and in two distinct ways.

**Hazard 1 — shape confusion.** Anthropic `thinking` blocks with signatures and OpenAI Responses
`reasoning` items with `encrypted_content` are not interchangeable; replaying one into the other is a
400 at best.

**Hazard 2 — the downgrade, which a tag does not fix.** A tag makes a *new* binary safe. An **old**
binary ignores unknown fields, so if Codex reasoning items were written into `raw_content`, a v0.1.0
binary resuming that session would find a non-empty `raw_content`, conclude "Anthropic blocks", and
replay Responses items onto the Messages wire. A tag's compatibility argument is **syntactic only**;
the storage has to be separated so the old reader sees nothing at all.

**The storage design.** `raw_content` is redefined as *Anthropic-Messages blocks only* — which is what
every existing record already contains, so there is no migration — and Responses-wire state gets its
**own new field**:

```rust
AssistantMessage {
    text: String,
    tool_calls: Vec<RecordedToolCall>,
    /// Anthropic Messages content blocks, verbatim. Only ever written by the
    /// `anthropic_messages` wire -- which is why an older record that has it
    /// can only have come from that wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    raw_content: Vec<serde_json::Value>,
    /// OpenAI Responses replay items (`reasoning` with `encrypted_content`),
    /// verbatim, in arrival order. Disjoint from `raw_content` by construction:
    /// a turn writes at most one of the two.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    responses_state: Vec<serde_json::Value>,
    /// Which wire *and which authority* produced the state above.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wire: Option<Wire>,
}
```

`Wire` is a closed snake_case enum naming the **wire and its authority together**, because the
credential's issuer is part of the replay contract and not a detail: `anthropic_messages`,
`vercel_gateway`, `codex_responses`, `grok_responses`. Codex and Grok share a serialization but are
different authorities — a Codex `encrypted_content` item is opaque state sealed by OpenAI, and handing
it to xAI is handing one provider's sealed blob to another. Folding them into a single
`openai_responses` value would leave that as a future decision; naming them apart makes it a
non-question.

**Compatibility, both directions.**

- *New binary, old record*: `responses_state` and `wire` default to empty/`None`. A non-empty
  `raw_content` with `wire: None` can only be Anthropic, because that is the only wire that ever wrote
  the field — replay it verbatim when the active wire is `anthropic_messages`, drop it otherwise.
- *Old binary, new record*: a v0.1.0 binary does not know `responses_state` and ignores it — `serde`
  skips unknown fields here, since `SessionEvent` carries `#[serde(tag = "kind", rename_all =
  "snake_case")]` and **no `deny_unknown_fields`** (`src/session/event.rs:88`), while only
  `EventEnvelope` denies them (`:178`), which is exactly why both new fields go **inside the variant**
  and never on the envelope. For a Codex or Grok turn that old binary therefore sees `raw_content`
  **empty** and rebuilds from text and tool calls: **degraded, never mis-wired.** That is the property
  a tag alone could not deliver.
- Neither field needs a `schema_version` bump, for the same reason `raw_content` needed none in 0.1.0.

**Replay rule — keyed by authority, not by shape:**

| Recorded `wire` | Active wire | Behavior |
|---|---|---|
| `anthropic_messages` | `anthropic_messages` | replay `raw_content` **verbatim**, in arrival order, ahead of text and tool calls |
| `codex_responses` | `codex_responses` | replay `responses_state` **verbatim**, items first, then text, then tool calls |
| `grok_responses` | `grok_responses` | replay `responses_state` verbatim, same ordering |
| `codex_responses` | `grok_responses`, or the reverse | **drop with notice.** Same serialization, different authority: the payload is state sealed by the other provider |
| `None` with non-empty `raw_content` | `anthropic_messages` | replay verbatim — a legacy record's blocks can only have come from that wire |
| `None` with non-empty `raw_content` | anything else | **drop with notice** |
| any | `vercel_gateway` | **drop with notice** — that wire has no replay contract |
| any | any other mismatch | **drop with notice** |

A drop is **never silent**: it emits a one-line user-visible notice on resume naming the recorded
wire, the active wire, and the fact that prior reasoning was not carried over, because the user is
entitled to know that the model resumed with less context than the log holds. The stored items are
**not** deleted — a later resume back onto the original authority replays them. Dropping shapes a
*request*; it never mutates a record.

A dropped state still sends a complete assistant turn: text and tool calls replay as normal, which is
exactly the degrade path 0.1.0 already ships for a Gateway-continued session.

## Session files

> **Target** for the subscription token files. The session *event log*'s two-field provenance
> (`raw_content`/`responses_state` keyed by wire) shipped and is in `docs/parity.md`.

Upstream stores three files under `~/.fx/`; xfx's homes are **`~/.xfx/codex-auth.json`** and
**`~/.xfx/grok-auth.json`** (upstream's names are `chatgpt-auth.json` / `grok-auth.json`,
`profile_paths.zig:5-8`; xfx renames the first for the same reason it renamed everything else — the
product's own identity).

The contract to port verbatim (`chatgpt_session.zig`, `grok_session.zig`, near-identical):

- Schema v1: `{"version":1,"access_token","refresh_token","expires_at_ms","account_id"}` (`:220-231`);
  Grok additionally validates header-safety of `account_id` on both parse and write (`:219,230`).
- **Load refuses a file that is not a regular file or whose mode has any group/other bit
  (`mode & 0o077 != 0`)** (`:116-119`); the profile directory is forced to `0700` (`:184-193`).
- Writes take a **timed advisory lock** (`*-auth.lock`, 2000 ms deadline, `:12-13`) and go through
  temp + rename + `fsync`.
- Expiry uses `expires_at_ms` with a **60 s safety skew** (`:18-20`); `loadAccess(stored|if_needed|
  force)` takes the lock, refreshes when expired, saves back (`chatgpt_oauth.zig:420-440`). Logout
  deletes under the lock and distinguishes `deleted | missing | deleted_not_durable`.

These are already xfx house style — `0600`, staged write plus rename, refuse rather than repair —
and the session store and `setup llmux`'s settings merge are the precedents to reuse.

**macOS Keychain is out of scope, on evidence.** It covers *only* the Vercel `fx login` session
(`oauth_session.zig`); the two subscription sessions are profile-file on every platform. Without
Vercel device login it buys nothing, and it is ~450 lines of migration state machine
(`selectResolution` `:91-100`, `publishAndVerifyKeychain` `:306-316`).

## llmux as the fourth provider

> **Shipped**, and as the *second* provider rather than the fourth: the two subscription providers
> this document numbers ahead of it do not exist in the binary. Everything in this section is in
> `docs/parity.md`'s `llmux` provider row and its `llmux keyless loopback credential` row; what
> follows is the design it was built from, kept because the impedance mismatches at the end are
> still the reasons the code looks the way it does.

llmux was already a working transport; what it lacked was an identity in a provider set.

- **`ProviderId::Llmux`** with a `ProviderEntry {slug: "llmux", subscription: false, name: "llmux",
  description: "local keyless daemon"}` so `/setup` and `/model` can list it.
- **Credential = keyless loopback.** Model it as `CredentialSource::LlmuxLoopback`, resolved from
  **configuration alone: a `llmux_url` that is present and passes the loopback-service rule.** It is
  *not* "the daemon is reachable" — resolving a credential must do no I/O, or `status` and `doctor`
  stop being safe to run. A daemon that is down still has a *present* credential and fails at the
  ping, the catalog load, or the turn, each of which reports in its own vocabulary.
  It behaves like an env-detected credential needing no login,
  so its `/setup` entry is the **API-key-style setup action, not a login action**
  (`app_auth_runtime.zig:279-303`) — the whole `SignInRuntime`/PKCE path is skipped.
- **Catalog from `GET /models`**, which `xfx setup llmux` already reads and proves non-empty
  (`src/llmux/setup.rs`, `docs/parity.md` setup row). Map it to `ModelCatalogEntry`, including
  **aliases** (either one entry per alias or carried like `ProviderEntry.aliases`) and **efforts**
  (`reasoning_efforts[]`), the same shape Codex/Grok fetchers produce
  (`openai_codex_models.zig:188-237`).
- **Transport stays the Anthropic Messages wire** — a fourth sibling to the three upstream wire
  modules, sharing none of the Responses serialization (`src/llmux/protocol.rs`, `src/llmux/sse.rs`).

Impedance mismatches the note flags, each a real design decision:

- fx threads a **bearer credential string** into every request and into `catalogAccess`. llmux is
  keyless, so add a `CatalogAccess` variant that is **authenticated-but-tokenless**. Do not model it
  as "the daemon is up" — that is a reachability probe wearing a credential's clothes — and do not
  pass an empty secret through a guard written for a non-empty one, which is how this becomes a
  confusing 401 later.
- Persistence is keyed by the provider **tag string**, and adding a variant touches every exhaustive
  match (`Set.select`, `authorizesCredential`, logout resolution, settings validation).
- llmux advertises Anthropic-style thinking, not OpenAI `reasoning.effort`. A shared `ReasoningEffort`
  enum must cover its set, and the request shaper must translate to the Messages `thinking` field —
  where xfx's measured constraint applies: pinning `thinking.type=disabled` is refused by the daemon
  for the default model, so adaptive is what it gets (`src/llmux/protocol.rs:10-16`).
- There is **no account id.** `credential_authority` needs a stable identity — use a fixed slot the
  way the gateway sources do (`credential_authority.zig:23-28`), not a synthesized account.

## `/setup` and `/model` — the user-facing surface

> **Half shipped.** `/setup <provider>` exists on both front ends and runs the transaction below;
> `/model` is the catalog browser below, on both front ends, and refuses an id the loaded catalog
> does not publish. **Target**: the sign-in, credential-switch and team menu choices, and
> `/logout` — all three need credentials xfx does not have.

Upstream **removed `/provider` in 0.0.5** and folded provider switching into `/setup` (the top-level
`fx provider` CLI command survives). That settles a design question for free: **do not build a
`/provider` slash command.** See [`05-upstream-delta.md`](05-upstream-delta.md) §B2.

`/setup` menu choices (`picker_presentation.zig:259-281`, `app_auth_runtime.zig:279-303`): sign in
with each provider, API-key setup, **Switch provider**, Switch credential, Change team.

`switchProvider` (`app_auth_runtime.zig` ~760-960) is a six-step transaction worth porting as a
transaction:

1. `decideProviderSwitch` gate — `no_change | busy` (a stream or queued work is in flight) | `prepare`
   (`auth_transition.zig:25-31`);
2. `resolveForProvider(target)`; if absent and login is allowed, start that provider's OAuth;
3. `authorizesCredential(target, source)` guard;
4. fetch the target catalog — must be non-empty and valid;
5. pick the model: saved preference, else `XFX_MODEL`, else the catalog's first valid entry;
6. **publish atomically**: catalog, `{provider, model}` selection, credential, then persist.

Persistence: top-level `provider` plus a `models` object keyed by provider tag
(`settings_store.zig:1409`), with legacy flat keys migrated away (`:1410`). In xfx these are
**profile-only** keys, like `model` and `backend` are today — a shared repository must not choose
which endpoint receives a prompt.

`/model` is a catalog browser: provider-advertised models with **context window and effort levels**
(`model_menu_presentation.zig:279-286`, `picker_presentation.zig:456,520`). Both front ends browse it
and select from it through one `ModelSelector`, so the catalog-membership refusal is one rule rather
than one per surface; the TUI's selection is a piece of work on the runtime thread, because that is
where the catalog and the socket that filled it are. `xfx setup llmux`'s existing behavior — keep the profile's model when
the catalog has it, else take the catalog's first entry, warn when a higher layer still outranks the
write — is the semantics to generalize, not to replace.

`/logout [vercel|codex|grok]` resolves explicit-arg first, else active, else the sole logged-in
subscription, else gateway (`auth_transition.zig:40-54`), then reconciles the active credential back
down the precedence order (`auth_runtime.zig:1482-1504`).

## Profile migration — shipped `backend` to target `provider`

> **Shipped.** `provider` and `models{}` are read and written, `backend`/`llmux_url` are kept in
> step for older binaries, and the conflict case is a named `doctor` check.

v0.1.0 persists a **flat** profile: `backend` (`gateway|llmux`), `llmux_url`, and one `model`
(`src/config.rs`, `docs/parity.md` "backend selection" row). The target persists `provider` plus a
`models` object keyed by provider tag. Both shapes will exist on real machines at once, so the rule
has to be written before the first write, not discovered after it.

**Migration rule (read-repair, never a rewrite on read).**

1. On load, if `provider` is absent, derive it: `backend: "llmux"` → `Llmux`, `backend: "gateway"` or
   absent → `Gateway`. A `backend` value that cannot be read stays what it is today — `backend_rejected`
   quoted back, the turn refuses, **no default is substituted.**
2. A flat `model` seeds `models[<derived provider>]` in memory only. The file is not rewritten by a
   read; `status` and `doctor` must stay side-effect-free, and a diagnostic command that edits the
   profile it is describing is the opposite of the contract.
3. The file gains `provider`/`models{}` only when something already writes it — `xfx setup <target>`
   or a `/model` selection — through the same staged-`0600`-plus-rename merge that preserves every
   unrelated key.
4. `llmux_url` is unchanged and stays profile-only. It is a **provider parameter**, not a credential,
   and it keeps its own loopback-service policy.

**Coexistence precedence (both keys present).** `provider` wins over `backend`, and
`models[provider]` wins over flat `model`, because the newer key is the one a newer binary wrote
deliberately. `backend` is **not** deleted on write — see rollback. When the two disagree, the
disagreement is reported: a `doctor` `config` check names both values, since a machine whose profile
says two different things about where prompts go is exactly the machine an operator should be told
about. Layer order is untouched: project → profile → exact-workspace → environment, and `provider`,
`models`, `backend`, `llmux_url`, `model` are **all profile-only**, so a cloned repository still
cannot choose the endpoint.

**Rollback — an old binary reading a new profile.** This is the dangerous direction, and it is
already decided by the shipped code: xfx reads only the keys it consumes and does not reject unknown
ones, so a v0.1.0 binary reading a profile containing `provider` and `models{}` **silently ignores
both** and falls back to `backend` + flat `model`. That is safe **only if the writer keeps those two
keys in sync**, which is why step 3 writes `backend` and `model` alongside `provider` and
`models{}` for as long as the pair is `Gateway`/`Llmux` — the two providers an old binary can
actually reach. For `Codex`/`Grok` there is no representable legacy value, so the writer leaves
`backend` **at its previous value** rather than inventing one: an old binary then keeps talking to
the backend it was last told about instead of to a provider it cannot authenticate.

**Why the endpoint-selection safety property survives.** The property is: *a prompt is never sent to
an endpoint the operator did not choose, and an unreadable choice is never replaced by a default.*
Each clause is preserved by construction — every new key is profile-only (a repository cannot set
them); an unreadable `provider` is refused exactly like an unreadable `backend` rather than defaulted;
`llmux_url` keeps the loopback-service rule; and the rollback path degrades to a **previously
operator-chosen** value, never to a built-in one. The one new failure mode is the desync in the
paragraph above, which is why it is reported by `doctor` rather than resolved silently.

## MVS order

> Steps 1 and 2 are **shipped**; 3, 4 and 5 are **target**. The order below is unchanged, and it is
> also the receipt for why the shipped half could ship first: it needed no OAuth, no browser and no
> third-party approval.

1. **Provider frame + llmux and Gateway as two `ProviderId`s.** Enum, bundle, selection runtime,
   persistence (`provider` + `models{}`). Pure plumbing, no network, no OAuth — and both transports
   already exist, so this alone delivers provider switching.
2. **`/setup` switch-provider + `/model` per-provider catalogs** against a `ModelCatalog` trait: the
   Gateway's existing credential, llmux's `GET /models`. This is the visible payoff and still needs no
   OAuth.
3. **Codex OAuth.** Self-contained: browser + loopback + PKCE, JSON refresh, JWT account id,
   `0600` profile session. Acceptance criteria the delta pins: a catalog-valid model is activated on
   login, the authorization URL is a clickable terminal link, and a credential that fails to load
   falls back rather than fataling.
4. **Grok OAuth.** ~90 % identical to Codex once it exists: form refresh, userinfo account id,
   revoke-on-logout.
5. **Deferred: macOS Keychain** (evidence above), Vercel device login, teams, credits, usage.

**This order is the release order, and the dependency is why** — recorded here once so
[`05-upstream-delta.md`](05-upstream-delta.md) §2 can defer to it instead of restating it. Steps 1–2
are a precondition for 3–4, not a preference: OAuth produces a credential that has nowhere to be
*selected* until `ProviderId` exists, and nothing to be *spent on* until a per-provider catalog can
name a model, so landing Codex first would mean writing the frame anyway and writing it under
schedule pressure. Steps 1–2 also need no network, no browser, and no third-party approval, so they
are the part that cannot be blocked; step 3 has an external gate (see Policy risks). The visible
payoff — switching providers and browsing models — therefore arrives before, not after, the riskiest
work.

Every new client inherits xfx's **bounded-decode discipline** — per-line, per-completion and
per-tracked-block limits with typed terminal errors rather than silent truncation — with upstream's
numbers as the floor (`openai_codex.zig:15-33`, `xai_grok.zig:17-26`).

## Policy risks

**Reusing fx's OAuth client ids presents xfx as fx.** This is a user decision, not an implementation
detail, and it should be made before step 3 rather than discovered after it.

The facts: the client ids are hardcoded and public in the upstream binary (`chatgpt_oauth.zig:14`,
`grok_oauth.zig:14`), and both flows are **public PKCE clients with no client secret** — so a port
*can* technically use them. What identifies the client to the provider beyond the id is fx's own
identity: Codex sends `originator=fx` (both as an authorize parameter and as a request header) plus
`codex_cli_simplified_flow=true`; Grok sends `referrer=fx`, `x-grok-client-identifier: fx`, and
`X-XAI-Token-Auth: xai-grok-cli`. Reusing the ids also binds xfx to fx's **registered redirect
URIs** — fixed ports 1455/1457 for Codex; Grok's ephemeral loopback is more forgiving.

Options, neutrally:

| Option | What it costs | What it risks |
|---|---|---|
| **A. Reuse fx's client ids** | Nothing to build; ships in step 3 | xfx traffic presents as fx to OpenAI/xAI — an ownership and ToS question, not a technical one. Also directly contradicts the reason xfx does *not* claim to be fx in its Gateway headers (`src/gateway/mod.rs`, header comment) and the whole `UPSTREAM.md` §"Why the name is `xfx`" argument |
| **B. Register xfx's own OAuth clients** | An application to OpenAI and to xAI; approval is not guaranteed for a third-party port; own redirect URIs and constants | Delay, and a rejection would leave the epic without subscription providers |
| **C. Route subscriptions through llmux** | Nothing new in xfx: llmux already owns Codex subscription tokens and its own credential story | Depends on llmux's own ToS posture and does not give xfx a native `login`; Grok is not covered today |
| **D. Ship the frame without native OAuth** — provider frame + Gateway + llmux + `/setup` switching + per-provider `/model` catalogs now, and defer Codex/Grok until an xfx-owned registration is approved | Nothing: it is MVS steps 1–2, which have no OAuth dependency at all. The user-visible payoff lands on schedule | Nothing is impersonated and nothing is delayed except the two subscription providers. The cost is that `login`/`logout` stay deferred rows for longer, and the honesty contract requires them to stay **absent from the binary** meanwhile — not a stub, not a "coming soon" line in `/setup` |

**D is the default unless the decision below says otherwise.** It is the only option that ships value
without making a policy claim, and it is exactly the boundary the MVS already draws between steps 1–2
and steps 3–4 — which means choosing it costs nothing that has to be rewritten later.

**Decision gate, before step 3 begins.** Codex/Grok work does not start until one of these is true,
and which one is recorded in `UPSTREAM.md`:

- an xfx-owned OAuth client is **registered and approved** by OpenAI / xAI (option B) — then step 3
  starts against xfx's own ids, redirect URIs and originator/referrer values, and no fx identity is
  sent;
- the user **explicitly accepts** presenting as fx (option A), with that acceptance written down
  rather than inferred from silence — this is a ToS judgment, not an engineering one, and it is
  precisely the class of decision the operator reserves;
- llmux mediation (option C) is chosen, in which case there is no xfx OAuth work at all and the item
  leaves this document.

Until one of those holds, the epic ends at step 2 and `docs/parity.md` keeps its `deferred` rows
unchanged. A deferred row is not a promise, so nothing about waiting is dishonest — but a `/setup`
entry that offers a sign-in xfx cannot perform would be, which is why the gate is placed before the
menu entry rather than before the flow behind it.

`[추정]` The research note's own read is that own-client registration (B) is the long-term path, and
that (A) works for a first slice **if it is flagged as a policy decision rather than shipped as a
silent default.** This section is that flag. Whichever is chosen, it belongs in `UPSTREAM.md` next to
the identity argument, because it is the same argument.

Second-order: subscription tokens are a **new class of persisted secret** in a product whose current
security story is "the credential is read from the environment and written nowhere". Step 3 therefore
also edits `README.md` §"Safety, in plain terms", flips `auth_refreshable`, and revisits
`check-no-secrets.sh` before the first token exists.
