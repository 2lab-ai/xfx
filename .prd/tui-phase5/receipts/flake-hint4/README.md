# flake-hint4: snap at a screen state, not at a second

The DIFF-WRAPb sweep found `p5-transcript-records/start` unresolved in 2 of 3
runs on its base and in 2 of 3 on its head, with no related code change
(that unit's `ctrlo-diff-wrap-b/records-reruns.txt`). The snap is taken 4 s
after launch. Both binaries run that scenario in full access, and the
full-access warning stays on the hint row for 4 s. The snap and the end of
the warning fall at the same moment.

This unit fixes it in the harness, for every timer a snap can race:

- a capture verb, `until shown <text>` / `until gone <text>`, that waits for
  the screen and fails the run if the screen never gets there;
- a check in the scenario loader that refuses a snap a second count puts
  inside a UI timer's window, and a keystroke that would stop the
  warning's clock inside it. The sweep refuses such a scenario before it
  runs anything, and `test-sweep.sh` checks every scenario in the
  repository, so the gate and CI both run it;
- 29 scenarios converted: the 20 the check refuses on the base,
  `permissions-btab` (a snap inside the warning's window after a Shift-Tab,
  which the check does not count), and 8 by hand for timers a .cap cannot
  see arm. Two of the 20 got a second, by-hand fix after a sweep caught
  them (`interrupt-recovered`, `interrupt-steer`). The code commit's message
  splits the 29 as "24 ... and 5"; the split here is the right one.

No product code changed. The code is `ba24235`; this receipt is the commit
after it, which adds only this directory. Line numbers are at `ba24235`
unless marked "base" (`24a6b912`). fx's are at `481104fd`. fx is the
v0.0.16 release binary (sha256 `8e6cda2e…`). xfx is `target/debug/xfx`
built at the commit each run names, and each banner says that commit:
`24a6b91` for the base, `ba24235` for the head sweeps. The GREEN reruns and
the second calibration ran on a build of `e2937ae`, an earlier amend of the
same commit: `git diff e2937ae ba24235` is empty for `src/`, `Cargo.lock`,
`build.rs`, `capture.sh` and the three scenarios they ran.

## Why `start` was a coin flip

- The warning's clock. xfx: `WARNING_VISIBLE` = 4000 ms
  (`src/tui/permissions.rs:38`), armed at launch in full access when the
  person has not seen it (`src/tui/shell.rs:2296-2302`). fx:
  `yolo_warning_visible_ms` = 4000 (`src/core/app/app_permission_runtime.zig:21`),
  armed by `initializeYoloWarning` (`:86-88`).
- The clock runs only while the row shows the warning. It starts at the
  first frame that has it (xfx `Warning::observe`, `permissions.rs:409-437`;
  fx `frameCommitted`/`tick`, `app_permission_runtime.zig:41-67`). It stops
  while a cue or a menu owns the row (xfx `warning_shown`,
  `shell.rs:13616-13641`; fx `dangerStatusText`,
  `src/ui/footer/input_presentation.zig:581-600`).
- Measured (`calib.txt`, `calib.cap`): two sets of twelve launches on each
  binary, at load 19.8 to 32.7 and 18.5 to 23.0. The warning was on the
  screen 0.02 to 0.14 s after launch and gone 4.04 to 4.12 s after launch
  (3.93 to 4.06 s after it came).
- The scenario (base `scripts/fxcap/scenarios/p5-transcript-records.cap:16-18`)
  is `wait 4`, then `snap start`. capture.sh adds its own overhead before
  the snap: one subshell per scenario line. So the snap lands a few tens of
  milliseconds either side of the warning's end, on each binary on its own.

## RED

- `red-records.txt` (`hint-runs.sh`, the base harness, scenario and binary,
  ten runs, load 11.9 to 36.5): fx's `start` shows the warning in 2 of 10,
  xfx's in 2 of 10, both in run 5. The pair disagrees in 2 of 10 (runs 8
  and 10).
- `ctrlc-base.txt`: the same count for Ctrl-C's exit hint, three runs of
  each scenario the check flags for it. A second race of the same kind:
  `p5-toolstop-retint` takes `dark` 3.0 s after a Ctrl-C, and the exit hint
  lasts 3 s. fx shows the hint in 2 of 3, xfx in 0 of 3; the pair agrees in
  1 of 3. Every other snap there is 3 of 3 or 0 of 3.
