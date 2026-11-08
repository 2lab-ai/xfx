# compact-notice-late: a stopped compaction's sentence came back after the next line

Registry #19, 21st sighting. PR #82's CI row `gate (aarch64-apple-darwin)`, step
"Tests, with the fault injector": `ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`
(`tests/tui.rs:12353`) timed out at its last `on_screen`, with `Compaction cancelled.` still on
the screen after `third question` and `MARKER-THREE`. It passed on the rerun.

The cause is an ordering, not a slow runner: the band told the stop's sentence itself, at a
tick's settle, while the runtime's own ending was still on the channel. The next line took the
sentence away, and the ending, read after the line, put it back. The band now tells it only
after a drain that began with nothing in hand and emptied the channel, so the runtime's ending,
when there is one, is always read first.

Base `58c92ba2` (main). Line numbers are at the base unless marked "head"; fx's are at
`481104fd` (`fx-src.txt`).

| commit | what |
|---|---|
| `2e724248` | RED: `a_stopped_compactions_sentence_the_next_line_took_away_stays_away` (event_loop.rs), alone |
| `0cc9b34c` | the fix, its shell tests, `docs/parity.md` |
| `bd0ee247` | the RED test's two more dismissals (an Escape, `/version`) |
| `ed0e4e3a` | the program-status test the mutants asked for; one redundant frame request gone |
| the receipt commits | docs only: this folder |

The product code last changed at `ed0e4e3a`; `bd0ee247` changes a test only, and the receipt
commits touch nothing outside this folder. Which build each receipt used:

| receipt | built at | how it was checked |
|---|---|---|
| `red.log` | `2e724248` | the commit (test binary) |
| `red-extended.log` | `2e724248`'s code with the test as at `bd0ee247` | a clone, patched (`git apply`) |
| `loops-base.txt` | `2e724248` (pty tests unchanged from `58c92ba2`) | built before the fix was written |
| `loops-head.txt` | `ed0e4e3a` | `strings` on both xfx binaries: `ed0e4e3a9fe0` |
| `sweep-base.md` | `58c92ba2` | its banner, `v0.1.0-58c92ba` |
| `sweep-head.md` | `0cc9b34c` | its banner |
| `sweep-head2.md` | `ed0e4e3a` | its banner |
| `mutants-r1.txt`, `mutants.txt` | `0cc9b34c`, `ed0e4e3a` | the clone's `git rev-parse` (first line) |
| `gate.txt` | the first receipt commit | the gate's own first line |

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

## The path, hop by hop

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
   `signals.rs:310`, `:671`, so nothing the runtime sends wakes the loop), read the keys (`:275`),
   `settle_input` (`:282`), drain the events (`take_ui_events`, `:285`, `:351`; `shell.apply`,
   `:385`), `settle_band` (`:291`).
5. `settle_band`, at the base: once `outstanding() == 0`, a compaction still `stopped()` is
   settled `Cancelled` on the spot (`shell.rs:10477-10490`), "an ending the runtime did send, a
   moment late, still replaces this one".
6. The next line: `submit` clears a compaction that is not active (`shell.rs:12927-12934`).
   An Escape does the same (`:12422-12423`).
7. The ending: `compaction_update`'s `Settled` arm settles `self.compaction`, **inserting one
   when there is none** (`get_or_insert_with`, `:11550`), and notes the program status when that
   one was running (`:11544-11548`).

Step 3's order makes `outstanding() == 0` proof that the ending is already on the channel,
not that none is coming. When both of step 3's steps land between one tick's drain (step 4)
and its settle (step 5), the settle tells the sentence with the ending unread. If the next
keys are read before the next drain, as a loaded runner's late tick reads them, step 6 clears
it and step 7 re-inserts it, in one tick: frame 5.

### Every ordering that shows the sentence after it was taken away

The tick that tells it early is always the same. What differs is what comes next:

- **a prompt** (the CI case): cleared, then re-inserted as a manual compaction;
- **`/compact`**: the line clears it and `compact` puts a new running compaction on the row
  (`:14259`); the stale `Cancelled` then settles **the new one**, which says
  `Compaction cancelled` while it runs and notes a second program-status result;
- **a command that sends nothing** (`/version`): the same as a prompt;
- **an Escape** (`:12422`): the same;
- **a stopped automatic compaction, then a prompt**: the same, and the re-inserted one is
  treated as manual (`automatic` reads `false` from no compaction, `:11538`).

Each of the five is a case of the RED test (three at `2e724248`, two more at `bd0ee247`). A line typed while the row still says
`Stopping compaction` is not one of them: the sentence that comes after it is the
compaction's real ending, and fx does the same (below).

