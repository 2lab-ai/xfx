# catalog-wait-clock: the catalog-wait stop tests prove "at once" without a 1 s window

Unit CATALOG-WAIT-CLOCK: test-only -- `src/tui/worker.rs`'s tests module and
one `[dev-dependencies]` line in `Cargo.toml`; no product line changes. Base
`afd891af` (train 13's head). Every claim below is tied to a file in this
folder; source lines are at `afd891af` unless marked `@5f55cfc8`.

| commit | what |
|---|---|
| `3c0a2ab3` | round 1: the stop placed by the loop; first loads held until the stop's end; the retry bound at 2 s on the wall clock |
| `b5e234e1` | round 2: the retry runs on tokio's paused clock; `test-util` in `[dev-dependencies]` |
| `5f55cfc8` | round 2: the stopped retry wait's end asserted at zero on that clock -- **the code head**; every head-side run below is this commit unless named |
| the next commits | this receipt, then its gate and load check |

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

## What each bound proved, and what proves it now

"With the stop: 300 ms to 1 s" meant **the stop ends the turn's catalog wait
at once, and the turn does not wait out the catalog.** What the turn would
otherwise wait out differs by case, so the proof does too.

| wait (cases) | what ends it without the stop | proof now |
|---|---|---|
| a first load: B `UltraFirstLoad`, `PromptImageFirstLoad`; C | the test's own gate (`Gated`), opened 1.5 s in | **order, no clock.** With a stop the gate stays shut until the stopped turn has ended (`Held::FirstLoad`; C's band holds it). A turn that ends at all, `cancelled`, was ended by the stop: the load had not landed. |
| a failed catalog's retry: A both cases; B `UltraRetry`, `PromptImageRetry`, `WindowRetry` | the product's own timer, `tokio::time::timeout(FAILED_CATALOG_RETRY_WAIT, ...)`, 2 s (`worker.rs:4555`) | **the wait's own clock, paused.** These runs use `on_a_paused_runtime` (`start_paused`, `worker.rs:7428@5f55cfc8`): tokio's time moves only when every task is parked, straight to the next timer, so a held thread is no time. The stopped turn's end is asserted at **zero** on that clock (`:18822`, `:19149@5f55cfc8`); a turn that waited the retry out ends at 2 s, one whose stop was honoured late at that delay. |

- **Why the paused clock fits the retry runs and not the first-load runs.**
  In a retry run nothing waits on another thread: the retry's gate is held
  for the whole loop, so every park ends on a tokio timer or a message from
  the same runtime. A first load is answered from the catalog's own thread
  (`PendingLoad::start`, `src/provider/model.rs:1697`); a paused runtime
  waiting on it runs its clock on to the next timer -- the harness's
  `TEST_DRAIN` -- and gives up. Those runs keep the wall clock and the order
  proof.
- **`CatalogDue` does not apply.** It says when a catalog read is due
  (`src/provider/catalog_clock.rs`, `is_due`); no catalog wait of a turn
  reads it. The retry wait's clock is tokio's; `test-util` only makes it
  pausable.
- **Round 1 tried the wall clock for the retry runs, and it was the wrong
  shape** (`green-3c0a2ab3.txt`). Its bound was `at < FAILED_CATALOG_RETRY_WAIT`,
  counted from before the loop, the timer starting after the count. But on
  one thread the band reads the first turn's end only once the loop parks
  again, after the second turn's start has run: with the thread held 1.2 s
  at each turn's start (2.4 s before the band read the end), all five retry
  stop cases failed at `3c0a2ab3`, the product right, and only they (the
  first-load cases passed at every stall). Round 2 changed the clock, not
  the fault model.
- **Where the stop lands, without a clock.** `first_of_two_and_a_stop`
  (`worker.rs:18681@5f55cfc8`) sends the Ctrl-C from the loop's own thread
  when the band hears `TurnStarted { submission: 1 }`. `tokio::join!(run,
  watch)` polls both on one task, so the band reads that event only after
  `run` has returned `Pending` since sending it, and `run_turn` polls the
  body right after the send (`worker.rs:3871-3891`): the turn is parked in
  its first wait, the catalog's (`one_turn` `:6124-6127`; `image_refusal`
  `:5850-5862` for the image route). `raced_against_control` reads control
  before the body (`:5742-5744`), so the stop is read before the turn moves
  again. The stopper thread and its 300 ms sleep are gone.
