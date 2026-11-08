# P5 STOP-LATE-EVENT receipt: a stopped turn's late end, a stop-window prompt, a reset's failed compaction

**S** (terminal escapes: the OSC 7501 wire). Spec: the round-5 opus review of L-U1+U2 (zbrain
`p5/L-U12/reviews/r5-opus.md`, "Findings": SHOULD 1-3 and the NITs) and L-U1+U2's scratch repro
`../osc-program-status/system-leak.txt`.

- **Base:** `feat/p5-osc-program-status` `e636db48` (L-U1+U2's closed head; this unit changes the
  code that unit added, which is not on main yet).
- **Code:** `e27f9bcf`, the commit every `xfx/` capture's banner names (`v0.1.0-e27f9bc`) and the
  gate ran at. Commits on the base:
  - `220f9361` **RED**, tests only (`red-test.txt`);
  - `c2493f8b` item 1, the stop window;
  - `372e4236` item 2, a prompt sent while a started turn is in hand;
  - `73fb9220` item 3, `/new` and `compaction_failed`;
  - `2a4e89e5` docs: the OSC 7501 parity row, the comments, the `785-788` mis-cite, L-U1+U2's
    README bullet marked corrected;
  - `c389ba9d` the scenario `p5-lifecycle-osc-stopnext`;
  - `e27f9bcf` docs: fx's submission mechanism behind the capture, in the hold's comment and the
    parity row (comments only);
  - the receipt commit after it adds this directory only.
- **One brief item was reversed by the oracle.** Item 2 was a "fix to verify": hold the report of
  a prompt whose turn starts in the drain its predecessor ends in, so `working` follows the
  `Thinking` frame. The fx v0.0.16 capture of that window (`fx/`, three runs) shows fx writes
  `working` **ahead** of the frame that first shows such a prompt's turn. The same capture shows
  L-U1+U2e's round-5 hold for a prompt typed after a stop rests on the same false premise (that
  fx's keystroke frame shows the prompt's activity there). This unit's first build, `997d7b1f`,
  did what the brief said; its captures are `prev/` and it fails the comparison (below). It is on
  no branch of the unit, so it is pinned by the local branch `prev/stop-late-event-r1` (not
  pushed). The code
  under this receipt goes the other way: such a prompt is reported ahead of the frame that shows
  its turn, however the runtime's two events split across ticks.

## The three items, each as a chain

### 1. The stop window (opus SHOULD 1)
The happy path: a user's stop is one `idle` on the wire and one `■ Cancelled` line in the
document, and the stopped turn's end, whenever it is read, adds neither.
- `cancel_turn_with` sets `cancelling` (`src/tui/shell.rs:12311` at `e27f9bcf`) and, with no call
  running, ends the row.
- The runtime sends the stopped turn's `TurnEnded { failure: Some("…the turn was cancelled") }`
  (`src/tui/worker.rs:3835`; the text, `src/tui/bridge.rs:1658`) and only then gives the place
  back (`worker.rs:3524`).
- **Where it broke:** the band's settle took the cancel once no place was held (`shell.rs` settle,
  `e636db48:10016-10020`), which can run between those two. `TurnEnded` then read `cancelling`
  false: outcome `Failed` (`shell.rs:4452-4454` now) and the failure announced as `✗ system:`.
  With a file call running (`cancel_owed`), the settle also put the turn back to work (its row
  was never ended) and dropped the owed `■ Cancelled` line. RED, `red-test.txt`:
  - no call: reports `[working, idle, None, error]`, a `system` line;
  - a call running: `[working, idle, working, error]`, a `system` line, no `■ Cancelled`.
- **Fix** (`c2493f8b`): the settle takes the cancel only when no turn has started
  (`&& self.tally.started.is_none()`, `shell.rs:10035`). A started turn keeps its cancel until its
  `TurnEnded` takes it (`:4508`).
- **No cancel is held for good.** Every writer of `tally.started`: `begin` at `TurnStarted`
  (`:4112`); `None` at `TurnEnded` (`:4501`); `Tally::default()` in `reset_session` (`:14793`),
  in `hydrate_as` (`:3721`) and in the drained `TurnEnded` (`:14916`). `run_turn` sends a
  `TurnEnded` for every turn it starts (`worker.rs:3835`; a panic is `Fatal` and ends the session).
