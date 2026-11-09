# catalog-refresh: which asks read the model catalog, and a clock that counts sleep

**Rounds 2 to 5** are at the end (§ Round 2 to § Round 5); the
sections between are round 1's, with later rounds' corrections made in place
where a sentence was wrong.

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
- The shared fixture's environment (round 3), at `d67326e9`
  (`red-fixture.txt`): `recording()'s runtime holds a home other than the
  test's own` -- the homes compared, neither printed.
- `renew`'s test seam and the fixture's variables (round 4), at `08cfdef0`
  (`mutants-before.txt`): a switch restoring the old provider's catalog state
  and a fixture holding a variable both survive (§ Round 4).
- GREEN at `bd67c79e` (code outside tests at `b2731faa`; since, comments and
  `#[cfg(test)]` lines): every RED test above, the clock's, the counts',
  rounds 2's to 4's, and the band's idle-tick test, 22 passing (`green.txt`).
  The source since differs by round 5's comment lines alone
  (`src-since-gate.txt`).

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` of the branch in the scratch directory, each under
`bounded.py` (300 s, 3 GiB), the edit undone after. Since round 2 the script
counts four verdicts apart -- killed by an assertion, does not build, timed
out, survived -- (round 3: a fifth, failed without a panic) and keeps each kill's failing tests with their panic's
place and first message line.

Round 4 adds two (§ Round 4), 25 in all. At `70fbfa1d` (code as at the head): **23 mutants -- 23 killed by an
assertion, 0 do not build, 0 timed out, 0 failed without a panic, 0
survived** (`mutants.txt`). Round 1's sixteen, with the two whose anchors
moved rewritten (the awake clock now `Some(..)`; `/settings`' comment); four
of round 2's: a failed reading as the clock's latest instead of its origin,
a switch's selector on the system clock, and the system clock in place of
the injected one at the menu over a failure and at the column's frame; and
three of round 3's: a switch keeping the old provider's catalog state and
read, a switch letting the old read run on, and the shared fixture back on
the process's environment. The "Instant restored" mutant swaps both arms of
`CatalogClock::now` and is killed through the fake arm (§ The clock). Round
1's first run, at `7fa5d9c2`, left one alive -- "the runtime reads for an ask
with no trigger" (`mutants-r1.txt`): the count test read the fetch counter
before the read's thread had reached it; `fccaf848` settles each ask first.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`--only p5-input-model* model* h1-models* settings-model*`, 20 scenarios, 92
pairs, base `e705a5dc` (banner `v0.1.0-e705a5d`) against `3d96f590`
(`v0.1.0-3d96f59`, round 2's code), fx's sha checked before and after each
run: 77 / 92 both, **0 pairs moved**. After `3d96f590` the code outside
tests changes by +7 -1 (`src-since-sweep.txt`), every line of it under
`#[cfg(test)]` -- round 4's `renew` test seam and `Environment::vars_for_test`
-- and the rest is comments and tests, so the swept binary's behaviour is the
head's. The 15 that fail in
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

Round 4's, at `bd67c79e`, under the suite lock: **20 steps, 0 failed** --
fmt, both clippys, `cargo test` (15 binaries, 5489 passed, 0 failed, 7
ignored) and `--features fault-injection` (5526 passed, 0 failed, 7
ignored), six `check-*` scripts and three `test-*` ones, both fxcap
self-tests, both release builds, smoke, and smoke-tui (37 scenarios and the
oracle, 1168 checks, 0 failures). The counts are round 3's: round 4 adds
assertions, not tests. "tree clean: no" is this README and the receipt files
written while the gate waited for its slot and ran. After the gate's head,
`src/` changes in round 5's comment lines alone (`bb67a055`;
`src-since-gate.txt` holds the diff), and every other commit is this
folder's.
Round 3's gate (`3a0bcdb1`, 5489 and 5526 passed), round 2's (`38ba66a7`,
5487 and 5524) and round 1's (`b57fa272`, 5482 and 5519) are replaced by this
one.

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

**Round 2's figures** were GREEN 20 of 20 and mutants 20 of 20 at
`ac3814fe`, the sweep at `3d96f590` (77 / 92, 0 moved), and the gate 20 of
20 at `38ba66a7` (5487 and 5524 passed). The files now hold round 3's
(§ Round 3), except the sweep, which still stands (§ Sweep).

