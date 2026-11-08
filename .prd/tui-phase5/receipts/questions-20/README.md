# I-U6 receipt: `ask_user_question` takes 1 to 20 questions (QUESTIONS-20)

The unit is the input cluster's U6, rule I11: fx v0.0.16 (`vercel-labs/fx@481104fd`,
commit `28cf53b5` *Allow up to 20 questions in one ask_user_question call*) raises
the cap on one call from 4 to 20. The spec is
`research/tui-phase5:.prd/tui-phase5/research/input.md` §2.6, §3 I11 and §5 U6;
its line numbers were leads, and the lines below were read again at the pin. The
base is `main` at `712dd8d1`.

| commit | what |
|---|---|
| `eeed1e88` | RED: the tests, the two new scenarios, `fx/`, `red/`, `probe/`, `red.txt` |
| `eef9fd5f` | **the code head**: the cap, the description, the refusal, the worst-case bound, the panel tests, `docs/parity.md` |
| `dc2410c6` | a resize step in the 20-question pty test; the ledger's cause swapped |
| `297c2523` | this file, `xfx/`, `green.txt`, `fx-src.txt`, the sweeps; comment-only edits in `src/` and `tests/` (below) |
| `803aaa5f` | `mutants.py`, `mutants.txt` -- **the gate ran here** |
| the last commit | `gate.txt`, `src-after-code-head.diff`, `sweep-head-rescore.md`; the ledger cause's row selectors |

The xfx captures in `xfx/` are from the binary built at `eef9fd5f` (row 1:
`x𝒇x v0.1.0-eef9fd5 [debug]`, sha256 `753fd4b5ceb2…`); those in `red/` and
`probe/` from the base (`v0.1.0-712dd8d`, `90f8098de8e6…`). fx is the v0.0.16
release, sha256 `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0`; `capture.sh`
checked after every run that its bytes had not changed. The commits after the
code head change tests, comments and the ledger only, so they do not change
what the binary does: `src-after-code-head.diff` is `git diff eef9fd5f
803aaa5f -- src`, five lines, each a `//` or `///` comment: three references
to `mod.rs:145` re-pointed to `:346`, where `Registry::execute` clips, and two
fx test ranges read again at the pin.

## How it was captured

```
FX_BIN=<fx v0.0.16> TMPDIR=<short dir>  scripts/fxcap/capture.sh fx  <scenario> <size> fx/
XFX_BIN=<xfx>       TMPDIR=<short dir>  scripts/fxcap/capture.sh xfx <scenario> <size> {red,xfx}/
```

- `p5-input-question` (on `main`): two questions, Escape, then six, Escape. 40x120.
- `p5-input-question20` (**new**): one batch of 20, `Pick N?` with `Keep N` and
  `Change N`. The panel (`q20-panel`), Left at the first page (`q20-left`), Tab
  ×9 to page 10 (`q20-tab10`), Left (`q20-back9`), Tab ×11 to page 20
  (`q20-tab20`), Tab past the last (`q20-wrap`), one ordinal (`q20-first`), the
  other nineteen in one write (`q20-answered`, `.full`), then `/quit` and a
  relaunch with the resume flag (`resumed`, `.full`). At **40x120, 24x80 and
  14x72**: the short and narrow sizes are where the counter has no room.
- `p5-input-question21` (**new**): 21 questions, then none; each turn goes on to
  its answer. 40x120.
- `probe/`: the base binary and fx on a two-question batch answered and resumed,
  to tell whether the resume gap is the cap's. The scenario was not committed;
  it was `p5-input-question20.cap`'s first half on a two-question batch:

  ```
  fx: env FX_AUTO_UPGRADE=0
  gateway <the first two questions of p5-input-question20-turns.json>
  wait 4
  type ask two
  key Enter
  wait 3
  type 12
  wait 3
  snap answered
  keep
  type /quit
  wait 0.5
  key Enter
  wait 2
  fx: relaunch --resume-last
  xfx: relaunch --resume last
  wait 5
  snap resumed
  ```

