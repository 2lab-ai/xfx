# flake-copyend: an ending session's copy, decided by order

`tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` failed
twice under load and passed alone: once in FLAKE-HINT4's gate (`cargo test
--locked` at load 43.6; that unit's `gate.txt`, branch `fix/p5-flake-hint4`)
and once in the L-U1+U2 worker's fault-injection run. Its fake tool slept
0.2 s and then copied. The session waited `GRACE` = 1 s
(`src/tui/copy.rs:48`) and then killed it. When the tool had not answered
inside that second, the clipboard file still held the first reply.

The fix decides the grace by the order of events, not by the clock. The
product change is a test seam (below): Drop's body became a method, and the
session's wait is unchanged. Line numbers are at `042908ae` unless marked
"base" (`e51685d6`).

## Why it failed

- The claim: a copy still running when the session ends gets `GRACE` to
  land, and then its tool is killed (`Errands`, base `copy.rs:50-99`).
- The old case (base `copy.rs:237-249`): a warm-up copy, then
  `start("the reply")`, then `drop(copies)` at once. The tool is
  `sleep 0.2; cat > copied`. Drop waits until 1 s after it began, then stops.
- The tool cannot start at once. Every start in the process waits for the
  one before it (`src/process.rs:60-88`, `SPAWNING`: one start at a time,
  so no pipe of one start reaches another's child). In the full suite
  another test's start can hold that lock, and macOS takes seconds to assess
  a fresh executable. The copy's start, plus its 0.2 s, then passes 1 s. The
  stop lands first. A start the stop reaches is refused
  (`clipboard.rs` `run`, `spawn_unless(|| stop.stopped())`), and a tool
  already running is killed. Either way `copied` keeps "a first reply".

## RED

- **The mechanism, every time** (`hold-runs.txt`, `harness.py`). In a
  throwaway clone at the base, one case is added: another thread takes the
  process's start lock (`crate::process::hold_every_start`) and holds it
  for 1.5 s from just before the copy. That is what another test's start
  does in the suite. The old body then fails **10 of 10**:
  `left: "a first reply"` (as bytes), at the case's `assert_eq!`.
- **Under load** (`load-runs.txt`), the test alone at `--test-threads 1`:
  - with `taskpolicy -b` (background QoS) beside 54 busy loops, machine load
    about 100 with other work: 3 of 3 failed. One failed at the claim
    (`copy.rs:248`, "a first reply"). Two failed at the warm-up's own 30 s
    bound (`copy.rs:201`, "the copy never finished"): the same class, a
    second wall-clock bound in the same case;
  - `-b` alone, `-b` with 18 or 36 loops, 54 loops without `-b`, the copy
    tests' neighbours in one process, and the whole lib suite three times
    under 36 loops: all passed (10, 10, 10, 20, 10 and 3 runs);
  - `load-batch.txt`: 54 loops with `-b` again, on a quiet machine (load
    11 to 35): 6 of 6 passed.

  So load alone reproduces it rarely, and only at the machine's limit. It
  takes a slow start in the same process, which the full suite supplies and
  the harness supplies on purpose. The loops ran only during each measured
  batch, at the sizes above.

## The fix

**The seam (product code).** `Errands::end(over)` (`copy.rs:84-105`) is
Drop's old body as a method. Each thread still running is waited for until
`over(began)` says the grace is over, asked only while a thread runs. Then
every thread is stopped and joined. Drop is
`self.end(|began| began.elapsed() >= GRACE)` (`copy.rs:108-112`), the same
wait as the old `Instant::now() < deadline`. A case that calls
`copies.errands.end(...)` leaves nothing running, so the Drop after it waits
for nothing. `FakeTool::GATE` and `FakeTool::open` (`src/clipboard.rs`,
under `#[cfg(all(test, unix))]`) are a polling gate a tool's body waits at.

**The cases** (`copy.rs` tests):

| case | before | now |
|---|---|---|
| `an_ending_session_lets_a_copy_that_answers_land` (`:261`) | a 0.2 s tool against 1 s of clock | the tool waits at the gate; the end's first look at the grace opens it. The copy can land only inside the grace, whatever the load. The grace runs out at `LOADED` (60 s), which only a tool that never answers reaches. The case asserts the end looked at all, and that `copied` is "the reply" |
| `an_ending_session_kills_a_copy_whose_answer_comes_after_its_grace` (`:285`, new) | none: the kill case below covers it by the clock | the tool is running, its answer behind the gate; the grace is over at the first look. The tool is dead, and opening the gate after lets nothing land |
| `a_copy_is_started_and_answered_later_rather_than_waited_for` (`:306`) | a `sleep 1` tool against an immediate `finished() == None`, and `start` bounded at 500 ms | the tool waits at the gate. `finished()` is `None` because the gate is shut. A start that waited for the tool would wait for ever, so a watchdog opens the gate after `LOADED` and the case asserts it had not fired |
| `an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread` (`:221`) | unchanged | it goes through the session's Drop, so it pins `GRACE` itself: `took >= GRACE` (mutant C3) |

