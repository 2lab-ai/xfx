# catalog-refresh: which asks read the model catalog, and a clock that counts sleep

**Round 2** is at the end (§ Round 2); the sections between are round 1's,
with the round-2 corrections made in place where a sentence was wrong.

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
| `6ab25d2a` | fix: an ask with no trigger over nothing read still starts the first load (found writing this receipt; RED in `red-first-load.txt`) -- round 1's code head and swept binary |
| `a1c36a6e` | `docs/parity.md`: that first load, in the `/model` row |
| `b57fa272` | round 1's receipt and gate head |
| `1703a4a4`, `425e7c52`, `9b93b619`, `484fb30f` | a comment in `system::read`; round 1's gate and mutants; its size line; this table -- round 1's head |
| round 2's commits | § Round 2 |

Round 1's figures -- its sweep at `6ab25d2a`, its gate at `b57fa272`, its 16
mutants -- were replaced by round 2's, at round 2's head (§ Round 2); the
files here are round 2's unless named otherwise (`mutants-r1.txt`,
`red-*.txt` at their own commits).

Source outside tests, base to round 2's head: `size.txt` (`size.py`; round
1's was every line +439 -125, code +266 -95) -- past the brief's ~400-line
mark with its doc comments, under it without. The dispatcher has split the
review of it into the triggers and the clock.

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
| `/model` menu, the settings Model row, C-p: all through `open_models_with` (`read-sites.txt`), MENU / REVALIDATE | `openModelMenu` -> `revalidateModelCache` (`app_input_runtime.zig:1897-1908`, `:2062-2066`, `:2104-2107`) | picker | same |
| `/settings` | `settingsCatalogSnapshot` (no load) | no refresh and no retry; it may start the first load nothing has made | was AWAITED (activity) with no answer yet; now TOLD |
| `/fast`, `/ultrafast on` | `resolveForApp` (no load) | no refresh and no retry; it may start the first load nothing has made | the same, fixed |
| resume from the picker, `/fork` | `startResumedSessionWarmup` | stale/failed | **xfx asked nothing; now ACTIVITY** |
| the line shell's bare `/model` (`interactive.rs:2151`, `apply_model` -> `ensure_catalog`) | none: fx's line mode is not this TUI | first load only | xfx's own surface; reads nothing once loaded |
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
  Linux, `CLOCK_MONOTONIC_FAST` on FreeBSD and DragonFly, `CLOCK_MONOTONIC` on
  other POSIX systems (`Io.zig:749-763`, `Io/Threaded.zig:14087-14108`).
- Rust 1.99.1's `Instant` is `CLOCK_UPTIME_RAW` on Apple, `CLOCK_MONOTONIC`
  elsewhere (`std/src/sys/time/unix.rs:53-70`).
- Darwin's `clock_gettime(3)`: `CLOCK_UPTIME_RAW` increments "in the same
  manner as CLOCK_MONOTONIC_RAW, but that does not increment while the system
  is asleep"; `CLOCK_MONOTONIC` "will continue to increment while the system
  is asleep".
- So xfx's `CatalogClock` reads `CLOCK_MONOTONIC_RAW` (Apple) or
  `CLOCK_BOOTTIME` (Linux) through `libc`, already a direct dependency
  (`Cargo.toml`, `[target.'cfg(unix)'.dependencies]`); nothing added. Other
  unix systems read `CLOCK_MONOTONIC` (Zig: `MONOTONIC_FAST` on FreeBSD and
  DragonFly). Off unix it counts from the first reading on `Instant`'s clock,
  which does not count sleep -- but no workflow builds a non-unix target
  (`targets.txt`: CI, release and preview build Linux and Apple only), so no
  parity row names it.