## What fx v0.0.16 does (`fx-src.txt`)

`fx-src.txt` holds every line cited here, read with `git show 481104fd:<path>`
(and `c1db9191:` for the old side).

- **The cap.** `max_questions: u32 = 20` (`question_answer.zig:7-9`) is shared by
  the schema's `maxItems` (`builtins/tools.zig:607`), the description built from
  it, `Ask the user 1-{d} multiple-choice questions …` (`:119-122`), the parser's
  bound (`ask_user_question.zig:190-191`) and saved-answer decoding
  (`question_answer.zig:43-74`, the bound at `:59`). Old: 4 in each place
  (`question_answer.zig:54`, `builtins/tools.zig:169-170`, `:708`,
  `ask_user_question.zig:134@c1db9191`).
- **The refusal, at 0 and at 21.** One branch, `items.len < 1 or items.len >
  max_questions`, gives `(ask_user_question: provide 1 to {d} questions)`
  (`ask_user_question.zig:18-22`, `:261`), before any prompt opens. The refusal
  is the call's **output** and the call **succeeds** with it
  (`executeWithRequester` `:117-122`, `call` `:104-114`): fx's bodies carry it as
  `{"type":"text"}` (`fx/fx-p5-input-question21-40x120-bodies/003-completion.json`,
  `005-…`).
- **Per question**: 2 to 6 options in the schema (`builtins/tools.zig:130-136`)
  and the parser (`ask_user_question.zig:208-209`, `provide 2 to 6 options per
  question`, `:268`). 28cf53b5 does not touch them.
- **Size budgets.** fx bounds no field (the encoder runs with `maxInt(usize)`,
  `ask_user_question.zig:249-253`); the result goes through the 64 KiB budget
  every tool has (`tool_result_limits.zig:7`), which does not scale with the
  count. The prompt's page index is a `u8` (`question_prompt.zig:119`), and fx's
  test of a full batch asserts 20 fits it (`:1702-1704`). The hint row's
  buffers are 512 and 32 bytes (`input_presentation.zig:483-485`, `:231`).
- **The panel at large M.** One question at a time; batch progress is on the
  hint row, right-aligned, and only where it leaves two cells of air beside the
  keys, which fall back to a shorter variant when they do not fit
  (`input_presentation.zig:192-246`). Measured on fx's screens:
  - 40x120: `Question 1 of 20`, `Question 10 of 20` and `Question 20 of 20` all
    show (`fx/…-40x120-q20-panel`, `-q20-tab10`, `-q20-tab20`);
  - 14x72: the short keys leave room for `Question 1 of 20` and not for
    `Question 10 of 20` (`fx/…-14x72-q20-panel`, `-q20-tab10`);
  - 24x80: the full keys fit and no counter does (`fx/…-24x80-q20-panel`).
  - Tab wraps past the last page (`cycleQuestionPage`, `question_prompt.zig:670-676`;
    `q20-wrap`), Left goes back and stops at the first (`retreatToPrevious`,
    `:609-615`, bound to Left and word-left, `input_question_runtime.zig:189-195`;
    `q20-left`, `q20-back9`), and a decision moves to the next unanswered page,
    cycling (`:657-667`; `q20-first`).
  - fx's own e2e of a full batch resizes to 48x24 at page 14 and back
    (`tui-decision-prompts.test.ts:2531-2590@481104fd`).
- **The resolutions.** `  N) <question>` and the answer under the question's
  first character, the indent the prefix's own width, so `  10) ` hangs six
  cells (`writeQuestionAnswerResolution`, `question_ui.zig:461-481`;
  `q20-answered.full`).
