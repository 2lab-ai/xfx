# CI-PTY-WAIT — receipt

Unit: CI-PTY-WAIT. Base `dcfa5146` (main). Branch `fix/p5-ci-pty-wait`: one
product commit, `f79f5e5b`; test commits `f311ad79`, `a70dcd89`, `a77fad76`,
`7b524d78`, `765f9170`; and receipt commits. Joined with train 16 (§9): merge
`931418d4`, join test `9a54c911`, re-points `3e17645a`, clippy fix `ba133c25`.
Join review case `25203a2c` (§10). Train 16's final head merged as `d0115e72`
(§11). PR #103's first red (§12): `8ab62f69`, `a981b4ec`, `a6c6de29`,
`f516d090`, re-points `e717f3f6`; its second (§13): `e4b591d2`, re-points
`f2b01cc1`, a comment `a6045a54` (the code head).
Every figure below comes from a file in this folder; paths are scrubbed (`<runner>`,
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
- `Session::write_raw_measured` (`tests/support/pty.rs:455`, `f311ad79`) bounds
  the longest the terminal can have gone without a byte of the paste: from one
  write attempt's start to the next attempt's return, carried across the
  4 KiB parts.
- `paste_unpaced` (`tests/tui.rs:3693`, `7b524d78`) stops writing at the first
  pause of `PASTE_QUIET` or more. That session is **timing-indeterminate**:
  the measure is an upper bound on how long the input queue can have been
  empty, not a reading of it, so the session neither proves nor disproves the
  quarantine. Since round 2 it keeps its evidence -- the pause, and the
  screen a second later -- printed and handed back.
- `unpaced_case` (`tests/tui.rs:3901`) then runs a new session, at most 3 in
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
(`tests/tui.rs:15051`) reads the sandbox's whole `home` and `workspace` paths,
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
| S2: label the clock defect | The paced case's comment says a red with every pause under 50 ms is not retried and can be the clock-after-poll defect (`src/tui/event_loop.rs:750-751` → `src/tui/shell.rs:11303-11304` → `src/tui/input.rs:983`), which PASTE-QUARANTINE-CLOCK fixes; so does open question 1 here. | `tests/tui.rs:4329-4335` | docs |
| S3 and gpt6 r2 SHOULD: narrow the claims | The paced comment says acceptance is measured on the writer; the threshold is pinned by `input.rs:5334`/`:5933`, not by this case; and the 20 ms mutant's kill rests on the writer oversleeping. `unpaced_case` now says a regression gives at worst a misleading red, never a green. The round-2 table's row is narrowed the same way. | `tests/tui.rs:4188-4195`, `:3891-3900`; §7 | docs |
| NIT 1: predicate doc | "unless this tick's keys moved the status, when the tick writes its report after its frame whatever the start shows" | `src/tui/shell.rs:11710` | docs |
| NIT 2: clock-expiring cues | The equivalence case reads the hint row at the keys' own tick (`cue`) and leaves it out of the end screen. Its cue is asserted per key: "press ctrl+c again to exit" after a stop, the plain hint otherwise. | `src/tui/event_loop.rs:16390`, `:16480`, `:16540` | RED still by `drain-never-stops` (`round3/mutants-f8bc123a.txt`) |
| NIT 3: tail cut | `last_bytes` backs a 400-byte cut off to a character's start. | `tests/tui.rs:3812`; case `:3821` | RED by `last-bytes-cut-anywhere` |
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
    `smoke-tui.sh` and `tests/tui.rs:10220`.
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
    `useless_conversion` at `src/config.rs:2639@ba133c25` (fixed in train
    16's round 5, now `src/config.rs:2642`; §11).
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

## 10. The join review, round 1 — `join-review/`

The join review (gpt6, two personas) found no MUST. This round does two of
its items. SHOULD 1 (a pending pair crossing its deadline during an empty
read) predates this branch; it is queued to ESC-MONITOR-TIMING and is not
changed here.

| commit | what |
|---|---|
| `25203a2c` | SHOULD 2: the pending pair through the loop's own turns — tests only, **the code head** |
| `4122bfcf` | the control's wording, the order RED, the rustup wording, the mutant suite widened |
| the next | this section and its files |

### SHOULD 2: the pending pair through the loop's turns (`order_red.py`, `order-red.txt`)

The review's point: the pending branch of `9a54c911`'s join test calls the
turn's steps by hand, so no ordering regression in the production pass can
reach it.

- **The new case**, `an_escape_pair_pending_through_one_loop_turn_settles_in_the_next_ahead_of_the_start_it_reads`,
  drives the pair through `loop_turn` twice.
  - The turn that reads `ab ESC ESC` settles it at a clock no deadline has
    passed, so the pair is no key and the draft stays.
  - The next turn reads nothing; its clock has passed both deadlines. The
    held prompt's start, its answer and its end are on the channel.
  - It asserts what the hand-driven branch asserts: the drain stops behind
    the start (2 events left), and `Thinking` comes before `working`. It also
    asserts that the pair arrived as two Escapes ahead of the start (the draft
    cleared, no stop sent), and the reports `idle`, `working`, `done`.
- **The clock is the case's.** The pair is read stamped 250 ms ahead of the
  clock, through `Shell::route_read_at`. That is `route_read` with the
  read's instant given; it is a helper appended to the end of `shell.rs`'
  tests. So the reading turn's own settle, at the clock, finds no deadline
  passed for any stall under 250 ms. The next turn's read then sleeps until
  both deadlines past the stamp have passed.
- **Kept:** the hand-driven branch, as its own case
  (`an_escape_pair_still_pending_in_the_tick_that_reads_a_held_prompts_start_is_no_key_and_the_start_ends_the_drain`).
  The settled case keeps its name and its line. The two closures they
  shared are helpers at the end of the tests module, so no cited line moves.
- **RED** (`order-red.txt`, on a `--no-local` clone at `25203a2c`). Each
  order of the production pass was broken alone, and the three cases ran:

| order broken in `loop_turn` | loop-driven pending (new) | hand-driven pending (kept) | settled |
|---|---|---|---|
| none (baseline) | ok | ok | ok |
| the band settled before the input (the reviewer's example) | **FAILED**: no `working` after a frame showing `Thinking` | ok | FAILED, the same |
| the input settled after the drain | **FAILED**: the pair was not two Escapes ahead of the start | ok | FAILED, the same |
| the drain before the read | **FAILED**: the pair was not two Escapes ahead of the start | ok | FAILED, the same |

  - With the band moved ahead of the input, it is also ahead of the drain.
    The start's frame is then built from a band no settle has seen since the
    start, so it shows no `Thinking`. That is what the case reads; the pair
    has no part in it.
  - The other two hand the pair's Escapes to a turn that has already
    started: an Escape pair at a running turn interrupts it, and the draft
    stays.
  - The settled case fails under the same orders. What the new case adds is
    the pending pair: it settles in a turn that reads nothing, and only the
    turn's own settle resolves it.
- **Mutants** (`mutants-25203a2c.txt`): `mutants.py` now selects every
  `an_escape_pair_` case and moves the two new orders (`loop-settles-band-first`,
  `loop-drains-before-the-read`). **19 of 19 killed**, with both baselines green (unit 9, harness 5).
  The new case kills all four `loop-*` orders, alongside the settled one.

### The same-load control's wording (`control_test.py`, `control-test.txt`)

The control saw no increase in rejected sessions at the joined build; it did
not show the rates equal. §9 now says so: Fisher's exact p 0.64, and only a
joined-build rate of 31.9% or more would have shown at p < 0.05.

### The gate (`gate.txt`)

At `25203a2c`, on a `--no-local` clone, in one hold of the lock
(`review-gate.sh`):
- fmt is clean, and clippy is clean for both feature sets.
- `cargo test --lib`: 5129 passed, 0 failed, 7 ignored. With
  fault-injection: 5152, 0 and 7. The touched cases are lib tests: this
  unit's three join cases, its other loop and shell cases, and
  META-PREFIX-DECODE's 15, all ok in both (`touched.txt`).
- **The whole tui test binary under rustup's own multiplexer.** PATH had
  rustup's `libexec/bin` first, where `cargo` links to `rustup`, not
  Homebrew's wrapper scripts, with `RUSTUP_TOOLCHAIN=1.99.1`. Result: 198
  passed, 0 failed. This unit's PTY cases are among them (`touched.txt`).
  The permissions test and the env self-test are in other binaries; their
  runner-only failures are train 16's round 5, not yet merged here.

### Numstat by class (`numstat.py`, `numstat.txt`)

| range | product | tests | receipt |
|---|---|---|---|
| `81ef5a0a..25203a2c` (SHOULD 2) | +0 −0 | +160 −17 | +0 −0 |
| `25203a2c..4122bfcf` | +0 −0 | +0 −0 | +194 −7 |
| `4122bfcf..` this section's commit | +0 −0 | +0 −0 | +295 −0 |

A `src/` line counts as tests when it sits inside its file's
`#[cfg(test)] mod tests`. `Shell::route_read_at` is one of those: an
`impl Shell` block in `shell.rs`' tests module.

### Paths

`join/pathscan.sh` over every commit this branch holds that `8be4693b` does
not, messages and lines, through this section's commit: 0 matching lines,
and 0 files in this folder (`pathscan.txt`). The content check moved
nothing: at `4122bfcf` the code and docs read the same 9 WRONG as at
`ba133c25`, and this README the same 18 references
(`refs-4122bfcf.txt`).

## 11. Train 16's final head merged — `join2/`

Train 16 settled at `1a4cb6b4` (code `23819f7a`), pushed to PR #102. Its
rounds 5 and 6, on top of `8be4693b`, fix PR #102's three runner-only CI
causes. They touch `src/config.rs`, `scripts/check-env-reads.py`,
`tests/support/isolated.rs` and its own receipt.

| commit | what |
|---|---|
| `d0115e72` | `git merge --no-ff 1a4cb6b4` — **the code head** |
| the next | this section, `join2/`, and §9's one pinned reference |

### The merge (`merge.txt`)

- **No conflict.** `git merge-tree` reported none beforehand, and its tree
  is the merge commit's (`45465042`). The merge itself wrote nothing:
  `git show --cc` prints no line beyond its header. There was no hand
  resolution to check.
- **Train 16's head plus this unit's changes:**
  - `git diff 1a4cb6b4 d0115e72` is byte-identical to `git diff 8be4693b
    5f9f38e2`, this branch on train 16 before the merge. It covers 137
    files: 129 are this receipt; the other 8 are this unit's code, tests and
    re-points.
  - What the merge brought, `git diff 5f9f38e2 d0115e72`, is
    byte-identical to train 16's own rounds 5 and 6, `git diff 8be4693b
    1a4cb6b4`.

### Numstat by class, against `1a4cb6b4` (`numstat.txt`)

| class | lines |
|---|---|
| product | +98 −33 (`src/tui/event_loop.rs`, `src/tui/shell.rs`, outside their tests) |
| tests | +1442 −76 (both files' tests modules, `tests/tui.rs`, `tests/support/pty.rs`) |
| receipt | +10526 −0 (this folder) |
| other | +14 −14 (the re-points: `docs/parity.md` 1, `scripts/` 13) |

The product lines are this unit's from before the join (§2a: the drain's
stop, `loop_turn`, `keyed()`). Neither join added a product line.

### References (`refs.txt`, `content-*.txt`, `readme-refs-*.txt`)

- **Code and docs:** WRONG 9 at the merge, the same set as at `5f9f38e2`.
  That is train 16's 8, and `docs/parity.md`'s pinned transport reference.
  **The merge moved none.** Train 16's three files cite nothing this unit
  cites, and this unit's files cite nothing in them.
- **This README:** 19 at the merge against 18 before. The new one is §9's
  `src/config.rs:2639@ba133c25`, the Linux clippy hit. Train 16's round 5 rewrote
  that line rather than moving it. It is now pinned to `ba133c25`, where it
  stands, with the fix's line (`:2642`) beside it. That is a pin, not a
  number change.

### The gate, as CI runs it (`gates-rustup.txt`, `gates-brew.txt`, `gate-counts-*.txt`)

Train 16's `gates.sh` mirrors every step of the joined `ci.yml`'s gate
job. It ran at `d0115e72` twice, side by side. Each run had its own fresh
`--no-local` clone and its own hold of the suite lock.

| run | how | steps | `cargo test` / with fault injection |
|---|---|---|---|
| rustup | rustup's own multiplexer (`libexec/bin`) first on PATH, no fallback default, `RUSTUP_TOOLCHAIN=1.99.1`, `TMPDIR` through the `/tmp` link | **29, 0 failed** (load 17.9 to 56.1) | **6077 / 6114 passed**, 0 failed, 8 ignored each |
| Homebrew | this Mac's PATH (Homebrew's 1.99.1), `TMPDIR` resolved | **29, 0 failed** (load 11.9 to 44.2) | **6077 / 6114 passed**, 0 failed, 8 ignored each |

- No step was run again in either.
- The counts are the same in both runs, binary by binary
  (`gate-counts-*.txt`). They are 2 more than at `ba133c25` (6075 and
  6112): those are the join review's two new cases.
- `check-env-reads` is OK in both. Its self-test has 68 fixtures, 0 not as
  expected, in both, including the rustup run's `TMPDIR` through the link.
- Train 16's `a_config_alias_cannot_redirect_the_admitted_cargo_builtins`,
  which failed in §9's run under rustup, passes in both.
- This unit's 22 tests and META-PREFIX-DECODE's 15 are ok in all four test
  runs (`touched.txt`).

### Linux clippy (`linux-clippy.txt`)

Train 16's `linux-clippy.sh` ran at `d0115e72` on a fresh clone. Its
proxy dir was rustup's own multiplexer, and it ran under the lock. It
covers both Linux targets (x86_64 and aarch64 `unknown-linux-gnu`), each
plain and with fault-injection: **0 of 4 runs failed, with no warning or
error line.** The `src/config.rs` hit of §9 is gone with train 16's fix, so
the whole crate is linted now, this unit's code included, with no probe.

### Paths

`join/pathscan.sh` scanned every commit this branch holds that `8be4693b`
does not: the messages, and what `git show` prints of each (for the merge,
its own resolution, which is empty). It ran through this section's commit:
0 matching lines, and 0 files in this folder (`pathscan.txt`). The tree
checks passed in both gates.

## 12. PR #103's first CI run: the paced hostile paste, every session rejected — `ci-pr103/`

**The fix is tests only:** `tests/tui.rs` and `tests/support/pty.rs`. No
product line changed.

### The evidence (`gate-aarch64-apple-darwin-failed.log`, `ci-part.txt`)

The coordinator pushed `461b4736` as PR #103.
- The aarch64-apple-darwin row's "Tests, with the fault injector" step went
  red, on `a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing`.
  Its three sessions were rejected at 61.3, 58.6 and 96.8 ms (12 ms pauses),
  so `unpaced_case` failed saying no session tested the paste.
- The other rows passed, and so did that step's status-frame test and both
  unpaced hostile pastes. The unpaced cases' longest pauses were 43.1 and
  28.0 ms: near the 50 ms bar.
- The log does not say where any pause fell. One can be placed from its
  screen. The third session typed `K00005121` through `K00005191`: tokens
  512 to 519, which are part 64 whole (`ci_part.py`). So that pause fell
  before part 64's first byte. Part 64 is one of the case's four planned
  pauses. The other two sessions' screens typed nothing, which places
  nothing.
- **The timing, from the log's own timestamps.** libtest starts tests in name
  order as its three threads free up. The three hostile pastes are
  neighbours (`a_mebibyte_of_hostile_clipboard_with_pauses...`, then the two
  `..._written_unpaced_...`).
  - The paced case began about 19:57:17.
  - The next two began as `a_level_chosen...` (19:57:18.22) and
    `a_fork_of_a_fork...` (19:57:19.25) finished.
  - The paced case's sessions ran until 19:57:25.13.
  - So for most of the paced case, all three hostile cases ran at once on
    the row's three vCPUs. [inferred from the start order and the finish
    times; the log has no start lines]

### What the harness says now (`8ab62f69`, `a981b4ec`, `a6c6de29`)

CI's message gave three numbers and no place. A rejected session, and an
accepted one's longest pause, now say:
- **where the pause fell:**
  - between parts N−1 and N, across a planned pause (with how long that
    sleep took), or with none planned there;
  - or inside part N, at write attempt k, behind a full queue's 2 ms poll
    (with how long that poll slept);
- **how long the `write` that ended it took;**
- **the rest:** the write before it, and the time the thread was off a CPU.

`Session::write_raw_traced` records it, and `where_it_fell` words it.
`a_pause_is_said_to_fall_between_parts_or_inside_one_with_its_sleep` pins
both.

### The cause (`runs/phases/`, `runs/recipe3-16-suites.txt`)

> **Corrected by §13 (PR #103's second CI run).** What follows is true of
> this machine's recipes and was not CI's cause. This section said as fact
> that the pauses are not the planned sleep oversleeping, and inferred that
> on CI, too, three pastes at once slowed the writes. CI's second run, with
> these words, said otherwise. Every rejected session on both macOS rows,
> 10 of 10, fell across a planned 12 ms pause whose sleep took 50–110 ms.
> The writes on either side took under 0.25 ms. CI's macOS runners stretch
> the harness's own timers, which no recipe here did. §13 has the evidence
> and the fix. The claims this changes are marked below; the rest stands as
> a record of what was measured here.

**Measured locally, with the new words.** On this machine's recipes, the
pauses that reject a session were neither the planned sleep oversleeping
nor the 2 ms poll stretching (**on CI they were the sleep, §13**):
- **Rejected inside a part, behind a poll:** 255 of the 280 before-fix
  rejections in the phase run. The poll slept 0–8 ms (median 3). The write
  that ended the pause took 10–50 ms (median 26), and the rest 0–44 ms
  (median 25).
- **Rejected across a planned 12 ms pause:** 13 of them. The sleep took
  15–21 ms (median 18). macOS sleeps a 12 ms sleep as about 18 ms even at
  idle. The ending write took 16–32 ms, and the rest 0–22 ms.
- **Rejected between parts with no pause planned:** 12 of them.
- **At idle**, a 4 KiB write takes 0.05–4 ms.

The measure spans two `write` calls and what lies between them (§2b): from
one attempt's start to the next one's return. Two writes of about 25 ms
each reach 50 ms with no help from any sleep. The planned pause adds about
18 ms, which is why the paced case is rejected more at lighter loads
(8.6% against 3.0% in recipe 1).

**Here, what slows the writes is mebibyte pastes beside one another, not
load as such** (on CI, the writes did not slow: §13).
- One suite beside eleven background suites that skip the hostile pastes:
  pauses of 3–18 ms (`runs/recipe4-one-suite.txt`).
- One suite beside 36 CPU-bound processes: pauses of 9–19 ms
  (`runs/recipe2-hogs.txt`; the scheduler runs woken threads first).
- 24 hostile pastes at once (8 suites): 3–9% of sessions rejected
  (`runs/recipe1-8-suites.txt`).
- 36 at once (12 suites): 75% (`runs/phases/`).

**On CI** (inferred, not measured there, and **wrong**: §13): libtest starts
a suite's three hostile cases together, so on the row's three vCPUs a suite
runs three mebibyte pastes at once. That is the condition that slows the
writes here. CI's second run measured the writes there at under 0.25 ms,
and the planned sleep at 50–110 ms.
- The one pause the log lets me place fell at the planned pause before part
  64.
- Where the other two fell, and how CI's 61–97 ms split between sleep, write
  and the rest, the old binary did not say. The new words will say it next
  time.

### The RED (`runs/phases/`, the "before" rows)

Twelve whole suites of the fault-injection test binary ran at once, three
test threads each, for 4 rounds under `SUITE_LOCK_EXCLUSIVE`. That puts 36
hostile pastes on this machine at once, the same per-suite shape as CI's
three on three vCPUs. **The paced case was red in 34 of 48 suites: in 27,
every session was rejected (CI's failure); in 7, an accepted session timed
out waiting for its screen.** Of its sessions, 88 of 117 were rejected
(75.2%).

### The fix (`f516d090`, tests only)

1. **One hostile paste at a time in the binary.** `unpaced_case` holds
   `ONE_PASTE_AT_A_TIME` for the whole case, all its sessions, so the three
   cases run one after another. The rest of the suite runs beside them as
   before. This takes away the condition that slows the writes here, and it
   is the part of the fix measured here. On CI it was not the cause: the
   paced case's rejected pauses were 50–110 ms with it and 59–97 ms without
   it (§13). It stays, for the reason measured here; CI's numbers do not
   decide it.
2. **Five sessions before giving up, not three** (`UNPACED_ATTEMPTS`). A
   rejected session still asserts nothing and returns before any assertion,
   and a failed assertion on an accepted session still fails the case at
   once. So another attempt cannot turn a failing case green; it is
   insurance for a runner busier than this run. Its effect is not measured
   separately: with the lock no session was rejected, so no fourth or fifth
   session ran.
3. **What did not change:** `PASTE_QUIET` (50 ms); the rejection bar; what
   an accepted session asserts; the pacing (four 12 ms pauses); and the
   other tests' parallelism.

Rejected candidates:
- **Serializing the hostile pastes against every PTY test.** The other
  tests write no mebibyte, and recipe 4 shows them not slowing the writer.
- **A different pacing.** The sleeps are not where the pauses go. (**Wrong
  on CI**, §13: there the sleep was the whole pause. §13 changes how the
  pause is waited, not the pacing.)
- **A time budget.** A count gives the same bound with one number less.

`no_two_unpaced_cases_run_at_once` pins the lock: three cases started
together never have two sessions at once.

### Before and after, under the same recipe (`runs/phases/run.txt`, `sessions.txt`)

**Recipe:** twelve suites of one build at once, one build at a time, the two
builds interleaved round by round. The load at the phases' starts was
12.0–45.6 before and 12.2–24.0 after. The after phases are lighter by
construction: twelve hostile pastes at once instead of 36.

| | before (`a6c6de29`: 3 attempts, no lock) | after (`f516d090`) |
|---|---|---|
| paced case | **red 34 / 48** (27 every session rejected, 7 timeouts) | **green 48 / 48** |
| unpaced, types nothing | red 35 / 48 (26 every session rejected, 9 timeouts) | green 48 / 48 |
| unpaced, no approval | red 16 / 48 (every session rejected) | green 48 / 48 |
| status frames | green 48 / 48 | green 48 / 48 |
| paced sessions rejected | 88 / 117 (75.2%) | 0 / 48 |
| unpaced sessions rejected | 192 / 257 (74.7%) | 0 / 96 |
| accepted sessions' longest pause, paced | median 47.6 ms, max 49.9 | median 23.5 ms, max 30.7 (every one at a planned pause) |
| accepted sessions' longest pause, unpaced | median 47.5 ms, max 49.8 | median 13.6 ms, max 33.0 |
| suite seconds | mean 164, max 192 | mean 159, max 167 |

- Zero in 48 suites bounds the after rate at about 6% (rule of three, 95%).
  Zero in 144 sessions bounds the per-session rate at about 2%.
- The suite is no slower with the lock. The three cases wait for one
  another, but the suite's other tests go on meanwhile.

### The unpaced cases and the status test (question 4)

- **The unpaced cases share the risk.** Before the fix they were red more
  often than the paced case (types-nothing 35 / 48), with the same
  mechanism: a 0 ns pause has only the two writes to cross 50 ms with. On
  CI they passed at 43 and 28 ms. The lock is in `unpaced_case`, so it
  covers all three.
- **The status-frame test does not share it.** It was green in all 96 phase
  runs and in every recipe here. It writes no mebibyte, and its CI failure
  was a product defect (§2a).
- **"CI's test parallelism is unchanged; none of the causes needed it"
  (open question 3) no longer holds for the hostile pastes.** Here they
  needed not to run beside one another. (On CI that was not the cause;
  §13.) The suite's thread count and every other test's parallelism are
  unchanged.

### The recipes that did not reproduce, and two runs of mine to discount (`runs/`)

- **Recipe 1** (8 suites at once, the first diagnostics binary): 3.0% of
  unpaced and 8.6% of paced sessions rejected, and no case red. That counts
  rounds 1, 3, 4 and 5.
  - **Round 2 is not counted.** It ran beside an unlocked build and run of
    mine: the tui test binary, compiled to check one message. Its ten
    timeouts are that build's doing.
  - **Round 6 is void.** I deleted the clone it ran from, and with it the
    `xfx` its tests spawn, while it ran.
- **Recipe 2** (one suite beside 36 busy processes) and **recipe 4** (one
  suite beside 11 suites that skip the hostile pastes) were each stopped
  after one suite. Neither slowed the writer, so neither emulates CI.
- **Recipe 3** (two builds' 16 suites at once, the fix's first form: the
  lock alone) was stopped after two rounds. Each build's pastes ran beside
  the other's, which the lock inside one suite cannot touch. Both builds
  were red often; the RED is the same in recipe 5's cleaner form.

### Mutants (`mutants-f516d090.txt`, `lib-threshold-f516d090.txt`, `quarantine-f516d090.txt`)

They ran at `f516d090`, on a `--no-local` clone under the lock. The code
head `e717f3f6` differs from it in one comment line's numbers.
- **`mutants.py`: 20 of 20 killed**, with both baselines green (unit 9,
  harness 7). The 20 are round 3's 15, the join's four loop orders, and
  `pastes-at-once`, which takes the lock out and is killed by
  `no_two_unpaced_cases_run_at_once`. `measure-never-grows` is re-aimed at
  the traced write and is killed by both measure tests.
- **`lib_threshold_mutants.py`:** each of the three quarantine mutants is
  killed by both of the lib's clock-free threshold cases.
- **`quarantine_mutants.py`, 3 runs per case:** the baseline is 9 of 9
  green, and no session was rejected.
  - The paced case fails 3 of 3 under any-quiet and under 5 ms, each time on
    an accepted session.
  - It passes under 20 ms, as in round 3 and §9: its gaps are about 18 ms.
  - The unpaced cases pass under every mutant, as before.
  - **The quarantine mutants still fail where they failed.** The threshold is
    held by the lib.

### The gate (`gates-rustup.txt`, `gates-brew.txt`, `gate-counts-*.txt`, `linux-clippy.txt`)

Train 16's `gates.sh` ran at the code head `e717f3f6` twice, side by side.
Each run had a fresh `--no-local` clone and its own hold of the lock.

| run | how | steps | `cargo test` / with fault injection |
|---|---|---|---|
| rustup | rustup's own multiplexer (`libexec/bin`) first on PATH, `RUSTUP_TOOLCHAIN=1.99.1`, `TMPDIR` through the `/tmp` link | **29, 0 failed** (load 5.0 to 36.6) | **6079 / 6116 passed**, 0 failed, 8 ignored each |
| Homebrew | Homebrew's 1.99.1, `TMPDIR` resolved | **29, 0 failed** (load 5.1 to 27.0) | **6079 / 6116 passed**, 0 failed, 8 ignored each |

- The counts are §11's plus this round's two harness cases.
- The env check's self-test is 68 of 68 in both.
- This unit's 24 tests and META-PREFIX-DECODE's 15 are ok in all four test
  runs (`touched.txt`).
- In those runs no hostile-paste session was rejected. The longest pauses
  were 3.1–4.8 ms unpaced and 17.3–18.1 ms paced; the paced ones were each
  the planned pause itself.
- **Linux clippy** (train 16's `linux-clippy.sh`, both Linux targets, plain
  and with fault-injection): **0 of 4 runs failed**, no warning or error
  line (`linux-clippy.txt`).
- The first try of these gates, at `f516d090`, was stopped when the content
  check found the comma list moved; its numbers are not used.

### References (`content-e717f3f6.txt`, `readme-refs-e717f3f6.txt`, `repoint-*.txt`)

- **This round's test lines moved 15 references** (the content check at
  `f516d090` against `d0115e72`). `e717f3f6` re-points them, numbers only
  (8 lines, `repoint-numbers.txt`):
  - six numbers of `tests/tui.rs:2721`'s comma list, each +190 and still
    the clear-line send (mapped through the diff);
  - nine of this README's references into `tests/tui.rs`.
- **At `e717f3f6`:**
  - Code and docs: WRONG 9, the same set as at `d0115e72`. 0 moved.
  - This README: 19. That is the same 18 as before, and §9's reference to
    `tests/tui.rs:2721`, the comma-list line itself. That line is the same
    line, its numbers re-pointed; the check reads the changed text as
    WRONG, and it was read by hand.

### Numstat by class (`numstat.txt`)

| range | product | tests | receipt | other |
|---|---|---|---|---|
| this round, `461b4736..e717f3f6` | **+0 −0** | +243 −14 (`tests/tui.rs`, `tests/support/pty.rs`) | +14 −9 (the re-points) | +0 −0 |
| the branch against train 16, `1a4cb6b4..e717f3f6` | +98 −33 (§2a, from before the joins) | +1671 −76 | in `numstat.txt` | +14 −14 (re-points in `docs/` and `scripts/`) |

This section's own commit is receipt only. `numstat.txt` has every range,
the commit that adds it included.

### Paths

`join/pathscan.sh` scanned every commit this branch holds that `8be4693b`
does not, through this section's commit: the messages, and what `git show`
prints of each. **0 matching lines, and 0 files in this folder**
(`pathscan.txt`). `check-no-local-paths.sh` passed in both gates.

## 13. PR #103's second CI run: the harness's own timers, fired late on CI's macOS rows — `ci-pr103-r2/`, `ci-probe/`

**The fix is tests only:** `tests/support/pty.rs` and `tests/tui.rs`. No
product line changed. §12's claims that this run refuted are corrected in
place, each marked.

### The evidence (`ci-pr103-r2/`)

The coordinator pushed `8e2f04c2` (§12's fix and its words) to PR #103.
- **Both macOS rows went red on the paced case**, and only there:
  aarch64-apple-darwin's "Tests, with the fault injector"
  (`gate-aarch64-apple-darwin-fi-failed.log`) and x86_64-apple-darwin's
  checks "Tests" (`gate-x86_64-apple-darwin-checks-tui.log`, the log's
  `tests/tui.rs` part; `ci-probe/excerpt.py` cut it, and its first line
  names the whole log's sha256).
- **Every rejected session, 10 of 10, fell "between parts N−1 and N,
  across a planned 12ms pause that slept" 50.2–109.5 ms.** Seven fell at
  parts 0→1, two at 127→128 and one at 191→192. The write that ended each
  took 0.10–0.24 ms, and the rest (the write before it and any time off a
  CPU) 0.03–0.22 ms. So the sleep was the whole pause.
- **The unpaced cases passed on both rows.** Each one's longest pause was a
  full queue's 2 ms poll, which slept 19.9–27.1 ms; the writes around it
  took at most 0.39 ms. That is half the bar from the poll alone.
- §12's mechanism, writes slowed to 20–30 ms by pastes beside one another,
  is not in this run: no write near a counted pause took a millisecond.

### Locally, by CI's signature (`ci-pr103-r2/signature.sh`, `signature-local.txt`)

`signature.sh` runs the paced case alone under a recipe and counts the
sessions "rejected across a planned pause that slept ≥50 ms" whose ending
write took under a millisecond. At `f516d090`'s fault-injection binary
(the code of `8e2f04c2`; the two differ in one comment's numbers), 2 runs
per recipe:

| recipe | runs green | sessions with CI's signature | what the sessions said |
|---|---|---|---|
| none (idle) | 2 / 2 | 0 of 2 | planned sleeps 17.5–18.0 ms |
| `taskpolicy -l 3`, `-l 5`, `-t 5` | 2 / 2 each | 0 | sleeps 15.1–18.0 ms |
| `taskpolicy -b`, `-c background` | 0 / 2 each | 0 of 10 each | a different signature: inside part 0, behind a 2 ms poll that slept 126–520 ms |
| **`taskpolicy -c utility`** | **0 / 2** | **10 of 10** | **planned sleeps 68.5–107.0 ms, ending writes 0.07–0.14 ms** |

Only a utility clamp gives CI's signature. It lets macOS coalesce the
thread's timers and fire them late; it does not withhold the CPU.

### The probe (`probe/p5-cpw-timer` at `f0d609e1`; run 43984803095; `ci-probe/`)

A test on its own branch, off `8e2f04c2`, never to merge and not on this
branch: in the paced case's own setting (a live TUI reading the pty, the
hostile mebibyte in 4 KiB parts, the lock held) it takes one of five 12 ms
waits in turn before each part from the second on, and meets each full
queue with one of two waits in turn, timing every one. Then the same with
the writer thread asking for the user-interactive QoS. It fails on purpose
so libtest prints the numbers. The coordinator ran it on every row
(workflow_dispatch); `ci-probe/probe-<row>-tui.log` are the four logs'
`tests/tui.rs` parts, cut by `excerpt.py`, each naming its whole log's
sha256.

**The environment.** aarch64 macOS: 3 CPUs, load 9–21, nice −10, every
thread of the test process at priority 20T (one at 19T), the writer's QoS
read as `QOS_CLASS_DEFAULT`, not darwin-background. x86_64 macOS: 4 CPUs,
load 3–6, every thread 20T. 20T is the utility clamp's priority (31T is the
default, 4 the background); the requested class does not show a clamp.
Linux: nice 0.

**The waits** (macOS, 51 of each per pass; the writer at its default QoS /
asking for user-interactive):

| wait | aarch64 macOS | x86_64 macOS |
|---|---|---|
| `thread::sleep` 12 ms | median 57 / 44 ms, max 109 / 108, ≥50 ms 31 / 20 of 51 | median 31 / 32 ms, max 106 / 106, ≥50 ms 12 / 18 |
| spin to a 12 ms deadline | max 12.95 / 12.07 ms | max 12.10 / 12.12 ms |
| `yield_now` to 12 ms | max 21.3 / 20.2 ms | max 13.4 / 27.2 ms |
| 1 ms sleeps to 12 ms | median 17.7 / 16.9 ms, max 22.3 / 24.0 | median 12.7 / 12.7 ms, max 17.8 / 17.8 |
| kqueue `NOTE_CRITICAL` 12 ms | median 12.2 / 12.2 ms, max 14.9 / 18.1 | median 12.2 / 12.2 ms, max 12.6 / 14.3 |
| full queue: `thread::sleep` 2 ms | median 16.4 / 16.8 ms, max 24.3 / 25.7 | median 3.0 / 3.0 ms, max 30.2 / 29.4 |
| full queue: `poll` for `POLLOUT` | max 160 / 178 ms, ≥50 ms 15 / 13 | max 578 / 548 ms, ≥50 ms 22 / 19 |
| `write` | max 5.9 / 2.1 ms | max 1.6 / 2.3 ms |

Linux (64 of each, no kqueue row, and no full queue: the reader kept up):
`thread::sleep` 12 ms took 12.04–12.14 ms, the spin 12.000–12.004 ms.

Before CI, the probe's first form ran here once each with no clamp, under
`taskpolicy -c utility` and under `taskpolicy -b`
(`ci-pr103-r2/probe-local-{idle,utility,bg}.txt`). Under the utility clamp
it matched CI: `thread::sleep` 12 ms took 15.8–108 ms (median 93–96), while
the spin stayed within 0.01 ms and the critical timer within 13.9 ms.

What it says:
- **The timer fires late; the CPU is not withheld.** A spin, which has no
  timer, stayed within 1 ms of its deadline on both macOS rows while the
  sleep beside it took up to 109 ms.
- **Asking for a higher QoS does not help.** The priorities stayed 20T, and
  the sleep's median 32–44 ms.
- **The full queue's 2 ms sleep is exposed too.** Up to 30 ms on CI, and in
  this run's unpaced cases it was each longest pause. Under the clamp here
  it reached 82 ms (`probe-local-2ms.txt`, below).
- **`POLLOUT` is no way out:** it came back up to 578 ms late.
- Linux does not stretch its sleeps.

### The fix (`e4b591d2`, tests only)

- **`support::pty::sleep_on_time(wait)`.** On macOS: a one-shot kqueue
  `EVFILT_TIMER` with `NOTE_USECONDS | NOTE_CRITICAL` (the man page: "override
  default power-saving techniques to more strictly respect the leeway
  value"). A signal ends the `kevent` early (`EINTR`), and this binary
  handles `SIGCHLD` (`stops`); the timer is set again for what is left of
  the deadline, so the wait is never short. An `EV_ERROR` event fails. The
  kqueue is an `OwnedFd`, closed on every way out, and not inherited by a
  fork. Elsewhere: `thread::sleep`, unchanged.
- **It takes the two waits the measure counts:** the writer's full-queue
  poll (`Session::write_raw_traced`) and the paced paste's planned pause
  (`paste_with_pauses`). The reader thread's poll, `stop_with`'s and every
  other wait are unchanged.
- **Why this primitive, from the CI numbers:**
  - `thread::sleep`: the defect.
  - 1 ms sleeps: each is stretched too; 22–24 ms for 12 on aarch64.
  - `yield_now`: up to 27 ms, and a busy loop where nothing else is runnable.
  - A spin: exact (≤12.95 ms). Fine for four planned pauses, but the
    full-queue poll runs for most of a paste (20–33 s on the probe's macOS
    rows), and a spinning writer takes a CPU from the TUI that drains the
    queue, on a 3-CPU runner at load 9–21. One primitive for both waits.
  - The critical kqueue timer: 12.0–18.1 ms for 12 ms on both macOS rows,
    p95 ≤14.1, and it blocks.
  - `POLLOUT`: up to 578 ms.
- **What CI has not measured:** the critical timer at 2 ms, and this
  harness. CI measured the critical timer at 12 ms only, and the fixed
  harness has not run there. The full-queue poll's GREEN below is local.
- **What did not change:** `PASTE_QUIET` (50 ms), the 50 ms bar, the pacing
  (four 12 ms pauses before parts 1, 64, 128 and 192), `UNPACED_ATTEMPTS`
  (5), what an accepted session asserts, and that a rejected session asserts
  nothing. The words a session says are the same, so the same scripts count
  both builds.
- **Pinned by** `a_wait_on_time_is_never_short_with_signals_landing_on_it`:
  40 waits of 2 ms and 40 of 12 ms with a signal sent to the waiting thread
  every millisecond (`SIGURG`, with a handler and no `SA_RESTART`); no wait
  may end short, and a signal must have landed.
- `unpaced_case`'s failure said "the harness was not scheduled while it
  wrote". It now says the harness was that late between two of its writes,
  and each rejected session says where.

### The lock

`ONE_PASTE_AT_A_TIME` stays, and its doc now says why. CI's numbers do not
decide it:
- The paced case's rejected pauses were 59–97 ms without it (the first run)
  and 50–110 ms with it (the second): it neither caused nor fixed CI's red.
- The unpaced cases' longest pauses were 43.1 and 28.0 ms without it, and
  20.1–27.3 ms with it. That is two sessions against four, and the first
  run's had no words saying where they fell: not evidence either way.
- The x86_64 row passed the paced case without the lock and failed with it.
  The timer alone predicts that often enough: the probe's x86_64 row slept
  12 ms past 50 ms in 12 and 18 of 51 tries, about 0.3 a pause, so a
  session of four pauses was rejected about 3 times in 4. The first run gave
  the case 3 sessions (all rejected about 0.42 of the time) and the second
  gave it 5 (about 0.24). A pass and then a fail comes out about 1 time in
  7 [computed from the probe's rates, which were taken with the lock held].
- Every CI number the fix's primitive rests on was taken with the lock held.
  Taking it out would put the fix on ground CI has not measured, and here it
  cannot be measured without CI. Keeping it also makes the RED below a
  change of one thing.
- Its reason is the one measured here (§12): 36 pastes at once slowed the
  writes to 20–30 ms. Its cost: the three cases wait for one another, and
  the suite was no slower with it (§12).

### RED and GREEN under the clamp (`ci-probe/red.sh`, `red-runs.txt`, `red-count.txt`, `red-sessions.txt`)

**Recipe:** the three hostile pastes in one run of the fault-injection test
binary (the lock runs them one after another), under `taskpolicy -c utility`
and with no clamp, 20 rounds each, the two builds interleaved round by round
(which goes first alternates). Before = `8e2f04c2`, after = `e4b591d2`, each
built the same way on a fresh `--no-local` clone. The machine was not idle:
other work held its load at about 25–35 (read at the probe runs just after,
`probe-local-2ms.txt`; not recorded during these), the same for both builds.
"No clamp" names the recipe, not the load.

| | before, clamp | after, clamp | before, no clamp | after, no clamp |
|---|---|---|---|---|
| paced case | **red 20 / 20** | **green 20 / 20** | green 20 / 20 | green 20 / 20 |
| unpaced, types nothing | green 20 / 20 | green 20 / 20 | green 20 / 20 | green 20 / 20 |
| unpaced, no approval | green 20 / 20 | green 20 / 20 | green 20 / 20 | green 20 / 20 |
| paced sessions with CI's signature | **100 of 100** | **0 of 20** | 0 of 20 | 0 of 20 |
| other rejected sessions | 0 | 1 of 60 (unpaced; below) | 0 | 0 |
| an accepted session failing an assertion | 0 | 0 | 0 | 0 |
| planned pauses that ended a session's longest pause | slept 50.8–107.8 ms, median 100.3 | 12.0–13.0 ms, median 12.0 | 16.0–18.0 ms, median 18.0 | 12.0–12.1 ms, median 12.0 |
| unpaced sessions' longest pause | 18.1–42.3 ms, median 21.3 | 2.6–36.1 ms, median 6.6 | 3.2–11.2 ms, median 6.2 | 2.2–10.9 ms, median 5.1 |
| seconds per run | 32–39 | 11–13 | 13 | 10–11 |

**The paced case alone, 60 more runs under the clamp after the fix**
(`ci-pr103-r2/signature.sh` at the after build, `signature-after.txt`):
60 of 60 green, 0 of 60 sessions with CI's signature or rejected at all,
planned pauses 12.0–13.3 ms, the longest pause of any session 21.6 ms.
With the 20 rounds above: **0 rejected paced sessions in 80 runs under the
clamp, which bounds the rate at about 4% a run (rule of three, 95%)**,
against 100 of 100 sessions before.

- **The paced case's RED is CI's, and the fix takes it away.** Before, every
  session under the clamp carried CI's signature and the case failed every
  time. After, no paced session was rejected, and each planned pause slept
  12.0–13.0 ms.
- **The full-queue poll is better at the median and not in its tail.** The
  unpaced sessions' longest pause fell from a median 21.3 ms to 6.6, but the
  longest of all was 42.3 ms before and 36.1 after, and one session after
  was rejected at 68.9 ms: "inside part 1 of 256 ... behind a full queue's
  2ms poll that slept 68.849792ms", its write 0.08 ms (`red-sessions.txt`,
  `utility.after.18`). Its case ran a second session, which passed.
- **That tail is time off a CPU, not the timer.** The probe, extended here
  to meet each full queue with a 2 ms `thread::sleep`, a 2 ms critical
  timer or a 2 ms spin in turn (`probe-local-2ms.diff`, never committed),
  says so (`probe-local-2ms.txt`, three runs under the clamp):

  | 2 ms wait, full queue | median | p95 | max | ≥50 ms |
  |---|---|---|---|---|
  | `thread::sleep` | 11.7–12.8 ms | 17.8–50.3 | 27.7–82.4 | 0–41 per pass |
  | critical kqueue timer | 2.03–2.07 ms | 2.7–3.1 | 24.2–53.2 | 0–23 |
  | spin | 2.000 ms | 2.4–2.6 | 35.2–111.9 | 0–51 |

  A spin has no timer, and its tail is as long as the timer's: the writer,
  clamped to priority 20 on a machine at load 25–35, sometimes waited for a
  CPU. No wait removes that. The measure counts it as a pause, as it should,
  and a session it rejects asserts nothing. On CI the spin's tail was within
  1 ms (the probe), so there the critical timer is what is left.

### §2b, looked at again

§2b's two CI failures (the unpaced paste typed as keys) were put down to the
writer not being scheduled for 50 ms [inferred; the logs had no breakdown].
This run shows a second contributor: the 2 ms poll's own sleep, fired late
under the clamp. It reached 30 ms on CI's rows in the probe, short of 50 by
itself, and 82 ms under the clamp here (`probe-local-2ms.txt`). What made
those failures' 50 ms stays [inferred]. The fix takes the timer's part away;
time off a CPU is what a rejected session is for.

### Mutants (`mutants-e4b591d2.txt`, `lib-threshold-e4b591d2.txt`, `quarantine-e4b591d2.txt`, `paced-power-*.txt`)

On a `--no-local` clone at `e4b591d2`, under the lock. The code head
`a6045a54` differs from it in one comment line and `f2b01cc1`'s numbers.
- **`mutants.py`: 22 of 22 killed**, both baselines green (unit 9, harness
  8). The two new ones are `sleep_on_time`'s (macOS): `wait-returns-at-once`
  and `wait-ends-at-a-signal` (an `EINTR` ends the wait), each killed by
  `a_wait_on_time_is_never_short_with_signals_landing_on_it`. Putting
  `thread::sleep` back is not a mutant here, since nothing at idle tells
  the two apart. The clamp runs above are that RED: the before build is
  that code.
- **`lib_threshold_mutants.py`:** each of the three quarantine mutants is
  killed by both of the lib's clock-free threshold cases.
- **`quarantine_mutants.py`, 3 runs per case:** baseline 9 of 9 green, and no
  session rejected. The paced case is red under 5 ms 3 of 3 and under
  any-quiet 2 of 3 (run 2 green), each time on an accepted session, and
  green under 20 ms 3 of 3. The unpaced cases pass under every mutant, as
  before.
- **The paced case's catch rate with no clamp, 10 runs more per build**
  (`ci-probe/paced_power.py`, `paced-power-e4b591d2.txt`): any-quiet and
  5 ms red 10 of 10, 20 ms green 10 of 10, baseline green 10 of 10. With
  the runs above, **12 of 13 catch any-quiet and 13 of 13 catch 5 ms.** §12
  had 3 of 3 each.
- **Under the clamp it catches neither** (`paced-power-clamp-e4b591d2.txt`,
  every run under `taskpolicy -c utility`): any-quiet green 10 of 10, 5 ms
  green 10 of 10, 20 ms and the baseline green 10 of 10. 40 of the 41
  sessions were accepted, with planned pauses that slept 12.0–14.2 ms; one
  was rejected behind a 53.5 ms poll, and its case's next session passed.
  **So on a clamped runner, as CI's macOS rows are, the paced case after
  this fix passes with the quarantine broken.** Before the fix it was red
  there on every build, the correct one included, so it caught nothing
  there either; it now says nothing instead of failing on every build.
  What still catches these mutants on every row: the lib's two clock-free
  threshold cases (above). What catches them in the paced case: an
  unclamped machine (above), and CI's Linux rows [inferred: their 12 ms
  sleep took 12.04–12.14 ms and nothing there read as a clamp; this run
  could not measure Linux].
- **Why a run can miss, and why the clamp blinds it** [read from the code,
  not measured]. Both mutants end the quarantine at an empty check that
  comes after an earlier empty check. With no input, the loop's next check
  is its `TICK` (8 ms) timeout (`wait_timeout`,
  `src/tui/event_loop.rs:317-322`). So a 12 ms pause catches them only when
  the TUI empties the queue in its first 4 ms or so, and its 8 ms timeout
  fires on time. Before this fix, macOS slept the pause as about 18 ms at
  idle (§12), which left more room. CI's Linux rows slept it as
  12.04–12.14 ms all along. Under the clamp the TUI's own 8 ms timeout is a
  timer that fires late, the way the harness's 12 ms one did, so the
  second check comes after the paste has resumed. That is the product's
  timer, which the brief puts out of this round's reach (look-ahead,
  below). No pause under the 50 ms bar is sure to hold a late timeout.
- **The pacing stays at 12 ms.** With no clamp, 18 ms gave the same counts
  in the same 10 runs per build, so a longer pause bought nothing
  measurable. Under the clamp no pause the bar allows would help.

### The gate

Train 16's `gates.sh` (ci.yml's gate job, 29 steps) ran at the code head
`a6045a54` twice, side by side, each on a fresh `--no-local` clone in its
own hold of the lock (`gates-rustup.txt`, `gates-brew.txt`,
`gate-counts-*.txt`). The machine's load was 15–71 through them, from other
work.

| run | how | steps | `cargo test` / with fault injection | `tests/tui.rs` / with fault injection | smoke-tui |
|---|---|---|---|---|---|
| rustup | rustup's own proxies first on PATH, `RUSTUP_TOOLCHAIN=1.99.1` | **29 of 29, tree clean** | 6080 / 6117 passed, 0 failed, 8 ignored | 201 / 215 | 37 scenarios, 1167 checks, 0 failures |
| Homebrew | PATH as this Mac has it (Homebrew's 1.99.1) | **29 of 29, tree clean** | 6080 / 6117 passed, 0 failed, 8 ignored | 201 / 215 | 37 scenarios, 1167 checks, 0 failures |

One test more than §12's gate in each count: the new wait test.
`check-no-local-paths.sh` passed in both.

Train 16's `linux-clippy.sh` ran at `a6045a54` on a fresh clone, with
rustup's own proxies (`linux-clippy.txt`). x86_64 and aarch64 Linux, plain
and with `fault-injection`: **4 of 4 exit 0.** On Linux `sleep_on_time` is
`thread::sleep`, and the kqueue code is compiled for macOS only.

### References (`content-*.txt`, `readme-refs-*.txt`, `repoint-*.txt`, `repoint_map.py`)

- **This round's test lines moved 16 references** (the content check at
  `e4b591d2` against `8e2f04c2`). `f2b01cc1` re-points them, numbers only
  (9 lines, `repoint-numbers.txt`):
  - six numbers of `tests/tui.rs:2721`'s comma list, each +64. Each is
    still the clear-line send. They were mapped through the diff
    (`repoint_map.py`, `repoint-map.txt`), because that line stands at many
    places and no window around it places one;
  - ten of this README's references into `tests/tui.rs` and
    `tests/support/pty.rs`, +2 to +84 (`repoint-readme-plan.txt`).
- **At `f2b01cc1`:**
  - Code and docs: WRONG 9, the same set as at `8e2f04c2`.
  - This README: 21. That is the 19 from before, plus §12's two references
    to `tests/tui.rs:2721`, the comma-list line itself. That line is the
    same line with its numbers re-pointed, so the check reads the changed
    text as WRONG. Read by hand.

### Numstat by class (`numstat.txt`)

| range | product | tests | receipt | other |
|---|---|---|---|---|
| the fix, `8e2f04c2..e4b591d2` | **+0 −0** | +165 −16 (`tests/support/pty.rs` +87 −2, `tests/tui.rs` +78 −14) | +0 −0 | +0 −0 |
| this round's code, `8e2f04c2..a6045a54` | **+0 −0** | +167 −18 (the fix, the re-pointed comma list, and the one-line note) | +8 −8 (the re-points) | +0 −0 |
| the branch against train 16, `1a4cb6b4..a6045a54` | +98 −33 (§2a, from before the joins) | +1821 −77 | in `numstat.txt` | +14 −14 (re-points in `docs/` and `scripts/`) |

This section's own commit is receipt only. `numstat.txt` has every range,
the commit that adds it included.

### Paths

`join/pathscan.sh` scanned every commit this branch holds that `8be4693b`
does not, through this section's commit: the messages, and what `git show`
prints of each (`pathscan.txt`). **0 matching lines, and 0 files in this
folder.** The CI logs here were masked
before they reached this folder, and each excerpt was scanned again.
`check-no-local-paths.sh` passed in both gates.

### Look-ahead: the product's own timers on a clamped runner

CI's macOS runners clamp the TUI under test as well as the harness: the
clamp holds for the whole process tree [inferred; the probe read only the
test process's threads]. What the clamp does to the product, from the code
(not fixed here):
- **The loop's turn with no input is a timer.** `wait_for_input`'s timeout
  is the `TICK` (8 ms) or `LISTING_POLL` (1 ms) (`src/tui/event_loop.rs:262`,
  `:317-322`). On a clamped runner that timeout fires late, so anything a
  quiet turn does comes later:
  - a held lone ESC becomes the Escape key later than `ESC_TIMEOUT` plus a
    tick (the module doc promises "within a tick of the timeout",
    `src/tui/input.rs:48-49`);
  - a meta pair's `PAIR_TIMEOUT` (30 ms) runs late the same way;
  - a paste's quarantine ends later than `PASTE_QUIET` after the quiet
    began. That is the safe direction: the quarantine is held longer;
  - a folder listing's rows are painted later.
- **Decoding parity holds.** A byte that comes after the timeout is read
  against the ESC's own time when it is fed (`Decoder::feed`/`flush`
  compare `now` with the ESC's `at`; `src/tui/input.rs:4316-4330` pins
  `ESC ESC` then `[` at `ESC_TIMEOUT` as two Escapes and a `[`). So a late
  turn changes when an Escape lands, not what the keys are [read from the
  code].
- **The paced case is blind under the clamp** (Mutants, above): the TUI's
  late 8 ms timeout means no empty check falls inside a 12 ms pause, so a
  quarantine that ends too early is not seen there. The lib's clock-free
  cases still catch it on every row.
- **The risk is to tests with a time bound**, not to what keys decode to: a
  test that bounds how soon an Escape lands or a quiet ends could miss its
  bound on a clamped runner. Under the clamp here, no session the fixed
  harness accepted failed an assertion: 80 paced sessions and 40 unpaced
  ones. Whether any other `tests/tui.rs` case has such a bound was not
  measured until this run. **One was found:** the whole tui binary ran
  twice under the clamp and twice without (`whole_suite.sh`,
  `suite-clamp.txt`). Under the clamp,
  `a_line_the_shell_did_not_end_is_pushed_with_the_rows_above_it` failed
  once ("the unfinished line was not pushed away"); every other run passed
  215 of 215. The product gives the terminal `probe::DEADLINE`, 100 ms
  (`src/tui/probe.rs:51`), to answer its cursor query, and assumes row 1
  after that. The test answers from the harness, after `wait_for` has seen
  the query. Under the clamp, `wait_until`'s 10 ms sleep
  (`tests/support/pty.rs:537`) and the reader's 2 ms poll are both timers
  that can fire late. So the answer can come after 100 ms, and the product
  then does what it does for a terminal that never answers [inferred from
  the code; the failure's own output does not time the answer]. It is in
  no failure of §1's table, and it passed in every PR #103 and probe log
  here, the macOS rows' included. The same `sleep_on_time` change to the
  harness's other two polls is the likely fix. Not made here: the brief
  scoped this round to the paste measure's waits.
- **The reader thread's 2 ms poll is not changed.** Under the clamp it is
  late too (Linux is not), so the reader drains the TUI's output later and
  the paste takes longer. It is not in the measure. If a test ever bounds
  how fast output is read, it is the same `sleep_on_time` change.

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
     not retried. The paced case's comment says so (`tests/tui.rs:4329-4335`).
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
3. **CI's test parallelism is unchanged, but for the three hostile pastes**
   (§12). None of the first three causes needed a change. PR #103's first
   red was answered by running the three cases one after another; its second
   showed CI's cause was the harness's own timers (§13), and the lock stays,
   for the reason measured here, which CI's numbers do not decide. The
   suite's thread count and every other test's parallelism are as they were.
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
