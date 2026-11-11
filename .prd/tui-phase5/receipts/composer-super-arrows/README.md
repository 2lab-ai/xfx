# COMPOSER-SUPER-ARROWS receipt: held Left, Right, Home and End, as fx reads them

fx v0.0.16's `modifiedArrowAction` reads Super first, so Super-Left and
-Right go to the logical line's start and end whatever else is held, and
Super- and Ctrl-Home and -End go to the draft's edges. Shift-Home and -End
go to the line's edges, and Alt-Home and -End are the plain keys. xfx's
decoder read Super on Left and Right as the one-character step. With Ctrl or
Alt also held it read them as the word move, and it read Home and End as the
line's edges whatever was held. MODIFIED-ARROWS-SCREENS and
COMPOSER-PARAGRAPH-MOVE copied the held Up and Down; this unit copies the
rest: Left, Right, Home and End, the tilde Home and End too.

| commit | what |
|---|---|
| `82895f13` | RED: three new byte-level shell tests, plus new rows in the question and full-transcript tests (the question field's two Super-Left/Right rows flipped to fx's). Scenarios `p5-super-arrows`, `-question` and `-approval`. Tests and scenarios only |
| `1fcf30b6` | **the code head**: `Action::LineStart`/`LineEnd`, `cursor`'s Super-first order and its Home and End rows, `tilde_key`'s rule, `folded`, `act`, the editor's arms, `question::Act::of`, comments in `decide` and `approval::edits`. As tests: the decoder tables (one new, five changed), the fold table and an editor test |
| `32969730` | comments only: bare fx citations given their file (`escape_parser.zig`) |
| the commits after | `docs/parity.md`, the ledger's refs, this folder; no `src/` change |

The source change outside the test modules is +163/-86 over five files
(`srcsize.txt`, `srcsize.py`: the hunks above each file's `mod tests` in
`git diff -U0 82895f13 1fcf30b6 -- src`, the same to `32969730`): input.rs +93/-43, shell.rs +35/-20, question.rs +19/-14,
editor.rs +11/-6, approval.rs +5/-3. That is under the brief's 300.

## fx's rule, key by key (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The decoder** (`modifiedArrowAction`, `escape_parser.zig:73-108`). The
mask bits are at `:61-64`, and every result is a `composerMove` whose
`extend_selection` is the Shift bit (`:66-71`).
- Super's arm is first: Left `line_start`, Right `line_end`, Home
  `draft_start`, End `draft_end` (`:74-82`).
- Then Alt's arm: Left and Right are the word moves, and it has no Home or
  End row (`:83-89`). The function returns its `null` and the plain key
  follows (`navigationKeyAction`, `:163-167`; `:727-733`). So Ctrl-Alt-Home
  and Shift-Alt-End are the plain keys.
- Then Ctrl's arm: Home `draft_start`, End `draft_end` (`:90-98`).
- Then Shift's: Home `line_start`, End `line_end` (`:99-107`).
- The keypad's Left, Right, Home and End take the same table (`:134-144`,
  `:196-197`).
- **The tilde Home and End** follow their own rule (`:694-718`). With no
  modifier they are `.home`/`.end`. With any bit in the field they are
  `draft_*` if Ctrl's bit is set and `line_*` if not (`:699-707`). Super, a
  lock bit and a bit past eight count as a modifier like any other and are
  not read further.

**What `.home` is**: to the composer it is `line_start`
(`fromEscapeAction`, `shortcuts.zig:51-52`; `terminal_action_decoder.zig:214-224`).
Past the composer's route it is `unreachable` (`app_input_runtime.zig:1021-1035`).
No surface tells `.home` from `line_start`:

| key (fx's move) | composer | question choice / field | approval choice / amendment | full transcript | menus |
|---|---|---|---|---|---|
| Super-Left/-Right, with anything (`line_start`/`line_end`) | the logical line's start / end (`horizontal_navigation.zig:31-38`; `logicalLineStart`/`logicalLineEnd`, `text_boundaries.zig:79-89`) | nothing (`input_question_runtime.zig:181-200`) / the field's start / end (`src/ui/input/runtime.zig:162-163`; `question_prompt.zig:456-468`) | nothing (`input_approval_runtime.zig:128`) / the draft's start / end (`src/ui/input/runtime.zig:82-83`; `approval_amendment.zig:84-92`) | to the composer (`keyForAction`, `input_full_transcript_runtime.zig:156-178`) | `/statusline`, `/usage` and the settings menu: nothing (they move on `.cursor_left/right` alone, `app_input_runtime.zig:984-992`, `:2228-2263`); a `/model` column dropped (`abandonModelPickerOptions`, `:298-309`) |
| Super-Home/-End, Ctrl(-Shift)-Home/-End, the tilde's with Ctrl (`draft_*`) | the draft's start / end (`horizontal_navigation.zig:29-30`) | as above | as above | as above | as above |
| Shift-Home/-End, the tilde's without Ctrl (`line_*`) | the line's edges, with the selection (COMPOSER-SELECTION-EXTEND) | as above | as above | as above | as above |
| Alt(-Ctrl, -Shift)-Home/-End (`.home`/`.end`) | the line's edges | as above | as above | as above | as above |

The composer's arm for all four is `routeComposerShortcutAction`'s
`line_start ... draft_end` (`app_input_runtime.zig:298-309`). It moves the
caret (`moveInputCursor`) and does nothing else: no edge, no history. The
sign-in picker swallows every one alike (`app_auth_runtime.zig:730-743`).

**Line edges are logical lines**: `logicalLineStart` walks back to the
byte after a `\n` and `logicalLineEnd` forward to the next one
(`text_boundaries.zig:79-89`), not to a screen row. xfx's editor `Home`/`End`
are already that (`Editor::line_start`/`line_end`, `src/tui/editor.rs`,
`rfind('\n')`/`find('\n')`). The oracle shows it: `fx-p5-super-arrows-40x120-super-left`
puts `A` at the wrapped line's first row, not the third row the caret was on.

## The oracle (`fx/`, `captures.txt`)

Each key is pressed on its own, and each is followed by its own letter.
Shift-Home and Shift-End are the exception (see below).

- `p5-super-arrows`:
  - First `/statusline`: Super-Left, Super-Right and Ctrl-Home leave every
    value where it was (`Context=off`).
  - Then the draft `one two/<249 cells over three rows>/the end`, the caret
    on the wrapped line's last row, seven cells in (`cursor draft`: 9,5):
    - Super-Left puts `A` at the line's start, on its first row;
    - Super-Right puts `B` at its end;
    - Ctrl-Home puts `C` at the draft's start;
    - Super-End puts `D` at the draft's end;
    - Ctrl-Super-Left from `the e|ndD` puts `E` at the line's start (Super
      wins);
    - Alt-Super-Right from `Eth|e` puts `F` at the end.
  - Shift-Home and Shift-End get a `cursor` line each (2,3 and 22,5: the
    logical line's first and last cell), then the plain Home/End. fx's
    selection makes a letter replace text.
  - Then:
    - Alt-Home is the plain Home (`G`);
    - Ctrl-Alt-Home is the plain Home too (`H`);
    - the tilde Ctrl-Home goes to the draft's start (`I`);
    - the tilde Super-End goes to the **line's** end (`J`);
    - the keypad's Ctrl-End goes to the draft's end (`K`).
- `p5-super-arrows-question`:
  - On page 3 at a choice, Super-Left, Ctrl-Super-Left, Super-Right,
    Ctrl-Home and Super-End each do nothing.
  - In the field, from `ab c|d ef`, each key is followed by a letter:
    Super-Left `z`, Super-Right `y`, Ctrl-Super-Left `x`, Alt-Super-Right
    `w`, Ctrl-Home `v`, Ctrl-End `u`, Super-Home `t`, Super-End `s`,
    Shift-Home `r`, tilde Ctrl-End `q`. The field ends as
    `rtvxzab cd efywusq`.
- `p5-super-arrows-approval`:
  - At choice 2 the same five keys do nothing.
  - In the amendment on choice 1, the same ten keys and letters give
    `rtvxzab cd efywusq`.

## RED (`red.txt`, `red/`, `red-pairs.txt`)

- `red.txt`: the tests at `82895f13` over the base's code. Four of five
  fail at their first fx key:
  - Super-Left from the wrapped line's middle landed one cell left (107 for
    8);
  - the amendment's Super-Left gave `ab zcd ef` for `zab cd ef`;
  - Super-Left flipped `/statusline`'s Context;
  - the question field's Super-Left gave `tzsyaxb cduvwqr` for
    `qtzsyaxb cduvwr`.

  The full transcript's test has no RED: Home and End already reached the
  composer behind it.