- **A resumed session.** An answered batch is replayed as the same resolutions:
  `writeAnsweredQuestionResult` decodes the saved document, 1 to 20 questions
  (`app_session_runtime.zig:4589-4607`), and fx's e2e checks all twenty after a
  resume (`tui-resume.test.ts:6154-6270`). The same replay existed at the old
  pin, bounded at 4 (`app_session_runtime.zig:5351-5369@c1db9191`).

## RED (`red.txt`)

At the base, six or twenty questions are refused, no panel opens, and the turn
goes on. `red.txt` is `cells.py` (`.prd/tui-phase4/receipts/m1/cells.py`, row 1
left out) over every pair, the wire as each side's bodies carry it, and the
tests at the base's code:

- **Pairs**: 36 of 42 differ. `p5-input-question`'s `q6`, `q6-after`,
  `q6-after.full` (row 9: fx's panel rule and `▪ Cancelled` against xfx's
  `Six done.`), and every `p5-input-question20` snap at every size (fx's panel
  or its resolutions where xfx has `Twenty done.`). The six that match are `q2-*` and the three
  `p5-input-question21` snaps: a refused call draws nothing on either.
- **Wire**: fx's bodies advertise `Ask the user 1-20 multiple-choice questions`
  and `"maxItems":20` and answer 21 and 0 with `provide 1 to 20 questions`;
  the base's say `Ask the user 1 to 4 …`, `"maxItems":4` and `provide 1 to 4
  questions`.
- **Tests**: five of `tools::question`'s fail (the cap, the worst-case bound
  `36977` against `184881`, the refusals at 0 and 21, the full 20 × 6 batch),
  `tool_loop`'s advertisement test (`maxItems` 4 against 20), and both pty tests
  time out waiting for `Question 1 of 20` and `Question 1 of 6`.

## What xfx does now (the code head `eef9fd5f`)

- `MAX_QUESTIONS = 20` (`src/tools/question.rs`), and everything that spells the
  count follows it: the schema's `maxItems`; the description's opening words,
  now fx's own `Ask the user 1-20 multiple-choice questions`; the `questions`
  property's `1 to 20 questions to ask in order …`; the refusal, built from the
  constants as fx builds it from `max_questions`. A test ties every spelling to
  the constant.
- `MAX_ENCODED_RESULT_BYTES` is the worst-case document at twenty questions,
  20 × 9 243 + 21 = **184 881** bytes, still inside the registry's 256 KiB clip
  (`ToolLimits::default`, `src/tools/spec.rs`), so a full batch with every
  field at its bound can never be clipped into invalid JSON. fx bounds nothing
  and clips at 64 KiB; xfx's bounds are its own adaptation (3) on the
  `docs/parity.md` row.
- The panel needed no change: its counter, Tab, Left, the next-unanswered walk
  and the resolution rows were already fx's rules for any count. They are pinned
  at 20 now (`src/tui/question.rs` tests: the counter at 120, 80 and 72 columns;
  Tab, Left and the walk; `  10) `'s six-cell hang).
- `docs/parity.md`'s `ask_user_question` row says all of this, and names what
  xfx does not copy (below). Row 242's `terminal_safe` reference moves from
  `src/tools/question.rs:57-73` to `:60-76`, the three lines the cap's comment
  added above it.

## GREEN (`green.txt`)

`green.txt` is `cells.py` over every pair of `fx/` against `xfx/`, and the wire.

| scenario | size | snaps identical |
|---|---|---|
| `p5-input-question` | 40x120 | all 6: `q2-panel`, `q2-down`, `q2-cancelled`, `q6`, `q6-after`, `q6-after.full` |
| `p5-input-question20` | 40x120, 24x80, 14x72 | at each size the 9 batch snaps: `q20-panel`, `q20-left`, `q20-tab10`, `q20-back9`, `q20-tab20`, `q20-wrap`, `q20-first`, `q20-answered`, `q20-answered.full` -- 27 in all |
| `p5-input-question21` | 40x120 | all 3: `q21`, `q0`, `q0.full` |

36 of 42 pairs are identical. The six that differ are `p5-input-question20`'s
`resumed` and `resumed.full` at each size: the resume gap below, which the
ledger's `question-resume-replay` cause names.

On the wire:
- every xfx request advertises `Ask the user 1-20 multiple-choice questions`
  with `minItems 1`, `maxItems 20`;
- 21 and 0 are answered `(ask_user_question: provide 1 to 20 questions)`;
- the twenty answers' document is **byte-identical** to fx's (863 bytes, 20
  `{question, answer}` in question order; `003-completion.json` on both sides).

The tests that were RED pass at the head; the two pty tests passed three runs
in a row on their own.

## What xfx does not copy (`docs/parity.md`, the `ask_user_question` row)

- **(6) The description's body.** xfx's is the `580a0c5d` text with the
  permission-screen sentence dropped (adaptation (1)); only its opening count is
  v0.0.16's. fx v0.0.16's also names safety-review escalation, GitHub handles and
  gh/auth/tool blockers. Not in 28cf53b5; named, not changed.
- **(7) A refusal's wire type.** xfx answers a refused batch as a failed call,
  `{"type":"error-text"}`; fx's argument refusals are a successful call's
  `{"type":"text"}` (`red.txt` and `green.txt`, the wire sections). The words
  are the same. This predates the unit (at 0 and at 5 the base did the same),
  so it is named and not changed.
- **The resume.** fx replays a resumed session's answered batch as its
  resolutions; xfx's replay writes nothing for an `ask_user_question` call at
  **any** count: `src/tui/hydration.rs` hands the call to the shell as a tool
  start and a result, and `ToolGroup::start` returns early for `Activity::Ask`
  (`src/tui/toolgroup.rs:511-513`). `probe/` shows it at two questions on the
  base (`xfx-probe-resume2-40x120-resumed`: `┃ ask two`, then the turn's closing
  text, no resolutions; fx's has `  1) Pick 1?` … `  2) Pick 2?`). So it is not the
  cap's, and porting the replay -- a decoder, a replay step, the inert pass for
  strings read back from the session file, Ctrl-O's detail -- is its own unit:
  **QUESTION-RESUME**, queued. The ledger's `question-resume-replay` cause
  (`resumed` and `resumed.full` of `p5-input-question20`, the rows those snaps
  hold -- resolutions, the closing text, the summary, the composer, the resume
  notice, blanks -- on each side) takes the place, line for line, of
  `question-cap-four`, which this unit retires. `sweep-head-rescore.md` is the
  head sweep rescored with that ledger: the same 51 / 76, both `resumed` pairs
  under the cause with no row left unattributed.

## Mutants (`mutants.txt`)

`mutants.py` ran each mutant on a `git clone --no-local` throwaway of this
branch (no remote), one at a time, with the file restored after each, under the
suite lock. The tests per mutant: `cargo test --lib question`, then
`--test tool_loop`, then the two pty tests, each with a 900 s timeout; the first
run that fails kills it. **13 of 13 killed** (`mutants.txt`, with the tests that
caught each):

| mutant | killed by |
|---|---|
| M1-M3 the cap at 4, 21, 19 | the cap test, the refusals, the worst-case bound, the full batch |
| M4 the parser's bound `>=` | the refusals at 0/21, the full batch |
| M5 the refusal's words at 4 | the cap test, the refusals, the execute-side test |
| M6 the description at `1 to 4` | the cap test |
| M7 the schema's `maxItems` at 4 | the cap test |
| M8 the `questions` property at `1 to 4` | the cap test |
| M9 the worst-case bound at 36 977 | the worst-case test |
| M10 the counter's air 2 -> 1 | the counter at 120/80/72 |
| M11 Tab stops at the last page | the 20-page walk, the draft-keeping Tab test |
| M12 Left wraps past the first | the 20-page walk, two Left tests |
| M13 the resolution indent fixed at 5 | the two-digit hang |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-diff.txt`)

