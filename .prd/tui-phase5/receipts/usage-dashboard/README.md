---
title: 0.0.16 delta -- P5-US-U3 receipt (the usage dashboard runtime)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, usage, security]
---

# P5-US-U3: the `/usage` dashboard runtime

Spec: `research/tui-phase5:.prd/tui-phase5/research/usage.md` §5 U3, with
the §3 rules it cites (fetch plan, C7). Oracle: fx v0.0.16 source at
`481104fd`: `src/usage/policy.zig` and its tests, the `Dashboard` in
`src/usage/usage.zig:229-616` with its tests (`:1011-1636`), and
`src/core/app/usage_host.zig` with `app_commands.zig:833-871` for when the
dashboard opens, refreshes, ticks and cancels. Base: US-U2's closed head
`feat/p5-usage-cache` @ `e83e3eb2`, on US-U1 `f5faa3b4`; US-U2c (`9ef4e60d`:
`bc42be90`, `934754b2`, `fa178805`) merged in as `50eb7753`. Unit class:
**S** (background requests that carry the credential).

Code: `src/usage/policy.rs` (new), `src/usage/dashboard.rs` (new) with its
tests in `src/usage/dashboard/tests.rs`, both declared from `src/usage.rs`;
`src/usage/report.rs` gains `Failure::Cache`, fx's month arithmetic and
`Question::origin`, and its worker body becomes `fetch_on_this_thread`; one
row in `docs/parity.md` (`AI Gateway usage dashboard runtime`, `partial`).

**No UI pair.** This unit is the runtime only: no screen, no keys beyond its
own entry points, no `/usage` command change, and no caller yet -- `/usage`
still reads the local ledger until U4 wires this in. There is no xfx screen to
put beside fx's, so the paired-capture rule (`p5-unit-common.md` § Method 2)
does not apply, and the `fetching…` -> table capture belongs to U4. RED is
the mutants below (`red-mutants.txt`): the module is new, so on the base it
does not compile. US-U3b's RED is a behaviour test that fails on
`be9b2318` (`red-u3b.txt`).

## What it exports (for U4)

`crate::usage::dashboard`:
- `Dashboard` -- `new(home: Option<cache::Home>, clock: Clock, spawn: Spawn)`
  (asks nothing); `open(Question)`; `open_unavailable(report::Unavailable)`;
  `close()`; `refresh() -> bool` (`r`); `tick()` (every event-loop pass while
  `/usage` is up, and harmless when it is not); `quit()` (also on drop);
  `snapshot() -> Snapshot`; `redraw_due(&Snapshot) -> bool`; `generation()`;
  `now_ms()`; `is_open()`. fx's names: `open`, `openUnavailable`, `close`,
  `refresh`, `tick`, `deinit`, `snapshot`, `generationCount`, `nowMs`.
- `Snapshot` -- `of(Period) -> &View`, `budget()`, `budget_reset_day()`,
  `fetched_at_ms()`, and the fields `status`, `label`, `now_ms`,
  `generation`. fx's `Snapshot` less the Codex/Grok parts and `scope`.
- `Status` -- `Ready`, `Fetching`, `Failed(report::Failure)`,
  `Unavailable(report::Unavailable)`: fx's `Status`.
- `Clock` (`Arc<dyn Fn() -> i64 + Send + Sync>`), `wall_clock()`, `Spawn`
  (`Box<dyn FnMut(report::Job) -> io::Result<JoinHandle<()>> + Send>`; U4
  passes a closure over `tui::signals::spawn_unsignalled`).

`crate::usage::policy`: `FLOOR_MS`, `TIMER_MS`, `Trigger`, `State`,
`floor_clear`, `due`, `Plan` (`today_range`, `week_range`, `month_range`,
`ranges`), `plan`. `crate::usage::report`, new: `Failure::Cache`,
`MonthDay`, `month_day`, `month_start`, `next_month_start`,
`budget_reset_day`, `Question::origin`; `fetch_on_this_thread` is
`pub(crate)`, the old private `run` renamed (`Fetch` is unchanged).

How U4 drives it, from fx's `app_commands.zig:808-871`: `/usage` calls
`report::question(config, endpoint)` and then `open` or `open_unavailable`
(U8 as the user chose on 2026-11-06: OIDC and llmux are
`UnsupportedProvider`, an unreadable provider is `ProviderRejected`, both
from U1); each pass calls `tick()` and redraws from `snapshot()` when
`redraw_due`; `r` calls `refresh()`; Esc calls `close()`; quitting the
shell calls `quit()`.

## How it works

