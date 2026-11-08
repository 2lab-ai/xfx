# compact-notice-late: a stopped compaction's sentence came back after the next line

Registry #19, 21st sighting. PR #82's CI row `gate (aarch64-apple-darwin)`, step
"Tests, with the fault injector": `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`
(`tests/tui.rs:12353`) timed out at its last `on_screen`, with `Compaction cancelled.` still on
the screen after `third question` and `MARKER-THREE`. It passed on the rerun.

The cause is an ordering, not a slow runner. The band inferred a stopped compaction's end from
the count of work in hand, and that count cannot tell a compaction the stop dropped from one
whose ending is still on the channel. Round 1 narrowed the inference (the count read before a
drain that emptied the channel). Round 2, the dispatcher's call, deletes it: the runtime now
tells the band when a stop drops a compaction that never began, as upstream's worker settles
the queued compaction it drops, and the band settles a compaction only on the runtime's word.

Base `58c92ba2` (main). Line numbers are at the base unless marked "head" (`d268896a`, the last
commit touching code); fx's are at `481104fd` (`fx-src.txt`). `<tmp>` in the committed scripts
stands for the short `TMPDIR` the runs used, under the harness temp folder; `<scratch>` for the
session's scratch directory.

| commit | what |
|---|---|
| `2e724248` | round 1 RED: `a_stopped_compactions_sentence_the_next_line_took_away_stays_away` |
| `0cc9b34c` | round 1 fix (drain-gated inference), since replaced |
| `bd0ee247` | round 1 RED's two more cases (an Escape, `/version`) |
| `ed0e4e3a` | round 1: a program-status test; a redundant frame request deleted |
| `4057117a`, `34d5d8bc`, `aae955a6` | round 1 receipts |
| `51d91447` | round 2 RED: the runtime's drop is never told; the next line's turn keeps the row |
| `31b38c84` | round 2 fix: the runtime tells the drop; the band's inference deleted; parity row |
| `d268896a` | round 2: a line sent before the stop takes nothing from a dropped compaction |
| the receipt commits after it | this folder only |

Which build each receipt used:

| receipt | built at | checked by |
|---|---|---|
| `red.log` | `2e724248` | the commit |
| `red-extended.log` | `2e724248`'s code, the test as at `bd0ee247` | a clone, patched |
| `red2.log` | `51d91447` | the commit |
| `red2-base.log` | `58c92ba2`'s code with `51d91447`'s tests | a clone: `worker.rs` at `51d91447` (round 1 left it alone), `event_loop.rs` = the base's code + `51d91447`'s tests module |
| `loops-base.txt`, `loops-head.txt` | `2e724248`, `ed0e4e3a` | round 1 only (below) |
| `sweep-base.md` | `58c92ba2` | its banner |
| `sweep-head.md`, `sweep-head2.md` | `0cc9b34c`, `ed0e4e3a` | round 1, their banners |
| `sweep-head3.md` | `d268896a` | its banner, `v0.1.0-d268896` |
| `mutants-r1.txt`, `mutants.txt` | `0cc9b34c`, `ed0e4e3a` | round 1, the clone's first line |
| `mutants3.txt` | `d268896a` | the clone's first line |
| `gate.txt` | `4057117a` | round 1 |
| `gate2.txt` | the round 2 receipt commit | the gate's first line |

## What CI drew (`ci-dump.txt`)

The dump split at each frame's `?2026h`:

- frame 3: `Stopping compaction (0s)`;
- frame 4: `Compaction cancelled. Try /compact again when ready.` on row 18;
- frame 5: `third question` written into row 18 **and** the sentence on row 20, in one frame;
- frame 6: `MARKER-THREE`, and the sentence still under it.

Frame 5 is the proof. The line was read and cleared the sentence, and the same tick put it
back. Had frame 4's sentence come from the runtime's ending, the line would have cleared it for
good: nothing more about that compaction was coming. So frame 4's sentence was the band's own,
told while the ending was unread, and the ending was read in the tick that read the line.

## The path, hop by hop (base)

1. Ctrl-C: `cancel_turn_with` (`shell.rs:12761`) sees a compaction running and calls
   `stop_compaction` (`:12763`, `:11594`): `TurnControl::Cancel { through }` goes to the runtime
   (`:11597`) and the row says `Stopping compaction` (`:11603`).