## Round 3

Round 2 (`9a49c58e`) had two confirming reviews: gpt6 APPROVE-WITH-FIXES
with one MUST, opus APPROVE-WITH-FIXES with none. Both confirmed items 1-9.

| # | item | commit | evidence here |
|---|---|---|---|
| 1 | the parity row's bold "**After a failure, nothing reads it ... whatever asks**" qualified, for readings that succeed and do not go back past the last read's start; the same promise in four source comments (`LoadClock::not_before`, `would_load`, `activity_due`, `CatalogKnown::Unread`) | `d2497396` | a grep of the unit's files for "never inside", "whatever asks" and the like, test comments about good readings left as they are |
| 2 | "Two differences remain" -> three, the failed reading's cost first | `d2497396` | `docs/parity.md` `/model` |
| 3 | GREEN cites the commit `green.txt` was taken at | this one | § RED -> GREEN |
| 4 | `recording()` over a temporary environment | RED `d67326e9`, fix `44d7f84c` | `red-fixture.txt`; `green.txt`; `mutants.txt` ("the shared fixture's runtime reads the process's environment") |
| 5 | a switch drops the old provider's rows, its read on the way and a failure's hold, refuses that read's late answer, and keeps the clock | `f3e73339`, `26375df5` | `green.txt` (`a_switch_drops_the_old_providers_rows_read_and_backoff_and_keeps_the_clock`); `mutants.txt` |
| 6 | the decision below | this one | -- |
| 7 | `mutants.py` labels a nonzero run with no panic apart ("failed without a panic"); parity names the band-thread failed reading | `70fbfa1d`, `d2497396` | `mutants.py`; `docs/parity.md` |

**Item 4 found the defect it was asked about**, so it has a RED: the shared
fixture's runtime held the process's environment. The fixture now names an
environment over its own home, so every test built on it reloads only that
home. The crate-wide guard (`Runtime::new` under `cfg(test)`) is the queued
TEST-ENV-ISOLATION unit, not this one. **Item 5 passed first time**: `renew`
already rebuilt the whole selector. Two mutants show the test fails when it
does not -- `renew` keeping the old catalog state and read, and `renew`
letting the old read run on (its gate still open after the switch). (Round 4
corrected this: the rows and failure checks rested on the test seam, and those
two mutants died at the hold and read checks; § Round 4.)

**Decision (the dispatcher's, 2026-11-09, round 2 opus SHOULD 4):** the
alternating-failure pin shows xfx's read count -- seven, three more for four
failed readings -- with fx's named as a difference (`last_attempt_ms = now`,
`model_cache_runtime.zig:636@481104fd`, so a failed ask reading costs fx two
reads and xfx one), rather than copying fx's count as clock-gpt6.md asked.
xfx's makes fewer requests under a failed clock reading, the safer side for
an S unit; the difference is unreachable with a working clock, and it is
named in `docs/parity.md`.

**Round 3's figures:** GREEN 22 of 22 at `70fbfa1d` (`green.txt`); the
fixture RED at `d67326e9` (`red-fixture.txt`); mutants 23 of 23 killed at
`70fbfa1d` (`mutants.txt`); the gate 20 of 20 at `3a0bcdb1` (5489 and 5526
passed, smoke-tui 1168 checks, `gate.txt`); the sweep still at `3d96f590`
(77 / 92 both, 0 moved; code outside tests since, +0 -0,
`src-since-sweep.txt`); the size against `e705a5dc`, +515 -135 in all and
+299 -98 in code outside tests (`size.txt`).

The files now hold round 4's, except `mutants.txt`, the last run of every
mutant, and the sweep (§ Round 4).

## Round 4

Round 3 (`86b86004`) had two confirming reviews: gpt6 APPROVE-WITH-FIXES with
one doc MUST, opus APPROVE-WITH-FIXES with none (two SHOULDs, nits). The
dispatcher's gate on `86b86004` passed 21 of 21.

