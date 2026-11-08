# P5 STOP-NEXT-2 receipt: the stop window's last items, and STOP-NEXT's review follow-ups

**S** (terminal escapes: the OSC 7501 wire, the window title; and what a keystroke submits). Spec: STOP-NEXT's
proposed list (`../stop-next/README.md`, "STOP-NEXT-2"), the two reviews of STOP-NEXT (zbrain
`p5/STOP-NEXT/reviews/opus.md`: SHOULD 1-2 and the NIT; `gpt6.md`: NTH 1-3), and the dispatcher's brief.

- **Base:** STOP-NEXT's closed head `feat/p5-stop-next` `d2459a37`.

## Design

### The scope, item by item
| # | item | what fx does at the pin | xfx at the base | mechanism | here? |
|---|---|---|---|---|---|
| R1 | opus SHOULD 1: item 5's exemption too broad | a second press on a running compaction only requests its cancel again (`cancelContextCompaction`, `worker_runtime.zig:1002-1035`); one on a provider preparation cancels it again (`cancelProviderPreparation`, `auth_runtime.zig:2052-2056`); neither writes a line or drops a queued prompt | the exemption (`shell.rs:12484-12488`) also covers the first press's own compaction (`active()` includes `Stopping`) and its own switch (`switch_pending()` until the loop top): a second press quotes `accepted()` again, drops the prompts typed between, and for a switch writes a second `■ Cancelled` | exempt only work the recorded stop did not cover: a manual compaction not yet stopped, a switch numbered past `cancel_through` (`WorkHandle::pending_switch`) | yes |
| R2 | opus SHOULD 2: wording | -- | "a provider switch" where `switch_pending` also holds a `/resume` or a fork | the receipt and the parity row name all three | yes |
| R3 | opus NIT: `apply`'s doc | -- | the doc sat above `turn_starts` | moved back | yes |
| R4 | gpt6 NTH 1: `stop_requested` clears at the UI's read of the start, fx's at the worker's take | the press is cancelled by stream state the UI sets when it drains the turn's `begin_prompt` (`app_worker_runtime.zig:965-975`); after a stop the keystroke cleared it (`input_interrupt_runtime.zig:132`), so until that drain a press finds no active operation (`activeCancellationTarget`, `:178-187`; `cancellationTarget`, `:12-21`; `handleSemanticCtrlC`, `app_input_runtime.zig:1510-1547`) and cancels nothing, whatever the worker took | the same: until the start is read the guard holds the press | no change; a test pins the schedule (the take, a second press, then the start) | test only |
| R5 | gpt6 NTH 2: "no card before the start" | -- | the pacer and full-transcript prompts' card tests assert only after the start | assert before and after, both branches | test only |
| R6 | gpt6 NTH 3: STOP-NEXT's item 6 "closes: yes" | -- | -- | STOP-NEXT's table (`../stop-next/README.md`, row 6) now reads: title ownership and leading order closed there, the one title a tick closed here (T1); its rows 9 and 10 point on to STOP-NEXT-3 | yes |
| T1 | one title per tick | each title is its own write when it is set (`syncTerminalTitleWith`, `app_session_runtime.zig:2965-2987`): the stopped end's on the pass that drains it (`ensureCachedSessionTitle`), a model's when it lands, later | a model's title read after the stopped end in one tick replaces it, and the tick writes one title, behind its report | the leading title is the string the end gave, kept apart from the title owed after it; when another title is owed behind it, the lead still leads the report and the other is held for the next tick's frame | yes |
| 7 | a stop and its turn's end in one tick | the end is drained on a later pass than the keystroke's (keys are handled after a pass collects its facts, `src/ui/event_loop.zig:77-117`): `idle`, the `Cancelled` frame, the title (`../stop-late-event/osc-stopnext.txt`: @1256, @1340, @1404) | the end's title leads the tick's report: `OSC 2`, `idle`, the `Cancelled` frame | a lead in the stop's own tick is held past that tick's frame and written by the next | yes |
| 8 | a finished turn's text read after the keystroke (covered turns too) | a batch whose segment holds the turn's own `turn_finished(.completed)`, or one taken after the worker let go of the turn (`active_turn_id` 0, `finishProcessing`), is no interrupted segment: its text is presented (`batchSegmentIsInterrupted`, `app_worker_runtime.zig:301-325`; `takeEventBatch`, `worker_runtime.zig:923-937`); a batch taken while the stopped turn still runs drops its text (`cancelledEventAdmission`, `:339-389`) | every `Delta` under a stop is dropped | under the user's stop, a `Delta` (and an operational line) is kept for the drain it arrived in; the turn's finished end in the same drain presents it, the end of the drain drops it | yes |
| 10a | `turn_start_held`: a prompt while another is a pending submission | `submit` returns while a pending submission exists -- `prompt_submit_deferred_for_pending` (`input_submit_runtime.zig:648-689`), the draft left in the composer -- prompts and commands alike, from the Enter (`installPendingSubmission`, `:1123-1160`) until the drain of its `begin_presented_prompt` (`acceptPresentedPrompt`, `:415-426`; queued already presented, `main.zig:1434-1441`, `:1263-1270`) | the second line is submitted and queued | a prompt fx would hold as a pending submission is recorded as pending until its start is read, a stop covers it, a reset or a resume; meanwhile Return submits nothing | built, then STOP-NEXT-3 (Decisions, 1) |
| 9 | a failed turn's title | depends on how the turn failed: a stream or non-retryable failure keeps a non-blank partial answer (`orchestrator.zig:8389-8400`, `:9246-9260`), a stall keeps its completed steps (`:5321-5330`), any other error keeps only completed steps and finishes it with a summary (`finishExecutionOnlyFailureIfNeeded`, `finalization.zig:226-246`, called at `orchestrator.zig:6118`, `:9443`); a kept turn titles the session | titles no failed turn | needs the runtime to say which path failed the turn and whether it kept it (xfx's log records every failed turn, `agent/machine.rs:744-746`, `:835-847`) | STOP-NEXT-3 |
| 10b | `session_transition_held` | a `/new`, `/clear` or `/reset` holds starts and defers every key typed until its transition settles (`app_session_runtime.zig:1532-1538`, `:1723-1737`; `app_input_runtime.zig:679-700`; `main.zig:2992-3010`) | no deferral | key deferral across a reset | STOP-NEXT-3 |