`scripts/fxcap/sweep.sh --only '*question*' --only '*approval*'` at 40x120, the
13 scenarios that ask or approve (76 snap pairs), with this branch's scenarios
and ledger, once with the base binary and once with the code head's, each under
the machine's suite lock. `sweep-base.md` and `sweep-head.md` are the
scoreboards (redacted by the sweep); `sweep-diff.txt` is every pair's verdict
at both, compared:

- **base 39 / 76, head 51 / 76.** The 12 that moved are the unit's targets, all
  from unresolved to identical or normalized-identical: `p5-input-question`'s
  `q6`, `q6-after`, `q6-after.full`, and `p5-input-question20`'s nine batch
  snaps.
- **None dropped**: no pair that passes at the base fails at the head.
- 25 stay unresolved at the head, the same 25 as at the base:
  `p5-input-question20`'s `resumed`, `resumed.full` (cause
  `question-resume-replay`); the approval scenarios' `approval-grant-scope`,
  `tool-schema`, `approval-scroll`, `amendment-card-after-stop`; and three no
  cause names, which this unit neither made nor touches:
  - `p5-transcript-question` `asked`: the panel wraps a long label beside its
    description at another column (fx `1) Postgres with the` / `write-ahead log
    kept on the`, xfx `1) Postgres with the write-ahead` / `log kept on the fast
    local`);
  - `p5-transcript-question` `narrow`: in Ctrl-O's view resized to 60 columns, fx's screen
    still holds the banner and a blank row above the turn, xfx's starts at the
    turn (the rows below match, two rows up);
  - `p5-input-approval` `after`: fx runs `shell`, which xfx does not advertise.

