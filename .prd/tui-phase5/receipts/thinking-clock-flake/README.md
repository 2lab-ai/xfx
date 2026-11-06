# thinking-clock-flake: `the_thinking_clock_is_frozen_while_the_panel_is_up`

This pty test failed at random on macOS CI with
`panicked at tests/tui.rs:5833:5: the turn never had a row, so its absence
below proves nothing`. That happened in run 42161589866 (PR #34, the
`--features fault-injection --test tui` step) and in run 41807697899
(PR #37). The cause is in the test. The fix changes `tests/tui.rs` and
`tests/support/fake_gateway.rs` only. Line numbers are at `main` @ `519ac87f`.

## The two facts that disagreed

- **The test** typed its prompt and waited for the panel title. It then
  required the activity row (`\d+s`) to appear somewhere in the bytes
  before the title (`tests/tui.rs:5831-5836`). In other words, it
  required a frame painted between the turn's start and its question.
- **The product** does not order those two. The worker sends `TurnStarted`
  (`src/tui/worker.rs:3416`) and goes on to the edit's approval without
  waiting for the UI. Each turn of the UI loop applies up to `UI_EVENTS`
  (256) queued events (`src/tui/event_loop.rs:282`, `:327`, `:348`) before
  it settles the band (`:288`, then `src/tui/shell.rs:8697`). The row is
  computed there (`tick_activity`, `shell.rs:9218`), and it is `None`
  while a question is up (`shell.rs:9290`).
- So suppose the UI thread is off the CPU from the moment the turn starts
  until it asks. The UI then takes `TurnStarted` and `Approval` in one
  batch, and the settle sees the question already up. The panel is the
  first frame either event is painted in. Coalescing events into one
  frame is the loop's contract (`event_loop.rs:318-325`), and keeping the
  row off the band while a question is up is what this test exists to
  prove. Neither one is a product bug.
- The window is short but not tiny. Across 72 loaded local runs, the time
  from the row's first paint to the panel was 83 ms at the least, 760 ms
  at the median and 910 ms at the most. Most of it is the fixture's own
  pacing. A 3-vCPU runner whose UI thread misses that interval fails the
  test.

## Fix

- `fake_gateway::Reply::Held` (and `Reply::held`) is a scripted reply that
  is written only once a flag is set, the fixture is dropped, or
  `HANG_TIMEOUT` passes. Its wait is the one the title hold already used,
  now shared as `wait_for_release`.
- The test holds the turn's first request until `Thinking (Ns)` is on the
  terminal, then releases it. The row now has a frame of its own before
  the turn can ask. If the product ever stops painting the row, the test
  times out on that wait instead of passing.
- **The next break, found by looking past the first one:** the absence
  check read only the bytes after the title. A frame is written top to
  bottom, and only its changed cells are sent. So a row that the
  question's own frame left above the panel lands before the title in the
  stream, and no later frame writes it again. The test now also reads the
  screen (`Screen::painted`) after the key frame.

## RED and GREEN

`loop.py` (here) runs one test N times, J at a time. Each run gets a
wall-clock limit and its own process group, which is killed at the end.

**Natural load did not reproduce it on this 18-core machine.** All of these
ran `519ac87f`:

| how it was run | failed |
|---|---|
| unloaded, 4 at a time | 0/20 |
| 18 CPU burners, 18 at a time | 0/60 |
| 72 CPU burners, 18 at a time | 0/72 |
| pty reads 20 ms late / 64 bytes per 5 ms (scratch harness shaping) | 0/10, 0/10 |
| the test typing 400 ms late (scratch harness shaping) | 0/6 |
| `XFX_TUI_FAULT=slow-ui` (40 ms per tick, shorter than the window) | 0/10 |
| the whole fault-injection tui suite, 18 threads | 0/3 (another test, `a_stop_and_resume_at_a_question_puts_the_review_back_on_a_plane_of_its_own`, failed 2/3 under this runner's process-group spawn; it passes under `cargo test`, 177/177 in the gate) |

**The fault model.** `ui-stall.patch` (here, never committed) sleeps the UI
thread for `XFX_SCRATCH_UI_STALL_MS` right after it takes `TurnStarted`.
That is the CI condition: the UI thread off the CPU while the turn runs.
One xfx build with this patch served every row below, and the product is
identical on both sides.

| test | stall | result |
|---|---|---|
| `519ac87f` | none (control) | 0/10 failed |
| `519ac87f` | 150 ms | 0/20 failed |
| `519ac87f` | 1000 ms | **20/20 failed**, CI's exact message at `tests/tui.rs:5833:5` (twice) |
| this branch | 1000 ms | 0/100 failed |
| this branch | none | 0/100 failed |

**Mutation.** A scratch build with the `shell.rs:9290` guard turned off
keeps `• Running (0s)` on row 8 beside the panel. The old byte check passed
it 3/3. This branch fails it 3/3, with `a turn row is on the screen beside
the question`.