- **The cache first.** `open` reads `cache::read` for the sign-in before
  anything else and never waits; every `snapshot` reads the file again and
  keeps the newer of each part (`reloadEntryLocked`, `usage.zig:484-505`).
- **One fetch, on a worker thread**, through `report::fetch_on_this_thread`
  (U1's transport, unchanged), then on the same thread `cache::store`
  (`fetchAll`, `usage.zig:692-711`). `StoreError::Invalid` is
  `Failure::Cache` (`:705-708`); `Refused` and `Io`, or no home, show the
  fetch from memory, merged with what can still be read (`unsaved`,
  `:713-723`). The store's fsync and lock wait stay off the UI thread, as in
  fx.
- **Two counters.** `generation()` is fx's redraw counter
  (`usage.zig:243-245`, `collectUsageDashboardFacts`), bumped on every change
  the screen should show; `redraw_due` is "it moved, or the minute turned".
  The private `epoch` is the stale guard's: closing, a switch, an unavailable
  sign-in and quitting move it, and each fetch carries the epoch it started
  in.
- **Close follows fx (US-U3b).** `close` leaves the fetch in flight running
  (`usage.zig:300-304`): it finishes, stores its answer under its own
  sign-in, and starts nothing new. Its end is taken in at the next pass,
  open or `r` -- the floor is stamped, and its answer or failure reaches the
  screen only through the cache, because it ended in a later epoch. So
  reopening within the floor shows its fresh numbers, read from the file.
  With no cache to keep it in, the closed generation's answer is not shown
  at all.
- **A switch, an unavailable sign-in and quit cancel.** The dashboard holds
  at most one fetch. These three let go of it: they swap its admission flag
  LIVE -> RETIRED and cancel its token. The worker, once its answer is in,
  swaps LIVE -> ADMITTED before it stores; if it loses, it drops the answer.
  **The linearization:** a worker that wins the swap just before a switch
  stores *after* the switch returns, under its own identity -- it was the
  current answer when it was taken -- and the switched dashboard never shows
  it: it reads only the new sign-in's file. One that loses never stores.
  fx's own stale guard is the identity compare in `finishJob` (`:589,605`),
  which the epoch and the swap replace.
- **Floors per sign-in.** `HashMap<Identity, policy::State>`: a start stamps
  `last_start_ms`, a failure `failed_at_ms`, a success clears the failure.
  Another sign-in's entry is forgotten once its floor has passed.

## fx's tests, ported

| xfx test | fx source |
|---|---|
| `policy::tests::opening_fetches_unless_the_floor_or_a_recent_failure_holds_it` | `src/usage/policy.zig:96-104@481104fd` |
| `policy::tests::r_needs_the_dashboard_open_and_nothing_in_flight_never_the_floor` | `src/usage/policy.zig:106-113@481104fd` |
| `policy::tests::a_clock_corrected_backward_does_not_hold_fetches_until_it_catches_up` | `src/usage/policy.zig:115-122@481104fd` |
| `policy::tests::the_timer_fires_every_15_minutes_while_open_never_while_closed` | `src/usage/policy.zig:124-131@481104fd`, plus xfx's own lines: a fetch in flight and a recent failure hold the timer |
| `policy::tests::finished_days_are_read_once_per_utc_day` | `src/usage/policy.zig:133-148@481104fd` (the scope-free lines; the budget flags `:137,143` have no field, the Codex/Grok plans `:144-147` are U9's) |
| `report::tests::months_start_on_the_1st_and_budgets_reset_on_the_next_1st` | `src/usage/report.zig:414-425@481104fd` (U1 left it for the fetch plan) |
| `dashboard::tests::opening_shows_the_cache_at_once_and_fills_it_from_ai_gateway_in_the_background` | `src/usage/usage.zig:1011-1051@481104fd` (an API key: three requests, no budget, no team) |
| `dashboard::tests::a_dashboard_reads_what_another_process_stored_since` | `src/usage/usage.zig:1106-1163@481104fd` |
| `dashboard::tests::a_first_fetch_creates_a_private_profile_for_its_cache` | `src/usage/usage.zig:1165-1182@481104fd` |
| `dashboard::tests::the_floor_holds_open_and_the_15_minute_timer_never_r` | `src/usage/usage.zig:1333-1375@481104fd` |
| `dashboard::tests::a_refused_refresh_keeps_the_cached_numbers_and_says_why` | `src/usage/usage.zig:1377-1396@481104fd` |
| `dashboard::tests::another_sign_in_never_shows_the_previous_ones_numbers` | `src/usage/usage.zig:1398-1433@481104fd` (a second API key; xfx's three unavailable reasons for fx's `no_gateway_sign_in`/`no_team`/`unsupported_provider`) |
| `dashboard::tests::a_clock_corrected_backward_does_not_freeze_usage` | `src/usage/usage.zig:1435-1473@481104fd` |
| `dashboard::tests::an_unreadable_cache_file_keeps_the_finished_days_on_screen_and_in_the_file` | `src/usage/usage.zig:1475-1512@481104fd` |
| `dashboard::tests::a_cache_that_cannot_be_written_still_shows_what_was_fetched` | `src/usage/usage.zig:1514-1532@481104fd` |
| `dashboard::tests::a_dashboard_never_shows_less_than_it_had_when_the_file_goes_back` | `src/usage/usage.zig:1534-1559@481104fd` |
| `dashboard::tests::a_key_switch_fetches_the_new_sign_in_at_once_and_r_works_for_it` | `src/usage/usage.zig:1561-1590@481104fd` (a key switch for fx's provider switch) |
| `dashboard::tests::a_sign_in_that_arrives_during_anothers_fetch_stops_it_and_fetches_at_once` | `src/usage/usage.zig:1592-1623@481104fd`, **changed**: fx owes the second sign-in a fetch until the first's ends; here the first is cancelled and the second fetches at once |
| `dashboard::tests::an_empty_or_relative_home_is_no_home_and_a_fetch_still_shows` | `src/usage/usage.zig:1625-1636@481104fd` |
| `dashboard::tests::an_answer_the_cache_would_refuse_is_the_cache_failure` | `fetchAll`'s `error.InvalidCache => .cache`, `src/usage/usage.zig:705-708@481104fd` (fx has no direct test): today and the week each finite, their sum not |
| `dashboard::tests::the_screen_redraws_when_the_dashboard_changes_or_the_minute_turns` | `collectUsageDashboardFacts`, `src/core/app/app_commands.zig:849-860@481104fd` |
| `dashboard::tests::a_worker_that_cannot_start_is_a_failure_not_a_fetch_for_ever` | `maybeStartLocked`'s spawn failure, `src/usage/usage.zig:570-574@481104fd` (xfx's own handling) |

Not ported: `usage.zig:1053-1104@481104fd` (fetches run inline without a
worker thread: fx's single-threaded WebAssembly host; xfx always has a
worker) and `:1184-1331@481104fd` (Codex and Grok sign-ins, U9).

## The five security properties

All against a loopback Gateway (one thread per connection, recording each
request's path and bearer, able to hold an answer while watching for the
client to hang up) with two fake keys, an injected clock, and a spawner that
counts and names the worker threads. Every wait is bounded (20 s).

| # | Property | Test(s) |
|---|---|---|
| 1 | Nothing new is requested unless the dashboard is open; a fetch started while open may finish after close and store, but starts nothing new; quit cancels and its worker ends within a bound | `nothing_is_requested_until_the_dashboard_opens` (a built dashboard -- what launch would do -- ticked, refreshed and snapshotted 50 times past the timer and midnight: 0 workers, 0 requests), `nothing_is_requested_while_the_dashboard_is_closed` (after a fetch and a close, and under an unavailable sign-in, 100 timer periods of ticks and `r`: no new request), `a_fetch_started_while_open_finishes_after_close_and_starts_nothing_new` (US-U3b: an answer held at the Gateway; `close` returns in < 100 ms and does not drop the request; released, all three ranges are answered and stored under the key's own identity; reopened within the floor: `Ready`, the fresh numbers from the cache, no new fetch; closed again, 50 timer periods of ticks, `r` and snapshots: nothing), `quit_cancels_the_fetch_in_flight_and_nothing_is_asked_after_it` (`quit` waits at most 1 s and the worker has ended when it returns; the Gateway sees the hang-up; `open`, `r` and `tick` after it ask nothing; dropping a dashboard does the same) |
| 2 | The 60 s floor per sign-in binds opening and the timer; `r` is held only by a fetch in flight (fx, `policy.zig:8-9,46-53`; decided 2026-11-06) | `the_floor_binds_opening_and_the_timer_and_r_waits_only_for_a_fetch_in_flight` (200 `r` and ticks during one held fetch: one fetch; 20 reopenings inside the floor: none; another key fetches at once, but coming back -- straight or through an unavailable sign-in -- does not; the floor passes at exactly 60 s from the start; `r` after a fetch ends does fetch, as fx), `the_floor_counts_from_a_failure_as_well_as_a_start` (a fetch failing 45 s after it started holds opening until 60 s after the failure), `the_floor_holds_open_and_the_15_minute_timer_never_r` (fx's) |
| 3 | The 15-minute timer refreshes only while open, and only through the same floor | `the_timer_refreshes_only_while_open_and_only_through_the_floor` (fires at exactly 15 min, not 1 ms before; none beside a fetch in flight through 20 periods; once, not 20 times, when that fetch ends; never while closed). The floor's other halves cannot hold a timer fetch through the dashboard -- it is due a period after the last start, and a failure comes at most one fetch (three 15 s requests) after its start -- so they are pinned by `policy::tests::the_timer_fires_every_15_minutes_while_open_never_while_closed` and, through the dashboard's open, `the_floor_counts_from_a_failure_as_well_as_a_start` |
| 4 | An answer from a closed or older generation is never shown; one from a switched-away sign-in is never stored either | `an_answer_from_a_closed_or_older_generation_is_never_shown` ((a) closed mid-fetch with no cache: the answer ends after the close and is shown neither by the closed dashboard nor after reopening; (b) reopened while the closed generation's fetch runs, and it fails: `Fetching` while it runs, then `Ready`, not the failure, which still holds the floor; (c) the closed fetch, held between its answer and its store, ends after another process stored a later fetch: the file and the screen keep the later today), and S5's tests for the switch |
| 5 | A sign-in switch takes effect at once: the old key is never sent after it, and no identity's answer is shown or stored under another's | `a_sign_in_switch_takes_effect_at_once_and_never_sends_or_keeps_the_old_key` (key A's request held when the key becomes B under an open dashboard: B's cached numbers at once, `Fetching` for B's own fetch, A's request dropped; every request after it carries B; A's answer is cached nowhere and never in B's file; again with A's worker held at the store: not stored; no file holds either key), `the_same_key_to_another_origin_is_another_sign_in` (a Gateway origin change drops the old origin's request; the new origin is asked once the key's floor passes, the old never again), `another_sign_in_never_shows_the_previous_ones_numbers` (fx's) |

## RED: mutants

Each guard taken out, one at a time, on a `git clone --no-local` copy at
`9c6c6c50` (US-U3b's code head) in the scratchpad (never the worktree),
only `usage::dashboard` and `usage::policy` run, bounded by `gtimeout -k 10
600`. Baseline on the copy: 31 passed. All 16 mutants compiled and each
failed a named test; the copy was clean at the end. Full output:
`red-mutants.txt`. History: a first run at `1c3b0359` found M12 surviving
(the timer test planted its floor by hand; replaced by
`the_floor_counts_from_a_failure_as_well_as_a_start`); M14 came with the
origin test; the set ran again at `a72e6cef` and `d42f07ab`. US-U3b turned
M1 around (close now keeps the fetch, so the mutant re-adds the cancel) and
added M15 and M16. Its behaviour RED is `red-u3b.txt`.

| Mutant | Guard removed | Property | Killed by |
|---|---|---|---|
| M1 | `close` cancels the fetch (US-U3's behaviour; fx's is restored) | S1, S4 | `a_fetch_started_while_open_finishes_after_close_…` (`:1016`, close dropped the request), `an_answer_from_a_closed_or_older_generation_…` (`:1254`, the fetch did not run to its end) |
| M2 | a switch keeps the old sign-in's fetch | S5 | `a_sign_in_switch_…` (`:1352`, A's request not dropped), `the_same_key_to_another_origin_…`, `a_sign_in_that_arrives_during_…` |
| M3 | no admission swap in the worker | S5 | `a_sign_in_switch_…` (`:1383`, A's answer stored after the switch) |
| M4 | `quit` keeps the fetch | S1 | `quit_cancels_…` (`:1063`) |
| M5 | `open` works after `quit` | S1 | `quit_cancels_…` (`:1074`, a request after quit) |
| M6 | the floor forgotten on a switch (fx's reset) | S2 | `the_floor_binds_opening_…` (`:1130`), `the_same_key_to_another_origin_…` |
| M7 | the fetch in flight not seen by the floor | S2, S3 | `the_floor_binds_opening_…` (`:1103`, `r` started a second fetch), `the_timer_refreshes_…` (`:1213`) |
| M8 | the timer not held to an open dashboard (policy) | S1, S3 | `nothing_is_requested_while_the_dashboard_is_closed` (`:994`), `a_fetch_started_while_open_finishes_after_close_…` (`:1047`), `the_timer_refreshes_…`, `policy::…the_timer_fires_…` |
| M9 | opening ignores the floor (policy) | S2 | 9 tests, among them `the_floor_binds_opening_…`, `the_floor_holds_open_…`, the policy ports |
| M10 | `StoreError::Invalid` shown, not the cache failure | cache failure | `an_answer_the_cache_would_refuse_…` (`:872`) |
| M11 | `snapshot` does not re-read the cache | cache first | `a_dashboard_reads_what_another_process_stored_since` (`:549`), `a_fetch_started_while_open_finishes_after_close_…` (`:1034`) |
| M12 | a failure does not stamp the floor | S2 | `the_floor_counts_from_a_failure_…` (`:1178`) |
| M13 | retiring does not cancel the token | S1, S5 | `quit_cancels_…` (`:1063`), `a_sign_in_switch_…`, `the_same_key_to_another_origin_…`, `a_sign_in_that_arrives_during_…` |
| M14 | a switch compares the key only, not the origin | S5 | `the_same_key_to_another_origin_…` (`:1409`) |
| M15 | an older epoch's outcome is shown (US-U3b) | S4 | `an_answer_from_a_closed_or_older_generation_…` (`:1257`, the closed fetch's answer shown with no cache) |
| M16 | `open` does not take in a fetch that ended while closed (US-U3b) | S1 | `a_fetch_started_while_open_finishes_after_close_…` (`:1033`, `Fetching` after the fetch had ended) |

## Decisions (reversible, mine)

- **Where the store runs**: on the fetch's worker, as fx. The alternative,
  storing on the UI thread at `tick`, makes the stale check trivially atomic
  but puts the store's fsync, and its lock wait of up to 5 s, on the UI thread once per fetch.
  The admission swap gives the same guarantee off the UI thread.
- **The floor map** instead of fx's reset on a switch: the brief's property 2
  says "per sign-in", and fx's reset lets a switch away and back ask again at
  once. The cost is a map of identity digests, pruned as floors pass.
- **No `fetch_owed`**: fx owes a new sign-in a fetch only because it cannot
  start one while the old sign-in's runs (`usage.zig:236-238`). Cancelling on
  a switch removes the reason, so the field and its `.open` trigger on `tick`
  are gone.
- **A worker that cannot start** is a `network` failure that stamps a start
  and a failure: fx records nothing and, with `fetch_owed`, would retry on
  every pass.
- **`quit` waits at most 1 s** for cancelled workers, then lets them go: a
  cancelled fetch ends at its next poll, and quitting must not hang on one.

## Decided since (US-U3b, the coordinator's call, 2026-11-06)

1. **`r` and the floor: fx.** `r` refreshes whenever no fetch is in flight
   (`policy.zig:8-9`); the 60 s floor binds opening and the timer. The S2
   test is named for what it proves.
2. **Close: fx.** `close` does not cancel; the fetch finishes and stores under
   its own sign-in, and reopening within the floor shows its numbers from the
   cache. A switch, an unavailable sign-in and quit still cancel. An answer
   from a closed or older generation is never shown but through the cache.
   RED on `be9b2318`: `a_fetch_started_while_open_finishes_after_close_and_starts_nothing_new`
   failed at its first check, `close dropped the request` (`red-u3b.txt`).

## Gates

**US-U3b**, run once, green, on `9c6c6c50` (the code; only this receipt
changed after it), rustc 1.99.1 on macOS (aarch64), with `TMPDIR` a short
directory in the harness temp folder (`claude-501/p5ud`):

| Gate | Result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 4642 passed, 0 failed, 7 ignored (14 binaries); the 26 `usage::dashboard` and 5 `usage::policy` tests among them |
| `cargo test --locked --features fault-injection --no-fail-fast` | 4663 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok, ok, ok |
| `cargo build --locked --release`; `… --features fault-injection --target-dir target/faulty` | ok, ok |
| `scripts/smoke.sh target/release/xfx <dir>` | 46 checks, 0 failures |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios + the oracle, 1107 checks, 0 failures |
| MSRV: `rustup run 1.96.1 cargo check --locked --all-targets` | ok |
| clippy and tests for `x86_64-unknown-linux-gnu` | not run here: no Linux target installed `[unverified]`; CI's Linux runners |

No pty test failed, so none was re-run. The `usage::dashboard` and
`usage::policy` tests were also run alone five times on `9c6c6c50`'s code:
green every time. The same gate list was green on US-U3's `d42f07ab` (same
counts), before the close change.

## Residuals

- `[unverified]` Linux: clippy and tests ran on macOS only; CI's Linux runners
  are the check.
- The admission swap orders the dashboard and its worker; it does not order
  two xfx processes. Two processes' fetches for one key both store, and the
  cache's merge keeps the newer part (U2's "never regresses").
- `Snapshot::label` is the heading's `&'static str` from U1; when the member
  scope arrives (U9) with a team-slug label it becomes owned text.