- **Kept, because load cannot break them:** the no-stop lower bounds (A
  `>= 1.9 s`, B and C `>= 1.2 s`: a turn that waited cannot end sooner; on
  the paused clock a retry run's no-stop end reads exactly its 2 s) and C's
  `>= 300 ms` after its band's stop.
- **Moved with the harness, assertions unchanged:**
  `a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
  the other caller of `first_of_two_and_a_stop`.
- **The one non-test file:** `Cargo.toml` `[dev-dependencies]` gains
  `tokio = { version = "1", features = ["test-util"] }` (`Cargo.toml:88@5f55cfc8`),
  for `start_paused`. Test builds only; tokio's `test-util = ["rt", "sync",
  "time"]` adds no crate, and `Cargo.lock` is unchanged: every `--locked`
  build here passed.

## RED on demand, GREEN under the same delay

`turn-stall.patch` (here; scratch only, never committed): the loop's thread
sleeps `XFX_SCRATCH_TURN_STALL_MS` right after each turn sends `TurnStarted`
(`run_turn`) -- a machine holding the thread at the start of every turn. The
same patch on both sides; `stall_runs.py` runs the three tests once per
stall under the suite lock (`stall-loads.txt`: the load when each command
began and ended).

| tree | stall 0 | 1200 ms | 1600 ms | 2500 ms | 5000 ms | file |
|---|---|---|---|---|---|---|
| base `afd891af` | A B C ok | **A B C FAILED** | **A B C FAILED** | -- | -- | `red-base.txt` |
| round 1 `3c0a2ab3` | A B C ok | A B FAILED, C ok | A B FAILED, C ok | A B FAILED, C ok | -- | `green-3c0a2ab3.txt` |
| head `5f55cfc8` | A B C ok | A B C ok | A B C ok | A B C ok | A B C ok | `green-head.txt` |

- **RED is the timing field only.** In every base failure the `left` and
  `right` of the final `assert_eq!` differ in the on-time flag of the stop
  cases alone -- `false` for `true` -- A's two, B's five and C's one
  (`red-base.txt`, the `left:`/`right:` lines): every turn still ended as the
  stop, with the same records.
- At round 1 only the five retry stop cases' flag failed; the first-load
  cases were `true` at every stall (`green-3c0a2ab3.txt`).

## Mutants

`mutants.py` (here) on `git clone --no-local` throwaways, never the worktree:
each mutant applied once (exact text, one match), the lib tests built, the
three tests run under a 900 s bound, the clone reset after each and clean
at the end. Base in `mutants-base.txt` and `mutants-base-late.txt`, head in
`mutants-head.txt`.

| mutant | base | head |
|---|---|---|
| `retry-wait-ignores-stop` (the retry wait's stop arm removed; train 7's J2) | killed A B | killed A B |
| `turn-catalog-read-ignores-stop` (the turn's catalog read: first load, Ultra, image) | killed B C | killed B C; C's turn never ended while its load was held |
| `retry-wait-stop-late` (the stop honoured 500 ms late, inside the 2 s) | killed A B | killed A B |
| `turn-catalog-read-stop-late` (the same, at the turn's catalog read) | **SURVIVED** | killed B (`UltraRetry` and `PromptImageRetry` stop cases, whose read is on the paused clock) |
| `window-first-load-ignores-stop` | SURVIVED | SURVIVED |
| `image-route-read-ignores-stop` | SURVIVED | SURVIVED |
| `prompt-images-first-load-not-waited` | killed B C | killed B C |
| `ultra-refusal-not-yielded` | killed B | killed B |
| `image-refusal-asked-when-stopped` | killed B C | killed B C |
| `candidate-bare-return` | killed B C | killed B C |
| `no-machine-guard` | SURVIVED | SURVIVED |
| `stop-leaks-to-next-turn` | killed A B | killed A B |
| `questions-cap-back-to-4` (QUESTIONS-20's M1) | killed C | killed C |

- Everything the base tests killed, the head tests kill, and one more. The
  three that survive on both reach no case of these three tests:
  `window-first-load-ignores-stop` is the default model's first-load read
  (`WindowRetry` reaches it behind a failed catalog, whose read returns at
  once); `image-route-read-ignores-stop` is the image route's read of a first
  load (A's image route reads a failed catalog); `no-machine-guard` is the
  machine test's
  (`agent::machine::tests::a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`;
  STOP-IN-CATALOG-WAIT's `mutants.txt`).
- A stop honoured late but before the load lands, at a first-load wait on
  the wall clock, passes the order proof by design: those cases prove that
  the load did not end the wait. The late-stop mutant above is caught by the
  retry cases that reach the same read.

## Load check

TBD

## Gate

TBD

## Other wall-clock bounds in the crate

`wallclock-others.txt`: 17 tests whose proof is a tight wall-clock bound (a
window, or a bound within about 1.5 s of the work it times), 13 with a wider
one, and one with the same race and no bound asserted
(`a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs`:
a 300 ms stopper thread against the 2 s retry). None changed here.

## Files

- `turn-stall.patch` -- the scratch fault (never applied to the repo).
- `stall_runs.py` -- the stall table; `red-base.txt`, `green-3c0a2ab3.txt`,
  `green-head.txt`; `stall-loads.txt`.
- `mutants.py`; `round.sh` (the head's stall table and mutants, and the
  base's two late-stop mutants, in one hold of the lock); `mutants-base.txt`
  (the base's first eleven, run before the late-stop pair was added),
  `mutants-base-late.txt`, `mutants-head.txt`.
- `burn.c`, `load_check.py`; `load-check.txt`.
- `gates.sh`; `gates.txt`.
- `wallclock-others.txt`.
