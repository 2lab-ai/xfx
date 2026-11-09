# APPROVAL-HORIZONTAL-KEYS receipt: Left, Right, Alt-b and Alt-f at an approval's choice, and row 1 after a move

At a permission question with no amendment open, fx v0.0.16 moves the marked
choice on Left and Alt-b (previous) and Right and Alt-f (next); xfx's `decide`
bound none of the four. QUESTION-WORD-LEFT recorded the gap with a probe, and
its round-2 opus review (SHOULDs 2-3) added two more: `SelectUp`'s doc in
`src/tui/input.rs`, and row 1 after a move (fx `1  Apply once`, xfx
`1  Apply once, ab zcd`). The base is QUESTION-WORD-LEFT's head `f686b736`
(branch `fix/p5-question-word-left`, not yet on `main`).

| commit | what |
|---|---|
| `97be5968` | RED: the shell tests, the panel test, the scenario `p5-approval-horizontal-keys`, `fx/`, `red/`, `fx-src.txt`, `pairs.sh`, `red.txt` |
| `582ccb50` | the change: the decoder's `AltF`, `ComposerLeft`, `ComposerRight`; `decide`; the folds; the drafts; the tests; `docs/parity.md` |
| `c814b534` | round 1's receipt: `xfx/` at `582ccb50`, `scope.py`, `green.txt`, `probe/` |
| `1bed6e20` | one more test: the full transcript holds the composer spellings |
| `09f94a35` | **the code head**: that test takes each side on its own (mutant M14 survived round 1), and `unspelled`'s doc; `git diff --stat 582ccb50 09f94a35 -- src` is `shell.rs` alone, inside `mod tests` but for that doc comment (`gate.txt`) |
| `12848e4c`, `3a0094fe` | `xfx/`, `probe/xfx/`, `green.txt` at `09f94a35`; `mutants.py`, `mutants.txt`, the sweeps, the gate, this file; no `src/` change |
| `e22a9738` | round 2: the revocation test, mutants M15 and M16 (below) |
| `ca6fdfa0` | **round 2's code head**: comments and `docs/parity.md` only (`git diff 09f94a35 ca6fdfa0 -- src` is that test and comments) |
| the commit after | round 2's receipt: `red-r2.txt`, `mutants.txt`, `probe/` (`p5-approval-vertical-keys`), this file |

Captures: `fx/` and `probe/fx/` are fx v0.0.16 (sha256 `8e6cda2e…`, run with
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after every run); `red/`
and `probe/red/` the base's binary (row 1 `x𝒇x v0.1.0-f686b73 [debug]`,
sha256 `31d44e252f15…`); `xfx/` and `probe/xfx/`'s `p5-approval-other-keys`
the code head's (row 1 `x𝒇x v0.1.0-09f94a3 [debug]`, sha256 `5e69835e4d8d…`);
`probe/xfx/`'s `p5-approval-vertical-keys` review round 2's head (row 1
`x𝒇x v0.1.0-ca6fdfa [debug]`, sha256 `fedecd213c2f…`). No `gwlog` step, so
no request body is in the receipt.

## The GREEN bar, first

**No pair here is identical cell for cell, and none can be.** An edit's
question differs from fx before any key is pressed (`ask`, rows 19 and 21-25):
choice 2 reads `Apply + allow future edits to this file for this session`
where fx's reads `Apply + allow workspace file access for this session`, and
xfx adds two rows saying what `2` buys; the three rows under them are pushed
down two. That is the parity ledger's `approval-grant-scope` cause
(`scripts/fxcap/parity-ledger.toml`), a decision the user has not made, which
no unit can resolve. So this receipt measures two ways:

- `scope.py`: each pair cell by cell, styles included, with the rows that
  cause's own regexes match set aside on each side (choice 2's row compared up
  to its words, so its marker and indent still count). It also leaves out
  row 1, the banner (`scope.py:39`), and the blank rows at the bottom
  (`:48-49`), which `cells.py` compares as rows; nothing else is set aside, and
  `pairs.sh` below shows the rows that differ with nothing set aside are only
  19 and 21-25 (`answered`: 20 and 22-26). **29 of 29 at the head**
  (`green.txt`), 3 of 29 at the base (`red.txt`: `ask`, `amend-alt-b`,
  `amend-keys`).
