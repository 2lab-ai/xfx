---
title: 0.0.16 delta -- QUESTION-RESUME receipt (a resume replays each question batch)
type: receipt
tags: [xfx, tui, phase5, question, resume]
---

# QUESTION-RESUME: a resumed session replays its question batches

QUESTIONS-20 ported the question panel and its batches of 20. Its reviews left
one gap to this unit: on a resume, fx v0.0.16 replays an answered batch into the
transcript and xfx wrote nothing for it. On `main` at the base, the ledger
cause `question-resume-replay` covered `p5-input-question20`'s two `resumed`
snaps. The base is `origin/main` at `32afd307`.

| commit | what |
|---|---|
| `3bd76ddb` | RED: the five shell tests (written against the base's API), the new scenario `p5-question-resume`, `fx/`, `red/`, `red.txt`, `red-tests.txt`, `compare.py` |
| `a623824c` | **the code head**: the decoder, the hydration's two steps, the shell's rows, the inert step, the unit and hydration tests, `docs/parity.md`, the ledger cause retired |
| `47384d15` | the Ctrl-O gap measured: scenario `p5-question-resume-ctrlo`, ledger cause `question-resume-ctrlo-detail`, the parity row's last sentence. No source change |
| `a5861b54` | the Ctrl-O cause's `rows = ["1-40"]`, so the sweep attributes the blank rows xfx's shorter screen leaves (`sweep.py` `attribute`) |
| `1f4b3d03` | round 1's receipt |
| `300b98fc` | round 2 RED: the object-only rejects (`[["Q","A"]]` and its mixes) at the decoder, the hydration and the shell; the `read, read, ask` and `ask, read` tests; `red-tests-r2.txt` |
| `63e8bbeb` | round 2's fix: `decode_answers` read by hand (objects only; a twenty-first refused after `IgnoredAny` scanned it, which round 3 fixed; a key given twice in an answer's own object refused, not inside a skipped value); the parity row's Escape, sentinel and questioner sentences; the failed-call mapping said in the row and the hydration. The captures and the sweep are this commit's |
| `56994078` | round 2's receipt |
| `37cdd637` | round 3 RED: a twenty-first element must be refused unread (fails: `Eof` where `Data` is wanted); the two looser documents pinned (pass); `red-tests-r3.txt` |
| `9a2921ce` | round 3's fix, **the code head**: the rejecting seed; the looseness said in `decode_answers`'s doc, the parity row and here; `docs/parity.md`'s full-screen TUI row's Escape sentence; the row's sentinel paths and `:15601-15605`; `input_question_runtime.zig:59-81` in `fx-src.txt` |
| `678a9552` | round 3's receipt: `mutants.*` (16 of 16 at `9a2921ce`) |
| round 4 | words and one test: the 8 MiB tail unclosed, so every tail of `the_twenty_first_element_is_refused_at_its_first_byte_unread` fails with round 2's scan (`red-tests-r4.txt`); the seed's value type uninhabited; Zig 0.17 marked inferred; the Escape-at-draft qualification; this file's false lines; `mutants.*` rerun |

