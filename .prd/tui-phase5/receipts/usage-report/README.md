---
title: 0.0.16 delta -- P5-US-U1 receipt (AI Gateway usage report client)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, security]
---

# P5-US-U1: the AI Gateway `/v1/report` client

Spec: `research/tui-phase5:.prd/tui-phase5/research/usage.md` §5 U1, with the
§3 rules it cites (who is asked, the wire, the status mapping). Oracle: fx
v0.0.16 source at `481104fd`. Base: `main` @ `0a51e1c4`. Unit class: **S**
(a credential to a new path).

Code: `src/usage/report.rs` (new, declared from `src/usage.rs`), one row in
`docs/parity.md` (Providers, `AI Gateway usage reports`, `partial`), and
`test_net::read_request` made `pub(crate)` for the loopback servers.

**No UI pair.** This unit is a library with no caller and no screen, so the
paired-capture rule (`p5-unit-common.md` § Method 2) does not apply: there
is no fx screen it changes and no xfx screen to put beside one. RED is shown
by mutants instead (below). The `/usage` screen pairs are U4's.

## What it exports (for U2-U4)

After the US-U1b trim: `Totals` (`total_tokens`, `add`, `cache_hit_pct`,
`error_pct`, `p90_ttft_ms`, `p90_tps`, `valid`), `ModelRow` (its `model` is
untrusted server text: see its doc), `parse_model_rows`, `InvalidResponse`,
`utc_day`, `format_day`, `Unavailable` (`NoGatewaySignIn`,
`UnsupportedProvider`, `ProviderRejected`), `Failure` (`Network`, `Refused`,
`NotAvailable`, `RateLimited`, `Server`, `BadResponse`), `status_failure`,
`Failed`, `Question` (`label`) and `question(config, endpoint)`, `DayRange`,
`Outcome`, `Fetch` (`start`, `try_outcome`, `cancel`; dropping it cancels),
`Job`, and the constants `MAX_BODY_BYTES`, `MAX_MODELS`,
`MAX_MODEL_NAME_BYTES`, `DEADLINE`, `API_KEY_LABEL`. The names follow fx's
(`report.zig`, `usage.zig`); fx's `Rows` arena is a `Vec<ModelRow>`, and fx's
`[2]i64` range is `DayRange`. Deleted until a unit builds them, with its own
tests: fx's `Scope` (and `readsReports`), `reportPath` for other scopes and
`error.NotAiGateway`, `Unavailable.{no_team,refresh_needed}`,
`Failure.cache`, `wait_timeout`, `Question::scope`.

## fx's tests, ported

| xfx test (`src/usage/report.rs`) | fx source |
|---|---|
| `the_report_path_is_fxs_for_an_api_key` | `src/usage/report.zig:352-362@481104fd`, the `api_key` vector (the member, budget and Codex vectors, `:355-358,363-367`, belong to U9) |
| `model_rows_parse_merge_duplicates_and_sort_by_cost` | `src/usage/report.zig:370-392@481104fd` |
| `perf_percentages_merge_by_request_count_and_p90s_by_request_weight` | `src/usage/report.zig:394-412@481104fd` |
| `malformed_reports_are_rejected_instead_of_shown` | `src/usage/report.zig:427-445@481104fd`, all ten vectors and the empty report |
| `a_status_maps_to_the_failure_fx_names` | `statusFailure`, `src/usage/usage.zig:889-896@481104fd` (fx has no direct test; `usage.zig:1377-1396@481104fd` exercises 403) |
| the no-team/no-account assertions in `the_key_goes_to_the_configured_origin_with_fxs_query` | `src/gateway/client.zig:370-373@481104fd` (`api_key` sends no extra header) |

Not ported, and why: `report.zig:414-425@481104fd` (month starts and budget
resets) and `:447-461@481104fd` (`parseBudget`) belong to the member scope's
budget (U9) and the fetch plan (U3); `cache.zig`, `view.zig` and
`policy.zig` tests are U2 and U3.

## The five security properties

