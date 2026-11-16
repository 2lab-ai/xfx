# CI-PTY-WAIT — receipt

Unit: CI-PTY-WAIT. Base `dcfa5146` (main). Branch `fix/p5-ci-pty-wait`: one
product commit, `f79f5e5b`; five test commits, `f311ad79`, `a70dcd89`,
`a77fad76`, `7b524d78` and `765f9170`; and the receipt commits. Joined with
train 16 (§9): the merge `931418d4`, a join test `9a54c911`, the re-points
`3e17645a` and a clippy fix `ba133c25`, the code head. Every figure
below comes from a file in this folder; paths are scrubbed (`<runner>`,
`<tmp>`, `<home>`).

## The question

Within an hour, CI's `aarch64-apple-darwin` row (macos-15 arm64) failed three
different PTY tests in `tests/tui.rs` on two PRs that change no product code.
Was it a starved runner, a product bug that load exposes, or a wait that can
never come true?

**Answer: each test has its own cause. One is a product defect that load
exposes, one is the harness pausing under load, and one is not timing at all.**

| test | CI failures | cause | layer fixed |
|---|---|---|---|
| `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | 5 of 50 jobs (PR #94: 3 of 3 attempts), and 2 more since: PR #96 (release) and run 42971215970 (`ci/table.md`, last table) | **product** (§2a): one drain could read a prompt's turn start, its answer and its end, so the session reported `idle`, `done` with no `working`, or wrote `working` behind a frame showing `Generating`. fx cannot do either: its keystroke holds the worker's start until it has reported `working`. | `src/tui/event_loop.rs`, `src/tui/shell.rs` |
| `a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing` | 1 of 50, and 1 on the x86_64-apple-darwin row (job 130320282880) | **harness under load** (§2b): the paste body was typed as keys (measured, `ci/hostile-keys.txt`). The TUI does that only after a 50 ms quiet inside the paste, and the harness is the terminal: its writer thread, when not scheduled for that long, writes that quiet. Recipe B reproduces it at base, with the writer's pause measured on every failing run. Which way it happened on CI is [inferred]: the CI logs do not show the writer's gap. | `tests/support/pty.rs`, `tests/tui.rs` |
| `slash_skills_lists_installs_from_a_local_directory_and_never_from_the_network` | 1 of 50 | **test, not timing** (§2c): `$foo` also searches each skill's path, and the sandbox root is `.tmp` plus 6 random letters and digits. A root whose name holds `foo` lists every skill. | `tests/tui.rs` |
| `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes` | 1 of 50 (2026-11-08, at `fdc5ac0b`) | not reproduced at base (§2d). `31b38c84` (2026-11-09) changed how a stopped compaction settles between that head and base; that this is the fix is [inferred]. | none |

## 1. CI evidence — `ci/`

- `ci/jobs.json`, `ci/table.md`: every `aarch64-apple-darwin` gate job of the
  last 45 CI runs, every attempt: 50 completed jobs, 41 of them passing. For
  each job: the failing step, the failing test, the panic location, and each
  step's `tests/tui.rs` run time. `ci/table.md` ends with the x86_64 job the
  dispatcher reported later.
  - Made by `ci/collect_jobs.py` (jobs and step times from
    `gh api repos/2lab-ai/xfx/actions/runs/<run>/attempts/<n>/jobs`), then
    `ci/table.py` and `ci/table_md.py` over the job logs
    (`gh api .../actions/jobs/<job>/logs`). The logs are not committed.
- `ci/excerpts/<job>.txt`: each failing test's own message. `ci/excerpts.py`
  scrubs it: runner paths become `<runner>`, escapes are shown as `␛`.
- **The failing runs were not unusually slow** (`ci/table.md`, last lines). At
  the current suite size, the failing jobs' `tests/tui.rs` runs took
  72.7–84.5 s and the passing ones 72.5–79.3 s. The two slowest failing runs
  (84.5 s and 83.5 s) are the two that sat out a 20 s wait. These failures
  happen at CI's ordinary load, not on an outlier runner.

## 2. Causes

### 2a. `working` skipped, or written before any `Thinking` frame: a product defect

What the failures show (`ci/excerpts/`):
- Jobs `128812788998`, `129416346998` and `130270115640` (tests/tui.rs:852),
  and since the sweep `130359885817` and `130408681868` (PR #96): the reports
  were `[idle, done]`, with no `working` at all.
- Jobs `130258279160` and `130276081022` (tests/tui.rs:857): the frame before
  `working` shows `• Generating (0s) (↑4 ↓5)`, no frame ever painted
  `Thinking`, and `working` and `done` came back to back.

**fx's rule, from its code at the pin (`git -C <fx> show 481104fd:<path>`):**
1. The keystroke installs a pending submission and holds the worker's next
   start (`installPendingSubmission`, `src/core/app/input_submit_runtime.zig:1123-1150`;
   `tryHoldTurnStart`, `src/core/agent/worker_runtime.zig:1272-1284`). The
   keystroke's frame shows the prompt and `• Thinking`
   (`osc/w01-working` on the research branch: the `Thinking` frame, then
   `working`).
2. That frame commits (`notePendingFrameCommitted`,
   `src/core/app/app_render_runtime.zig:1042-1045`; `noteCommittedFrame`,
   `input_submit_runtime.zig:289-309`).
3. On the next pass, `collectPendingSubmissionFacts`
   (`input_submit_runtime.zig:311-405`) calls `finalizePendingSubmission`. It
   queues the prompt and calls `WorkerAppRuntime.syncState`
   (`src/main.zig:1245-1274`), and `syncState` reports `working`, busy because
   `queued_count > 0` (`src/core/app/app_worker_runtime.zig:762-790`).
4. Only after that do `markQueued` and `releaseTurnStartHold` let the worker
   start (`input_submit_runtime.zig:395-405`).

So in fx `working` is always written, and always after a frame showing
`Thinking`, however fast the turn: the worker cannot start the turn before the
report. The test states fx's rule, and xfx broke it under load.

**xfx's path at base** (line refs at round 3's head, where the same steps
live):
- Nothing holds the runtime: it starts the prompt as soon as it is sent.
- Each tick reads the keys, then drains the runtime, taking everything on the
  channel (the loop's turn, `loop_turn`, `src/tui/event_loop.rs:357`: the keys
  at `:366`, the drain at `:376`; `drain_ui_events`, `:427`).
- A prompt's `working` waits for the frame that first shows its turn
  (`take_status_moved_by_input`, `src/tui/shell.rs:11667`; `Keyed::NotYet` at
  `:11696`).
- So when one drain held the turn's start, its answer and its end, that tick
  reported the settled `done` and nothing else. When it held the start and the
  first answer bytes, the turn's first frame showed `Generating`.

**Fix** (`f79f5e5b`):
- The drain stops right after a turn start whose `working` is owed after the
  frame that first shows the turn. That is a prompt held for its turn, or one
  this tick's keys sent (`Shell::turn_start_owes_its_frame`,
  `src/tui/shell.rs:11724`; the stop, `src/tui/event_loop.rs:458`). Since
  round 2 the predicate is the tick's own answer, `keyed()` (`:11681`), which
  `take_status_moved_by_input` takes: §7.
- That tick's frame shows the turn beginning (`Thinking`) and `working` follows
  it. The rest of the turn is read on the next tick.
- The OSC 7501 row in `docs/parity.md` says so.
- `a_turn_start_owes_its_frame_exactly_when_its_tick_writes_working_after_the_frame`
  (`src/tui/shell.rs:22728`; `a77fad76`, widened in round 2) holds the drain's
  stop to the tick's own `Keyed::AfterFrame`: for the four combinations of held
  and keyed, and for a held prompt whose start a stop's count covers
  (`cancelling`) or a reset's drain drops (`draining`). At round 1 the stop was
  the two flags alone, which a covered or dropped start with no key behind it
  stopped for while the tick wrote nothing after its frame (opus r1 NIT 1: one
  extra tick, no report moved). Since round 2 both ask one `keyed()`, so
  "exactly" holds by construction and the case pins that it stays so.

**RED:** `local/red-unit-base.txt` shows the new unit case
`a_turn_read_whole_in_one_drain_is_working_after_a_frame_that_shows_its_thinking`
failing against base product code: the start's tick read past the start, with
`ANSWER` and the end in it. The case drives the real drain over a real channel,
in both orders: the start read on the tick after the keys, and in the keys' own
tick. It passes at head.

**At the PTY level, neither local recipe reproduced this race** (§3):
- 0 failures in 20 base runs (recipe A), and 0 in 18 (recipe B).
- Single runs at base under `taskpolicy -b`, and under the fault injector's
  `slow-ui` (40 ms after each frame), all passed (`local/probes.txt`).
- [hypothesis] The race needs the UI thread off the CPU while the runtime runs,
  and both recipes slow the whole process. The base-fails evidence is the unit
  RED and the five CI failures.

### 2b. The hostile paste typed as keys: the harness paused inside its paste

`ci/hostile-keys.txt` counts what the two CI failures typed:
- Job `130258279160` (arm64): the paste body was typed as keys from its line
  `K0000822` on: 1206 lines in the composer, and 71 requests the fake gateway
  had no script for.
- Job `130320282880` (x86_64, the same test, PR #93): from line `K0001743` on:
  304 lines, and 2 unscripted requests.

The same thing happened on two rows, so it is not specific to the arm64
runner.

The TUI ends a paste's quarantine once the input has been quiet for
`PASTE_QUIET` (50 ms, `src/tui/input.rs:835`): "a terminal that pauses that
long inside one paste ends the quarantine there" (`src/tui/input.rs:195-196`).
That is a documented residual. In the test, the terminal is the harness's
writer thread (`Session::write_raw`, which sleeps 2 ms whenever the queue is
full), and a thread that a loaded runner does not schedule for 50 ms writes
exactly that pause.

**What the study build shows.** It is a diagnostic instrument, not a candidate.
It was built from the working tree between `f79f5e5b` and `f311ad79`, with the
product fix in, and its banner reads the base revision. Its harness measures
the paste's longest pause, prints `STUDY-GAP`, and asserts whatever the pause
was, with no retry.
- **Recipe A** (`local/recipe-a/study.txt`): 40 runs, the longest pause
  11.9–13.2 ms, no failure.
- **Recipe B** (`local/recipe-b/study.txt`): 36 runs, every one with a pause of
  at least 50 ms (305 ms to 3.5 s), and every one failed with the paste body
  typed as keys. That is CI's symptom.
- Across both recipes, no run failed with every pause under 50 ms.

**Fix:**
- `Session::write_raw_measured` (`tests/support/pty.rs:371`, `f311ad79`) bounds
  the longest the terminal can have gone without a byte of the paste: from one
  write attempt's start to the next attempt's return, carried across the
  4 KiB parts.
- `paste_unpaced` (`tests/tui.rs:3693`, `7b524d78`) stops writing at the first
  pause of `PASTE_QUIET` or more. That session is **timing-indeterminate**:
  the measure is an upper bound on how long the input queue can have been
  empty, not a reading of it, so the session neither proves nor disproves the
  quarantine. Since round 2 it keeps its evidence -- the pause, and the
  screen a second later -- printed and handed back.
- `unpaced_case` (`tests/tui.rs:3819`) then runs a new session, at most 3 in
  all, and fails saying so when every session paused, with what each screen
  held.
- A session whose paste went out unpaced makes every assertion it made before.
  `PASTE_QUIET` itself is untouched.
- The harness's own cases (`a70dcd89`) hold the measurement and the retry.
- `paste_unpaced` prints the paste's longest pause (`765f9170`). The test
  captures it and shows it only on failure, so a CI failure says which way it
  went: the "no session tested the paste" panic is the harness pausing, and a
  20 s timeout after a paste that never paused is the product's.
- The early stop exists because, at `a77fad76`, a paused session went on
  writing the rest of the body, which was being typed as keys by then. That
  slowed reading enough that, in 2 of 18 head runs, one part outlasted the
  write's 20 s bound (`local/recipe-b/failures.txt`).

### 2c. `$foo` listing all three skills: the sandbox's random name

- **Repro:** `local/skills-foo-tmpdir-base.txt`. The base binary, with `TMPDIR`
  under a directory whose name holds `foo`, draws CI job `130267239024`'s last
  frame byte for byte: `Skills 3` with `foo`, `demo` and `release-notes` under
  `$foo`.
- **Mechanism:** a `$` search matches a skill whose path holds the word
  (`rank`, `src/tui/skills_menu.rs:149`, the path at `:165`), and
  `TempDir::new()` names the sandbox root `.tmp` plus 6 random letters and
  digits.
- The same job's plain step passed this test on the same runner, so the
  machine's own temp directory does not hold `foo`; the random name did.
- That happens about 4 × (2/62)³ ≈ 1 in 7 400 per run, for each of the two
  skill-search cases (`foo`, and `dem` in `a_dollar_token_*`). That is an
  estimate: no log shows this run's sandbox name.

**Fix** (`f311ad79`, widened in round 2 `d4f660f7`): `skills_sandbox`
(`tests/tui.rs:14664`) reads the sandbox's whole `home` and `workspace` paths,
case folded (`found_in_sandbox`, `:14694`). A word in the root's random name
draws the root again, as `src/tui/shell.rs`'s own `skilled_in` fixture does; a
word above the root -- the machine's `TMPDIR`, which no draw changes -- fails
at once, saying so, where the case used to time out
(`local/skills-foo-tmpdir-round2.txt`: 21 s at `c601ef29`, at once at round 2).
`a_skills_sandbox_path_never_holds_what_its_case_searches` (`:14717`) pins
`found_in_sandbox`.

### 2d. The 2026-11-08 compaction case

`ci/excerpts/128747443572.txt`: `Compaction cancelled.` was still on screen
behind `MARKER-THREE`, at `fdc5ac0b`.
- `31b38c84` (2026-11-09, on main before base) replaced how the band settles a
  stopped compaction.
- At base the test failed in 0 of 20 runs (recipe A) and 0 of 18 (recipe B).
- That `31b38c84` is what fixed it is [inferred]: no run at `fdc5ac0b` was
  made here.

## 3. Rates — `starve.sh`, `rates.py`, `local/`

The recipe is `starve.sh`. It runs the five tests many at once, every build
in the same round so that the builds share one machine state, for a fixed
number of rounds, under the suite lock (one slot). It uses no busy loops: the
load is the tests' own beside the machine's other suites.

- **Recipe A**: 3 builds × 5 tests × 2 copies, so 30 at once, for 10 rounds.
  Base is the plain test binary at `dcfa5146` (its xfx is base's
  fault-injection build, with no fault set), head is at `a70dcd89`, plus the
  study build. `local/recipe-a/`
- **Recipe B**: `STARVE_BACKGROUND=1` (every test at background QoS, so the
  machine's other work comes first), 3 builds × 5 tests × 3 copies, so 45 at
  once, for 6 rounds. Base is the fault-injection binary at `dcfa5146`, head is
  at `a77fad76`. `local/recipe-b/`
- **Recipe B, again (B2)**: the same recipe for 2 rounds, 2026-11-11
  09:55–10:02Z, head = the test binary at `765f9170` (`c601ef29` adds docs
  only). The machine's load averages at its end were 22.6 / 60.5 / 63.6
  (`local/recipe-b2/failures.txt`, first line), several times recipe B's. In
  every build, sessions then spent their 20 s budgets on the launch, the fake
  gateway's answer or the permission panel. `local/recipe-b2/`

Failures out of runs (`rates.md` in each folder):

| test | A: base | A: head | B: base | B: head (`a77fad76`) | B2: base | B2: head (`765f9170`) |
|---|---|---|---|---|---|---|
| status between frames | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | 0 / 6 | 2 / 6 (both a 20 s wait for the answer, `MARKER-STATUS`; neither the :852 nor the :857 assert) |
| hostile paste, types nothing | 0 / 20 | 0 / 20 | 18 / 18 (body typed as keys) | 18 / 18 (16 "no session tested the paste", 2 write bound) | 6 / 6 (5 write bound, 1 launch wait) | 6 / 6 (all "no session tested the paste") |
| hostile paste, answers no approval | 0 / 20 | 0 / 20 | 18 / 18 (body typed as keys) | 18 / 18 ("no session tested the paste") | 6 / 6 (5 write bound, 1 wait for the permission panel) | 6 / 6 (5 waits for the permission panel, before any paste; 1 "no session tested the paste") |
| slash skills | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | 0 / 6 | 0 / 6 |
| compaction Ctrl-C | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | 5 / 6 (20 s waits for a turn's answer or the launch) | 4 / 6 (20 s waits for a turn's answer) |

What the rates show:
- **Hostile paste.** In recipe B, base fails every run with CI's symptom. Head
  fails every run too, but the failure now says what happened: no session could
  write the paste unpaced, rather than the product typing the clipboard. In
  recipe A, where the longest pause was 13 ms, all three builds pass.
- **Status test.** Neither recipe reproduces it; its RED is the unit case
  (§2a).
- **Slash skills.** It is not a timing failure, so its RED is the foo-named
  `TMPDIR` (§2c), deterministic at base.
- **B2** discriminates nothing. Every failure, in every build, is a 20 s
  budget spent on something no fix here touches -- the launch, a turn's answer,
  the permission panel, the write bound -- or the harness's own "no session
  tested the paste". None is the status test's :852 or :857 assert, and none
  is head typing a paste that never paused. The study build, which carries the
  product fix, failed the status test 2 / 6 waiting for its first frame.
- The breakdown of every recipe B failure is in `local/recipe-b/failures.txt`;
  B2's, with the panic location and message of each, in
  `local/recipe-b2/failures.txt`.

## 4. Mutants — `mutants.txt`, `mutants.py`, `mutants-r1.txt`

`mutants.py` runs on a `git clone --no-local` copy, never the worktree. The
file now holds review round 2's set (§7); the two runs below were made with
the set as it stood at their commits.

- **Round 2, at `a77fad76`** (`mutants.txt`): 9 of 9 killed.
  - The drain never stops; or it stops after any event, not just a start.
  - The owed-frame predicate keeps only its held half; or only its keyed half.
  - The pause measurement is not carried across parts; or it never grows.
  - The retry gives up quietly; or it runs once.
  - The sandbox-name check is case-sensitive.
- **Round 1, at `a70dcd89`** (`mutants-r1.txt`): the keyed-only mutant survived,
  because the drain case's typed prompt sets both flags; `a77fad76` added the
  shell case that kills it. One mutant did not compile; its text was fixed in
  `mutants.py`.
- **Not covered by a mutant:** `7b524d78`'s early return in `paste_unpaced`.
  Without it, a paused session writes the rest of its paste and still returns
  the pause at the end. The two differ only in time and in the write bound,
  which no unit case observes.

## 5. Gate — `gate.txt`, `gates.sh` (and `gate-a70dcd89.txt`)

`gates.sh` ran once on a `git clone --no-local` snapshot at `c601ef29` (the
product fix and every test commit; only the receipt changed after it), under
the suite lock, 10:02–10:10Z (`gate.txt`):
- fmt, clippy and clippy with the fault injector: clean.
- `cargo test --no-fail-fast`, plain and with the fault injector: no failure
  (`tests/tui.rs` 192 and 206 cases). There was no catalog-wait failure to
  rerun.
- `check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh`,
  `check-no-local-paths.sh`: exit 0.

An earlier full gate, at `a70dcd89`, also passed (`gate-a70dcd89.txt`, with the
subset rechecks made at `a77fad76` and `7b524d78`).

## 6. Merged with main — `merged.txt`

`origin/main` (`35cb4c7a`, release v0.2.0) was merged into the branch as
`5ddfbbc0`. One conflict, in `docs/parity.md`: main rewrote four TUI rows next
to the OSC 7501 row. It was resolved as main's rows, with this branch's one
sentence in the OSC 7501 row; that row is otherwise main's text unchanged.

On a snapshot of the merge, under the suite lock (`merged.txt`):
- fmt and both clippys: clean.
- `cargo test --no-fail-fast`: no failure (`tests/tui.rs` 193 cases).
- The two new unit cases and the touched PTY cases with the fault injector:
  all passed. The hostile paste's longest pause was 4.7 ms.

## 7. Review round 2 — `round2/`

`origin/main` `e2fc7e94` (#97, fxcap scripts only) was merged first, as
`67e709c4`. Then the items from the two round-1 reviews
(`CI-PTY-WAIT/reviews/r1-gpt6.md`, `r1-opus.md`):

| item | change | where (at `62ea9f56`) | RED → GREEN |
|---|---|---|---|
| gpt6 SHOULD 1: keys between the consumed start and the queued end, and a sustained stream, through the real loop | `loop_turn` is `session`'s turn after its first reconcile, and `session` runs it. The equivalence case: typing, a stop, two stops, Escape, Ctrl-D and `/quit`, read between a split turn's start and its queued end, come to the same reports, screen, exit and runtime messages as when the end lands right after the start's drain. The sustained case: three channels of text behind the start; the start's tick reads nothing past it, and every later tick reads its keys and empties the channel. | `src/tui/event_loop.rs:357` (`loop_turn`), `:16512`, `:16565` | RED by `drain-never-stops` (both); `drain-stops-after-every-event` (sustained) |
| gpt6 SHOULD 2: a rejected sample | Described as **timing-indeterminate** in the code and here (§2b): the measure bounds how long the queue can have been empty and does not read it. Each rejected session keeps its pause and its screen a second later, printed, and the every-paused failure lists them. | `tests/tui.rs:3572-3680`; `:3725` | RED by `rejected-screens-unsaid` |
| …a quarantine regression cannot go green only through rejection and retry | `round2/quarantine.txt`: 3 quarantine mutants × 3 cases × 3 runs. **No run of any case rejected a session.** The new paced case (20 ms between parts: a loop turn and more, under `PASTE_QUIET`) failed 3/3 under each mutant, each time on an accepted session. The two unpaced cases stay green under the mutants: their refill gaps at idle (3–10 ms) are under one loop turn, so the loop never records a quiet's start there (`round2/quarantine-first-pass.txt`), which is what the paced case was added for. Narrowed in round 3 (§8): acceptance is measured on the writer and does not prove the reader saw each pause; the threshold itself is pinned without a clock by `src/tui/input.rs:5334` and `:5933`, not by this case; the 20 ms mutant's kill rested on the writer oversleeping its 20 ms sleep (gaps of 30–37 ms); and a regression can at worst give a misleading red, never a green. | `tests/tui.rs:3756` (paced), `:3579` (`paste_with_pauses`) | baseline 9/9 green; each mutant 3/3 red on the paced case |
| opus NIT 1: predicate | `keyed()` works out the take's answer, taking nothing, and both the take and the drain's stop ask it. A held start a stop covers (`cancelling`) or a reset's drain drops (`draining`), with no key behind it, owes no frame. "Exactly" now holds by construction, and the name keeps it. | `src/tui/shell.rs:11501`, `:11542`; case `:22274` | RED by `predicate-by-flags-alone` (round 1's predicate), `owed-only-when-held`, `owed-only-when-keyed` |
| opus NIT 3: the drain reads past a start that owes no frame | A prompt queued behind a running turn; and a tick whose keys owe a report (a question answered) with no start among its events. | `src/tui/event_loop.rs:16704`, `:16743` | RED by `drain-stops-at-every-start`, `drain-stops-after-any-event` |
| opus NIT 4: whole path | `found_in_sandbox` checks `home` and `workspace` whole, case folded. A word in the root's name draws again; one above it fails at once, saying so. | `tests/tui.rs:14483`, `:14513`; case `:14536` | RED by `search-check-root-name-only`, `search-check-case-sensitive`; `local/skills-foo-tmpdir-round2.txt` (21 s timeout at `c601ef29`, a clear panic at round 2) |
| opus NIT 5: `PASTE_QUIET` | Kept as a copy. The product's is `pub(crate)`, out of this suite's reach, and the comment names its line, `src/tui/input.rs:674`. | `tests/tui.rs:3630` | — |
| opus NIT 6: line refs | Every ref in this README was refreshed against `62ea9f56`; outside this table they are at the join's code head now (§9). | — | — |
| opus NIT 2: no frame between `working` and `done` when starved | Not fixed; recorded as open question 4. | — | — |

- **Mutants** (`round2/mutants.txt`, on a `--no-local` clone at `62ea9f56`,
  under the lock): the round-1 nine, re-aimed where the code moved, and five
  new. All 14 killed, with both baselines green (unit 6, harness 4).
- The first pass at `d4f660f7` (`round2/mutants-first-pass.txt`) had 2
  survivors, `drain-stops-after-any-event` and `owed-only-when-keyed`;
  `62ea9f56` adds the cases that kill them.
- **Touched test files** (`round2/touched-tests.txt`, on a snapshot at
  `62ea9f56`, under the lock): `--lib`, `--test tui` and `--test interactive`,
  plain and with the fault injector, all passed (lib 4933/4956, tui 194/208,
  interactive 79).
- **Paths:** the per-commit grep over
  `git rev-list --no-merges HEAD --not origin/main` prints 0.

## 8. Review round 3 — `round3/`

From the two round-2 reviews (`CI-PTY-WAIT/reviews/r2-gpt6.md`,
`r2-opus.md`). Refs are at the join's code head (§9).

### S1 — the paced hostile paste under load (`round3/`)

**Recipe:** `starve.sh` (recipe A, plain QoS), 20 rounds × 2 copies of seven
entries, so 14 processes at once: the whole tui suite in parallel (`@suite`),
the paced case, the two unpaced hostile cases, and the status, skills and
compaction cases. Every run used `--nocapture`.
- Each paste prints its longest pause and the pause it means to take.
  `round3/count_paced.py` counts them per session, for single runs (exact)
  and runs inside a suite.
- Each folder keeps `results.txt`, `counts.txt` and the raw per-session lines
  (`sessions.txt`).
- The machine's other suite-lock slots were not reserved, so other workers'
  jobs could run beside these (not recorded).

**Threshold, stated before the second measurement:** rejections are *rare*
when a paced session is rejected at most 1 time in 10, so that all three
sessions of one run are rejected less than 1 time in 1000, and no run is red
with all three rejected.

| version (commit) | paced sessions rejected: single / suite | unpaced, same runs: single / suite | runs with all three rejected | reds with every pause < 50 ms | rare? |
|---|---|---|---|---|---|
| 20 ms before every part (`6acbcd7e`) | 10/50 (20.0%) / 9/38 (23.7%)* | 12/92 (13.0%) / 4/63 (6.3%)* | 0 of 80 | 0 | no |
| 20 ms before parts 1, 64, 128, 192 (`661d78d9`) | 6/46 (13.0%) / 8/41 (19.5%)* | 11/91 (12.1%) / 3/63 (4.8%)* | 0 of 80 | 0 | no |
| **12 ms before parts 1, 64, 128, 192 (`f8bc123a`)** | **3/43 (7.0%) / 3/43 (7.0%)** | 3/83 (3.6%) / 0/80 (0%) | **0 of 80** | **0** | **yes** |

\* The first two suite counts are partial. `eprintln!` wrote each diagnostic
in pieces, and libtest's `test ... ok` lines landed inside 34 and 31 of them.
`f8bc123a`'s `say()` writes each line in one call, and 0 were cut.

- **Why:** macOS let a 20 ms sleep run to 29–83 ms under this load
  (accepted medians 36–39 ms), so each pause had about 12 ms of room below
  the 50 ms line.
- **At 12 ms:** accepted paced pauses were 18–48 ms (median 25 ms). 12 ms
  still outlasts a loop turn (8 ms), so the loop records a quiet's start at
  each pause.
- All 80 paced runs of each version passed: 240 paced runs, no red of any
  kind.
- **One other failure:** `a_streamed_answer_grows_the_document_only_inside_the_update_that_paints_the_band`
  failed once inside a suite ("xfx did not exit", a 20 s exit wait). It
  passed alone 3/3 (`s1-four-boundaries-12ms/other-failure.txt`). It is not
  this unit's case, and it is not counted above.

**The quarantine mutants on each version** (`quarantine.txt` in each folder):
- At 20 ms × 4: the paced case fails 3/3 under all three (any quiet, 5 ms,
  20 ms).
- At 12 ms × 4 (the version kept): it fails 3/3 under any-quiet and 5 ms. The
  20 ms one passes, because 18 ms gaps never reach 20 ms.
- No run of either version rejected a session.
- **The threshold is held by the lib, not by this case:**
  `round3/lib-threshold.txt` shows `src/tui/input.rs:5334` and `:5933` killing
  all three mutants, as opus r2 said.

### The other items

| item | change | where | RED → GREEN |
|---|---|---|---|
| S2: label the clock defect | The paced case's comment says a red with every pause under 50 ms is not retried and can be the clock-after-poll defect (`src/tui/event_loop.rs:750-751` → `src/tui/shell.rs:11303-11304` → `src/tui/input.rs:983`), which PASTE-QUARANTINE-CLOCK fixes; so does open question 1 here. | `tests/tui.rs:3942-3948` | docs |
| S3 and gpt6 r2 SHOULD: narrow the claims | The paced comment says acceptance is measured on the writer; the threshold is pinned by `input.rs:5334`/`:5933`, not by this case; and the 20 ms mutant's kill rests on the writer oversleeping. `unpaced_case` now says a regression gives at worst a misleading red, never a green. The round-2 table's row is narrowed the same way. | `tests/tui.rs:3934-3941`, `:3809-3818`; §7 | docs |
| NIT 1: predicate doc | "unless this tick's keys moved the status, when the tick writes its report after its frame whatever the start shows" | `src/tui/shell.rs:11710` | docs |
| NIT 2: clock-expiring cues | The equivalence case reads the hint row at the keys' own tick (`cue`) and leaves it out of the end screen. Its cue is asserted per key: "press ctrl+c again to exit" after a stop, the plain hint otherwise. | `src/tui/event_loop.rs:16390`, `:16480`, `:16540` | RED still by `drain-never-stops` (`round3/mutants-f8bc123a.txt`) |
| NIT 3: tail cut | `last_bytes` backs a 400-byte cut off to a character's start. | `tests/tui.rs:3755`; case `:3764` | RED by `last-bytes-cut-anywhere` |
| NIT 4: stop-next parity | Recorded as open question 5. | — | — |

- **Mutants** on a `--no-local` clone under the lock: the round-2 14 plus
  `last-bytes-cut-anywhere`, all 15 killed with both baselines green, at
  `6acbcd7e` and again at `f8bc123a` (`round3/mutants-*.txt`).
- **Per-commit path grep** over `rev-list --no-merges HEAD --not origin/main`:
  0. fmt is clean, and both clippys are clean.

## 9. The join with main and train 16 — `join/`

Main moved to `e0aedbf6`: #101 pins the CI gate to Rust 1.99.1, #99 is
LOCAL-PATHS-EXIT-TRAP and #100 is train 15. Train 16 (PR #102) is at
`8be4693b`, and `e0aedbf6` is its ancestor. This unit is joined against
`8be4693b`. Refs in this section, and in §1–§8 outside the round-2 table,
are at the code head `ba133c25`.

| commit | what |
|---|---|
| `931418d4` | `git merge --no-ff 8be4693b`: one conflict, resolved by script |
| `9a54c911` | the join test, appended at the end of `event_loop.rs`' tests |
| `3e17645a` | the 59 references the join moved, re-pointed, numbers only |
| `ba133c25` | the harness's `/bin/sh` through `support::command`, which the joined clippy requires — **the code head** |
| `1b1b62e8` and the next | this folder's `join/`, the mutant scripts' per-run bound, and this section |

### The merge (`resolve_parity.py`, `remerge.sh`, `remerge.txt`, `fns.py`, `fns.txt`)

- **One conflict, `docs/parity.md`'s OSC 7501 row.** This branch added one
  sentence to the row. Train 16 re-pointed two `worker.rs` references in it
  (`3922` → `3955`, `3561` → `3594`). `resolve_parity.py` takes train 16's row
  and puts this branch's sentence back after the same anchor. It refuses
  anything else: a second conflict, a missing anchor, or a difference between
  the sides beyond that sentence and those two numbers.
- **The resolution is reproducible.** `remerge.txt`: in a `--no-local` clone,
  `ef595df2` merged with `8be4693b` stops on that one conflict, the script
  resolves it, and the written tree is the merge commit's (`e57a5389`).
- **The auto-merged files** (`src/tui/event_loop.rs`, `src/tui/shell.rs`,
  `tests/tui.rs`). `fns.txt` takes 26 functions on this unit's path and
  compares each at the base `e2fc7e94`, at both parents and in the merge.
  The 26 include the worker's `turn_loop` and `run_turn`, which send what
  the drain reads; neither side changed them.
  - **0 changed by both sides.**
  - This branch changed `session`, `loop_turn`, `drain_ui_events`, `keyed`,
    `take_status_moved_by_input` and `turn_start_owes_its_frame`.
  - The trains changed `settle_band` (two `worker.rs` numbers in a comment),
    `consume_inputs` (key routing: C-j as an answer, a rule confirmation read
    as the approval reads it, the pickers' held arrows) and `input.rs`'
    `feed` and `flush` (META-PREFIX-DECODE's pair).
  - The merge holds each side's version of each function.
- **Train 16's event_loop.rs changes are two test comments.** `session` at
  `8be4693b` is the base's.
- **`tests/tui.rs` around this branch's harness.** TEST-ENV-ISOLATION builds
  every child's environment from `PATH` and `TMPDIR` alone and runs the cases
  in a copy of the test binary. The sandbox's `home` and `workspace` layout,
  which `found_in_sandbox` reads, did not change. The hostile-paste harness
  reads no environment.

### The semantic join

**MPD's per-turn flush runs once a turn in `loop_turn`, after the read.**
- fx flushes before the read. `loopCollectFacts` flushes the decoder and
  routes the keys it resolves (`main.zig:2975-2982@481104fd`), then ticks the
  worker (`:2983`). The loop reads after `collect_facts`
  (`event_loop.zig:80`, `:92-104`).
- xfx's flush is `Shell::settle_input` (`src/tui/shell.rs:11394-11398`).
  `loop_turn` calls it once (`src/tui/event_loop.rs:373`), after the read
  (`:366`) and before the drain (`:376`), and `session` runs `loop_turn` once
  a turn (`:279`). The base `e2fc7e94` and train 16's `8be4693b` have the same
  order in `session` itself (their lines 279, 286 and 289): `loop_turn`
  kept it.
- `Decoder::feed` flushes before each byte (`src/tui/input.rs:1069`), so a
  pair gone quiet is resolved ahead of the next byte read, as fx's
  flush-before-read resolves it. Nothing else resolves the ESC stage:
  `end_delivery_at` and `still_at` touch only the paste quarantine.

**The drain's stop asks the tick's own `keyed()` with a pair pending or
settled in that tick.**
- A pair still pending is no key yet. `flush` pushes nothing before
  `PAIR_TIMEOUT` (`src/tui/input.rs:1109-1121`), and `consume` returns on no
  events (`src/tui/shell.rs:12883-12885`), so `status_moved_by_input` is as
  the read's keys left it.
- A pair settled there is two Escapes through `consume`, as read keys are
  (`src/tui/shell.rs:11396-11397`, then `:12882-12891`, where the flag is
  set), before `take_ui_events`. So the drain's
  `turn_start_owes_its_frame` (`:11724-11725`) and the commit's
  `take_status_moved_by_input` (`:11667-11668`) ask the same `keyed()`.
- Train 15's decoder line changed `consume_inputs` (key routing) and not
  `consume` (`fns.txt`).

**A held prompt's start still ends the drain.** `keyed()` is this branch's,
unchanged by the trains (`fns.txt`). At a held prompt the turn is not
running, so a pair's Escapes are the composer's (`Gestures::escape`,
`src/tui/gesture.rs:147-175`: with a draft, armed then cleared). They move
no status, and `status_held && working` makes the start's tick
`Keyed::AfterFrame`.

**CATALOG-WAIT-CLOCK is not on this path.** It is test-only: `worker.rs`'
tests and one `[dev-dependencies]` line (its receipt's first paragraph).

**The join test**,
`an_escape_pair_settled_in_the_tick_that_reads_a_held_prompts_start_leaves_the_start_ending_the_drain`
(`src/tui/event_loop.rs:20722`). It is appended at the end of the tests
module, so no cited line moves.
- No test reached the meeting before. META-PREFIX-DECODE's pair tests drive
  the decoder (`input.rs`) or call `Shell::settle_input` directly
  (`shell.rs`), never the loop's turn with a start on the channel. This
  unit's loop tests send a lone `ESC`, in the tick after the start's.
- **Settled in that tick, through `loop_turn`:** `ab ESC ESC` is read, and the
  turn settles it at least `ESC_TIMEOUT` + 20 ms later. That bound holds
  however the clock falls. The draft is cleared; the drain stops, leaving the
  answer and the end; the tick shows `Thinking` before `working`; the reports
  are `idle`, `working`, `done`; and no stop is sent.
- **Still pending in that tick**, with the clock given to `settle_input`: the
  draft is kept, with the same stop and the same order.
- Its two mutants (below) are the order this question is about:
  `loop-never-settles-input` and `loop-settles-input-after-the-drain`. With
  the second, the pair's Escapes reach a running turn and interrupt it.

### What the join broke: one `Command::new` (`gate-3e17645a-clippy.txt`, `clippy-fix.txt`)

TEST-ENV-ISOLATION's `clippy.toml` disallows `std::process::Command::new`. An
integration test makes its child through `support::command`
(`tests/support/mod.rs`), as train 16's own `/bin/sh` tests do. This branch's
`the_harness_measures_a_pause_between_two_writes_of_one_paste` made its
`/bin/sh` with `Command::new`. The merge built and its tests ran, but both
clippy runs failed at `3e17645a` (`gate-3e17645a-clippy.txt`; that gate was
stopped there). `ba133c25` uses `support::command`. Both clippy runs and
`check-env-reads.py` pass (`clippy-fix.txt`). It is one line, with no line
count change, so no reference moved.

### References (`content-*.txt`, `readme-refs-*.txt`, `refs-bare-*.txt`, `repoint*.py`, `repoint-*.txt`, `numbers_only.py`)

The content check is train 16's `content.sh` and `refs_content.py`. Each
reference, comma lists included, is read against the commit that wrote its
numbers. `content.sh` skips `.prd/`, so this README is run through
`refs_content.py` alone. Old numbers below carry `@<commit>`, which the
checks skip.

| at | WRONG (code and docs) | WRONG (this README) |
|---|---|---|
| this branch before the join, `ef595df2` | 30 | 12 (the round-2 table) |
| train 16, `8be4693b` | 8 (train 16 left them) | — |
| the merge `931418d4` | 34 | 47 |
| the code head `ba133c25`, as it is | **9** | **18** |
| the same, blame ignoring the re-point `3e17645a` | 9 | 21 |

- **What the merge moved** (`repoint.py`, `repoint-plan.txt`; train 16's
  plan, with its LEAVE table kept). A WRONG reference at the merge is the
  merge's when neither parent reads it WRONG with the same citing file,
  frame and numbers.
  - In code and docs, 24 are the merge's: the comma list at
    `tests/tui.rs:2721` (six numbers), this branch's quarantine comments into
    `input.rs` and `shell.rs` (six), and the trains' references into
    `shell.rs` that this branch's lines above them moved (+36, or +104 past
    this branch's `shell.rs` test): the parity ledger's ten, `capfile.py`,
    `smoke-tui.sh` and `tests/tui.rs:9833`.
  - Re-pointed with them is one this branch had moved itself before the
    join: `event_loop.rs:1128-1131@04e4491d`, 29 lines below where
    `loop_turn` went in, now `src/tui/event_loop.rs:1157-1160`. The content
    check read it WRONG at `ef595df2` too.
  - Left: `docs/parity.md`'s transport reference pinned to an old commit's
    file (train 16's LEAVE). Blame at the merge reads that row through the
    first parent, so here it counts as the merge's.
  - Two list numbers no window places (the line
    `session.type_bytes(&[0x15, 0x04]);` stands at many places) are mapped
    through the diff from `8be4693b`'s `tests/tui.rs` to the merge's: +270,
    as the list's other moved numbers.
- **This README:** 29 head references re-pointed by the plan
  (`repoint-readme-plan.txt`). Left as written: the round-2 table (its 16,
  pinned at `62ea9f56` by its header), except the two `input.rs` numbers
  round 3 wrote into its text; and CI's two printed assertion lines (§1).
- **Bare continuations on a following line,** which `refs_content.py` does
  not read: five, by hand (`repoint_hand.py`, `repoint-hand.txt`, each
  checked against the line it names). Four are in this README (`keyed`,
  `Keyed::NotYet`, `found_in_sandbox` and its test), one in
  `scripts/fxcap/capfile.py` (train 16's own re-point, moved again).
  `refs_bare.py` (train 16's, reading this README too) found three. It
  cannot see the other two. `keyed`'s has another file's reference nearest
  it, and the test's has no reference in the four lines above it. Both were
  found by reading all 18 bare references in this README.
- **Numbers only:** `repoint-numbers.txt`, 6 files and 43 lines, each
  differing from `9a54c911` in its numbers alone. 59 references in all.
- **At the code head, 0 moved:**
  - Code and docs WRONG 9: train 16's 8 and the pinned transport reference,
    as it is and ignoring the re-point.
  - This README WRONG 18: the table's 16 and CI's 2. Ignoring the re-point,
    three more: references to `tests/tui.rs` comment blocks whose own
    numbers the re-point changed, each with its first line the same
    (`readme-refs-ba133c25-ignoring.txt`).
  - `refs-bare-ba133c25.txt`: 76 flagged, 66 in the historical plans and 9
    in the pinned table. The last is train 16's parity-ledger reference into
    `docs/parity.md`, whose lines 1–240 are the same at `8be4693b` and the
    head.

### Tests, flake runs and mutants

**The gate** (`gate.txt`): train 16's `gates.sh` is ci.yml's gate job at
the joined main, with the pinned toolchain. It ran at `ba133c25` on a
`--no-local` clone, in one hold of the suite lock (`toolchain.txt`: rustc and
cargo 1.99.1). **29 steps, 0 failed**, the tree clean after; the load at the steps' starts was 41.5 to 91.6.
- fmt and both clippy runs are clean.
- `check-env-reads.py` passes, and its self-test too.
- `cargo test --locked`: 6075 passed, 0 failed, 8 ignored over 14 test
  binaries. With `--features fault-injection`: 6112, 0 and 8.
- **This unit's tests and META-PREFIX-DECODE's on the touched paths**
  (`touched.txt`, read out of those two logs): this unit's 20 (its REDs, the
  join test, the lib threshold cases and the PTY cases) and MPD's 15, ok in
  both runs.

**Flake runs under load** (`flake/`: `run.txt`, `results.txt`, `counts.txt`,
`sessions.txt`, `reds.txt`).
- **Recipe:** `starve.sh` recipe A at `ba133c25`'s tui test binary. It ran 10
  rounds × 2 copies of five entries, so 20 runs of each and 10 processes at
  once. The entries were the whole tui suite (`@suite`), the status test, the
  paced hostile paste and the two unpaced ones. Every run used
  `--nocapture`.
- **Load:** the one-minute load was 41.8 at the start and 74.8 at the end, on
  18 cores; the gate and other workers' builds ran beside it.

| test | green / runs |
|---|---|
| `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | **20 / 20** |
| the paced hostile paste | 18 / 20 |
| the unpaced hostile paste, types nothing | 18 / 20 |
| the unpaced hostile paste, answers no approval | 20 / 20 |
| the whole tui suite (198 tests) | 18 / 20 |

