---
title: 0.0.16 delta -- P5 train 9 receipt (STOP-IN-CATALOG-WAIT, QUESTIONS-20, PASTE-AFTER-END, HISTORY-NOTICE-STOP into one branch)
type: receipt
tags: [xfx, phase5, fx-0.0.16, train]
---

# P5 train 9: four closed units, one branch

Branch `train/p5-1109b`, from `origin/main` `ccced1f5` (#86, train 8), as
the brief set it; `origin/main` had not moved. Each unit was closed by its
own two-engine review and dispatcher gate; this train combines them. The
brief named STOP-IN-CATALOG-WAIT and QUESTIONS-20, with HISTORY-NOTICE-STOP
to follow once closed; the dispatcher added PASTE-AFTER-END (a security
unit, both engines approving its round 2) mid-train and then fixed the scope
at these four. The program goal (the user's words): «최신 fx랑 비교해서 ui쪽으로
모든 기능 구현하고 기본적으로 최신 fx ui를 그대로 구현해줘» (fx v0.0.16, pin
`481104fd`).

Oracle: fx v0.0.16 (`481104fd`), executable sha256
`8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked
by the sweep before and after every run, `FX_AUTO_UPGRADE=0`.

**At a glance.**
- **Merges:** four `--no-ff` merges, in order. Merges 1 and 2 are git's own
  (`--remerge-diff` empty). Merges 3 and 4 each had **one conflict, the same
  kind**: both sides appended tests at the end of a tests module
  (`event_loop.rs` for merge 3, `worker.rs` for merge 4). Both kept whole,
  nothing else touched. Every merge tree passed `cargo check` (all targets,
  with and without `fault-injection`), `cargo fmt --check` and, from merge 3
  on, clippy, before it was committed: no build conflict.
- **Token check** (train 7's script, byte for byte): **all four merges OK**;
  negative control **30 of 30** (4 + 8 + 9 + 9). **References:** 7 moved by
  merge 3 and 1 of them moved again by merge 4, re-pointed in two
  numbers-only commits: **7 of 7 the same code**; 0 moved at the head.
- **Join checks:** 1 (STOP-IN-CATALOG-WAIT x train 8): the named tests pass
  at the head; no stopped-turn path reaches held rows differently. 2
  (QUESTIONS-20 x STOP-IN-CATALOG-WAIT) and 5 (PASTE-AFTER-END x QUESTIONS-20):
  three new tests; each of the worker and paste tests **fails on either
  unit alone**. 6 (the paste invariant): `feed` and the whole of `input.rs`
  **byte for byte `a832f4d7`'s**. 7 (HISTORY-NOTICE-STOP x the rest): every
  function on the notice's path is the unit's own or unchanged by any other
  unit; no new test needed. 4 (network): see the end.
- **Mutants** on the join tests: **7 of 8 killed**; the survivor is a
  compaction mutant no join test drives (STOP-IN-CATALOG-WAIT's machine test
  kills it).
- **Gate:** run three times, each a full run in one hold of the lock: at
  `dfedcd1a` (merges 1-2) 20 of 20; at `44c5d3e0` (merges 1-3) 20 of 20; at
  **`ac0279c8` (all four) 20 of 20**: `cargo test` **5481** passed, with
  fault injection **5518** (each +21 on train 8's at `main`'s code),
  smoke-tui 1167 checks (PASTE-AFTER-END's own count), `test-capture` 76 OK.
- **Sweeps:** the final sweep at `ac0279c8`, 124 scenarios (the stop,
  catalog, model and question scenarios; merge 3's paste scenarios; merge
  4's stop and history scenarios): **419 of 590 pass, 0 failed, 0 timed
  out**. Against `main` over three sweeps of the same selections: **212 ->
  214** (4 up, 2 down), **188 -> 195** (11 up, 0 down), **112 -> 112**. The
  two down (`p5-lifecycle-retryok` `k00-0.5s`, `k01-1.5s`) moved on **fx's
  screen alone**, and three reruns at the head pass 3 of 3 each. Against each
  unit's own sweep: **0 down** (3, 0, 7 and 0 up). Every move is explained
  below.
- **Network:** lib 4559 and tui 186 tests through a loopback logger: 2
  CONNECTs, both `w.example`; **none to the Gateway**.

**Head:** the branch's tip, this receipt's last commit (`train/p5-1109b`; no
push, no PR); its code is `99ef633f`'s.

## Units and merges

| # | unit | branch @ tip | merge | base | conflicts |
|---|---|---|---|---|---|
| 1 | STOP-IN-CATALOG-WAIT | `fix/p5-stop-in-catalog-wait` @ `05a0a857` | `b4211654` | `9298583e` (train 7's head) | none; no file both sides changed |
| 2 | QUESTIONS-20 | `feat/p5-questions-20` @ `3824f6ba` | `b9658fe7` | `712dd8d1` (#85) | none; two files both changed, 68 and 119 lines apart |
| 3 | PASTE-AFTER-END | `feat/p5-paste-after-end` @ `a832f4d7` | `189b8b10` | `712dd8d1` (#85) | `src/tui/event_loop.rs`, one hunk: the tests module's end |
| 4 | HISTORY-NOTICE-STOP | `fix/p5-history-notice-stop` @ `62d96c98` | `534d5a81` | `05a0a857` (STOP-IN-CATALOG-WAIT's tip) | `src/tui/worker.rs`, two hunks: the tests module's end |

Every base is git's merge base. After the merges, in order:

| commit | what |
|---|---|
| `afe31cb2` | join check 2's two tests; test only |
| `dfedcd1a` | the receipt's scripts; **gate 1 and sweep 1 ran here** (code of `afe31cb2`) |
| `189b8b10` | merge 3 |
| `b9409b36` | join check 5's test; test only |
| `8120e9a4` | the 7 references merge 3 moved, re-pointed; numbers only |
| `44c5d3e0` | merge 3's scripts, sweep 1's files; **gate 2, the paste sweep, jobs-a and the mutants ran here** (code of `8120e9a4`) |
| `534d5a81` | merge 4 |
| `3d83254e` | merge 4's scripts; merge 3's gate, sweep and mutants files |
| `99ef633f` | the one reference merge 4 moved again, re-pointed; numbers only -- **the code head** |
| `9aa61e28`, `ac0279c8` | the merge checks over all four merges; **gate 3, the final sweep, its reruns and jobs-d ran at `ac0279c8`** (code of `99ef633f`) |
| the next ones | this README and the final runs' files |

`git show --remerge-diff` of each merge is `remerge-1.txt` .. `remerge-4.txt`.

### Merge 3's conflict: the end of `event_loop.rs`'s tests

Train 8's join test (`a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays`)
and PASTE-AFTER-END's `Readable` helper and its 32-read carry-over test
(`a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled`) were
both appended after the same last test. Resolved: both, train 8's first,
with the closing `);` and `}` each needs (`remerge-3.txt`: the markers
removed and those two lines added; nothing else). No product line of either
side was in conflict.

### Merge 4's conflict: the end of `worker.rs`'s tests

Train 9's join test (`a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
its `RequestsCounted` provider and `twenty_questions` helper) and
HISTORY-NOTICE-STOP's tests (`NOT_CARRIED`, `sealed_on_llmux`, the RED
`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
`the_history_notice_ledger_goes_with_its_conversation`) were both appended
after STOP-IN-CATALOG-WAIT's last test; git lined up the few lines the two
blocks share (`state, &events, &mut work_rx, &mut changes,` of two
`turn_loop` calls) and split the rest into two hunks. Resolved: each side's
block whole, the join test first. Checked by line counts: the merged file is
the base plus HISTORY-NOTICE-STOP's change (272 added, 14 removed against
`05a0a857`, as on its branch) and plus ours (240 added, 0 removed), and each
side's 342 and 343 functions are all in it. `remerge-4.txt` shows the
interleaved markers and the two blocks put back whole.

### Where the sides meet (`overlap.txt`)

- **Merge 1: no file.** Since STOP-IN-CATALOG-WAIT's base, `main` changed
  train 8's files (`check.rs`, `event_loop.rs`, `frame.rs`,
  `full_transcript.rs`, `grid.rs`, `toolgroup.rs`, `transcript.rs`,
  `wrap.rs`, the harness, the ledger's numbers, `ci.yml`); the unit changed
  `worker.rs`, `machine.rs`, `docs/parity.md` and added its scenario. They
  meet at run time: join check 1.
- **Merge 2: two files.** `docs/parity.md` (the `/model` row, 93; the
  `ask_user_question` row, 162, and row 242) and the ledger (train 8's
  re-points; the `question-cap-four` entry replaced). Neither unit changed
  the other's code.
- **Merge 3: four files.** `event_loop.rs` (the conflict, above);
  `docs/parity.md` (rows 4 lines apart: the paste paragraph of the
  full-screen TUI row, and QUESTIONS-20's row 242); `src/tui/question.rs`
  (249 lines apart: QUESTIONS-20's counter and tests, PASTE-AFTER-END's
  refusal path deleted) and `tests/tui.rs` (307 apart). The meeting point
  is the question panel's freeform field: join check 5.
- **Merge 4: three files.** `worker.rs` (the conflict); `docs/parity.md`
  (the `/provider` row against QUESTIONS-20's, 66 lines apart);
  `shell.rs` (HISTORY-NOTICE-STOP's one test, 393 lines from
  PASTE-AFTER-END's nearest hunk). Join check 7.

## The token check (`token_check.py`, `token-check.txt`, `token-self-test.txt`)

Train 7's `token_check.py`, byte for byte (`cmp` against
`../train-7/token_check.py`; its clause, order, unchanged-row and
numbers-only rules, and its re-pointed-ref rules), over the four merges.
**Result: all four OK** [measured]:

- **Merge 1** (base `9298583e`): the `/model` row is STOP-IN-CATALOG-WAIT's
  (152 tokens added, 0 deleted, all there and in order); train 8's thirteen
  re-pointed ledger entries are `main`'s, kept so; no comment line lost.
- **Merge 2** (base `712dd8d1`): QUESTIONS-20's `ask_user_question` row (303
  added, 1 deleted) and row 242 (its `terminal_safe` reference, one for one)
  are theirs, in order; `/model` ours, kept; the ledger's
  `question-cap-four` deleted by theirs and gone, `question-resume-replay`
  added and in it as written, its comment line kept.
- **Merge 3** (base `712dd8d1`): the full-screen TUI row's paste paragraph is
  PASTE-AFTER-END's (170 added, 43 deleted, in order); the three rows above
  ours, kept; the ledger ours.
- **Merge 4** (base `05a0a857`): the `/provider` row is HISTORY-NOTICE-STOP's
  (226 added, 0 deleted, in order); QUESTIONS-20's two rows and the paste
  paragraph ours, kept; the ledger ours (merge 3's re-points among them).

**The negative control** (`--self-test`, on each merge's own texts, the cases
that apply): **4 of 4, 8 of 8, 9 of 9, 9 of 9 found -- 30 of 30**
[measured]. Merge 2's include the added entry dropped and the deleted entry
put back; every merge's, each changed row put back as the other side has it
and the order rule's two-token swap. No case of train 7's re-pointed-ref
rules applies: no reference was re-pointed by both sides of a merge.

## The references (`repoint.py`, `repointed.txt`, `refs*.txt`, `stale.txt`)

Train 7's `refs.py`, `refs_all.py`, `refs_short.py`, six sides: `main`
`ccced1f5` and the four units' tips. Merges 1, 2 and 4 alone move nothing
the ledger or `docs/parity.md` cites; QUESTIONS-20's own new references
(`toolgroup.rs:511-513`, `docs/parity.md:162`, `question.rs:60-76`) name the
same lines at the head as on its branch.

**Merge 3 moved seven** (PASTE-AFTER-END deleted the paste refusal above
them and added tests): four ledger citations into `shell.rs` (`send`,
`full_transcript::shown_event`, `resize`, and the `tools: {} advertised` row
six entries cite), `capfile.py`'s comment on `warning_shown`,
`smoke-tui.sh`'s `input.rs:702` and `event_loop.rs`'s
`event_loop.rs:972-975`. Re-pointed in `8120e9a4`, numbers only (12 lines
in, 12 out, each the same once its numbers are masked). **Merge 4 moved one
of them again**: HISTORY-NOTICE-STOP's 80 lines of tests in `shell.rs` sit
above the `tools: {} advertised` row (38636 -> 38716); re-pointed in
`99ef633f` (`repoint.py --merge-4`). `repointed.py` checks each against
`main`'s copy of the cited lines and two either side: **7 of 7 the same
code** (`repointed.txt`) [measured]. Two of the seven named other code on
`main` already (`input.rs:702` is a doc comment of the paste body, not
`Action::Undo`; `event_loop.rs:972-975` is train 8's stale one): re-pointed
by line, as train 8 did with `wrap.rs`'s, so each names what it named on
`main` (`stale.txt`).

At the head (`refs-head.txt`): 82 references, **0 moved**. The widened scans
still flag the historical plans under `docs/superpowers/plans/` (38
references into code QUESTIONS-20 and PASTE-AFTER-END moved, written about
their plans' day; train 8 left these for REF-LINT, left so here) and train
8's stale `frame.rs:1615-1628` (`stale.txt`).

## Join checks

### 1. STOP-IN-CATALOG-WAIT x train 8

**At the head**, run by name (`jobs-a.txt` at `44c5d3e0`, `jobs-d.txt` at
`ac0279c8`) and in every gate's two cargo steps: STOP-IN-CATALOG-WAIT's
`a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one` and
`a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`;
train 7's join test
`a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number`,
which now compares each second turn's end with its no-stop control; train
7's other two (`a_catalog_answer_read_in_a_stops_window_reaches_the_band_and_leaves_the_stop_as_it_was`,
`catalog_asks_around_a_stop_leave_its_count_and_the_next_start_its_own`);
GRID-ROW-COST's per-tick session test
(`a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize`,
`stale_laid_rows` empty after every tick) and train 8's join test
(`a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays`):
**all pass** [measured].

**Does any stopped-turn path reach held rows differently? No** [read from
the code at the head]. STOP-IN-CATALOG-WAIT acts only before a turn's first
request: the turn-start catalog read, now also for a prompt with images
(`worker.rs:6060`), the image refusal skipped for a stopped turn (`:6090`),
Ultra's refusal giving way to the stop (`:6130`), the machine's check before
any compaction (`machine.rs:728`). Such a turn has streamed nothing and run
no tool, and every event it sends after the stop is refused by its token
(`bridge::send_ui`, `bridge.rs:1498-1509`); only its end goes out
(`send_terminal`, `:1519-1521`). So STOP-NEXT-2's hold (`Shell::late`,
`shell.rs:1610`; `mark_tool`, `:9193-9199`; `release_late`, `:9204-9213`)
holds nothing for it, on `main` and at the head alike. What changes is the
end the shell reads: the stop's own `TURN_CANCELLED` for a turn in Ultra,
where `main` read Ultra's refusal (`ULTRAFAST_UNAVAILABLE`, `worker.rs:150`);
a prompt image's stop already ended as `TURN_CANCELLED` on `main` and now
keeps the prompt too (the session log, not the shell). At the end
(`shell.rs:4737-4783`) the stop's own end makes the turn `quiet`
(`Interrupted`, no `✗ system:` row) where `main`'s Ultra case was `Failed`;
either way `presented` is false and `release_late(false)` lets go of an
empty hold. A stop after the request (text streaming, a call running, a
question up) never reaches those lines; its rows go through `late` and
`Shell::mark` as on `main`, and GRID-ROW-COST's records see them only as a
later write of the group's block.

### 2. QUESTIONS-20 x STOP-IN-CATALOG-WAIT (`afe31cb2`; `join_test_worker.rs`, `join_test_tui.rs`, `apply_join.py`)

**What covered it already, and what did not** [read from the tests]. Neither
unit has a test of either case. A stop at an open panel: QUESTIONS-20's
`six_questions_open_the_panel_and_twenty_one_or_none_are_refused_in_upstreams_words`
stops a **six**-question panel with **Escape**; its 20-question tests answer
the batch and stop nothing. Older tests stop a panel of one or two
(`the_request_after_a_cancelled_question_answers_it_aborted_as_upstreams_does`,
`tests/tui.rs`; `shell.rs`'s
`escape_and_ctrl_c_at_a_model_question_leave_cancelled_in_the_document`,
`ctrl_c_at_a_question_stops_the_turn_and_sends_nothing_else`,
`a_ctrl_c_at_a_models_question_leaves_only_the_questions_cancelled_line`;
`worker.rs`'s `a_ctrl_c_at_a_question_stops_the_turn_before_any_later_call_or_request`).
A stop in a catalog wait: STOP-IN-CATALOG-WAIT's test never asks the model
in its stop arm, but its script answers text and its runtime's conversation
has no question requester (`under_model` opens it with `None`), so a turn
there could not have asked anything. So two tests were added:

- **`a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`**
  (`worker.rs`, the real `turn_loop`): STOP-IN-CATALOG-WAIT's
  `PromptImageFirstLoad` wait (a prompt with an image on
  `anthropic/claude-sonnet-5`, behind a first load that lands 1.5 s in with
  a row that reads images) and a model that, asked, asks twenty questions;
  the band's questioner is a `TuiQuestioner` on the band's own channels, as
  `spawn` wires it, and the band answers a batch it is shown. **Without the
  stop**: one batch of **20** reaches the band, it is answered, two
  requests, `asked first`, `1 image(s)`, `final stop`, past 1.2 s. **With a
  Ctrl-C 300 ms in**: the stop's own end between 300 ms and 1 s, **no
  request, no batch**, the log `asked first`, `1 image(s)`,
  `interrupted cancelled`: the prompt and its image kept, as fx's
  interrupted turn (`interruption.zig:198-223@481104fd`). The band writes
  that end as `▪ Cancelled · What can xfx do differently?` (STOP-IN-CATALOG-WAIT's
  capture, `../stop-in-catalog-wait/xfx-r2/`, row 5).
- **`a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt`**
  (`tests/tui.rs`, a pty, 40x120): twenty questions, Tab nine times to
  `Question 10 of 20`, then **Ctrl-C**, and in a second session **Escape**:
  the panel goes and exactly one row says `Cancelled`, `▪ Cancelled` (fx's
  `cancelQuestionPrompt`, `input_question_runtime.zig:59-83`). Then `next`:
  the Gateway is asked again, and the request carries `ask twenty`, the
  call with its twenty questions, `{"type":"error-text","value":"aborted by user"}`,
  `<turn_aborted>` and `next`: the prompt kept.

### 5. PASTE-AFTER-END x QUESTIONS-20 (`b9409b36`; `join_test_paste.rs`, `apply_join_paste.py`)

**`a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing`**
(`tests/tui.rs`, a pty, 40x120): twenty questions, Tab nine times to page
10, its freeform slot opened (`3`) and `keep` typed; then one write, so one
delivery: `ESC[200~new ESC[201~` and a CR behind the marker. **The paste
applied** (`3) keepnew`), **the warning written** (`! input: Input that
arrived right after the paste was ignored.`), **nothing answered** (the
Gateway still at one request), **the panel still on its question**
(`Pick 10?`, `Question 10 of 20`; not `Pick 11?`), and no
`Paste was not applied`. An Enter of its own then takes `keepnew` for
question 10, the walk moves to 11, nineteen ordinals answer the rest, and
the answer document carries `keepnew` at 10 and `Keep N` elsewhere.
PASTE-AFTER-END's own shell test
(`a_paste_with_input_behind_its_end_marker_reaches_the_answer_and_answers_nothing`,
a two-question batch) and its two pty tests pass at the head (below).

**On each side alone** (`join_units.sh`, `join_units_paste.sh`; `jobs-a.txt`;
a `git clone --no-local` of each unit's tip, the tests put in unchanged;
where a target did not build, the other unit's test helpers it names copied
in from the head -- test code only -- and the tests run):

| test | STOP-IN-CATALOG-WAIT `05a0a857` | QUESTIONS-20 `3824f6ba` | PASTE-AFTER-END `a832f4d7` |
|---|---|---|---|
| worker: a stop in the catalog wait of a 20-question turn | builds; **fails**: without the stop the band is shown **no** batch (the cap of 4 refuses it) | needs `turns_logged`; then **fails**: the stopped turn's log is **empty**, the prompt and image not kept | -- |
| pty: a stop at a 20-question panel | needs `question_batch`, `tool_output`; then **fails**: no `Question 1 of 20` | builds and **passes** | -- |
| pty: a paste with Enter behind its marker on page 10 | -- | builds; **fails**: no warning (X1's refusal) | needs QUESTIONS-20's helpers; then **fails**: no `Question 1 of 20` |

So the worker test needs both of its units and the paste test both of its.
The panel test is QUESTIONS-20's with the stop path that was already there:
it passes on QUESTIONS-20 alone. At the head it shows that
STOP-IN-CATALOG-WAIT's new start-of-turn check (`machine.rs:728`) does not
end the next turn after the stop; mutant S4 below (a stop that outlives its
turn) makes it fail.

**Mutants** (`join_mutants.py`, `jobs-b.txt`): on a `git clone --no-local`
of `44c5d3e0`, one textual change at a time, the file restored after each
and the clone clean at the end, each cargo run bounded at 600 s; run
against the three join tests and one test of each unit (STOP-IN-CATALOG-WAIT's
catalog-wait test, QUESTIONS-20's 20-question pty test, PASTE-AFTER-END's
pty test of an Enter behind the marker). The unmutated clone passes all six.

| mutant | join-wait | join-panel | join-paste | the units' |
|---|---|---|---|---|
| Q1 the cap back to 4 (QUESTIONS-20's M1) | **FAILED** | **FAILED** | **FAILED** | q20 FAILED |
| S1 a prompt's images do not wait for the first load (STOP-IN-CATALOG-WAIT's) | **FAILED** | ok | ok | stop FAILED |
| S2 the image refusal asked of a stopped turn (its) | **FAILED** | ok | ok | stop FAILED |
| S3 the machine measures a stopped turn's compaction (its) | ok | ok | ok | ok -- **survived** |
| S4 a stop outlives its turn (its) | ok | **FAILED** | ok | stop FAILED |
| P1 a stop at the panel answers the batch declined, not aborted | ok | **FAILED** | ok | ok |
| X1 the bytes behind the marker are decoded (PASTE-AFTER-END's 1) | ok | ok | **FAILED** | paste FAILED |
| X6 no warning (its 6) | ok | ok | **FAILED** | paste FAILED |

**7 of 8 killed, every one of them by a join test; none hung**. S3 survives
here because no test in this set has a compactor: it is killed by
STOP-IN-CATALOG-WAIT's machine test
(`a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing`,
its receipt's `no-machine-guard`), which every gate runs.

### 6. The paste invariant at the head (`invariant.py`, `invariant.txt`)

`input.rs`'s `Decoder::feed`, whose first statement is the early return for
a byte behind the marker (`if let Some(behind) = self.behind.as_mut() {
*behind = behind.saturating_add(1); return; }`), read from `a832f4d7` and
from the head: **the function byte for byte the same** (`input.rs:471-509`,
2080 bytes, sha256 `6e1257dc37c1…` on both), the early return found once,
ahead of the paste's own branch and any decoding, and **the whole of
`input.rs` byte for byte `a832f4d7`'s** (no other unit touches it) [measured].
The carry-over test
`a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled` and the
two pty tests `a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing`
and `an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too`
(`tests/tui.rs:3362`, `:3409`) **pass** at `44c5d3e0` (`jobs-a.txt`) and at
`ac0279c8` (`jobs-d.txt`), and in every gate after merge 3.

### 7. HISTORY-NOTICE-STOP x the rest (`shared.py`, `shared.txt`)

HISTORY-NOTICE-STOP branched from STOP-IN-CATALOG-WAIT's tip, so its meeting
with that unit's stop path is its own RED
(`a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn`,
on STOP-IN-CATALOG-WAIT's `WindowRetry` and `PromptImageFirstLoad` waits).
What is left is where its notice marking meets QUESTIONS-20's and
PASTE-AFTER-END's code. `shared.py` reads every function on the notice's
path on each of the six sides and at the head, by sha256 [measured]:

- `one_turn` (finds, sends and marks the notice): changed by
  STOP-IN-CATALOG-WAIT and then HISTORY-NOTICE-STOP (built on it); QUESTIONS-20
  and PASTE-AFTER-END hold `main`'s; **the head's is HISTORY-NOTICE-STOP's**.
- `Conversation::unsaid`, `said`: HISTORY-NOTICE-STOP's alone; the head's is
  its.
- The screen's consumer: the `UiEvent::Notice` arm, `announce`, `mark`,
  `abandon_answer`, and `bridge::send_ui`: **the same on every side and at
  the head**. PASTE-AFTER-END's warning goes through `announce` too, but as
  the shell's own notice, never through the conversation's ledger.

So no merge changed a line on the notice's path that the unit's own tests
did not run with, and no new join test is needed. Its five tests and the pty
pin of a switch's notice said once
(`a_switch_off_llmux_sends_its_text_and_calls_and_leaves_the_signed_thinking_behind`)
**pass at the head** (`jobs-d.txt`), with joins 1, 2, 5 and 6's again.

### 4. Network hermeticity (`net.py`, `jobs-d.txt`, `connect.log`)

Train 7's method (FLAKE-CATALOG-EVENT's `connect_logger.py`, a loopback
stand-in for a proxy that logs each connection's first request line,
answers 403 and forwards nothing; the four proxy variables at it,
`NO_PROXY`, `no_proxy`, `ALL_PROXY`, `all_proxy` unset), on a
`git clone --no-local` at `ac0279c8`, one hold of the suite lock:

| run | result | CONNECTs logged during it |
|---|---|---|
| `cargo test --locked --lib --no-fail-fast` | 4559 passed, 0 failed, 7 ignored | **2**, both `CONNECT w.example:443` |
| `cargo test --locked --test tui --no-fail-fast` | 186 passed, 0 failed | **0** |

**No CONNECT to the Gateway host** [measured]. The two `w.example` lines are
the known MCP probe (trains 6 to 8).

## Gate (`gates.sh`, `gates.txt`, `gates-3.txt`, `gates-final.txt`)

`gates.sh` (trains 5's to 8's, its comment only changed): every step the
brief lists, in a `git clone --no-local` at the commit, one after another,
**the whole script inside one hold of the suite lock**, each step's exit
taken from `$?` on the line right after its command, tree clean before and
after. Run three times, each whole: after merges 1-2 (as the brief asked),
after merge 3 (the dispatcher: "rerun the gate steps the merge touches";
PASTE-AFTER-END touches `src` broadly, so all), and after merge 4 (the
dispatcher: "run the full gate"). **60 steps, 0 failed, none run again**
[measured].

| gate | `dfedcd1a` (merges 1-2) | `44c5d3e0` (1-3) | **`ac0279c8` (all four)** |
|---|---|---|---|
| `cargo fmt --check`; clippy; clippy `--features fault-injection` | ok; ok; ok | ok; ok; ok | **ok; ok; ok** |
| `cargo test --locked --no-fail-fast` | 5471 passed, 0 failed, 7 ignored (lib 4551, `tui` 184) | 5476 (4554, 186) | **5481 passed, 0 failed, 7 ignored (lib 4559, `tui` 186)** |
| the same `--features fault-injection` | 5508 (4574, 198) | 5513 (4577, 200) | **5518 passed, 0 failed, 7 ignored (lib 4582, `tui` 200)** |
| `check-xfx-identity` / `check-no-stubs` / `check-no-secrets` | ok / ok / ok (15989 files) | ok / ok / ok (16214) | **ok / ok / ok (16279)** |
| `check-preview-contract` / `check-tui-preflight-cost` / `check-usage-clock` | ok / ok (200x300 replay 1.29 ms) / ok | ok / ok / ok | **ok / ok (1.21 and 1.84 ms; budget 32) / ok** |
| `test-check-no-secrets` / `test-gate-env` / `test-preview-contract` | ok (13) / ok (62) / ok (24) | ok / ok / ok | **ok (13) / ok (62) / ok (24 mutants)** |
| `fxcap/test-sweep.sh` / `fxcap/test-capture.sh` | 70 + 27 OK / 76 OK | 70 + 27 / 76 | **70 + 27 OK / 76 OK** |
| release build; `fault-injection` release build | ok; ok | ok; ok | **ok; ok** |
| `smoke.sh` / `smoke-tui.sh` | 47, 0 failures / 1168, 0 failures | 47 / **1167** | **47 checks / 37 scenarios + the oracle, 1167 checks, 0 failures** |

Load at the steps' starts 5.7-17.3, 4.8-20.0 and 3.8-13.1 (other workers'
gates, sweeps and mutant runs beside each). smoke-tui's 1167 after merge 3
is PASTE-AFTER-END's own count (its receipt, both rounds): its scenario 7
asserts the kept paste where X1 asserted the refusal.

**Every side's tests are in** (`test-counts.txt`, train 7's count: lines
holding `#[test]` under `src` and `tests`): `main` 5374; STOP-IN-CATALOG-WAIT
+1 (its machine test is a `#[tokio::test]`, which the count does not read),
QUESTIONS-20 +7, the two join tests +2, PASTE-AFTER-END +4, the paste join
+1, HISTORY-NOTICE-STOP +5: **5394 at the head**. The harness's `def test_`
173 throughout (no unit adds one). `cargo test` 5481 against train 8's 5460
at `main`'s code: **+21** = 2 (STOP-IN-CATALOG-WAIT, both) + 7 + 2 + 4 + 1 +
5; with fault injection 5518 against 5497, the same +21.

## Sweeps

**Builds** (debug, as trains 4 to 8), each in its own `git clone --no-local`,
each swept with its own tree's harness, scenarios and ledger (`sweep.sh`,
train 7's, and its copies that read another selection file). fx: the
v0.0.16 binary, sha256 `8e6cda2e…` before and after each sweep,
`FX_AUTO_UPGRADE=0`; 40x120, `--jobs 2`; each sweep inside one hold of the
suite lock (`sweep-runs.txt`). Every reference sweep -- `main`'s three and
each unit's own -- was rescored with the final head's ledger (`sweep.sh
--rescore` on a copy, no capture taken); it moved no pass count. Every xfx
snap that shows the banner shows the build swept: `v0.1.0-dfedcd1` on all
280 of sweep 1's head, `v0.1.0-44c5d3e` on all 229 of the paste sweep's,
`v0.1.0-ac0279c` on all 493 of the final's, and `v0.1.0-ccced1f` on all 268,
207 and 128 of `main`'s three.

**Selections**, each group with why:
- `selection.txt` (the brief's): the stop, catalog and model scenarios --
  STOP-IN-CATALOG-WAIT's own selection widened to train 7's families
  (`p5-lifecycle-*`, `h1-models*`, `model*`, `p5-input-model*`,
  `settings-model*`, `interrupt*`, `compact-cancel`, `p5-transcript-cancel`,
  `*stop*`); `p5-input-question*` with QUESTIONS-20's own (`*question*`,
  `*approval*`); `p5-stop-in-catalog-wait`; the ledger entries the merges
  touched (`question-cap-four`, `question-resume-replay`: in the globs).
  81 scenarios at the head.
- `selection-p.txt` (the dispatcher's, merge 3): PASTE-AFTER-END's own 41
  input scenarios (every one that pastes, every `p5-input-*`, `question*`,
  `approvals*`), with QUESTIONS-20's two new ones: 43.
- `selection-h.txt` (merge 4): HISTORY-NOTICE-STOP's own twelve globs (the
  38 stop and history scenarios, `provider-carry-back-stop` among them).
- `selection-final.txt`: the three together, **124 scenarios**, swept at the
  final head. `main`'s side of each is its own sweep of the same selection;
  merge 4's re-point changed reference numbers only, which no sweep reads.

**Results** (`sweep-*.md`; `families-final.txt`; pass / pairs, each sweep
scored with the final head's ledger, over its own pairs):

| family | **final `ac0279c8`** | `main` | `main`, paste | `main`, history | STOP-IN-CATALOG-WAIT | QUESTIONS-20 | PASTE-AFTER-END | HISTORY-NOTICE-STOP |
|---|---|---|---|---|---|---|---|---|
| the stop scenarios (*stop*, interrupt*, compact-cancel, p5-transcript-cancel) | 79 / 92 | 75 / 86 | - | 64 / 73 | 3 / 3 | 4 / 4 | - | 67 / 79 |
| model and catalog (model*, h1-models*, p5-input-model*, settings-model*) | 77 / 92 | 77 / 92 | 26 / 32 | - | 59 / 70 | - | 26 / 32 | - |
| p5-lifecycle-* | 26 / 69 | 28 / 69 | - | 1 / 6 | - | - | - | 1 / 6 |
| p5-stop-in-catalog-wait | 3 / 3 | - | - | - | 3 / 3 | - | - | 3 / 3 |
| the question scenarios (p5-input-question*, question*) | 44 / 46 | 29 / 32 | 29 / 32 | - | - | 44 / 46 | 29 / 32 | - |
| the paste scenarios (p5-input-paste*, paste-*, *-paste) | 29 / 36 | - | 17 / 22 | - | - | - | 29 / 36 | - |
| provider* and resume* (HISTORY-NOTICE-STOP's) | 42 / 94 | - | - | 42 / 91 | - | - | - | 42 / 94 |
| provider-carry-back-stop | 0 / 3 | - | - | - | - | - | - | 0 / 3 |
| the approvals (approvals*, p5-input-approval) | 2 / 21 | 2 / 21 | 2 / 21 | - | - | 2 / 21 | 2 / 21 | - |
| every scenario swept | 419 / 590 | 212 / 305 | 188 / 239 | 112 / 176 | 62 / 73 | 51 / 76 | 200 / 253 | 115 / 182 |

The final sweep: 124 scenarios, **0 failed, 0 timed out**, 590 pairs,
**419 pass** (71.0%). The earlier head sweeps: `dfedcd1a`, 80 scenarios, 322
pairs, 231 pass, 0 failed; the paste sweep at `44c5d3e0`, 43 scenarios, 267
pairs, 215 pass, 0 failed. Each was compared as the final one is below, and
dropped nothing: sweep 1 against `main` **212 -> 216** (QUESTIONS-20's three
`q6` and `interrupt-retry` `f0.15` up, 0 down; `compare-main.txt`), against
STOP-IN-CATALOG-WAIT's own 62 -> 65 and QUESTIONS-20's 51 -> 51
(`compare-unit-*.txt`); the paste sweep against `main` **188 -> 195** (12 up,
0 down; `compare-p-pmain.txt`) and against PASTE-AFTER-END's own **200 ->
203** (8 up, 0 down; `compare-p-unit-paste-after-end.txt`).

### Against `main` (`compare-final-main.txt`, `-pmain`, `-hmain`; `screen-moves-final-*.txt`)

Over the pairs both took, three `main` sweeps:

- **selection.txt's 305 pairs: 212 -> 214; 4 up, 2 down.** Up:
  QUESTIONS-20's three `p5-input-question` `q6`, `q6-after`, `q6-after.full`
  (six questions now open fx's panel and Escape leaves `▪ Cancelled`); and
  `interrupt-retry` `f0.15`, which is a capture's timing, not a change: its
  xfx row at 0.15 s showed `Thinking` in `main`'s first run only, and the same
  pair passes in every other sweep folder on this machine's scratch that holds
  it, 27 of 29 (some are this receipt's rescored copies of one run), `main`'s
  own second run (11:32Z, the history sweep) included
  (`interrupt-retry-history.txt`).
  Down: `p5-lifecycle-retryok` `k00-0.5s` and `k01-1.5s`, **fx's screen moved
  and xfx's did not** (compare.py's own count: 2 of 2): fx's retry row at
  0.5 s read `retrying request` without its countdown, and at 1.5 s fx was
  still retrying where xfx (as on `main`) had `RECOVERED-ANSWER.`; load was
  16 at the sweep's end. **Swept three times more at the head, the scenario passes 3 of 3 each time** (`rerun-retryok.txt`, load 28.5 at the first). No unit touches the retry path
  (`git diff ccced1f5 99ef633f -- src/gateway` is empty).
- **The paste selection's 239 pairs: 188 -> 195; 11 up, 0 down.**
  PASTE-AFTER-END's four (`p5-input-paste` and `paste-suffix`'s `paste-enter`
  and `.full`: the paste kept, the warning written), QUESTIONS-20's three
  `q6`, and four `p5-transcript-submit*` pairs from normalized to identical
  (`main`'s run showed `1s` where fx showed `0s` in the turn's summary; a
  clock, `screen-moves-final-pmain.txt`).
- **The history selection's 176 pairs** (`main` has no
  `provider-carry-back-stop`): **112 -> 112, 0 moved.**
- Rows gained on pairs unresolved on both, each run-local text: the
  `/status` Workspace row's random temporary directory
  (`p5-lifecycle-relhome-unsaved` `u02-status`, `p5-panels-keys`'s seven,
  `p5-effort-fork-fork-edge` `status-empty`), a network notice's
  milliseconds (`interrupt-recovery` `full`), fx's retry countdown
  (`p5-lifecycle-retry` `r04a-esc-armed`, `6s` -> `7s`). Of the xfx snaps that
  moved beyond clock-like or run-local text, the rest are saved sessions'
  resume ids, the build id on `/status` and the banner.

### Against each unit's own sweep (`compare-final-unit-*.txt`)

| unit (its sweep's head) | shared pairs | pass, unit -> final | moved |
|---|---|---|---|
| STOP-IN-CATALOG-WAIT (`889dcfaf`, its 18) | 73 | 62 -> 65 | **3 up, 0 down**: `model-column-idle`'s three, which its own receipt names as FLAKE-MODELSDOWN's skew in that run (both its reruns pass 3/3) |
| QUESTIONS-20 (`dc2410c6`, its 13) | 76 | 51 -> 51 | **0** |
| PASTE-AFTER-END (`fd67ed2d`, its 41) | 253 | 200 -> 203 | **7 up, 0 down**: QUESTIONS-20's three `q6`, and the four `p5-transcript-submit*` clocks above |
| HISTORY-NOTICE-STOP (`dec785aa`, its 35) | 182 | 115 -> 115 | **0**; one row gained, a network notice's milliseconds |

**So no pair that passes on any unit's own sweep fails on the train, and the
two that pass on `main`'s and not on the final are fx's screen alone.**

## Redaction

The repository is public. Every file here taken from a sweep, a gate log, a
mutant run, the logger or a clone went through `redacted.py` (train 7's,
unchanged): `scripts/fxcap/redact.py`'s `Redactor.write`, every path under
a local root to `<local-path>` or a named root (`<scratch>`, `<clone>`,
`<repo>`, `<sweep-…>`), a checkout's path cut short to `<checkout>/`, the
account's name to `<user>`. The scripts take the oracle's path, the suite
lock and the scratch directories as arguments or variables. Over every file
of this directory, the five forms trains 5 to 8 grep for -- each spelt with a
bracket class so the command does not match its own text: **0 hits**
(`redaction-grep.txt`). Over the whole train, `git diff origin/main..HEAD`'s
added lines holding the home root, the account's name, the scratch root or
the uid fragment: **0** (the dispatcher's check, `gates-files.txt`).

## Found on the way, not changed here

- **The historical plans' references.** QUESTIONS-20 and PASTE-AFTER-END
  move code under 38 references in `docs/superpowers/plans/`; left for
  REF-LINT, as train 8 left the plans (`stale.txt`, `refs-all.txt`).
- **Two references named other code on `main` already** (`smoke-tui.sh`'s
  `input.rs:702`, `event_loop.rs`'s `event_loop.rs:972-975`): re-pointed by
  line so they name what they named, not fixed to name what their text
  says (`stale.txt`).
- **A reference re-pointed and then moved by the next merge**
  (`shell.rs:38615` -> `:38636` -> `:38716`): `refs-head.txt` reads only a
  side's own citation, so a number written on this branch and moved by a
  later merge is invisible to it; `repointed.py` caught it. A train that
  re-points between merges has to run `repointed.py` after the last one.
- **One shell `&` in a wait loop** during this train (01:52Z), against the
  brief's rule: the loop was found by its command line and killed by its
  PID; it started nothing.

## Files

- `remerge-1.txt` .. `remerge-4.txt`: `git show --remerge-diff` of each
  merge. `overlap.py`, `overlap.txt`: every file both sides of a merge
  changed, and the nearest pair of hunks.
- `merge-checks.sh`: writes the token check, its control, the
  remerge-diffs, the reference maps, the re-points' check, the stale list,
  the invariant, the shared-code check and the overlaps.
- `token_check.py` (train 7's, byte for byte), `token-check.txt`,
  `token-self-test.txt`.
- `refs.py`, `refs_all.py`, `refs_short.py` (train 7's); `refs.txt`,
  `refs-all-merges.txt`, `refs-short-merges.txt` (at `b9409b36`, before the
  re-points); `refs-head.txt`, `refs-all.txt` (at `9aa61e28`); `repoint.py`
  (both re-point commits' edits), `repointed.py`, `repointed.txt`;
  `stale.py`, `stale.txt`.
- `join_test_worker.rs`, `join_test_tui.rs`, `apply_join.py` (join check 2);
  `join_test_paste.rs`, `apply_join_paste.py` (join check 5);
  `join_units.sh`, `join_units_paste.sh` (each side alone);
  `join_mutants.py` (the mutants); `invariant.py`, `invariant.txt` (join
  check 6); `shared.py`, `shared.txt` (join check 7).
- `jobs-a.sh` / `jobs-a.txt` (the named tests and the sides alone, at
  `44c5d3e0`), `jobs-b.sh` / `jobs-b.txt` (the mutants), `jobs-c.sh` (the
  network alone; not run: `jobs-d.sh` took it at the final head),
  `jobs-d.sh` / `jobs-d.txt` (join check 7's tests, the others' again, the
  network), `net.py`, `connect.log`.
- `gates.sh`, `gates_txt.py`; `gates.txt` (`dfedcd1a`), `gates-3.txt`
  (`44c5d3e0`), `gates-final.txt` (`ac0279c8`); `gates-files.txt`: the file
  checks with the receipt in its final form.
- `test_counts.sh`, `test-counts.txt`.
- `selection.txt`, `selection-p.txt`, `selection-h.txt`,
  `selection-final.txt`; `sweep.sh`, `sweep-p.sh`, `sweep-h.sh`,
  `sweep-final.sh` (the same script, each reading its selection);
  `sweep-rerun.sh`; `sweep-runs.txt`: every sweep's build, oracle and load
  lines; `sweep-*.md`: each sweep's scoreboard (rescored with the final
  head's ledger); `sweep-compare.sh`, `compare.py`, `screen_moves.py`,
  `families.py`, `families_p.py`, `families_final.py`; `compare-*.txt`,
  `screen-moves-*.txt`, `families*.txt` -- the unprefixed ones sweep 1's
  (`sweep-head.md` is its head), `-p-` the paste sweep's, `-final-` the
  final's; `rerun-retryok.txt`; `interrupt-retry-history.txt`.
- `redacted.py`; `redaction-grep.txt`.