- `pairs.sh`: nothing set aside, `cells.py` as the dispatcher re-runs it. At
  the head 28 of 29 pairs differ in exactly `ask`'s six rows and `answered` in
  the next question's six (rows 20, 22-26: the write's grant scope, one row
  lower under the `┃ aby zcd` card); at the base 26 of 29 differ beyond them.
- The sweep (below) puts every pair of the scenario under
  `approval-grant-scope` alone with no uncaused row at the head.

## What fx v0.0.16 does (`fx-src.txt`, every line read with `git show 481104fd:<path>`)

- **Which actions move the choice.** `routeApprovalEscapeAction`
  (`input_approval_runtime.zig:71-130`): with no amendment open,
  `.cursor_left, .word_left` apply `move_choice = .previous` and
  `.cursor_right, .word_right` `.next` (`:90-110`); Up and Down the same, open
  amendment or not (`:111-126`); everything else is `else => {}` (`:128`).
- **It wraps at both ends.** `moveChoice` (`approval_decision.zig:214-225`):
  below 0 is the last choice, past the last is 0; then `clearActive` ends the
  amendment. fx's own test walks `previous` from choice 0 to 2 (`:249-271`).
- **What the four actions are.** `.cursor_left/right` is the bare arrow in every
  encoding (`plainNavigationAction`, `escape_parser.zig:146-167`; SS3 and the
  kitty keypad resolve through it). `.word_left/right` is Alt-b/Alt-f alone:
  `ESC b`/`ESC f` (`:528-539`) and the keycode table, kitty's `CSI 98;3u`/
  `CSI 102;3u` and xterm's `CSI 27;3;98~`/`CSI 27;3;102~` (`:243-250`).
- **What leaves the choice alone.** Ctrl-, Alt-, Shift- and Super-Left and
  -Right are `composerMove` (`modifiedArrowAction`, `:73-108`), a
  `.composer_shortcut` action the switch has no arm for; so are Shift-Alt-b
  and Shift-Alt-f (`:244`, `:248`) and `ESC ESC [ D` (the meta prefix,
  `:552-555`, `:561-568`, `:83-88`). Ctrl-B and Ctrl-F are raw bytes: at a question only
  `approvalActionFromRawByte` reads them (`src/ui/input/runtime.zig:103-108`,
  `app_input_runtime.zig:1161-1185`), as `.edit_draft = .cursor_left/right`,
  which `apply` answers with `.none` when no amendment is open
  (`approval_decision.zig:170-174`).
- **In an amendment** every one of them is the draft's: the decoded action's
  composer shortcut becomes `focused_edit` (`approvalDraftActionFromShortcut`,
  `src/ui/input/runtime.zig:75-96`; `withApprovalInput`, `:110-133`), applied
  before the switch (`input_approval_runtime.zig:79-89`) -- the arrow and
  Shift-arrow and Ctrl-B/F the step, Alt-b/f and the Ctrl/Alt arrows the word
  move, Super-arrows `cursor_home`/`cursor_end` (`:82-83`).
- **Row 1 after a move.** A choice's label carries its draft only while that
  choice's amendment is open: `appendFileApprovalChoiceLabel` checks
  `amendmentChoice() == choice_index` (`approval_ui.zig:1181-1193`), as the
  panel row (`:1413-1418`) and the compact label (`:1688-1694`) do. Moving the
  choice clears only `active_choice` (`approval_amendment.zig:170-172`), so the
  text is kept: Tab on that choice opens it again with its text
  (`approval_decision.zig:142-151`, `begin` at `approval_amendment.zig:20-22`),
  and Enter sends the marked choice's draft whether or not it is open
  (`materializeResponse`, `approval_decision.zig:112-127`); fx's test "preserves
  independent drafts while choices move" (`:285-303`).
- **fx's screens agree** (`fx/`): from choice 2, Left 1, Left 3 (wrap), Right 1
  (wrap), Right 2, Alt-f 3, Alt-b 2, kitty Alt-f 3, xterm Alt-f 1, kitty Alt-b
  3, xterm Alt-b 2; then Ctrl-, Alt-, Shift- and Super-Right and -Left, Ctrl-F,
  Ctrl-B and Shift-Alt-f all leave 2. In the amendment `ab cd`, Alt-b, `z`,
  Shift-Left, Ctrl-B, `y` gives `aby zcd`; after Down row 1 is `1  Apply once`;
  Tab back on 1 shows `Apply once, aby zcd`; Down, Up and Enter answer with the
  card `┃ aby zcd` (`answered`).

