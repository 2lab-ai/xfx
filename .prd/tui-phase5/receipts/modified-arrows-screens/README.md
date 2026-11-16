# MODIFIED-ARROWS-SCREENS receipt: a held arrow under the full transcript and at a question, as fx reads it

fx v0.0.16 decodes every arrow held with Super, Alt, Ctrl or Shift to a
composer move, and keeps Ctrl-B and Ctrl-F raw bytes. Its full transcript
binds the plain arrows only, so each of those goes on to the composer behind
the screen; its question moves the choice only on the selection-extending
`visual_up`/`visual_down`, and reads the rest as its field's moves or as
nothing. xfx's full transcript and question read the event with the
decoder's composer spellings folded back to the arrows (`unspelled`), so
Ctrl-, Alt- and Super-Up and -Down scrolled the screen and moved the choice,
and Shift- and Super-Left and -Right and Ctrl-B and -F were held by the
screen, and the Left ones went back a question. MODIFIED-PAGE-KEYS' opus
review found it (round 1, SHOULD 1), and its round 2 recorded it in
`docs/parity.md` with this unit as the owner.

| commit | what |
|---|---|
| `08d6d605` | RED: the full-transcript test, the question test, the decoder's two-modifier pins (Shift-Alt-Up/Down flipped, three added), the fold table's column for `unspelled` dropped; the scenarios `p5-modified-arrows-full` and `p5-modified-arrows-question`. Tests and scenarios only |
| `8b5aacaa` | **the code head**: the full transcript and the question read the event as it came; `unspelled` merged into `folded`; `question::Act::of`'s arms and `Act::StepLeft`; the decoder's Alt before Shift on Up and Down; the docs of the decoder's variants |
| `33cfeac4` | comments only: four fx citations named by file (`git diff 8b5aacaa 33cfeac4` changes `//` and `///` lines alone) |
| `bc188108`, `5e985efb`, `8a9be523` | round 1's records: `docs/parity.md`, the ledger, this folder (round 1's gate ran at `5e985efb`); no `src/` change |
| `0f8c6e6a` | round 2: the Super-Right field pin (test), the question scenario split, three stale comments; no non-test code change |
| the commits after | round 2's records (Round 2 below); no `src/` change |

The source change outside the test modules is +133/-111 lines over the three
files (`git diff -U0 c77a77ce 33cfeac4 -- src`, the hunks above each file's
`mod tests`), under the brief's 300.

## fx's rule, surface by surface (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The decoder** (`modifiedArrowAction`, `escape_parser.zig:73-108`), for the
legacy `CSI 1;<m> A-D/H/F` (`escape_parser.zig:722`, `:727-730`), the kitty
arrows (`:208-217`) and the keypad's (`:196-197`, `:163-167`): Super first
(`:74-82`: Up/Down `draft_start`/`draft_end`, Left/Right `line_start`/
`line_end`), then Alt (`:83-89`: Up/Down `paragraph_up`/`_down`, Left/Right
the word moves), then Ctrl (`:90-98`: Up/Down `visual_up`/`_down`,
Left/Right the word moves), then Shift (`:99-106`: `visual_up`/`_down`,
`character_left`/`_right`); each a `composerMove` whose `extend_selection` is
Shift (`:66-71`). Ctrl-B and Ctrl-F are raw bytes 2 and 6 (`appendRaw`,
`terminal_action_decoder.zig:207-212`), the composer's `character_left` and
`character_right` (`shortcuts.zig:15-16`); kitty's Ctrl-b and Ctrl-f are the
same bytes (`escape_parser.zig:255-258`).