## Gate (`gate.txt`)

`gate.txt`: every step of `.github/workflows/ci.yml`'s native row, in its
order, plus `sweep.yml`'s two harness tests, at `803aaa5f` (the code of
`eef9fd5f`), the whole run under one suite-lock slot, `TMPDIR` short. All 20
exit 0:

| step | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo test --locked --all-targets --no-fail-fast` | 5452 passed, 0 failed |
| `cargo clippy … --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --features fault-injection --test tui` | 197 passed, 0 failed |
| `cargo test --locked --lib --features fault-injection` | 4557 passed, 0 failed |
| `check-no-stubs`, `check-no-secrets`, `test-gate-env` (62 cases), `check-xfx-identity` | ok |
| `check-preview-contract`, `test-preview-contract`, `check-tui-preflight-cost`, `check-usage-clock` | ok |
| `cargo build --release`; `smoke.sh` | 47 checks, 0 failures |
| `cargo build --release --features fault-injection`; `smoke-tui.sh` | 37 scenarios + the oracle, 1168 checks, 0 failures (`23-question-panel`, `23b-question-cancelled` ok) |
| `scripts/fxcap/test-sweep.sh`, `test-capture.sh` | OK |

The registry and help counts do not move: no tool, command or key is added or
removed (12 tools, `docs/parity.md`'s Tools section); `check-no-stubs` passes
unchanged.

## Files

- `fx/`, `red/`, `xfx/`: the captures, `<bin>-<scenario>-<size>-<snap>.{txt,ansi}`,
  with each 40x120 run's `-gateway.log` and `-bodies/` (the 24x80 and 14x72 runs'
  too).
- `probe/`: the two-question resume probe, both binaries.
- `red.txt`, `green.txt`: the pairs, the wire, the tests.
- `fx-src.txt`: every fx line cited above.
- `mutants.py`, `mutants.txt`; `sweep-base.md`, `sweep-head.md`, `sweep-diff.txt`;
  `refs.txt` (every `path:line` the ledger and `docs/parity.md` cite, checked at
  the head against the base: none moved); `gate.txt`; `src-after-code-head.diff`;
  `sweep-head-rescore.md`.