`git diff 63e8bbeb HEAD -- src` is `src/tools/question.rs` alone: the
decode of a document past twenty elements, comments and tests. No capture
holds such a document, so `xfx/`, `green.txt` and the sweep stand at
`63e8bbeb`; the gate runs on the round-4 head (the dispatcher's).

The captures in `xfx/` come from the binary built at `63e8bbeb` (row 1:
`x𝒇x v0.1.0-63e8bbe [debug]`, sha256 `8b3ce14f0854…`); round 1's, from
`a623824c` (`0aeba9aac5a8…`), are in `1f4b3d03`. Those in `red/` come
from the base, built in a throwaway clone at `32afd307` (`v0.1.0-32afd30`,
`d105571b9c8c…`). fx is the v0.0.16 release, sha256 `8e6cda2e…`, run with
`FX_AUTO_UPGRADE=0`; `capture.sh` checked after every run that its bytes had
not changed.

## How it was captured

```
FX_BIN=<fx v0.0.16> TMPDIR=<short dir>  scripts/fxcap/capture.sh fx  <scenario> <size> fx/
XFX_BIN=<xfx>       TMPDIR=<short dir>  scripts/fxcap/capture.sh xfx <scenario> <size> {red,xfx}/
```

- `p5-question-resume` (**new**, 40x120): a batch of 1 and a batch of 4
  answered by ordinal; a batch of 21 and a batch of none, refused before any
  panel opens; one step of a failed read, an empty batch and another failed
  read (`ask mixed`); a batch of 2 cancelled with Escape, which stops its
  turn. `live` (and `.full`) is the screen before `/quit`; `resumed` (and
  `.full`) is the relaunch with the resume flag.
- `p5-input-question20` (on `main`, unchanged): one batch of 20, answered, then
  resumed. At 40x120, 24x80 and 14x72, every snap.
- `p5-question-resume-ctrlo` (**new**, 40x120, `TZ=UTC`): a batch of 1
  answered and a batch of none refused, resumed, then Ctrl-O (`resumed-ctrlo`).
  The red and head captures of it are from the same two binaries as above
  (by sha256); its screen is the full transcript, which has no banner row.
- Only `.txt` and `.ansi` are kept. The request bodies are left out: fx's name
  the run's scratch folder, and this unit makes no claim about the wire (the
  resumed session's history reaches the model as before).

## What fx v0.0.16 does (`fx-src.txt`)

`fx-src.txt` holds every fx line cited here, read with `git show 481104fd:<path>`.

- **How the saved call is decoded.** A resume replays each step's calls in
  order, each call followed by its result (`writeExecutionHistoryToSink`,
  `app_session_runtime.zig:4120-4127`). Each result goes first to
  `writeAnsweredQuestionResult` (`:4210`, `:4589-4609`). That function takes
  only a **successful** result of `ask_user_question` (`:4595-4597`) and
  decodes its output with `decodeJson` (`:4601-4604`). `decodeJson`
  (`question_answer.zig:41-74`) parses the output as JSON. It returns null
  for anything that is not an array of 1 to `max_questions` = 20 objects
  (`:7-9`, `:57-59`), each with a string `question` and a string `answer`
  (`:62-71`). fx's own tests list the rejects (`:101-111`) and the bound at 20
  and 21 (`:113-141`).
- **Answered** (it decodes): the batch's resolutions, composed at the screen's
  width (`writeHistoricalQuestionResolution`, `main.zig:2335-2346`). The
  composer is the live panel's (`composeResolvedQuestionAnswers`,
  `question_ui.zig:446-459`): `  N) <question>`, with the answer under it in
  the statusline grey (`:461-511`). They stand where the call stood in its
  turn. fx's e2e checks all twenty and no `● Asked` after a resume
  (`tui-resume.test.ts:6174-6185`).
- **Anything else** (a refusal, the declined sentinel, a document that does not
  decode, or one past 20) takes the generic path (`:4211-4335`). fx's own test
  pins this (`execution replay keeps invalid question results on the generic
  path`, `app_session_runtime.zig:7991-8035`). fx's refusal is a successful
  call's text (`ask_user_question.zig:99-134`), so it takes the completed
  branch (`:4275-4295`). That branch writes the tool's completed label `Asked`
  (`builtins/tools.zig:616`) as a completed status (`:4316-4335`):
  `● Asked`, the marker in `system_notice_text_style`, 250 dark and 241 light
  (`transcript/runtime.zig:613-616`, `:661-671`; `core/shared/theme.zig:85`, `:128`).
  The projection never puts that row in a group, and it ends a run of calls
  (`statusNamesAsk`, `tool_group_projection.zig:235-244`, `:1355-1377`).
- **Cancelled** (Escape at the panel): the stop leaves the call no result to
  replay. The turn's own cancellation line stands alone:
  `▪ Cancelled · What can fx do differently?` (`fx/…-resumed.full.txt`).
- **Batches of 1, 4 and 20** are the same rule (`fx/…-question-resume-…`,
  `fx/…-question20-…`).
