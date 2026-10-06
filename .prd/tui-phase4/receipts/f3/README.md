# F3 ask_user_question — fx `c1db919` vs xfx, 40x120 and 24x80

Two scenarios, each driven identically on both binaries by `scripts/fxcap/capture.sh`
(isolated tmux socket, empty `HOME`, fake key, loopback fake gateway, desktop shims). The tool is
`ask_user_question` in both products, so one turns file serves both:

```
export TMPDIR=/private/tmp/claude-501
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  question       40x120 <out>
XFX_BIN=$PWD/target/release/xfx scripts/fxcap/capture.sh xfx question       40x120 <out>
# the same for `question-other`, and both again at 24x80
```

(`XFX_BIN` must be absolute: the harness starts the binary from inside the scratch workspace.)

- `question.cap` asks a two-question batch (`Which database should I use?` with two described
  choices and a bare one; `Deploy now?` with a bare and a described one), walks Down onto `Other`
  and past it, back Up three, Tabs to the second question, answers it with Enter (the batch returns
  to the unanswered first) and answers that with `2`, which submits the batch.
- `question-other.cap` asks one question, opens `Other`, types `MariaDB please`, answers with
  Enter; then asks a second question and presses Escape.

24x80 is captured because width decides two things here: the hint row's `Question i of n` (only
with two cells of air beside the keys) and the freeform hint (a narrower variant of its ladder).

Scratch paths are replaced with `<scratch>` (none occur in these captures). The request bodies the
gateway received are not committed; what they carried is quoted below.

## How "matches" was measured

Cell for cell, not byte for byte: tmux re-serializes each row's SGR as a change from the pen the
row before left (`capture-pane -e`), so the same cells can be spelled differently when the rows
around them differ. fx writes something into the blank row under the question and xfx's band leaves
that row's cells unwritten, so on the 120-column capture fx's marked first choice reads
`ESC[1mESC[38;5;255m    1) Postgres` and xfx's reads `ESC[38;5;255m    1) Postgres` -- the bold is
still open from the question row above it in xfx's serialization. Both are bold 255. Each `.ansi`
was parsed into (character, bold, reverse, foreground) per cell with the pen carried across rows,
and the question's band -- top rule through hint row -- and the summary rows were compared.

## What matches

| scenario | snap | fx rows | xfx rows | result |
|---|---|---|---|---|
| question 40x120 | `q1` | 5-14 | 8-17 | cell-identical: rule, `  Which database should I use?` (bold), blank, `    1) Postgres` + `Relational, strong consistency` from the 38th column, `    2) SQLite` + `Single file, embedded`, `    3) DynamoDB`, `    4) Other`, blank, rule, `1–4 choose now    ↑↓ options    tab questions    enter answer    esc cancel` + `Question 1 of 2` ending at the last cell |
| question 40x120 | `q1-down` | 5-14 | 8-17 | cell-identical: the bold moved to `2) SQLite`, nothing else changed |
| question 40x120 | `q1-down4` | 5-14 | 8-17 | cell-identical: Down past `Other` wrapped to `1) Postgres` |
| question 40x120 | `q1-back` | 5-14 | 8-17 | cell-identical: Up three from the first wrapped to `2) SQLite` |
| question 40x120 | `q1-tab` | 5-13 | 8-16 | cell-identical: `  Deploy now?`, `    1) Yes`, `    2) No` + `Wait for review` in the same column, `    3) Other`, hint `1–3 …` + `Question 2 of 2` |
| question 40x120 | `q2` | 5-14 | 8-17 | cell-identical: Enter answered the second question and the batch went back to the unanswered first, marked choice kept |
| question 40x120 | `answered` | 5-8 | 7-10 | cell-identical: `  1) Which database should I use?`, `     SQLite` (245), `  2) Deploy now?`, `     Yes` (245) |
| question 24x80 | `q1` … `q2` | 5-14 / 5-13 | 8-17 / 8-16 | the same, cell-identical; at 80 columns neither binary shows `Question i of n` (the keys take 75 cells and the counter needs 17 more) |
| question 24x80 | `answered` | 5-8 | 7-10 | cell-identical |
| question-other 40x120 | `q` | 5-13 | 8-16 | cell-identical: a lone question, hint `1–3 choose now …` with **no** counter |
| question-other 40x120 | `answered` | 5-6 | 7-8 | cell-identical: `  1) Which database should I use?`, `     MariaDB please` (245) |
| question-other 40x120 | `q2` | 14-22 | 20-28 | cell-identical: the second batch's band under the first batch's summary |
| question-other 40x120 | `q2-esc` | 14 | 19 | cell-identical: `■ Cancelled`, the glyph 252 and the word in none |
| question-other 24x80 | `q`, `q2`, `answered`, `q2-esc` | | | cell-identical, as at 40x120 |
| both, both sizes | `q1-down3`, `q-other`, `q-typed` | | | every row of the band but the open field and the hint row (below) |