- `red-pairs.txt` (`pairs.sh red`, the base's binary `v0.1.0-28bd181`):
  10 of 48 pairs identical, and 6 more differ only in the approval's scope
  rows (Pairs, below).
  - Identical: the draft before any key; `/statusline` before a key, and
    after Super-Right, which flipped Context back, and after Ctrl-Home;
    every question key at a choice.
  - Different:
    - `statusline-super-left` (`Context=on`);
    - every composer snap after it (`super-left` puts `A` mid-row, and
      `ctrl-home`'s `C` lands at the line's start);
    - every question-field snap and every amendment snap (`ab zcd ef` ...);
    - the `shift-end` cursor (26,5 for 22,5, the text before it already
      off).

## GREEN (`green-pairs.txt`, `green.txt`, `xfx/`)

- `green-pairs.txt` (`pairs.sh xfx`, the code head's binary
  `v0.1.0-1fcf30b`): 32 of 48 pairs identical, 15 more differing only in the
  approval's scope rows, and one different, `amend-super-right`.
  - Identical: every composer and question pair, and the three cursor lines
    (9,5; 2,3; 22,5).
  - **Pairs**: every approval pair differs in rows 19 and 21-25, `down`
    (before any key under test) included: the grant scope's sentence and the
    saved session's id. Those are the rows APPROVAL-HORIZONTAL-KEYS'
    receipt read its pairs against too. `pairs.sh` calls a pair SCOPE when
    its rows are exactly `down`'s.
  - **`amend-super-right`** differs in row 18 as well. Its text is
    identical (`zab cd efy`). fx's captured frame has no reverse-video
    caret cell after the `y`, and xfx's does. fx's capture is the same:
    - when captured a second time
      (`probe/fx-rerun-p5-super-arrows-approval-40x120-amend-super-right.ansi`);
    - with the **plain End** in place of Super-Right (`probe/probe-end.cap`,
      `probe/fx-probe-end-40x120-amend-super-right.ansi`).

    So the difference is not the key's effect. Whether fx or the capture
    leaves the cell out is not traced [inferred: fx's frame] (Open).
- `green.txt`: the unit's 19 tests at the head, on both suites: 19 + 19 ok.

## The readers, and what changed for each

| reader | before | now | why |
|---|---|---|---|
| the decoder (`cursor`, `src/tui/input.rs`) | Super-Left/-Right `ComposerLeft`/`Right` (with Ctrl or Alt, `WordLeft`/`Right`); Home and End `Home`/`End` whatever was held | Super first: `LineStart`/`LineEnd`, Home/End `DraftStart`/`DraftEnd`; then Alt: Home/End the plain keys; Ctrl: `DraftStart`/`DraftEnd`; Shift: `LineStart`/`LineEnd`; `shifted` remembered | `escape_parser.zig:73-108` |
| the tilde keys (`tilde_key`) | Home/End through `cursor`'s rows: `Home`/`End` | any bit: `DraftStart` with Ctrl's bit, `LineStart` without | `:699-707`; through `cursor`, Super-Home would have become the draft's start where fx's tilde gives the line's |
| the full transcript (`key`) | `Unbound` | `Unbound` (its `_`) | **unchanged**: fx binds neither `.home` nor a composer move |
| a question (`Act::of`) | Super-Left `StepLeft`, with Ctrl/Alt `WordLeft`; Home `Home` | `LineStart`/`LineEnd` `Home`/`End`, beside `DraftStart`/`DraftEnd` | `src/ui/input/runtime.zig:162-163` |
| an approval's choice (`decide`) | swallowed | swallowed (`Input::Action(_) => return`) | **unchanged** |
| an approval's amendment (`decide` → `folded` → `approval::edits`) | Super-Left the step (folded `ComposerLeft`) | `Home` (folded `LineStart`); Ctrl/Super-Home `DraftStart`, ahead of the fold | `src/ui/input/runtime.zig:82-83` |
| `folded` (`src/tui/shell.rs`) | -- | `LineStart`/`LineEnd` -> `Home`/`End` | fx's `.home` is `line_start` |
| the compact menus, the settings menu, the pickers, `/model`'s column | the folded `Left`: Super-Left flipped `/statusline`, changed `/usage`'s period, changed a setting, stepped a column back | `Home`/`End`, which **none binds** (unchanged readers); the column drops on them as before | `app_input_runtime.zig:984-992`, `:2228-2263`, `:298-309` |
| the composer (`act`) | `ComposerLeft` -> `act(Left)` | `LineStart` -> `act(Home)` (a fallback: `folded` already made it `Home`) | `:298-309` |
| the editor (`Editor::apply`) | -- | `LineStart`/`LineEnd` with `Home`/`End` | `horizontal_navigation.zig:31-38` |

These readers stay unchanged, as fx treats the keys alike:
- the full transcript;
- an approval's choice;
- `approval::edits`;
- every menu and picker, which now see `Home`/`End`.

**Pins changed**:
- the decoder's `alt_f_and_the_composer_spellings_of_the_arrows_are_their_own_actions`
  (`1;9D`, `1;10D`, `1;9C`, `57418;9u`, `1;11D`);
- `a_sequence_is_a_key_only_in_the_exact_shapes_a_terminal_sends_one`
  (`1;16D`, `1;5H`);
- the tilde tests that said `Home`/`End` for a held key: the four
  `assert_modified_tilde` tests, the field sweep (renamed
  `every_modifier_field_on_the_tilde_home_and_end_is_upstreams_move`), and
  the leading-zero, past-256 and lock-bit tests.

The fold table gains four rows. `TILDE_MODIFIERS` gains each field's draft
and Shift bits.

## Mutants (`mutants.txt`, `mutants.py`)

On a `--no-local` clone at `32969730` under the suite lock, one mutant at a
time, each running `cargo test --lib -- tui::`: **12 of 12 killed**, the
clone clean after the run.

| mutant | killed by |
|---|---|
| M1 Super-Left/-Right the step (the base's `ComposerLeft`) | the decoder tables, the composer, question and amendment tests (+1) |
| M2 Ctrl and Alt read before Super on Left/Right (the base's order) | the decoder tables, the composer, question and amendment tests |
| M3 Alt's Home/End fall through to Ctrl's (no `null`) | the Left/Right/Home/End table, the composer test |
| M4 Ctrl-Home/-End the line's edges | the decoder tables, the composer and amendment tests |
| M5 Super-Home/-End the line's edges | the Left/Right/Home/End table, the composer test |
| M6 Shift-Home the bare Home (its Shift forgotten) | the Left/Right/Home/End table |
| M7 the tilde Home/End through `cursor`'s rows | the tilde tests (past-256, lock-bit, the four `assert_modified_tilde`, the field sweep) |
| M8 the tilde Home/End the line's edges under Ctrl | the tilde tests (leading-zero, past-256, the four, the field sweep, +2) |
| M9 the tilde keys read a lock bit alone as nothing | past-256, the page-key test, the lock-bit test |
| M10 no fold of the line's edges | the amendment test, the menu test, the fold table |
| M11 a question reads the line's edges as nothing | the question test |
| M12 the editor's line start the line's end | the editor test |

M11's list also names `tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop`,
a catalog-wait test (the known flake class); the question test kills M11 on
its own.

Not mutated: `act`'s `LineStart => act(Home)` fallback, which `folded`
makes unreachable (an equivalent mutant while the fold stands; M10 removes
the fold and the fallback alone does not save the menus' column or the
amendment).

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

The command:

```
scripts/fxcap/sweep.sh --only 'composer*' --only 'p5-modified-*'
  --only 'p5-paragraph-*' --only 'question*' --only 'p5-question*'
  --only 'approval*' --only 'p5-approval*' --only 'fulltranscript*'
  --only 'p5-input-*' --only 'p5-super-arrows*'
```

It ran at 40x120 with `--jobs 2`, on this branch's scenarios and its ledger
with the refs re-pointed. It ran once with the base's binary
(`v0.1.0-28bd181`) and once with the code head's (`v0.1.0-1fcf30b`), both
in one suite-lock slot. That is 47 scenarios and 432 pairs.
APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py` (unchanged, run from its
folder) compares them pair by pair:

- **base 264 / 432, head 286 / 432, drops 0.**
- **This unit's three scenarios**: 10 of their 48 pairs pass at the base and
  32 at the head, as `pairs.sh` counts them.
  - The 16 that do not pass are the approval's pairs. The ledger's
    `approval-grant-scope` cause leaves them unresolved on both sides.
  - The head's approval pairs carry no uncaused row but
    `amend-super-right`'s caret (GREEN).
- **The other 44 scenarios' 384 pairs** pass 254 on both sides. One pair
  changes class, `p5-input-question20/q20-answered`. It goes from identical
  to normalized-identical, a pass on both sides, with no cause and no
  uncaused row on either.
- These held keys' pairs keep their class: `p5-modified-arrows-question`'s
  `super-left`, `p5-approval-horizontal-keys`' `super-left` and
  `super-right`, and `p5-modified-arrows-full`'s `full-super-left`. Each
  has nothing at a choice, or the scope cause, on both sides.
- **Uncaused rows**: 41 pairs have them at the base and 10 at the head.
  - Nine of the head's ten have the same rows at the base: `p5-input-approval`'s
    `cmd-ask`, `edit-ask` and `after`; `p5-input-paste-amend`'s `end`;
    `p5-modified-page-keys`' `full-pgup`; `p5-paragraph-move`'s four
    full-transcript screens. All are older than this unit.
  - The tenth is `amend-super-right`'s caret row (Open).
- No scenario was refused, failed or timed out.

## Gate (`gate.txt`)

The brief's gate once, under the suite lock, on a `--no-local` clone at
`5bc8f9dc` (its `src/` is `32969730`'s):
- `cargo fmt --check` and both clippy runs pass;
- `cargo test`: 5718 passed; with `fault-injection`: 5755 passed; 0 failed
  and 7 ignored each;
- the identity, stubs, secrets and local-paths checks all exit 0;
- the clone is clean after.

The base unit's gate was 5713 and 5750 (COMPOSER-PARAGRAPH-MOVE's
`gate.txt`, round 2). That is +5 each: the three new shell tests, the
decoder's Left/Right/Home/End table and the editor test. Every other
changed test was rewritten or renamed, not added. No catalog-wait failure
came up.

## Refs (`refs.txt`, `refs.py`)

`refs.py` is COMPOSER-PARAGRAPH-MOVE's, with `question.rs` added to its
files. It checks the ledger's refs into the files this unit changes, by
content, against the base (`refs.py 28bd1810`). 11 of 16 had moved, and
`--fix` re-pointed them; `refs.txt` reads 16 checked, 0 wrong.

## Parity (`docs/parity.md`)

- The full-screen TUI row's two queued items ("Super with Left or Right is
  a one-character step here", "Ctrl with Home or End is the caret line's
  edge here") are now one paragraph. It states the copied rule above, with
  this unit's scenarios.
- The tilde Home/End sentence states fx's Ctrl-bit rule.
- The approval paragraph says Super-Left is `Action::LineStart`, and that
  its amendment takes Super-Left/-Right as Home and End.
- The paste-spaces paragraph names the new keys on each side of a line
  break, and a new "Also not changed, found by COMPOSER-SUPER-ARROWS"
  sentence beside it records fx's missing caret cell (GREEN, Open).
- The `ask_user_question` row's field sentence and the full transcript
  row's composer clause drop the gap.
- No row names COMPOSER-SUPER-ARROWS as an owner. It appears only as
  "before COMPOSER-SUPER-ARROWS" or as the attribution of a rule.

## Decisions

- **Ctrl- and Super-Home reuse `DraftStart`/`DraftEnd`.** fx makes the same
  `composerMove(.draft_start)` for them as for Super-Up (`:75`, `:79`,
  `:95`), so no reader can tell them apart.
- **`LineStart`/`LineEnd` are new actions, folded to Home/End**, rather
  than decoded as `Home`. They keep Shift's bit for COMPOSER-SELECTION-EXTEND,
  and the fold is where `ComposerLeft` already becomes `Left`. fx's `.home`
  is `line_start` on every surface, so the fold changes no reader.
- **The tilde keys are in scope.** `parity.md`'s row named this unit as
  the proposed owner (found by DECODER-TILDE-HOME-END). Also, `tilde_key`
  went through `cursor`'s rows, so changing those alone would have made the
  tilde Super-Home the draft's start where fx's is the line's.
- **Super-Left in an amendment is Home there.** On a draft with a pasted
  line break, that is the line's start, where fx's is 0. fx's draft never
  holds a break (`input_paste_runtime.zig:19-25`), and fx treats the two
  alike; the break is APPROVAL-PASTE-SPACES' (parity).
- **The editor's `LineStart`/`LineEnd` sit with `Home`/`End`**, not in its
  session arm. No production caller hands them to it -- `folded` makes them
  `Home`/`End` before `act` and an amendment's draft -- and only its own
  test (`the_line_edge_moves_are_home_and_end`) reaches them. They are that
  move, so a later caller that skipped the fold would still get fx's place,
  where the session arm would make them nothing.
- **`/usage` left the menus' band test.** With no sign-in it shows no
  period to change, so its reading is pinned on the folded actions instead
  (`UsageAction::of_action`).

## Open

- **fx's amendment caret cell** (GREEN, `amend-super-right`). fx's
  captured frame shows no caret cell after a letter appended at the end of
  a 10-character draft, in that scenario state, on two runs and with the
  plain End. Not this unit's key; fx's frame or the capture, not traced. Recorded in `docs/parity.md`'s
  approval paragraph ("Also not changed, found by COMPOSER-SUPER-ARROWS");
  unowned, and not in the ledger.
- **kitty's Ctrl-Shift-a/-e/-b/-f** (`escape_parser.zig:235-242`) are
  `line_*` and `character_*` with the selection. xfx reads them as
  `Ignore`. They are now expressible (`LineStart{shifted: true}`), but they
  are not `modifiedArrowAction`'s, so this unit leaves them.
- The meta-prefixed `ESC ESC [ C/D` word moves stay META-PREFIX-DECODE's.
  Shift-Left still flips `/statusline`, which is COMPACT-MENU-HELD-KEYS'
  (unchanged).