- **An older session.** The document's shape has not changed: c1db9191 wrote
  and read the same array (`question_answer.zig:10-33`, `:35-68@c1db9191`), with
  the bound at 4 (`:54`). So an older session's batch of 4 or fewer decodes
  under 20 the same way, and fx keeps no other saved shape.

## RED (`red.txt`, `red-tests.txt`)

- `red.txt` is `compare.py` over every pair of `fx/` against `red/`: 29 of
  38 identical (27 raw, 2 normalized) and 9 differing. The 9 are every
  resumed pair:
  - `p5-question-resume` `resumed` and `.full`: no resolutions under `ask one`
    or `ask four`, and no `● Asked` under `ask too many`, `ask none` or `ask
    mixed`, whose two reads replay as one group;
  - `p5-input-question20` `resumed` and `.full` at all three sizes: none of
    the twenty resolutions;
  - `p5-question-resume-ctrlo` `resumed-ctrlo`: the same, in Ctrl-O.

  Every snap before a resume matches, `ask mixed`'s live group included.
- `red-tests.txt` is the five shell tests at `3bd76ddb`, whose `src/` is the
  base's plus the tests. Four fail, each with its left/right; the fifth (a
  cancelled batch) passes at the base and is kept as a guard. At the code head
  these tests differ from the RED commit only by clippy's `.contains(&row)` for
  two `.iter().any(|row| *row == …)` and one rustfmt wrap
  (`git diff 3bd76ddb a623824c -- src/tui/shell.rs`).
- `red-tests-r2.txt` is round 2's RED at `300b98fc`, whose `src/` is
  `1f4b3d03`'s. Three tests fail on `[["Q","A"]]`: the decoder returns
  `Some([SavedAnswer { question: "Q", answer: "A" }])`, the hydration makes
  `answered 1`, and the shell draws `  1) Q` / `     A` where fx draws
  `● Asked`. The two ordering tests pass there; they pin behaviour that was
  already right.

## The change (`a623824c`, then `63e8bbeb` and `9a2921ce`)

