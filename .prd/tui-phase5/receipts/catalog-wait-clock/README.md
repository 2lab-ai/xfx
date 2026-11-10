# catalog-wait-clock: the catalog-wait stop tests prove "at once" without a 1 s window

Unit CATALOG-WAIT-CLOCK: test-only -- `src/tui/worker.rs`'s tests module and
one `[dev-dependencies]` line in `Cargo.toml`; no product line changes. Base
`afd891af` (train 13's head). Every claim below is tied to a file in this
folder. Source lines are at `afd891af` unless marked `@a6ab3ee6`,
`@9d2e7f3c` or `@6073cdc5`.

| commit | what |
|---|---|
| `3c0a2ab3` | round 1: the stop placed by the loop; first loads held until the stop's end; the retry bound at 2 s on the wall clock |
| `b5e234e1`, `5f55cfc8` | round 1: the retry runs on tokio's paused clock (`test-util` in `[dev-dependencies]`), the stopped end asserted at zero |
| `64cfd5a1`..`d525cc7b` | round 1's receipt, gate and repeat check (files without a `2` in their name) |
| `a6ab3ee6` | round 2 (the reviews of `d525cc7b`): every stopped run on the paused clock, first loads included; no clock places a stop; the compaction test; two new first-load cases |
| `6b0c613f`, `b54cffb4` | round 2's receipt, gate and repeat check (files with a `2` in their name) |
| `ce221384` | round 3 (the reviews of `a6ab3ee6`): the wall-clock stop-to-end bound, at 5 s; `Held::run`; C's give-up at 30 s; the closed-channel comments |
| `9d2e7f3c` | round 3: the bound at 10 s, sized from `ce221384`'s repeat |
| `bbf15d6d`, `6f07dd89` | round 3's receipt and gate (files with a `3` in their name) |
| `6073cdc5` | round 4 (gpt6 on round 3): the watcher stopped by an unwind guard (`watched`), its stamp named an observation, both stamps required, two tests (F, G) -- **the code head** |
| the next commits | round 4's receipt (files with a `4` in their name, and this README), gate (`gates4.txt`) |

## The gap

- Gate `31ce4320` (STATUS 2026-11-10 08:52Z, load 65 to 106): the three tests
  failed at their final `assert_eq!` -- test 2, test-fi 3 -- and passed 4 of 4
  rerun on the same head. That gate's logs were later overwritten by another
  run on the same paths, so which field differed there cannot be read back;
  `red-base.txt` makes the failure on demand and shows it is the timing field
  alone.
- The bound, in all three: the stopped turn's end, counted from just before
  the loop began, in `[300 ms, 1 s)`, with the stop sent by a thread that
  slept 300 ms (`worker.rs:18736`, `:19054`, `:19354`). A machine that holds
  the loop's thread, or the stopper's, misses the window with the product
  right.

## The tests

A `a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number`;
B `a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one`;
C `a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`;
D `a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs`
(round 2: the same race, fixed here); E
`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`
(on the same harness; its assertions are unchanged). Round 4 adds F
`the_place_watcher_ends_when_the_run_it_watches_panics` and G
`a_late_watcher_moves_the_stop_to_end_reading_and_fails_it_only_past_the_bound`,
which test the harness's watcher.

## What each bound proved, and what proves it now

"With the stop: 300 ms to 1 s" meant **the stop ends the turn's catalog wait
at once, and the turn does not wait out the catalog.**

- **Where the stop lands: no clock.** A, B, D and E send the Ctrl-C from the
  loop's own thread when the band hears `TurnStarted { submission: 1 }`
  (`first_of_two_and_a_stop`, `worker.rs:18763@a6ab3ee6`; D at `:19990`).
  `tokio::join!(run, watch)` polls both on one task, so the band reads that
  event only after `run` has returned `Pending` since sending it; nothing
  awaits between the send and the body's first catalog wait
  (`worker.rs:3871-3891`; `one_turn` `:6124-6127`; `image_refusal`
  `:5850-5862`); `raced_against_control` reads control first (`:5742-5744`).
  C's band runs on a thread of its own (its questioner parks the loop's
  thread while a panel is up) and sends the stop when it reads the same
  event (`:19551@a6ab3ee6`): the stop may be read just before the wait or in
  it, and every catalog select takes it first (`biased`, `:4554`, `:4587`,
  `:5852`, `:6152`). No test places a stop by a clock any more, and none can
  send it before the loop has taken submission 1 (`Taken::Abandon`, the
  `select` stall below).
