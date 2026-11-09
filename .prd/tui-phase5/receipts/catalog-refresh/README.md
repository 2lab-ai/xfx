# catalog-refresh: which asks read the model catalog, and a clock that counts sleep

The rest of U9 (I15, I16), and the sleep clock CATALOG-BOUNDS named; **S**: it
decides when xfx sends background catalog requests. Base `e705a5dc`
(CATALOG-BOUNDS, closed). Every claim below is tied to a file in this folder.

| commit | what |
|---|---|
| `9ffb6310` | RED: a resume or fork asks nothing; `/settings` and the speed commands ask with activity's trigger; an earlier reading is held back (`red-triggers.txt`) |
| `04a1ed4e` | fix: a resume or fork is activity; `/settings`, `/fast`, `/ultrafast` ask with no trigger (`CatalogAsk::TOLD`) |
| `2810f53c` | fix: `CatalogClock` (counts sleep), `CatalogTime`, `CatalogDue` (an earlier reading is due); RED in `red-clock.txt` |
| `7fa5d9c2` | `docs/parity.md` `/model` row |
| `55a6d10c`, `bc83ad25` | two comment citations made exact |
| `fccaf848` | the runtime-level count test settles each read before counting (a mutant survived the first version, `mutants-r1.txt`) |
| `6ab25d2a` | fix: an ask with no trigger over nothing read still starts the first load (found writing this receipt; RED in `red-first-load.txt`) -- **the code head and the swept binary** (`sweep-head.md`, banner `v0.1.0-6ab25d2`) |
| `a1c36a6e` | `docs/parity.md`: that first load, in the `/model` row |
| (this one) | this receipt |

After `6ab25d2a` nothing under `src/` changes (`git diff 6ab25d2a HEAD -- src` is empty), so the swept
binary's behaviour is the head's.

Source outside tests, base to head: every line +439 -125, code alone (no
blank or `//` line) +266 -95 (`size.txt`, `size.py`) -- past the brief's
~400-line mark with its doc comments, under it without. The natural split, if
wanted, is the triggers (`9ffb6310`, `04a1ed4e`, `6ab25d2a`) and the clock
(`2810f53c`), each with its own RED; `7fa5d9c2`'s parity sentence covers
both.

## What fx v0.0.16 does (`fx-cites.txt`, `fx-commits.txt`)

- `LoadTrigger` is `activity` and `picker_open`; "Every trigger is user
  activity, so an idle session never polls" (`model_cache_runtime.zig:27-35`).
  `beginLoad` loads `.idle`/`.failed` on any trigger past the backoff, `.ready`
  rows when `expired` (5 min, `gateway.zig:2348`), `picker_due` (5 s) or
  `retry_due` (`:609-625`).
- **36e8c6fc moved one hook only.** It takes `startModelCacheWarmup` out of
  `snapshotAndQueuePrompt` and calls it from the two submission paths
  (`fx-commits.txt`). The inline column's own `activity` stays at the pin:
  every frame of the column calls `modelPickerCompletions` ->
  `app.modelCompletions` -> `ensureModelCache` -> `startModelCacheWarmup` ->
  `.activity` (`app_render_runtime.zig:559-566`,
  `input_completion_runtime.zig:1108-1110`, `main.zig:2167-2175`,
  `:2007-2045`). The brief's lead -- that the pin narrowed activity to
  submitted prompts -- does not hold, and xfx's column ask is fx's.
- `/settings` reads `resolvedModelCapabilities`, the cache's metadata, and
  starts no load (`app_commands.zig:3230-3246`, `main.zig:2060-2066`); `/fast`
  reads `resolveForApp` (`session_commands.zig:825-828`,
  `model_capabilities.zig:133-141`). The settings Model row opens the menu
  (`app_input_runtime.zig:2062-2066`, `:2104-2107` -> `openModelMenu`).
- A resume from the picker and a fork call `startResumedSessionWarmup` ->
  `startModelCacheWarmup` (`app_session_runtime.zig:2048-2050`, `:2063-2066`,
  `:2121-2124`); the launch, with or without `--resume`
  (`app_entry_runtime.zig:330-340`), and a credential change
  (`app_auth_runtime.zig:555-560`, `:1786-1796`, `:2015-2024`) too.
- The clock: `clockMs` reads `std.Io.Clock.boot`, which "keeps counting while
  the machine sleeps, so a catalog goes stale overnight", and a reading earlier
  than a stored one is due (`model_catalog.zig:72-79`; `beginLoad`'s
  `elapsed` and `backoff_over`, `model_cache_runtime.zig:611-617`). The
  backoff's end is stamped on it (`:951`) and so is the 2 s retry wait
  (`:849`). Requests use `.clock = .awake` (`gateway.zig:875-879`,
  `:1160-1164`).

## The trigger table

