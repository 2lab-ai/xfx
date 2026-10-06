# F3 ask_user_question — fx `c1db919` vs xfx

Three scenarios, each driven identically on both binaries by `scripts/fxcap/capture.sh`
(isolated tmux socket, empty `HOME`, fake key, loopback fake gateway, desktop shims). The tool is
`ask_user_question` in both products, so one turns file serves both. xfx is the release build of
`feat/tui-phase4-question` at the commit these receipts were taken with (plan `2870b42` merged in):

```
export TMPDIR=<scratch>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  question 40x120 <out>
XFX_BIN=$PWD/target/release/xfx scripts/fxcap/capture.sh xfx question 40x120 <out>
# question and question-other at 40x120 and 24x80; question-field at 20x64
```

(`XFX_BIN` must be absolute: the harness starts the binary from inside the scratch workspace.)

- `question.cap` asks a two-question batch (`Which database should I use?` with two described
  choices and a bare one; `Deploy now?` with a bare and a described one), walks Down onto `Other`
  and past it, back Up three, Tabs to the second question, answers it with Enter (the batch returns
  to the unanswered first) and answers that with `2`, which submits the batch.
- `question-other.cap` asks one question, opens `Other`, types `MariaDB please`, answers with
  Enter; then asks a second question and presses **Escape**.
- `question-field.cap` is fx's e2e `tui-decision-prompts.test.ts:2785` (paste) and `:2825` (plain
  arrows walk the answer, shifted arrows choose) at their size (64x20): opens
  `Other`, bracketed-pastes `line-one\nxy\nline-three`, Up onto `xy`, types `!`, **Shift+Up**
  (chooses `2)`), **Shift+Down** (back to the field), Enter; then asks again and presses
  **Ctrl-C**.

24x80 is captured because width decides the hint row's `Question i of n` (shown only with two cells
of air beside the keys) and which rung of the freeform hint's ladder fits.

Scratch paths are replaced with `<scratch>` (none occur). Request bodies are not committed; what
they carried is quoted below.

## How "matches" was measured

Cell for cell: tmux re-serializes each row's SGR as a change from the pen the row before left
(`capture-pane -e`), so the same cells can be spelled differently when the rows around them differ.
Each `.ansi` was parsed into (character, bold, reverse, foreground) per cell, the pen carried across
rows, and **every row of every snap** compared at the same row number.

## What matches

Every row of every snap, cell for cell -- reverse video included -- except the rows listed under
"What differs". By snap:

| scenario | snaps | rows that differ |
|---|---|---|
| question 40x120, 24x80 | `q1`, `q1-down`, `q1-down3`, `q1-down4`, `q1-back`, `q1-tab`, `q2` | row 1 (banner) only |
| question 40x120, 24x80 | `answered` | row 1, and the turn summary's seconds and token count |
| question-other 40x120, 24x80 | `q`, `q-other`, `q-typed` | row 1 only |
| question-other 40x120, 24x80 | `answered`, `q2` | row 1, the turn summary |
| question-other 40x120, 24x80 | `q2-esc` | row 1, the turn summary, and the interrupt's notices under `■ Cancelled` (#3) |
| question-field 20x64 | `q`, `open`, `pasted`, `edited`, `shift-up`, `shift-down` | row 1 only |
| question-field 20x64 | `answered` | row 1, the turn summary |
| question-field 20x64 | `q2` | the turn summary (row 1 has scrolled off) |
| question-field 20x64 | `q2-cc` | the interrupt's notices under `■ Cancelled` (#3), which push the screen up |

What those identical rows are:

- The panel: rule, `  Which database should I use?` (bold), blank, `    1) Postgres` with
  `Relational, strong consistency` from the 38th column (30th below 72 columns, its own rows below
  52), `    2) SQLite`, `    3) DynamoDB`, `    4) Other`, blank, rule; the marked choice
  `ESC[1mESC[38;5;255m`, every other `ESC[38;5;245m`, no marker glyph.
- The hint: `1–4 choose now    ↑↓ options    tab questions    enter answer    esc cancel` and
  `Question 1 of 2` ending at the last cell (40x120; no counter at 80 columns or for a lone
  question); with the field open `type answer    ↑↓←→ cursor    shift+↑↓ options    tab questions
  enter answer    esc cancel`, at 80 columns `↑↓ cursor · shift+↑↓ options · …`, at 64
  `shift+↑↓ options · tab questions · enter answer · esc cancel`.