- **The cases the settle is for still pass** (no turn in hand in either):
  `a_prompt_the_cancel_dropped_before_it_started_does_not_silence_the_next` and
  `a_cancel_that_landed_after_its_turn_ended_does_not_silence_the_next`; deleting the settle's
  clear entirely is mutant 3 (`mutants.txt`).
- **fx** (`fx-src.txt`): a stopped turn is persisted `.interrupted` and returns
  (`orchestrator.zig:7233-7235`, `interruption.zig:219`), the finalization pushes
  `turn_finished(.interrupted)` (`finalization.zig:140`, `app_callbacks.zig:838-844`) -- the same
  on the stream's cancel path (`:8261-8263`) and the cancelled-calls one (`:8849-8851`) -- the reporter
  settles `idle` (`app_worker_runtime.zig:1293`, `program_status.zig:62-69`); `request failed`
  under `system`, and `noteFailure`, come only from an error out of `processQueuedWork`
  (`app_process_runtime.zig:157-169`, `app_worker_runtime.zig:1207-1216`).
- **No capture reaches this window**, and the unit tests are its receipt. It lies inside one pass
  of the loop, between the drain (`take_ui_events`, `src/tui/event_loop.rs:285`) and the settle
  (`:291`), which nothing between them blocks; the only delay seam the fault injector has
  (`SlowUi`, `:311`) is after the commit, outside it. `stopnext` does not reach it either: the
  settle of its keystroke's pass sees two places held (the stopped turn and the prompt).