## fx at the pin (`fx-src.txt`)

- The state is the worker's, behind its mutex (`worker_runtime.zig:577-619`). The UI reads a
  snapshot; it never settles an operation from what it infers.
- An operation has an identity (`compaction_activity.zig:3-4`). `settle` reaches only the
  matching operation, and only while it is active (`:118-120`, `:146-149`). The test checks
  that a second settle of a terminal operation changes nothing (`:158-161`;
  `worker_runtime.zig:2987-2988`). "Acknowledged settlement wins" (`app_agent_runtime.zig:1035`).
- The next line dismisses by id and revision (`input_submit_runtime.zig:639-647`; the
  shell's comment cites `:628-635`, the old revision's lines), and so does Escape
  (`input_interrupt_runtime.zig:57-65`). `dismiss` keeps the operation and marks it dismissed
  (`compaction_activity.zig:129-137`); `visible` is false once it is (`:52-53`), and the row
  shows nothing (`activity_status.zig:23-24`). Its e2e waits for the pane to stay clear of the
  sentence once dismissed (`tui-compaction-activity.test.ts:466-475`).
- A queued compaction the stop drops is settled `cancelled` by the worker itself
  (`cancelContextCompaction`, `worker_runtime.zig:1007-1009`; a discard, `:1846-1847`).

So in fx, nothing re-shows the sentence after the line: the late settle finds a terminal (and
dismissed) operation and does nothing. A line sent while the operation is still `stopping`
dismisses nothing (`dismiss` refuses an active one, `:133`), and its later settle shows the
sentence. That is upstream's behaviour too.

## The fix

xfx's runtime sends nothing for a compaction the stop dropped before taking it
(`Queue::abandon`, `worker.rs:3731-3742`), so the band still has to tell that one. It now does
so only when no ending can follow:

- `Shell::take_ui_events(drain)` (head `shell.rs:9197`) reads `outstanding()` **before** the
  drain, runs the drain, and, when the drain emptied the channel and nothing was in hand as it
  began, calls `settle_unstarted_compaction` (head `:9221`). By step 3's order every ending the
  runtime will send for the work it gave back is then already read. The event loop's
  `take_ui_events` is now that call (head `event_loop.rs:357`).
- `settle_band` no longer settles a stopped compaction (head `shell.rs:10524`).
- Not read after the drain: the place can be given back while the drain runs, with the ending
  behind it (mutant M3 below). Not taken from the previous tick's settle either: a `/compact`
  and a Ctrl-C read in between would be a compaction in hand.
- `get_or_insert_with` stays. `Began { automatic: true }` goes through the cancellable
  `send_ui` (`worker.rs:4764-4769`), so a stopped turn can drop it while its `Settled`
  still comes; the fallback shows that one, as fx shows its synchronously begun operation. With
  the inference after the ending, nothing re-inserts a dismissed sentence.
- `docs/parity.md`, `/compact`'s row, names the difference: fx's worker settles the dropped one,
  xfx's band does, after the drain.
- The runtime's order this rests on was read on every path: manual, `run_compact` sends before
  `turn_loop` gives back (`:3910`, `:3550`); automatic, every path after `Began` settles
  (`worker.rs:4804`, `:4809`, `:4834`, `:4862`; `worker.rs` is unchanged) before the turn's `TurnEnded`
  (`:3867`) and its give-back; the dropped one is given back by `abandon` with nothing sent.

## RED -> GREEN

- `red.log`: the test at `2e724248`. All three first cases: the line took the sentence away,
  and it was on the row three drains later; `/compact`'s new compaction showed the sentence
  instead of `Preparing compaction`.
- `red-extended.log`: the five-case test run on the base's code (`2e724248` with the test as it
  is at head): all five came back.
- GREEN: the same test at head, with the shell tests: see `gate.txt`.

The test drives the loop's own order with a real channel and the real `take_ui_events`: one
tick's drain finds nothing, then the runtime's two steps, then the tick's settle. The next line
is typed as a person types it, once the row says the sentence. With the fix that is one tick
later, after the drain that reads the ending.

`a_compaction_stopped_before_the_runtime_took_it_ends_cancelled` (head `shell.rs:43340`) is
changed in meaning on purpose: it gave the place back and expected the sentence at the tick's
settle. Now it expects `Stopping compaction` there and the sentence after the next drain that
empties the channel. `a_stopped_compaction_is_over_only_after_a_drain_that_began_with_nothing_in_hand_emptied`
(`:43393`) pins the boundary: given back during a drain, no; a drain cut short, no; the next
drain that empties, yes.