- `tools::question::decode_answers` is `decodeJson`, read by hand (round 2;
  round 1's derived struct also took serde's positional form, so
  `[["Q","A"]]` was an answer):
  - the array's visitor reads element by element and refuses a twenty-first
    at its first byte, its value unread (round 3: a seed that never calls the
    deserializer, `TwentyFirst`; round 2's `IgnoredAny` scanned it), and
    refuses an empty one (`MIN_QUESTIONS`, `MAX_QUESTIONS`, the tool's own
    constants);
  - each element is read as an object only (`deserialize_map`): an array, a
    string or a number in its place is refused (`question_answer.zig:63`);
  - `question` and `answer` must be strings; a key given twice in the
    answer's own object is refused, read or not, as fx's `std.json` refuses
    it (`duplicate_field_behavior = .@"error"`, `std/json/static.zig:20-26`,
    `dynamic.zig:149-156`); other keys are skipped with `IgnoredAny`;
  - **looser than fx in one place**: a skipped value is not checked, where
    fx's parse refuses a key given twice in any object and an unpaired
    surrogate in any string (the same lines; `Scanner.zig:25-32`). These are
    Zig 0.16.0's std lines; the pin builds with 0.17.0
    (`build.zig.zon:28@481104fd`), whose std is inferred, not read, to be
    the same: the release binary was not run on such a session, because no
    fxcap step rewrites a saved session between a run and its resume and fx
    checksums each line over the run's own path (`log.zig:30-34`). So
    `[{"question":"Q","answer":"A","x":{"k":1,"k":2}}]` and the same with
    `"x":"\ud800"` are answered batches here and, by those lines, null in
    fx. Only a
    hand-forged log holds them, and the rows are drawn inert;
    `a_skipped_value_is_not_checked_as_upstreams_parse_checks_it` pins both;
  - anything after the array, and invalid JSON, are refused. Nothing goes
    through `serde_json::Value`, whose parse stops at a depth of 128.
- `tui::hydration` takes an `ask_user_question` result by its own tool name,
  as fx does, and makes one of two steps:
  - `Answered(pairs)`: the call succeeded and its document decodes;
  - `Asked`: anything else.

  A step that asked a question holds its other calls' starts. Each is
  written just before its own result, so the ask's row ends the group before
  it. Steps without a question are unchanged. Pairing a start with its result
  by id is call order because xfx runs a step's calls one at a time, in call
  order, and saves each result as it comes back (`agent/machine.rs:1197`, the
  loop; `:1257`, `SessionEvent::ToolResult`).
- `tui::question::resolution_rows` and the live `answered_rows` are one
  function, so a replayed record and a live one are the same rows.
  `asked_row` is `● Asked`: `Palette::notice` (250/241) and the tool's own
  `PRESENTATION.done`. The shell writes both as blocks. A block closes an
  open group (`write_block`).
- **xfx's refusal.** xfx records a refused batch as a failed call
  (adaptation (7), kept), where fx's is a successful call's text. The replay
  shows `● Asked` for it, as fx does for the same refusal, because the rule
  above draws `● Asked` for anything that is not a decoded successful batch.
  For a failed call that grey row is xfx's own mapping: fx draws a failed
  call's marker red (`lifecycleMarker`, `transcript/runtime.zig:616`). xfx's
  failed asks are its refusals, which fx records as successes.

## Bounds and safety (the tests in `tools/question.rs`, `tui/hydration.rs`, `tui/shell.rs`, `tui/worker.rs`)

A saved session is file input. Each bound fx has is kept but one -- a
skipped value's own checks (above, "looser than fx in one place") -- and
each has a test:

- **Count 1 to 20, refused while parsing.** `a_saved_document_of_one_to_twenty_questions_decodes_and_no_other_count_does`
  checks 1, 4 and 20 (Some) and 21 and 0 (None); the visitor is the only
  count check, so mutant M13, which removes its stop, is killed by it. The shell's
  `a_resumed_batch_that_left_no_answered_document_is_one_asked_row` replays a
  21-answer document and `[]` as `● Asked`.
- **Shape.** `anything_but_an_answer_document_is_none_and_never_a_panic`
  covers 25 rejects: fx's five, the sentinels, a non-array, a non-object
  element (a string, a number, `[["Q","A"]]`, `[["Q","A","x"]]`, and an array
  beside an object either way), a missing, null or non-string field, `question` given
  twice, an unread key given twice, a truncated array, and a lone surrogate
  in `question`.
  It also checks:
  - an unread key nested 100 000 deep is skipped without recursion (no
    stack to overflow), and decodes, as fx's `std.json` keeps it on the heap;
  - 100 000 nested arrays where an answer is read are None.
- **Status.** A failed call whose output is a valid document is not an
  answered batch: `an_ask_replays_its_decoded_batch_or_one_asked_row_and_never_a_tool_row`,
  and the shell test's `(false, valid)` case.
- **Terminal safety.** The decoded texts go through `bridge::inert_hydration`
  (the new `Answered` arm) and then the rows' `safe_rows`/`answer_safe`.
  - `a_hostile_answer_document_paints_no_sequence_of_its_own`: an OSC title,
    a clear, a bell and a C1 CSI in both texts paint nothing of their own.
  - `a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker` now
    carries an answered batch with the same bytes. It finds them inert both
    ways in, the launch's and the picker's.
- **Size.** fx bounds no field, and neither does this decode. xfx writes a
  frame of at most 1 MiB (`MAX_EVENT_FRAME_BYTES`, `session/event.rs:56`,
  checked on encode at `:621`). A longer, hand-written line is read whole by
  the session reader, as any event is. The decode is linear in a text the
  reader already holds, as fx's is. It refuses a count past 20 at the
  twenty-first element's first byte, without reading its value; fx parses the
  whole array first (`question_answer.zig:47-59`).
  `the_twenty_first_element_is_refused_at_its_first_byte_unread` feeds four
  twenty-first values whose reading fails -- a truncated object, an unclosed
  100 000-deep array, an 8 MiB string with no closing quote, a control byte
  -- and gets the count's error for each (`Category::Data`, `invalid length
  21`). With round 2's `IgnoredAny` back (mutant M16), each one is read and
  gives its parse error instead: `Eof` at columns 975, 100 963 and 8 389 572,
  and `Syntax` at 964 (`red-tests-r4.txt`). Round 3's version of the test
  closed the 8 MiB string, which a scan reads whole without error, so that
  input did not tell the two apart (`red-tests-r3.txt` shows the first).

## GREEN (`green.txt`)

`green.txt` is `compare.py` over every pair of `fx/` against `xfx/` at
`63e8bbeb`. **37 of 38 are identical**. 31 are identical cell for cell. 6 are
identical after the ledger's normalizations, which `compare.py` names:

- `turn-wall-time`: `p5-input-question20` at 40x120, `q20-answered` and
  `resumed`, each with its `.full`, a turn's seconds;
- `product-name`: `p5-question-resume` `resumed` and its `.full`, the turn's
  `What can fx do differently?`.

Every `resumed` pair is identical: `p5-input-question20` at 24x80 and 14x72
(raw) and 40x120 (normalized), and `p5-question-resume` (normalized). Round
1's `green.txt` at `a623824c` had the same 37 (32 raw, 5 normalized; which
rows a second apart is the run's clock).

The one that differs is `p5-question-resume-ctrlo` `resumed-ctrlo`, Ctrl-O
after a resume. It differs at the base too (`red.txt`). Its rows are covered
by the new ledger cause `question-resume-ctrlo-detail`.

## Ctrl-O after a resume: measured, not ported (`fx/`, `xfx/` `*-resumed-ctrlo*`)

fx hands each replayed ask the call's saved detail (`attachHistoricalToolDetail`
after `appendQuestionResolution`, `app_session_runtime.zig:4607`, and after the
generic status, `:4337`). So its Ctrl-O shows two things xfx's does not:

- under the resolutions, the call's arguments (`│  questions: [...]`) and its
  saved result (`│  [{"question":…}]`);
- under `● Asked`, the same two, with the entry headed `<ts> UTC · Tool`.

xfx's replay writes both rows as blocks with no call behind them. Its Ctrl-O
shows the same resolution rows and the same `● Asked`, with nothing under
them, and the rows after them stand higher.

This is the full transcript's call model, not the replay this unit ports.
The ledger cause names the owner, **QUESTION-RESUME-CTRLO** (queued), and the
parity row says so. The live Ctrl-O matches: a live ask has no row in it on
either side.

## Mutants (`mutants.py`, `mutants.txt`)

`mutants.py` ran on a throwaway `git clone --no-local` of the repository at
`9a2921ce` (round 3), never on the worktree, under one suite-lock slot. Each mutant is
one or two exact substitutions (each asserted to match once), then the
unit's tests, then `git checkout -- .`. **16 of 16 killed** (round 2: 15 of 15 at `63e8bbeb`; round 1: 11 of 11 at `a623824c`); the clone was clean after the run.

| mutant | what it puts back or breaks |
|---|---|
| M1 | the replay's ask branch removed (`if tool == ASK_USER_QUESTION.name()`): the ask's result goes to the group as a tool result, and its start, still dropped by the filter at `hydration.rs:251`, is not there to settle, so nothing is drawn -- the base's screen |
| M2 | the group's own early return removed: a live question becomes a group row |
| M3, M4 | the upper bound one past 20, one short of 20 |
| M5, M6 | the lower bound one below 1 (an empty batch), one above 1 |
| M7 | a malformed document drawn as answered: a missing `question` defaults |
| M8 | a failed call's document drawn as answered: `ok` ignored |
| M9 | a document that does not decode drawn as an (empty) answered batch |
| M10 | a step that asked starts its calls together |
| M11 | the decoded texts not made inert at the bridge |
| M12 | the derived struct put back (round 1's decode), which also takes the positional form |
| M13 | the early count stop removed: any count past twenty decodes |
| M14 | `question` or `answer` given twice accepted |
| M15 | an unread key given twice accepted |
| M16 | `IgnoredAny` put back for the twenty-first (round 2's code): its value is scanned before the count's refusal; killed by `the_twenty_first_element_is_refused_at_its_first_byte_unread` |

`mutants.txt` names the tests each one failed.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-diff.txt`)