- `check-base.txt`: the new check over the base's scenarios. It refuses 71
  lines in 20 scenarios, `p5-transcript-records`' `snap start` among them
  ("4.0 to 4.0 s after the full-access warning is armed, inside its window
  (3 to 7 s)").

## The verb

`scripts/fxcap/capture.sh` (header, `until shown <text>` / `until gone <text>`):

- It reads the pane with `capture-pane -p`, the same screen a `snap`
  writes, every 50 ms. `<text>` matches as a fixed string anywhere on one
  row, not as a glob.
- It waits up to 30 s. If the screen never gets there, it fails the run
  (exit 1), prints the screen it last read (its rows that are not blank),
  and the guards still run. It never continues silently. This mirrors `held`.
- It prints how long it waited and how long since the binary was launched
  or last relaunched. The launch is stamped with `: > <file>` (no fork), so
  stamping it moves no scenario's timing.
- Before launch, an `until` without `shown`/`gone` and a text is refused
  with exit 2, as every verb's arguments are.
- The same on fx and xfx. A line can still be `fx: ` or `xfx: ` only, as
  any line can (`p5-transcript-cancel` waits for fx's warning only; its xfx
  runs in `auto`).

Tests, `scripts/fxcap/test_capture.py`: `Verbs.test_until_waits_for_a_row_to_come_and_then_to_go`
(a row with `[` and `*` in it that a stub shell puts up for 1.5 s),
`…_fails_the_run_when_the_row_never_comes`, `…_fails_the_run_when_the_row_never_goes`,
and four refusals in `Refusals.test_each_verbs_arguments`.

## The check

`scripts/fxcap/capfile.py`: `TIMERS`, `BEFORE`/`AFTER`, `SPENDS`, `races`.

- **The timer list, in one place.** A timer has a row that is on the screen
  while it runs, its seconds, the file:line of each binary's number, and
  what arms it in a way the .cap states for certain:
  - the full-access warning, 4 s. Armed at launch when the binary's `env`,
    or a file a `seed` or `homefile` line writes, sets full access, and again
    at each `relaunch`. Its text is `Full access`: both forms of the
    warning begin so (`permissions.rs:30,34`; fx
    `src/core/permissions/permissions.zig:90`,
    `app_permission_runtime.zig:20`), and the mode on the status line is
    `full access`, lower case;
  - Ctrl-C's exit hint, 3 s (`src/tui/gesture.rs:67`;
    `src/core/input/gesture_state.zig:3`). Armed by `key C-c`. Any other
    keystroke takes it down (`key`, `type`, `pastex`; fx keeps it over a
    paste frame).
- **The window**: from `BEFORE` = 1 s before the timer runs out to
  `AFTER` = 3 s after. The measured spread is under 0.1 s at load up to 33
  (above), and one loaded GREEN launch drew its first frame 1.28 s late
  (below). The window is wider than both on purpose: a CI runner is
  slower.
- **The clock**: for each binary's reading of the file, the seconds since
  arming are a range. A `wait` adds its seconds. `bells` and `wire` add
  their 0.3 s sleep. `held`, `signal` and an `until` on any other row make
  the upper end unbounded. `until shown <the timer's text>` resets the lower
  end to 0. `until gone <the timer's text>` disarms it.
- **Refused**: a `snap` or `snapfull` whose range meets the window. For the
  warning, also a Ctrl-C, an Escape or a typed `/` inside its window: each
  can take the row, the clock stops while the warning is off it, and the
  moment it goes moves by that much. Such a line before the window stops
  the lower bound from growing (below).
- **Enforced** in two places. `sweep.py` `start` refuses the selected
  scenarios before anything runs (exit 2, every line listed). `test_sweep.py`
  `Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer` checks every
  scenario in the repository; `test-sweep.sh` runs it, so the gate and CI's
  "The sweep's own tests" step do too.
