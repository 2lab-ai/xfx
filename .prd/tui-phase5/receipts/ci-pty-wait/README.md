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
| `the_session_reports_its_status_between_frames_and_clears_it_on_the_way_out` | 5 of 50 jobs (PR #94: 3 of 3 attempts) | **product** (§2a): one drain could read a prompt's turn start, its answer and its end, so the session reported `idle`, `done` with no `working`, or wrote `working` behind a frame showing `Generating`. fx cannot do either: its keystroke holds the worker's start until it has reported `working`. | `src/tui/event_loop.rs`, `src/tui/shell.rs` |
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
- Jobs `128812788998`, `129416346998` and `130270115640` (tests/tui.rs:852):
  the reports were `[idle, done]`, with no `working` at all.
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

**xfx's path at base:**
- Nothing holds the runtime: it starts the prompt as soon as it is sent.
- Each tick reads the keys (`read_burst`, `src/tui/event_loop.rs:279`), then
  drains the runtime (`take_ui_events`, `:289`; `drain_ui_events`, `:365`),
  taking everything on the channel.
- A prompt's `working` waits for the frame that first shows its turn
  (`take_status_moved_by_input`, `src/tui/shell.rs:11092`; `Keyed::NotYet` at
  `:11108`).
- So when one drain held the turn's start, its answer and its end, that tick
  reported the settled `done` and nothing else. When it held the start and the
  first answer bytes, the turn's first frame showed `Generating`.

**Fix** (`f79f5e5b`):
- The drain stops right after a turn start whose `working` is owed after the
  frame that first shows the turn. That is a prompt held for its turn, or one
  this tick's keys sent (`Shell::turn_start_owes_its_frame`,
  `src/tui/shell.rs:11132`; the stop, `src/tui/event_loop.rs:396`).
- That tick's frame shows the turn beginning (`Thinking`) and `working` follows
  it. The rest of the turn is read on the next tick.
- The OSC 7501 row in `docs/parity.md` says so.
- `a_turn_start_owes_its_frame_exactly_when_its_tick_writes_working_after_the_frame`
  (`a77fad76`) holds the drain's stop to the tick's own `Keyed::AfterFrame`, for
  all four combinations of held and keyed.

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
- `paste_unpaced` (`tests/tui.rs:3448`, `7b524d78`) stops writing at the first
  pause of `PASTE_QUIET` or more and returns it.
- `unpaced_case` (`tests/tui.rs:3482`) then runs a new session, at most 3 in
  all, and fails saying so when every session paused.
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

**Fix** (`f311ad79`): `skills_sandbox` (`tests/tui.rs:14257`) draws the root's
name again while it holds a word the case searches, as `src/tui/shell.rs`'s own
`skilled_in` fixture does. `a_skills_sandbox_root_is_never_named_with_what_its_case_searches`
pins `names_a_search`.

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
- **Recipe B, again**: the same recipe for 2 rounds, head at `7b524d78`.
  `local/recipe-b2/`

Failures out of runs (`rates.md` in each folder):

| test | A: base | A: head | B: base | B: head (`a77fad76`) | B2: base | B2: head (`7b524d78`) |
|---|---|---|---|---|---|---|
| status between frames | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | RUN4_STATUS |
| hostile paste, types nothing | 0 / 20 | 0 / 20 | 18 / 18 (body typed as keys) | 18 / 18 (16 "no session tested the paste", 2 write bound) | RUN4_TYPES |
| hostile paste, answers no approval | 0 / 20 | 0 / 20 | 18 / 18 (body typed as keys) | 18 / 18 ("no session tested the paste") | RUN4_APPROVAL |
| slash skills | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | RUN4_SKILLS |
| compaction Ctrl-C | 0 / 20 | 0 / 20 | 0 / 18 | 0 / 18 | RUN4_COMPACTION |

What the rates show:
- **Hostile paste.** In recipe B, base fails every run with CI's symptom. Head
  fails every run too, but the failure now says what happened: no session could
  write the paste unpaced, rather than the product typing the clipboard. In
  recipe A, where the longest pause was 13 ms, all three builds pass.
- **Status test.** Neither recipe reproduces it; its RED is the unit case
  (§2a).
- **Slash skills.** It is not a timing failure, so its RED is the foo-named
  `TMPDIR` (§2c), deterministic at base.
- The breakdown of every recipe B failure is in
  `local/recipe-b/failures.txt` (and `local/recipe-b2/failures.txt`).

## 4. Mutants — `mutants.txt`, `mutants.py`, `mutants-r1.txt`

`mutants.py` runs on a `git clone --no-local` copy, never the worktree.

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

## 5. Gate — `gate.txt`, `gates.sh`

`gates.sh` ran in the worktree at `a70dcd89`, under the suite lock, once:
- fmt, clippy and clippy with the fault injector: clean.
- `cargo test --no-fail-fast`, plain and with the fault injector: no failure
  (`tests/tui.rs` 192 and 206 cases). There was no catalog-wait failure to
  rerun.
- `check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh`,
  `check-no-local-paths.sh`: exit 0.

Later commits:
- `a77fad76` added one unit case; `gate.txt`'s tail has fmt, both clippys and
  the `tui::shell` module rechecked at it.
- `7b524d78` changed only `paste_unpaced` and its two callers. fmt, both
  clippys and the five hostile-paste and harness cases pass at it.

## Open questions

1. **The quarantine's clock is read after the readiness check.**
   - `burst` calls `shell.end_delivery()` once its check finds nothing
     (`src/tui/event_loop.rs:688-689`).
   - That reads `Instant::now()` (`src/tui/shell.rs:10732-10733`).
   - The decoder compares it with the quiet's start (`src/tui/input.rs:822`).
   - A UI thread preempted between a check that found nothing and that clock
     read stretches a short refill gap into a "50 ms quiet". So the quarantine
     can end on a pause shorter than `PASTE_QUIET`, and the rest of a hostile
     paste is then typed as keys.
   - Not seen here: every recipe B failure had a writer pause of 305 ms or
     more.
   - It is a candidate follow-up in PASTE-GAP's security area: compare against
     a clock read before the check, and stamp the quiet's start with one read
     after it.
   - Not changed in this unit.
2. **xfx's keystroke frame shows no `Thinking`, and fx's does.** xfx paints
   `Thinking` only once the turn starts. That is the documented divergence in
   the OSC 7501 row, unchanged here: the fix keeps xfx's frame and makes the
   turn's first frame show `Thinking`.
3. **CI's test parallelism is unchanged.** None of the three causes needed it.