`scripts/fxcap/sweep.sh --only '*question*' --only '*resume*' --jobs 4` at
40x120: 22 scenarios, 125 snap pairs, with this branch's scenarios and final
ledger. It ran once with the base binary (`32afd307`) and once with the
round-2 code head's (`63e8bbeb`), both in one suite-lock slot. The
scoreboards are the sweep's own, redacted by it. `sweep-diff.txt` is every
pair's verdict at both. Round 1's, the head at `a623824c`, gave the same
figures (in `1f4b3d03`).

- **base 87 / 125, head 91 / 125.** Four pairs moved to passing, all
  targets of this unit: `p5-input-question20` `resumed`, `resumed.full`, and
  `p5-question-resume` `resumed`, `resumed.full`.
  - `p5-input-question20` `q20-answered` went from normalized-identical to
    identical, and `p5-question-resume` `live` the other way: a turn's wall
    seconds, the run's clock.
- **None dropped**: no pair that passes at the base fails at the head.
- At the head, every pair this unit touches either passes or is covered by
  a cause:
  - `p5-question-resume-ctrlo` `resumed-ctrlo` is all `question-resume-ctrlo-detail`;
  - `question-resume-replay` is retired, and no row needs it.
- 13 pairs keep rows no cause names, the same 13 at the base: `p5-lifecycle-v1resume`
  (4), `p5-transcript-question` `asked`, `narrow` (QUESTIONS-20 listed both),
  `provider-resume` (4) and `provider-resume-missing` (3). This unit neither
  made nor touches them.