| # | Property | Test(s) |
|---|---|---|
| 1 | The key goes only to the configured Gateway origin, under the bearer rule: no redirect, no proxy over cleartext, cleartext only to loopback; nothing is sent under an unreadable provider setting | `the_key_goes_to_the_configured_origin_with_fxs_query` (path, query, `authorization`, no team/account header; a non-loopback `http` endpoint cannot be built), `the_report_takes_no_proxy_over_cleartext` (a copy of the test binary with `HTTP_PROXY`/`ALL_PROXY` set: the proxy receives nothing), `only_an_ai_gateway_api_key_is_asked_for_usage` (`VERCEL_OIDC_TOKEN`, both set, none, blank, llmux: no `Question` exists, so no request can be made), `an_unreadable_provider_setting_sends_the_key_nowhere` (US-U1b) |
| 2 | A redirect is an error, never followed with the key | `a_redirect_is_an_error_and_never_followed_with_the_key` (301, 302, 303, 307, 308: `bad_response`, `HTTP 3xx`; the `Location` server receives nothing) |
| 3 | The answer is bounded: 1 MiB, refused not read whole; 15 s, bounded test | `an_answer_over_1_mib_is_refused_without_being_read_whole` (exactly 1 MiB accepted; 1 MiB + 1 refused; a declared length over the cap refused at the head; an endless chunked body: the client hangs up, the server's write fails before its own 64 MiB bound), `a_body_over_the_cap_is_not_parsed`, `a_gateway_that_never_answers_fails_at_15_seconds` (a listener that never reads, one that stalls mid-body, and one that drips a byte a second and never finishes, all `network` at >= 15 s and < 20 s; each wait capped at 25 s, every server thread at 20-30 s), `a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body` (the worker thread is joined within 5 s and the server sees the hang-up), `a_dropped_fetch_stops_and_lets_go_of_the_key` (US-U1b) |
| 4 | The key never appears in output: failure values hold no key even when echoed, and are one bounded line | `the_key_never_appears_in_a_failure_even_when_echoed` (the key echoed in a 403's header, JSON body and `\u`-escaped body, a 400 with 4000 bytes and a newline, a 500 reason phrase, a non-JSON 200, a model name that is the escaped key, a refused row naming it: the `Failed` and the whole `Outcome`'s `Debug` hold no key, no newline, <= 128 bytes); `Question`'s `Debug` in `only_an_ai_gateway_api_key_is_asked_for_usage` |
| 5 | Hostile rows rejected as fx rejects them | `malformed_reports_are_rejected_instead_of_shown` (fx's vectors, overflowing product and sum included), `numbers_are_read_as_fxs_parser_reads_them`, `a_key_twice_in_any_object_refuses_the_report`, `names_and_models_are_bounded_as_fx_bounds_them`, `perf_percentages_…` (a negative percent) |

## RED: mutants

The module is new, so "fails on the base" is only "does not compile". RED is
each guard taken out, one at a time, on a `git clone --no-local` copy in the
scratchpad (never this worktree), with only the guarding test run under
`gtimeout -k 10 600`; the unmutated copy passed first. Full redacted output:
`red-mutants.txt`.

| Mutant | Property | Failing test and assertion |
|---|---|---|
| M1 client without `bearer_rules` (reqwest's default redirect policy) | 2 | `a_redirect_…`: `a 301 was followed: ["GET /v1/report …"]` |
| M2 `Policy::none()` kept, `no_proxy` dropped | 1 | `the_report_takes_no_proxy_…`: `the proxy received 1 request(s), bearer attached: [true]` |
| M3 `tokio::time::timeout(DEADLINE, …)` removed | 3 | `a_gateway_that_never_answers_…`: `an outcome within 25 s` (the test's own bound; finished in 25.01 s) |
| M4 per-chunk cap removed | 3 | `an_answer_over_1_mib_…`: `left: Failed { failure: BadResponse, detail: "unexpected report: over 1 MiB" }` -- the 1 MiB + 1 body was read whole |
| M5 a non-200's body quoted into the detail | 4 | `the_key_never_appears_…`: the detail held `bad key xfx-usage-report-test-key…` |
| M6 the key-in-model-name refusal removed | 4 | `the_key_never_appears_…`: `a failure, not Fetched([[ModelRow { model: "xfx-usage-report-test-key…` |
| M7 the per-row `Totals::valid()` removed | 5 | `malformed_reports_…`: fx's vector `request_count 1e17 × p90_ttft_ms 1e300` accepted |
| M8 `VERCEL_OIDC_TOKEN` taken as `api_key` | 1 | `only_an_ai_gateway_api_key_…`: `left: Ok(()) right: Err(UnsupportedProvider)` |
| M9 the duplicate-key refusal removed | 5 | `a_key_twice_in_any_object_…`: `{"results":[],"results":[]}` accepted |

Bounds on those runs: every server in the tests stops by itself (the
streaming one at 64 MiB or 20 s, the silent ones within 20-30 s), every wait
is capped at 25 s, and the process was killed at 600 s by `gtimeout`. macOS
does not enforce `ulimit -v`, so the memory bound is the servers' own (the
M4 client could have buffered at most the 64 MiB the server would write).

## Decisions (each reversible)

- **Failures quote nothing from the answer.** fx's `Failure` carries no text
  (`usage.zig:94-105@481104fd`); a `Failed`'s detail here is a status code or
  a fixed phrase. So `credits.rs`'s `mask` and `safe_one_line` have nothing to
  act on and are not used: a server that echoes the key has no channel into a
  failure, which property 4's test pins with escaped and split echoes that a
  text mask would miss.
- **Refuse, not truncate, past 1 MiB.** `crate::gateway::read_bounded` keeps
  the first `limit` bytes; fx refuses (`error.StreamTooLong =>
  error.UsageResponseTooLarge`, `client.zig:430-431@481104fd`), which reaches
  `/usage` as `network` (`usage_host.zig:187-197`, `usage.zig:880-883`). The
  body is read before the status is judged, as fx reads it
  (`client.zig:424-434`), so an oversized 404 is `network` too.
- **A declared length over the cap is refused before the body is read**:
  the same `network` outcome fx reaches after reading 1 MiB.
- **A report whose model name holds the key is refused** (`bad_response`).
  fx would show it and write it to the usage cache, where fx keeps even the
  key's hash input out of the file (`cache.zig:22-35@481104fd`).
- **The endpoint is a parameter.** `question(config, endpoint)` takes the
  `Endpoint` a turn is sent to, so the origin is one that passed the bearer
  rule by construction and the function reads no environment.
- **The worker's spawner is a parameter**, so the shell can start it the way
  it starts every non-UI thread (a closure around `spawn_unsignalled`) while
  this library stays outside `crate::tui`.
- **OIDC and llmux are `UnsupportedProvider`**, fx's mapping. **The user
  decided U8 on 2026-11-06: copy fx**, so both get fx's unsupported message.
- **An unreadable provider setting is `ProviderRejected`** (US-U1b), checked
  before anything else in `question`. It is xfx's own state with no fx twin:
  `config.rs` keeps the rejection and leaves `provider` at the Gateway
  default, and that default must not be what sends the key, as a turn
  refuses it.
- **The JSON is read the way fx's `std.json.Value` parse reads it**, where
  serde_json allows: a duplicate key anywhere is refused
  (`duplicate_field_behavior = .@"error"`, Zig 0.16.0
  `lib/std/json/dynamic.zig:146-157`, `static.zig:20-26`; fx pins 0.17.0,
  not installed here, so `[unverified]` that 0.17 is the same), and an integer
  past `i64` is not a number (`number_string`, `dynamic.zig:30-47`). Three
  limits serde_json does not let this copy are in the parity row.

## US-U1b (review round on `1b9f80ba`)

Reviews: opus APPROVE, no MUST; gpt6 one MUST, confirmed by the dispatcher.
Code: `3137a750`. Full redacted output of every run below: `red-u1b.txt`.

**MUST: an unreadable provider setting sent the key.** `config.rs` keeps a
`provider` (or `backend`) value it cannot read as `provider_rejected` and
leaves `provider` at the default, the Gateway; that rejection must poison
every request that would carry the Gateway key. `question()` ignored it.

- RED, on the `1b9f80ba` code with only the new test added, bounded
  (`gtimeout -k 10 600`, a fake key, a loopback listener):
  `an_unreadable_provider_setting_sends_the_key_nowhere` failed with
  `question: Ok(Question { origin: "http://127.0.0.1:…", credential:
  Credential { source: AiGatewayApiKey, secret: "<redacted>" } }); requests
  that reached the Gateway: 1`.
- GREEN: `question()` returns `Err(Unavailable::ProviderRejected)` before
  anything else, for `{"provider":"gatewya"}`, for the same beside a readable
  `backend`, and for `{"backend":"nonsense"}`; the listener receives nothing.
  Mutant M11 (the refusal removed) gives the RED line again.

**Folded in:**

| Item | Change | Test, and the mutant it kills |
|---|---|---|
| Drop | `impl Drop for Fetch` cancels its token | `a_dropped_fetch_stops_and_lets_go_of_the_key`: worker joined within 5 s, server sees the hang-up. M10 (`Drop` removed): `the worker outlived its fetch` |
| Worker exit after `cancel()` | the test spawner keeps the `JoinHandle`; cancel now lands mid-body | `a_fetch_runs_on_its_own_thread_and_can_be_cancelled_mid_body`: joined within 5 s, server sees the hang-up |
| Slow drip | a server that sends its head, then a byte a second, never finishing | `a_gateway_that_never_answers_fails_at_15_seconds`, third case (>= 10 bytes dripped). M3d (a 15 s timeout per `send()` and per `chunk()`, resetting on progress): the silent and stalled cases still pass, the drip fails -- `left: … "AI Gateway's answer stopped partway"` at the server's own 30 s bound. M3b (deadline on `send()` only) and M3c (reqwest `read_timeout`) fail the same test |
| Trim | deleted `Scope`, `report_path`, `NotAiGateway`, `Unavailable::{NoTeam,RefreshNeeded}`, `Failure::Cache`, `wait_timeout`, `Question::scope` | `Scope` went whole: with `Member`, `Codex` and `Grok` gone its one variant was read by nothing. The tests wait with a bounded `try_outcome` loop |
| `ModelRow.model` | documented as untrusted server text: render through `crate::mcp::terminal_safe`, not `bridge::inert` alone, never `Debug`; a cache encodes it | -- |
| Docs | U8 recorded as decided (2026-11-06, copy fx); claim 1 reads "no proxy over cleartext"; the proxy test is renamed `the_report_takes_no_proxy_over_cleartext` | -- |

**Residuals, not this round:** one refuse-on-cap reader shared with
`mcp/transport.rs:1216-1228`, and one bearer GET shared with
`credits.rs:183-208`; the backslash-in-URL question (gpt6 SHOULD 3:
`Endpoint`'s hand-written authority parser versus the client's URL parser),
PLAUSIBLE with no RED. Seen, not tested: `credits::question` matches on
`config.provider` only (`src/credits.rs:148-163`), while a turn refuses a
rejected provider in `Provider::select` (`src/provider/mod.rs:338-345`);
whether `/credits` can be reached in a shell with a rejected provider is not
established here `[unverified]` -- a lead for the credits owner, not this
unit's change.

## Closed from the research doc

`usage.md` § Unknowns marks the native 15 s deadline `[unverified]`. It is
`usage_request_timeout_ms: i64 = 15_000` at `src/gateway/client.zig:269@481104fd`,
applied to the whole GET by `runBoundedHttpOperation` at `:299-308`, with
`usage_response_max_bytes = 1024 * 1024` at `:268` and
`.redirect_behavior = .unhandled` at `:418`.

## Gates

US-U1b, run once, green, on `3137a750` (the code; the receipt is the only
change after it), rustc 1.99.1:

| Gate | Result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4568 passed, 0 failed, 7 ignored (14 binaries); the 22 `usage::report` tests among them |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4589 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
| `cargo test --locked --test parity` (the ledger row) | 24 passed |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1107 checks, 0 failures |
| MSRV: 1.96.1 `cargo check --locked --all-targets` | ok |

No pty test failed, so none was re-run. The first round's gates (on
`f0cfe711`, 4566 and 4587 passed) were the same list, also green.