### R1: the exemption
`cancel_turn` holds a press while `stop_requested`, save for work typed since. At the base "typed since" is read as "a
manual compaction is active or a switch is pending", which also holds for the first press's own compaction or switch
while it unwinds. Now:
- a manual compaction is exempt only while it is running and not stopped (`compaction.stopped()` is the first press's
  mark);
- a switch is exempt only when its number is past `cancel_through`: `TitleDesk` already records the switch's number
  (`switching(at)`), and `WorkHandle::pending_switch()` returns it while it is still to be made.

### T1 and item 7: where a stopped end's title goes
At the base the lead is a flag and the title is computed when taken, so a later title in the same tick replaces it.
Now the lead holds the string the end gave the session. When the band takes the lead (`take_leading_title`):
- **the stop was pressed in this tick** (item 7): the lead is dropped and the title owed is held past this tick's frame
  (`take_title` answers nothing this tick), and `Shell::tick_committed`, called once the tick's frame is committed,
  asks for a frame, so the next tick's frame carries it:
  `idle`, the `Cancelled` frame, the title;
- **another title is owed behind the lead** (T1): the lead is the band's title, ahead of the report, and the other
  title is held past this tick's frame (`take_title` answers nothing) for the next tick's frame to write;
- otherwise as at the base: the lead is the band's title and the report rides behind it.

### Item 8: a finished turn's late text
fx presents text in a batch that holds the turn's completed finish, or that the worker had already let go of; it
drops text in a batch taken while the stopped turn still runs. xfx's drain (`take_ui_events`) is its batch. Under the
user's stop of a turn whose end may be a finish (`stop_reads_end`, no compaction ended it), a `Delta` or an
operational line is kept; a finished end (`None`, not quiet) read in the same drain presents what was kept, below the
keystroke's `Cancelled` line and above the summary; `Shell::ui_events_taken`, called when a drain ends, drops what is
left. A turn the runtime finished before the stop reached it has all its events in the channel already, so one drain
takes them together, as fx's one batch does.