## Gate (`gate.txt`, `gate2.txt`)

- `gate.txt` is the brief's gate at `63e8bbeb`, the round-2 code head. It ran
  under one suite-lock slot with a short `TMPDIR`, and every step exited 0:
  - `cargo fmt --check`, and `cargo clippy` with and without
    `fault-injection`;
  - `cargo test --locked --no-fail-fast`: 15 suites, 5550 passed, 0
    failed, 7 ignored;
  - the same with `--features fault-injection`: 5587 passed, 0 failed, 7
    ignored;
  - `check-xfx-identity`, `check-no-stubs`, `check-no-secrets`.

  Round 1's, at `47384d15`, was 5549 and 5586 (in `1f4b3d03`).
- `gate2.txt` is CI's remaining quick steps and `sweep.yml`'s two harness
  tests at `63e8bbeb` with the final ledger, also under the lock.
  - all 8 steps exited 0: `check-no-stubs`, `test-gate-env` (62 cases),
    `check-preview-contract`, `test-preview-contract` (24 mutants caught),
    `check-tui-preflight-cost`, `check-usage-clock`, `fxcap/test-sweep.sh`,
    `fxcap/test-capture.sh`.
  - Not run: CI's release build, `smoke.sh` and `smoke-tui`.

## Files

| file | what |
|---|---|
| `fx/`, `red/`, `xfx/` | the captures: fx v0.0.16, xfx at the base, xfx at the round-2 code head `63e8bbeb` |
| `compare.py` | the pair comparison: raw `cells.py`, then the ledger's normalizations |
| `red.txt`, `green.txt` | `compare.py` over `fx/` against `red/` and against `xfx/` |
| `red-tests.txt`, `red-tests-r2.txt`, `red-tests-r3.txt`, `red-tests-r4.txt` | the RED tests at `3bd76ddb`, round 2's at `300b98fc`, round 3's at `37cdd637`, round 4's on a clone at `678a9552` with M16 |
| `fx-src.txt` | every fx line cited, at `481104fd` (and `c1db9191` for the old bound) |
| `probe/` | live Ctrl-O (`qr-ctrlo.cap`, not a committed scenario): fx and xfx at the code head, `compare.txt` |
| `mutants.py`, `mutants.txt` | the mutants and their results |
| `sweep-base.md`, `sweep-head.md`, `sweep-diff.txt` | the sweeps' scoreboards (rescored with the final ledger) and every pair's verdict at both |
| `gate.txt`, `gate2.txt` | the gate |

