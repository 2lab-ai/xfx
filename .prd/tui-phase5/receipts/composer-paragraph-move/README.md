# COMPOSER-PARAGRAPH-MOVE receipt: a held Up or Down, as fx moves it

fx v0.0.16 decodes Up and Down held with Super, Alt, Ctrl or Shift to moves
of their own, and only Ctrl's reaches the draft's edge, the waiting steer and
the history. xfx's decoder read Alt and Ctrl alike (`ComposerUp`), and its
shell folded every held Up and Down into the plain arrow for every menu and
for the composer (`folded`): one row, then the draft's edge, then the steer,
then the history. So on the draft's first or last row, and at an empty
composer, xfx's Alt-, Super- and Shift-held Up and Down recalled a line where
fx's never do -- since MODIFIED-ARROWS-SCREENS behind the full transcript too
-- and every held arrow walked a compact menu fx's leave alone.
MODIFIED-ARROWS-SCREENS' opus review found it (r1, "The dispatcher's
question" and SHOULD 1); the dispatcher widened this unit to "held Up/Down as
fx".

| commit | what |
|---|---|
| `f9995dd5` | RED: five byte-level shell tests (the test that pinned Shift-Up's recall, `shift_on_the_arrows_is_the_arrow_everywhere_but_a_question`, inverted), the scenarios `p5-paragraph-move` and `p5-paragraph-move-menus`. Tests and scenarios only |
| `45d9ad1f` | **the code head**: the decoder's Alt arm (`Action::ParagraphUp`/`ParagraphDown`), `folded` without the vertical arms, `arrow_to_pickers`, the compact menus reading the event unfolded, `act`'s arms, the editor's paragraph moves and fx's boundaries, `forget_column`, the `/model` column's drop, the approval's list; the decoder table, the fold table, the editor tests and the approval list as tests |
| `3df0b24d` | one more test (Shift-Up's forgotten column, a unit case and `p5-paragraph-move-column`) and a scenario comment; no `src/` change outside the test modules |
| the commits after | `docs/parity.md`, the ledger's refs, this folder; no `src/` change |

The source change outside the test modules is +240/-69 lines over four files
(`git diff -U0 3f2d6e78 3df0b24d -- src`, the hunks above each file's
`mod tests`): editor.rs +111/-5 (88 of them fx's boundaries, docs and blank lines included), shell.rs
+90/-38, input.rs +36/-24, approval.rs +3/-2. 240 added is under the brief's
300; added and removed together are 309.

## fx's rule, key by key (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The decoder** (`modifiedArrowAction`, `escape_parser.zig:73-108`): Super
first (`:74-76`, `draft_start`/`draft_end`), then Alt (`:83-85`,
`paragraph_up`/`_down`), then Ctrl (`:90-94`, `visual_up`/`_down`), then
Shift (`:99-101`, the same); each a `composerMove` whose `extend_selection`
is the Shift bit (`:66-71`). So Shift-Alt and Ctrl-Alt are Alt's,
Shift-Super is Super's, and Ctrl-Shift is `visual_up` with the selection.
The event is `.composer_shortcut` (`shortcuts.zig:61`), never `.cursor_up`.

**The route** (`routeResolvedEscapeAction`, `app_input_runtime.zig:896-1015`):
the full transcript binds none of them (`:930`; `keyForAction`,
`input_full_transcript_runtime.zig:147-198`); a question and an approval read
them first (`app_input_runtime.zig:955-971`); an open compact menu -- `/statusline`, the read-only
panel, `/usage`, `/workspace` (`activeCompactCommandMenu`, `app_input_runtime.zig:1840-1846`) --
takes the event and moves only on `.cursor_up`/`.cursor_down` (`else =>
return`, `app_input_runtime.zig:2259-2263`); everything else reaches the composer's move
(`app_input_runtime.zig:999-1001`, `routeComposerShortcutAction`, `:244-330`), where:

- `paragraph_*` and `draft_*` drop a later `/model` column
  (`abandonModelPickerOptions`, `input_completion_runtime.zig:1434-1437`) and
  move the caret, and nothing else (`app_input_runtime.zig:298-309`;
  `moveIntent`, `horizontal_navigation.zig:19-51`, which forgets the run's
  column at `:25`): `draft_start` 0, `draft_end` the length (`:29-30`),
  `paragraph_*` fx's boundaries (`:39-46`; `previousParagraphStart`,
  `nextParagraphStart`, `lineIsBlank`, `text_boundaries.zig:79-147`, a blank
  line being spaces, tabs and carriage returns).
- `visual_*` go to `routeComposerVertical` (`input_completion_runtime.zig:215-291`):
  an open picker first (`routeNonSlashPickerMove`, `:230`, `:395-422`: the
  `/mcp`, settings, help, model, effort, sign-in, skills, sessions and fork
  menus, the `@` picker, the model and provider pickers), then the slash
  completion (`:255-259`), then the row (`applyTargetWithSelection`, `:262-268`;
  `vertical_navigation.zig:56-80`, which forgets the column where there is no
  row, `:70-73`), then `if (page or extend_selection) return;`
  (`input_completion_runtime.zig:270`), then the draft's edge, the steer and
  the history (`:272-291`).
- A rule change's confirmation is an approval (`submitRuleManagementChoice`
  clears the approval prompt, `input_approval_runtime.zig:71-130`), whose
  `else => {}` (`:128`) takes every composer move.

| key | composer, mid-draft | first / last row | empty draft | a picker open | a compact menu, a rule confirmation | approval choice / amendment | question choice / field | full transcript |
|---|---|---|---|---|---|---|---|---|
| Alt, Shift-Alt, Ctrl-Alt (`paragraph_*`) | its paragraph's start, or the one before from there or from a blank line / the next paragraph's start | the first paragraph's start, then nowhere / the draft's end | nothing | not walked; the caret moves; a later `/model` column back to the models | nothing | nothing / nothing (`src/ui/input/runtime.zig:86`) | nothing / nothing (`src/ui/input/runtime.zig:155-197`) | to the composer |
| Super, Shift-Super (`draft_*`) | the draft's start / end | there, then nowhere | nothing | as Alt | nothing | nothing / the draft's edges (`src/ui/input/runtime.zig:82-83`, `approval_amendment.zig:84-92`) | nothing / the field's edges (`src/ui/input/runtime.zig:162-163`) | to the composer |
| Shift, Ctrl-Shift (`visual_*`, selecting) | a row (the selection: COMPOSER-SELECTION-EXTEND) | nothing, the column forgotten | nothing | walked as the arrows | nothing | nothing / nothing | the choice (`src/ui/input/runtime.zig:184-189`) / the choice | to the composer |
| Ctrl (`visual_*`) | a row | the draft's edge, then (Up, empty) the steer, then the history | the steer, then the history | walked | nothing | nothing / nothing | nothing / nothing | to the composer |

The question's and the full transcript's columns are MODIFIED-ARROWS-SCREENS'
(its README) and unchanged here; the approval's are APPROVAL-VERTICAL-SPELLINGS'.

## The oracle (`fx/`, `captures.txt`)

Each key on its own, each followed by a letter where its effect shows, or by
a `cursor` line where one key is pressed twice to cross a paragraph (the same
key twice cannot cancel).

- `p5-paragraph-move`: one turn first, so the history holds `hello`. At an
  empty composer, Alt-, Super-, Shift-, Shift-Alt- and Ctrl-Shift-Up then a
  letter: `a`, `b`, `c`, `d`, `e` alone; Ctrl-Up recalls (`helloh`). The
  draft `one/two/<two spaces>/<empty>/three/four/<empty>/five`, the caret at
  its end: Alt-Up to `five`'s start (`Afive`); twice more, across the empty
  line, to `three`'s (`Bthree`); twice more, across the two-space line, to
  `one`'s (`Cone`); twice more and nowhere, no history (`DCone`); Alt-Down to
  `Bthree`'s start (`EBthree`), `Afive`'s (`FAfive`), the end (`FAfiveG`);
  Super-Up 0 (`SDCone`), Super-Down the end (`...GT`), Super-Up twice and no
  history (`USDCone`); Shift-Up on the first row one cell in, no edge
  (`UWSDCone`); from 0, no history (`XUWSDCone`); Ctrl-Up from the first
  row's second cell, the edge (`YXUWSDCone`). Under Ctrl-O: Alt-Down
  (`VEBthree`), Super-Up (`Z...`), Shift-Up on the first row (`ZQ...`), each
  then Escape and the letter.
- `p5-paragraph-move-menus`: `/st`'s completion (`/stats`, `/status`,
  `/statusline`): Shift-Down to `/status`, Ctrl-Down to `/statusline`, Alt-Up
  leaves the mark and puts the caret in front (`x/st`), Super-Up the same
  (`y/st`). `/model glm` and Enter, the effort column: Shift-Down marks
  `low`, Alt-Down puts the column back to the models (the hint row's model
  too), Super-Down the same. `@`: Shift-Down marks `README.md`, Alt-Up puts
  the caret in front and the list goes (`z@`). `/statusline`: Ctrl-, Shift-,
  Alt- and Super-Down leave the mark on `Context`; Down moves it to
  `Session`.