- **Tests**: `Capfile.test_a_snap_inside_the_warnings_window_is_refused`.
  Its first case is the old `records.cap` shape. The others: before and
  after the window, another row's `until`, `held`, `until shown` then a
  second count, `wire`/`bells`, a `relaunch`, each binary's own `env`, fx's
  `full-access`. Also `…_line_that_takes_the_warnings_row_inside_its_window_is_refused`,
  `…_ctrl_c_arms_the_exit_hint_and_any_other_keystroke_takes_it_down`,
  `…_profile_in_full_access_arms_it_too`,
  `…_each_timer_cites_both_binaries_sources` (xfx's line holds the number)
  and `Sweep.test_a_snap_that_races_a_timer_runs_nothing`.

What the check does not see, and why that is safe or why it is left:

- **A relaunch in full access** is armed again, though the first launch
  already wrote `yolo_acknowledged` and the second shows no warning. This
  refuses more than it must, never less.
- **A key that arms the warning** (Shift-Tab, `/permissions full-access`) is
  not counted: the .cap cannot say which mode the key lands on, and the
  notice `/permissions` answers with holds the row until the next key. A
  scenario that means the warning takes its snaps after `until shown`, and
  from there the check counts (`permissions-btab`, below).
- **A line that takes the row before the window** (a menu opened at 1 s
  and closed) moves the warning's end by however long it held the row. The
  check counts that too: from such a line its lower bound stops growing
  until an `until` names the row
  (`Capfile.test_a_line_that_takes_the_warnings_row_before_its_window_stops_its_clock`,
  mutant M16). No scenario in the repository has one.

## Timers found, and which the check holds

| timer | xfx | fx `@481104fd` | in `TIMERS` |
|---|---|---|---|
| full-access warning, 4 s | `src/tui/permissions.rs:38` | `src/core/app/app_permission_runtime.zig:21` | yes, armed at launch |
| Ctrl-C exit hint, 3 s | `src/tui/gesture.rs:67` | `src/core/input/gesture_state.zig:3` | yes, armed by `key C-c` |
| Escape: clear 0.5 s, interrupt 1 s | `gesture.rs:70`, `:74` | `gesture_state.zig:4`, `:5` | no (1) |
| compaction's "No context to compact." and busy refusal, 1.5 s | `src/tui/compaction.rs:28` | `src/core/output/compaction_activity.zig:123` | no (2) |
| a retried request's recovered row, 1.5 s | `src/tui/retry.rs:71` | `src/core/output/worker_status.zig:5` | no (3) |
| activity marker blink, 0.5 s | `src/tui/activity.rs:35` | `src/core/output/activity_status.zig:124` | no (4) |
| MCP startup quiet, 10 s | `src/tui/statusline.rs:546` | `src/core/mcp_host/menu.zig:443` | no (5) |
| MCP sign-in news, 5 s | none (`statusline.rs:561`: sign-in is not built) | `menu.zig:436` | no (6) |

Each "no" is armed by something the .cap does not state:

1. An Escape arms only over a draft or running work. A rule armed by every
   Escape (a prototype of the check, window 0 to 4 s) flags 186 snaps,
   both binaries counted, in 60 scenarios. Most come after an Escape that
   closes a menu and arms nothing.
2. The transient comes only when `/compact` has nothing to compact or meets
   running work. Success, failure and cancel stay. A rule armed by every
   `/compact` flags 17 snaps on each binary in 7 scenarios. 4 of them
   follow a transient outcome; those 4 are converted by hand (below).
3. A request that the gateway answers after a retry arms it. The sweeps
   below caught `interrupt-recovered`'s `rec-0.5` and `rec-1.8` against it
   under load, so that scenario is converted by hand (below). The retry
   scenarios that show the backoff counting down (`interrupt-retry`,
   `p5-lifecycle-retry`, `p5-lifecycle-retryok`) snap the countdown at set
   moments by design: the seconds left are the row's text. xfx retries on
   its own schedule, 250 and 500 ms against fx's countdown from 1 s, and
   the ledger names that as a unit's for `interrupt-retry` (the `K-recover`
   cause, `scripts/fxcap/parity-ledger.toml:647-653`); the two
   `p5-lifecycle-retry*` scenarios sample the same row under the same
   schedule. When that unit ports fx's schedule, the countdown snaps are
   its to time. They are not converted here.
4. The ledger masks the marker cell (`scripts/fxcap/parity-ledger.toml:106`).
5. The status line keeps its MCP text back until no server is still
   starting, or 10 s (xfx `startup_settled`, `statusline.rs:663-673`; fx
   `startupSettled`, `menu.zig:445-458`): a server's state decides it. Five
   snaps sit 2 to 2.3 s past the 10 s on purpose, and their status line
   changes at it; they are converted by hand (below). The other MCP snaps
   near 10 s are menus, whose key-help row hides the status line.
6. fx only, for an MCP sign-in, which no scenario reaches.

## The scenarios

29, in eight groups. "Before" is the base's line that took the snap. No snap
changed what it shows; each now waits for the screen it was meant to show.

**The warning at launch** (the check). The turn or the snap now comes after
`until shown Full access`, the scenario's own wait, and
`until gone Full access`. So the warning is checked to have come up and then
gone.