## What xfx did at the base

- `decide` (`src/tui/shell.rs`) bound Up, Down, `C-p`, `C-n` and the panel's
  keys, the draft's keys while one was open, and swallowed everything else, the
  four horizontal keys included (`Input::Action(_) | Input::PasteByte(_) =>
  return`). It read the event **after** the shell's fold, so Alt-b reached it
  as `WordLeft`.
- The decoder gave Alt-f, Ctrl-Right and Alt-Right one action, `WordRight`,
  and gave Left to Shift-Left, Super-Left, the kitty keypad Left with either,
  and Ctrl-B (`cursor`'s "Shift is dropped"; `control`'s `0x02`); the same on
  the right. Binding Left alone would have moved the choice on all of those.
- `Drafts::blocks` (`src/tui/approval_amendment.rs`) painted a draft while it
  had the keys **or held text**, on the reasoning that a sentence that still
  travels should be shown. `docs/parity.md` did not record it: a silent
  divergence, the one QUESTION-WORD-LEFT's probe showed as row 1.

## The change

- **The decoder** (`src/tui/input.rs`): `Action::AltF` for `ESC f`,
  `CSI 102;3u` and `CSI 27;3;102~`, as `AltB` is for Alt-b; Shift-Alt-f stays
  `WordRight`. `Action::ComposerLeft`/`ComposerRight` for Left/Right with Shift
  or Super and no Ctrl or Alt (`cursor`, which the kitty keypad goes through
  too) and for `0x02`/`0x06` (`control`, which kitty's `CSI 98;5u` reaches):
  fx's composer moves and raw bytes, named for what they are to fx.
- **The fold, and the choice made: the approval reads the event before any
  fold**, the full transcript and the question read it `unspelled`, and every
  other surface reads it `folded` (`src/tui/shell.rs`). Why this and not "the
  fold skips approvals": `decide` already translates every action into the
  panel's own vocabulary (`approval::Action`), so it is the place that can name
  each spelling, and it needs the folded form only for its draft (it calls
  `folded` there); `unspelled` keeps the full transcript (which holds Left and
  Right) and the question (whose Shift-Left and Ctrl-B are
  QUESTION-LEFT-SPELLINGS' to change) on exactly the actions they had. The
  three surfaces are never up together (`Self::slot`, `full_transcript_sees`),
  so the order of their branches is unchanged.
- **`decide`**: `Left | AltB` and `Right | AltF` with no draft open are
  `approval::Action::Up`/`Down`, the move Up and Down already make (wrapping,
  ending editing); `SelectUp`/`SelectDown` stay Up/Down as the fold made them
  before. In a draft each key is translated through `folded` before
  `approval::edits`, so every spelling is the caret's move.
- **Not letting xfx's Shift-Left move the choice** where fx's does not:
  Shift-, Super-Left and Ctrl-B are `ComposerLeft`, which `decide` sends to its
  swallow at a choice; the same for Ctrl- and Alt-Left (`WordLeft`) and
  Shift-Alt-b (`WordLeft`). Pinned by the shell test below and mutants M4-M7.
- **Row 1**: `Drafts::blocks` paints only the draft that has the keys, as fx
  does. The text is kept, Tab paints it again, and Enter on its choice sends
  it, as fx's does. Leaving a filled draft now reshapes the panel, so it
  revokes a readiness receipt as opening one does; the next disclosed frame
  re-earns it (the scenario's `unpainted` -> `answered` shows Enter accepted).
- **The comment** (brief item 4): `SelectUp`'s doc no longer says the fold runs
  "before any other surface sees them"; it says the approval takes them as the
  arrows and the fold runs once the full transcript and the question have read
  the event. `AltB`'s doc and `folded`'s say which two surfaces read it.
- `docs/parity.md`, the full-screen TUI row: QUESTION-WORD-LEFT's sentence
  ("Left, Right, Alt-b and Alt-f move nothing at a choice …", owner this unit)
  is replaced by what xfx does now, with the fx lines; the amendment sentence
  says only the draft being typed is painted; the `ask_user_question` row's
  fold clause names the approval. Recorded there as **not changed** (below):
  `ESC ESC [ D`, and the vertical spellings.

## RED (`red.txt`, at the base `f686b736`)

- `tui::shell::tests::the_horizontal_keys_at_an_approval_choice_are_fxs`:
  `[27, 91, 68] did not mark choice 1` (Left from choice 2).
- `tui::shell::tests::a_draft_left_behind_is_not_painted_and_still_travels_with_its_choice`:
  row 1 after Down was `    1  Apply once, ab cd`, wanted `    1  Apply once`.
- `tui::approval::tests::moving_the_choice_leaves_editing_and_keeps_both_drafts`
  (its painting assertion now fx's): Up out of a filled draft was `Moved`,
  wanted `Reshaped`.
- The pair: 26 of 29 differ beyond `ask`'s rows; `scope.py`: 3 of 29. At the
  base the marker stays on 2 through every key, row 1 keeps `aby zcd`, Tab on
  2 cycles to 3, and the closing Enter refuses the edit (`1 denied`).

## GREEN (`green.txt`, at the code head `09f94a35`)

- The pair as above: `scope.py` 29 of 29, `pairs.sh` 28 + 1 at the scope rows.
- The tests at the head, 166 passed, among them the three RED tests and:
  - `alt_b_in_an_approval_amendment_is_the_word_move`, now every spelling in
    the draft: `ab cd`, Alt-b, `z`, Shift-Left, Ctrl-B, `y`, Ctrl-F,
    Shift-Right, `x`, Alt-f, `w`, Left, Left, `v` sends `aby zxcvdw` with
    `Once`;
  - `alt_f_and_the_composer_spellings_of_the_arrows_are_their_own_actions`
    (the decoder, 24 spellings);
  - `the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`
    (`folded` and `unspelled`);
  - `the_full_transcript_holds_the_composer_spellings_of_the_arrows_as_it_holds_the_arrows`;
  - `alt_b_at_a_choice_goes_back_a_question_and_the_word_arrows_do_not`, now
    also Shift-Left and Alt-f in the question's draft (`ab yzcdvxw`);
  - `up_and_down_move_the_choice_whether_or_not_a_draft_is_open` and
    `the_arrows_leave_a_draft_on_the_review_plane_even_though_c_p_scrolls_it`,
    which read the kept draft after Tab paints it again.

## Not changed, shown by `probe/` (`probe/probe.txt`, `probe/vertical.txt`)

`probe/p5-approval-other-keys.cap` (round 1) and
`probe/p5-approval-vertical-keys.cap` (round 2) are not parity scenarios; each
is captured on fx, the base and the head (`probe/fx`, `probe/red`,
`probe/xfx`). At an edit's question:

- **`ESC ESC [ D` and `ESC ESC [ A` cancel the turn** on xfx (`▪ Cancelled
  file`) and leave the choice on fx: xfx's decoder has no meta prefix and
  reads Escape, then the arrow (`kitty_key`'s doc). Owner
  QUESTION-LEFT-SPELLINGS (proposed), with the question's.
- **The vertical spellings: eight keys at a choice, and the amendment side.**
  From choice 1 with no amendment open, Shift-, Ctrl-, Super- and Alt-Down,
  kitty's Alt-Down (`CSI 57353;3u`) and `C-n`, then the same six Up spellings
  (kitty's `CSI 57352;3u`) and `C-p`, each move xfx's choice one step
  (wrapping); fx v0.0.16 leaves it on 1 for every one of the twelve
  (`vertical.txt`; round 1's `probe.txt` has seven of them, Super-Up and the
  Alt spellings added in round 2). The modified arrows are `composerMove`
  (`escape_parser.zig:73-108`, `:208-217` for kitty's), which reaches the
  switch's `else` (`input_approval_runtime.zig:128`); the raw route at a
  question reads no `C-p`/`C-n` (`src/ui/input/runtime.zig:61-73`,
  `:103-108`), while `.cursor_up`/`.history_up` still move (`:111-126`).
  **In an amendment** fx keeps it open: Shift-Up and `C-p` do nothing and
  Super-Up moves the caret to the start (`approvalDraftActionFromShortcut`,
  `src/ui/input/runtime.zig:79-87`, which maps `draft_start` and no
  `visual_up`): `ab cd`, Shift-Up, `x`, Super-Up, `y`, `C-p`, `w` is
  `ywab cdx` on fx, while xfx's end the amendment and move the choice
  (`amend-*`). xfx's `decide` names this (`C-p` and `C-n`). Owner unit:
  **APPROVAL-VERTICAL-SPELLINGS** (queued); this unit changes none of them:
  the marker lands where the base's does after every key, and only row 1
  differs, the base painting the draft left behind (this unit's row 1).
- **Super-Left and -Right** jump to the line's edge in fx's amendment
  (`src/ui/input/runtime.zig:82-83`) and composer (`escape_parser.zig:74-82`),
  and to the draft's start and end for Super-Up and -Down; xfx's draft and
  composer take Super-Left/Right as a step and Super-Up/Down as the arrow.
  `docs/parity.md` records the composer half on the key-report sentence and
  the amendment half on the approval sentence. Owner unit:
  **COMPOSER-SUPER-ARROWS** (queued).

## Mutants (`mutants.py`, `mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway of the branch (its remote
removed) under `$TMPDIR/mut/`, one at a time, the file restored after each,
under the suite lock; per mutant `cargo test --locked --lib -- approval
amendment alt_b alt_f the_fold the_full_transcript_holds horizontal draft_left
up_and_down_move the_arrows_leave question:: input::tests`, 900 s each.

- **Round 1, at `1bed6e20`: 13 of 14** (`mutants.txt`, kept below round 2).
  M14 survived: the full-transcript test sent three lefts and three rights, so
  the ones that reached the composer cancelled out. `09f94a35` takes each side
  from the end it would move away from.
- **Round 2, at `09f94a35`: 14 of 14 killed**, the clone's tree clean after the run.
- **Review round 2, at `ca6fdfa0`: 16 of 16 killed** (on top of `mutants.txt`), the clone's tree clean.
  M15 (a reshape does not revoke the receipt) is killed by the new revocation
  test and by `opening_a_draft_revokes_a_committed_readiness_receipt`; M16 (an
  arrow out of a draft is not a reshape) by the new test and the panel's
  `moving_the_choice_leaves_editing_and_keeps_both_drafts`.

| mutant | killed by (round 2) |
|---|---|
| M1 Left and Right at a choice back to nothing (the base) | the horizontal-keys shell test |
| M2 `ESC f` decoded as Ctrl-Right (`WordRight`) again | the decoder tests, the horizontal-keys test |
| M3 kitty/xterm Alt-f decoded as Ctrl-Right again | the decoder test, the herdr table, the horizontal-keys test |
| M4 Shift-Left/Right decode as the arrows (and move the choice) | the decoder tests, the horizontal-keys test |
| M5 Super-Left/Right decode as the arrows | the decoder test, the horizontal-keys test |
| M6 Ctrl-B decodes as Left again | the control-byte tables, the decoder test |
| M7 Ctrl-Left (the word arrow) moves the choice too | the horizontal-keys test |
| M8 Up/Left stop at the first choice where fx wraps | the panel's wrap test, the horizontal-keys test |
| M9 Down/Right stop at the last where fx wraps | the same |
| M10 row 1: a draft left behind is painted again (the base's rule) | the panel test, the shell row-1 test, the review-plane test |
| M11 a draft left behind does not travel with its choice | the panel test, the shell row-1 test |
| M12 an approval's draft reads the spellings unfolded | the amendment test |
| M13 the question reads the approval's spellings unfolded | the question's word-left test |
| M14 the full transcript reads them unfolded | round 2's full-transcript test |
| M15 a reshape does not revoke the readiness receipt (review round 2) | the revocation test, the Tab-open revocation test |
| M16 an arrow out of a draft is not a reshape (review round 2) | the revocation test, the panel test |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only '*approval*' --only '*question*' --only
filepicker-keys --only composer-keys` at 40x120, `--jobs 4`, this branch's
scenarios and ledger, once with the base's binary and once with the head's
(row 1 `v0.1.0-09f94a3`, sha256 `5e69835e…`), both in one suite-lock slot.
`filepicker-keys` and `composer-keys` are the scenarios that send Alt-b,
Alt-f and the modified arrows to the composer and the `@` list: a fold that
leaked would show there. `cmp_sweeps.py` (paste-after-end's, unchanged)
compares them pair by pair (`sweep-compare.txt`):

- **base 82 / 139, head 82 / 139, drops 0.** No pair changed class.
- `p5-approval-horizontal-keys`: all 29 pairs stay `unresolved-defect` under
  `approval-grant-scope` alone (no unit can pass them); the rows no cause
  covers go to **0 in every one** at the head, from 2-6 in 26 of them at the
  base (`ask`, `amend-alt-b`, `amend-keys` had 0 at both).
- The 5 pairs elsewhere with uncaused rows are the same 5, with the same
  counts, at both (`p5-input-approval` `after`, `cmd-ask`, `edit-ask`;
  `p5-transcript-question` `asked`, `narrow`); this unit touches neither
  scenario.
- `composer-keys` 8 / 11 and `filepicker-keys` 7 / 7 at both.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at
`12848e4c`, whose `src/`, `tests/`, `Cargo.toml` and `Cargo.lock` are the code
head `09f94a35`'s (`gate.txt` carries the empty `git diff --stat`). Every step
exits 0 and the tree was the same at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5548 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5585 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5548 is QUESTION-WORD-LEFT's round-2 gate (5544) plus this unit's four new
tests. No test here builds a `Runtime`: the shell cases use the shell fixture,
the others the panel, the drafts, the decoder and the folds alone. Every
commit's added lines were scanned for local paths before it was made.

## Round 2: the reviews of `3a0094fe`

gpt6 (one MUST) and opus (MUST none, five SHOULDs), with the dispatcher's
checks. `xfx/` and `green.txt` stay at `09f94a35`: round 2 changes a test,
comments and docs, so the scenario's binary is the same code.

- **MUST: leaving a filled draft revokes the readiness receipt, and nothing
  pinned it** (the row-1 test calls `delivered` before Enter). New shell test
  `leaving_a_filled_draft_revokes_the_receipt_and_the_next_frame_earns_it_back`:
  the receipt is earned with `ab cd` painted on choice 1; Down and Up leave
  the draft; the receipt is gone, an Enter sends nothing and the question
  stays up; after the next delivered frame Enter answers `Once` with
  `ab cd`. **RED (`red-r2.txt`)**, on a throwaway clone at `e22a9738`
  (`3a0094fe` plus the test): it passes as committed, and fails with the
  revocation turned off either way -- M15 (a reshape does not call
  `invalidate_approval`) and M16 (an arrow out of a draft is not a reshape) --
  at `leaving a filled draft kept a receipt about the screen that showed it`,
  the assertion before the Enter.
- **Mutants:** M15 (a `Reshaped` reply does not call `invalidate_approval`)
  and M16 (an arrow out of a draft is not measured as a reshape) join the
  set; at `ca6fdfa0` all 16 are killed, and the new test kills both (`mutants.txt`).
- **The wording** (opus 1-5, gpt6's NTH): the vertical item above, now with
  Super-Up and the Alt spellings probed and the amendment side, owner
  APPROVAL-VERTICAL-SPELLINGS; Super-Left/Right owner COMPOSER-SUPER-ARROWS
  with the composer half in `docs/parity.md`; `scope.py`'s row 1 and blank
  rows (the GREEN bar); `decide`'s comment on Ctrl-B/F (fx's raw route,
  `apply` answering `.none`, never `:128`) and on `C-p`/`C-n`;
  `MAX_DRAFT_ROWS`' "both drafts"; and the `ask_user_question` row's cause for
  Shift-Left and Ctrl-B, which reach the question as Left through the shell's
  `unspelled` now.
- **Checks at `ca6fdfa0`:** the unit's tests 168 passed (the round-1 set plus
  the new test, with `opening_a_draft_revokes_a_committed_readiness_receipt`);
  `cargo test --locked --test parity` 26 passed; `cargo fmt --check` and both
  clippy runs clean. The full gate is the dispatcher's.

## Decisions (each reversible)

- **The approval reads before the fold**, not "the fold skips approvals": one
  translation table names every spelling, and the surfaces that never told
  them apart keep their actions exactly (reason above).
- **Row 1 copies fx whole**: the draft left behind is hidden, kept, painted
  again by Tab, and sent with its choice. The old rule ("shown while it holds
  text") was a safety argument recorded nowhere in `docs/parity.md`; the
  sentence is the user's own and context, never authority. Reverting is one
  condition in `Drafts::blocks` (mutant M10).
- **Two new decoder actions for the composer moves** rather than one per key:
  the approval's distinction is binary (fx's cursor key or not), and the names
  say what the keys are to fx.