- **"At once": every stopped run is on tokio's clock, paused**
  (`on_a_paused_runtime`, `:7428@a6ab3ee6`), and its first turn's end is
  asserted at **no tokio time gone** (`:18931`, `:19292`; C over its whole
  run, `:19620`). The paused clock moves only when every task is parked,
  straight to the next timer, so a held thread is no time:
  - a retry's wait ends by itself on `FAILED_CATALOG_RETRY_WAIT`, tokio's 2 s
    timer (`worker.rs:4555`): a turn that waited it out ends at 2 s;
  - a first load's wait has no timer -- it is the catalog thread's oneshot
    (`PendingLoad::answer`, `src/provider/model.rs:1695-1722`, `:1815`) -- and
    the load stays held until the stopped turn has ended (`Held::FirstLoad`;
    C's band holds it), so a turn that ends at all was not ended by the load;
  - a stop honoured late on a tokio timer ends that much later, in either.
- **Why a paused clock is sound for the first-load runs** (gpt6's MUST,
  opus Q1). A paused runtime that waits on another thread runs its clock on
  to the next timer. So: (1) the band's give-up is no tokio timeout but a
  wall-clock deadline thread (`WallDeadline`, `:18752@9d2e7f3c`, 30 s), which
  a paused clock cannot jump to -- a first load's missed stop fails at that
  deadline instead of hanging, the give-up letting the load go. It covers
  first loads only: a retry's gate is let go after the run, so a retry wait
  that lost both its stop and its 2 s timer would hang (as before this unit);
  (2) in a stopped first-load run the harness asserts that every event came
  with no tokio time gone (`:18921@9d2e7f3c`), the second turn's too, which
  waits for the load's answer from its thread: **no advance happened before
  the last logged event**, so nothing was run past up to there. The no-stop
  first-load runs stay on the wall clock, from which the load's 1.5 s are
  counted.
- **The no-stop lower bounds**: a retry run's first end at `>= 1.9 s` on the
  paused clock (it reads exactly the timer's 2 s), a first load's at `>= 1.5 s`
  on the wall clock, counted from a reading taken **before** the load's
  opener starts (`begun`, `:18783`; C's `started`). Round 1 read B's start
  after the opener had started, so a thread held in between made the load
  look early (`red2-base.txt`, `start=1200`): round 1's "load cannot break
  them" was wrong for that one.
- **Held picks the clock and lets the gate go** (round 3, opus SHOULD 1):
  `Held` is made only where its gate is made (`retry_never_lands`,
  `first_load_behind_a_history_image`, `waiting_in`) and spent only by
  `Held::run` (`:18703@9d2e7f3c`), which runs the loop on the wait's clock and
  lets the gate go -- a retry's after the run, a first load's to the band (with
  a stop) or to an opener (without). The harness and D both run through it
  (D at `:20037@9d2e7f3c`). C takes no `Held`: it builds its own gate and picks
  its clock by `stop`.
- `CatalogDue` does not apply: it says when a catalog read is due
  (`src/provider/catalog_clock.rs`, `is_due`), and no catalog wait of a turn
  reads it.
- **Strictness**: `at.is_zero()` fails any tokio timer a later change puts on
  the stop path. That is deterministic, and meant. One case it does not fail
  every time: a timer standing, not awaited, while the second turn waits for
  the load's answer -- whether the clock advances to it depends on whether
  the catalog thread's wake lands within the runtime's zero-timeout park
  (`did_wake`, tokio's `runtime/time/mod.rs:263-275`).
- **The one non-test file**: `Cargo.toml` `[dev-dependencies]` gains
  `tokio = { version = "1", features = ["test-util"] }`, for `start_paused`.
  Test builds only; it adds no crate, and `Cargo.lock` is unchanged (every
  `--locked` build here passed).

### What the paused clock cannot see, and the wall-clock backstop

**The class.** Any lateness that is not on a tokio timer of the loop's
runtime reads as no tokio time: the loop's thread held (a blocking sleep or
join, CPU work on the stop path), `spawn_blocking` (it stops auto-advance), or
a wait on another OS thread's answer while no timer stands (the runtime parks
without advancing). A machine holding the thread looks the same to any
deterministic test. It applies at all four stop points: the window's,
the image route's and the turn's catalog reads (`:4585-4589`, `:5850-5854`,
`:6150-6154`) and the retry wait (`:4552-4556`).

**The backstop (round 3): an empirical stop-to-end watchdog on the wall
clock** in `first_of_two_and_a_stop` (A, B, E; not C, whose band runs on its
own thread). It is a measured margin, not a guarantee:

- **The segment.** From the stop, stamped as the band sends it
  (`:18935@6073cdc5`), to the first turn's end as **a watcher thread observes
  it**: the loop giving the first turn's place back (`outstanding`; the loop
  gives it back after the turn's terminal event and before it takes the next
  work, `worker.rs:3554-3561`). The stamp is the watcher's observation, not
  the release (`watched`, `:18755@6073cdc5`): the watcher looks every
  millisecond, and if its thread waits for a core it stamps late -- possibly
  after the next turn has started, and so after a stall at that turn's start.
  No product hook stamps the release itself; this unit stays test-only (the
  dispatcher's decision, reversible). It is not the band's read of
  `TurnEnded` either: the band reads a turn's end only after the next turn has
  started (round 1's finding), which would put every turn-2 stall inside.
  **Retracted from round 3:** that no stall point falls in the segment, and
  that the bound does not measure the scheduler. What holds: in a normal run
  nothing stalls a turn's start (the stall points exist only in the RED
  patch), so the watcher's lateness is scheduling noise. The stall table's
  passes at `turn=5000` and `turn=12000` (`green3b-head.txt`,
  `green4-head.txt`) are one schedule's evidence that the watcher usually
  looks before turn 2's stall, not a proof for every schedule.
- **The bound, `STOP_TO_END` = 10 s** (`:18745@6073cdc5`, asserted at
  `:18980`), an empirical watchdog sized at **about twelve times** the
  segment's measured maximum. The brief: at least max(2 s, 10 x the
  maximum over the 50-run repeat). Measured at `ce221384`, 550 stopped runs at
  load 5.6-23.4 (`load-check3.txt`): median 97.9 ms, p99 629.3 ms, **max
  831.9 ms**; 10 x = 8.3 s, so 10 s (the crate's `TEST_DRAIN`, 12.0 x). The
  segment holds the stopped turn's own `fsync`ed record (`session::store`),
  which is why its tail is long; neither `fsync` nor scheduling has a known
  upper bound, so a load past anything measured could breach it with the
  product right. The repeats since: `load-check3b.txt` (`9d2e7f3c`) max
  725.9 ms; `load-check4.txt` (`6073cdc5`, load 4.3-14.2) median 63.0 ms,
  p99 291.0 ms, max 505.8 ms. Round 3 first committed
  5 s (`ce221384`), before the repeat measured; the 832 ms maximum put that
  under the rule, so `9d2e7f3c` raised it.
- **Round 4: the watcher cannot hang an unwinding test.** `watched` stops the
  watcher from a drop guard that ends as the run ends or unwinds, before the
  scope joins (`:18786@6073cdc5`); round 3 stopped it only on the normal path,
  so a panic before the first place came back left the scope waiting for
  ever. F runs a panicking run under `watched` and must see the panic within
  10 s. A stopped run now requires both stamps (`:18974`) rather than
  skipping the bound when one is missing.
- **Round 4: a late watcher, on purpose** (G). The harness's watcher stamps 3 s
  after it sees the place given back: the run passes, its first turn ended as
  the stop. Then 11 s, past `STOP_TO_END`: the run fails on the bound, the
  product right -- what a watcher descheduled that long would do.
- **What it catches** (`mutants3b-head.txt`, at 10 s): a 12 s hold, by the
  thread or by a cross-thread wait, at the turn's read (B E) and at the retry
  wait (A B E). What it does not: 700 ms, 500 ms and 6 s holds (6 s was killed
  under `ce221384`'s 5 s bound, `mutants3-head.txt`).

**What is left, by resolution:**

| stop point | caught deterministically | wall-clock resolution |
|---|---|---|
| retry wait | a tokio-timer delay (`is_zero`) | 10 s (`STOP_TO_END`, A B E); the sweep scenario `p5-stop-in-catalog-wait` snaps 1 s after its Ctrl-C (paired with fx, sweeps only) |
| first loads (window, image route, turn's read) | a tokio-timer delay (`is_zero`) | 10 s (`STOP_TO_END`, A B E); the give-ups, 30 s (`RUN_DEADLINE`, harness and C) |

- **The contract reduction, against the base:** the base's 1 s window killed
  a 700 ms thread hold, and a 700 ms cross-thread wait, at the turn's read
  (`mutants2-base.txt`, `mutants3-base.txt`: B C); the head catches neither
  below 10 s. At the window's and the image route's reads the base caught no
  such hold at all (no base test reached them); at the retry wait the base did
  not catch 500 ms either (`retry-wait-stop-late-thread-500`,
  `-xthread-500`: SURVIVED at base).
- **The finer backstop to come:** the queued unit
  **STOP-IN-FIRST-LOAD-SCENARIO**, an fxcap first-load stop scenario paired
  with fx, about 1 s resolution in the sweeps.

## RED on demand, GREEN under the same delays

`stall_patch.py` (here) plants three stall points in a throwaway clone,
never the repo; `stall-base.patch` and `stall-head.patch` are what it planted
at each side:

- `turn` (round 1's only point): the loop's thread, right after each turn
  sends `TurnStarted`;
- `select`: the loop's thread, before the loop's first select;
- `start`: the test's thread, right before the harness reads its wall-clock
  start (at the base after B's load opener has started; at the head before;
  round 3's head reads it in `Held::run`, `stall-head3.patch`).

`stall_runs.py` runs the five tests once per stall spec, in one normal slot
of the suite lock (`round2.sh`, `round3.sh`, `round3b.sh`; their loads in
`stall2-loads.txt`, `stall3-loads.txt`).

| spec | base `afd891af` (`red3-base.txt`) | round 2 `a6ab3ee6` (`green2-head.txt`) | `ce221384`, bound 5 s (`green3-head.txt`) | head `9d2e7f3c`, bound 10 s (`green3b-head.txt`) |
|---|---|---|---|---|
| none | 5 ok | 5 ok | 5 ok | 5 ok |
| `turn=1200` | A B C FAILED | 5 ok | 5 ok | 5 ok |
| `turn=2500` | A B C E FAILED | 5 ok | 5 ok | 5 ok |
| `select=1200` | A B C D E FAILED | 5 ok | 5 ok | 5 ok |
| `start=1200` | B E FAILED | 5 ok | 5 ok | 5 ok |
| `turn,select,start=1200` | A B C D E FAILED | 5 ok | 5 ok | 5 ok |
| `turn=5000` | A B C D E FAILED | 5 ok | 5 ok | 5 ok |
| `turn=12000` | -- | -- | -- | 5 ok |

At round 4's head `6073cdc5` (`green4-head.txt`, `stall-head4.patch`), the
same specs over A to G: 7 ok at every one. These passes are one schedule's
evidence, not a proof for every schedule (the segment's end is an
observation; see above).

The base column is round 3's rerun; round 2's (`red2-base.txt`) is the same
but for `turn=5000`, which it did not run.

- What failed at base (`red2-base.txt`, the `left:`/`right:` lines):
  `turn=1200` the on-time flag only; `turn=2500` the stop read past the
  retry's 2 s, so A's and B's second-turn ends differ from the no-stop runs'
  (the first turn answered or refused), and E's no-stop second turn ended
  cancelled: the base harness let go of `control_tx` after its stopper's 8 s
  hold, and a closed control channel is the session's end, which stops every
  turn (`worker.rs:5810-5816`) -- with 2.5 s at each turn's start and two 2 s
  retry waits, E's no-stop run outlived those 8 s; `select=1200` the stop read before the
  loop took the first prompt (`Taken::Abandon`): the first prompt was dropped
  unasked, so A, B, D and E have one turn where two should end, and C's band
  saw no turn at all; `start=1200` the load's opener (started in `waiting_in`,
  before the base's start read) ran on through the stall: B's no-stop
  first-load ends read short of 1.2 s, and in B's and E's stopped first-load
  cases the load landed before the 300 ms stop, so the turn was refused or
  answered instead of stopped.
- Round 1 (`red-base.txt`, `green-3c0a2ab3.txt`, `green-head.txt`) probed the
  `turn` point only.

## Mutants

`mutants.py` (here) on `git clone --no-local` throwaways, never the worktree:
each mutant applied once (exact text, one match), the five tests run (the
whole lib suite for the two marked "full lib"), under a 900 s bound, the
clone reset after each and clean at the end. Round 2: `mutants2-head.txt`
(all 19), `mutants2-base.txt` (the 8 new ones). Round 1's base runs of the
first 11 ran over A to C only (`mutants-base.txt`, `mutants-base-late.txt`),
so for those rows the base and head columns span different test sets; every
base kill among them is by A, B or C, which the head also runs. Round 3:
`mutants3-head.txt` (all 26 at `ce221384`, bound 5 s), `mutants3b-head.txt`
(the lateness ones at `9d2e7f3c`, bound 10 s), `mutants3-base.txt` and
`mutants3b-base.txt` (round 3's at the base).

| mutant | base | head `a6ab3ee6` |
|---|---|---|
| `retry-wait-ignores-stop` | killed A B | killed A B |
| `retry-wait-stop-late` (500 ms, a tokio sleep) | killed A B | killed A B |
| `turn-catalog-read-ignores-stop` | killed B C | killed B C E |
| `turn-catalog-read-stop-late` (500 ms, a tokio sleep, any catalog) | SURVIVED | killed B C E |
| `window-first-load-ignores-stop` (full lib) | **SURVIVED, 4761 tests** | killed B (`WindowFirstLoad`) |
| `image-route-read-ignores-stop` (full lib) | **SURVIVED, 4761 tests** | killed A (the image route's first load) |
| `first-load-late-window-tokio` (700 ms, first load only) | SURVIVED | killed B |
| `first-load-late-window-thread` | SURVIVED | SURVIVED |
| `first-load-late-turn-read-tokio` | killed B C | killed B C E |
| `first-load-late-turn-read-thread` | killed B C | **SURVIVED** (the contract reduction above) |
| `first-load-late-image-route-tokio` | SURVIVED | killed A |
| `first-load-late-image-route-thread` | SURVIVED | SURVIVED |
| `prompt-images-first-load-not-waited` | killed B C | killed B C |
| `ultra-refusal-not-yielded` | killed B | killed B |
| `image-refusal-asked-when-stopped` | killed B C | killed B C |
| `candidate-bare-return` | killed B C | killed B C |
| `no-machine-guard` | SURVIVED | SURVIVED |
| `stop-leaks-to-next-turn` | killed A B | killed A B D E |
| `questions-cap-back-to-4` | killed C | killed C |

Round 3, lateness off tokio's timers (`thread` = the loop's thread held;
`xthread` = the stop handler awaits a oneshot another OS thread answers after
the delay, with no timer standing -- gpt6's case):

| mutant | base | `ce221384` (5 s) | head `9d2e7f3c` (10 s) |
|---|---|---|---|
| `first-load-late-turn-read-thread` (700 ms) | killed B C | SURVIVED | SURVIVED |
| `first-load-late-turn-read-thread-6000` | killed B C | killed B E | SURVIVED |
| `first-load-late-turn-read-thread-12000` | killed B C E | -- | killed B E |
| `first-load-late-turn-read-xthread-700` | killed B C | SURVIVED | SURVIVED |
| `first-load-late-turn-read-xthread-6000` | killed B C | killed B E | SURVIVED |
| `first-load-late-turn-read-xthread-12000` | killed B C E | -- | killed B E |
| `retry-wait-stop-late-thread-500` | SURVIVED | SURVIVED | SURVIVED |
| `retry-wait-stop-late-thread-6000` | killed A B D E | killed A B E | SURVIVED |
| `retry-wait-stop-late-thread-12000` | killed A B D E | -- | killed A B E |
| `retry-wait-stop-late-xthread-500` | SURVIVED | SURVIVED | SURVIVED |
| `retry-wait-stop-late-xthread-6000` | killed A B D E | killed A B E | SURVIVED |
| `retry-wait-stop-late-xthread-12000` | killed A B D E | -- | killed A B E |

Round 4, at `6073cdc5` over A to G (`mutants4-head.txt`): the watcher's
guard and the bound itself, then round 3's lateness mutants again.

| mutant | head `6073cdc5` |
|---|---|
| `watcher-guard-removed` (stopped on the normal path only, round 3's shape) | killed F: its 10 s wall bound fired (the run otherwise waits for ever) |
| `bound-not-asserted` (`took < Duration::MAX`) | killed G: the run with the watcher 11 s late passed |
| `first-load-late-turn-read-thread` (700 ms) | SURVIVED |
| `first-load-late-turn-read-thread-12000` | killed B E |
| `first-load-late-turn-read-xthread-700` | SURVIVED |
| `first-load-late-turn-read-xthread-12000` | killed B E |
| `retry-wait-stop-late-thread-500` | SURVIVED |
| `retry-wait-stop-late-thread-12000` | killed A B E G |
| `retry-wait-stop-late-xthread-500` | SURVIVED |
| `retry-wait-stop-late-xthread-12000` | killed A B E G |

- Over round 2's 19, the head kills every one the base kills but
  `first-load-late-turn-read-thread`, and kills five the base does not. Round
  3's 12: the head kills every hold past its 10 s bound, at both points and
  of both kinds; under it, it kills none -- the reduction stated above. C is
  never among the head's kills here: it has no wall bound.
- `window-first-load-ignores-stop` and `image-route-read-ignores-stop` survived
  the base's **whole lib suite** (4761 passed): those two waits were
  uncovered crate-wide. The new `CatalogWait::WindowFirstLoad` case
  (`:19062`) and A's "the image route's first load" case
  (`first_load_behind_a_history_image`, `:18641`) kill them.
- `no-machine-guard` is the machine test's
  (`agent::machine::tests::a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`;
  STOP-IN-CATALOG-WAIT's `mutants.txt`).

## Repeat check

**Round 4** (`load-check4.txt`, the same script, A to E -- F and G test the
watcher, not the stop, and run in the stall table, the mutants and the gate):
base `afd891af` and head `6073cdc5` 50/50 on all five, at load 4.0-13.8
(base, median 7.25) and 4.3-14.2 (head, median 6.96); the stop-to-end
segment over 550 stopped runs: median 63.0 ms, p99 291.0 ms, max 505.8 ms.

**Round 3** (`load_check.py`, here, now run with `--nocapture` to collect the
harness's `stop-to-end-us` lines): the five tests 50 times on the base's lib
test binary and 50 on the head's, one after the other in each iteration, in
one normal slot, no burner.

| run | binary | A | B | C | D | E | load at the runs' start | stop-to-end, 550 segments |
|---|---|---|---|---|---|---|---|---|
| `load-check3.txt` | base `afd891af` | 50/50 | **49/50** | 50/50 | 50/50 | 50/50 | 5.38-22.61, median 9.96 | -- |
| `load-check3.txt` | `ce221384` | 50/50 | 50/50 | 50/50 | 50/50 | 50/50 | 5.62-23.36, median 9.59 | median 97.9 ms, p99 629.3 ms, max 831.9 ms |
| `load-check3b.txt` | base `afd891af` | 50/50 | 50/50 | 50/50 | 50/50 | 50/50 | 4.21-15.75, median 6.93 | -- |
| `load-check3b.txt` | head `9d2e7f3c` | 50/50 | 50/50 | 50/50 | 50/50 | 50/50 | 3.96-13.89, median 6.73 | median 68.6 ms, p99 361.3 ms, max 725.9 ms |

- **The base failed on its own once**, B in run 4 of `load-check3.txt` at
  load 12.73: its `PromptImageFirstLoad` stop case's on-time flag, the
  original flake's field (the file's `FAIL` lines). The head did not fail in
  either run.
- Round 2's repeat: `load-check2.txt`, below.

`load-check2.txt` (`load_check.py`, here): the five tests 50 times on the
base's lib test binary (built at `afd891af`) and 50 on the head's (built at
`a6ab3ee6`), one after the other in each iteration, in **one normal slot** of
the suite lock, with no burner -- the shared machine's own load, read at each
run's start and written beside it. (Its header says "the three tests (A to
E)": the script's header string was stale at that run, fixed since; every
line counts five.)

| binary | A | B | C | D | E | 1-min load at the runs' start (18 cores) |
|---|---|---|---|---|---|---|
| base `afd891af` | 50/50 | 50/50 | 50/50 | 50/50 | 50/50 | min 5.48, median 11.33, max 18.13 |
| head `a6ab3ee6` | 50/50 | 50/50 | 50/50 | 50/50 | 50/50 | min 5.31, median 11.58, max 18.05 |

- At load 5 to 18 the base does not fail either (the gate's misses came at 65
  to 106); the stall tables are what tell base from head. Round 1's repeat
  check, three tests at `5f55cfc8`: `load-check.txt`, 50/50 on both sides.
- The exclusive run with a burner, which round 1's brief asked for, was
  stopped before it began at the dispatcher's word (it starved for slots and
  would have held every gate off for up to 90 minutes); its burner never ran.

## Gate

TBD-GATE4

`gates3.txt` (`gates.sh`), in a `git clone --no-local` at `bbf15d6d` -- the
code head `9d2e7f3c` plus round 3's receipt but for the gate's own file --
inside one hold of the suite lock: **22 steps, 0 failed**, the tree clean
before and after. `test` 5685 passed, 0 failed; `test-fault` 5722 passed, 0
failed; A to E `ok` in both; smoke-tui 37 scenarios and the oracle, 1167
checks, 0 failures.

`gates2.txt` (`gates.sh`, train 13's, unchanged but for its header), in a
`git clone --no-local` at `6b0c613f` -- the code head `a6ab3ee6` plus round
2's receipt but for the gate's own file and the repeat check -- inside one
hold of the suite lock: **22 steps, 0 failed**, the tree clean before and
after. `test` 5685 passed, 0 failed; `test-fault` 5722 passed, 0 failed; A to
E `ok` in both; smoke-tui 37 scenarios and the oracle, 1167 checks, 0
failures. Round 1's gate at `64cfd5a1`: `gates.txt`, the same counts.

## Other wall-clock bounds in the crate

`wallclock-others.txt`: 17 bounds in 16 tests that are tight (a window, or a
bound within about 1.5 s of the work it times) and 13 wider ones, two of
whose tests are also in the tight list. None changed here. The one that had
this unit's race with no bound asserted, D, is fixed in round 2. Round 3 adds
one wide bound of its own, `STOP_TO_END`, listed at the end.

## Files

- Round 4: `round4.sh`, `stall-head4.patch`, `stall4-loads.txt`,
  `green4-head.txt`, `mutants4-head.txt`, `load-check4.txt`, `gates4.txt`.
  `stall_runs.py` and `mutants.py` are round 4's versions (A to G).
- Round 3: `stall-head3.patch` (at `ce221384`), `stall-head3b.patch` (at
  `9d2e7f3c`), `round3.sh`, `round3b.sh`, `stall3-loads.txt`, `red3-base.txt`,
  `green3-head.txt`, `green3b-head.txt`, `mutants3-head.txt`,
  `mutants3-base.txt`, `mutants3b-head.txt`, `mutants3b-base.txt`,
  `load-check3.txt`, `load-check3b.txt`, `gates3.txt`. `stall_patch.py`,
  `mutants.py` and `load_check.py` are round 3's versions.
- Round 2: `stall_patch.py`, `stall-base.patch`, `stall-head.patch`,
  `stall_runs.py`, `round2.sh`, `stall2-loads.txt`, `red2-base.txt`,
  `green2-head.txt`, `mutants.py`, `mutants2-base.txt`, `mutants2-head.txt`,
  `load_check.py`, `load-check2.txt`, `gates2.txt`.
- Round 1: `turn-stall.patch`, `red-base.txt`, `green-3c0a2ab3.txt`,
  `green-head.txt`, `stall-loads.txt`, `round.sh`, `mutants-base.txt`,
  `mutants-base-late.txt`, `mutants-head.txt`, `load-check.txt`, `gates.txt`.
  `stall_runs.py`, `mutants.py` and `load_check.py` are round 2's versions (a
  superset: five tests, stall specs; round 1 ran them over three tests and
  the `turn` point).
- `gates.sh`; `wallclock-others.txt`.