| scenario | snaps inside the window (base) | what they show |
|---|---|---|
| p5-transcript-records | `start` 4.0 s | the launch screen before the turn: the RED |
| p5-transcript-resize | `start` 4.0 s | the same |
| p5-mcp-turn | `ready` 6.0 s | the launch screen once the fixture server is up |
| interrupt-cmd | `cmd-running` 4.5, `cmd-cc` 4.9, `cmd-cc-2` 6.4 s | the command, then Ctrl-C over it; the warning was incidental |
| interrupt-recovered | `rec-1.4` to `rec-3.2`, 3.4 to 5.2 s | the recovered row; its own timer is the group below |
| interrupt-steer | `steer-stream`, after `held`: 3.4 s to unbounded | the steer over a held stream |
| interrupt-steer-retract | `steer-wait`, `steer-up`, `steer-again`, 3.6 to 4.4 s | the steer taken back |
| p5-transcript-cancel | `mid`, after `held` (fx only) | the held stream; fx runs full access, xfx `auto` |

**A Ctrl-C inside the warning's window** (the check). `until gone Full access` before the
Ctrl-C. Otherwise the Ctrl-C's cue takes the row, the warning's clock stops,
and the warning comes back once the exit hint goes.

| scenario | before | |
|---|---|---|
| interrupt-next-cmd | `key C-c` 4.5 s after launch | |
| p5-toolstop-retint, -light, -tc, -light-tc | `key C-c` 5.5 s after launch | also below |

**Ctrl-C's exit hint, 3 s** (the check). `until shown press ctrl+c again to exit` where
the snap is the hint, and `until gone …` where it is the screen after it.

| scenario | snap (base: seconds after Ctrl-C) | what it shows |
|---|---|---|
| p5-toolstop-retint and its three twins | `dark` / `light` 3.0 s | the cancelled call before the theme switch: the second RED |
| interrupt | `idle-cc` 0.3, `cc-0.3`, `cc-1.5` / `idle-cc-expired` 3.8, `cc-3.5` | the hint up / gone (base: 3 of 3 / 0 of 3) |
| interrupt-help | `help-cc` 0.5 / `help-cc-expired` 4.0 | the same over `/help` |
| interrupt-cmd | `cmd-done` 5.9 | after the hint (base 0 of 3) |
| p5-mcp-live | `end` 3.5 | after the hint (base 0 of 3) |
| settings-collapse | `later` 5.5 | after the hint (base 0 of 3) |
| p5-effort-fork-effort-ctrlc | `ctrlc` 0.3, `ctrlc-later` 2.8 | the hint over the `/effort` menu, twice. `ctrlc-later` was 0.2 s from the hint's end (base 3 of 3 shown); it is now 1.8 s after the hint came. The snap still shows the hint over the menu, as it did |
| approvals | `after-cc-cmd-ask` 2.5 | after a Ctrl-C that answers a permission question |
| permissions-midturn | `after-refusal` 3.0 | the same |

At a permission question, Ctrl-C is the question's own deny and arms no
exit hint (fx `src/ui/input/runtime.zig:63`, `3 => .deny`; xfx
`shell.rs` `ctrl_c_at_a_question_answers_only_the_question_it_is_about`).
Base: 0 of 3 on both binaries. So in `approvals` and `permissions-midturn`
the `until gone` returns at once. It is there because the check cannot see
the question and counts every Ctrl-C's 3 s.

**Shift-Tab into full access** (the check). `permissions-btab`: `btab-full` (0.5 s after
the key: the warning up) follows `until shown Full access`;
`btab-full-later` (5.0 s: gone after its 4 s) follows `until gone Full access`.
The check does not arm the warning on a key: Shift-Tab is a menu's own key
in some scenarios and leaves full access in others. This one was converted
because its window holds the snap. Its `until shown` re-anchors the check's
clock, so dropping the `until gone` is refused (a test case covers that
shape).

**Compaction's transient, 1.5 s** (converted by hand; see (2)):

| scenario | snap (base: seconds after Enter) | now |
|---|---|---|
| compact | `empty-2.3s` 2.3, `busy-2.0s` 2.0 | `until gone` the sentence: the screen after it |
| p5-lifecycle-compactok | `c00-noop` and `wire w00-noop` 2.0 | `until gone No context to compact.` |
| p5-panels-sweep-a | `compact-empty` 1.5, the transient's own 1.5 s | `until shown No context to compact.`: the snap is `/compact`'s answer |

