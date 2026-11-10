# catalog-wait-clock: the catalog-wait stop tests prove "at once" without a 1 s window

Unit CATALOG-WAIT-CLOCK: test-only -- `src/tui/worker.rs`'s tests module and
one `[dev-dependencies]` line in `Cargo.toml`; no product line changes. Base
`afd891af` (train 13's head). Every claim below is tied to a file in this
folder. Source lines are at `afd891af` unless marked `@a6ab3ee6`.

| commit | what |
|---|---|
| `3c0a2ab3` | round 1: the stop placed by the loop; first loads held until the stop's end; the retry bound at 2 s on the wall clock |
| `b5e234e1`, `5f55cfc8` | round 1: the retry runs on tokio's paused clock (`test-util` in `[dev-dependencies]`), the stopped end asserted at zero |
| `64cfd5a1`..`d525cc7b` | round 1's receipt, gate and repeat check (files without a `2` in their name) |
| `a6ab3ee6` | round 2 (the reviews of `d525cc7b`): every stopped run on the paused clock, first loads included; no clock places a stop; the compaction test; two new first-load cases -- **the code head**; every round-2 head-side run is this commit |
| the next commits | round 2's receipt (files with a `2` in their name, and this README), gate and repeat check |

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
(on the same harness; its assertions are unchanged).

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
  wall-clock deadline thread (`WallDeadline`, `:18703`), which a paused clock
  cannot jump to -- a missed stop fails at that deadline instead of hanging;
  (2) in a stopped first-load run the harness asserts that every event came
  with no tokio time gone (`:18858`), the second turn's too, which waits for
  the load's answer from its thread: no timer stood in the runtime for the
  clock to run to, so nothing was run past. The no-stop first-load runs stay
  on the wall clock, from which the load's 1.5 s are counted.
- **The no-stop lower bounds**: a retry run's first end at `>= 1.9 s` on the
  paused clock (it reads exactly the timer's 2 s), a first load's at `>= 1.5 s`
  on the wall clock, counted from a reading taken **before** the load's
  opener starts (`begun`, `:18783`; C's `started`). Round 1 read B's start
  after the opener had started, so a thread held in between made the load
  look early (`red2-base.txt`, `start=1200`): round 1's "load cannot break
  them" was wrong for that one.
- **Held is structural** (opus SHOULD 3): `Held` is made only where its gate
  is made (`retry_never_lands`, `first_load_behind_a_history_image`,
  `waiting_in`), so no test holds a gate's sender of its own; the harness
  alone opens it and picks the clock from it (`enum Held`, `:18672`).
- `CatalogDue` does not apply: it says when a catalog read is due
  (`src/provider/catalog_clock.rs`, `is_due`), and no catalog wait of a turn
  reads it.
- **Strictness**: `at.is_zero()` fails any tokio timer a later change puts on
  the stop path. That is deterministic, and meant.
- **The one non-test file**: `Cargo.toml` `[dev-dependencies]` gains
  `tokio = { version = "1", features = ["test-util"] }`, for `start_paused`.
  Test builds only; it adds no crate, and `Cargo.lock` is unchanged (every
  `--locked` build here passed).

### What the paused clock cannot see, and the contract it gives up

A stop held up by the loop's **thread** -- a blocking sleep, a blocking join,
CPU work on the stop path -- reads as no tokio time, exactly like a loaded
machine holding that thread; no deterministic test can tell the two apart.
The `first-load-late-*-thread` mutants below hold the thread 700 ms after a
stop, at each first-load select:

- at the window's and the image route's selects they survive at base and
  head alike (no base test reached those waits);
- at the turn's own catalog read (`:6150-6154`) the base killed it (B and C:
  300 ms + 700 ms broke the 1 s window, the window that flaked) and the head
  does not. **That is a contract reduction**: the head no longer catches
  thread-held lateness at a stop in the first-load wait. The same holds for
  the retry wait (`retry-wait-stop-late` is caught only as a tokio sleep).
- The backstop the dispatcher named, the sweep scenario
  `p5-stop-in-catalog-wait`, presses Ctrl-C 0.7 s into a turn's wait and
  snaps the screen 1 s later on the wall clock -- but its turn waits on a
  failed catalog's **retry** (`modelsdown`, then a prompt whose read takes
  10 s: `scripts/fxcap/scenarios/p5-stop-in-catalog-wait.cap`), not on a
  first load. No sweep scenario stops a first-load wait; the first-load
  thread-held case has no backstop today.

## RED on demand, GREEN under the same delays

`stall_patch.py` (here) plants three stall points in a throwaway clone,
never the repo; `stall-base.patch` and `stall-head.patch` are what it planted
at each side:

- `turn` (round 1's only point): the loop's thread, right after each turn
  sends `TurnStarted`;
- `select`: the loop's thread, before the loop's first select;
- `start`: the test's thread, right before the harness reads its wall-clock
  start (at the base after B's load opener has started; at the head before).

`stall_runs.py` runs the five tests once per stall spec, in one normal slot
of the suite lock (`round2.sh`; `stall2-loads.txt`, load 13.48 at the start).

| spec | base `afd891af` (`red2-base.txt`) | head `a6ab3ee6` (`green2-head.txt`) |
|---|---|---|
| none | 5 ok | 5 ok |
| `turn=1200` | A B C FAILED | 5 ok |
| `turn=2500` | A B C E FAILED | 5 ok |
| `select=1200` | A B C D E FAILED | 5 ok |
| `start=1200` | B E FAILED | 5 ok |
| `turn,select,start=1200` | A B C D E FAILED | 5 ok |
| `turn=5000` | -- | 5 ok |

- What failed at base (`red2-base.txt`, the `left:`/`right:` lines):
  `turn=1200` the on-time flag only; `turn=2500` the stop read past the
  retry's 2 s, so A's and B's second-turn ends differ from the no-stop runs'
  (the first turn answered or refused); `select=1200` the stop read before the
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
first 11, over A to C: `mutants-base.txt`, `mutants-base-late.txt`.

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

- Over these 19, the head kills every one the base kills but
  `first-load-late-turn-read-thread`, and kills five the base does not.
- `window-first-load-ignores-stop` and `image-route-read-ignores-stop` survived
  the base's **whole lib suite** (4761 passed): those two waits were
  uncovered crate-wide. The new `CatalogWait::WindowFirstLoad` case
  (`:19062`) and A's "the image route's first load" case
  (`first_load_behind_a_history_image`, `:18641`) kill them.
- `no-machine-guard` is the machine test's
  (`agent::machine::tests::a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`;
  STOP-IN-CATALOG-WAIT's `mutants.txt`).

## Repeat check

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
this unit's race with no bound asserted, D, is fixed in round 2.

## Files

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
