# P5 STOP-NEXT receipt: which turn a start belongs to

**S** (terminal escapes: the OSC 7501 wire, the window title). Spec: STOP-LATE-EVENT's receipt, its section
"STOP-NEXT's scope" (`../stop-late-event/README.md:424-462`), its `next-break.txt`, `fx-src.txt` and `osc.py`,
and the reviews of its rounds 2 and 3 (zbrain `p5/STOP-LATE-EVENT/reviews/r2-*.md`, `r3-*.md`).

- **Base:** STOP-LATE-EVENT's closed head `fix/p5-stop-late-event` `af4fbacf`, with main `690aa505` merged in:
  `8c5e9e16` (below, "The merge"). Every capture and sweep called `base` is a debug build of `8c5e9e16`, from a
  `--no-local` clone at that commit (banner `v0.1.0-8c5e9e1`).
- **Code:** `9712d8b8`, the commit every `xfx/` capture's and the head sweep's banner names (`v0.1.0-9712d8b`). Its
  last code change is `01212cf0`; `9712d8b8` changes the parity row, the ledger, `capfile.py` and one comment ref in
  `shell.rs`. After it come `107168f5` (the parity row), `d6be6c30` (tests only) and the receipt's three commits,
  none of which changes code; the gate ran at `c8666457`, the receipt's first commit (below).