**MCP startup quiet, 10 s** (converted by hand; see (5)). Each snap now
follows `until shown <the status text>`, the text both binaries show there
(`mcpq.txt`, one base run of each).

| scenario | snap (base: seconds after launch) | text |
|---|---|---|
| p5-mcp-menu-new | `startup-settled` 12.0 | `proj is waiting for approval` (`remote`, 127.0.0.1:9, keeps retrying: the full 10 s) |
| p5-mcp-verbs | `show-fixture` 12.3 | the same |
| p5-mcp-config-vars | `startup` 12.0 | `MCP config has 5 problems` |
| p5-mcp-notes | `startup` 12.0 | `MCP config has 1 problem` |
| p5-mcp-config-ignored | `startup` 12.0 | fx only, `MCP config has 2 problems`: xfx says nothing of them, a difference that is not timing, so only fx waits |

**A retried request's recovered row, 1.5 s** (converted by hand; see (3)).
`interrupt-recovered`: the row `✓ recovered · attempt 2` comes when the
retry answers, and that answer is the command. `rec-0.5` now follows
`until shown recovered · attempt 2` and `until shown Running sleep 4`;
`rec-1.4` is 0.4 s after it, the row still up; `rec-1.8` follows
`until gone recovered · attempt 2`; `rec-2.4` and `rec-3.2` keep their waits
after it, inside the 4 s command. The labels stay: a label names a pair, and
the ledger and earlier receipts name these.

