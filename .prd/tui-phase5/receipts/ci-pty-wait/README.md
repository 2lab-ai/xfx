# CI-PTY-WAIT — receipt

Unit: CI-PTY-WAIT. Base `dcfa5146` (main). Branch `fix/p5-ci-pty-wait`: one
product commit, `f79f5e5b`; five test commits, `f311ad79`, `a70dcd89`,
`a77fad76`, `7b524d78` and `765f9170`; and the receipt commits. Every figure
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
  (`take_status_moved_by_input`, `src/tui/shell.rs:11487`; `Keyed::NotYet` at
  `:11516`).
- So when one drain held the turn's start, its answer and its end, that tick
  reported the settled `done` and nothing else. When it held the start and the
  first answer bytes, the turn's first frame showed `Generating`.

**Fix** (`f79f5e5b`):
- The drain stops right after a turn start whose `working` is owed after the
  frame that first shows the turn. That is a prompt held for its turn, or one
  this tick's keys sent (`Shell::turn_start_owes_its_frame`,
  `src/tui/shell.rs:11544`; the stop, `src/tui/event_loop.rs:458`). Since
  round 2 the predicate is the tick's own answer, `keyed()` (`:11501`), which
  `take_status_moved_by_input` takes: §7.
- That tick's frame shows the turn beginning (`Thinking`) and `working` follows
  it. The rest of the turn is read on the next tick.
- The OSC 7501 row in `docs/parity.md` says so.
- `a_turn_start_owes_its_frame_exactly_when_its_tick_writes_working_after_the_frame`
  (`src/tui/shell.rs:22276`; `a77fad76`, widened in round 2) holds the drain's
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
`PASTE_QUIET` (50 ms, `src/tui/input.rs:674`): "a terminal that pauses that
long inside one paste ends the quarantine there" (`src/tui/input.rs:134-135`).
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
- `paste_unpaced` (`tests/tui.rs:3572`, `7b524d78`) stops writing at the first
  pause of `PASTE_QUIET` or more. That session is **timing-indeterminate**:
  the measure is an upper bound on how long the input queue can have been
  empty, not a reading of it, so the session neither proves nor disproves the
  quarantine. Since round 2 it keeps its evidence -- the pause, and the
  screen a second later -- printed and handed back.
- `unpaced_case` (`tests/tui.rs:3698`) then runs a new session, at most 3 in
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
(`tests/tui.rs:14546`) reads the sandbox's whole `home` and `workspace` paths,
case folded (`found_in_sandbox`, `:14576`). A word in the root's random name
draws the root again, as `src/tui/shell.rs`'s own `skilled_in` fixture does; a
word above the root -- the machine's `TMPDIR`, which no draw changes -- fails
at once, saying so, where the case used to time out
(`local/skills-foo-tmpdir-round2.txt`: 21 s at `c601ef29`, at once at round 2).
`a_skills_sandbox_path_never_holds_what_its_case_searches` (`:14599`) pins
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
| …a quarantine regression cannot go green only through rejection and retry | `round2/quarantine.txt`: 3 quarantine mutants × 3 cases × 3 runs. **No run of any case rejected a session.** The new paced case (20 ms between parts: a loop turn and more, under `PASTE_QUIET`) failed 3/3 under each mutant, each time on an accepted session. The two unpaced cases stay green under the mutants: their refill gaps at idle (3–10 ms) are under one loop turn, so the loop never records a quiet's start there (`round2/quarantine-first-pass.txt`), which is what the paced case was added for. Narrowed in round 3 (§8): acceptance is measured on the writer and does not prove the reader saw each pause; the threshold itself is pinned without a clock by `src/tui/input.rs:3549` and `:4148`, not by this case; the 20 ms mutant's kill rested on the writer oversleeping its 20 ms sleep (gaps of 30–37 ms); and a regression can at worst give a misleading red, never a green. | `tests/tui.rs:3756` (paced), `:3579` (`paste_with_pauses`) | baseline 9/9 green; each mutant 3/3 red on the paced case |
| opus NIT 1: predicate | `keyed()` works out the take's answer, taking nothing, and both the take and the drain's stop ask it. A held start a stop covers (`cancelling`) or a reset's drain drops (`draining`), with no key behind it, owes no frame. "Exactly" now holds by construction, and the name keeps it. | `src/tui/shell.rs:11501`, `:11542`; case `:22274` | RED by `predicate-by-flags-alone` (round 1's predicate), `owed-only-when-held`, `owed-only-when-keyed` |
| opus NIT 3: the drain reads past a start that owes no frame | A prompt queued behind a running turn; and a tick whose keys owe a report (a question answered) with no start among its events. | `src/tui/event_loop.rs:16704`, `:16743` | RED by `drain-stops-at-every-start`, `drain-stops-after-any-event` |
| opus NIT 4: whole path | `found_in_sandbox` checks `home` and `workspace` whole, case folded. A word in the root's name draws again; one above it fails at once, saying so. | `tests/tui.rs:14483`, `:14513`; case `:14536` | RED by `search-check-root-name-only`, `search-check-case-sensitive`; `local/skills-foo-tmpdir-round2.txt` (21 s timeout at `c601ef29`, a clear panic at round 2) |
| opus NIT 5: `PASTE_QUIET` | Kept as a copy. The product's is `pub(crate)`, out of this suite's reach, and the comment names its line, `src/tui/input.rs:674`. | `tests/tui.rs:3630` | — |
| opus NIT 6: line refs | Every ref in this README was refreshed against `62ea9f56`; outside this table they are at round 3's head now (§8). | — | — |
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
`r2-opus.md`). Refs are at `f8bc123a`.

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
  `round3/lib-threshold.txt` shows `src/tui/input.rs:3549` and `:4148` killing
  all three mutants, as opus r2 said.