| spelling | the full transcript | a question's choice | a question's field |
|---|---|---|---|
| plain Up, Down | three rows (`keyForAction`, `input_full_transcript_runtime.zig:161-162`) | the choice (`input_question_runtime.zig:182-189`) | the field's row, or the choice past its edge (`input_question_runtime.zig:149-166`) |
| plain Left, Right | held (`input_full_transcript_runtime.zig:159-160`, `.navigate`) | back a question / nothing (`input_question_runtime.zig:190-198`) | the step (`src/ui/input/runtime.zig:160-161`) |
| Shift-Up/Down, Ctrl-Shift-Up/Down (`visual_up` with the selection) | not its key (`input_full_transcript_runtime.zig:156-178`): to the composer behind it (`app_input_runtime.zig:930`, `:997-1001`) | the choice (`questionActionFromShortcut`, `src/ui/input/runtime.zig:184-189`) | the choice (the same check, `src/ui/input/runtime.zig:184-189`, ahead of the field's at `:194-196`) |
| Ctrl-Up/Down (`visual_up`), Alt- and Shift-Alt-Up/Down (`paragraph_up`) | to the composer behind it | nothing (the switch's `else`, `input_question_runtime.zig:199`) | nothing (no field move, `src/ui/input/runtime.zig:158-167`) |
| Super-Up/Down, with Shift or not (`draft_start`/`draft_end`) | to the composer behind it | nothing | the field's start and end (`src/ui/input/runtime.zig:162-163`) |
| Shift-Left/Right (`character_left`/`_right`) | to the composer behind it | nothing | the step (`src/ui/input/runtime.zig:160-161`) |
| Super-Left/Right (`line_start`/`line_end`) | to the composer behind it | nothing | the field's start and end (`src/ui/input/runtime.zig:162-163`) |
| Ctrl- and Alt-Left/Right (the word moves) | to the composer behind it | nothing | the word move (`src/ui/input/runtime.zig:164-165`) |
| Ctrl-B, Ctrl-F (bytes 2, 6) | not its byte (`keyForByte`, `input_full_transcript_runtime.zig:147-154`): to the composer (`app_input_runtime.zig:872`, `:887-893`) | no question action (`questionActionFromByte`, `src/ui/input/runtime.zig:135-153`), swallowed (`app_input_runtime.zig:1136-1159`) | the step (`fromFocusedEditorControlByte`, `shortcuts.zig:31-41`; `src/ui/input/runtime.zig:199-209`) |
| Home/End held (`H`/`F` with a modifier: Super and Ctrl `draft_start`/`draft_end`, Shift `line_start`/`line_end`, Alt the plain key, `escape_parser.zig:79-80`, `:95-96`, `:104-105`, `:83-89`) | to the composer behind it, as the plain Home and End (`.home`/`.end` are no key of the screen's, `input_full_transcript_runtime.zig:156-178`) | nothing | the field's start and end (`src/ui/input/runtime.zig:162-163`) |

xfx read the held Home and End alike before this unit, and still does: its decoder reads every one as Home or End (`cursor`), which the screen leaves to the composer and the question reads as `Act::Home`/`Act::End` (the field's edges, nothing at a choice); the composer's Ctrl-Home is the line's start where fx's is the draft's (COMPOSER-SUPER-ARROWS, the full-screen TUI row).

**What the composer behind the screen does with them** is the composer's
own move (`routeComposerShortcutAction`, `app_input_runtime.zig:244-330`):
`visual_up` the row above (`routeComposerVertical`,
`input_completion_runtime.zig:211-292`), `line_start` the line's start,
`paragraph_up` the previous paragraph's start (`horizontal_navigation.zig:39-46`),
`character_left` the step. Where xfx's composer already differs from those
(Super's jump, the paragraph move, the selection), it differs behind the
screen too; see Decisions.

**The oracle** (`fx/`):
- `p5-modified-arrows-full`: a four-row draft (`one`, `two`, `three`, `f`),
  each key on its own under Ctrl-O, then Escape and a letter. fx's screen
  stays where it is for every key (rows 2-40 of `full`, `full-super-left`,
  `full-ctrl-up`, `full-ctrl-b` and `full-ctrl-f` hash alike,
  `93aa600dbf5e`), and the letters land where the composer moved the caret:
  after Super-Left `Yf` (the row's start), after Ctrl-Up `tXhree` (the row
  above, one cell in), after Ctrl-B `tWXhree` (one cell left), after Ctrl-F
  `tWXVhree` (one right).
- `p5-modified-arrows-question`: twenty questions. At a choice Ctrl-Down
  (page 1), Alt-Up (2), Super-Up (3), Shift-Super-Down (4) and
  Shift-Alt-Up (5) move nothing, Ctrl-Shift-Down (6) moves the choice, and
  on page 7 Super-Left, Shift-Left, Ctrl-B, Shift-Right and Ctrl-F move
  nothing (fx stays on page 7). In page 7's field (`ab cd`): Super-Up, `z`:
  `zab cd`; Ctrl-Up, `y`: `zyab cd`; Shift-Alt-Down, `k`: `zykab cd` (each
  moved nothing, and the field is still the choice: a choice move would
  have left it and swallowed the letter); Ctrl-F, `x`: `zykaxb cd`;
  Super-Down, `w`: `zykaxb cdw`; Shift-Left and Ctrl-B, `v`: `zykaxb cvdw`
  (round 2's split scenario; round 1 pressed Ctrl-Up and Shift-Alt-Down
  together, so two choice moves could have cancelled).

## What xfx does now (`git diff c77a77ce 8b5aacaa -- src`)

- **The route** (`Shell::consume_inputs`): the full transcript and the
  question read the event as the decoder spelled it, as the approval already
  did. The full transcript binds no composer spelling (`full_transcript.rs`'s
  `key`, `_ => Unbound`, unchanged), so each goes on, and the fold makes it
  the arrow the composer moves by.
- **`unspelled`'s readers.** At the base it had three:
  `consume_inputs` for the full transcript and the question
  (`src/tui/shell.rs:12356` at c77a77ce); `folded` (`:14233`), whose own
  readers are `consume_inputs` for every surface after the approval
  (`:12387`) and `decide`'s draft arm (`:9756`); and the fold table's test
  (`:24193`). The first is this unit's change. With it gone `folded` was the
  only reader left, so the two are one function now, and `folded` gives every
  input what it gave before (its column of the fold table is unchanged, row
  for row): the menus, the composer and the approval's amendment read the
  keys they read. No other module reads it (`full_transcript.rs`'s
  `unspelled` is a diff line's spelling, unrelated; `toolgroup.rs`'s a
  local).
- **The question** (`question::Act::of`): Super-Up and -Down (`DraftStart`,
  `DraftEnd`, Shift or not) are `Act::Home`/`Act::End`, the field's edges and
  nothing at a choice; Shift- and Super-Left and Ctrl-B (`ComposerLeft`) are
  the new `Act::StepLeft`, the field's step and nothing at a choice; their
  Right (`ComposerRight`) is `Act::Right`, already nothing at a choice;
  Alt-f (`AltF`) is the word move, as it was through the fold; Ctrl- and
  Alt-Up and -Down (`ComposerUp`, `ComposerDown`) fall to `None`.
- **The decoder** (`cursor`, `src/tui/input.rs`): Alt before Shift on Up and
  Down, as fx's table reads it, so Shift-Alt-Up and -Down (and
  Ctrl-Alt-Shift) are `ComposerUp`/`ComposerDown`, fx's `paragraph_up`,
  which moves no choice, where they were `SelectUp`/`SelectDown`, the
  choice's move. Every other surface reads the two alike: the approval
  returns on both (`Shell::decide`), the full transcript binds neither, and
  `folded` makes both the arrow.

## The fold table's rows (`the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`)

Its third column was `unspelled`'s, "what the full transcript and the
question see". Those two read the event as it came now, so every row's third
value would be the event itself, with no fold to pin: the column is dropped,
and what each surface does with a spelling is pinned by that surface's test
(`a_held_arrow_under_the_full_transcript_moves_the_composer_behind_it_not_the_screen`,
`a_held_arrow_at_a_questions_choice_does_nothing_and_in_its_field_is_fxs_move`).
The values that locked in the old reading, row by row: `ComposerLeft` →
`Left` and `ComposerRight` → `Right` (the screen held them as the arrows, and
the question went back a question on the first); `ComposerUp` → `Up`,
`ComposerDown` → `Down` and `DraftStart`/`DraftEnd { shifted: false }` →
`Up`/`Down` (the screen scrolled, the question moved the choice);
`DraftStart`/`DraftEnd { shifted: true }` → `SelectUp`/`SelectDown` (the
question moved the choice); `AltF` → `WordRight` (the question's word move,
which `Act::of` now reads from `AltF` itself). The rows whose third value
was the event already (`AltB`, `SelectUp`, `SelectDown`, the held page keys,
`PageUp`, `WordLeft`, `Left`, `Up`, `ControlP`) lose nothing. The `folded`
column is unchanged in every row; its comment now says what it is for.

## RED (`red.txt`, `red-pairs.txt`)

- The tests at `08d6d605` over the base's code, in a `--no-local` clone:
  **3 fail**, each on the base's reading (`red.txt`): the decoder's pins
  (`CSI 1;4B` is `SelectDown` where `ComposerDown` is wanted), the
  full-transcript test (`ctrl-up moved the screen`) and the question test
  (`ctrl-up ... changed the question`: the choice wrapped to the field). The
  other 9 of the run pass: the fold table (its `folded` column is the
  base's), the question-word-left test, the approval's vertical and
  horizontal tests and the held-page-key tests.
- The scenarios on the base's binary (`v0.1.0-c77a77c`, `red/`): **4 of 28**
  pairs identical (`red-pairs.txt`): `answered`, `draft`, `p1` and the guard
  `ctrl-shift-down`. The typed rows read `fY`, `fYX`, `fYXW` and `fYXWV`
  (every key held by the screen or scrolling it, each letter at the draft's
  end); the base's `full-ctrl-up` hashes apart from its `full`
  (`fc92be6b4dc8` against `5f924c8659f2`: the screen scrolled); and at the
  question the base moved the choices fx's left, opened the field on page 5
  (Shift-Alt-Up wrapped to it), and went back from page 7 to page 5.

## GREEN (`green-pairs.txt`, `green.txt`)

- The scenarios on the code head's binary (`v0.1.0-8b5aaca`, `xfx/`, for
  `p5-modified-arrows-full`) and round 2's (`v0.1.0-0f8c6e6`, for the
  question scenario; its non-test code is the code head's but for comments):
  - **23 of 28 pairs identical**, cell by cell (`cells.py`, row 1 left out):
    all seventeen of `p5-modified-arrows-question` (round 2's split, on
    round 2's head binary `v0.1.0-0f8c6e6`), and `answered`, `draft`,
    `typed` (`Yf`), `typed2` (`tXhree`), `typed3` (`tWXhree`) and `typed4`
    (`tWXVhree`) of `p5-modified-arrows-full`;
  - the five full-transcript snaps differ across the sides before any key,
    as `p5-modified-page-keys`' do: each side's own record times, fx's taller
    `* session:` record and its second `· Response` header (the ledger's
    `session-record` and `fulltranscript-response-header` causes, the second
    now naming this scenario; Sweep below). So they are read within each
    side: on both, all five hash alike (fx `93aa600dbf5e`, xfx
    `d7af2665592f`), and every held-key snap differs from fx's in `full`'s
    rows, 2-32 and 35. On the base side `full-ctrl-up` hashes apart, and its
    differing rows are a list of its own (2-23, 25-30, 32-36).
- `green.txt`: the unit's tests in the gate's run.

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` (its remote removed) of `33cfeac4` under
`$TMPDIR/mut/`, one at a time, the file restored after each, the run under
the suite lock, each running the TUI's unit tests (`cargo test --lib --
tui::`): **14 of 14 killed**, the clone's tree clean after the run
(`mutants.txt`, each naming the first six failing tests):

| mutant | killed by |
|---|---|
| M1 the full transcript reads the composer spellings as the arrows (the base's reading) | the full-transcript test |
| M2 the question reads them as the arrows (the base's reading) | the question test |
| M3 a question takes Shift-Left, Super-Left and Ctrl-B as Left | the question test |
| M4 a question binds no Alt-f | the question-word-left test (its draft's Alt-f) |
| M5 a question takes Super-Up as Up | the question test |
| M6 a question takes Ctrl- and Alt-Up as Up | the question test |
| M7 the decoder reads Shift before Alt on Up and Down (the base's order) | the decoder's two-modifier pins, the question test |
| M8 a question binds no Super-Up | the question test (its field) |
| M9 a question binds no Shift-Left, Super-Left or Ctrl-B | the question test, the question-word-left test |
| M10 a question binds no Shift-Right, Super-Right or Ctrl-F | the question test (its field) |
| M11 the full transcript scrolls on Ctrl- and Alt-Up | the full-transcript test |
| M12 the full transcript holds Shift-Left, Super-Left and Ctrl-B | the full-transcript test |
| M13 the merged fold leaves a composer move on Left as itself | the fold table, the approval amendment's word-move test |
| M14 Shift-Left, Super-Left and Ctrl-B go back a question at a choice | the question test |

M1 and M2 put `unspelled` back, as a match over the decoded action, in front
of the full transcript's `key` and of `question::Act::of`. Not mutated: the
approval's reading, which this unit does not change and its own tests pin.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'fulltranscript*' --only p5-modified-page-keys
--only 'question*' --only 'p5-question*' --only 'p5-input-question*' --only
'composer*' --only 'p5-modified-arrows*'` at 40x120, `--jobs 2`, this
branch's scenarios, once with the base's binary (`v0.1.0-c77a77c`) and once
with the code head's (`v0.1.0-8b5aaca`), both in one suite-lock slot, then
both rescored (`--rescore`) with this branch's ledger once its refs were
re-pointed and the response-header cause named `p5-modified-arrows-full`.
Sixteen scenarios, 145 pairs: `composer-keys`, `composer-wrap`,
`fulltranscript`, `fulltranscript-notice`, `p5-input-question`, `-20`,
`-21`, `p5-modified-page-keys`, `p5-question-resume`, `-resume-ctrlo`,
`p5-question-word-left`, `question`, `question-field`, `question-other` and
this unit's two. APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py` (unchanged, run
from its folder) compares them pair by pair:

- **base 101 / 145, head 119 / 145, drops 0.**
- The eighteen that pass at the head and not at the base are this unit's:
  `typed`-`typed4` of `p5-modified-arrows-full` (unresolved to identical) and
  fourteen of `p5-modified-arrows-question` (unresolved to
  normalized-identical; `p1` and `ctrl-shift-down` passed on both).
  `full-ctrl-up` keeps its class, its uncaused rows 5 → 0: the base had
  scrolled the screen.
- One other pair changes class, and both classes pass:
  `p5-input-question20/resumed`, identical at the base and
  normalized-identical at the head -- a normalization the ledger already
  makes matched a row on one run and not the other; the scenario presses no
  key this unit changes.
- The head's one pair left with uncaused rows is `p5-modified-page-keys`'
  `full-pgup`, the document's head, with the same uncaused rows at the base:
  older than this unit (MODIFIED-PAGE-KEYS' README, its round-2 sweep).

## Gate (`gate.txt`, `green.txt`)

The brief's gate, once, under the suite lock in one normal slot with a short
`TMPDIR`, at `5e985efb`, whose `src/` is the code head `8b5aacaa`'s but for
`33cfeac4`'s comment lines (`gate.txt` carries the diff stat and the empty
list of its non-comment lines). Every step exits 0 and the tree was the same
at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5706 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5743 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh`, `check-no-local-paths.sh` | exit 0 each |

MODIFIED-PAGE-KEYS' round-2 gate read 5705 and 5742; this unit adds two
`#[test]`s (the full-transcript test, the question test) and replaces one
(`the_full_transcript_holds_the_composer_spellings_of_the_arrows_as_it_holds_the_arrows`).
No catalog-wait failure in this run. `green.txt`: the unit's twelve tests
(its three, the fold table, the question-word-left test, the approval's four
key tests, the held-page-key tests and `Act::of`'s test) in both suites of
this run, 24 of 24 ok.

## The citations (`fx-src-check.txt`)

`fx_src.sh` over this file and the unit's added lines (`src`, `tests`, the
scenarios, and `docs/parity.md`'s added words), the commands in the file:
every citation falls inside a range of `fx-src.txt`.

## The ledger (`refs.py`, `refs.txt`)

- **Refs.** This unit changes lines of `src/tui/shell.rs` above nine of the
  ledger's refs into it (`answer-drawn-wider-named`,
  `at-resize-band-at-the-bottom`, `session-record`, `transcript-page-offset`,
  `toolgroup-ctrlo-record-short`, `safe-page-edge`, `cancel-page-edge`,
  `full-transcript-shift`, `resize-document-repaint`). MODIFIED-PAGE-KEYS'
  `refs.py` (docstring only changed) read 16 checked, 9 wrong against the
  working tree; `--fix` re-pointed them, and it now reads **16 checked, 0
  wrong** (`refs.txt`). No ledger ref names `src/tui/question.rs`.
  `docs/parity.md`'s rows stay on lines 162, 237 and 238.
- **`fulltranscript-response-header`** names `p5-modified-arrows-full` and
  its four held-key snaps beside `full`: the same answer document
  (`p5-transcript-md-turns.json`), the same second header on fx's tail page,
  which no held key pages.

## `docs/parity.md`

- **The full transcript row (238):** the screen binds the plain arrows only;
  an arrow held with Super, Alt, Ctrl or Shift and Ctrl-B and Ctrl-F go to the
  composer behind it with the other unbound keys, here as there
  (`p5-modified-arrows-full`), less what the composer itself does not copy
  (the selection, Super's jump, the paragraph move), and with `/mcp`'s menu
  taking no key under the screen, as for a held page key. MODIFIED-ARROWS-SCREENS
  is no longer named as an owner; the base's reading is said once.
- **The `ask_user_question` row (162):** fx's rule at a choice and in the
  field, copied, with Shift-Alt-Up and -Down the paragraph move; Super-Left
  and -Right in the field are the step where fx's are the line's edges
  (COMPOSER-SUPER-ARROWS). Its "Three Left spellings" sentence is one now
  (`ESC ESC [ D`, META-PREFIX-DECODE): Shift-Left and Ctrl-B no longer go
  back a question, which closes what QUESTION-LEFT-SPELLINGS was proposed
  for.
- **The full-screen TUI row (237):** the decoder's classification sentence
  (Alt before Shift; the three surfaces read the event as it came, the fold
  is for the menus and the composer), the approval-horizontal sentence (the
  same), the META-PREFIX-DECODE note on QUESTION-LEFT-SPELLINGS, and one new
  sentence, rewritten in round 2: **a held Up or Down is the plain arrow
  here, where fx's is a move of its own that never reaches the history**
  (Alt by paragraph, Super to the draft's edges, Shift a row that stops at
  the first and last row), with xfx's edge-then-recall road and its reach
  under the full transcript, owner proposed COMPOSER-PARAGRAPH-MOVE,
  widened to held Up and Down as fx's (Decisions). It was unrecorded, and
  this unit makes it reachable under the full transcript.
- **Round 2** also rewrites row 237's COMPOSER-SUPER-ARROWS sentence, which
  still said only an approval tells `ComposerLeft`/`ComposerRight` and
  `DraftStart`/`DraftEnd` apart (opus SHOULD 2), names the word move for
  Super-Left/Right with Ctrl or Alt in rows 162 and 237 (opus NIT 1), and
  gives row 238 the history recall in place of "moves the caret as it does
  with the screen closed" (opus SHOULD 1).

## Decisions (reversible)

- **`unspelled` merged into `folded`**, not kept as a helper: after the
  change its one reader was `folded`, and a function named for what two
  surfaces read would describe a route that no longer exists.
- **Shift-Alt-Up and -Down in the decoder, in this unit**: they are held
  arrows at a question, fx's line is in hand (`escape_parser.zig:83` before
  `:99`), and only the question reads the change (the approval returns on
  both spellings, the screen binds neither, the fold gives both the arrow).
  Ctrl-Shift-Up stays `SelectUp`: fx moves the choice on it.
- **`Act::StepLeft`, a new variant**, rather than reading `ComposerLeft` as
  `Act::Left`: at a choice `Act::Left` is the previous question, which fx's
  composer move and its raw byte never reach.
- **Not copied here, each recorded and owned elsewhere:**
  - **A held Up or Down takes the plain arrow's road in the composer**,
    where fx's has a move of its own that never reaches the history. fx:
    Alt (`paragraph_*`) and Super (`draft_*`) move by `moveInputCursor`
    alone (`app_input_runtime.zig:298-309`); Shift and Ctrl-Shift
    (`visual_*` with the selection) move one row and stop at the draft's
    first or last row (`vertical_navigation.zig:74-77`, called at
    `input_completion_runtime.zig:262-268`; `input_completion_runtime.zig:270`),
    before the edge jump, the steer and the history (`:272-291`). xfx folds
    every one to the arrow, and `Shell::act`'s Up and Down go one row, then
    to the draft's edge, then (Up, empty composer) to the waiting steer,
    then to `Shell::recall`. So on the draft's first or last row, and at an
    empty composer, xfx's Alt-, Super- and Shift-held Up and Down recall
    the line before or after where fx's stop; Ctrl-Up and -Down are fx's
    plain `visual_*` and recall alike. This unit made it reachable under
    the full transcript, where Alt- and Super-Up and -Down scrolled the
    screen before (Shift-Up reached the composer already): there the recall
    happens behind a screen that does not show it. Owner proposed:
    COMPOSER-PARAGRAPH-MOVE, widened to held Up and Down as fx's (Alt by
    paragraph, Super to the draft's edges, Shift a row that stops at the
    first and last row, none reaching the history), the decoder line's next
    unit, ahead of META-PREFIX-DECODE (the dispatcher's decision); the
    selection itself stays COMPOSER-SELECTION-EXTEND's (round 2, opus
    SHOULD 1).
  - **Super-Left and -Right are the step** in the composer and in the
    question's field, and with Ctrl or Alt held too the word move (`cursor`
    reads Ctrl and Alt first), where fx's jump to the line's edges whatever
    else is held (Super's arm first, `escape_parser.zig:74-78`), because the
    decoder reads Super on Left and Right as `ComposerLeft`, like Shift
    (COMPOSER-SUPER-ARROWS). The scenario's Super-Left is pressed with the
    caret one cell into its row, where xfx's step and fx's jump land on the
    same cell; anywhere else they differ by that owner's gap, which the
    question test pins as xfx's step with that owner named, for Super-Left
    from the field's end and, since round 2, Super-Right from its start.
  - Shift's selection (COMPOSER-SELECTION-EXTEND).
- **The scenarios press each key on its own**, each followed by a letter
  where its effect shows (MODIFIED-PAGE-KEYS' round-2 lesson, its opus NIT
  2); at the question each vertical key on a page of its own.

## Round 2 (the reviews of `8a9be523`: gpt6 no MUST; opus APPROVE-WITH-FIXES, no MUST)

1. **The history recall** (opus SHOULD 1): recorded in `docs/parity.md`'s
   rows 237 and 238 and in Decisions above, with fx's lines
   (`app_input_runtime.zig:298-309`; `vertical_navigation.zig:74-77`,
   `input_completion_runtime.zig:262-270`, `:272-291`, now in `fx-src.txt`);
   owner COMPOSER-PARAGRAPH-MOVE, widened. No code change, as the dispatcher
   decided: the fix is that unit's.
2. **The stale clause** (opus SHOULD 2) and **Super with Ctrl or Alt**
   (NIT 1): `docs/parity.md` above.
3. **Comments** (NIT 2, `0f8c6e6a`): `editor.rs`' note on the vertical
   arrows (Shift-Alt is `ComposerUp` now), and two in `input.rs` (Ctrl-B and
   Ctrl-F are told apart by a question's choice and the full transcript
   too).
4. **The scenario** (NIT 3, `0f8c6e6a`): Ctrl-Up and Shift-Alt-Down each on
   its own, each followed by a letter (`field-ctrl-up`, then the new
   `field-shift-alt-down`). Recaptured on fx, on round 2's head binary
   (`v0.1.0-0f8c6e6`) and on the base's (`captures.txt`, round 2's block):
   the scenario's pairs went from **16 of 16** identical (round 1) to **17
   of 17**, and on the base from 2 of 16 to 2 of 17 (`p1` and the guard
   `ctrl-shift-down`); the receipt's totals from 22 of 27 to **23 of 28**,
   and 4 of 27 to 4 of 28 (`green-pairs.txt`, `red-pairs.txt`).
5. **Super-Right in the field** (NIT 4, `0f8c6e6a`): pinned as xfx's step
   from the field's start (Super-Up, then Super-Right, then `p`:
   `tpzsyaxb cduvwqr`), beside the Super-Left pin; fx's `line_end` would put
   the `p` last. COMPOSER-SUPER-ARROWS flips both.
6. **Refs and citations**: the comment and test lines moved six of the
   ledger's refs into `src/tui/shell.rs`; `refs.py --fix` re-pointed them
   (`refs.txt`: 16 checked, 0 wrong). `fx-src-check.txt` is round 2's run.
7. **Gate**: the brief's gate on a `--no-local` clone at `72984456` (round
   2's records commit; its non-test `src/` is the code head's but for
   comments), under the suite lock: every step exits 0, test 5706 passed,
   test-fi 5743 passed, 0 failed (round 2 adds no `#[test]`); `green.txt`
   24 of 24 ok (`gate.txt` and `green.txt`, round 2's block first).