Every place xfx can start a catalog read, and every ask the band sends
(`read-sites.txt`, a `grep` at the head outside tests), and what the pin does
there. "ask" is the band's control message (`CatalogAsk`,
`src/tui/bridge.rs`).

| xfx site | fx @481104fd | reads | verdict |
|---|---|---|---|
| launch: `turn_loop` -> `begin_catalog_read`, first load only (`wants_first_load`) | `startModelCacheWarmup` at launch (`app_entry_runtime.zig:340`) | first load | same |
| provider switch: new selector, read by the same first load (mid-work: `CatalogAsks::AfterWork`) | credential/provider change -> reset + warmup (`app_auth_runtime.zig:2015-2024`) | first load | same (xfx has no sign-in or token refresh) |
| submitted prompt or steer: `note_catalog_activity` -> ACTIVITY | `noteModelCatalogActivity` (`main.zig:1193-1201`, `:1225-1232`, `:1248-1256`) | stale/failed | same |
| inline column frame: `settle_column` -> ACTIVITY; an edit with no answer yet: `want_catalog(AWAITED)` | `modelCompletions` -> `ensureModelCache` every frame (`main.zig:2167-2175`) | stale/failed | same -- **not** a divergence (above) |
| `/model` menu, settings Model row: MENU / REVALIDATE | `openModelMenu` -> `revalidateModelCache` (`app_input_runtime.zig:1897-1908`) | picker | same |
| `/settings` | `settingsCatalogSnapshot` (no load) | **none** but a first load nothing has made | was AWAITED (activity) with no answer yet; now TOLD |
| `/fast`, `/ultrafast on` | `resolveForApp` (no load) | **none** | the same, fixed |
| resume from the picker, `/fork` | `startResumedSessionWarmup` | stale/failed | **xfx asked nothing; now ACTIVITY** |
| a turn that needs the catalog, `/model <q>` with no rows: `load_catalog` -> `ensure_catalog` | `resolveForRequest` waits on a first load; `resolveModelQuery` -> `fetchModelIds` (`session_commands.zig:1290-1309`) | first load / one-shot | same |
| `/model <q>` that held rows do not match | `fetchModelIds` once more (`resolveModelQueryFromIds` returns null, `:1425-1448`) | one-shot | **remaining divergence**: xfx goes as typed (parity row) |
| the 2 s wait for a failed catalog's retry (`await_failed_catalog_retry`) | `resolveForRequest` on `clockMs` (`model_cache_runtime.zig:843-853`) | none (waits) | **remaining divergence**: xfx counts awake time (tokio), parity row |

**Which asks still read:** a read starts only for an ask whose trigger is set
and due -- MENU/REVALIDATE (`picker_open`), ACTIVITY (prompt or steer, column
frame, resume, fork) and AWAITED (the column with no answer yet: activity,
which over nothing held is the first load) -- or as a first load (launch,
provider switch, a turn or `/model <q>` that needs one). TOLD starts no
refresh and no retry, whatever is held; over nothing read at all it starts the
first load the runtime owes anyway (`trigger_over`, `src/tui/worker.rs`), as
fx's warmup on a provider change is started whatever asks. Before,
`/settings` and the speed commands sent AWAITED, but only while the band held
no answer, when the runtime's own first load is on its way or about to start,
so no extra read was reachable from them; the split makes that structural.
Its first version told an ask put aside behind a provider switch that the
new provider had no rows (`apply_deferred` answers it before `turn_loop`
starts the first load): `6ab25d2a`, RED `red-first-load.txt`
(`no first load for the ask`).

**Idle never fetches.** None of the sites above runs on a timer: `turn_loop`'s
`begin_catalog_read` is a first load only, and every other read is an ask the
band sends from a key, a submit, a resume or a frame it owes. Unit tests drive
the clock with no ask: `an_idle_selector_reads_nothing_whatever_its_clock_does`
(the machine running an hour, asleep 8 h, set back 30 s, three times) and
`the_catalog_is_read_by_a_first_load_the_menu_and_activity_only_and_never_while_idle`
(the same, through `turn_loop`'s own call, then each ask counted after it has
settled) (`green.txt`). The band asks nothing on an idle tick
(`the_column_drawn_over_a_failed_catalog_asks_again_from_its_own_frames_once_its_hold_is_over`,
"an idle tick asked", unchanged). A fakegw capture idle past 5 min of a fake
clock is not possible on a real binary (its clock is the machine's), so this
rests on the tests and the table.

## The clock (`clock-sources.txt`, `clocks.txt`)

- Zig 0.16.0's std (fx pins `minimum_zig_version = "0.17.0"`; 0.16.0 is the
  only std here): `boot` is `CLOCK_MONOTONIC_RAW` on Apple, `CLOCK_BOOTTIME` on
  Linux, `CLOCK_MONOTONIC` on other POSIX systems (`Io.zig:749-763`,
  `Io/Threaded.zig:14087-14108`).