SGR as captured on both (dark, 256 colours): question `  ESC[1m…ESC[0m`; marked choice
`ESC[1mESC[38;5;255m    1) PostgresESC[0m` (upstream's `selected_completion_style`, no marker
glyph); unmarked `ESC[38;5;245m    2) SQLiteESC[39m`; description `ESC[38;5;245m…ESC[39m` from the
38th column (30th below 72 columns, own row below 52); hint `ESC[38;5;245m…` from column 0; summary
answer `ESC[38;5;245m     SQLiteESC[39m` with the indent inside the colour; `ESC[38;5;252m■ESC[39m Cancelled`.

Behaviour, from the gateway logs and request bodies:

- **The answer document is the same bytes.** `question`: both send
  `[{"question":"Which database should I use?","answer":"SQLite"},{"question":"Deploy now?","answer":"Yes"}]`
  as the tool result. `question-other`: both send
  `[{"question":"Which database should I use?","answer":"MariaDB please"}]`.
- **A digit that completes the batch submits it** (`q2` → `answered` on `2`), on both.
- **Enter returns to the unanswered question** (`q1-tab` → Enter → `q2` shows question 1), on both.

## What differs, and why

| # | snap | fx | xfx | why |
|---|---|---|---|---|
| 1 | `q1-down3`, `q-other`, `q-typed` | the open field ends in a reverse-video cell, `    4) ESC[0;7m ESC[0m` (`writeSelectedFreeformOptionLines`, `src/ui/footer/question_ui.zig:323-399`) | `    4) ` with the terminal's own caret just past it (and past the draft) | reverse video is outside the painter's vocabulary (`src/tui/pacer.rs` `is_palette_sgr`), and xfx puts the real caret in a draft as it does at an approval's amendment (ssot Tensions, "Approval review scrolling") |
| 2 | `q1-down3`, `q-other`, `q-typed` | `type answer    ↑↓←→ cursor    shift+↑↓ options    tab questions    enter answer    esc cancel`; at 80 columns `↑↓ cursor · shift+↑↓ options · …` (`input_presentation.zig:179-184`, `:212-214`) | `type answer    ←→ cursor    ↑↓ options    tab questions    enter answer    esc cancel`; at 80 `←→ cursor · ↑↓ options · …` | fx's plain arrows walk a draft that wraps onto rows of its own and its shifted arrows choose (`input_question_runtime.zig:150-185`, `src/ui/input/runtime.zig:175-188`); xfx's decoder reads a shifted arrow as the arrow (`src/tui/input.rs` `cursor`) and its draft is one row, so in xfx `↑↓` choose and only `←→` move the caret -- the row says what the keys do |
| 3 | `q2-esc` | Escape cancels the **turn**: `■ Cancelled`, no further request (gateway log ends at the second question; fx e2e `tests/e2e/tui-decision-prompts.test.ts:2454` asserts the sentinel is never sent; `cancelQuestionPrompt`, `src/core/app/input_question_runtime.zig:61-83`, via `app_input_runtime.zig:3137`) | Escape declines the **question**: `■ Cancelled`, then the tool's sentinel `(user cancelled the question)` goes to the model and the turn goes on (`[tool] ask_user_question ok`, `after cancel.`; fourth request in the gateway log) | kept on purpose: the brief keeps the question tool's semantics -- what the model receives -- unchanged, and fx's Escape changes it (the model never hears the sentinel). Ctrl-C at a question still stops the turn, as in fx (`src/ui/input/runtime.zig:133-151` `3 => .cancel`). Switching Escape to fx's meaning is one line in `Shell::answer` and is a decision for the plan, not this unit |
| 4 | every panel snap | the activity row is hidden while the question owns the footer (`render_input.zig:705`) | `• Running (0s) (↑4 ↓0)` and a blank row above the question's rule | the activity row under a modal is unit F1's (`.prd/tui-phase4/loop.md`, F1 row) |
| 5 | every snap after the call | the question tool never appears in the transcript (`statusNamesAsk`, `tool_group_projection.zig:228`) | `[tool] ask_user_question running` / `… ok` around the summary | the tool rows are unit F1's |
| 6 | `question-other 24x80 q2-esc` | one blank row between the card's row and `■ Cancelled` | two blank rows between `[tool] ask_user_question running` and `■ Cancelled` | not this unit's writer: the same extra row appears before the interrupt notice in `approvals` at 24x80 on this build, where a pinned band with the activity row directly under the document shrinks; the summary block itself writes one blank row above and one below (`Shell::write_block`) |
| 7 | every snap | banner `𝒇x v0.0.13` | `x𝒇x v0.1.0` | the welcome row is another unit's |
| 8 | `answered` | turn summary `6s (↑4 ↓80)` | `5s (↑4 ↓40)` | the token count is the activity unit's |

Not captured, asserted in unit tests instead (`src/tui/question.rs`, `src/tui/shell.rs`):

- **Escape at a draft with text in it arms a clear** (`esc again to clear` on the hint row's right
  side), a second inside 500 ms empties the draft and leaves the slot marked, and only an Escape at
  an empty draft cancels (`app_input_runtime.zig:3120-3135`).
- **Left at a choice is the previous question** and stops at the first; Right does nothing
  (`input_question_runtime.zig:186-195`).
- **Long questions, labels, descriptions and answers wrap** with continuation rows aligned under
  their first column, never an ellipsis (e2e `tui-decision-prompts.test.ts:2625`).