- **Every red is round 8's, and every one is the harness giving up, saying
  so** (`reds.txt`). Each of a run's 3 sessions paused 50 ms or more (51 to
  129 ms): the writer was not scheduled. **No red came from an accepted
  session.**
- `counts.txt` files one suite red as "on an accepted session". That is a cut
  line. Its message is two lines below its panic, past other tests' output,
  and reads "each of 3 pastes paused for 50ms or more". The `[50ms, 50ms,
  50ms]` lines in the suite logs are
  `an_unpaced_case_whose_every_paste_paused_fails_saying_so`'s own caught
  panic, and that test passed.
- **Rejection under this load:** 24–33% of sessions per kind (`counts.txt`).
  Round 3 measured 3.6% and 7.0%.
  - Accepted paced sessions' longest pauses had a median of 35.4 ms alone
    and 30.8 ms in a suite. In round 3 the median was 25.8 ms.
  - Every rejection here is a pause of 50 ms or more, the threshold a
    rejection needs. A run is red only when all three of its sessions are
    rejected, and that happened in round 8 alone.
- **The same-load control** (`flake-control/`). `ef595df2`'s tui test binary
  (this branch before the join) and `ba133c25`'s ran in the same rounds: 10
  rounds × 1 copy of the same five entries each, 10 processes at once, the
  load at 52.5 and then 24.6.
  - All 50 runs of each build are green.
  - Rejected sessions: `ef595df2` 12 of 72 (16.7%), `ba133c25` 9 of 69
    (13.0%) (`counts-pre.txt`, `counts-head.txt`).
  - **No increase was observed; the rates are not shown equal**
    (`join-review/control-test.txt`). The difference, 12 of 72 against 9 of
    69, has Fisher's exact two-sided p 0.64: this run cannot tell the two
    builds apart either way. With these totals, an increase would have
    shown at p < 0.05 only if the joined build had rejected 22 of 69
    (31.9%) or more. So the run rules out roughly a doubling, and nothing
    smaller. `81ef5a0a`'s message ("the join does not slow the reader")
    says more than that; this paragraph is the claim.
  - The heavier run rejected more: `flake/`'s, at load 41.8 to 74.8,
    rejected 44 of 157 (28.0%). That is two runs, an association with the
    load and not a measured dependence.
  - The quiet-machine control is the quarantine mutants' baseline: its paced
    sessions' longest pauses were 18–21 ms, and none was rejected
    (`quarantine-3e17645a.txt`). Round 3's were 16–18 ms
    (`round3/s1-four-boundaries-12ms/quarantine.txt`).

