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

Everything up to "Round 2" is round 1 as delivered (`06706509`), corrected
in place where review found it wrong; each such place says "corrected in
round 2". Round 2 is its own section at the end, with line numbers at its
head.

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
- That is a sufficient cause, not a proven one (corrected in round 2). The
  hold below fails the old case every time, but no lock-wait trace of the
  two failures seen was captured. The case also failed alone, with no other
  test's start in the process, at load about 100 (`load-runs.txt`): a slow
  start of its own does the same. The fix removes the clock either way.

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

  So load alone reproduces it rarely, and only at the machine's limit. A
  slow start does it every time; the harness supplies one on purpose. The
  loops ran only during each measured batch, at the sizes above.

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

No case now lands a copy through the session's own Drop: the landing case
calls `end` directly. The Drop end to end rests on this kill case's
`took >= GRACE` and on Drop being `end(|began| began.elapsed() >= GRACE)`
(review's note, recorded in round 2).

## GREEN

- `hold-runs.txt`: the same 1.5 s hold on the start lock against the new
  body (`harness.py head`): **10 of 10 pass**. The gate opens at the end's
  first look, the start goes through after 1.5 s, and the only bound is
  `LOADED`.
- `load-batch.txt`: the three converted cases, 6 runs each, at 54 busy
  loops with `-b`: 18 of 18 pass. That batch does not tell base from head
  (corrected in round 2): the base passed 6 of 6 in it too. It shows only
  that the converted cases do not fail under that load.
- The whole suite twice in the gate (below).

## Mutants (round 1)

Round 2's are `r2/mutants.py` and `r2/mutants.txt`. `mutants.py`, in a `git clone --no-local` at `042908ae`: each a one-place
change to `copy.rs`, then the case named. `mutants.txt`: **4 of 4 killed**.

| mutant | killed by |
|---|---|
| C1 the end gives a copy up before it waits (the session kills before the answer) | `an_ending_session_lets_a_copy_that_answers_land` |
| C2 the end never waits: the grace is never asked | the same |
| C3 a session's Drop gives no grace | `an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread` (`took >= GRACE`) |
| C4 starting a copy waits for it | `a_copy_is_started_and_answered_later_rather_than_waited_for` (after the watchdog's 60 s) |

One mutant was left out because it hangs rather than fails: the end, its
grace over, joins without giving the copy up. Its join waits on a tool
behind a gate nobody opens. Round 2's capped gate makes it a failure, and it
runs there (C5).

## The other cases of the same files, checked

A sleep racing a fixed grace or deadline, or a wall-clock bound a loaded
machine can cross:

| case | what times it | verdict |
|---|---|---|
| `copy.rs:221` the kill case | `took >= GRACE` (always holds); `took < GRACE + 10 s`, against a tool whose own end is `exec sleep 30` | not this shape: the bound is "kill and join", milliseconds, with 20 s to spare before the tool would end by itself |
| `copy.rs:340` selection copy | waits up to `LOADED` for a tool that keeps what it is fed | no grace in it |
| `copy.rs:362` no tool | none | |
| `clipboard.rs:889` a tool that leaves before reading | `exit 0` against a 4 MiB write | order: the write fills the pipe and fails when the tool has gone; no clock |
| `clipboard.rs:929` a file copy past its deadline (macOS) | deadline 200 ms against `exec sleep 30`; `elapsed < 10 s` | **corrected in round 2: PLAUSIBLE, this class.** The clock starts before `copy_file` (`:932`), and the deadline only after the start (`run`: `spawn_unless` `:256`, `hold` `:261`, the deadline `:272`). So the 10 s holds any wait for another start and the fresh script's first start. Round 1's reason was wrong. Converted in round 2 |
| `clipboard.rs:954` a copy given up from another thread | waits for the tool (`until`, 60 s), then `elapsed < 5 s` against `exec sleep 30` | kill and join against a 30 s tool |
| `clipboard.rs:1000`, `:1029`, `:1057`, `:1087` | the stop's lock and the start lock held by the case (`held`, `hold_every_start`) | order already |
| `clipboard.rs:1116`, `:1145` (macOS) | waits for the tool, then a stop; `waitid` for the exited child | order |
| `clipboard.rs:724` `real_osascript` | 60 s for a real interpreter | a bound for "never", not a race |
| `src/tui/trace.rs:306` (also an `Errands` user) | `took < GRACE + 10 s` against a hanging tool (30 s) | **corrected in round 2: PLAUSIBLE, this class.** The clock starts right after `start` (`:314-315`), so it holds the report's write and `copy_file_until`'s wait for another start (`src/trace.rs:121`, `process.rs` SPAWNING); the join cannot end before that start does. The kill case starts its clock after its tool has started (`copy.rs:231-237`), so this is not its shape. Converted in round 2 |

## Gates (round 1)

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
- `mutants.py`, `mutants.txt`: round 1's.
- `gate.txt`: round 1's.
- `r2/`: round 2 (its own list, below).

## Round 2

Two reviews of round 1 (`06706509`): one conditionally ready with one MUST
item, one APPROVE-WITH-FIXES with one MUST (two rows of the sweep table).
This section lists each item, what changed, and what shows it. Line numbers
are at `f39790e1`.

| commit | what |
|---|---|
| `a093bb36` | RED: the gate's cap, two cases, failing (item 3) |
| `63427ceb` | the gate gives up and ends with its case; each gated case asks `gave_up` first (item 3) |
| `026a979a` | RED: an end that panics in its look at the grace (item 1) |
| `b26101f3` | the end's threads in a guard that stops and joins them (item 1) |
| `b0c73b5a` | the two cases the sweep got wrong, off the start's clock (item 2) |
| `d42ab154` | merges main `e6cde019` (train 3, #79): no file of this branch is among main's changes since `e51685d6`, no conflicts |
| `f39790e1` | /trace's end case: the copy's deadline past the gate's, after a mutant survived (below) |
| the next | this receipt |

The product change in round 2 is `Errands::end` (`src/tui/copy.rs:97-121`).
Everything else is under `#[cfg(test)]` or `#[cfg(all(test, unix))]`.

### 1. A panic inside the end's look at the grace (gpt6 MUST)

- **The defect.** `end` takes every thread out of the errands
  (`std::mem::take`), then asks `over` while one still runs. A panic in
  `over` (a case's `tool.open()`, which expects its write) unwound past the
  stop and the join. The errands' Drop after it found nothing, so the tool
  was never killed and the copy's thread was left behind.
- **The fix** (`copy.rs:97-121`). The taken threads are held in `Ending`,
  whose drop stops every one and then joins every one: at the end's own end,
  as before, or on the way out of a panic. Neither step can panic: `Stop::stop`
  handles a poisoned lock, and a join's result is not looked at. So the drop
  cannot abort while one unwinds. A session's end still waits, then stops
  all, then joins all.
- **The case:** `an_end_whose_look_at_the_grace_panics_still_stops_and_joins_every_copy`
  (`copy.rs:331`). A gated copy is started and has started its tool, then
  `end` is called with a look that panics, inside `catch_unwind`. Right
  after, the case asks three things. The tool is dead. `copies.finished()`
  is `Some(false)`: the copy's thread sent its answer, so it is over. The
  gate did not give up.
- **RED** (`r2/red-panic.txt`, at `026a979a`): it fails at the first
  question, "a panic in the end left the tool running". The tool it left
  ended on its own once the case's directory went (item 3): no fake tool was
  running after the run. **GREEN** (`r2/green-panic.txt`, at `b26101f3`):
  it passes. Mutant P1 (the guard does nothing while unwinding) is killed by
  it.

### 2. The two sweep rows (opus MUST)

Both rows are corrected above to **PLAUSIBLE, this class**, with the reason.
Both could be made state-based in the same files, so both were converted,
RED first.

- **RED** (`r2/hold12.py`, `r2/hold-runs.txt`). In a throwaway clone, each
  case is given a first statement. Another thread takes the process's start
  lock (`crate::process::hold_every_start`) and holds it for 12 s, as
  another test's slow start does in a loaded suite. On round 1's bodies (at
  `b26101f3`), both fail **3 of 3**:
  - `clipboard.rs` `a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure`
    fails at "the copy waited for a tool that never answered" (12.2 s);
  - `tui/trace.rs` `an_ending_session_joins_a_report_whole_and_gives_its_copy_up`
    fails at "the end waited on the tool: 11.87 to 12.00 s".
- **GREEN**: the converted bodies (at `b0c73b5a`) under the same hold pass
  **3 of 3** each.
- **The file copy's deadline case** (`clipboard.rs:929`) runs the copy on a
  thread. It starts its clock once the tool says it has started, or once the
  copy is over if that comes first, as the deadline itself does. It still
  pins the deadline's size: from the tool's start, the copy is over within
  10 s against a 30 s tool, and it is a failure. If the deadline is ignored,
  mutant D1 waits out the tool and is killed.
- **/trace's end case** (`tui/trace.rs:306`) has no clock. Its file copy
  says who it is, then waits at a gate nothing opens
  (`FakeTool::gate_file_copies`, `clipboard.rs:1464`). An end that waited
  for the tool would wait until the gate gave up, and the case fails on
  `gave_up`; mutant C5 is killed that way. The copy's own deadline is an
  hour (`f39790e1`). At `b0c73b5a` it was the fake's usual 60 s, shorter
  than the gate's give-up, and C5 survived: the deadline ended the copy at
  60 s before the gate gave up (`r2/mutants-first.txt`). It also asks that a copy given up
  kept no path. GRACE's own size is pinned by `tui::copy`'s kill case
  (`took >= GRACE`, mutant C3), as before.

### 3. A gate that gives up (opus SHOULD)

- **The gate** (`clipboard.rs:1419-1450`, `FakeTool::gate()` /
  `gate_after(looks)`). It looks for its `open` file at most
  `GATE_LOOKS` = 12,000 times, 0.01 s apart. That is two minutes at the
  least (the mutants measured 180 to 185 s, each look's `sleep` a fork):
  past the minute any case gives a tool to start or answer, and the copy
  cases' watchdog opens its gate before then. It then writes `gave-up`
  and exits 1. It also exits 1 as soon as its directory is gone.
- So a case that never opens its gate fails instead of hanging, and a tool
  left at its gate ends on its own: by a failed case, a panicking end
  (item 1), or a killed test binary. Each gated case in `tui::copy` asks
  `!tool.gave_up()` first, so a start or an end that waited for the tool
  fails there, naming that reason.
- **The cases**: `a_gate_nobody_opens_gives_up_and_says_so` (`:986`; a gate
  of 3 looks) and `a_tool_at_its_gate_ends_itself_once_its_case_is_gone`
  (`:1009`; the tool's case drops it while it waits).
- **RED** (`r2/red-gate.txt`, at `a093bb36`): the gate seam was there with
  the old, uncapped line behind it, so the cases ran rather than failed to
  build. Both failed after their minute, each naming its tool's pid. Each
  left its tool polling with its directory gone (ppid 1); both were killed
  by hand. **GREEN** (`r2/green-gate.txt`, at `63427ceb`): 1.9 s and 0.4 s.
  Mutants G1 to G3 are killed by them.

### 4. The other review items

| item | done |
|---|---|
| gpt6 NTH: the no-stop mutant hangs | the capped gate makes it a failure: C5, against the after-grace case and /trace's end |
| gpt6 NTH: qualify the root cause | "Why it failed" now says sufficient, not proven, and why |
| opus (2): no case lands a copy through the session's Drop | a sentence under the cases table |
| opus (3a): C4 was run against the start case only | C4 is run against the landing and the after-grace cases too |
| opus (6): no mutant credited to the after-grace case | C4 and C5 against it |
| opus (6): `load-batch.txt` does not tell base from head | said under GREEN |
| opus SHOULD 2: convert the two cases | item 2 |
| opus SHOULD 3: the session-end copy queued on the start lock | not fixed here (COPY-AT-EXIT); what the runs show is below |

**COPY-AT-EXIT, as seen here.** In `r2/hold-runs.txt`, /trace's end case
under the 12 s hold took 12.0 s in every run. Round 1's body said so in its
failure ("the end waited on the tool: 11.87 to 12.00 s"); the converted body
passed in 12.01 s. The end waited GRACE, then gave the copy up. Its join
then waited out the other start, and the copy kept no path: once the stop is
set, a start that gets the lock is refused (`spawn_unless`).
The exit pays the other start's whole wait, and still loses the copy.

**Seen, not swept:** `tests/tui.rs:13003`
`a_quit_right_after_slash_trace_leaves_the_whole_report_and_no_clipboard_tool_behind`
bounds a whole xfx process's exit at 4 s with a hanging file copy. That is
the same class: the 4 s holds GRACE and the fresh fake `osascript`'s first
start, which `FakeTool::clipboard`'s own note puts at 0.5 to 2.4 s on an
idle machine. It is PLAUSIBLE and has not been seen to fail. It is not
converted here: it is outside this unit's files, and it times a whole
process, so it needs its own unit (a follow-up of this one).

### Mutants

`r2/mutants.py`, on a `git clone --no-local` at `f39790e1`, built in its
own target directory. Each mutant is one change, followed by the case named.
Fake tools left orphaned are counted and killed after each run.
`r2/mutants.txt`: **13 of 13 killed**, none orphaning a tool but G2, whose change is the orphan (the driver killed it).

A first run at `d42ab154` (`r2/mutants-first.txt`) is kept because it
found two faults:
- C5 survived against /trace's end case, fixed in `f39790e1`.
- G1 did not build: the change dropped the format argument it replaced,
  and the driver counted the build error as a kill. G1 now keeps the
  argument (`… || true || …`), and the driver says NOT BUILT for a change
  that does not compile.

| mutant | killed by |
|---|---|
| C1 the end gives a copy up before it waits | `an_ending_session_lets_a_copy_that_answers_land` |
| C2 the end never asks the grace | the same |
| C3 a session's Drop gives no grace | `an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread` (`took >= GRACE`) |
| C4 starting a copy waits for it | `a_copy_is_started_and_answered_later_rather_than_waited_for`; and, run again, the landing case and the after-grace case (each on `gave_up`, once the gate gives up) |
| C5 the end joins without stopping (round 1 left it out: it hung) | the after-grace case and /trace's end case, each on `gave_up` |
| P1 the end's guard does nothing while unwinding | `an_end_whose_look_at_the_grace_panics_still_stops_and_joins_every_copy` |
| G1 the gate never gives up | `a_gate_nobody_opens_gives_up_and_says_so` |
| G2 the gate does not end with its directory | `a_tool_at_its_gate_ends_itself_once_its_case_is_gone` |
| G3 the gate gives up without saying so | `a_gate_nobody_opens_gives_up_and_says_so` |
| D1 a file copy ignores its deadline | `a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure` |

### Gates

`r2/gate.txt`, at `f39790e1`, every CI step, one at a time (load 7 to 9,
the machine's own; no loops of this unit's ran during it, and the mutants
were done before it began). The same list passed at `d42ab154` before the
last test change; that log is `r2/gate-d42ab154.txt`.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same, `--features fault-injection` | ok |
| `cargo test --locked` | 15 suites, 5230 passed, 0 failed (7 ignored) |
| `cargo test --locked --features fault-injection` | 15 suites, 5267 passed, 0 failed (7 ignored) |
| `scripts/check-*.sh` (6) | all ok |
| `scripts/test-*.sh` (3: `check-no-secrets`, `gate-env`, `preview-contract`) | all ok |
| `scripts/fxcap/test-sweep.sh` | test_sweep 50 OK, test_ci_sweep 26 OK |
| `scripts/fxcap/test-capture.sh` | **2 of 42 failed** in the gate; 42 OK alone right after (below) |
| `cargo build --locked --release`, `scripts/smoke.sh` | 47 checks, 0 failures |
| `cargo build --locked --release --features fault-injection --target-dir target/faulty`, `scripts/smoke-tui.sh` | 37 scenarios + the oracle, 1127 checks, 0 failures |

**fxcap test-capture in the gate** (`r2/fxcap-capture-flakes.txt`). Two
of its 42 cases failed. This branch changes nothing under `scripts/`
(`git diff e6cde019 f39790e1 -- scripts` is empty). The same suite passed
42 of 42 at `d42ab154`, and 42 of 42 alone right after. Both are row 19's
class, in the harness:
- `Verbs.test_models2_and_modelsdelay`: the second catalog came a fixed 5 s
  after the gateway started. This is the 14th sighting, fixed on
  `fix/p5-flake-hint4` (`d40bf1a3`), not yet on main.
- `Verbs.test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run`
  (`scripts/fxcap/test_capture.py:385`): `wait 1`, then `signal`, against
  a fresh wrapper script whose body is `exec /bin/cat`. At 1 s the pid
  still ran `/bin/sh`: the fresh script's first start, which macOS
  assesses, had not reached its `exec`. A new sighting. It is not fixed
  here: it is outside this unit's files.

### Files (round 2)

- `r2/red-gate.txt`, `r2/green-gate.txt`: item 3.
- `r2/red-panic.txt`, `r2/green-panic.txt`: item 1.
- `r2/hold12.py`, `r2/hold-runs.txt`: item 2's 12 s hold on the start
  lock, and its runs, 3 a side for each case.
- `r2/mutants.py`, `r2/mutants.txt`; `r2/mutants-first.txt`, the first
  run at `d42ab154`.
- `r2/gate.txt` (at `f39790e1`), `r2/gate-d42ab154.txt`,
  `r2/fxcap-capture-flakes.txt`.
