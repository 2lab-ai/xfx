# LAUNCH-SETTLE: the capture harness waits for the binary to be up

Harness only (not an **S** unit): `scripts/fxcap/capture.sh`, `capfile.py`,
`test_capture.py`, `test_sweep.py`, `test-capture.sh` and three scenarios,
`h1-models2.cap`, `p5-mcp-fail.cap` and `p5-mcp-menu-old.cap`. No product
code changes. Base `train/p5-1108a` at `7589ec42` (train 4, PR #80).

It closes what FLAKE-HINT4's two round-2 reviews left open (opus r2 SHOULD
1-4 and its item (a); gpt6 r2's settle wording), and the 15th and 16th
sightings of registry #19 (timing flakes in the xfx tests and harness).

## Commits

| commit | what |
|---|---|
| `5b437b8c` | RED: the new and changed tests, over the base harness and check |
| `b734d834` | the fixes (capture.sh, capfile.py, h1-models2.cap, the wrapper test, `nodraw` in the /bin/cat tests) |
| `85428cc9` | one test made stricter: the gone-pane case (below) |
| `5bd5c05f` | two scenarios: the snaps the full sweep dropped wait for the screen (first form) |
| `84ee1680` | the same two, settled (Sweep, below) |
| `35e2e977` | a comment in `p5-mcp-menu-old.cap` corrected |
| `6f3cb284` | the receipt |
| this one | `final-checks.txt`, run at `6f3cb284` |

`85428cc9` is a third commit because its RED was found after the fix: the
first form of `test_a_binary_gone_at_once_is_final_under_keep_and_fails_the_run_without`
used `snap a` after the wait, and a `snap` fails a run with no pane anyway, so
that half passed on the base harness too. `85428cc9` asks `home h` instead,
which needs no pane. `gone-pane.txt`: exit 0 on `7589ec42`'s capture.sh,
exit 1 at the launch wait on `b734d834`'s.

**Which build is where.** `85428cc9` changes `test_capture.py` only, so
`capture.sh`, `capfile.py` and every scenario are the same at `b734d834` and
`85428cc9`, and so is every compiled file. The probes (`up-probe.txt`,
`early-cc.txt`, `interrupt-fresh.txt`, `warning-after-up.txt`,
`models2-late.txt`) ran `b734d834`'s harness with xfx built at `b734d834`
(row 1 `x𝒇x v0.1.0-b734d83 [debug]`). The full sweeps, the mutants and the
gate ran at `85428cc9` (xfx row 1 `v0.1.0-85428cc`). `5bd5c05f`, `84ee1680`
and `35e2e977` change two .cap files and nothing else. So the full sweep at
`85428cc9` stands for every other scenario, and nothing the mutants or the
gate's cargo, smoke and build steps read changed. The two scenarios were
swept four more times at `84ee1680` (xfx row 1 `v0.1.0-84ee168`,
`conversions.txt`). `35e2e977` changes a comment only. `test-sweep.sh`,
which reads every .cap, and the quick checks were run again at the final
head (Gate).

## Item 1: the launch wait

**What.** A scenario's first line, and the line after every `relaunch`, runs
only once the binary is up (`capture.sh` `await_up`; its header says so).
"Up" is the binary's first frame: anything but blanks on the pane's screen.
The wait looks every 0.05 s, for 30 s of elapsed time, and fails the run
(exit 1) with an `up: nothing on the screen ...` line if no frame comes.
Under `keep`, a pane that is already dead is a final state: the scenario
goes on and reads it. Without `keep`, a pane that is gone, or dead, fails the
run. The wait prints one `up:` line. Where it returns, `$run/launched` is
stamped, so "since launch" in every `until` and `absent` line now counts from
the first frame. That is also where the full-access warning's own clock
starts (xfx `src/tui/permissions.rs` `observe`, :409-436).

**Why the screen and not ISIG.** opus r2 offered both. I picked the screen:
- **It implies ISIG off, on both binaries.** Both make the terminal raw
  before they write anything visible: xfx at `src/tui/mod.rs:213`
  (`term::enter_raw`, before the band's first frame), fx at
  `src/core/app/app_lifecycle.zig:880@481104fd` (`enableRawMode`, before
  `queryLayout` and the shadow VT). Raw clears ISIG (xfx `term.rs:157`; fx
  `src/ui/shell_runtime.zig:119@481104fd`).
- **Measured** (`up-probe.txt`): `probe/probe.patch` makes the wait read the
  pane tty's ISIG (`stty -a -f`) before every look and look every 0.01 s.
  Six launches, three per binary, each of a fresh copy: 186 samples.
  - Every look that saw a drawn screen saw `-isig`: drawn with ISIG still on,
    0.
  - ISIG on with a blank screen: 119 samples. ISIG off before the first
    frame: 1 sample, on xfx.
  - After the first frame, `-isig` on all 60 further samples.
- **It works where the tty cannot be read.** It is one `capture-pane`, the
  harness's own verb, and the stub binaries of `test_capture.py` show a
  prompt or a line. Reading another process's tty modes needs `stty -f`
  (BSD) or `stty -F` (GNU). And an interactive bash stub keeps ISIG on at
  its prompt (`tmux-checks.txt`: `-icanon isig -echo`), so it would never
  read as up.
- `nodraw` is the one way out: for a stub that draws nothing until it reads
  input. Only `test_capture.py`'s /bin/cat tests use it. No scenario does.

**RED → GREEN.**
- `red.txt`, `LaunchWait`: on the base harness a stub that comes up 1.5 s
  late (sleep, then `stty -isig`, then a line) dies by SIGINT from a
  first-line Ctrl-C (`Pane is dead (signal int`) and from a Ctrl-C right
  after a `relaunch`; "since launch" read 3.7 s for a row drawn as the stub
  came up; a binary that never draws and one gone at once were not waited
  for. All pass at the head (gate, `test-capture.sh` ×3).
- **On the real binaries** (`early-cc.txt`): `probe/zearly.cap` (a Ctrl-C
  0.5 s after the launch) on fresh copies. On the base harness, 6 of 6
  panes died by SIGINT, fx 3 and xfx 3. On the head harness, 0 of 6 died:
  the waits were 0.69 to 6.25 s, at load 8.3.
- `interrupt-fresh.txt`: `interrupt.cap` itself, on fresh copies, two runs a
  side. On the base, xfx run 2 failed: `no server running`, the pane gone by
  SIGINT before its first snap (the 15th sighting again). On the head, 4 of 4
  ran, with first frames at 0.33 to 7.83 s.

## Item 2: `settle`, re-measured, and `absent` over a launch-armed timer

**Decision.** For a timer the check knows a launch armed (the full-access
warning under a yolo profile or environment), an `absent` never counts it
gone. opus r2 offered two ways: refuse it, or show that the re-anchored
clock makes it sound. I refused it. If such an `absent` passes, either it
looked before the binary drew the row, or the binary never armed the row,
and a snap pair shows the second anyway. So its word adds nothing, and on a
loaded machine it could clear the check vacuously. The one way left to count
that row gone is the screen: an `until shown` that sees it, then an
`until gone` or an `absent`.

The rule in `capfile.py` `races` is now:
- an `absent` counts once an `until shown` has seen the row in this arming;
- or, for a key-armed timer, from `settle` seconds after the key;
- otherwise it does not count.

`settle` is now a key-arming allowance only. The warning has none (no key
arms it), so its `settle=3.0` is deleted. `test_settle_is_an_allowance_for_a_key_only`
pins both halves.

The wording follows gpt6 r2: the `Timer` docstring and the hint's comment
call `settle` a measured allowance, not a bound either binary promises.

**`test_sweep.py:714` flipped.** `YOLO / wait 3 / absent Full access / wait
1 / snap a` was `[]`. It is now a race on both binaries. `capfile-cases.txt`
runs 12 cases through both checks. They differ on 5:
- the flip at :714, plus the same at 3.5 s and after a relaunch: race now,
  none before;
- `absent` after an `until shown` saw the row, warning or hint: it counts
  now, and waited a settle out before. A row on the screen is drawn.

**Is `until gone` over a launch-armed warning still sound?** It still
discharges the timer. That is sound only if the binary's first frame already
holds the warning, because the launch wait sends a scenario's first `until`
after that frame. Measured:
- `warning-after-up.txt`: six fresh launches. The warning was on the screen
  0.01 to 0.02 s after the wait returned, on the first or second look.
- The sweeps' yolo scenarios whose first line is `until shown Full access`
  (`waits-*.txt`, "full-access warning"): fx 12 runs, longest 0.02 s, on both
  sweeps. xfx 11 runs: median 0.07 s and longest 0.08 s on the base, where
  the clock started at the launch; median and longest 0.01 s on the head,
  where it starts at the first frame. That drop is the re-anchoring itself.

This is a measurement, not a guarantee. A binary that drew the warning a frame
later than its first would make that `until gone` early.

### settle

One threshold is left, the exit hint's `settle=1.0` after the Ctrl-C that
arms it. Re-measured after item 1, as the time from the `key C-c` line to
the hint on the screen (`waits-*.txt`, "Ctrl-C hint, `until shown` straight
after `key C-c`"):
- base sweep: fx 4 runs, longest 0.01 s; xfx 4, longest 0.02 s;
- head sweep: fx 4, longest 0.01 s; xfx 4, longest 0.01 s, at a peak load of
  156;
- the larger earlier sample: within 0.03 s in every FLAKE-HINT4 round-2 head
  sweep (`../flake-hint4/r2/until-waits.txt`).

The launch wait does not move this time, which runs from a key, so the value
stays at 1.0 s, about 30 times the longest seen. capfile.py now calls it what
it is: a measured allowance with room for a loaded machine, not a bound
either binary promises (gpt6 r2). The warning's old `settle=3.0` was the
other threshold. Its 3.0 came from a launch's first frame at 2.13 s, and it
is gone with the rule above.

## Item 3: h1-models2 waits for the one-model catalog

`h1-models2.cap`: after the first `/model`, the scenario now waits with
`until shown Models 1  [All]`, and its snap carries `expect before Models 1
[All]`. While the launch's ask is unanswered, both binaries' menus say
`Models 0` / `Loading models…`. They switch to the one-model list when the
answer lands, menu open or not. So the launch ask is answered before the
`models2` line, in order.

`models2-late.txt` holds the launch's ask 15 s (`modelsdelay 15000`), past
the `models2` line:
- **Base scenario.** `before` showed `Models 0`, and both binaries' launch
  asks got the **second** catalog (`-> second catalog`, `after` = `Models 2`).
  The run exited 0, so nothing flagged it.
- **Head scenario.** The `until` waited 10.95 and 10.97 s, `before` showed
  `Models 1`, and the asks got the first catalog.

## Item 4: `expect` reads only this run's snaps

`snap` and `snapfull` append their label to `$run/snapped` (a `snapfull`
label as `<label>.full`). `expect` refuses a label this run did not snap
(`no snap <label> was written before it in this run`), even when a file of
that name is in the out directory.
- RED (`red.txt`, `test_expect_reads_only_a_frame_this_run_wrote`): on the
  base harness, a frame an earlier run left passed `expect`.
- It does not use `-nt "$run/launched"`, as the review suggested. bash 3.2's
  `-nt` compares whole seconds, so a snap taken in the second the run began
  would read as stale. Also, `launched` moves at a `relaunch`.

## Item 5: `seen` is reset at `relaunch`

A relaunch starts a new binary on a cleared screen. tmux's `respawn-pane`
clears it (`tmux-checks.txt`: blank at once after it). So what an `until`
saw before the relaunch says nothing about the new binary. `never seen` now
means "since the binary was last launched", and the header says so.
- RED (`red.txt`, `test_never_seen_is_of_the_binary_since_its_last_launch`).

## Item 6: the wrapper test waits for the exec

`test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run`
now waits on states, not time:
- its wrapper turns the terminal's echo off and prints `armed`;
- the scenario waits for `armed`, types `ready`, and waits for `ready`.
  With echo off, `ready` reaches the screen only once cat has read it and
  written it back, so the pid runs /bin/cat.

A second subtest puts `sleep 4` before the exec, as a loaded machine made it.
- RED (`red.txt`): with the old `wait 1`, that subtest failed with
  `executable /bin/bash`, the 16th sighting reproduced on demand.

## Sweep, base against head

**Full sweeps** (`sweep-compare.txt`; scoreboards `sweep-base-1.md` and
`sweep-head-1.md`). Every scenario, 332 units; the brief's 313 predates
train 4. 40x120, `--jobs 2`, each under the suite lock.
- base: `7589ec42`'s harness and scenarios, xfx at `7589ec4`, a fresh copy.
  Wall 138 min, load 4.6 to 25.1.
- head: `85428cc9`'s, xfx at `85428cc`, a fresh copy. Wall 150 min, load 7.7
  to 156.4 (other workers' jobs). Its first xfx launch drew in 1.09 s.
  Every other xfx launch drew in 0.23 s or less, and every fx launch in
  0.12 s or less (`waits-head-1.txt`; 726 `up:` lines).
- Both: 1716 pairs, **1102 pass** (64.2%), 573 unresolved, 2 failed, 0
  timed out.
- The 2 failed pairs are `p5-panels-badsettings` on fx, in both sweeps. fx
  ends at once under that scenario's bad settings, and the scenario has no
  `keep`. On the base the run failed at `no server running`; on the head it
  fails at `up: the pane is gone`. That is the same failure, now named.

**7 pairs changed in run 1.** Their 5 scenarios were run three more times a
side, alternating base and head, at load 4.8 to 14.6. Four runs a side
(`sweep-compare.txt`):

| pair | base (4 runs) | head (4 runs) | what moved it |
|---|---|---|---|
| `model-column-idle/col-down`, `col-idle` | fail 2, pass 2 | fail 1, pass 3 | timing: `modelsdown 9000` is on the gateway's clock, which the launch wait does not touch; a coin flip on both sides |
| `model-column-idle/col-typed` | pass 2, fail 2 | pass 3, fail 1 | timing, as above |
| `model-parked/menu-loading` | fail 4 | pass 1, fail 3 | timing [inference]: a mid-turn moment, fx `~ Thinking` against xfx `~ Generating`; it passed once, on the head |
| `read-failures/resumed` | identical 4 | normalized-identical 1, identical 3 | a pass both ways; the normalized run differs only in a normalized cell |
| **DROP** `p5-mcp-fail/server-1` | pass 4 | fail 2, pass 2 | timing, moved by item 1: fixed below |
| **DROP** `p5-mcp-menu-old/closed` | pass 4 | fail 1, pass 3 | timing on a 500 ms window, not moved by item 1: fixed below |
| **DROP** `p5-mcp-menu-old/end.full` | pass 4 | fail 2, pass 2 | as `closed` |

**The three drops, bracketed on the base harness** (`drops-bracketed.txt`):
- **`p5-mcp-fail/server-1`.** The failing stdio server is retried with a
  1 s backoff that doubles (xfx `src/mcp/runtime.rs:90`, after fx's
  `reconnectDelay`), so one try lands near 15 s. The scenario's Enter opens
  the server's screen at 15 s. Opened just after that try, fx shows
  `Loading tools…` until the next try, near 31 s. The launch wait moved the
  Enter about 0.08 s later against fx's own clock. On the base harness, a
  `wait 0.2` up front flips fx to `Loading tools…` in 3 of 3 runs. This is
  item 1's timing move crossing an edge that was already there.
- **`p5-mcp-menu-old/closed` and `end.full`.** The ending was `Escape / wait
  0.5 / Escape / wait 0.5 / snap`, and both waits sit on the edge of the
  500 ms `esc again to clear` window (xfx `gesture.rs:70`, fx
  `gesture_state.zig:4@481104fd`). On the base harness, 0.45 s shows the
  row on both binaries, and 0.6 s shows it on fx and not on xfx. Nothing
  item 1 changed sits between those keys and the snap.

**Fixed at the source**, as FLAKE-HINT4 fixed its timers, by waiting for the
screen. The forms tried and their sweeps are in `conversions.txt`.
- **`84ee1680` `p5-mcp-fail`.** `until shown Its tools couldn't load`, then
  the old `wait 3`. 5bd5c05f's form snapped right after the `until`, while
  fx still showed the try as `connecting`: it failed 4 of 4.
- **`84ee1680` `p5-mcp-menu-old`.** One Escape, `until shown esc again to
  clear`, two Backspaces that empty the `xd` draft, `until gone esc again to
  clear`, then the snaps. So `closed` and `end` read an empty prompt with no
  timer up.
- **Four sweeps of both at `84ee1680`** (`conversions.txt`, `conv2-*`):
  **0 drops** against the four base runs.
  - Every pair of both scenarios passes 4 of 4, except the two
    `p5-mcp-fail` pairs that fail in every run on both sides (`show-bad`,
    `end.full`).
  - fx met the try near 31 s in 2 of the 4 runs. Its `until` waited 16.0 s
    there and the pair still passed, so both phases are covered.
  - A forced late run (`wait 0.3` up front) passed too.
- Each scenario's `startup` and `list` snaps are taken before the changed
  lines, so they keep their place.

**One more difference seen, not this unit's.** In those sweeps, fx's `until
gone esc again to clear` returned 0.01 to 0.02 s after the Backspaces. xfx's
returned 0.42 to 0.53 s after, when its window ran out (`conversions.txt`).
So fx takes the hint down on a Backspace, and xfx keeps it up for the rest
of its 500 ms [inference from the waits]. No pair shows it, because the
scenario waits it out. A parity SHOULD for the gesture unit; I changed
nothing for it.

## Mutants

`mutants.py` holds 17 one-place mutants, each run on a `git clone --no-local`
at `85428cc9`, and each against the test that should catch it
(`mutants.txt`):
- 11 in `capture.sh`: no wait at launch; no wait at a relaunch; a blank
  screen counted as a frame; the 30 s bound giving up silently; a dead pane
  under `keep` failing the run; a gone pane taken for up; `since launch`
  counted from the launch; `nodraw` ignored; `seen` kept over a relaunch;
  `expect` reading any frame of the name; a `snapfull` label recorded
  without `.full`.
- 6 in `capfile.py`: the base `absent` rule; `until shown` not counted as
  seen; no settle after a key; a relaunch keeping `seen`; a paused row's
  `absent` counting once seen; the warning keeping a settle.

**17 of 17 killed, but only after one fix.** The first form of L7 (`since
launch` from the launch) was a no-op: it stamped `launched` when the wait
began, and the stamp after the loop overwrote it. So it survived. The
corrected L7 copies the begin stamp's mtime instead, and it was killed. Both
lines are in `mutants.txt`.

## Gate

`gates.sh` is train 4's gate script with `test-capture.sh` run three times.
It ran at `85428cc9` in a clean clone, and each heavy step held a slot of
the suite lock (`gate.txt`). 22 steps, 0 failed:
- fmt, both clippys;
- `cargo test`: 5279 passed, 0 failed, 7 ignored. With the fault injector:
  5316 passed, 0 failed, 7 ignored;
- every `check-*.sh` and `test-*.sh`;
- fxcap `test-sweep.sh`: test_sweep 67 OK, test_ci_sweep 26 OK;
- `test-capture.sh` three times: 67 OK, 67 OK, 67 OK. This unit's purpose
  is that it passes every time;
- both release builds;
- smoke: 47 checks, 0 failures. smoke-tui: 37 scenarios, 1127 checks, 0
  failures.

At the final head (`35e2e977` plus this receipt), `test-sweep.sh` and the
quick checks were run again: `final-checks.txt`.

## Registry #19

The 15th sighting (`interrupt`'s first-line Ctrl-C before raw mode) and the
16th (the wrapper test's `wait 1` against a late exec) are what this unit's
durable fixes address: the launch wait, and the state-based exec wait. The
registry is the dispatcher's, outside this repository, so this receipt does
not edit it.

## Files

- `red.txt`: the RED runs at `5b437b8c`.
- `up-probe.txt`, `probe/probe.patch`, `probe/zup.cap`: the ISIG probe.
- `early-cc.txt`, `probe/zearly.cap`; `interrupt-fresh.txt`: item 1 on the real binaries.
- `warning-after-up.txt`, `probe/zyolo.cap`: the warning after the wait.
- `capfile-cases.txt`, `capfile_cases.py`: item 2's cases through both checks.
- `models2-late.txt`: item 3.
- `sweep-compare.txt`, `sweep-base-1.md`, `sweep-head-1.md`: the full sweeps and the reruns.
- `drops-bracketed.txt`, `conversions.txt`, `escape-edge-trial.txt`: the three drops and their fix.
- `tmux-checks.txt`: respawn clears the screen; a bash stub keeps ISIG.
- `gone-pane.txt`: `85428cc9`'s case on both harnesses.
- `waits.py`, `waits-base-1.txt`, `waits-head-1.txt`: every wait line of the full sweeps, tallied.
- `mutants.py`, `mutants.txt`; `gates.sh`, `gate.txt`; `final-checks.txt`.
- `redacted.py`: how the logs here were redacted.
