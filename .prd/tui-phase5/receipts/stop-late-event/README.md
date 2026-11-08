# P5 STOP-LATE-EVENT receipt: a stopped turn's late end, a stop-window prompt, a reset's failed compaction

**S** (terminal escapes: the OSC 7501 wire). Spec: the round-5 opus review of L-U1+U2 (zbrain
`p5/L-U12/reviews/r5-opus.md`, "Findings": SHOULD 1-3 and the NITs) and L-U1+U2's scratch repro
`../osc-program-status/system-leak.txt`; round 2, the two reviews of round 1 (zbrain
`p5/STOP-LATE-EVENT/reviews/gpt6.md`, `opus.md`) and the dispatcher's decisions on them.

- **Base:** `feat/p5-osc-program-status` `e636db48` (L-U1+U2's closed head; this unit changes the
  code that unit added, which is not on main yet).
- **Merged:** `origin/main` `884497c0` (train 4), in `6958cdc4` (below, "The merge of main").
- **Code:** `2b21b797`, the commit every `xfx/` capture's banner names (`v0.1.0-2b21b79`) and the
  round-2 suite ran at; then `d4ce0e0c`, one test and nothing else (the guard the mutant run asked
  for, below), and `27714762`, the parity row only. Round 1's commits on the base:
  - `220f9361` **RED**, tests only (`red-test.txt`);
  - `c2493f8b` item 1, the stop window;
  - `372e4236` item 2, a prompt sent while a started turn is in hand;
  - `73fb9220` item 3, `/new` and `compaction_failed`;
  - `2a4e89e5` docs: the OSC 7501 parity row, the comments, the `785-788` mis-cite, L-U1+U2's
    README bullet marked corrected;
  - `c389ba9d` the scenario `p5-lifecycle-osc-stopnext`;
  - `e27f9bcf` docs: fx's submission mechanism behind the capture, in the hold's comment and the
    parity row (comments only);
  - `1db01416`, `f8285685` round 1's receipt, and a note in L-U1+U2's README.

  The merge and round 2:
  - `6958cdc4` the merge of main `884497c0`; `c711f016`, `1fe55854` the refs it moved;
    `91f03d71` STOP-NEXT named as the owner in the parity row; `aca87fc8` the merge's receipt;
  - `4abcc8c4` **round 2's RED**, tests only (`red2-test.txt`);
  - `96645f92` a stopped turn's end says what the turn came to (MUSTs 1 and 2, the title);
  - `8999d4b2` the three queue-at-once states and a dropped prompt (SHOULD, MUST 4; MUST 3's
    comment);
  - `10481b46` the stopnext header; `6eddaf5a` the parity row; `2b21b797` a guard test;
  - `d4ce0e0c` the guard test for code mutant 20; `27714762` a failed turn's title in STOP-NEXT's
    list;
  - the receipt commit after them adds this directory's round-2 files only.
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
The happy path: a user's stop of a turn whose start was read is one `idle` on the wire and one
`■ Cancelled` line in the document, and the stopped turn's end -- the stop's own end -- adds
neither, whenever it is read. (Any other end is round 2's, below.)
- `cancel_turn_with` sets `cancelling` (`src/tui/shell.rs:12364`) and, with no call
  running, ends the row.
- The runtime sends the stopped turn's `TurnEnded { failure: Some("the turn was cancelled") }`
  (`src/tui/worker.rs:3835`; the text, `bridge::TURN_CANCELLED`, `src/tui/bridge.rs:111`) and only
  then gives the place back (`worker.rs:3524`).
- **Where it broke:** the band's settle took the cancel once no place was held (`shell.rs` settle,
  `e636db48:10016-10020`), which can run between those two. `TurnEnded` then read `cancelling`
  false: outcome `Failed` and the failure announced as `✗ system:`.
  With a file call running (`cancel_owed`), the settle also put the turn back to work (its row
  was never ended) and dropped the owed `■ Cancelled` line. RED, `red-test.txt`:
  - no call: reports `[working, idle, None, error]`, a `system` line;
  - a call running: `[working, idle, working, error]`, a `system` line, no `■ Cancelled`.
- **Fix** (`c2493f8b`): the settle takes the cancel only when no turn has started
  (`&& self.tally.started.is_none()`, `shell.rs:10080`). A started turn keeps its cancel until its
  `TurnEnded` takes it.