| # | item | commit | evidence here |
|---|---|---|---|
| 1 | the parity row's "Inside a failure's hold `/model` ... asks nothing" and "asked again only at the next ask after the backoff" qualified, and two more of its sentences pointed at the rule (the column's frame, the retryable sentences); in the source comments, every line saying "backoff", "hold" or "asks nothing" read, and each promise of no read inside a hold, or closed list of when a read is due, among them qualified -- outside tests and in them (closed lists written without those words: § Round 5) | `08cfdef0` | `hold-sentences.txt`: every sentence checked, each with its decision (`sentences.py` lists them) |
| 2 | `CatalogKnown::Unread`: the hold holds for readings not earlier than the failed read's start; an earlier one is due at once -- said apart | `08cfdef0` | `src/tui/shell.rs` |
| 3 | `renew`'s test seam turns only `Unavailable` into `NotLoaded` | RED run at `08cfdef0`, recorded in `4dd1bb65`; fix `bd67c79e` | `mutants-before.txt`; `mutants-r4.txt` |
| 4 | `fresh == 0` after both switches; the seeded hold pinned with `retry_at()`; the fixture guard compares the environment's variables with an empty map, printing neither | RED (the variables) run at `08cfdef0`, recorded in `4dd1bb65`; fix `bd67c79e` | `mutants-before.txt`; `mutants-r4.txt`; `green.txt` |

**Item 3 had the hole the review named.** At `08cfdef0`, a mutant restoring
the old provider's `catalog_state` after `Self::new` survived
(`mutants-before.txt`): the seam set `NotLoaded` over whatever was there, so
the switch test's rows and failure checks held whatever `renew` kept -- round
3's "item 5 passed first time" rested on its hold and read checks alone. The
seam now changes only `Unavailable`, the state `Self::new` builds for a
provider with no catalog (`catalog_for`, `model.rs:1085-1104`): the Gateway's
needs a credential, read only from the environment's variables
(`resolve_credential`, `config.rs:2262-2275`), and the switch tests' are an
empty map. So the deferred-TOLD test passes for the reason it gives -- the
switch builds `Unavailable`, the test's catalog stands in for the provider's,
and the one read is counted through it -- and the restoring mutant now dies at
the switch test's first check. The fixture guard's variables check had the
same kind of gap: a fixture holding a variable it was not given survived it
(`XFX_MUTANT`, a name xfx never reads -- no real variable is read).

**Mutants for items 3 and 4** (`mutants-r4.txt`, at `bd67c79e`, the six
whose names hold "switch" or "shared fixture"): **6 killed**, 0 do not build,
0 timed out, 0 failed without a panic, 0 survived. The restoring mutant and
round 3's "keeps the old provider's catalog state and read" now both die at
the switch test's first check (`worker.rs:17018`, `NotLoaded`); the switch's
clock and read mutants at "the clock lost" and "the old provider's read was
still waiting"; the two fixture mutants at the guard's home and variables
checks (`worker.rs:10086`, `:10092`). The other 19 last ran at `70fbfa1d`
(`mutants.txt`, 23 of 23); every one of the 25 edits' texts still stands, as
many times as the script says, at `bd67c79e` -- round 4 changed comments
beside them and tests, not the lines they edit.

**Gate** (`gate.txt`): 20 of 20 at `bd67c79e`, 5489 and 5526 passed,
smoke-tui 1168 checks and 0 failures (§ Gate).

**Round 4's figures:** GREEN 22 of 22 at `bd67c79e` (`green.txt`); the two
REDs at `08cfdef0` (`mutants-before.txt`, 2 survived); mutants 6 of 6 killed at `bd67c79e` (`mutants-r4.txt`), the 25 edits'
texts all standing there; the gate 20 of 20 at `bd67c79e` (5489 and 5526
passed, smoke-tui 1168 checks, `gate.txt`); the sweep
still at `3d96f590` (§ Sweep); the size against `e705a5dc`, +548 -144 in all
and +305 -98 in code outside tests (`size.txt`), the six more code lines
`#[cfg(test)]`.

## Round 5

Round 4 (`487a76cd`) had two confirming reviews: gpt6 APPROVE, opus
APPROVE-WITH-FIXES with one SHOULD and receipt nits. The dispatcher's gate on
`487a76cd` passed 21 of 21. Docs and receipt only: nothing executable
changes, so no gate and no mutants ran.

