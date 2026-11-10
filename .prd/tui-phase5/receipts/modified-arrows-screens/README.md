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
| the commits after | `docs/parity.md`, the ledger, this folder; no `src/` change |

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
  `zab cd`; Ctrl-Up and Shift-Alt-Down, `y`: `zyab cd` (nothing moved, the
  field still marked); Ctrl-F, `x`: `zyaxb cd`; Super-Down, `w`:
  `zyaxb cdw`; Shift-Left and Ctrl-B, `v`: `zyaxb cvdw`.

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
- The scenarios on the base's binary (`v0.1.0-c77a77c`, `red/`): **4 of 27**
  pairs identical (`red-pairs.txt`): `answered`, `draft`, `p1` and the guard
  `ctrl-shift-down`. The typed rows read `fY`, `fYX`, `fYXW` and `fYXWV`
  (every key held by the screen or scrolling it, each letter at the draft's
  end); the base's `full-ctrl-up` hashes apart from its `full`
  (`fc92be6b4dc8` against `5f924c8659f2`: the screen scrolled); and at the
  question the base moved the choices fx's left, opened the field on page 5
  (Shift-Alt-Up wrapped to it), and went back from page 7 to page 5.

## GREEN (`green-pairs.txt`, `green.txt`)

- The scenarios on the code head's binary (`v0.1.0-8b5aaca`, `xfx/`):
  - **22 of 27 pairs identical**, cell by cell (`cells.py`, row 1 left out):
    all sixteen of `p5-modified-arrows-question`, and `answered`, `draft`,
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

## Mutants, sweep and gate

Recorded by the commit after this folder's first.

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
  sentence: **Alt with Up or Down is a one-row step here, where fx's is a
  paragraph move**, owner proposed COMPOSER-PARAGRAPH-MOVE. It was
  unrecorded, and this unit makes it reachable under the full transcript.

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
- **Not copied here, each recorded and owned elsewhere:** Super-Left and
  -Right are the step in the composer and in the question's field, where fx's
  jump to the line's edges, because the decoder reads Super on Left and
  Right as `ComposerLeft`, like Shift (COMPOSER-SUPER-ARROWS); Super-Up and
  -Down move the composer one row where fx's go to the draft's edges (the
  same owner); Alt-Up and -Down move the composer one row where fx's move by
  paragraph (COMPOSER-PARAGRAPH-MOVE, proposed); Shift's selection
  (COMPOSER-SELECTION-EXTEND). The scenario's Super-Left is pressed with the
  caret one cell into its row, where xfx's step and fx's jump land on the
  same cell; anywhere else they differ by that owner's gap, which the
  question test pins as xfx's step with that owner named.
- **The scenarios press each key on its own**, each followed by a letter
  where its effect shows (MODIFIED-PAGE-KEYS' round-2 lesson, its opus NIT
  2); at the question each vertical key on a page of its own.