- **Commits** (`git log 8c5e9e16..`):
  - `a2995eea` the protocol change, no behaviour change: `UiEvent::TurnStarted { submission }`, the band's waiting
    prompts a struct with their number, two tests of the number;
  - `8d074c47` **RED**, tests only (`red-test.txt`);
  - `61f35ffb` items 1-3 (one mechanism: the stop's count);
  - `8709876e` item 4; `c2b71ed6` item 5; `0d021cd0` item 6;
  - `c1c2fbd0` item 5's guard steps aside for a manual compaction or a provider switch typed since (found writing
    item 5's mutants: the guard had made both unstoppable), with item 4's `shown` guard test;
  - `8ee259ae` **RED** for item 6's other half (`red2-test.txt`); `01212cf0` its fix;
  - `9712d8b8` the parity row and the refs the code moved; `107168f5` the parity row's one-title residual;
  - `d6be6c30` tests only: a guard for each mutant the first mutant run left;
  - the receipt: `c8666457` (the gate ran here), `47a33759` (the gate's record), and a last commit that quotes four
    fx ranges this receipt cited whole into `fx-src.txt` (`input_interrupt_runtime.zig:69-90`, and the gaps in
    three quoted in parts) and corrects this list.

## The merge (`remerge.txt`)
`git merge --no-ff 690aa505` on `af4fbacf`. main's side since L-U1+U2 (`e636db48`, which both sides hold)
brings the clipboard, `/copy` and `/trace` changes (`src/clipboard.rs`, `src/tui/copy.rs`, `src/tui/trace.rs`);
`src/tui/shell.rs` is `af4fbacf`'s byte for byte. Three conflicts, each taken from STOP-LATE-EVENT's side:
- `docs/parity.md`, the OSC 7501 row: main's row is `e636db48`'s, which STOP-LATE-EVENT rewrote;
- `scripts/fxcap/capfile.py`, `scripts/fxcap/parity-ledger.toml`: `src/tui/shell.rs` line refs. Both sides name
  the same code (`warning_shown`, `send`, the `tools:` record line, the `/stats` line); since `shell.rs` is
  `af4fbacf`'s, STOP-LATE-EVENT's numbers are the merged file's.

`remerge.txt` is `git show --remerge-diff 8c5e9e16`.

## Design

### The question
STOP-LATE-EVENT's list ends: the items "need the band to know which turn a start or an end belongs to". At the base
the band guesses. At a stop it counts the waiting prompts it still holds (`cancel_covers`, `shell.rs:12404` at
`8c5e9e16`) and treats that many later starts as the stopped work starting late. The count cannot tell a prompt
the runtime took before it read the stop from one it dropped on reading it (`../stop-late-event/next-break.txt`,
"Why"), and any `TurnEnded` or settle in between zeroes it (`:4562`, `:10112` there).

### What is already exact
- **One index space.** The UI's `WorkHandle::accepted` counts the submissions the work channel took
  (`worker.rs:445-448`; the channel has one sender, `submit`, which `submit_switch` goes through). The runtime's
  `Queue::taken` counts what it removed from that channel, by taking (`Queue::took`) or by dropping
  (`Queue::abandon`), in the channel's order (`worker.rs:3689-3716`). So the `n`-th submission is the `n`-th
  taken. A stop already quotes this index: `Cancel { through: accepted() }`, and the runtime drops exactly the
  waiting items numbered `<= through` (`Queue::abandon`). Pinned on both sides:
  `a_turns_start_names_its_submission_past_the_work_a_stop_dropped` (worker: a listing and a prompt dropped, the
  one start is `3`) and `a_waiting_prompt_is_numbered_as_the_runtime_numbers_its_turn` (shell: a prompt behind a
  turn, one behind the second turn, a handed-on steer: `2`, `3`, `4`).
- **A start always arrives.** `run_turn` sends `TurnStarted` through `send_ui` on the turn's own token before it
  races the control channel (`worker.rs:3786-3790`, the race at `:3809`). That token is a child of the session's
  root; a stop raises the turn's mirror flag and cancels the token only inside the race (`bridge.rs:1330-1340`,
  `worker.rs:3810`). So only the session's end can drop a start, and `TurnEnded` needs no number: it ends the turn
  whose start came last.

### The protocol change
`UiEvent::TurnStarted { submission }`: `run_turn` sends `queue.taken`, the number of the work item it runs
(`a2995eea`). Nothing else in the protocol changes. A fence for the prompts the runtime drops was considered and is
not needed: the stop's count already says which prompts it covers (item 3).

### The band
- Each waiting prompt (`WaitingPrompt`, `shell.rs:1932`) carries its submission number, read from `accepted()`
  right after `submit` returned `Ok`, at both places that queue a prompt (`send`, `hand_on_steers`).
- A stop records the `through` it sends (`cancel_through`, `shell.rs:1063`), at both senders: `cancel_turn_with`
  (`:12539`) and `stop_compaction` (`:11370`).
- **A start is covered iff `submission <= cancel_through`** (`TurnStarted`, `:4188`). Such a turn was taken by the
  runtime before it read the stop, which stops it, or it had already finished. It re-enters the stop
  (`cancelling`, `stop_reads_end`, no bell) whatever ended or settled in between, writes its prompt's card, and
  ends as its end says -- STOP-LATE-EVENT's `TurnEnded` arm already reads an end under the user's stop. A start
  past `through` is a new turn and takes the stop back.
- **On a start, the prompts numbered below it are gone** -- the runtime takes in order, so each started earlier or
  was dropped -- and the one numbered it is the turn's.
- **A prompt the stop covers is no longer waiting**, by derivation, not a flag (`waiting_prompts`, `:9020`): off the
  band, and no work for the status, a provider switch, `/compact` or a model change. Its entry is kept for the
  turn it may still start and goes with the next start, a reset or a resume. This replaces the keystroke's
  `prompts.clear()` and the `cancel_covers` count, which is deleted.

### Per item

| # | item | mechanism | closes? | fx | captured / read |
|---|---|---|---|---|---|
| 1 | next-break: a stop after the runtime took the next prompt | the covered start re-enters the stop; its card is written | yes (`61f35ffb`) | the take pushes the prompt's `begin_prompt` and clears the stop (`worker_runtime.zig:1484-1512`); a stop then requested is that turn's (`takeEventBatch`, `:923-937`); that turn's segment admits `begin_prompt`, `turn_finished` and `finish_prompt` and drops the answer (`app_worker_runtime.zig:301-325`, `:339-389`, `:920-946`); `begin_prompt` writes the card with no check on the stop and notes a new turn (`:965-975`, `app_callbacks.zig:1431-1434`, `main.zig:1297-1299`, `:799-801`, `program_status.zig:57-60`); the turn's history adds no transcript entry (`app_session_runtime.zig:2581-2595`, `:2635-2645`, `:2741-2795`), so one `Cancelled` line, the keystroke's; the turn before it holds its own `turn_finished(.completed)` and is presented whole: `done` (`program_status.zig:62-64`) | read only: the window is the runtime taking the next item before the UI's next drain, microseconds on both |
| 2 | the covered start: a settle takes the stop back with the start and end unread | coverage reads the number, not `cancelling` | yes (`61f35ffb`) | as item 1 | read only (the same window) |
| 3 | a dropped prompt's stale entry | the stop's count covers the entry: no work, not in the band | yes (`61f35ffb`) | fx's stop drops no queued prompt (`requestInteractiveCancel`, `worker_runtime.zig:753-773`; the take clears the stop, `:1512`), so its prompt would run next: the drop is xfx's own (`docs/parity.md`, the full-screen TUI row). What is left is the earlier turn's `done` (item 1) | read only |
| 4 | the same-read card on the Cancelled frame | a prompt fx would queue at once, with no turn of its own running, keeps its card, unshown, for its start (`shell.rs:12983`) | yes (`8709876e`) | a prompt that cannot install a pending submission is queued at once (`input_submit_runtime.zig:1103-1149`); the band lists only steering prompts (`snapshotSteeringPresentation`, `worker_runtime.zig:1749-1792`); the card is the `begin_prompt`'s (item 1) | **captured**: `p5-lifecycle-osc-stopnext` (fx's three runs, `../stop-late-event/fx*/`: the `Cancelled` frame has nothing of the prompt); `osc.py`'s new `prompt` claim |
| 5 | a stop while the cancel is still requested | `stop_requested` (`shell.rs:1076`): set by a stop, taken back by a start past `through`, by nothing left in hand, a reset or a resume; while set, the Ctrl-C and Escape-pair stop returns (`cancel_turn`, `:12483`), save for a manual compaction or a provider switch typed since | yes (`c2b71ed6`, `c1c2fbd0`) | `if (app.worker.isCancelRequested()) return;` (`input_interrupt_runtime.zig:94`), after the provider-preparation and compaction branches (`:69-90`, quoted in `fx-src.txt`'s last section); set by the keystroke (`worker_runtime.zig:753-773`), cleared at the next take (`:1512`) | read only: a second Ctrl-C inside the stopped turn's last moments |
| 6 | the leading title takes whatever title is owed | the lead is set only when the stopped end itself gave the session its title (`:4706`), and any later title in the tick takes it back (`retitle`, `:15347`) | yes (`0d021cd0`, `01212cf0`) | a title is derived from history only while the session has none (`ensureCachedSessionTitle`, `app_session_runtime.zig:2997-3010`) | read only: no scenario reads a model's title and a stopped end in one drain |
| 7 | a stop and its turn's end in one tick | -- | STOP-NEXT-2 | | |
| 8 | a finished turn's text read after the keystroke | -- | STOP-NEXT-2 | | |
| 9 | a failed turn's title | -- | STOP-NEXT-2 | | |
| 10 | starts held by `turn_start_held` or `session_transition_held` | -- | STOP-NEXT-2 | | |

Items 7-10 are each a mechanism of their own, not a use of the number ("STOP-NEXT-2", below).

### Decisions
- **No fence event for dropped prompts.** The stop's count says which waiting prompts it covers, so the band
  derives "no longer waiting" from it; a covered entry that never starts is swept by the next start, a reset or a
  resume, and is neither shown nor work meanwhile. One event less, and no second copy of the fact.
- **A covered start writes its card.** fx's interrupted segment admits the prompt's `begin_prompt`
  (`app_worker_runtime.zig:373-387`) and its handler writes the card with no check on the stop. The base wrote no
  card for a start its count covered.
- **A covered start rings no bell.** The keystroke's `sound.interrupted()` is a one-shot the turn before it takes
  at its end (`sound.rs:170-180`); the covered turn is the stop's too, and fx's turn-end cue is silent for an
  interrupted turn.
- **Item 4 covers every "queued at once" prompt with no turn of its own running**, not only the stop window: the
  pacer and the full-transcript cases go through the same fx branch (`installPendingSubmission`'s `.unavailable`,
  `input_submit_runtime.zig:1138-1147`). The one existing test that looked for such a card at the keystroke (the
  full transcript's keys) now starts the turn first.
- **Item 5's guard holds only a turn's stop.** fx stops a manual compaction and a provider's preparation ahead of
  its check (`input_interrupt_runtime.zig:69-90`); without the exception, a `/compact` or a provider switch typed
  after a stop could not be stopped until the stopped turn's place came back, and a second press would leave the
  session. A turn typed after a stop is stopped once its start is read; before that fx's keystroke returns too
  (`:91`, the stream inactive).
- **Item 6: one title a tick.** When a model's title arrives in the same drain as the stopped end, after it, xfx
  writes that title, behind the report; fx writes the end's title on the pass that drains it and the model's on a
  later one. The band writes one title per tick, as it writes the launch title once where fx writes it twice. The
  difference is in the parity row's STOP-NEXT-2 list.
- **Item 10 goes to STOP-NEXT-2 whole**, though the brief listed it with the items the protocol change closes
  directly. The number says which turn a start is; neither hold is about that. `turn_start_held` needs fx's submit
  path under a held start, which is not traced (the assert at `input_submit_runtime.zig:1148` says a second pending
  submission never happens, not what fx does instead); `session_transition_held` is half of a key-deferral mechanism
  xfx does not have (STOP-NEXT-2, item 4).
- **The ledger:** no cause row changed (the osc scenarios take no snaps, and every sweep pair is unchanged on xfx's
  side); only refs this unit's code moved were re-pointed.

## RED (`red-test.txt`, `red2-test.txt`)
**`8d074c47`'s tests over `a2995eea`'s code** (the protocol change, no behaviour change), in the worktree at
`8d074c47`, tree clean. 6 of 6 fail:
- item 1, `a_stop_pressed_after_the_runtime_took_the_next_prompt_is_that_turns`: `([working, idle, working, error],
  true, true, 1, true)` for `([working, idle, done, idle], false, false, 1, true)` -- the stopped turn's answer and
  a `system` line, as in `next-break.txt`;
- item 2, `a_covered_start_read_after_a_settle_cleared_the_stop_is_still_the_stops`: `[(true, true, false, true),
  ...]` for `[(true, false, false, true), ...]` -- a `system` line. Its second case (the covered turn had finished)
  passes on the base too: a guard;
- item 3, `a_turn_that_finished_before_a_stop_that_dropped_the_prompt_behind_it_is_done`: `[idle, working, idle]`
  for `[idle, working, idle, done]` -- the finished turn's `done` lost to the stale entry;
- item 4, `a_prompt_typed_in_the_stops_read_shows_nothing_of_itself_until_its_turn_starts`: `(true, true, false)` for
  `(true, false, true)` -- the prompt on the stop's frame;
- item 5, `a_second_stop_while_the_first_is_requested_does_nothing`: `[("a second stop", 2, true, 0), ("after a
  compaction's stop", 1, true, 0), ...]` for `[("a second stop", 1, false, 1), ("after a compaction's stop", 0,
  false, 0), ...]` -- a second line, a second stop, the prompt typed between dropped. Its third case (after a
  compaction's failure, the press is a stop of its own) passes on the base too: a guard;
- item 6, `a_title_another_event_owed_in_a_stopped_turns_tick_follows_its_report`: the model's title ahead of
  `working`.

**`8ee259ae`'s test over `c1c2fbd0`'s code**: item 6's other half, the model's title read after the stopped end, ahead
of `working` (`red2-test.txt`).

**Guard tests added with the fixes**, each failing only under its mutant (below): the number on both sides
(`a2995eea`); `work_typed_after_a_stop_that_is_not_a_turn_is_still_stopped_by_the_next_press` and
`a_prompt_queued_at_once_under_a_manual_compactions_row_is_not_shown_waiting` (`c1c2fbd0`).

## GREEN
- **The unit tests** (`green-test.txt`): the six REDs and the four guards pass at `9712d8b8`. The suites: "Gates".
- **Existing tests changed** (each named in its commit): the two about which turn a start is
  (`a_prompt_the_cancel_dropped_before_it_started_...`, `a_cancel_that_landed_after_its_turn_ended_...`) name the
  start's number; `the_ctrl_c_that_stopped_one_turn_does_not_end_the_session_on_the_next` has the second turn say it
  started before its press (fx's keystroke returns before that too); the full transcript's key test starts the
  turn before it looks for the card. Every other test that applied a bare start applies the oldest waiting
  prompt's no stop covered (`Shell::start_turn`, test-only).

- **Wires** (`osc-all.txt`): every osc scenario fx has a wire of, on two builds, `base/` (`8c5e9e1`) and `xfx/`
  (`9712d8b`): 47 wire pairs each, with the same report sequence on both builds on every pair.
  - **The head has 0 failing claims.**
  - The base has 3, all `prompt` on `stopnext`'s three post-stop labels: the prompt typed in the stop's read is on
    the frame before the next `working` (`[F, F, F, T, F]` for fx's `[F, F, F, F, T]`).
  - (`p5-lifecycle-osc`, which has no fx wire, is captured on both builds and compared by the sweep's snaps only.)
- **`stopnext`, three runs per build** (`osc-stopnext.txt`): `xfx*` 0 failing claims on each run, `base*` 3 on each
  (`prompt`). fx's three runs agree (`stable`, with the prompt answers). On every head run the wire is fx's order
  after the stop: `idle`, the `Cancelled` frame, `OSC 2 first question` (@969), `working` (@988), then the frame with
  `second question` (@1070) and `Thinking` -- fx: @1404, @1423, @1480. On the base, `second question` is on the
  stop's frame (@987), ahead of the title (@1127) and `working` (@1146).
- **What no capture reaches** (items 1-3, 5, 6): the unit tests are their receipt. Items 1-3 lie in the window
  between the runtime taking the next item and the UI's next drain; item 5 needs a second press inside a stopped
  turn's last moments; item 6 a model's title and a stopped end in one drain.

## The checker (`osc.py`, `wire_mutants.py`)
STOP-LATE-EVENT's `osc.py` with these changes:
- fx's wires are the earlier units' at the same pin: `stopnext`'s three runs from `../stop-late-event/fx*/`, every
  other scenario's from `../osc-program-status/fx/`. The fx binary (`8e6cda2e…`, checked) and the scenario files are
  unchanged, so fx is not captured again.
- One claim more, `prompt`: on `stopnext`, for every report, whether a frame since the report before it showed the
  prompt typed in the stop's read (`second question`), as on fx; `stable` compares it too. On STOP-LATE-EVENT's
  head wires (`../stop-late-event/xfx/`, `06ca9f1`) it fails on three labels: `[F, F, F, T, F]` (the prompt on the
  stop's frame, before the next `working`) for fx's `[F, F, F, F, T]`.

**Wire mutants** (`wire-mutants.txt`): at `9712d8b`, `osc.py` passes the unchanged copy of `stopnext`'s wires and
fails each of 10: STOP-LATE-EVENT's eight (round 1's `working` behind the `Thinking` frame, twice; the stopped
turn's end as a failure; working again then failed; an fx run's `working` behind its frame; the title behind the
next `working`, twice; an fx run's title behind it), and two for `prompt`: the base's frame -- the prompt's card on
the stop's `Cancelled` frame -- fails `prompt`; the same on an fx run fails `stable`.

## Code mutants (`mutants.py`, `mutants.txt`)
26 mutants, each one exact replacement in `shell.rs` or `worker.rs`, run on a `--no-local` clone against every TUI
library test (`tui::`), 900 s each (a suite past it would be KILLED (hang); none was). **All 26 are killed, 0 failed
to build.**
- **The first run, at `9712d8b8`:** 21 killed, 5 survived: a covered start that rings the bell (5); the entries below
  a start not swept (7); a start that takes the oldest entry whatever its number (8, the base's pop); a compaction's
  stop that records no count (11); a stop not taken back once nothing is in hand (22). The unmutated clone passes
  2789 of 2789.
- **`d6be6c30` adds a test for each** (tests only): `the_turn_a_stop_took_from_the_runtime_rings_no_bell`,
  `a_start_past_a_prompt_the_stop_dropped_is_its_own_prompts_turn` (7 and 8), `a_prompt_behind_a_stopped_automatic_
  compaction_is_no_work`, `a_stop_whose_work_has_all_ended_holds_no_later_press`. The five again, there: 5 of 5
  killed, the unmutated clone 2793 of 2793. (Row 8 also lists a worker timing test the mutation does not touch; it
  passed unmutated right after.)
- The rows, by what they break: which turn a start is (1-15: the comparison off by one, ignored, or always true;
  the end's say, the bell, the card; the sweep and the take; the coverage of a waiting prompt; each stop's count;
  the band's number, a handed-on steer's, the runtime's); item 4 (16-17: the keystroke echo, the band row); item 5
  (18-24: no guard, either exemption gone, each of the three take-backs, each stop's request); item 6 (25-26: the
  base's lead, a later title keeping it).

## Sweep (`sweep.txt`)
Every `p5-lifecycle-*` scenario (osc and lifecycle, 30 scenarios), fx v0.0.16 against the base's debug build
(`8c5e9e1`) and the head's (`9712d8b`), each sweep in a suite-lock slot; both banners and both sha256s are checked in
each summary. 69 pairs on each side, none on one side only, 0 failed and 0 timed out, in both pairs of sweeps.
- **Each pair has 1 changed pair, the same one:** `p5-lifecycle-retryok` `k01-1.5s`, normalized-identical on the base,
  unresolved on the head (first pair: base 27 identical, head 26; second: 28 and 27).
- **The change is fx's capture, not xfx's** [measured]: xfx's `k01-1.5s` snap is the same, every row but the banner,
  in all ten runs (both sweeps of each side, and `retryok` alone three times on each, alternately). What moved is
  fx's own snap, captured afresh in each sweep: still `⚠ Provider unavailable · … · retrying request in 1s` at 1.5 s in
  both head sweeps, `RECOVERED-ANSWER.` in both base sweeps and all six alone runs. Alone, all three of `retryok`'s
  pairs are identical on both builds, three times each. The first base sweep's `k00-0.5s` also differs from
  STOP-LATE-EVENT's head sweep (`../stop-late-event/sweep.txt`), the same scenario's fx clock. Why fx ran late in the
  two head sweeps is not traced [the sweeps' loads: 5.8-6.8 and 6.4-27.4, 1-minute averages].
- No pair drops on xfx's side. `sweep.txt` holds both pairs' summaries, the reruns, the snaps side by side and the
  pair-by-pair verdicts.
## Gates (`gates.sh`, `gates.txt`)
Every CI step and `sweep.yml`'s two test scripts, one at a time, in one suite-lock slot (10:54:29Z to 11:11:29Z), at
`c8666457`: the commit that adds this receipt, so the secrets, identity and stub checks read it tracked. The tree had
0 tracked changes before and after. Step 00 is `false` and reads `rc=1`, so each step's `rc` is its own.

| gate | result |
|---|---|
| `cargo fmt --check`; `clippy --all-targets -D warnings`, without and with `fault-injection` | ok; no output, 0 warnings, 0 warnings |
| `cargo test --locked --all-targets --no-fail-fast` | 5357 passed, 0 failed, 7 ignored (14 binaries) |
| `cargo test --features fault-injection --test tui`; `--lib` | 193 passed; 4466 passed, 0 failed, 7 ignored |
| `check-no-stubs`, `check-no-secrets`, `check-xfx-identity`, `check-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock` | ok (13939 tracked files) |
| `test-check-no-secrets`, `test-gate-env`, `test-preview-contract` | ok (13 cases; 5 cases; 24 mutants caught) |
| fxcap `test-sweep`; `test-capture` | 66 + 26 tests OK; 59 tests OK |
| release and faulty release builds | ok |
| `smoke.sh` (release); `smoke-tui.sh` (release, `--faulty`) | 47 checks, 0 failures; 37 scenarios + the oracle, 1168 checks, 0 failures |
| osc wires, `stopnext` x3, wire mutants | 47 pairs, 0 failing at the head; 0 failing on each run; 10 of 10 mutants fail (above) |
| code mutants; the unmutated clone | 26 of 26 killed; 2789 and 2793 of 2789 and 2793 passed (above) |
| sweeps, base vs head | 69 pairs each, twice; one changed in each, fx's capture (above) |

The captures, the sweeps and the first mutant run were taken before the gate, from `9712d8b8`'s builds; the gate's
`cargo test` rebuilt the debug binary at `c8666457` afterwards, and nothing was captured from it.

## The refs (`refs.py`, `refs-before.txt`)
STOP-LATE-EVENT's `refs.py`, with the base as the one frame: every xfx `file:line` ref in the documents and the code,
checked against the code at `8c5e9e16`. Before `9712d8b8`: 44 hold, 11 moved, 2 lost (the parity row's two
`worker.rs` refs, on a line this unit rewrote). Re-pointed in `9712d8b8`: the ledger's `shell.rs:12718` -> `:12865`,
`:37926` -> `:38150` five times, `:1677` -> `:1700`; `capfile.py`'s `warning_shown` `:14172-14197` -> `:14341-14366`
and its bare `:14190-14192` -> `:14359-14361` (by hand); `shell.rs`'s `worker.rs:3835` -> `:3840`, and the parity
row's (by hand; its `:3524` holds). Left as they were: two comment refs main's own tests had wrong before this unit
(`event_loop.rs`'s `shell.rs:2706`, `shell.rs`'s `:5816`), as STOP-LATE-EVENT left them.

## Line budget
`git diff -U0 8c5e9e16..9712d8b8 -- src`, each hunk counted by whether its new side starts below the file's
`mod tests {`: outside tests +300 / -103 lines; inside tests +606 / -86. Under the brief's ~600.

## STOP-NEXT-2 (proposed; the parity row's "Where xfx differs" names each, owner unit STOP-NEXT-2)
1. **A stop and its turn's end in one tick** (item 7). If the keystroke and the stopped turn's end are read in one
   tick, the stopped turn's title leads the tick's report, ahead of the stop's own `idle`; fx's order is `idle`, the
   `Cancelled` frame, the title. Needs `commit_tick` to hand a stopped end's title read in the stop's own tick to the
   frame instead. No capture reaches it (the runtime would have to finish a stop within one tick).
2. **A finished turn's text read after the keystroke** (item 8). xfx drops a `Delta` under the stop; fx's drain
   presents a segment that holds its own `turn_finished(.completed)` (`batchSegmentIsInterrupted`,
   `app_worker_runtime.zig:301-325`). Needs the band to keep a stopped turn's late text until its end says whether
   it finished -- now for a covered turn too (item 2's second case) -- against the keystroke-time stop.
3. **A failed turn's title** (item 9). fx keeps a failed turn with a partial answer or a completed step in the
   history and titles the session from it (`persistFailedPartialTurnOnce`, `persistFailedTurnWorkOnce`,
   `interruption.zig:226-293`). Needs the runtime to say whether a failed turn was kept; fx's outer caller of the
   failed-compaction branch is still untraced (`../stop-late-event/fx-src.txt`, "Mutant 20").
4. **The two holds** (item 10).
   - `turn_start_held`: a second prompt cannot install a pending submission while one is held -- the assert ahead of
     `tryHoldTurnStart` (`input_submit_runtime.zig:1148-1149`) -- and fx's submit path's guard for that state is
     not traced, so whether fx queues the second at once or keeps it from the submit at all is not read.
   - `session_transition_held`: fx holds it for a `/new`, `/clear` or `/reset` while its live transition is pending
     (`app_session_runtime.zig:1532-1538`) and **defers every key typed meanwhile** (`app_input_runtime.zig:679-700`),
     replaying them with the hold still on (`app_session_runtime.zig:1723-1737`; the loop's replay,
     `main.zig:2992-3010`). xfx defers no keys across a reset; copying the hold's report alone would copy half a
     mechanism whose other half shows in the composer.
5. **One title a tick** (item 6's residual): a model's title read in the tick of a stopped turn's end is the one
   title that tick writes, behind its report, where fx writes the end's title on the pass that drains it and the
   model's on a later one. Needs the band to write two titles in a tick.

## Files
- `README.md`; `remerge.txt` (the merge); `fx-src.txt` (every fx citation, read at the pin).
- `red-test.txt`, `red2-test.txt`, `green-test.txt`: the unit's tests.
- `osc.py`, `osc-all.txt`, `osc-stopnext.txt`: the checker and the wires' verdicts; `wire_mutants.py`,
  `wire-mutants.txt`.
- `mutants.py`, `mutants.txt`: the code mutants.
- `sweep.txt`; `gates.sh`, `gates.txt`; `refs.py`, `refs-before.txt`.
- The wires: `base/`, `base-run2/`, `base-run3/` (`8c5e9e16`); `xfx/`, `xfx-run2/`, `xfx-run3/` (`9712d8b8`). fx's are
  the earlier units' (above).

Every committed file is redacted: it holds no home-directory path and no user name.