| # | item | commit | evidence here |
|---|---|---|---|
| 1 | `LoadTrigger::Activity`'s "read again only once ..." names a failure's hold and the earlier reading (`LoadClock::next`, `CatalogDue`); so do `::PickerOpen`'s and the same closed lists in the refresh failure's event and handler, the menu's (`use_model`), the column's (`settle_column`), a resume's, and the control message's (`TurnControl::Catalog`, which now names TOLD's ask too) | `bb67a055` | `src-since-gate.txt`: the diff, comment lines only |
| 1b | found by the same search: six lists of the model cache's triggers that left out a resume or a fork, activity since round 1 (`04a1ed4e`) -- `TurnControl::Catalog`, `UiEvent::CatalogUnread`, the launch read's comment and `ask_catalog`, the band's `catalog_due` and `note_catalog_activity` -- and the model tests' `Asked` | `bb67a055` | the same |
| 2 | the search widened to the closed-list words, each sentence decided; round 4's item 1 now says what round 4 searched | this one | `hold-sentences.txt` (round 5's section), `sentences.py` |
| 3 | the RED cells name the run's commit and the one that recorded it; `hold-sentences.txt` regenerated after this README's last edit; the 372 recorded; GREEN and the gate at `bd67c79e`, and what `src/` changes since | this one | `unit-tests.txt`, `src-since-gate.txt` |

**Item 2: widened, and the claim says what was searched.** Every comment
sentence of the seven files, and every sentence of the parity row, that says
"only", "not before", "never" or "once" and also names the catalog's reads --
the catalog, a refresh, a retry, a read again, due, a trigger, the model
cache, a first load, a backoff or a hold -- each decided (`hold-sentences.txt`,
round 5's section): 176 sentences, 93 outside tests, 66 in the
test modules and 17 in the row: 13 qualified in this round, 20 qualified
before (13 in round 4, 7 in round 3), 7 that say a read comes once a hold is
over or `retry_at` has come (the hold being `CatalogDue`'s, which carries the
rule), 8 the due rule itself or a pointer to it, 7 the ask with no trigger
(no refresh, no retry, whatever the clock reads), 6 the line-mode
`ensure_catalog` or `/model <query>`, 10 a test's own readings, 1 the
earlier-reading case itself, 3 quotes of fx, and 101 no rule of when the
catalog is read. The four words alone, with no word for the
catalog's reads in the sentence, are on 1551 comment lines of these files,
too many to decide line by line; the search stops at the sentences that name
the catalog's reads, and this paragraph is the claim.

**Item 3.** The RED for items 3 and 4 ran at `08cfdef0`
(`mutants-before.txt:1`) and was recorded in `4dd1bb65`; the table says both.
The 372 is round 4's run of the unit's test modules at `bd67c79e`
(`unit-tests.txt`). GREEN, that run and the gate ran at `bd67c79e`;
`git diff --stat bd67c79e..487a76cd -- src` prints nothing, and
`bd67c79e..bb67a055` changes the four files of round 5's comment commit, 0
of the changed lines anything but a comment (`src-since-gate.txt`, with the
whole diff).

**Checks** at `bb67a055` with this folder's edits (`checks-r5.txt`): fmt, clippy and clippy with fault
injection, as `gates.sh` runs them, exit 0 under the suite lock;
check-no-secrets and check-xfx-identity exit 0. check-no-local-paths is not
on this branch or on main at `32afd307`: run unmodified from
`fix/p5-local-paths` (`5743e3c7`), it exits 1 on 341 lines in 161 files,
every one of them tracked at the base already -- the LOCAL-PATHS unit's
sweep. The one file of this unit's 44 among them is `src/tui/shell.rs`, at
three lines that base commits wrote (`666e0b2d`, `1e2a627b`); no line this
unit wrote holds a local path, as the per-commit grep says.

**Round 5's figures:** source outside tests against `e705a5dc`, +597 -169 in
all and +305 -98 in code (`size.txt`); since the swept binary, code +7 -1,
all `#[cfg(test)]` (`src-since-sweep.txt`); round 4's GREEN, mutants and
gate stand, at `bd67c79e`.