### The other items

| item | change | where | RED → GREEN |
|---|---|---|---|
| S2: label the clock defect | The paced case's comment says a red with every pause under 50 ms is not retried and can be the clock-after-poll defect (`src/tui/event_loop.rs:750-751` → `src/tui/shell.rs:11123-11124` → `src/tui/input.rs:822`), which PASTE-QUARANTINE-CLOCK fixes; so does open question 1 here. | `tests/tui.rs:3821-3827` | docs |
| S3 and gpt6 r2 SHOULD: narrow the claims | The paced comment says acceptance is measured on the writer; the threshold is pinned by `input.rs:3549`/`:4148`, not by this case; and the 20 ms mutant's kill rests on the writer oversleeping. `unpaced_case` now says a regression gives at worst a misleading red, never a green. The round-2 table's row is narrowed the same way. | `tests/tui.rs:3813-3820`, `:3688-3697`; §7 | docs |
| NIT 1: predicate doc | "unless this tick's keys moved the status, when the tick writes its report after its frame whatever the start shows" | `src/tui/shell.rs:11530` | docs |
| NIT 2: clock-expiring cues | The equivalence case reads the hint row at the keys' own tick (`cue`) and leaves it out of the end screen. Its cue is asserted per key: "press ctrl+c again to exit" after a stop, the plain hint otherwise. | `src/tui/event_loop.rs:16389`, `:16479`, `:16539` | RED still by `drain-never-stops` (`round3/mutants-f8bc123a.txt`) |
| NIT 3: tail cut | `last_bytes` backs a 400-byte cut off to a character's start. | `tests/tui.rs:3634`; case `:3643` | RED by `last-bytes-cut-anywhere` |
| NIT 4: stop-next parity | Recorded as open question 5. | — | — |

- **Mutants** on a `--no-local` clone under the lock: the round-2 14 plus
  `last-bytes-cut-anywhere`, all 15 killed with both baselines green, at
  `6acbcd7e` and again at `f8bc123a` (`round3/mutants-*.txt`).
- **Per-commit path grep** over `rev-list --no-merges HEAD --not origin/main`:
  0. fmt is clean, and both clippys are clean.

## Open questions

1. **The quarantine's clock is read after the readiness check.**
   - `burst` calls `shell.end_delivery()` once its check finds nothing
     (`src/tui/event_loop.rs:750-751`).
   - That reads `Instant::now()` (`src/tui/shell.rs:11123-11124`).
   - The decoder compares it with the quiet's start (`src/tui/input.rs:822`).
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
     not retried. The paced case's comment says so (`tests/tui.rs:3821-3827`).
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
   (`Shell::take_status_moved_by_input`, `src/tui/shell.rs:11487`) and the
   drain does not stop behind it. When the stopped turn's end and the prompt's
   start, answer and end all land in one drain, the session reports
   `[.., idle, done]` with no `working` [read from the code, as opus read it;
   not run]. No test asserts it, and fx's behaviour in that schedule is
   unverified. Recorded, not fixed.