**Mutants** (`mutants-*.txt`, `lib-threshold-3e17645a.txt`,
`quarantine-3e17645a.txt`). They ran on `--no-local` clones under the lock.
Each cargo run, its build included, is now bounded at 1200 s, and its
process group is killed past that (`mutants.py`, `quarantine_mutants.py`,
`round3/lib_threshold_mutants.py`). No run reached the bound.
- `mutants.py`: round 3's 15 and the join's two, **17 of 17 killed**, both
  baselines green (unit 7, harness 5). That held at `3e17645a` and again at
  `ba133c25`, whose one changed line is in the harness test that kills
  `measure-not-carried` and `measure-never-grows`.
  - `loop-never-settles-input`: the pending pair is never resolved, and the
    draft stays.
  - `loop-settles-input-after-the-drain`: the pair's Escapes interrupt the
    turn that just started.
  - The join test kills both.
- `lib_threshold_mutants.py` at `3e17645a`: the three quarantine mutants are
  each killed by both of the lib's clock-free threshold cases
  (`src/tui/input.rs:5334`, `:5933`), with the baseline green.
- `quarantine_mutants.py` at `3e17645a`, 3 runs per case. The baseline is
  9/9 green. The paced case fails 3/3 under any-quiet and 5 ms, on an
  accepted session, and passes under 20 ms (18 ms gaps), as round 3's 12 ms
  version did. The unpaced cases pass, but for 5 ms: 3 of its 6 runs were
  red. No session was rejected. The script exits 1 by its rule (a mutant run
  was green). The threshold is held by the lib, as §8 says.