### Item 10a: a pending submission (built, not landed: Decisions, 1)
fx holds a prompt as a pending submission when no turn streams, its pacer is empty, the full transcript is closed and
its worker can hold the next start (`installPendingSubmission`), and while one exists its `submit` returns at once.
In xfx that is a prompt sent with no turn of its own in hand, not one upstream would queue at once
(`!waits && !queued_at_once`), with no other prompt waiting and no compaction running. `pending_submission` holds its
number until its start is read, a stop covers it, a reset or a resume; while it is set, `submit` returns with the draft
left where it is.

## Commits (`feat/p5-stop-next-2`, from `d2459a37`)
| commit | what |
|---|---|
| `24fbc062` | R3: `apply`'s doc comment back above it (no code change) |
| `dac51b53` | the RED: tests only (below) |
| `2c2c0e77` | R1, R2: the exemption narrowed to work the stop did not cover; the field's doc names the three switches and why the guard reads the UI's start |
| `621a0ce4` | T1 and item 7: the lead is the end's string; a stop's own tick holds the title past its frame; a later title in the end's tick is held for the next frame |
| `cc231700` | item 8: a finished turn's late text read with its end in one drain is presented |
| `7d5e3d4b` | the parity row (R2's wording, items 7, 8, T1, STOP-NEXT-3); the refs this unit's code moved |
| `2a6e8fa7` | guard tests for the code mutants that survived the first run (tests only) |
| the receipt commit | this receipt; STOP-NEXT's table qualified (R6); `gates.txt` follows in its own commit |

## RED (`red-test.txt`)
**`dac51b53`'s tests over `24fbc062`'s code**, in the worktree at `dac51b53`, tree clean. 5 fail, 2 guards pass:
- R1, `a_second_press_while_the_first_ones_own_work_unwinds_stops_nothing_more` (opus's RED): `/compact`, Ctrl-C,
  `next`, a second press before the compaction settles; and the same over a provider switch. `[("/compact",
  Some(Cancel { through: 2 }), 0, [], true), ("a provider switch", Some(Cancel { through: 2 }), 1, [], true)]` for
  `[("/compact", None, 0, [2], true), ("a provider switch", None, 0, [2], true)]` -- a second stop, `next` dropped,
  and over the switch a second `■ Cancelled`;
- T1, `a_model_title_after_a_stopped_end_in_one_tick_follows_the_ends_title_and_the_report`: the wire after the
  end holds `working` and then `]2;Named`, and no `]2;go` (the stopped turn's title lost to the model's);
- item 7, `a_stop_and_its_turns_end_in_one_tick_put_the_title_after_the_cancelled_frame`: `]2;go`, `idle`, the
  `Cancelled` frame (fx: `idle`, the frame, the title);
- item 8, `a_finished_turns_late_text_is_presented_with_its_end_in_one_drain`: `("one drain", false, ...)` and
  `("a cut drain", false, ...)` for `true` -- the text dropped; `a_covered_turn_that_finished_before_the_stop_presents_its_text`:
  the covered turn's `done` and summary with no text.
- Guards, passing on the base too: R4, `a_press_after_the_runtime_took_a_turn_typed_after_the_stop_waits_for_its_start`
  (the schedule gpt6 asked for: a stop, `next` typed, the stopped turn's end, the runtime takes `next`, a second
  press, then the start, then a third press -- the second stops nothing, as fx's finds no active stream until its UI
  drains the start, and the third stops that turn); R5, `a_prompt_queued_at_once_writes_no_card_before_its_turn_starts` (the pacer's branch and the full
  transcript's: no card before the start, the card after it).

**Item 10a's RED** (`item10a-red-test.txt`): `103bb986`'s tests over `cc231700`, 1 of 2 failing: a second line
while the first is a pending submission is submitted, where fx leaves it in the composer. Dropped from the branch
with its fix (Decisions).

## GREEN
- The seven tests above pass at `cc231700`, where the full suite passed (5366 passed, 0 failed, one run in a
  suite-lock slot), and in the gate at the receipt commit (below).
- **Existing tests changed:** `a_title_another_event_owed_in_a_stopped_turns_tick_follows_its_report` (comment only:
  the band no longer writes one title a tick); `a_completed_turns_text_unread_at_the_stop_leaves_the_cancelled_line_above_its_summary`
  (comment only, naming item 8's test). No assertion of an existing test changed (`git diff -U0 d2459a37..2a6e8fa7
  -- src`: every removed line in a test module is a comment).
- **Guards with the fixes** (each failing only under its mutant, below): `late_text_a_stop_holds_counts_against_the_drains_bound`
  (`cc231700`); `a_compactions_stop_and_its_turns_end_in_one_tick_put_the_title_on_the_next_tick` and item 8's cut-drain case (`2a6e8fa7`).

## Decisions
1. **Item 10a dropped to STOP-NEXT-3 after it was built.** The fix (`item10a-carried.txt`, with the RED) made both
   item 10a tests pass and 46 others fail, every one a fixture that sends two lines to an idle session with no start
   read between them: the prompt history's walk, image and paste recall, the queue's refusals, `/compact`, `/copy`,
   `/fork`, `/resume`, a reset with a prompt waiting. fx defers every Enter while a pending submission is held, so
   each of those fixtures is pinning what fx would not do -- but changing 46 fixtures and re-reading what each pins
   against fx is a unit of its own, with a capture (two lines in one send-keys on an idle session) and `smoke-tui`.
   The branch went back to `cc231700` (`git reset --hard`, the patches kept in the receipt), and the full suite
   passed there.
2. **Item 9 and item 10b to STOP-NEXT-3**, unbuilt: item 9 needs the runtime to say how a turn failed and whether
   fx would keep it (four failure paths, `fx-src.txt`, "STOP-NEXT-3, item 9"); item 10b needs keys deferred across a
   reset's transition, the composer's half of fx's hold.
3. **R4 is no defect** [read from the code]: fx's press is gated on the stream its UI sets when it drains
   `begin_prompt` (`app_worker_runtime.zig:965-975`; `handleSemanticCtrlC`, `app_input_runtime.zig:1510-1547`;
   `activeCancellationTarget`, `input_interrupt_runtime.zig:178-187`), and its stop's keystroke cleared that stream
   (`:132`), so until the UI drains the next turn's start a press cancels nothing, whatever the worker took. xfx's
   guard clears at the same read. A test pins the schedule; no code changed.
4. **R6, STOP-NEXT's item 6**: its "closes?" cell now reads yes for title ownership and leading order and no for one
   title a tick, which is T1, closed in `621a0ce4` (`../stop-next/README.md`, edited in the receipt commit; its rows 9
   and 10 now point on to STOP-NEXT-3).
5. **No wire claim added.** Items 7, 8 and T1 need the runtime to finish a stop, or a model to name the session,
   within one tick of the UI; R1 needs a second press inside a stop's own unwinding. No capture reaches those
   windows, so the unit tests are their receipt, and `osc.py` is STOP-NEXT's unchanged.
6. **A drain is fx's batch only when it emptied the channel.** `drain_ui_events` now says whether it did; a drain
   its bound cut short (`bridge::UI_EVENTS` events, or `PACED_BACKLOG`) keeps the late text for the next, and the
   kept text counts against the backlog bound, so the bound that keeps the pacer short still does.

## The checker (`osc.py`, `wire_mutants.py`)
STOP-NEXT's, unchanged but for their docstrings and the receipt they copy. fx's wires are the earlier units' at the
same pin (`stopnext`: `../stop-late-event/fx*/`; every other scenario: `../osc-program-status/fx/`).
- **Wires** (`osc-all.txt`): every osc scenario, on the base (`d2459a3`, `base/`) and the head (`7d5e3d4`, `xfx/`):
  47 wire pairs each, **0 failing claims on both**, and each build's banner on every wire of its side.
- **`stopnext`, three runs per build** (`osc-stopnext.txt`): 0 failing claims on each of the six. Every run of both
  builds has the same offsets after the stop: `idle` @797, the `Cancelled` frame @883, `OSC 2 first question` @969,
  `working` @988, `second question` @1070 -- fx's order (@1256, @1340, @1404, @1423, @1480 on each of its three
  runs). The base's title already follows its `Cancelled` frame, so the captured stop's end is read on a later tick
  than the keystroke's and item 7's window is not reached: nothing moved.
- **Wire mutants** (`wire-mutants.txt`): at `7d5e3d4`, the unchanged copy passes and each of STOP-NEXT's 10 mutants
  fails.

## Code mutants (`mutants.py`, `mutants.txt`)
18 mutants, each one exact replacement in `shell.rs` or `event_loop.rs`, run on a `--no-local` clone against every
TUI library test (`tui::`), 900 s each (a suite past it would be KILLED (hang); none was).
- **The first run, at `7d5e3d4b`:** 15 killed, 3 survived, 0 failed to build: a compaction's stop that marks no tick
  (6); every drain dropping late text (16); a drain cut at its bound counted as emptied (17). The unmutated clone
  passes 2801 of 2801.
- **`2a6e8fa7` adds a test for each** (tests only): `a_compactions_stop_and_its_turns_end_in_one_tick_put_the_title_on_the_next_tick`
  (6), and item 8's cut-drain case with its text in the drain the bound cuts and its end in the next (16, 17; before
  it, the text came after the cut, so it crossed no boundary). The three again, there (`mutants-rerun` rows in
  `mutants.txt`): 3 of 3 killed, each by its new test; the unmutated clone passes 2802 of 2802.
- **All 18 are killed, 0 failed to build.**
- The rows, by what they break: R1 (1-3: the base's two exemptions, a switch the stop covers by one); item 7 and T1
  (4-12: the base's lead, each stop's mark, a later title replacing the lead, a held title taken anyway or counted
  as this tick's, no frame asked for it, the stop's tick or the loop's tick never ending); item 8 (13-18: the base's
  drop, late text under the stop's own end, an emptied drain keeping it, every drain dropping it, a cut drain
  counted as emptied, late text outside the drain's bound).

## Sweep (`sweep.txt`)
Every `p5-lifecycle-*` scenario (osc and lifecycle, 30 scenarios), fx v0.0.16 against the base's debug build
(`d2459a3`, sha256 `951485d4…`) and the head's (`7d5e3d4`, `1d663f2f…`), in one suite-lock slot with the captures;
both banners and both sha256s are in each summary. **69 pairs on each side, none on one side only, 0 failed and 0
timed out, and 0 changed pairs**: 28 normalized-identical, 1 approved-divergence-cascade and 40 unresolved-defect on
both. `retryok`'s `k01-1.5s`, which moved in STOP-NEXT's sweeps with fx's own capture, is normalized-identical on
both sides here (the loads: 3.9-4.0 and 2.9-6.4, 1-minute averages). No pair drops on xfx's side.

## Gates (`gates.sh`, `gates.txt`)
Run at the receipt commit, in one suite-lock slot; the table is written with `gates.txt`, in the commit after.

## The refs (`refs.py`, `refs-before.txt`, `refs-after.txt`)
STOP-NEXT's `refs.py`, with the base `d2459a37` as the frame. **Before `7d5e3d4b`** (`refs-before.txt`, the code at
`cc231700`): 43 hold, 12 moved, 2 lost (the parity row's two `worker.rs` refs, on lines this unit rewrote).
Re-pointed in `7d5e3d4b`: the ledger's `shell.rs:12865` -> `:12959`, `:38150` -> `:38274` five times, `:1700` ->
`:1719`; `capfile.py`'s `warning_shown` `:14341-14366` -> `:14435-14460` and its bare `:14359-14361` ->
`:14453-14455` (by hand); `event_loop.rs`'s own `:959-962` -> `:970-973`; `shell.rs`'s `worker.rs:3840` -> `:3846`,
and the parity row's `:3840`/`:3524` -> `:3846`/`:3530` (by hand). Left as they were: the two comment refs main's
own tests had wrong before STOP-NEXT (`event_loop.rs`'s `shell.rs:2706`, `shell.rs`'s `:5816`), as STOP-NEXT left
them. **After** (`refs-after.txt`, frames `d2459a37` and `7d5e3d4b`, the code at `2a6e8fa7`): 57 hold, 0 moved, 0
lost -- the guard tests moved none.

## Line budget
`git diff -U0 d2459a37..2a6e8fa7 -- src`, each hunk counted by whether its new side starts below the file's
`mod tests {`: outside tests +192 / -41 lines; inside tests +425 / -14. Under the brief's ~600, so the stop was
not the budget: item 10a's fallout was (Decisions, 1).

## STOP-NEXT-3 (proposed; the parity row's "Where xfx differs" names each)
1. **A failed turn's title and summary** (item 9). fx keeps a failed turn by how it failed: a stream or
   non-retryable failure keeps a non-blank partial answer (`orchestrator.zig:8389-8400`, `:9246-9260`), a stall its
   completed steps (`:5321-5330`), any other error only its completed steps, finished with a summary
   (`finishExecutionOnlyFailureIfNeeded`, `finalization.zig:226-246`, called at `orchestrator.zig:6118`, `:9443`);
   a kept turn titles the session. Needs the runtime to say which path failed the turn and whether it kept anything
   (xfx's log records every failed turn, `agent/machine.rs:744-746`, `:835-847`).
2. **A line sent while a prompt is fx's pending submission** (item 10a). fx's `submit` returns while one exists
   (`prompt_submit_deferred_for_pending`, `input_submit_runtime.zig:648-689`), prompts and commands alike, until
   its UI drains the turn's `begin_presented_prompt` (`acceptPresentedPrompt`, `:415-426`). The RED and the fix
   tried are in `item10a-carried.txt`, with the 46 tests the fix broke; each needs its fixture to start the turn
   between its lines, or what it pins re-read against fx. With a capture (two lines in one send-keys on an idle
   session) and `smoke-tui`.
3. **A press on a pending submission** (with 2): fx's Ctrl-C takes the pending submission back and returns before
   the stop (`app_input_runtime.zig:1541-1543`): the queued prompt removed, or its turn asked to cancel if the worker
   already runs it, and nothing written into the transcript (`cancelPendingSubmission`,
   `input_submit_runtime.zig:429-486`; `clearPendingSubmissionWithSnapshots`, `:549-569`) [read from the code]. xfx's
   press there is a stop, with its `■ Cancelled` line.
4. **Keys during a reset's transition** (item 10b): fx holds a `/new`, `/clear` or `/reset`'s transition
   (`app_session_runtime.zig:1532-1538`) and defers every key typed meanwhile (`app_input_runtime.zig:679-700`),
   replaying them with the hold still on (`app_session_runtime.zig:1723-1737`; `main.zig:2992-3010`). xfx defers no
   keys across a reset.

## Files
- `README.md`; `fx-src.txt` (every fx citation this unit adds, read at the pin).
- `red-test.txt`: the unit's RED; `item10a-red-test.txt`, `item10a-carried.txt`: item 10a's RED, the fix tried and
  its fallout, for STOP-NEXT-3.
- `osc.py`, `osc-all.txt`, `osc-stopnext.txt`: the checker and the wires' verdicts; `wire_mutants.py`,
  `wire-mutants.txt`.
- `mutants.py`, `mutants.txt`: the code mutants.
- `sweep.txt`; `gates.sh`, `gates.txt`; `refs.py`, `refs-before.txt`, `refs-after.txt`.
- The wires: `base/`, `base-run2/`, `base-run3/` (`d2459a37`); `xfx/`, `xfx-run2/`, `xfx-run3/` (`7d5e3d4b`). fx's are
  the earlier units' (above).

Every committed file is redacted: it holds no home-directory path and no user name.