- Rust 1.99.1's `Instant` is `CLOCK_UPTIME_RAW` on Apple, `CLOCK_MONOTONIC`
  elsewhere (`std/src/sys/time/unix.rs:53-70`).
- Darwin's `clock_gettime(3)`: `CLOCK_UPTIME_RAW` increments "in the same
  manner as CLOCK_MONOTONIC_RAW, but that does not increment while the system
  is asleep"; `CLOCK_MONOTONIC` "will continue to increment while the system
  is asleep".
- So xfx's `CatalogClock` reads `CLOCK_MONOTONIC_RAW` (Apple) or
  `CLOCK_BOOTTIME` (Linux) through `libc`, already a direct dependency
  (`Cargo.toml`, `[target.'cfg(unix)'.dependencies]`); nothing added. Off unix
  it counts from the first reading on `Instant`'s clock.
- This machine has not slept since it started (`clocks.txt`: `MONOTONIC_RAW -
  UPTIME_RAW = 0.000 s`), so no live reading here tells the two apart; the
  behaviour is shown on a `FakeMachine` the tests put to sleep, and the
  platform test pins the clock id (`the_system_catalog_clock_is_the_one_that_counts_sleep`).
- Catalog times are their own type (`CatalogTime`), so the compiler names
  every place an `Instant` stood for one; the band and the runtime decide by
  one predicate (`CatalogDue { since, at }.is_due`), the runtime telling the
  band the last read's start beside each due time.

## RED -> GREEN

- Triggers and the earlier reading, at `9ffb6310` (`red-triggers.txt`), 4 of 4
  failing: `a_resume_or_a_fork_is_activity_for_the_catalog` (`left: []`,
  `right: [("activity", false)]`);
  `settings_and_the_speed_commands_wait_for_the_catalog_and_never_ask_for_a_read`
  (`left: [("activity", true)]`, `right: [("told", true)]`);
  `a_reading_earlier_than_the_last_reads_start_is_due_whatever_held_it_back`
  and `a_reading_earlier_than_the_last_reads_start_owes_the_catalog_a_read`.
- The clock, at `2810f53c`'s tree with `CatalogClock::now` reading the awake
  clock -- what the base's `Instant` reads; the base has no clock to inject
  (`red-clock.txt`, the edit at its top): 5 of 8 failing, among them
  `rows_read_before_the_machine_slept_are_stale_after_it`,
  `a_failures_hold_ends_while_the_machine_sleeps` and
  `rows_and_a_failure_read_before_the_machine_slept_are_due_on_waking`. The
  platform test passes under it, as `clocks.txt` predicts.
- The first load an ask with no trigger owes, at `bc83ad25` with the new test
  added (`red-first-load.txt`): `no first load for the ask`.
- GREEN at `6ab25d2a`: every RED test above, the clock's, the counts' and the
  band's idle-tick test, 14 passing (`green.txt`).

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` of the branch in the scratch directory, each under
`bounded.py` (300 s, 3 GiB), the edit undone after. MUTANTS_SUMMARY

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`--only p5-input-model* model* h1-models* settings-model*`, 20 scenarios, 92
pairs, base `e705a5dc` (banner `v0.1.0-e705a5d`) against `6ab25d2a`
(`v0.1.0-6ab25d2`), fx's sha checked before and after each run: 77 / 92
both, **0 pairs moved**.
The 15 that fail in both are the same pairs: `sound-max` (6), the C7 help
line (1), activity tokens (1), and fuzzy search (`model-anchor/alt-enter`,
`p5-input-model` `q-*`, `inline-gpt6`: MODEL-FUZZY, a unit in flight).

**Oracle.** No capture pair applies: nothing on screen changes. The column
already asked at the base, as fx's does; `/settings` and the speed commands
wait exactly as before (their ask goes only while the band holds no answer,
and TOLD then starts the same first load AWAITED did); a resume's
or fork's read paints only if the catalog changed; and the clock shows only
across a machine sleep, which a capture cannot take. The sweep above is the
check that nothing moved.

## Gate (`gates.sh`, `gate.txt`)

GATE_SUMMARY

## Security

- Nothing new reads a credential, writes a file or starts a process; the
  catalog read is CATALOG-BOUNDS' `GatewayCatalog::fetch`, unchanged.
- New network requests: a resume or fork over rows five minutes old, or a
  failure past its hold, reads once -- fx's activity. `/settings`, `/fast` and
  `/ultrafast` can no longer start a refresh or a retry. After a machine sleep
  the first ask reads
  a catalog the sleep aged, at most once per ask and never inside a failure's
  backoff; no read without an ask.
- `clock_gettime` is called with a constant id into a local `timespec`; a
  failed call reads as the clock's origin, which is due -- never a catalog
  held fresh (`catalog_clock.rs`, `system::read`).
- Nothing model- or file-controlled reaches the screen in this unit.