## The original test under load (`loops-base.txt`, `loops-head.txt`, `loops-both.sh`)

`ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes`, `--exact --test-threads 1`, 20 runs on
each build (with the fault injector, as CI's failing step, and without), one after another,
under 18 busy loops on this 18-core machine (`loaded.pl`), in one `SUITE_LOCK_EXCLUSIVE=1` hold:

| build | fault-injection | plain | 1-minute load |
|---|---|---|---|
| base (`2e724248`'s pty tests = `58c92ba2`'s) | 20 / 20 pass | 20 / 20 pass | 10.4 to 24.3 |
| head (`ed0e4e3a`) | 20 / 20 pass | 20 / 20 pass | 25.7 to 31.0 |

The loaded loop did not reproduce the failure on the base, so the head's 40 passes prove
nothing about the fix. The race needs the runtime's two steps between one tick's drain and its
settle, and the next keys read before the next drain. Busy loops on idle cores do not make that
order: CI's runner was slow in its own way. The proof is the deterministic schedule (RED above)
and the frame-5 reading of CI's dump. What the loops do show is that the original test still
passes under load with the fix, where the sentence now comes one drain later.

## Mutants (`mutants-r1.txt`, `mutants.txt`, `mutants.py`)

Each mutant is one string replacement on a `git clone --no-local` copy in the scratch
directory, reset before the next, under the suite lock. Each runs the lib tests matching
`compact`, `stop` or `cancel` and the pty tests matching `compact`, each under a 20-minute
limit.

Round 1, at `0cc9b34c`: M1-M5 killed, and three survived:

- M7, no program-status note for the dropped one. A real gap. When the drop lands in the tick
  that read `/compact` and Ctrl-C, nothing reports `working` first, so only this note moves the
  report off a failure before it to `idle`, as fx's `noteCompaction` does
  (`src/ui/terminal/program_status.zig:80-96@481104fd`). `ed0e4e3a` adds
  `a_compaction_dropped_unstarted_reports_idle_after_a_failed_ones_error`.
- M8, no frame request. Redundant: the tick's settle comes after the drain, and
  `tick_activity` asks for a frame whenever the row changes (head `shell.rs`, the end of
  `tick_activity`); program-status reports are taken every tick whatever is owed
  (`commit_tick`). `ed0e4e3a` deletes the line.
- M6, see below.

Round 2, at `ed0e4e3a` (`mutants.txt`):

| mutant | result | killed by |
|---|---|---|
| M1 no "nothing in hand" gate | killed | the RED test; the drain-boundary test |
| M2 no "emptied" gate | killed | the drain-boundary test ("a drain cut short") |
| M3 count read after the drain | killed | the drain-boundary test ("given back during the drain") |
| M4 the base's settle-time inference back | killed | the RED test; two of the shell tests |
| M5 no inference at all | killed | the three shell tests |
| M6 settle any running compaction, not only a stopped one | survived: equivalent | -- |
| M7 no program-status note | killed | the status test |

M6 is equivalent. It differs only for a compaction that is running, not stopping, with nothing
in hand and the channel emptied. That cannot exist: (a) every manual compaction the runtime
takes sends its ending before its place is given back (`worker.rs:3910`, then `:3550`);
(b) every automatic path after `Began` sends one (`:4804`, `:4809`, `:4834`, `:4862`) before
the turn's `TurnEnded` (`:3867`) and its give-back; (c) the only drop that sends nothing is
`abandon` (`:3731-3742`), which only a cancel reaches; (d) every cancel the band sends while a
compaction runs marks it stopping (`cancel_turn_with`, `shell.rs:12761-12763`, then
`stop_compaction`), and a reset clears it (`:15404`). `stopped()` is kept: it is the claim the
function makes.

## Compaction sweep (`sweep-*.md`, `pairs.txt`, `osc.txt`, `sweeps.sh`)

`scripts/fxcap/sweep.sh --only 'compact*' --only 'p5-lifecycle-*compact*'`, fx v0.0.16
(sha256 `8e6cda2e…`, checked before and after each run), 40x120, `--jobs 2`, under the suite
lock: 12 scenarios, the `compact-*` ones and every `p5-lifecycle-*compact*` one
(`osc-compact`, `osc-compactfail`, `osc-autocompactfail`, `compactfail`, `compactok`).

| side | xfx banner | snap pairs | OSC wire pairs |
|---|---|---|---|
| base | `v0.1.0-58c92ba` (built from a clone at `58c92ba2`) | 29 / 29 pass | 7, 0 failing claims |
| head | `v0.1.0-0cc9b34` | 29 / 29 pass | 7, 0 failing claims |
| head2 | `v0.1.0-ed0e4e3` (the product code as it ships) | 29 / 29 pass | 7, 0 failing claims |

`pairs.txt` lists every pair's verdict on the three sides: no pair drops. The snap pairs that
exercise this surface are `compact-cancel`'s: `cancelled` and `cancelled-2.8s` (the sentence
after the stop), `dismissed` (after the Escape that takes it away) and `next-turn` (the next
prompt's turn). All four match fx (normalized-identical) on all three sides. The OSC wires are
compared by STOP-NEXT-2's checker (`../stop-next-2/osc.py`, unchanged) against the fx wires it
reads (`../osc-program-status/fx/`).

## Gate (`gate.txt`)

`../train-7/gates.sh`, unchanged, at `4057117a` (the first receipt commit; the product code is
`ed0e4e3a`'s), the tree clean before and after, in one hold of the suite lock: **20 steps, 0
failed**. fmt, clippy and clippy with the fault injector; `cargo test` (5,449 passed, 0 failed,
15 binaries) and with the fault injector (5,486 passed, 0 failed); every `check-*.sh` and
`test-*.sh`; fxcap `test-sweep` and `test-capture`; both release builds; `smoke` and `smoke-tui`.
This unit's four tests and the original one pass in both test runs.

## A neighbour looked at

`settle_band`'s turn cancel (`shell.rs:10465-10471`) reads the same count the same way. A probe
of the same race on a stopped turn's end (`TurnStarted` and `TurnEnded` sent, the place given
back, between a tick's drain and its settle) wrote the same transcript both ways: `first
question`, `▪ Cancelled · …`, nothing more (`probe-turn.txt`). Nothing to fix there from this
schedule.

### Observation, not changed here: a prompt queued behind a compaction the stop dropped (`probe-prompt-behind.txt`, `probe-window.txt`)

`/compact`, Ctrl-C before the runtime takes it, and a prompt typed before the runtime has dropped
it (typed fast, or while other work ahead of the compaction still runs). The drop leaves the
prompt in hand, so nothing is in hand only once that prompt's turn is over. Until then the row says
`Stopping compaction (Ns)` over the prompt's turn (a running compaction's row takes the turn's,
`tick_activity`), and the sentence comes when the turn ends. The probe shows it at `0cc9b34c`;
the base's settle needs `outstanding() == 0` as well (`shell.rs:10477`), so the base does the
same. fx's worker settles the dropped one at once (`cancelContextCompaction`,
`worker_runtime.zig:1007-1009@481104fd`).

While other work is in hand the band cannot tell "dropped" from "taken, its ending on the way".
The runtime has to say it: `Queue::abandon` (`worker.rs:3731-3742`) would send
`Settled(Cancelled)` for a `TurnWork::Compact` it drops, before it gives that place back, and the
band's inference would go entirely. `abandon` runs inside the race's stop closure, which cannot
await (`worker.rs:3903-3904`, `:5664`), so that changes its contract at every call site. It is a
unit of its own.

This fix widens that case by one tick, and it is the one way it is worse than the base. A
compaction the stop dropped is now told at the next drain rather than at the tick's settle. If
the drop lands between a tick's drain and its settle, and a line is submitted in the very next
tick, before its drain, the base had already said the sentence and the line took it away. Now
that line puts work in hand before the drain that would have told it, and the row says
`Stopping compaction` over that line's turn. The line has to be read within one tick (8 ms) of
the drop, so a pasted or scripted Enter, not a typed one. The band cannot do better while it
infers: at that settle a dropped compaction and a taken one whose ending is on the way look the
same, and telling the dropped one there is this unit's bug. The runtime-side change above closes
both cases.

## Files

- `ci-dump.txt`: the failed run's message and frames.
- `fx-src.txt`: every fx line cited, at `481104fd`.
- `red.log`, `red-extended.log`: RED.
- `loaded.pl`, `loops.sh`, `loops-both.sh`, `loops-base.txt`, `loops-head.txt`: the loaded loops.
- `mutants.py`, `mutants-r1.txt`, `mutants.txt`: mutants, on `git clone --no-local` copies.
- `sweeps.sh` (base and head; head2 is the same loop with the side `head2`), `sweep-base.md`,
  `sweep-head.md`, `sweep-head2.md`, `pairs_compare.py`, `pairs.txt`, `osc.txt`: the compaction
  sweep.
- `probe-prompt-behind.txt`, `probe-window.txt`: the observation below the neighbour.
- `probe-turn.txt`: the neighbour's probe.
- `gate.txt`: the gate, every CI step, under the suite lock; step logs stayed in the scratch
  directory.