- `p5-paragraph-move-column`: `x/abcdef`, the caret at its end; Up, then
  Shift-Up on the first row, then Down: one cell into `abcdef` (`aBbcdef`),
  where with no Shift-Up between Down goes back to column 6 (`abcdefC`).

## RED (`red.txt`, `red/`, `red-pairs.txt`)

- `red.txt`: the five tests at `f9995dd5` over the base's code, 5 of 5
  failing, each at its first held key: Shift-Up at an empty composer
  recalled `first`; Alt-Up from `e|f` landed on row 3 (`9`, the plain
  arrow's row) for `10`; Super-Down walked `/st`'s menu; Alt-Up took the
  steer back; Ctrl-Down moved `/statusline`'s mark.
- `red-pairs.txt` (`pairs.sh red`, the base's binary `v0.1.0-3f2d6e7`):
  13 of 51 pairs identical -- the answered turn, the draft, Ctrl-Up at an
  empty composer, each menu before any held key, the menus' Shift- and
  Ctrl-Down (the base folded them to Down, as fx walks on them), the
  column scenario's control `up-down`, and `statusline-alt-down`, where the
  base's mark had walked three rows on Ctrl-, Shift- and Alt-Down and wrapped
  back to `Context` (the snaps before and after it differ). Every other
  held-key snap differs: at an empty composer each recalled `hello`
  (`helloa` ...); in the draft each was a row, then the edge, then the
  history (`oneD`, `tBhree`, `YhelloX`), and nine of the ten `cursor` lines
  are off fx's; `x/st` was `/stx` (the mark walked, the caret stayed); the
  effort column walked (`high`, `low`) where fx's goes back to the models;
  `/statusline`'s mark walked on Ctrl-Down (`Session`); the column
  scenario's `Babcdef` (Shift-Up went to the edge). The base's full
  transcript did not move under the held keys either (its four hashes
  alike): MODIFIED-ARROWS-SCREENS already left them to the composer.

## GREEN (`green-pairs.txt`, `green.txt`, `xfx/`)

- `green-pairs.txt` (`pairs.sh xfx`, the code head's binary `v0.1.0-45d9ad1`):
  47 of 51 identical, and every `cursor` line at fx's place. The
  four that differ are `p5-paragraph-move`'s full-transcript screens, rows
  3-25, before any key as after one: each side's record times and fx's
  longer session record (`system prompt:`, `skills:`, fourteen tools; the
  ledger's `session-record` cause). Within each side the four hash alike
  (`pairs.sh`'s last table), so no held key moved either screen, and the
  three snaps typed after Escape are identical.
- `green.txt`: the unit's twelve tests on both suites, 12 + 12 ok.

## The readers, and what changed for each

| reader | before | now | why |
|---|---|---|---|
| the decoder (`cursor`, `src/tui/input.rs`) | Alt and Ctrl alone `ComposerUp` | Alt (with Shift, Ctrl or neither) `ParagraphUp`, Ctrl alone `ComposerUp` | fx's `paragraph_up` and `visual_up` are different moves (`escape_parser.zig:83-94`) |
| the full transcript (`key`) | `Unbound` | `Unbound` (its `_`) | unchanged; fx binds no composer move |
| a question (`Act::of`) | `None` | `None` (its `_`) | unchanged |
| an approval (`decide`) | swallowed | swallowed (`ParagraphUp`/`Down` named in its list) | unchanged; the list names every held spelling |
| an approval's amendment (`approval::edits`, through `folded`) | never reached | never reached | unchanged |
| `folded` (`src/tui/shell.rs`) | every held Up/Down the arrow | none folded | fx folds none of them for any surface |
| the compact menus and a rule confirmation (`offer_to_statusline`, `_info_panel`, `_usage`, `_workspace`, `_rule_confirm`) | the folded arrow: the mark moved | the event unfolded, swallowed: nothing | `else => return` (`app_input_runtime.zig:2259-2263`), `else => {}` (`input_approval_runtime.zig:128`) |
| the pickers (`offer_to_mcp`, `_settings`, `_sessions`, `_fork`, `_catalog`, `_models`, `_effort`, `_skills`, `_column`, `_files`, `_picker`, `_provider_picker`, `_mcp_args`) | the folded arrow | Ctrl's and Shift's the arrow (`arrow_to_pickers`); Alt's and Super's no key of theirs, as Home is | `routeNonSlashPickerMove` and the slash completion see `visual_*` only (`input_completion_runtime.zig:230-259`) |
| `/model`'s column (`offer_to_column`) | Up/Down walked it | Alt and Super drop a later column, as Home does | `abandonModelPickerOptions` (`app_input_runtime.zig:307`) |
| the composer (`act`) | every one `act(Up)` | Ctrl `act(Up)`; Shift a row and no further (`move_rows(.., 1, ..)`, `forget_column`); Alt and Super the caret move with Home's | `app_input_runtime.zig:298-327` |
| the editor (`Editor::apply`) | `ComposerUp` a row | `ParagraphUp`/`Down` fx's boundaries; `ComposerUp` a row | `text_boundaries.zig:104-147` |

**The rows and the fallbacks that locked the old behaviour in**:
- the fold table (`the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`):
  its eight vertical rows said `Up`/`Down`; its eight now say each spelling
  is itself (`ParagraphUp`/`Down` in, two of the four Super rows out), and a
  second table pins `arrow_to_pickers`;
- `act`'s "never reaches here" fallback (`SelectUp | ComposerUp |
  DraftStart => act(Up)`): it is reached now, and each spelling has its own arm;
- `shift_on_the_arrows_is_the_arrow_everywhere_but_a_question` asserted that
  Shift-Up recalled; it is now `a_held_up_at_an_empty_composer_recalls_only_under_ctrl`;
- the decoder table's eleven Alt rows (`ComposerUp` to `ParagraphUp`), two
  Ctrl-Alt rows added.

## Mutants (`mutants.txt`, `mutants.py`)

In the next commit, once the run on its clone ends.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'composer*' --only 'fulltranscript*' --only
'p5-modified-arrows*' --only p5-modified-page-keys --only 'question*' --only
'p5-question*' --only 'p5-input-*' --only 'slash*' --only 'model*' --only
'p5-paragraph-move*'` at 40x120, `--jobs 2`, this branch's scenarios, once
with the base's binary (`v0.1.0-3f2d6e7`) and once with the code head's
(`v0.1.0-45d9ad1`), both in one suite-lock slot, then both rescored
(`--rescore`) with this branch's ledger once its refs were re-pointed.
56 scenarios, 399 pairs; APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py`
(unchanged, run from its folder) compares them pair by pair:

- **base 299 / 399, head 333 / 399, drops 0.**
- The 34 pairs that change are all this unit's three scenarios (unresolved
  to identical or normalized-identical): 13 of their 51 pass at the base,
  47 at the head. The other 53 scenarios' 348 pairs pass 286 on both sides,
  pair for pair (no other pair changes class, cause or uncaused count).
- The head's nine pairs with uncaused rows: `p5-input-approval`'s
  `cmd-ask`, `edit-ask` and `after`, `p5-input-paste-amend`'s `end` and
  `p5-modified-page-keys`' `full-pgup`, each with the same rows at the base
  (older than this unit); and this unit's four full-transcript screens,
  rows 36-37, two blank rows xfx's page has under a session record two
  lines shorter than fx's -- the `session-record` cause's cascade, which the
  ledger's `transcript-page-offset` cause describes ("a record shorter than
  the screen leaves xfx two more blank rows") for other scenarios. Not
  added there: that cause's `snaps = ["full-*"]` would not take the bare
  `full`, and widening it to `full` would reach other scenarios' snaps. The
  claim for those four rests on `pairs.sh`'s hashes (no held key moved
  either screen).

## Gate (`gate.txt`)

In the next commit: the gate runs on a clone of this commit.

## Refs (`refs.txt`, `refs.py`)

The ledger's refs into the files this unit changes, checked by content
against the base (`refs.py 3f2d6e78`): 11 of 16 moved and `--fix` re-pointed
them; 16 checked, 0 wrong.

## Parity (`docs/parity.md`)

- The full-screen TUI row's held Up/Down sentence is now the rule above, with
  this unit's scenarios; its Shift-arrow parenthetical, its Super-Up sentence
  and its approval decoder sentence say `ParagraphUp` and the moves; the full
  transcript row's composer clause says the same behind the screen; the
  question row names `Action::ParagraphUp` and `ComposerUp`. No row names
  COMPOSER-PARAGRAPH-MOVE as an owner.
- Two differences stay, recorded there: Shift's selection
  (COMPOSER-SELECTION-EXTEND), and a row move whose target is inside a
  collapsed paste, which fx refuses (`decideTarget`, `vertical_navigation.zig:20`)
  and xfx lands beside, as its plain arrows do.

## Decisions

- The compact menus and a rule confirmation read the event unfolded for every
  held Up and Down, Ctrl's included, where the brief said Ctrl stays plain:
  fx's do (`app_input_runtime.zig:2259-2263`, and the capture), and the brief
  said to keep a reader's behaviour unless fx differs there.
- Alt and Super reach the composer through the pickers' offers as Home does
  (no picker binds them), not around them: every picker already passes Home
  on, and fx routes `line_start`, `paragraph_*` and `draft_*` through the one
  arm (`app_input_runtime.zig:298-309`).
- Shift's row is `move_rows(.., 1, ..)`, the page key's road: the same row
  rule as the arrow's, and no edge or history by construction.
- fx's boundaries are copied over bytes, as fx reads them; a line start can
  never fall inside a collapsed paste, whose summary holds no line break.

## Open

- `folded` still runs before the compact menus for the held Left and Right
  (`ComposerLeft` to `Left`) and the held page keys (`ComposerPageUp` to
  `PageUp`), so `/statusline` flips on Shift-Left (`StatuslineAction::of`,
  `src/tui/statusline.rs`) and `/usage` changes period on it
  (`UsageAction::of_action`, `src/tui/usage_menu.rs`), where fx's compact
  menus act on `.cursor_left`/`.cursor_right` only and take a composer move
  as nothing (`app_input_runtime.zig:2228-2263`). [code, no capture] Not
  this unit's keys (held Up/Down); an owner for the horizontal and page
  halves is the dispatcher's call.
