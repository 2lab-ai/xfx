# F3 ask_user_question — fx `c1db919` vs xfx, 40x120 and 24x80

Two scenarios, each driven identically on both binaries by `scripts/fxcap/capture.sh`
(isolated tmux socket, empty `HOME`, fake key, loopback fake gateway, desktop shims). The tool is
`ask_user_question` in both products, so one turns file serves both. xfx is the release build of
`feat/tui-phase4-question` after `plan/tui-phase4` (`2c09164`, with F1's tool groups and C2) was
merged into it:

```
export TMPDIR=<scratch>
FX_BIN=<fx c1db919 build> scripts/fxcap/capture.sh fx  question 40x120 <out>
XFX_BIN=$PWD/target/release/xfx scripts/fxcap/capture.sh xfx question 40x120 <out>
# the same for `question-other`, and both again at 24x80
```

(`XFX_BIN` must be absolute: the harness starts the binary from inside the scratch workspace.)

- `question.cap` asks a two-question batch (`Which database should I use?` with two described
  choices and a bare one; `Deploy now?` with a bare and a described one), walks Down onto `Other`
  and past it, back Up three, Tabs to the second question, answers it with Enter (the batch returns
  to the unanswered first) and answers that with `2`, which submits the batch.
- `question-other.cap` asks one question, opens `Other`, types `MariaDB please`, answers with
  Enter; then asks a second question and presses Escape.

24x80 is captured because width decides two things here: the hint row's `Question i of n` (shown
only with two cells of air beside the keys) and the freeform hint (a narrower rung of its ladder).

Scratch paths are replaced with `<scratch>` (none occur in the captures). The request bodies the
gateway received are not committed; what they carried is quoted below.

## How "matches" was measured

Cell for cell, not byte for byte: tmux re-serializes each row's SGR as a change from the pen the
row before left (`capture-pane -e`), so the same cells can be spelled differently when the rows
around them differ -- e.g. xfx's blank row under the question holds no written cells and fx's does,
so on the 120-column `q1` fx's marked first choice reads `ESC[1mESC[38;5;255m    1) Postgres` and
xfx's `ESC[38;5;255m    1) Postgres`, the bold still open from the bold question row above it.
Both are bold 255. Each `.ansi` was parsed into (character, bold, reverse, foreground) per cell with
the pen carried across rows, and **every row of every snap** was compared at the same row number.

## What matches

Every row of every snap, cell for cell, except the rows listed under "What differs" -- which are
the banner (row 1, every snap), the open freeform field and its hint, the turn summary's token
count, and what follows an Escape:

| scenario | snap | rows (both) | what the matching rows are |
|---|---|---|---|
| question | `q1` | 3-14 | the card, the rule, `  Which database should I use?` (bold), blank, `    1) Postgres` + `Relational, strong consistency` from the 38th column, `    2) SQLite` + `Single file, embedded`, `    3) DynamoDB`, `    4) Other`, blank, rule, `1–4 choose now    ↑↓ options    tab questions    enter answer    esc cancel` + `Question 1 of 2` ending at the last cell (at 24x80 no counter: the keys take 75 cells and the counter needs 17 more) |
| question | `q1-down` | 3-14 | the bold moved to `2) SQLite`, nothing else changed |
| question | `q1-down3` | 3-10, 12-13 | every row of the band but the open field (row 11) and the hint (row 14) |
| question | `q1-down4` | 3-14 | Down past `Other` wrapped to `1) Postgres` |
| question | `q1-back` | 3-14 | Up three from the first wrapped to `2) SQLite` |
| question | `q1-tab` | 3-13 | `  Deploy now?`, `    1) Yes`, `    2) No` + `Wait for review` in the same column, `    3) Other`, hint `1–3 …` + `Question 2 of 2` |
| question | `q2` | 3-14 | Enter answered the second question and the batch went back to the unanswered first, its marked choice kept |
| question | `answered` | all but 1 and 12 | `  1) Which database should I use?`, `     SQLite` (245, the indent inside the colour), `  2) Deploy now?`, `     Yes`, blank, `  Got it.`, and the composer and statusline back |
| question-other | `q` | 3-13 | a lone question, hint `1–3 choose now …` with **no** counter |
| question-other | `q-other`, `q-typed` | 3-9, 11-12 | every row of the band but the open field (row 10) and the hint (row 13) |
| question-other | `answered`, `q2` | all but 1 and 10 | `  1) Which database should I use?` / `     MariaDB please`, and the second batch's band under it |
| question-other | `q2-esc` | 2-9, 11-15 | `■ Cancelled` on row 14, the glyph 252 and the word in none |

The same at 24x80 for both scenarios, row for row.

