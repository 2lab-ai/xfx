# flake-hint4: snap at a screen state, not at a second

The DIFF-WRAPb sweep found `p5-transcript-records/start` unresolved in 2 of 3
runs on its base and in 2 of 3 on its head, with no related code change
(that unit's `ctrlo-diff-wrap-b/records-reruns.txt`). The snap is taken 4 s
after launch. Both binaries run that scenario in full access, and the
full-access warning stays on the hint row for 4 s. The snap and the end of
the warning fall at the same moment.

This unit fixes it in the harness. It does not fix every timer a snap can
race (corrected in round 2: round 1 said it did). A check now holds the two
timers whose arming a .cap states, the full-access warning and Ctrl-C's
exit hint (`TIMERS`). For three other timers (compaction's transient, a
retried request's recovered row, the MCP startup quiet), the snaps found
racing them were converted by hand, and nothing stops a new snap racing
them (the table under "Timers found"). One such snap is left: `p5-mcp-menu-old`'s
`prompts` sits on the MCP startup quiet's 10 s and is MCP-HOLD's (reason 5,
below). The unit adds:

- a capture verb, `until shown <text>` / `until gone <text>`, that waits for
  the screen and fails the run if the screen never gets there; and, in
  round 2, `absent <text>` (fails the run if the text is on the screen now)
  and `expect <label> <text>` (fails it if a snap already written does not
  hold the text);
- a check in the scenario loader that refuses a snap a second count puts
  inside the window of one of those two timers, and a keystroke that would
  stop the warning's clock inside it. The sweep refuses such a scenario before it
  runs anything, and `test-sweep.sh` checks every scenario in the
  repository, so the gate and CI both run it;
- 29 scenarios converted: the 20 the check refuses on the base,
  `permissions-btab` (a snap inside the warning's window after a Shift-Tab,
  which the check does not count), and 8 by hand for timers a .cap cannot
  see arm. Two of the 20 got a second, by-hand fix after a sweep caught
  them (`interrupt-recovered`, `interrupt-steer`). The code commit's message
  splits the 29 as "24 ... and 5"; the split here is the right one.
  Round 2 turned two of them (`approvals`, `permissions-midturn`) back into
  a look at the moment the base took, with `absent`, and added
  `until shown Full access` and `expect` lines (section "Round 2").

Everything up to "Round 2" is round 1 as delivered, corrected in place
where review found it wrong; each such place says "corrected in round 2".

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
  8 of the 12 scenarios the check flags for it (corrected in round 2: the
  file's header says "each", but `interrupt-help` and three toolstop twins,
  `-light`, `-tc` and `-light-tc`, were not run; `check-base.txt:20-21,51-61`
  flags them). A second race of the same kind:
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
  Corrected in round 2: round 1's 30 s was 600 looks 50 ms apart, which
  runs longer under load; it is now 30 s of elapsed time (bash `SECONDS`),
  and the failure says how long it waited and how many looks it took.
- It prints how long it waited and how long since the binary was launched
  or last relaunched. The launch is stamped with `: > <file>` (no fork), so
  stamping it moves no scenario's timing. From round 2, an `until gone`
  whose text no `until` of the run has seen on the screen adds
  `; never seen`: it shows nothing came and went.
- Round 2 adds two checks of the same kind. `absent <text>` fails the run,
  with the screen, if the text is on it now: a row that must not be there
  at all, checked at the moment rather than waited away. `expect <label>
  <text>` fails the run unless the snap `<label>`, already written, holds
  the text on one of its rows: the frame itself shows the row the snap is
  for.
- Before launch, an `until` without `shown`/`gone` and a text is refused
  with exit 2, as every verb's arguments are.
- The same on fx and xfx. A line can still be `fx: ` or `xfx: ` only, as
  any line can (`p5-transcript-cancel` waits for fx's warning only; its xfx
  runs in `auto`).

Tests, `scripts/fxcap/test_capture.py`: `Verbs.test_until_waits_for_a_row_to_come_and_then_to_go`
(a row with `[` and `*` in it that a stub shell puts up for 1.5 s),
`…_fails_the_run_when_the_row_never_comes`, `…_fails_the_run_when_the_row_never_goes`,
and four refusals in `Refusals.test_each_verbs_arguments`. Round 2's:
`Verbs.test_until_gone_says_never_seen_when_its_text_was_not_there_at_all`,
`Verbs.test_absent_passes_a_screen_without_its_text_and_fails_one_with_it`
(a stub puts up the exit hint where none may be),
`Verbs.test_expect_checks_the_frame_a_snap_wrote`, and the refusals of
`absent` and `expect`.

## The check

`scripts/fxcap/capfile.py`: `TIMERS`, `BEFORE`/`AFTER`, `SPENDS`, `races`.

- **The timer list, in one place.** A timer has a row that is on the screen
  while it runs, its seconds, the file:line of each binary's number, and
  what arms it in a way the .cap states for certain:
  - the full-access warning, 4 s. Armed at launch when the binary's `env`,
    or a file a `seed` or `homefile` line writes (round 2: or a `reseed` or
    `rehomefile` line), sets full access, and again at each `relaunch`. Its text is `Full access`: both forms of the
    warning begin so (`permissions.rs:30,34`; fx
    `src/core/permissions/permissions.zig:90`,
    `app_permission_runtime.zig:20`), and the mode on the status line is
    `full access`, lower case;
  - Ctrl-C's exit hint, 3 s (`src/tui/gesture.rs:67`;
    `src/core/input/gesture_state.zig:3`). Armed by `key C-c`. Any other
    keystroke takes it down: a `key` or a `type` line. A paste does not, and
    a terminal report does not (`paste`, `pastex`, `key -H …`): xfx's
    `consume` lets them through (`src/tui/shell.rs:10983-10987` at
    `1e22c660`, `:11058-11062` after the merge), and fx keeps the hint over a
    paste frame. Corrected in round 2: round 1 counted `pastex` and
    `key -H` as taking it down, and a test pinned that.
  - An `until` or `absent` line speaks of a timer only with exactly its
    text (round 2). Round 1 took any text that held it, so
    `until gone Full access never-rendered`, true while the warning is up,
    discharged the warning.
- **The window**: from `BEFORE` = 1 s before the timer runs out to
  `AFTER` = 3 s after. The measured spread is under 0.1 s at load up to 33
  (above), and one loaded GREEN launch drew its first frame 1.28 s late
  (below). The window is wider than both on purpose: a CI runner is
  slower.
- **The clock**: for each binary's reading of the file, the seconds since
  arming are a range. A `wait` adds its seconds. `bells` and `wire` add
  their 0.3 s sleep. `held`, `signal` and an `until` on any other row make
  the upper end unbounded. `until shown <the timer's text>` resets the lower
  end to 0; when nothing the check counts armed the timer, it sets the
  upper end to the time since the last input line (round 2: round 1 set it
  to 0). `until gone <the timer's text>` and `absent <the timer's text>`
  disarm it, but not while a line before the window has paused it (round 2:
  gone then is hidden, not spent), and an `absent` only once the row could
  have been drawn: the timer's `settle`, 1 s after the Ctrl-C for the hint,
  3 s after launch for the warning (round 2).
- **Refused**: a `snap` or `snapfull` whose range meets the window. For the
  warning, also a Ctrl-C, an Escape, a Ctrl-O or a typed `/` inside its
  window: each can take the row, the clock stops while the warning is off
  it, and the moment it goes moves by that much. Such a line before the
  window stops the lower bound from growing (below). Corrected in round 2:
  round 1 left out Ctrl-O, which opens the full transcript. xfx gives the
  screen to it (`shell.rs:4775`), and `warning_shown` answers nothing then
  (`:13634-13636` at `1e22c660`, `:13718-13720` after the merge). fx's
  `yolo_warning_visible` is false while the alternate screen renders
  (`src/core/app/app_render_runtime.zig:1849-1850`). Both clocks pause on
  such a frame (`permissions.rs:409-437`;
  `app_permission_runtime.zig:41-47`). These are the lines known to take
  the row. A line that does and is not in the list goes uncounted.
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
  from there the check counts (`permissions-btab`, below). Round 2: that
  `until shown` keeps the upper end at the time since the last input line,
  so after an uncounted arm and a wait (review's
  `key BTab / wait 3.5 / until shown Full access / snap`) the snap is still
  counted.
- **A line that takes the row before the window** (a menu opened at 1 s
  and closed) moves the warning's end by however long it held the row. The
  check counts that too: from such a line its lower bound stops growing
  until an `until shown` names the row
  (`Capfile.test_a_line_that_takes_the_warnings_row_before_its_window_stops_its_clock`,
  mutant M16). No scenario in the repository has one. Round 2: an
  `until gone` there no longer disarms it either, since the row may only be
  hidden.

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
   near 10 s are menus, whose key-help row hides the status line. That is
   not the whole of it (corrected in round 2): a menu's body can change at
   the same moment. `p5-mcp-menu-old`'s `prompts` is 10.5 s after launch
   by the .cap's waits, and review saw its body change with the 10 s probe.
   `mcpq.txt` read only its key-help row (`mcpq.txt:22-23`). It is not
   converted here; it is left to MCP-HOLD.
6. fx only, for an MCP sign-in, which no scenario reaches.

Review's objection, recorded (round 2): for (3) and (5) the .cap does state
what arms them, in the gateway turns that answer after a retry and in the
profile that names a server that keeps retrying. They are left out because
the check would have to read those files to know. That is the generic
settle guard review suggests; it is not built here.

## The scenarios

29, in eight groups. "Before" is the base's line that took the snap. Each
snap now waits for the screen it was meant to show. These changed what they
show, on purpose (corrected in round 2: round 1 said none did):

- `p5-panels-sweep-a/compact-empty` was a coin flip on the transient's own
  1.5 s; it now always shows the transient.
- `p5-effort-fork-effort-ctrlc/ctrlc-later` was 2.8 s after the Ctrl-C; it
  is now about 1.8 s after the hint came up. It still shows the hint over
  the menu.
- `interrupt-recovered/rec-0.5` now also waits for `Running sleep 4`.
- Five scenarios start their turn about 2 s later, after the warning has
  gone: `interrupt-cmd`, `interrupt-recovered`, `interrupt-steer`,
  `interrupt-steer-retract`, `p5-mcp-turn`.

Review found no ledger entry or receipt that names those moments.

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

**A Ctrl-C inside the warning's window** (the check). `until shown Full access`, then
`until gone Full access`, before the Ctrl-C (round 2 added the `until shown`:
a bare `until gone` returns at once if the warning is not up yet, so a
launch that drew late would have let the Ctrl-C in). Otherwise the Ctrl-C's cue takes the row, the warning's clock stops,
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
Base: 0 of 3 on both binaries. Round 1 put `until gone` before these two
snaps. That returns at once when no hint is up, but it would also wait away
a hint armed by mistake, which the base's snap could have shown. Round 2
replaced it: `absent press ctrl+c again to exit` halfway, then the snap at
the base's moment. `absent` fails the run if the hint is up, and the check
reads it as "this Ctrl-C armed nothing" (section "Round 2").

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

## GREEN (round 1)

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

## Sweep, base against head (round 1)

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

## Mutants (round 1)

Round 2's run replaced `mutants.py` and `mutants.txt` (section "Round 2");
round 1's are in git at `9f968a65`. `mutants.py`, on a `git clone --no-local` at `ba24235` (`GIT_DIR`,
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
| M16 a line that takes the row before the window stops nothing | `…_line_that_takes_the_warnings_row_before_its_window_stops_its_clock`, 2 cases (corrected in round 2: round 1's `mutants.txt` says `failures=2`) |

## Gates (round 1)

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

## Round 2

Two reviews of round 1 (`9f968a65`): one REJECT with two MUST items, one
APPROVE-WITH-FIXES with three (one of them this receipt). This section
lists each item, what changed, and what shows it. It also covers one more
sighting of row 19 that the coordinator added to this round: `models2`
(item 12).

The commits, in order:

| commit | what |
|---|---|
| `1e22c660` | the code: exact texts, `absent`, Ctrl-O, pastes, `expect`, the scenarios |
| `8c2e7def` | merges main `6c93f359` (SECRETS-ARGMAX), no conflicts |
| `b0522162` | a comment in `capfile.py`: xfx `shell.rs` lines moved by that merge, and fx's `yolo_warning_visible` line, which round 2 had miscited |
| `bf794f0d` | `until gone`'s `never seen` counts every `until` of the run (item 6) |
| `d40bf1a3` | `models2` by order, not 5 s from the gateway's start (item 12) |
| `32932ad5` | this receipt, first cut |
| `8a995bea` | an `absent` counts only once its row could be drawn (item 2) |
| the next | this receipt, brought up to `8a995bea` |

No product code changed in round 2 either: `git diff 6c93f359 8a995bea --
src tests Cargo.toml Cargo.lock build.rs` is empty.

xfx line numbers in this section are at `1e22c660` unless marked.

### What changed

`r2/red-lint.txt` is round 2's `Capfile` tests run against round 1's
`capfile.py`. Its lines are the failures; the RED column cites them.

| # | review item | change | RED on round 1 | mutant |
|---|---|---|---|---|
| 1 | an `until` with any text that held the timer's text discharged it | `races` counts an `until` or `absent` line only with the timer's exact text | lines 8, 9, 12, 13 (`until gone Full access never-rendered`, `… enabled`, `…exit, please`, `until shown Full access enabled`) | N1 |
| 2 | `approvals`' `until gone` would wait away a hint armed by mistake | new verb `absent <text>`: fails the run if the text is on the screen now. `approvals` and `permissions-midturn` look halfway and snap at the base's moment. `races` reads `absent <exact text>` as "this armed nothing", but only from the timer's `settle` on (`8a995bea`): 1 s after the Ctrl-C for the hint, 3 s after launch for the warning. Sooner, the row may not be drawn yet, and the `absent` would pass while it asserted nothing | lines 7, 10, 11; `r2/red-settle.txt` | N2, N10, N11, N18 |
| 3 | Ctrl-O not counted as taking the warning's row | `C-o` in the warning's `pauses`; the test's `key C-o` now refused | line 4 | N3 |
| 4 | `pastex` and `key -H` counted as taking the exit hint down | only a `key` line that is not `key -H` and a `type` line disarm; the test's cases flipped | lines 1, 2 | N4 |
| 5 | receipt claims the evidence did not back | corrected in place above, each marked "corrected in round 2" | | |
| 6 | a bare `until gone` whose text never came passed silently | `until gone` says `; never seen` when no `until` of the run saw its text; `until shown Full access` before the five bare `until gone Full access` (`interrupt-next-cmd`, the four toolstop scenarios) | (capture.sh) | N13, N15 |
| 7 | `until gone` disarmed while the row was paused; an `until shown` after an uncounted arm put hi at 0; `reseed`/`rehomefile` not read | not while paused; hi = the time since the last input line; both read | lines 3, 5, 6, 14 | N5, N6, N7 |
| 8 | no test told `SPENDS["until"]` or `signal` apart | `test_an_until_on_another_row_and_a_signal_take_as_long_as_they_take` | passes on round 1, which had both right; the mutants show it tells them apart | N8, N9 |
| 9 | "30 s" was 600 looks | `until` gives up after 30 s of elapsed time (bash `SECONDS`, no fork per look); its failure says how long it waited and how many looks it took | (capture.sh) | N14 |
| 10 | the Normalize fixture's text changed with no reason | back to `Full access enabled: …` | | |
| 11 | the recovered row's "up" snaps held no check on the frame | new verb `expect <label> <text>`: fails the run unless the snap already written holds the text. After `rec-0.5` (`recovered · attempt 2`, `Running sleep 4`) and `rec-1.4` (`recovered · attempt 2`), and after each snap meant to show a timer's row: `btab-full`, `idle-cc`, `cc-0.3`, `cc-1.5`, `help-cc`, `ctrlc`, `ctrlc-later`, `compact-empty` | (capture.sh) | N12 |
| 12 | `Verbs.test_models2_and_modelsdelay` failed under load: the second catalog came a fixed 5 s after the gateway started, and the first ask came 9 s after (the MCP-E worker's gate, 4 of 4) | `models2 <catalog.json>` is a scenario line: from it on, the models path answers the catalog. capture.sh renames it into place, as `usage` does; fakegw serves it while the file exists. The old `<ms>` form is refused before launch. The test waits for each answer on the screen; the fakegw test puts the file in place between asks; `h1-models2` switches between its two `/model`s | `r2/models2.txt` | N16, N17 |

`absent` and `expect` are checked before launch like every verb's
arguments (`absent takes <text>`, `expect takes <label> <text>`).

The models2 evidence (`r2/models2.txt`): the old test failed 3 of 25 runs
here at the machine's own load (load 6 to 11, no added load), its first ask
8.6 and 12.4 s after the gateway started; in 20 rounds alternating with it at
the same load, the new test passed 20 of 20 (the old failed 1 of those 20).
Twelve added busy loops did not reproduce it in 4 runs (first asks 1.4 to
2.7 s; stopped right after), so the mechanism is also shown by hand: with
`wait 5.5` before the first ask, the old harness answered it with the
second catalog 3 of 3, the new one with the first 3 of 3. `h1-models2`,
base (the fixed 9 s) 4 runs against head 3: no pair moved
(`r2/sweep-compare.txt`). One head run was loaded: the launch's asks came at
9.9 s (xfx) and 14.3 s (fx) and got the first catalog, where the fixed 9 s
would have answered them with the second.

### The injected hint (item 2)

`r2/inject.patch` puts a defect into xfx at `1e22c660`: a Ctrl-C that a
question answers also arms the exit hint. `r2/inject-runs.txt`:

- `permissions-midturn`, round 2: fails at `absent` in 4 of 4 runs, the
  hint on the screen.
- The same scenario, round 1: its `until gone` returned at once (the hint
  was not up yet), and its snap had no hint.
- The same scenario, base: the snap at 3 s had no hint in 5 of 5. The hint
  lasts 3 s from the Ctrl-C, so it had gone by then. The base's snap could
  not catch this defect either.
- `approvals`, round 2: passes. In xfx, the command's question that follows
  owns the hint row, so a hint armed by mistake is not on the screen at
  either look. The base's snap could not see it there either. This is a
  limit: `permissions-midturn` is the scenario that pins the defect.

### The check over the repository

`r2/check-head.txt`: at `8a995bea`, 0 races over 307 scenarios.
`test-sweep.sh` runs the same check in the gate.

### Sweeps, base against head

`r2/sweep-compare.txt` (`pairs_compare.py`): the 13 scenarios round 2
touched. Base = main `6c93f359`'s harness and scenarios with xfx built at
`6c93f35`; head = the branch's with xfx built at `8c2e7de` (no later commit
of the branch changes `src/`). Two base sweeps and four head sweeps, two at
`b0522162` and two at `bf794f0d`; `d40bf1a3` then changed `capture.sh` only
in `models2`'s lines, which none of the 13 uses, and `8a995bea` changed
only the check, which runs before a sweep does. Load 6.9 to 25.6.

- **No pair went from passing to anything else** (drops: 0) in any of the
  comparisons: base 1-2 against heads 3-4, against heads 1-2, and base 2
  against heads 2-4.
- Pairs passing in every run: base 24 of 93, heads 3-4 29. The five more are
  `interrupt-recovered/rec-0.5` and `rec-1.8` (round 1's fix: base
  unresolved in 1 and 2 of its 2 runs, head passing in all) and
  `interrupt`'s three passing pairs, which base-1 lost (below).
- `interrupt` failed on xfx in base-1 and head-1 before its first snap
  (section below). In base-2 and every later head it ran, and its pairs
  are the same in all of them.
- `r2/until-waits.txt`: every `until`, `absent` and `expect` line of the
  head sweeps. At `bf794f0d`, `never seen` appears only on
  `until gone press ctrl+c again to exit` in the four toolstop scenarios:
  8 of 8 xfx runs and 5 of 8 fx runs. That Ctrl-C stops a running tool.
  xfx never had the exit hint up there; fx had it in 3 of 8 runs, much as
  on the base (`ctrlc-base.txt`: fx 2 of 3, xfx 0 of 3). The snaps wait it
  away and compare the screen after. Whether that Ctrl-C should arm the
  hint is a parity question this unit does not answer. At `b0522162` the
  same line was on 48 of 54 `until gone`s, because `never seen` then
  counted only the `until gone`'s own looks; `bf794f0d` changed that.

### Mutants

`mutants.py`, on a `git clone --no-local` at `8a995bea` (`GIT_DIR`,
`GIT_INDEX_FILE`, `GIT_WORK_TREE` unset). Round 1's 16 (M), updated where
round 2 moved their text, and round 2's 18 (N1 to N18), each a one-place
change and the test it names. `mutants.txt`: **34 of 34 killed**. N5 (a
paused row's `until gone` counted) is killed by the paused-row test: at
`8a995bea` an `absent` of a paused warning is also short of its `settle`,
so the `absent` test no longer tells N5 apart (a first run with that test
had N5 survive; it was retargeted, not dropped).

### Gates

`r2/gate.txt`, at `8a995bea` with this directory's changes not yet
committed, one at a time (load 6 to 9). The same list passed at `d40bf1a3`
with the same counts (that log is `r2/gate.txt` in git at `32932ad5`).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| the same, `--features fault-injection` | ok |
| `cargo test --locked` | 15 suites, 5164 passed, 0 failed (7 ignored) |
| `cargo test --locked --features fault-injection` | 15 suites, 5201 passed, 0 failed (7 ignored) |
| `scripts/check-*.sh` (6) | all ok |
| `scripts/test-*.sh` (3: `check-no-secrets`, `gate-env`, `preview-contract`) | all ok |
| `scripts/fxcap/test-sweep.sh` | test_sweep 62 OK, test_ci_sweep 26 OK |
| `scripts/fxcap/test-capture.sh` | 48 OK |

### xfx's `interrupt` in the first sweep of each side

`r2/launch-probe.txt`. In base-1 and head-1, xfx's `interrupt` failed
before its first snap: the pane was gone. base-2 and head-2 ran it. The
cause, from the probes:

- The first launch of a freshly copied xfx binary drew its first frame
  2.13 s after launch; the next two launches of that copy, 0.10 s.
- xfx takes ISIG out of the terminal's modes when it goes raw
  (`src/tui/term.rs:157`). Before that, a Ctrl-C byte is SIGINT. A Ctrl-C
  0.5 s after the first launch of a fresh copy left `Pane is dead (signal
  int)`; on that copy's second launch, xfx was up.
- `interrupt.cap`'s first key is `key C-c`, 2 s after launch, and the sweep
  runs the longest scenario first. So in each side's first sweep, it was the
  first launch of the binary copied for the sweep.

This is not a drop: both sides failed it the same way, in the same run, and
this unit changes nothing before that Ctrl-C (round 2's `until shown` comes
after it). It is row 19's class, a key at a second count racing the screen,
here the launch. It is not fixed here: the fix belongs in `capture.sh` for
every scenario (wait for the binary's first frame, or for the terminal to
leave ISIG, before a scenario's first line). That moves every scenario's
timing and needs its own sweep.

### Not closed by this unit

Repeated-failures row 19 is the class of tests and captures that hang on
a time or a moment and break under load. This unit closes it for snaps
racing the two timers in `TIMERS`, and for `models2`'s fixed moment. These
remain, each named by review or found here:

- `p5-mcp-menu-old/prompts`, on the MCP startup quiet's 10 s: MCP-HOLD.
- `interrupt`'s Escapes: `esc1` is snapped 0.3 s after the first Escape,
  and the second Escape comes 0.6 s after it, inside the 1 s Escape
  interrupt window (`gesture.rs:74`). The same in every sweep here; the
  check does not hold Escape (reason 1).
- A scenario's first key racing the binary's launch (above): not a UI
  timer, and every scenario's.
- `interrupt-recovered`'s `rec-2.4` and `rec-3.2` keep their waits inside
  the 4 s command. `rec-1.4` now has an `expect`.
- The ledger's time-based `nondeterminism` causes that CI retries
  (`parity-ledger.toml:929-948`, `session-age` and `catalog-window`).

## Files

- `hint-runs.sh`: runs a scenario n times on both binaries and counts a
  row in each snap. It produced `red-records.txt`, `ctrlc-base.txt` and the
  GREEN counts.
- `calib.cap`, `calib.txt`: when the warning comes and goes.
- `check-base.txt`: the check over the base's scenarios, and over the
  head's (no race in its 307).
- `mcpq.txt`: the MCP status lines, one base run.
- `mutants.py`, `mutants.txt`: round 2's run (round 1's are in git at
  `9f968a65`).
- `pairs_compare.py`, `sweep-compare.txt`: the sweeps.
- `steer-stream.txt`: `interrupt-steer`'s `steer-stream` in every sweep.
- `until-waits.txt`: the head sweeps' `until` lines.
- `green-*.txt`: the GREEN reruns.
- `gate.txt`: round 1's gate.
- `r2/`: round 2. `red-lint.txt`, `red-neverseen.txt` and
  `red-settle.txt` (RED), `inject.patch` and `inject-runs.txt` (the injected
  hint), `models2.txt` (item 12's RED and GREEN), `check-head.txt` (the
  check over the repository), `sweep-compare.txt` and `until-waits.txt` (the
  sweeps), `launch-probe.txt` (`interrupt`'s first runs), `gate.txt`.