- The field: `    4) ` and a reversed blank (`ESC[0;7m ESC[0m`), `    3) MariaDB please` and the
  reversed blank after it; the pasted answer on three rows hanging under its first column
  (`    3) line-one` / `       xy!` + reversed blank / `       line-three`) -- `question-field`'s
  `edited` is byte-identical, not only cell-identical; after Shift+Up the field is the unmarked
  slot's three dim rows and `2) Run tests` is marked; Shift+Down opens it again.
- The record: `  1) Which database should I use?` / `ESC[38;5;245m     SQLiteESC[39m` /
  `  2) Deploy now?` / `     Yes`; `  1) What should happen first?` / `     line-one` / `     xy!` /
  `     line-three` (a line break in an answer is a row break, hung under the question);
  `ESC[38;5;252m■ESC[39m Cancelled` after Escape and after Ctrl-C.
- The activity row is hidden while the question owns the footer, the composer gives the question
  its rows, and the question tool is no tool row -- on both.

Behaviour, from the gateway logs and request bodies:

- **The answer document is the same bytes**: `[{"question":"Which database should I
  use?","answer":"SQLite"},{"question":"Deploy now?","answer":"Yes"}]`;
  `[{"question":"Which database should I use?","answer":"MariaDB please"}]`; and the pasted answer
  with its line breaks, `line-one\nxy!\nline-three`, on both.
- **Escape and Ctrl-C cancel the turn on both**: three requests in each `question-other` and
  `question-field` log (fx's also has the title request), none after the second question. The
  model is never sent the sentinel `(user cancelled the question)`: a probe that sent a prompt after
  the Escape shows both carry the question's call as `aborted by user` (#4).
- **A digit that completes the batch submits it**, and **Enter returns to the unanswered
  question**, on both.

## What differs, and why

| # | where | fx | xfx | why |
|---|---|---|---|---|
| 1 | row 1, every snap | `𝒇x v0.0.13` | `x𝒇x v0.1.0` | the welcome row is another unit's |
| 2 | the turn summary under an answer | `5s (↑4 ↓80)` | `5s (↑4 ↓40)` | the token count is the activity unit's; the seconds are wall time and differ by a second between runs |
| 3 | under `■ Cancelled` after Escape (`q2-esc`) or Ctrl-C (`q2-cc`) | nothing (`cancelQuestionPrompt`, `src/core/app/input_question_runtime.zig:59-83`, writes only the record) | `⊘ turn: interrupted -- stopping the turn; another Ctrl-C before it stops exits.` and `✗ system: the turn was cancelled` | the interrupt's own wording is unit F4's (coordinator: left to F4) |
| 4 | the next request after a cancelled question | the call's result `{"type":"error-text","value":"aborted by user"}` (`session.zig:64`), then a user message `<turn_aborted>…</turn_aborted>` (`session.zig:53-56`) | `{"type":"text","value":"aborted by user"}`, no `<turn_aborted>` message | xfx's tool result has no error flag on the wire (`src/gateway/protocol.rs` `Message::tool_result`); the turn-aborted message is the interrupt's history, unit F4's |
| 5 | (not visible in a capture) the terminal's cursor while a draft's reversed caret cell is up | hidden (`paint_plan.zig:989`, `input_visible` false) | shown, standing on the reversed cell | xfx's frame always ends `?25h` (`frame::END_FRAME`), and `check` refuses a frame that leaves the cursor hidden |

Not captured, asserted in unit tests (`src/tui/question.rs`, `src/tui/freeform.rs`,
`src/tui/shell.rs`, `src/tui/event_loop.rs`):

- Escape at a draft with text arms a clear (`esc again to clear` on the hint row's right), a second
  inside 500 ms empties it and leaves the slot marked; only an Escape at an empty field cancels
  (`app_input_runtime.zig:3120-3135`).
- Left at a choice is the previous question; Right does nothing (`input_question_runtime.zig:186-195`).
- The field's keys: Home/End are the whole answer's ends; word left/right, `C-w`, `C-u`, `C-k`,
  Delete as fx's field binds them (`question_prompt.zig:437-571`). xfx decodes `C-w` and
  Alt-Backspace as one key; both act as fx's `C-w` (fx's Alt-Backspace deletes by word characters,
  `question_prompt.zig:523-529`).
- A paste at a marked choice is dropped whole; a paste past the answer's budget is refused whole.
- An answer taller than a third of the screen shows the third ending at the caret's row (fx clips
  the field at the band instead, which can clip the caret's row away).
- A theme report repaints an answer row's grey as the answer's (241 on light), not a turn
  summary's (247): the row's kind is recorded when it is written
  (`a_theme_report_recolours_an_answered_questions_record_as_the_answer`).