**A steered answer, settled** (by hand). `interrupt-steer`'s
`steer-stream` was 0.4 s after the steer's Enter. On xfx the steer runs as
the next turn, and under load its answer was sometimes still streaming
there (`Generating (4s) (↑4 ↓73)` beside fx's settled `3s (↑4 ↓73)`): in a
base sweep stopped half way, and in a head sweep of an earlier amend.
That is the stream's position, not a UI timer: the class FLAKE-STREAMMID
fixed with `held`. The snap now follows `until shown steered answer.` and
`until gone Generating (`: the screen it showed on every passing run.

## GREEN

The converted scenarios run again on the head (build of `e2937ae`; the
three scenarios, `capture.sh` and `src/` are the same at `ba24235`), under
the proof runs' added load: twelve busy loops beside the machine's own
work, load 19 to 43.

| file | scenario, runs | warning or hint in the snap, fx / xfx | pair agrees | RED |
|---|---|---|---|---|
| `green-records.txt` | p5-transcript-records, 10 | `start` 0 / 0 of 10 | 10 of 10 | 2 / 2 of 10, agrees 8 of 10 |
| `green-resize.txt` | p5-transcript-resize, 3 | `start` 0 / 0 of 3 | 3 of 3 | (the check, `check-base.txt`) |
| `green-toolstop.txt` | p5-toolstop-retint, 5 | `dark` 0 / 0 of 5 | 5 of 5 | 2 / 0 of 3, agrees 1 of 3 |

Each file ends with how long every `until` waited. In `green-records.txt`
one xfx launch put the warning up 1.28 s after launch, not the usual 0.04
to 0.11 s. Its `until gone` returned at 5.32 s, so the old `wait 4` /
`snap start` would have caught the warning there. The `until gone` lines
returned after one or two polls (0.01 to 0.04 s): by then the warning had
gone. When it has not, the verb waits for it: in `calib.txt` the
`until gone` right after `until shown` waited 3.93 to 4.06 s.

## Sweep, base against head

`sweep-compare.txt` (`pairs_compare.py`): the 29 converted scenarios, three
sweeps on each side, one ledger (`parity-ledger.toml`, unchanged here). Base
= `24a6b912`'s harness and scenarios with xfx built at `24a6b91`; head =
`ba24235`'s with xfx built at `ba24235`. Under the added load, peaks 28.7 to
46.3. Every sweep ran 29 of 29 scenarios, none failed or timed out.

- 236 pairs a side. **No pair went from passing to anything else** (drops:
  0). Pairs passing in every run: base 90, head 92.
- Two pairs changed, both from a coin flip to passing:
  `interrupt-recovered/rec-0.5` and `rec-1.8`, base unresolved 2 of 3, head
  passing 3 of 3. That is the recovered row's 1.5 s (above).
- No other pair's verdict moved in any of the six runs.
  `p5-transcript-records/start` passed in all six. Its race is the RED's
  2 of 10 (`red-records.txt`), which three sweeps need not hit. A sweep of
  an earlier amend did hit its twin: `p5-transcript-resize/start` was
  unresolved on the base and passing on the head (`sweep-compare.txt`,
  "earlier amends").
- The earlier amends' sweeps found the two by-hand conversions above:
  `rec-0.5` and `rec-1.8` dropping on `62edc9d`, and `steer-stream` on
  `e2937ae` (`steer-stream.txt`). Each was fixed at its source in the
  scenario, and the sweeps above are of the commit with those fixes.
- `until-waits.txt`: all 309 `until` lines those three head sweeps ran,
  per condition, with the longest wait. `until gone Full access` waited up
  to 2.03 s where a scenario's own wait was 2 s. `until gone recovered ·
  attempt 2` waited up to 1.05 s: there it replaced a second count.

## Mutants

`mutants.py`, on a `git clone --no-local` at `ba24235` (`GIT_DIR`,
`GIT_INDEX_FILE`, `GIT_WORK_TREE` unset), each a one-place change and then
the test named. `mutants.txt`: **16 of 16 killed**.

| mutant | killed by |
|---|---|
| M1 `records.cap` back to `wait 4` / `snap start` | `test_no_snap_of_the_repos_scenarios_races_a_timer` |
| M2 no window (`BEFORE` = `AFTER` = 0) | the window test, 9 cases |
| M3 an `until` on any row takes the timer down | the window test |
| M4 `held` counted as no time | the window test |
| M5 a relaunch does not arm it again | the window test |
| M6 a seeded profile not read | `…_profile_in_full_access_arms_it_too` |
| M7 fx's `full-access` spelling not matched | the window test |
| M8 other keystrokes leave the exit hint up | the Ctrl-C test, 3 cases |
| M9 the exit hint dropped from `TIMERS` | the Ctrl-C test |
| M10 the sweep does not refuse a race | `Sweep.test_a_snap_that_races_a_timer_runs_nothing` |
| M11 `until` gives up silently at 30 s | `test_until_fails_the_run_when_the_row_never_comes` |
| M12 `until gone` waits for shown | `test_until_waits_for_a_row_to_come_and_then_to_go` |
| M13 `until` matches its text as a glob | the same |
| M14 a Ctrl-C, Escape or menu inside the window not counted | `…_line_that_takes_the_warnings_row_inside_its_window_is_refused` |
| M15 `interrupt-next-cmd`'s Ctrl-C back inside the window | `test_no_snap_of_the_repos_scenarios_races_a_timer` |
| M16 a line that takes the row before the window stops nothing | `…_line_that_takes_the_warnings_row_before_its_window_stops_its_clock`, 3 cases |

## Gates

`gate.txt`, at `ba24235` with this directory untracked, one at a time:

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same, `--features fault-injection` | ok |
| `cargo test --locked` | 15 suites, 5157 passed, 0 failed (the second run; see below) |
| `cargo test --locked --features fault-injection` | 15 suites, 5194 passed, 0 failed |
| `scripts/check-*.sh` (6) | all ok |
| `scripts/fxcap/test-sweep.sh` | test_sweep 58 OK (8 new), test_ci_sweep 26 OK |
| `scripts/fxcap/test-capture.sh` | 45 OK (3 new, and 4 new cases in the refusals test) |

The first `cargo test --locked` ran at load 43.6, just after the proof
runs: one failure, `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land`
(`src/tui/copy.rs:248`), and cargo stopped there. That test's tool sleeps
0.2 s before it copies, and the session ends it after `GRACE` = 1 s
(`copy.rs:48`); at that load the second copy had not landed, and the file
still held the first one. That is the time-based class this unit
belongs to, in product test code this unit does not touch. The whole suite
again, alone, at load 11.5: 5157 passed, 0 failed.

## Files

- `hint-runs.sh`: runs a scenario n times on both binaries and counts a
  row in each snap. It produced `red-records.txt`, `ctrlc-base.txt` and the
  GREEN counts.
- `calib.cap`, `calib.txt`: when the warning comes and goes.
- `check-base.txt`: the check over the base's scenarios, and over the
  head's (no race in its 307).
- `mcpq.txt`: the MCP status lines, one base run.
- `mutants.py`, `mutants.txt`.
- `pairs_compare.py`, `sweep-compare.txt`: the sweeps.
- `steer-stream.txt`: `interrupt-steer`'s `steer-stream` in every sweep.
- `until-waits.txt`: the head sweeps' `until` lines.
- `green-*.txt`: the GREEN reruns.
- `gate.txt`.