### 2. A prompt sent while a started turn is in hand (opus SHOULD 2, reversed)
**fx, captured** (`fx/`, `fx-run2/`, `fx-run3/`; `p5-lifecycle-osc-stopnext`: a turn held
mid-stream, Ctrl-C and `second question` + Enter in one `send-keys`, so the prompt is in hand
before the stopped turn has ended). On every fx run, after the stop: `idle`, the `▪ Cancelled`
frame with nothing of the prompt, then `working`, then the frame with the prompt and its
activity -- `Thinking` on one run, `Generating` on two (`osc-stopnext.txt`, "fx's three runs";
on `fx/`'s `s03-exit` wire: the stop's `idle` @1256, `Cancelled` @1340, `working` @1423, the
prompt's text first @1480). **Why** (`fx-src.txt`, item 2): fx holds a prompt as a pending
submission -- whose activity its keystroke frame shows (`pendingPromptActivityVisible`,
`app_render_runtime.zig:286-290`) -- only while no turn streams and its worker can hold the next
start (`installPendingSubmission`, `input_submit_runtime.zig:1136`, `:1149`; `tryHoldTurnStart`
refuses while the worker processes or has work queued, `worker_runtime.zig:1273-1280`).
Otherwise it queues the prompt with the worker at once (`enqueuePromptForSubmit`, `:1112-1118`),
and nothing of the prompt's turn is painted until the worker takes it. `interruptnext` types its
prompt 1.5 s after the stop, when the stopped turn has finished: a pending submission, frame
first, as L-U1+U2e found. In the window it is the other branch. fx's stop stays requested until
the worker takes the next item (`worker_runtime.zig:1512`; `finishProcessing` leaves it,
`:1541-1563`) and nothing is busy while it is (`app_worker_runtime.zig:734-735`, `:762-765`).

**xfx before:** the runtime sends the stopped turn's `TurnEnded` and the prompt's `TurnStarted`
with no wait on the UI thread (`worker.rs:3835`, then `:3784` after the loop takes the next item).
- In one drain, nothing held the report: `working` ahead of the `Thinking` frame -- fx's order
  (`red/` x3: 0 failing claims, `osc-stopnext.txt`).
- In two drains, L-U1+U2e's hold took the prompt as waiting and wrote `working` after the
  `Thinking` frame -- not fx's order. RED: the split half of
  `a_prompt_typed_before_a_stopped_turn_ends_is_reported_ahead_of_the_frame_that_shows_its_turn`
  (`red-test.txt`, `one drain false`); the one-drain half passes on the base.
- The brief's shape, `997d7b1f` (`prev/` x3), held both splits: 6 failing claims on each run,
  `order … working [3] after Thinking: [False] on fx, [True] on prev` and the `activity` claim
  (`osc-stopnext.txt`).

**Fix** (`372e4236`): a waiting prompt carries whether a turn that had started was in hand when it
was sent (`Shell::prompts`' fourth, set from `tally.started` at submit, `shell.rs:12724`), and the
hold skips such a prompt (`behind_a_turn`, `:10245`). This is xfx's side of fx's two branches: a
started turn in hand is fx's stream active or worker processing. A steer handed on after its turn
is marked too (`hand_on_steers`), as fx's is already queued work (`finishProcessing`'s
`.continuation`, `worker_runtime.zig:1548-1551`); the report cannot tell the two values apart,
because the turn's `working` is what is on the terminal when it runs (mutant 8 survives, below). A prompt typed
with no turn in hand -- an idle session, behind a provider switch -- is held as before, frame
first, as fx's pending submission is (`osc/w02-done`); one queued behind a running turn writes
nothing new either way (`a_prompt_queued_behind_a_running_turn_writes_nothing_new_…`, one drain
and two). `a_prompt_typed_after_a_stop_is_reported_after_the_frame_that_shows_its_thinking`
(L-U1+U2e) is replaced by the test above, which asserts fx's order for both splits.

**Head** (`xfx/` x3): 0 failing claims on each run (`osc-stopnext.txt`). The split itself cannot
be captured -- the runtime's two sends are microseconds apart -- so the unit test holds it.

### 3. `/new` and `compaction_failed` (opus SHOULD 3)
- An automatic compaction that fails its turn sets `compaction_failed` and reports `error` at its
  settle (`shell.rs:11135`, L-U1+U2's); only that turn's `TurnEnded` or the next
  `TurnStarted` took it.
- `/new` (or `/clear`, `/reset`) typed before that end is read drains the end (`drained`,
  `:14915-14921`), so the flag stayed, and the status read the next conversation's prompt or
  manual `/compact` as settled (`status_activity`, `:10174`). RED (`red-test.txt`): a prompt typed
  before the dropped end is read, and a prompt or a `/compact` after the fence, each `None` where
  `working` is owed.
- **Fix** (`73fb9220`): `reset_session` clears it beside `cancelling` (`:14805`). The test's first
  case types the prompt before the drained end is applied, so clearing it there instead would not
  pass (mutant 11).

### The NITs
`app_worker_runtime.zig:785-788` is main's numbering; at the pin it is `cancellation_stops_turn`
at `:734-735` and the `busy` it feeds at `:762-765` (`fx-src.txt`). Corrected in the
`take_status_moved_by_input` comment, the replaced test's comment, the parity row and L-U1+U2's
README (`2a4e89e5`).

## Decisions
- **Item 2 follows the capture, not the brief.** The brief allows a divergence only for a user
  decision, an unsafe copy, a later pin or a platform limit; none applies, and the oracle is three
  fx runs with one order. The brief's shape is kept as `prev/` so the reversal is checkable.
- **The discriminator is "a started turn in hand at submit"** (`tally.started`), not "typed under a
  stop": a prompt queued behind a running turn is fx's queued prompt too, and its report is that
  turn's `working` either way; marking it keeps one rule.
- **Ctrl-C pressed while a completed turn's `TurnEnded` is still in the channel** now always ends
  as the stop said: `idle`, no summary, the `■ Cancelled` the keystroke wrote. Before, a settle in
  between made it `done` with a summary under that line. The user saw the stop, so the stop wins.
  (Read from the code -- the guard keeps `cancelling` for the `TurnEnded`, which then takes the
  `Interrupted` branch, `shell.rs:4452` -- with no test of its own.)
- **No osc.py claim was weakened.** Two were added (`activity`, `stable`); `order`'s `Thinking`
  claim already separated `prev/` from fx.

## Recorded difference (capture-backed, not this unit's)
A prompt typed in the same read as a stop: xfx writes its card on the stop's `Cancelled` frame
(`s03-exit`, `osc-stopnext.txt`: on `red/` `Cancelled` @883, `second question` @987, `working` @1127;
on `xfx/` @999, @1103, @1243); fx shows
nothing of it until its worker takes it (`fx/`: `working` @1423, the prompt @1480). The report
order is the same. fx queues that prompt with the worker and shows it only when the worker takes
it (item 2's mechanism); xfx writes the card of a prompt sent with no running activity at once
(`waits`, `shell.rs` submit). Added to the OSC 7501 row's "Where xfx differs" with no owner unit.

## The checker (`osc.py`)
L-U1+U2's `osc.py` with: `stopnext` read from this receipt's `fx/`, every other scenario from
L-U1+U2's `fx/` (the same pin; `../osc-program-status/fx/`); `s03-exit` an exit label; and two
claims -- `activity` (for every `working` on a scenario this receipt captured on fx, whether a
frame since the report before it showed `Thinking` or `Generating`, as on fx) and `stable` (every
further fx run has `fx/`'s reports, which-frame and activity answers).

## RED (`red-test.txt`)
The tests of `220f9361` over `e636db48`'s code, on a `--no-local` clone: 4 of 5 fail.
- `a_stopped_turn_whose_place_comes_back_before_its_end_is_read_ends_as_the_stop_said`:
  `([working, idle, None, error], true, 1)` against `([working, idle, None, None], false, 1)`.
- `a_stopped_turn_with_a_call_running_keeps_its_line_and_its_idle_through_a_late_end`:
  `([working, idle, working, error], true, false)` -- `working` again, `error`, a `system` line and
  no `■ Cancelled`.
- `a_prompt_typed_before_a_stopped_turn_ends_is_reported_ahead_of_the_frame_that_shows_its_turn`:
  `one drain false` -- the split held behind the `Thinking` frame.
- `a_reset_before_a_failed_compactions_turn_ends_takes_that_result_with_its_conversation`: all
  three cases `None` where `working` is owed.
- The fifth, `a_prompt_queued_behind_a_running_turn_writes_nothing_new_when_its_turn_starts`, is the
  guard for item 2's discriminator and passes there too, as it should.

## GREEN
- **The unit tests:** all of the above pass at `e27f9bcf` (`gates.txt`, step 04, 5272 passed).
- **Wires** (`osc-all.txt`): every osc scenario on the base's build (`red/`, banner `e636db4`) and
  on the head's (`xfx/`, `e27f9bc`): 47 wire pairs each, **0 failing claims on both**, and the same
  report sequence on every pair, base vs head. 13 scenarios read L-U1+U2's fx captures at the pin;
  `stopnext` reads this receipt's.
- **`stopnext`, three runs per build** (`osc-stopnext.txt`): fx's three runs agree (`stable`);
  `red*` and `xfx*` 0 failing claims on each run; `prev*` (`997d7b1f`) 6 on each run: `order …
  working [3] after Thinking: [False] on fx, [True] on prev` and `activity` on `s01`-`s03`.

## Wire mutants (`wire-mutants.txt`, `wire_mutants.py`)
`osc.py` passes the unchanged copy and fails each of 5 mutants of `stopnext`'s wires:
- the prompt's `working` moved behind the frame that shows its `Thinking` (round 1's order), on
  `s01` and on `s03` -- `order` and `activity` each fail;
- `error` written after the stop's `idle` (item 1's wire) -- `same`;
- `working`, `error` after it (item 1's wire with a call running) -- `same` and `activity`;
- one fx run's `working` moved behind the prompt's frame -- `stable`.

## Code mutants (`mutants.txt`, `mutants.py`)
11 mutants on a `--no-local` clone at `e27f9bcf`, each against every TUI library test
(`tui::`): **10 killed, 1 survived, as predicted.**
- Item 1: the base's settle (1), the guard inverted (2), the settle's clear deleted (3: killed by
  `a_prompt_the_cancel_dropped_before_it_started_…`, the case the settle exists for).
- Item 2: the base's hold (4), no prompt marked (5), every prompt marked (6: an idle session's
  prompt reported ahead of its `Thinking`, killed by L-U1+U2's held-prompt test), marked by running
  activity instead of a started turn (7), round 1's start-holds shape (9).
- **8 survives:** a steer handed on after its turn unmarked. The report is that turn's `working`,
  already written, whichever way the marker reads; it is kept for fx's model (queued work), and no
  test can tell it apart.
- Item 3: the base's reset (10), the mark dropped by the drained `TurnEnded` instead (11).

## Sweep (`sweep.txt`)
Every `p5-lifecycle-*` scenario (osc and lifecycle), fx v0.0.16 against the base's debug build
(`e636db4`) and the head's (`e27f9bc`), inside this unit's lock slot: **69 pairs on each side, 0
changed** (28 normalized-identical, 40 unresolved, 1 approved cascade on both; 0 failed, 0
timeout). No pair drops. The osc scenarios take no snap; their evidence is the wire, above.

## Gates (`gates.txt`)
At `e27f9bcf`, every step of `ci.yml` and `sweep.yml`'s two test scripts, one at a time, inside one
suite-lock slot with the sweeps and mutants (the lock admitted nothing of this unit for 2 h 20 m
while it polled job by job, so the heavy jobs ran as one).

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings`, without and with `fault-injection` | ok, ok |
| `cargo test --locked --all-targets --no-fail-fast` | 5272 passed, 0 failed, 7 ignored (14 binaries) |
| `cargo test --locked --features fault-injection --test tui`, `--lib` | 192 passed; 4382 passed, 0 failed, 7 ignored |
| `check-no-stubs`, `check-no-secrets`, `check-xfx-identity`, `check-preview-contract` | ok |
| `test-check-no-secrets`, `test-gate-env`, `test-preview-contract` | ok (13 cases), ok (5 cases), ok (24 mutants caught) |
| `check-tui-preflight-cost`, `check-usage-clock` | ok, ok |
| `scripts/fxcap/test-sweep.sh` | 50 + 26 tests OK |
| `scripts/fxcap/test-capture.sh` | **41 of 42**: `test_models2_and_modelsdelay` errors (below) |
| release and faulty release builds | ok; `xfx status` of each says `build_revision=e27f9bcf5f1c` |
| `scripts/smoke.sh` (release) | 47 checks, 0 failures |
| `scripts/smoke-tui.sh` (release, `--faulty`) | 37 scenarios + the oracle, 1168 checks, 0 failures |

**`test_models2_and_modelsdelay` is load, not this unit.** It runs `capture.sh` with `/bin/bash` as
the binary -- no xfx code -- against a catalog swapped at 5000 ms, and its harness files are
identical at base and head. Under the lock, alone: head 2 of 3, the base's clone 3 of 3; then
five interleaved head/base pairs: **10 of 10 fail, on both alike** (`gates.txt`, 17b-17d).

## Look ahead: the next break (`next-break.txt`, not fixed here)
A stop pressed after the runtime has ended a turn and taken the prompt queued behind it, before the
UI read either. The keystroke counts that prompt as covered (`cancel_covers = 1`), the first turn's
`TurnEnded` zeroes the count and takes the cancel, and the second turn -- which the runtime does
stop -- is painted and ends `working`, `error`, `✗ system:` (scratch test on `c389ba9d`, failing).
The count guesses which turn a start belongs to; a prompt the runtime dropped on reading the stop
is indistinguishable from one it took. The fix that removes the guess is `TurnStarted` carrying the
submission number of the turn it starts, compared with the last cancel's `through`. It changes the
event every TUI test applies, so it is left for a unit of its own (backlog).

## Files
`red-test.txt` (RED), `fx-src.txt` (every fx citation, read at the pin), `osc.py` (the checker),
`osc-all.txt`, `osc-stopnext.txt` (GREEN and the reversal), `wire_mutants.py` and
`wire-mutants.txt`, `mutants.py` and `mutants.txt`, `sweep.txt`, `gates.txt`, `next-break.txt`;
wires under `fx*/` (fx v0.0.16, `stopnext` only), `red*/` (`e636db48`), `xfx*/` (`e27f9bcf`),
`prev*/` (`997d7b1f`). Every committed file is redacted (no home-directory path, no user name).