## GREEN

- `hold-runs.txt`: the same 1.5 s hold on the start lock against the new
  body (`harness.py head`): **10 of 10 pass**. The gate opens at the end's
  first look, the start goes through after 1.5 s, and the only bound is
  `LOADED`.
- `load-batch.txt`: the three converted cases, 6 runs each, at 54 busy
  loops with `-b`: 18 of 18 pass.
- The whole suite twice in the gate (below).

## Mutants

`mutants.py`, in a `git clone --no-local` at `042908ae`: each a one-place
change to `copy.rs`, then the case named. `mutants.txt`: **4 of 4 killed**.

| mutant | killed by |
|---|---|
| C1 the end gives a copy up before it waits (the session kills before the answer) | `an_ending_session_lets_a_copy_that_answers_land` |
| C2 the end never waits: the grace is never asked | the same |
| C3 a session's Drop gives no grace | `an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread` (`took >= GRACE`) |
| C4 starting a copy waits for it | `a_copy_is_started_and_answered_later_rather_than_waited_for` (after the watchdog's 60 s) |

One mutant was left out because it hangs rather than fails: the end, its
grace over, joins without giving the copy up. Its join waits on a tool
behind a gate nobody opens.

## The other cases of the same files, checked

A sleep racing a fixed grace or deadline, or a wall-clock bound a loaded
machine can cross:

| case | what times it | verdict |
|---|---|---|
| `copy.rs:221` the kill case | `took >= GRACE` (always holds); `took < GRACE + 10 s`, against a tool whose own end is `exec sleep 30` | not this shape: the bound is "kill and join", milliseconds, with 20 s to spare before the tool would end by itself |
| `copy.rs:340` selection copy | waits up to `LOADED` for a tool that keeps what it is fed | no grace in it |
| `copy.rs:362` no tool | none | |
| `clipboard.rs:889` a tool that leaves before reading | `exit 0` against a 4 MiB write | order: the write fills the pipe and fails when the tool has gone; no clock |
| `clipboard.rs:929` a file copy past its deadline (macOS) | deadline 200 ms against `exec sleep 30`; `elapsed < 10 s` | the deadline is the claim; a tool not started by then is killed all the same |
| `clipboard.rs:954` a copy given up from another thread | waits for the tool (`until`, 60 s), then `elapsed < 5 s` against `exec sleep 30` | kill and join against a 30 s tool |
| `clipboard.rs:1000`, `:1029`, `:1057`, `:1087` | the stop's lock and the start lock held by the case (`held`, `hold_every_start`) | order already |
| `clipboard.rs:1116`, `:1145` (macOS) | waits for the tool, then a stop; `waitid` for the exited child | order |
| `clipboard.rs:724` `real_osascript` | 60 s for a real interpreter | a bound for "never", not a race |
| `src/tui/trace.rs:306` (also an `Errands` user) | `took < GRACE + 10 s` against a hanging tool (30 s) | the kill case's shape, not this one |

## Gates

`gate.txt`, at `042908ae`, one step at a time, load 4.5 to 24:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same, `--features fault-injection` | ok |
| `cargo test --locked` | 15 suites, 5165 passed, 0 failed |
| `cargo test --locked --features fault-injection` | 15 suites, 5202 passed, 0 failed |
| `scripts/check-*.sh` (6) | all ok |
| `scripts/fxcap/test-sweep.sh` | 50 OK, 26 OK |
| `cargo build --locked --release`, `scripts/smoke.sh` | 47 checks, 0 failures |
| `cargo build --locked --release --features fault-injection --target-dir target/faulty`, `scripts/smoke-tui.sh` | 37 scenarios + the oracle, 1111 checks, 0 failures |

## Files

- `harness.py`: the 1.5 s hold on the start lock, as a case added in a
  throwaway clone (`base` or `head` body).
- `hold-runs.txt`: its runs, 10 a side.
- `runs.sh`, `busy.pl`, `batch.sh`: the runner, the busy loops (each ends
  itself), the measured batch (starts and stops its own loops).
- `load-runs.txt`, `load-batch.txt`: every load run, by size.
- `mutants.py`, `mutants.txt`.
- `gate.txt`.