- **A failed reading is the clock's origin, as fx's is.** Zig 0.16.0's
  `nowPosix` returns `.zero` when `clock_gettime` fails
  (`Io/Threaded.zig:11428-11435`, quoted in `clock-sources.txt` beside fx's
  `clockMs` and its rule "callers treat a reading earlier than a stored one as
  due", `model_catalog.zig:72-79`); 0.16.0 only -- fx pins 0.17.0, which is
  not on this machine. xfx maps a failed call to `Duration::ZERO` in one place
  for both the system and the fake clock (`CatalogClock::now`). The parity pin
  `a_failed_clock_reading_costs_at_most_one_read_and_none_without_an_ask`
  alternates failed and good readings on the fake machine -- an ask's, a
  read's start, a failed refresh's answer, a successful one's -- and counts
  seven reads where four failed readings cost three more, and none with no
  ask (`green.txt`). fx's count differs in one place: it stamps a read's start
  with the ask's own reading (`last_attempt_ms = now`, `:636`), so a failed
  ask's reading costs it two reads where xfx's, which stamps the start with
  a reading of its own, costs one (`docs/parity.md`).
- This machine has not slept since it started (`clocks.txt`: `MONOTONIC_RAW -
  UPTIME_RAW = 0.000 s`), so no live reading here tells the two apart; the
  behaviour is shown on a `FakeMachine` the tests put to sleep (one lock over
  its clocks since round 2). **The "Instant restored" mutant is killed through
  the fake arm**: it swaps both arms to the awake clock, and the fake machine's
  sleep is what the tests see. The system arm is pinned only by the platform
  test's clock-id assertion
  (`the_system_catalog_clock_is_the_one_that_counts_sleep`), the man page and
  Zig's docs -- this machine cannot show it counting sleep.
- Catalog times are their own type (`CatalogTime`), so the compiler names
  every place an `Instant` stood for one; the band and the runtime decide by
  one predicate (`CatalogDue { since, at }.is_due`), the runtime telling the
  band the last read's start beside each due time.

## RED -> GREEN

- Triggers and the earlier reading, at `9ffb6310` (`red-triggers.txt`), 4 of 4
  failing: `a_resume_or_a_fork_is_activity_for_the_catalog` (`left: []`,
  `right: [("activity", false)]`);
  `settings_and_the_speed_commands_wait_for_the_catalog_and_never_ask_for_a_read`
  (renamed `..._never_ask_for_a_refresh` in round 2)
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
- The selector a provider switch builds (round 2), at `bcc278d0`
  (`red-switch.txt`): `left: CatalogTime(923554.366891291s)`,
  `right: CatalogTime(3600s)` -- the system clock, not the injected one.
- GREEN at `3d96f590` (code at `b2731faa`): every RED test above, the
  clock's, the counts', round 2's, and the band's idle-tick test, 20 passing
  (`green.txt`).

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` of the branch in the scratch directory, each under
`bounded.py` (300 s, 3 GiB), the edit undone after. Since round 2 the script
counts four verdicts apart -- killed by an assertion, does not build, timed
out, survived -- and keeps each kill's failing tests with their panic's
place and first message line.

At `ac3814fe`: **20 mutants -- 20 killed by an assertion, 0 do not build, 0
timed out, 0 survived** (`mutants.txt`). Round 1's sixteen, with the two
whose anchors moved rewritten (the awake clock now `Some(..)`; `/settings`'
comment), and four of round 2's: a failed reading as the clock's latest
instead of its origin, a switch's selector on the system clock, and the
system clock in place of the injected one at the menu over a failure and at
the column's frame (item 5). The "Instant restored" mutant swaps both arms of
`CatalogClock::now` and is killed through the fake arm (§ The clock). Round
1's first run, at `7fa5d9c2`, left one alive -- "the runtime reads for an ask
with no trigger" (`mutants-r1.txt`): the count test read the fetch counter
before the read's thread had reached it; `fccaf848` settles each ask first.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`--only p5-input-model* model* h1-models* settings-model*`, 20 scenarios, 92
pairs, base `e705a5dc` (banner `v0.1.0-e705a5d`) against `3d96f590`
(`v0.1.0-3d96f59`, round 2's code), fx's sha checked before and after each
run: 77 / 92 both, **0 pairs moved**. After `3d96f590` only a test changes
under `src/` (`ac3814fe`, in `worker.rs`'s tests module). The 15 that fail in
both are the same pairs: `sound-max` (6), the C7 help line (1), activity
tokens (1), and fuzzy search (`model-anchor/alt-enter`, `p5-input-model`
`q-*`, `inline-gpt6`: MODEL-FUZZY, a unit in flight).

**Oracle.** No capture pair applies: nothing on screen changes. The column
already asked at the base, as fx's does; `/settings` and the speed commands
wait exactly as before (their ask goes only while the band holds no answer,
and TOLD then starts the same first load AWAITED did); a resume's
or fork's read paints only if the catalog changed; and the clock shows only
across a machine sleep, which a capture cannot take. The sweep above is the
check that nothing moved.

## Gate (`gates.sh`, `gate.txt`)

Round 2's, at `38ba66a7`, under the suite lock: **20 steps, 0 failed** --
fmt, both clippys, `cargo test` (15 binaries, 5487 passed, 0 failed, 7
ignored) and `--features fault-injection` (5524 passed, 0 failed, 7
ignored), six `check-*` scripts and three `test-*` ones, both fxcap
self-tests, both release builds, smoke, and smoke-tui (37 scenarios and the
oracle, 1168 checks, 0 failures). "tree clean: no" is this README, edited
while the gate waited for its slot and ran; nothing under `src/` changed
after `ac3814fe`, and the commits after the gate's head are this folder's.
Round 1's gate (`b57fa272`, 20 of 20, 5482 and 5519 passed) is replaced by
this one.

## Security

- Nothing new reads a credential, writes a file or starts a process; the
  catalog read is CATALOG-BOUNDS' `GatewayCatalog::fetch`, unchanged.
- New network requests: a resume or fork over rows five minutes old, or a
  failure past its hold, reads once -- fx's activity. `/settings`, `/fast` and
  `/ultrafast` can no longer start a refresh or a retry. After a machine sleep
  the first ask reads a catalog the sleep aged, at most once per ask and --
  for readings that succeed and do not go backwards -- never inside a
  failure's backoff, since a reading earlier than the last read's start is
  due whatever held the read back. No read without an ask.
- `clock_gettime` is called with a constant id into a local `timespec`, an id
  each of these platforms defines (`clock-sources.txt`, libc's constants), so
  it is not expected to fail there. Were every call to fail, every reading would be the
  origin: the first load would still happen, but no refresh would come due,
  for activity or the menu, and a failure's hold would never end; one failed
  call among good ones reads as earlier than the last read's start, which is
  due (`catalog_clock.rs`, `system::read`).
- Nothing model- or file-controlled reaches the screen in this unit.

## Round 2

The dispatcher split round 1 (`484fb30f`) in two -- the triggers and the
clock -- and had each reviewed by two engines (gpt6: APPROVE and
APPROVE-WITH-FIXES; opus: APPROVE-WITH-FIXES and APPROVE). What each item
became:

| # | item | commit | evidence here |
|---|---|---|---|
| 1 | comments that said TOLD never reads (bridge, model, shell x2), the shell test's name, this README's two rows, the parity row's bold | `b2731faa`, `2094935a`, this one | the trigger table above |
| 2 | Zig's `nowPosix` beside fx's rule; "never inside a failure's backoff" qualified; "does not fail" narrowed | `3d96f590`, `b2731faa`, this one | `clock-sources.txt`; § Security |
| 3 | parity pin: alternating failed and good readings, read count | `e88e0da8` | `green.txt` (`a_failed_clock_reading_costs_at_most_one_read_and_none_without_an_ask`) |
| 4 | `FakeMachine`: one lock (`Mutex<Machine>`) | `e88e0da8` | `catalog_clock.rs` |
| 5 | the menu over `Unread` and the column's frame: negative halves, and a system-clock mutant at each | `ce25440d` | `green.txt`; `mutants.txt` |
| 6 | a switch's selector keeps the catalog's clock (`ModelSelector::renew`) | RED `bcc278d0`, fix `27aa8000` | `red-switch.txt` (`left: CatalogTime(923554.366891291s)`, `right: CatalogTime(3600s)`); `green.txt` |
| 7 | a deferred TOLD through `run_setup` -> `apply_deferred` -> `begin_catalog_read` | `f724d5ad`, corrected `ac3814fe` (below) | `green.txt` (`a_told_ask_put_aside_behind_a_switch_...`) |
| 8 | `mutants.py`: killed / does not build / timed out / survived, each kill's test and panic line | `3d96f590` | `mutants.txt` |
| 9 | the pin's `app_commands.zig:3230-3246`; the line shell's row; `trigger_over`'s doc; FreeBSD and DragonFly; the unused clock read in `AfterWork`; the fake-arm note; non-unix targets | `b2731faa`, this one | the trigger table; § The clock; `targets.txt` |

Items 3, 5 and 7 found nothing to fix in the source: each passed on its first
run, and the mutants below show each fails when the rule it pins is undone.
Item 6 changed behaviour, so it has a RED. The "Not this unit" item
(`/model <q>` over a failed catalog, then Ctrl-C) is untouched.

**Item 7's first version read the process's own profile.** It built its
runtime with `recording()`, whose `Runtime::new` takes
`Environment::from_process()`; `run_setup` reloads the profile through that
environment (`select_provider`), so the reload read the real home's
profile -- and any credential variable in the test process's environment --
into memory, in every run of that version (`f724d5ad` to
`3d96f590`: its own runs, one module-wide test run, round 2's first mutant run
and a GREEN run). By the code path, nothing was written there:
the setup writes the configuration's own settings path, a temporary home's,
and the one other write on that path, `save_speed`, runs only with a speed
switch on, which no test turned on. And no socket was opened: the selector
the switch built read the fake catalog the test handed it, and nothing else
on that path reads the network. One value from that profile -- its provider's name -- reached this
session's log, through a probe I added and removed to find out why the
"switch" ended on llmux. `ac3814fe` builds the runtime with a named
environment over a temporary home seeded on llmux with no daemon named, and
switches to the Gateway, whose setup opens no socket; it asserts the reload
read that home and the provider changed. The first version's runs counted as
evidence for nothing here: `green.txt` and `mutants.txt` are from
`ac3814fe` on.

**Round 2's figures:** GREEN 20 of 20 at `ac3814fe` (`green.txt`); mutants
20 of 20 killed by an assertion at `ac3814fe` (`mutants.txt`); sweep 77 / 92
on base and head, 0 pairs moved, the head binary `v0.1.0-3d96f59`
(`sweep-head.md`, `sweep-compare.txt`); gate 20 of 20 at `38ba66a7`
(`gate.txt`); source outside tests, base to head, in `size.txt`.