SGR as captured on both (dark, 256 colours): question `  ESC[1m…ESC[0m`; marked choice
`ESC[1mESC[38;5;255m    1) PostgresESC[0m` (upstream's `selected_completion_style`, no marker
glyph); unmarked `ESC[38;5;245m    2) SQLiteESC[39m`; description `ESC[38;5;245m…ESC[39m` from the
38th column (30th below 72 columns, own row below 52 -- unit tests); hint `ESC[38;5;245m…` from
column 0; summary answer `ESC[38;5;245m     SQLiteESC[39m`; `ESC[38;5;252m■ESC[39m Cancelled`.

Behaviour, from the gateway logs and request bodies:

- **The answer document is the same bytes.** `question`: both send
  `[{"question":"Which database should I use?","answer":"SQLite"},{"question":"Deploy now?","answer":"Yes"}]`
  as the tool result. `question-other`: both send
  `[{"question":"Which database should I use?","answer":"MariaDB please"}]`.
- **A digit that completes the batch submits it** (`q2` → `answered` on `2`), on both.
- **Enter returns to the unanswered question** (`q1-tab` → Enter → `q2` shows question 1), on both.
- **The question tool is not a tool row** on either (`statusNamesAsk`; F1's group skips it).
- **The activity row is hidden while the question owns the footer** on both (`render_input.zig:705`;
  F1's `modal()`), and the composer gives the question its rows on both.

## What differs, and why

| # | snaps | fx | xfx | why |
|---|---|---|---|---|
| 1 | `q1-down3` row 11, `q-other`/`q-typed` row 10 | the open field ends in a reverse-video cell, `    4) ESC[0;7m ESC[0m` (`writeSelectedFreeformOptionLines`, `src/ui/footer/question_ui.zig:323-399`) | `    4) ` with the terminal's own caret just past it (and past the draft) | reverse video is outside the painter's vocabulary (`src/tui/pacer.rs` `is_palette_sgr`), and xfx puts the real caret in a draft as it does at an approval's amendment (ssot Tensions, "Approval review scrolling") |
| 2 | `q1-down3` row 14, `q-other`/`q-typed` row 13 | `type answer    ↑↓←→ cursor    shift+↑↓ options    tab questions    enter answer    esc cancel`; at 80 columns `↑↓ cursor · shift+↑↓ options · …` (`input_presentation.zig:179-184`, `:212-214`) | `type answer    ←→ cursor    ↑↓ options    tab questions    enter answer    esc cancel`; at 80 `←→ cursor · ↑↓ options · …` | fx's plain arrows walk a draft that wraps onto rows of its own and its shifted arrows choose (`input_question_runtime.zig:150-185`, `src/ui/input/runtime.zig:175-188`); xfx's decoder reads a shifted arrow as the arrow (`src/tui/input.rs` `cursor`) and its draft is one row, so in xfx `↑↓` choose and only `←→` move the caret -- the row says what the keys do |
| 3 | `q2-esc` rows 16-22 | Escape cancels the **turn**: `■ Cancelled`, then the composer; no further request (gateway log ends at the second question; fx e2e `tests/e2e/tui-decision-prompts.test.ts:2454` asserts the sentinel is never sent; `cancelQuestionPrompt`, `src/core/app/input_question_runtime.zig:61-83`, via `app_input_runtime.zig:3137`) | Escape declines the **question**: `■ Cancelled`, then the tool's sentinel `(user cancelled the question)` goes to the model and the turn goes on (`  after cancel.`, its summary; a fourth request in the gateway log) | kept on purpose: the brief keeps the question tool's semantics -- what the model receives -- unchanged, and fx's Escape changes it (the model never hears the sentinel). Ctrl-C at a question still stops the turn, as in fx (`src/ui/input/runtime.zig:133-151`, `3 => .cancel`). Switching Escape to fx's meaning is one branch in `Shell::answer` and is a decision for the plan, not this unit |
| 4 | row 1, every snap | `𝒇x v0.0.13` | `x𝒇x v0.1.0` | the welcome row is another unit's |
| 5 | `answered`, `q2`, `q2-esc` | turn summary `(↑4 ↓80)` | `(↑4 ↓40)` | the token count is the activity unit's |

Not captured, asserted in unit tests instead (`src/tui/question.rs`, `src/tui/shell.rs`):

- **Escape at a draft with text in it arms a clear** (`esc again to clear` on the hint row's right
  side), a second inside 500 ms empties the draft and leaves the slot marked, and only an Escape at
  an empty draft cancels (`app_input_runtime.zig:3120-3135`).
- **Left at a choice is the previous question** and stops at the first; Right does nothing
  (`input_question_runtime.zig:186-195`).
- **Long questions, labels, descriptions and answers wrap** with continuation rows aligned under
  their first column, never an ellipsis (e2e `tui-decision-prompts.test.ts:2625`).
- **Ctrl-C at a question** writes the same `■ Cancelled` and stops the turn.