### Two checks this Mac's gate misses (`linux-clippy.txt`, `rustup-tests.txt`, `touched-rustup.txt`)

Train 16's PR #102 went red on CI for three reasons no local gate caught:
- this Mac's PATH cargo is Homebrew's, not a rustup proxy;
- the Mac is not Linux.

Both checks below ran at `ba133c25`, under the lock, with rustup's 1.99.1
(`RUSTUP_TOOLCHAIN=1.99.1`; rustup's default untouched). This run installed
that toolchain and the Linux std. **Correction (join review):** the files
say "rustup's proxies first on PATH", but what went first was Homebrew's
`rustup/bin`. Each of those is a two-line script that runs rustup's
multiplexer with Homebrew's settings file. rustup's own multiplexer
(`rustup/libexec/bin`) ran the tui binary after the review
(`join-review/`).

- **Linux clippy** (`--target x86_64-unknown-linux-gnu`, plain and with
  `fault-injection`).
  - The C setup: ring's build script compiles C for the target, and this Mac
    has no `x86_64-linux-gnu-gcc`. It is given clang, with ring's
    no-libc-headers switch. Nothing links in a check.
  - As it is, both runs stop at one error, train 16's:
    `useless_conversion` at `src/config.rs:2639`.
  - That error stops the lib, so nothing behind it was linted. A probe
    clone allowed that one lint on that one function and ran again. Both
    runs pass, with 0 errors and 0 warnings. **Nothing of this unit's, and
    nothing else, is Linux-only.**
- **The whole suite with rustup's proxies first:** 6074 passed, 1 failed
  and 8 ignored.
  - The failure is train 16's: `a_config_alias_cannot_redirect_the_admitted_cargo_builtins`
    (`tests/permissions.rs`). Its scrubbed `cargo metadata` child, under an
    empty HOME, has rustup sync and download a stable toolchain into that
    HOME. The download fails.
  - `check-env-reads.py --self-test` passes here: 66 fixtures, 0 not as
    expected. TMPDIR here had no symlink in its path, and a symlinked
    TMPDIR is train 16's round-5 cause, so this pass does not test it.
  - This unit's 20 tests and META-PREFIX-DECODE's 15 are ok
    (`touched-rustup.txt`).

### Paths (`pathscan.sh`, `pathscan.txt`)

The brief's eight terms, train
16's pattern, were scanned over every commit this branch holds that
`8be4693b` does not: each message and what `git show` prints of it (for a
merge, its own resolution). That is 27 commits, through the receipt's last;
**0 matching lines**. This folder on disk has 0 files matching.
`check-no-local-paths.sh` also passed in the gate.

**At the receipt's last commit** (`head-files.txt`), the four gate steps that
scan the tree pass: identity, no stubs, no secrets and no local paths.
`git diff ba133c25 HEAD` outside `.prd/` is empty, so the gate's code head
is the head's code.

## Open questions

1. **The quarantine's clock is read after the readiness check.**
   - `burst` calls `shell.end_delivery()` once its check finds nothing
     (`src/tui/event_loop.rs:750-751`).
   - That reads `Instant::now()` (`src/tui/shell.rs:11303-11304`).
   - The decoder compares it with the quiet's start (`src/tui/input.rs:983`).
   - A UI thread preempted between a check that found nothing and that clock
     read stretches a short refill gap into a "50 ms quiet". So the quarantine
     can end on a pause shorter than `PASTE_QUIET`, and the rest of a hostile
     paste is then typed as keys.
   - Not seen here: every recipe B failure had a writer pause of 305 ms or
     more.
   - **What it can look like** (round 3, S2): the paced hostile paste records
     a quiet's start at each of its pauses, so a single reader stall there can
     end the quarantine on a session the harness accepted. That is a red with
     every pause under 50 ms, which reads as a quarantine regression and is
     not retried. The paced case's comment says so (`tests/tui.rs:3942-3948`).
     None was seen in round 3's 240 paced runs.
   - It is a candidate follow-up in PASTE-GAP's security area: compare against
     a clock read before the check, and stamp the quiet's start with one read
     after it.
   - Not changed in this unit: it is the queued PASTE-QUARANTINE-CLOCK unit
     (opus r1 §2, with a deterministic RED recipe through `burst`'s `ready`).
2. **xfx's keystroke frame shows no `Thinking`, and fx's does.** xfx paints
   `Thinking` only once the turn starts. That is the documented divergence in
   the OSC 7501 row, unchanged here: the fix keeps xfx's frame and makes the
   turn's first frame show `Thinking`.
3. **CI's test parallelism is unchanged.** None of the three causes needed it.
4. **Open parity note: no frame between `working` and `done` when the turn ran
   whole** (opus r1 NIT 2). The start's tick writes `working` after its frame
   (`commit_tick`, `src/tui/event_loop.rs:1433-1436`); the next tick reads the
   answer and the end, and writes `done` ahead of its frame (`:1385-1398`),
   because the result is recorded at `TurnEnded`. fx's wire
   has `Generating` frames between the two (`fx-p5-lifecycle-osc-40x120-w02-done.wire`
   on the research branch: `working`, the `(0s)` and `Generating` frames, then
   `done`). It shows only in a starved schedule, and moves no report: both are
   written, in fx's order. Not fixed here; its own unit if fx's frame between
   them is to be matched.
5. **Open parity note: a prompt sent while a stop is pending, read whole in one
   starved drain, reports no `working`** (opus r2 NIT 4; from before this
   unit). A prompt typed while a stop is still requested is queued at once
   (`queued_at_once`), so its start owes no frame of its own
   (`Shell::take_status_moved_by_input`, `src/tui/shell.rs:11667`) and the
   drain does not stop behind it. When the stopped turn's end and the prompt's
   start, answer and end all land in one drain, the session reports
   `[.., idle, done]` with no `working` [read from the code, as opus read it;
   not run]. No test asserts it, and fx's behaviour in that schedule is
   unverified. Recorded, not fixed.