- **No cancel is held for good.** Every writer of `tally.started`: `begin` at `TurnStarted`
  (`:4123`); `None` at `TurnEnded` (`:4532`); `Tally::default()` in `reset_session` (`:14852`),
  in `hydrate_as` (`:3731`) and in the drained `TurnEnded` (`:14976`). `run_turn` sends a
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
  of the loop: between the drain (`take_ui_events`, `src/tui/event_loop.rs:285`) and the pass's
  settle (`:291`), and -- a second Escape or a Ctrl-C within a tick of the runtime's end -- between
  the keystroke's own settle inside `read_burst` (`:275`; `shell.rs:12288` for a Ctrl-C, `:9171`
  for an Escape at an approval, `:9319` at a question) and that pass's drain. Nothing in either
  blocks, and the only delay seam the fault injector has (`SlowUi`, `event_loop.rs:311`) is after
  the commit, outside both. `stopnext` does not reach it either: the settle of its keystroke's pass
  sees two places held (the stopped turn and the prompt).

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

**Fix** (`372e4236`, widened in `8999d4b2`): a waiting prompt carries whether fx would have queued it
at once (`Shell::prompts`' fourth, set at submit, `shell.rs:12780`), and the hold skips such a
prompt while the runtime holds a place for it (`behind_a_turn`, `:10298`). Round 1 read one of
fx's `.unavailable` branches: a turn that started still in hand, which is fx's stream active or
its worker processing. Round 2 reads two more, each with a RED (below): the pacer still holding
text (`input_submit_runtime.zig:1138-1142`) and the full transcript open (`:1143-1147`). A start
held by another waiting prompt or by a session transition (`worker_runtime.zig:1276-1277`) is
not read: STOP-NEXT. A steer handed on after its turn is marked too (`hand_on_steers`), as fx's
is already queued work (`finishProcessing`'s `.continuation`, `worker_runtime.zig:1548-1551`); the
marker decides the report's tick when the turn's last report was a result, not `working` (round
2's MUST 3: mutant 10, killed). A prompt typed
with no turn in hand -- an idle session, behind a provider switch -- is held as before, frame
first, as fx's pending submission is (`osc/w02-done`); one queued behind a running turn writes
nothing new either way (`a_prompt_queued_behind_a_running_turn_writes_nothing_new_…`, one drain
and two). `a_prompt_typed_after_a_stop_is_reported_after_the_frame_that_shows_its_thinking`
(L-U1+U2e) is replaced by the test above, which asserts fx's order for both splits.

**Head** (`xfx/` x3): 0 failing claims on each run (`osc-stopnext.txt`). The split itself cannot
be captured -- the runtime's two sends are microseconds apart -- so the unit test holds it.

### 3. `/new` and `compaction_failed` (opus SHOULD 3)
- An automatic compaction that fails its turn sets `compaction_failed` and reports `error` at its
  settle (`shell.rs:11190`, L-U1+U2's); only that turn's `TurnEnded` or the next
  `TurnStarted` took it.
- `/new` (or `/clear`, `/reset`) typed before that end is read drains the end (`drained`,
  `:14975-14981`), so the flag stayed, and the status read the next conversation's prompt or
  manual `/compact` as settled (`status_activity`, `:10220`). RED (`red-test.txt`): a prompt typed
  before the dropped end is read, and a prompt or a `/compact` after the fence, each `None` where
  `working` is owed.
- **Fix** (`73fb9220`): `reset_session` clears it beside `cancelling` (`:14865`). The test's first
  case types the prompt before the drained end is applied, so clearing it there instead would not
  pass (mutant 14).

### The NITs
`app_worker_runtime.zig:785-788` is main's numbering; at the pin it is `cancellation_stops_turn`
at `:734-735` and the `busy` it feeds at `:762-765` (`fx-src.txt`). Corrected in the
`take_status_moved_by_input` comment, the replaced test's comment, the parity row and L-U1+U2's
README (`2a4e89e5`).

## The merge of main (`remerge.txt`, `refs.txt`)
`origin/main` `884497c0` (train 4: MCP-E, TC-NOTATION, FLAKE-HINT4) merged into the branch in
`6958cdc4`. **One conflict, `docs/parity.md`:** this branch inserted the two OSC 7501 rows right
above "colored and hyperlinked TTY output", which main rewrote; kept both, this branch's rows
then main's row. `src/` and `scripts/fxcap/parity-ledger.toml` merged on their own (no
remerge hunk); this branch never touched the ledger, so main's copy, its deletions included
(the `truecolor-grey-notation` cause), is the merge's. **Refs:** `refs.py` read every xfx
`file:line` reference in the documents against the code it was written about, on the merge
before any re-point: 7 moved, all main's refs into `shell.rs`, which this branch grew above
them (the ledger's `shell.rs:37424` -> `:37813` five times and `:12272` -> `:12634`; capfile.py's
`warning_shown` `:13705-13730` -> `:14083-14108`), re-pointed in `c711f016` with capfile's second
ref by hand (`:13723-13725` -> `:14101-14103`); and one it read against the wrong frame, the
ledger's `docs/parity.md:241`, which is the colored row (`:243` here), fixed in `1fe55854`.
**The timer lint** (capfile.py, main's): 346 units, 0 races, `stopnext` included. The one gate
failure of round 1, `test_models2_and_modelsdelay`, is main's timed form, which FLAKE-HINT4
replaced with an order-based one (`gates.txt`, round 2).

## Round 2: the reviews (`red2-test.txt`)
### MUST 1: a real failure after a stop (gpt6 1)
**fx** (`fx-src.txt`, round 2): a stopped turn is finished `.interrupted` and then still
returns its propagation error (`interruption.zig:208-223`); an error that is not the
cancellation is returned with the cancel flag set (`orchestrator.zig:5957`); the error leaves
the worker as `request failed` under `system` (`app_process_runtime.zig:157-169`), which a
cancelled drain admits (`.error_text => .admit`, `app_worker_runtime.zig:386`) and which is
`error` (`noteFailure`, `:1215`). So fx: the keystroke's `▪ Cancelled`, then `✗ system: request
failed: …` and `error`.
**xfx before:** under `cancelling` every end was quiet, so a real failure was swallowed (RED,
one drain: `[working, idle, None]`, no line; the two-drain half let it through only because the
base's settle cleared the cancel).
**Fix** (`96645f92`): the runtime ends a stopped turn in one word, `bridge::TURN_CANCELLED`,
whichever layer read the cancellation (the machine's `Cancelled`, or a send the turn's token
refused, `Sink(Interrupted)`), and in the log's failure when the log would not take the stopped
turn -- xfx's propagation error (`end_of_turn`, `worker.rs:6112`, with its own table test). The
band, after the user's stop of a turn (`stop_reads_end`, set at `shell.rs:12365`), is quiet only
for that word (`quiet`, `:4478`); any other failure is announced and is `error`.

### MUST 2: "the stop wins" was not fx (gpt6 3, opus SHOULD 1)
**fx:** the keystroke writes the `Cancelled` line whenever its stream is active
(`cancelActiveOperation`, `input_interrupt_runtime.zig:91-127`) -- and the stream is active until
the UI drains the turn's end, so **yes, it writes the line once the runtime has finished**. But
the turn was finished `.completed` and stays so: a second finish is ignored
(`finalization.zig:126-136`); the batch that carries the finish is no interrupted segment -- its
`turn_finished` is `.completed` (`batchSegmentIsInterrupted`, `app_worker_runtime.zig:313`), and
the worker holds no turn by then (`takeEventBatch`'s `cancelled_turn_id`, `worker_runtime.zig:
930-935`) -- so the summary is written (`finishPromptPresentation`, `app_callbacks.zig:276-283`)
and the result is `done` (`program_status.zig:62-64`).
**Fix** (`96645f92`): `None` after the user's stop is the turn's finish: the summary, `done` and
the title, under the keystroke's line as on fx. RED: `[working, idle, None]`, no summary, no title.
A compaction's ending or its stop keeps the end quiet (`stop_reads_end` cleared at
`:11185-11186`, `:11218-11219`), pinned by `a_stop_a_compaction_then_ends_…` (mutant 17).
**Found on the way:** the runtime counted a compaction that compacted nothing (`NoOp`) or found
another running (`Busy`) as having ended the turn, so a later stop or failure ended in `None`;
`end_of_turn` now counts only an ending, as the band does and as fx's turn goes on after a no-op
compaction (`orchestrator.zig:7934-7937`).

### MUST 3: mutant 8 was not equivalent (opus MUST)
Opus's chain holds: after an automatic compaction's `error`, the steer the failed turn hands on
decides, by its marker, whether its `working` is the end's tick or waits for its turn's start.
`a_steer_a_failed_compactions_turn_hands_on_is_reported_when_that_turn_ends` (event_loop, opus's
recipe; the claim is the tick, as the start's frame still shows the compaction's failure on the
row) passes at 91f03d71 and kills the mutant (mutant 10). The comment at `shell.rs:12859-12866`,
this README and `mutants.txt` say so.

### MUST 4: a dropped prompt (opus SHOULD 6)
A prompt typed behind a running turn, the turn ended by the runtime and its place given back, a
stop the runtime reads before it takes the prompt: it drops the prompt (`Taken::Abandon`,
`worker.rs:3306`, `:3355`) and says nothing. The base held its stale entry silent; round 1's
marker made it `working` with nothing running (RED at 91f03d71: `[idle, working, idle,
working]`). **This unit's regression; fixed** (`8999d4b2`): the marker counts only while the
runtime holds a place (`self.work.outstanding() > 0`, `shell.rs:10299`). The stale entry itself
is STOP-NEXT's: it keeps the status at work, held, until the next start, and so loses the `done`
of a turn that had finished before the stop.

### The SHOULDs
- Item 1's happy path is scoped to a turn whose start was read; its window takes in the
  keystroke settles (above).
- "xfx's side of fx's two branches" is narrowed: the pacer and the full-transcript branches are
  read now, each with a RED (`a_prompt_sent_while_the_last_answer_is_still_paced_out_…`,
  `a_prompt_sent_from_under_the_full_transcript_…`: `NotYet` where `AfterFrame` is owed);
  `turn_start_held` and `session_transition_held` are STOP-NEXT's.
- `stopnext.cap`'s header says fx's order (`10481b46`).
- **The OSC 2 title** (STOP-NEXT's list, "fix here if small and safe"): small -- a stopped turn
  whose start was read now titles the session (`:4565`), as fx commits it `.interrupted` and the
  capture's `]2;first question` shows; a turn a compaction failed, or one whose end is a real
  failure, titles nothing, as before. RED: `(None, None)` for `(Some("first question"), None)`.
  On `stopnext` the title text is now fx's; its place against the prompt's `working` is not
  (xfx: `working`, then the title; fx: the title, then `working`) -- STOP-NEXT.

## Decisions
- **Item 2 follows the capture, not the brief** (round 1; the dispatcher accepted it). The brief
  allows a divergence only for a user decision, an unsafe copy, a later pin or a platform limit;
  none applies, and the oracle is three fx runs with one order. The brief's shape is kept as
  `prev/` so the reversal is checkable.
- **The marker reads what fx reads**, not "typed under a stop": a started turn in hand, the
  pacer, the full transcript. A prompt queued behind a running turn is fx's queued prompt too, and
  its report is that turn's `working` either way.
- **A stopped turn's end is the runtime's word** (round 2, replacing round 1's "the stop wins"):
  the stop's own end is quiet, a failure after it is reported, a finish before it stands -- as fx's
  stop rewrites no finish. The text of a finished turn read after the keystroke is still dropped
  (STOP-NEXT).
- **The stop's own end is one constant**, set by the runtime from the typed error, not a pattern the
  band matches: the band compares against the word the runtime chose.
- **No osc.py claim was weakened.** Two were added (`activity`, `stable`); `order`'s `Thinking`
  claim already separated `prev/` from fx.

## STOP-NEXT's scope (owner: STOP-NEXT; recorded in the OSC 7501 row's "Where xfx differs")
Each is older than this unit or lies outside what one keystroke's bookkeeping can know:
- **next-break** (`next-break.txt`): a stop pressed after the runtime has taken the next prompt;
  that prompt's stopped turn is painted and ends `working`, `error`, `✗ system:` (round 2's head
  too).
- **The covered start** (opus §1): a settle that runs while a covered prompt's `TurnStarted` and
  `TurnEnded` are both still unread clears the cancel (`shell.rs:10080`), and the start then reads
  as a new turn (`:4115`).
- **A dropped prompt's stale entry** (MUST 4, above).
- **The same-read card**: xfx's `Cancelled` frame shows the card of a prompt typed in the same read
  as the stop (`s03-exit`, `osc-stopnext.txt`: on `xfx/` `Cancelled`, then `second question`, then
  `working`); fx shows nothing of it until its worker takes it. The report order is the same.
- **The title's place**: on `stopnext`, xfx writes the stopped turn's title after the prompt's
  `working`; fx before it.
- **A finished turn's text read after the keystroke**: dropped by xfx (`Delta` under a cancel),
  presented by fx's drain, as its segment is no interrupted one [read from the code].
- **A failed turn's title** (found by code mutant 20): fx keeps a failed turn that carries a partial
  answer or a completed step in the history (`persistFailedPartialTurnOnce`,
  `persistFailedTurnWorkOnce`, `interruption.zig:226-293`), and the session takes its title from it
  [read from the code]; xfx titles no failed turn. Older than this unit.
- **A start held by another waiting prompt or a session transition** (`worker_runtime.zig:
  1276-1277`): fx queues such a prompt at once; xfx holds it for its turn's frame.
All need the band to know which turn a start or an end belongs to -- `TurnStarted` carrying the
submission number the runtime runs, against the last cancel's `through` -- or a capture of fx in
a window no scenario reaches yet.

## The checker (`osc.py`)
L-U1+U2's `osc.py` with these changes:
- `stopnext` is read from this receipt's `fx/`. Every other scenario is read from L-U1+U2's `fx/` (the
  same pin; `../osc-program-status/fx/`).
- `s03-exit` is an exit label.
- Two claims are new:
  - `activity`: for every `working` on a scenario this receipt captured on fx, whether a frame
    since the report before it showed `Thinking` or `Generating`, as on fx.
  - `stable`: every further fx run has `fx/`'s reports, which-frame and activity answers.

## RED (`red-test.txt`, `red2-test.txt`)
**Round 1:** the tests of `220f9361` over `e636db48`'s code, on a `--no-local` clone. 4 of 5 fail.
- `a_stopped_turn_whose_place_comes_back_before_its_end_is_read_ends_as_the_stop_said`:
  `([working, idle, None, error], true, 1)` against `([working, idle, None, None], false, 1)`.
- `a_stopped_turn_with_a_call_running_keeps_its_line_and_its_idle_through_a_late_end`:
  `([working, idle, working, error], true, false)`. That is `working` again, `error`, a `system`
  line and no `■ Cancelled`.
- `a_prompt_typed_before_a_stopped_turn_ends_is_reported_ahead_of_the_frame_that_shows_its_turn`:
  `one drain false`. The split was held behind the `Thinking` frame.
- `a_reset_before_a_failed_compactions_turn_ends_takes_that_result_with_its_conversation`: all
  three cases are `None` where `working` is owed.
- The fifth test, `a_prompt_queued_behind_a_running_turn_writes_nothing_new_when_its_turn_starts`,
  guards item 2's discriminator. It passes there too, as it should.

**Round 2:** the tests of `4abcc8c4` over `91f03d71`'s code (the branch after the merge), on a
`--no-local` clone. 6 of 7 fail.
- MUST 1, `a_stopped_turn_whose_end_carries_a_real_failure_reports_it_as_upstream_does`:
  `([working, idle, None], false, 1)` for `([working, idle, error], true, 1)`. There is no `error`
  and no `✗ system:` line.
- MUST 2, `a_stop_read_before_a_completed_turns_end_takes_the_runtimes_word`:
  `([working, idle, None], false, 1, None)` for
  `([working, idle, done], true, 1, Some("first question"))`. There is no `done`, no summary and no
  title.
- The title, `a_stopped_turn_whose_start_was_read_titles_the_session_as_upstream_does`:
  `(None, None)` for `(Some("first question"), None)`.
- The SHOULD, `a_prompt_sent_while_the_last_answer_is_still_paced_out_…` and
  `a_prompt_sent_from_under_the_full_transcript_…`: `NotYet` for `AfterFrame`.
- MUST 4, `a_prompt_the_runtime_drops_after_a_stop_reports_no_work`: `[idle, working, idle,
  working]`. The last `working` comes with nothing running.
- The seventh, MUST 3's `a_steer_a_failed_compactions_turn_hands_on_…`, passes there. Round 1's code
  already marks the steer. The test fails under the mutant that unmarks it (code mutant 10).

Two guard tests came after the fixes. Each one fails only under its mutant:
- `a_stop_a_compaction_then_ends_…` (`2b21b797`, mutant 17).
- `a_failed_automatic_compactions_turn_titles_nothing_and_a_stopped_ones_does` (`d4ce0e0c`,
  mutant 20). The mutant run found that this test was missing.

## GREEN
- **The unit tests** pass at `2b21b797` (`gates.txt`, round 2: 5334 passed, 0 failed). At
  `d4ce0e0c`, every TUI library test passes on a `--no-local` clone (2774 of 2774, the added one
  included). `fmt` is clean, and `clippy` is clean without and with `fault-injection`.
- **Wires** (`osc-all.txt`): every osc scenario, captured on three builds:
  - `red/`: the base, `e636db4`;
  - `base/`: `1230329`, the base with main `884497c0` merged in (L-U1+U2 and main, without this
    unit). This is a throwaway merge in scratch, tree `bb036b01`. Its `--remerge-diff` is empty
    for `src/` (its one conflict was `docs/parity.md`), so its build is its two parents' code;
  - `xfx/`: the head, `2b21b79`.

  Each build has 47 wire pairs and **0 failing claims**, and every pair has the same report sequence
  on all three builds.
- **`stopnext`, three runs per build** (`osc-stopnext.txt`):
  - fx's three runs agree (`stable`).
  - `red*`, `base*` and `xfx*` have 0 failing claims on each run.
  - `prev*` (`997d7b1f`) has 6 failing claims on each run (`order` and `activity`, `s01`-`s03`).
  - The head's wire now carries the stopped turn's title, `]2;first question`, with fx's text. Its
    place differs. On xfx it comes after the prompt's `working` (@1157 against @1127). On fx it comes
    before (@1404 against @1423). That place is STOP-NEXT's.

## Wire mutants (`wire-mutants.txt`, `wire_mutants.py`)
At `2b21b79`, `osc.py` passes the unchanged copy and fails each of 5 mutants of `stopnext`'s wires:
- The prompt's `working` is moved behind the frame that shows its `Thinking` (round 1's order), on
  `s01` and on `s03`. `order` and `activity` each fail.
- `error` is written after the stop's `idle` (item 1's wire). `same` fails.
- `working`, then `error` after it (item 1's wire with a call running). `same` and `activity` fail.
- One fx run's `working` is moved behind the prompt's frame. `stable` fails.

## Code mutants (`mutants.txt`, `mutants.py`)
23 mutants ran on a `--no-local` clone at `2b21b797`. Each ran against every TUI library test
(`tui::`) inside the round-2 slot: **21 killed, 1 survived, 1 did not build.** After the fixes
below, all **23 of 23 are killed**.

**20 survived.** That mutant drops `!noted`, so a turn that an automatic compaction failed titles the
session. A failed automatic compaction settles the band as stopped, just as a stopped one does, and
no test told the two apart. `d4ce0e0c` adds the test that does.

**Mutant 21 did not build in the slot's run.** Its replacement made `if false if …`, and the runner
counted the build failure as a kill. Its replacement is now `if false && …`. The runner now reports
a mutant that does not build as NOCOMPILE, which counts as no kill. No other row of the run was a
build failure.

Mutants 20 and 21 then ran again at `d4ce0e0c`, in a second lock slot, with the same runner and
command. **Both are killed.** Mutant 20 is killed by the new test. Mutant 21 is killed by the
runtime's table test (`a_turns_end_says_a_stop_in_one_word_…`). The same clone, unmutated, passes
2774 of 2774. `d4ce0e0c` adds only a test, so the other 21 kills from `2b21b797` still stand: each
was a failing test, and the added test changes none of them.

The rows, by what they break:
- Item 1 (1-3): the base's settle, the guard inverted, and the settle's clear deleted.
- Item 2 (4-12):
  - the base's hold;
  - no prompt marked, or every prompt marked;
  - marked by running activity;
  - the pacer, or the full transcript, not read;
  - a steer handed on unmarked: round 1's mutant 8, now 10, killed by MUST 3's test;
  - a dropped prompt still marking work;
  - round 1's start-holds shape.
- Item 3 (13-14): the base's reset, and the mark dropped by the drained end.
- The end of a stopped turn (15-20):
  - every end quiet (round 1);
  - the stop not marking the end;
  - a compaction leaving the mark;
  - a real failure read as the stop's own;
  - no title (the base);
  - a failed compaction's turn titled.
- The runtime's word (21-23):
  - a refused send's stop;
  - the log's failure dropped;
  - a no-op compaction ending the turn (the base).

## Sweep (`sweep.txt`)
The sweep ran every `p5-lifecycle-*` scenario (osc and lifecycle) inside the round-2 slot. It ran
fx v0.0.16 against the merged base's debug build (`1230329`) and against the head's (`2b21b79`).
- **Each side has 69 pairs, and 0 changed.** Both sides have 28 normalized-identical, 40
  unresolved and 1 approved cascade.
- 0 failed and 0 timed out.
- No pair drops.

The osc scenarios take no snap. Their evidence is the wire (above).

## Gates (`gates.txt`)
**Round 2,** at `2b21b797`: the dispatcher's list ran inside one suite-lock slot (04:26:57Z to
05:13:23Z), together with the sweeps, the captures and the mutants.

**The runner's `rc` column is not evidence.** Its step helper read `$?` after a command substitution
in the same `echo`, so it printed `rc=0` for every step. Each result below was read from the step's
own log (`gates.txt` shows the check). The capture loop read `$?` first, so its `rc` is real.

| gate | result |
|---|---|
| `cargo test --locked --all-targets --no-fail-fast` | 5334 passed, 0 failed, 7 ignored (14 binaries) |
| `cargo test --locked --features fault-injection --test tui`, `--lib` | 192 passed; 4444 passed, 0 failed, 7 ignored |
| `scripts/fxcap/test-sweep.sh` | 66 + 26 tests OK |
| `scripts/fxcap/test-capture.sh` | **57 of 59**: `test_hang_holds_until_the_mode_changes` and `test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run` fail (below) |
| release and faulty release builds | ok |
| `scripts/smoke-tui.sh` (release, `--faulty`) | 37 scenarios + the oracle, 1168 checks, 0 failures |
| the 32 captures | 32 of 32 exit 0 |

**The two test-capture failures are not this unit's.** Both are main's harness tests:
- `test_capture.py`, `capture.sh` and `mcphttp.ts` are identical at `884497c0` and `2b21b797`.
- Neither test runs xfx. One runs a `/bin/sh` wrapper, the other the fake MCP server alone.

Each test then ran alone, in five interleaved pairs on the head and on the merged base:
- The pid test failed 2 of 5 on the head and 3 of 5 on the base.
- The hang test failed 0 of 5 on both.

**Round 1,** at `e27f9bcf`: every step of `ci.yml` and `sweep.yml`'s two test scripts, one at a
time:
- `fmt`, `clippy` ×2, the three `cargo test` runs (5272, 192 and 4382 passed, 0 failed) and every `check-*` and
  `test-*` script were ok.
- `smoke.sh` ran 47 checks and `smoke-tui.sh` ran 1168 checks, with 0 failures on each.
- `test-capture` was 41 of 42. `test_models2_and_modelsdelay` failed 10 of 10 on base and head alike
  under load. FLAKE-HINT4 on main has since replaced that timed test.

## Look ahead: the next break (`next-break.txt`, not fixed here)
**The case.** The runtime ends a turn and takes the prompt queued behind it. The user presses stop
before the UI reads either event.

**What xfx does.** The keystroke counts that prompt as covered (`cancel_covers = 1`). The first
turn's `TurnEnded` zeroes the count and takes the cancel. The runtime does stop the second turn, but
xfx paints it, and it ends `working`, `error`, `✗ system:`. A scratch test shows this. It fails on
`c389ba9d`, and it still fails on `2b21b797`, unchanged by round 2.

**Why.** The count guesses which turn a start belongs to. A prompt the runtime dropped on reading the
stop looks exactly like one it took.

**The fix.** `TurnStarted` carries the submission number of the turn it starts, and the band
compares that with the last cancel's `through`. This changes the event every TUI test applies.
fx's word for this schedule was read from the code and not captured. Both the capture and the fix
are left to their owner unit, **STOP-NEXT**.

## Files
- `red-test.txt`, `red2-test.txt`: RED, rounds 1 and 2.
- `fx-src.txt`: every fx citation, read at the pin.
- `refs.py`, `refs.txt`, `remerge.txt`: the merge.
- `osc.py`: the checker.
- `osc-all.txt`, `osc-stopnext.txt`: GREEN and the reversal.
- `wire_mutants.py`, `wire-mutants.txt`, `mutants.py`, `mutants.txt`: the wire and code mutants.
- `sweep.txt`, `gates.txt`, `next-break.txt`.

The wires are in:
- `fx*/`: fx v0.0.16, `stopnext` only.
- `red*/`: `e636db48`.
- `base*/`: `1230329`, the merged base.
- `xfx*/`: `2b21b797`.
- `prev*/`: `997d7b1f`.

Every committed file is redacted: it holds no home-directory path and no user name.