2. The runtime is in `run_compact` (`worker.rs:3892`), racing the body against the control
   channel. The cancel is `stop(through)` (`:5664`): `turn.cancel()`, `queue.abandon(through)`
   (`:3903-3904`). The body is polled to its end (`:5734`). Its feedback is `Cancelled` either
   way: the notes request stopped (`ProviderError::Cancelled` -> `Failure::Cancelled`,
   `compact.rs:2974-2975`, -> `Feedback::Cancelled`, `worker.rs:4112`), or finished with the token
   already cancelled (`worker.rs:4069-4072`).
3. `run_compact` sends `UiEvent::Compaction(Update::Settled(feedback))` through `send_terminal`
   (`:3910`; `bridge.rs:1519`), **then** `turn_loop` gives the place back
   (`queue.outstanding.fetch_sub`, `worker.rs:3550`). Sent first, given back second.
4. The UI thread's loop, every tick (`event_loop.rs:255-291`): wait up to `TICK` = 8 ms for
   stdin or the self-pipe (`:84`, `:256-260`; only a signal handler writes the pipe,
   `signals.rs:310`, `:671`, so nothing the runtime sends wakes the loop), read the keys
   (`:275`), `settle_input` (`:282`), drain the events (`take_ui_events`, `:285`, `:351`;
   `shell.apply`, `:385`), `settle_band` (`:291`).
5. `settle_band`: once `outstanding() == 0`, a compaction still `stopped()` is settled
   `Cancelled` on the spot (`shell.rs:10477-10490`), "an ending the runtime did send, a moment
   late, still replaces this one".
6. The next line: `submit` clears a compaction that is not active (`shell.rs:12927-12934`).
   An Escape does the same (`:12422-12423`).
7. The ending: `compaction_update`'s `Settled` arm settles `self.compaction`, **inserting one
   when there is none** (`get_or_insert_with`, `:11550`), and notes the program status when that
   one was running (`:11544-11548`).

Step 3's order makes `outstanding() == 0` proof that the ending is already on the channel, not
that none is coming. When both of step 3's steps land between one tick's drain and its settle,
the settle tells the sentence with the ending unread. If the next keys are read before the next
drain, step 6 clears it and step 7 re-inserts it, in one tick: frame 5.

### Every ordering

A taken compaction told early, then:

- **a prompt** (the CI case): cleared, then re-inserted as a manual compaction;
- **`/compact`**: the stale `Cancelled` settles **the new one** (`:14259`), which says
  `Compaction cancelled` while it runs and notes a second program-status result;
- **`/version`**, **an Escape** (`:12422`): the same as a prompt;
- **a stopped automatic compaction, then a prompt**: the same, the re-inserted one treated as
  manual (`automatic` reads `false` from no compaction, `:11538`).

A compaction the stop dropped before the runtime took it (`Queue::abandon`,
`worker.rs:3731-3742`, sends nothing), then a line:

- **pasted into the tick after the drop**, read before that tick's drain: round 1 (the count
  read before the drain) saw the line's own work in hand and never told it, so `Stopping
  compaction` sat over the line's whole turn and the sentence came after it (`probe-window.txt`).
  The base told it at the drop's settle, before the line.
- **typed with the stop**, before the runtime read it: the line's work is in hand until its turn
  ends, so the base and round 1 both left `Stopping compaction` over that turn
  (`probe-prompt-behind.txt`).

The band cannot get these right while it infers. With other work in hand, a dropped compaction
and a taken one whose ending is on the way look the same from the band.

## fx at the pin (`fx-src.txt`)

- The state is the worker's, behind its mutex (`worker_runtime.zig:577-619`). The UI reads a
  snapshot; it never settles an operation from what it infers.
- An operation has an identity (`compaction_activity.zig:3-4`). `settle` reaches only the
  matching operation, and only while it is active (`:118-120`, `:146-149`); a second settle of a
  terminal one changes nothing (`:158-161`; `worker_runtime.zig:2987-2988`). "Acknowledged
  settlement wins" (`app_agent_runtime.zig:1035`).
- Ctrl-C over a compaction calls `cancelContextCompaction` from the input thread
  (`cancelActiveOperation`, `input_interrupt_runtime.zig:77-90`). A queued one is taken off the
  queue and settled `cancelled` there and then (`worker_runtime.zig:1007-1009`); a running one is
  only marked stopping (`:1022-1024`). A reset's discard settles a queued one too (`:1846-1847`).
- The next line dismisses by id and revision (`input_submit_runtime.zig:639-647`), and so does
  Escape (`input_interrupt_runtime.zig:57-65`). `dismiss` refuses an operation still running and
  keeps the one it dismisses, dismissed (`compaction_activity.zig:129-137`); `visible` is false
  once it is (`:52-53`), and the row shows nothing (`activity_status.zig:23-24`). The e2e waits for
  the pane to stay clear of the sentence once dismissed (`tui-compaction-activity.test.ts:466-475`).

So in fx a dropped compaction's sentence is up from the stop's keystroke and the next line takes
it away; a taken one's comes when it ends, and a line sent while it is stopping takes nothing
away.

## The fix (round 2)

### The runtime (`worker.rs`, head)

- `Queue` gains `owed` (`:3735`): compactions `abandon` dropped whose ending has not been sent.
- `abandon` (`:3766`) gives back every dropped item's place except a compaction's, which it
  counts in `owed`.
- `tell_dropped` (`:3787`) sends `UiEvent::Compaction(Update::Dropped)` through `send_terminal`
  for each, and only then gives the place back: a piece of work's last word before its place,
  as everything else in `turn_loop` does.
- It runs at the top of every pass of `turn_loop` (`:3301`), and before a reset's fence in
  `reset_conversation` (`:3585-3592`), which the three reset paths now call (`:3393`, `:3412`,
  `:3573`). So a dropped compaction's word is on the near side of the fence, with the
  conversation it was about, and the band's draining filter takes it (`shell.rs`'s reset arm).

Each `abandon` call site, and how its contract changes:

| site (head) | where the drop happens | when the band is told |
|---|---|---|
| `turn_loop`, `Taken::Abandon` (`:3388`) | between items, the stop read first (`biased`) | the next pass's top: at once |
| `turn_loop`, `Taken::Reset` (`:3392`) | between items | `reset_conversation`, before the fence |
| `run_turn` (`:3889`) | in the race's stop, while a turn runs | when the turn lets go: the next pass's top, or `reset_conversation` for a reset |
| `run_compact` (`:3955`) | in the race's stop, while a compaction runs | the same |
| `run_setup` (`:4973`) | in the race's stop, while a provider switch runs | the same |
| `change_model` (`:5200`, also through `apply_deferred`) | in the race's stop | the same |

The race's stop cannot await (`raced_against_control`), so a drop made there waits for the work
it raced. That work is cancelled by the same stop (`turn.cancel()`), so it lets go at once,
except a provider probe that does not take the token (`run_setup`'s doc: up to its 3 s connect
and 5 s read timeouts). `Ended::Session` returns without telling: the band is leaving, and
`Worker::shutdown` drains until the thread joins, not until the count is zero
(`worker.rs:1081-1117`). In practice `/compact` is refused while a turn runs or a prompt waits
(`compact`, `shell.rs:14254-14257`), so only a listing or a switch can be in front of one.

### The band (`compaction.rs`, `shell.rs`, head)

- `Update::Dropped` (`compaction.rs:119`) settles a compaction that is still stopping, as
  `Cancelled`, notes its program status once (`idle`), and does nothing to any other: not one
  that runs (a newer one), not a row with none (`shell.rs:11522-11542`).
- A line sent while a compaction is stopping is noted on it (`answer`, `compaction.rs:193`;
  `submit`, `shell.rs:12944`). A drop of a compaction so noted dismisses it at once: upstream
  settled it at the stop's keystroke, so its next line already took the sentence away. A taken
  compaction ends with `Settled` and ignores the note, as upstream's dismissal refused it while
  it ran.
- **Deleted**: the base's settle at `outstanding() == 0` (`shell.rs:10477-10490`) and all of
  round 1's drain-gated inference (`take_ui_events` with its closure, `settle_unstarted_compaction`).
  `take_ui_events` and `ui_events_taken` are back to the base's code.
- `get_or_insert_with` stays: `Began { automatic: true }` goes through the cancellable `send_ui`
  (`worker.rs:4815-4820` at head), so a stopped turn can drop it while its `Settled` still comes.

**Decision: a word of its own, not `Settled(Cancelled)`.** The brief named `Settled(Cancelled)`.
The band has to treat a line sent during the stop in two ways: a dropped compaction's sentence
is already gone in fx, a taken one's still comes. Both end cancelled, so one word could not carry
the difference. `Dropped` carries it, and the mutant that sends `Settled(Cancelled)` instead is
killed (N5).

### What is left (`docs/parity.md`, `/compact`'s row)

The row now says the drop is the runtime's word, as upstream's worker settles it. It names what
is left. xfx queues `/compact` behind a listing or a provider switch, so a stop that lands while
that work runs is told when the work lets go (after a provider probe, at worst). An Escape or a
`/compact` typed before then meets a compaction still stopping: the Escape arms the interrupt
and the `/compact` does nothing. Upstream's settle at the keystroke would have let the Escape
take the sentence away and the `/compact` begin again.

## RED -> GREEN

| test (head) | round 1 RED | round 2 RED | GREEN |
|---|---|---|---|
| `a_stopped_compactions_sentence_the_next_line_took_away_stays_away` (`event_loop.rs:15876`), five lines after a taken one's early sentence | `red.log`, `red-extended.log`: all came back | `red2-base.log`: fails on the base | head |
| `a_compaction_a_stop_drops_before_it_began_is_told_how_it_ended` (`worker.rs:10170`), the real loop, a cancel and a reset | -- | `red2.log` (round 1's head) and `red2-base.log`: nothing told | head |
| `a_line_after_a_stop_that_dropped_the_compaction_has_its_turn_to_itself` (`event_loop.rs:16004`), the real loop's drop, then a line pasted into the next tick or typed with the stop | -- | `red2.log`: both schedules fail at round 1's head; `red2-base.log`: the pasted one passes on the base, the typed one fails | head |
| `a_dropped_compactions_place_is_held_until_its_ending_is_on_the_channel` (`worker.rs:10215`), the loop polled by hand with the channel full | -- | -- (needs `Update::Dropped`) | head |
| `a_compaction_stopped_before_the_runtime_took_it_ends_cancelled` (`shell.rs:43315`): no give-back and no emptied drain ends it, the drop does | -- | -- | head |
| `a_line_sent_while_a_compaction_stops_takes_the_sentence_only_of_one_that_never_began` (`:43344`) | -- | -- | head |
| `a_compaction_dropped_unstarted_reports_idle_after_a_failed_ones_error` (`:43405`) | -- | -- | head |

The band test of the drop runs the real `turn_loop` (`loop_reads_its_control`, `worker.rs:6514`,
test support), over the shell's own work channel and count, reading the stop the shell sent and
then `Shutdown`. The loop reads control first, so it drops the compaction and returns before it
takes the line. The line's turn is then the band's own events (`start_turn`, `TurnEnded`). The
drop is real, the turn modelled.

## The original test under load (round 1 only: `loops-base.txt`, `loops-head.txt`)

`ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`, `--exact --test-threads 1`, 20 runs on
each build (with and without the fault injector), under 18 busy loops on this 18-core machine
(`loaded.pl`), in one `SUITE_LOCK_EXCLUSIVE=1` hold:

| build | fault-injection | plain | 1-minute load |
|---|---|---|---|
| base (`2e724248`'s pty tests = `58c92ba2`'s) | 20 / 20 pass | 20 / 20 pass | 10.4 to 24.3 |
| round 1 head (`ed0e4e3a`) | 20 / 20 pass | 20 / 20 pass | 25.7 to 31.0 |

The loaded loop did not reproduce the failure on the base, so these passes prove nothing about
either fix. The proof is the deterministic schedule tests and the CI frames. Round 2's brief did
not ask for them again. Round 2's head runs the original test in both gate test steps
(`gate2.txt`).

## Mutants

Round 2 (`mutants3.py`, `mutants3.txt`): each mutant is one string replacement on a
`git clone --no-local` copy of `d268896a` in the scratch directory, reset before the next, under
the suite lock. Each runs the lib tests matching `compact`, `stop`, `cancel`, `drop` or `reset`
and the pty tests matching `compact`, each under a 20-minute limit. **12 of 12 killed.**

| mutant | killed by |
|---|---|
| N1 the runtime never tells a drop (abandon gives the place back, as the base) | the loop test, the held-place test, the band's drop test |
| N2 told after the place goes back | the held-place test |
| N3 no telling at the loop's top | the loop test, the held-place test, the band's drop test |
| N4 no telling before a reset's fence | the loop test ("a reset") |
| N5 the word is `Settled(Cancelled)`, as the brief first named | the band's drop test, the held-place test |
| N6 the band's settle at `outstanding() == 0` back | the five-line RED test; `…stopped_before_the_runtime_took_it…` |
| N7 a line during the stop is not noted | the band's drop test; `…takes_the_sentence_only_of_one_that_never_began` |
| N8 a line before the stop is noted too | `…takes_the_sentence_only_of_one_that_never_began` ("the line first") |
| N9 a drop settles any compaction, not only a stopping one | the same ("a newer one") |
| N10 a drop notes no program status | `…reports_idle_after_a_failed_ones_error` |
| N11 a drop always dismisses | three shell tests |
| N12 a drop never dismisses | the band's drop test; `…takes_the_sentence_only…` |

Round 1's mutants (`mutants.py`, `mutants-r1.txt`, `mutants.txt`) were of the drain-gated
inference that round 2 deleted. They are kept as the record of that round; none of their
anchors exists at head.

## Compaction sweep (`sweep-*.md`, `pairs.txt`, `osc.txt`, `sweeps.sh`)

`scripts/fxcap/sweep.sh --only 'compact*' --only 'p5-lifecycle-*compact*'`, fx v0.0.16
(sha256 `8e6cda2e…`, checked before and after each run), 40x120, `--jobs 2`, under the suite
lock: 12 scenarios, the `compact-*` ones and every `p5-lifecycle-*compact*` one
(`osc-compact`, `osc-compactfail`, `osc-autocompactfail`, `compactfail`, `compactok`).

| side | xfx banner | snap pairs | OSC wire pairs |
|---|---|---|---|
| base | `v0.1.0-58c92ba` (built from a clone at `58c92ba2`) | 29 / 29 pass | 7, 0 failing claims |
| round 1 | `v0.1.0-0cc9b34`, `v0.1.0-ed0e4e3` | 29 / 29 pass each | 7 each, 0 failing claims |
| **round 2 (head)** | `v0.1.0-d268896` | **29 / 29 pass** | **7, 0 failing claims** |

`pairs.txt` lists every pair's verdict on the four sides: no pair drops. The snap pairs on this
surface are `compact-cancel`'s: `cancelled` and `cancelled-2.8s` (the sentence after a stop of a
compaction the runtime took), `dismissed` (after the Escape that takes it away) and `next-turn`.
All four match fx (normalized-identical) on every side. No scenario stops a compaction the
runtime had not taken: it would need one queued behind a listing or a switch. The OSC wires are
compared by STOP-NEXT-2's checker (`../stop-next-2/osc.py`, unchanged) against the fx wires it
reads (`../osc-program-status/fx/`).

## Gate

GATE2

## A neighbour looked at

`settle_band`'s turn cancel (`shell.rs:10465-10471`) reads the same count the same way. A probe
of the same race on a stopped turn's end (`TurnStarted` and `TurnEnded` sent, the place given
back, between a tick's drain and its settle) wrote the same transcript both ways: `first
question`, `▪ Cancelled · …`, nothing more (`probe-turn.txt`). Nothing to fix there from this
schedule.

## Files

- `ci-dump.txt`: the failed run's message and frames.
- `fx-src.txt`: every fx line cited, at `481104fd`.
- `red.log`, `red-extended.log`, `red2.log`, `red2-base.log`: RED.
- `probe-window.txt`, `probe-prompt-behind.txt`: round 1's probes of the dropped compaction, the
  source of round 2's band RED.
- `probe-turn.txt`: the neighbour's probe.
- `loaded.pl`, `loops.sh`, `loops-both.sh`, `loops-base.txt`, `loops-head.txt`: round 1's loaded
  loops.
- `mutants.py`, `mutants-r1.txt`, `mutants.txt`: round 1's mutants; `mutants3.py`,
  `mutants3.txt`: round 2's.
- `sweeps.sh` (each side the same loop; `head2`, `head3` named the same way), `sweep-base.md`,
  `sweep-head.md`, `sweep-head2.md`, `sweep-head3.md`, `pairs_compare.py`, `pairs.txt`,
  `osc.txt`: the compaction sweep.
- `gate.txt` (round 1), `gate2.txt` (round 2): every CI step, under the suite lock; step logs
  stayed in the scratch directory.
