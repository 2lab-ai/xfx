# APPROVAL-VERTICAL-SPELLINGS receipt: the modified Up and Down, `C-p` and `C-n` at an approval

At a permission question fx v0.0.16 moves the marked choice on the plain
arrows alone. Shift-, Ctrl-, Super- and Alt-Up and -Down (Alt also as
kitty's `CSI 57352;3u`/`57353;3u`) and `C-p` and `C-n` leave the choice
where it is, and in an amendment they leave the amendment open: Super-Up and
-Down take its caret to the draft's start and end, the other ten do nothing.
xfx moved the choice one step on all twelve and ended the amendment.
APPROVAL-HORIZONTAL-KEYS recorded the gap with a probe (its `probe/`); its
head `2593f49c` (branch `fix/p5-approval-horizontal-keys`, not yet on
`main`) is this unit's base.

| commit | what |
|---|---|
| `fc33fc86` | RED: the four shell tests, the scenario `p5-approval-vertical-keys`, `fx/`, `red/`, `fx_src.sh`, `fx-src.txt`, `pairs.sh`, `scope.py`, `red.txt` |
| `ed5ab8b9` | **the code head**: the decoder, the fold, `decide`, the editor, the decoder and fold tests, `docs/parity.md`, the ledger's moved refs |
| `92d087df` | the paste finding in `docs/parity.md`, a test comment and a doc's file name (comments only in `src/`), `fx_src.sh`'s check reading the piped lines; **the gate ran here** |
| `eb381074` | **the head's tests**: one more case, Up first out of a draft (mutant M13 survived round 1); tests only |
| the commit after | the receipt: `xfx/`, `green.txt`, `mutants.py`, `mutants.txt`, the sweeps, `gate.txt`, this file; no `src/` change |

Captures: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`;
`capture.sh` checked its bytes after the run); `red/` the base's binary
(row 1 `x𝒇x v0.1.0-2593f49 [debug]`, sha256 `cdcbaa2b457b…`, built at
`2593f49c` before any change but the tests); `xfx/` the code head's (row 1
`x𝒇x v0.1.0-ed5ab8b [debug]`, sha256 `a398ecbd8718…`). No `gwlog` step, so
no request body is in the receipt.

## The GREEN bar, first

As in APPROVAL-HORIZONTAL-KEYS, **no pair of an edit's question is identical
cell for cell, and none can be**: before any key, in `ask`, choice 2 reads
`Apply + allow future edits to this file for this session` where fx's reads
`Apply + allow workspace file access for this session`, and xfx adds two rows
saying what `2` buys -- the ledger's `approval-grant-scope` cause
(`scripts/fxcap/parity-ledger.toml`), a user decision no unit can make. So:

- `scope.py`: each pair cell by cell, styles included, with only that
  cause's rows set aside on each side (choice 2's row compared up to its
  words). **27 of 27 identical at the code head** (`green.txt`), 3 of 27 at
  the base (`red.txt`). The amendment text is in the compared rows: every
  `amend-*` snap's row `❯ 1  Apply once, <draft>`, and `answered`'s card
  `┃ ywvutsrqpoab cdxn`.
- `pairs.sh`: nothing set aside but row 1, the banner, which `cells.py` never
  compares. At the head 26 of 27 pairs differ in exactly `ask`'s rows
  (19, 21-25), and `answered` in the next question's (20, 22-26: the write's
  grant scope, one row lower under the card); at the base 24 of 27 differ
  beyond them.

## What fx v0.0.16 does (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The route.** A key the parser decodes reaches the approval as
`routeApprovalEscapeAction(app, action, approval_focused_edit)`
(`app_input_runtime.zig:964-971`). The action carries its composer shortcut
(`appendAction`, `terminal_action_decoder.zig:214-224`; `fromEscapeAction`,
`shortcuts.zig:43-65`: `.cursor_up` is `move(.visual_up)`, `:45`, a
`.composer_shortcut` is itself, `:61`), and `withApprovalInput` makes the
focused edit of it (`src/ui/input/runtime.zig:121-129`) with
`approvalDraftActionFromShortcut` (`:75-96`): `.line_start, .draft_start`
are `.cursor_home` (`:82`), `.line_end, .draft_end` `.cursor_end` (`:83`),
the other moves nothing (`:86`). In `routeApprovalEscapeAction`
(`input_approval_runtime.zig:71-130`), an open amendment with a focused edit
takes it (`:79-89`); otherwise `.history_up, .cursor_up` and their Down move
the choice (`:111-126`), and everything else is `else => {}` (`:128`). A raw
byte goes `handleRawTerminalInputWithLimits` (`app_input_runtime.zig:840-894`)
to `routeActiveModalInput` (`:1126-1185`), where the approval takes it and
does only `raw.approval_action`, if there is one (`:1173-1184`).

**What the twelve are.** The CSI modifier is one more than the mask
(`escape_parser.zig:722`, bits `:61-64`), and kitty's report the same
(`:689`); kitty's arrow codes (`:38-39`) take the main-row table
(`:208-214`). That table, `modifiedArrowAction` (`:73-108`), reads Super
first, then Alt, then Ctrl, then Shift.

| spelling | bytes | fx decodes it | at a choice | in an amendment |
|---|---|---|---|---|
| Shift-Up / -Down | `CSI 1;2A` / `B` | `visual_up`/`_down`, extending (`escape_parser.zig:99-101`) | nothing (`input_approval_runtime.zig:128`) | nothing (`src/ui/input/runtime.zig:86`, then `input_approval_runtime.zig:128`) |
| Ctrl-Up / -Down | `CSI 1;5A` / `B` | `visual_up`/`_down` (`escape_parser.zig:90`, `:93-94`) | nothing | nothing |
| Alt-Up / -Down | `CSI 1;3A` / `B` | `paragraph_up`/`_down` (`escape_parser.zig:83-85`) | nothing | nothing |
| kitty Alt-Up / -Down | `CSI 57352;3u` / `57353;3u` | the same, through `escape_parser.zig:208-214` | nothing | nothing |
| Super-Up / -Down | `CSI 1;9A` / `B` | `draft_start`/`draft_end` (`escape_parser.zig:74-76`) | nothing | the caret to 0 / to the draft's length (`.cursor_home`/`.cursor_end`, `src/ui/input/runtime.zig:82-83`; `apply`'s `.edit_draft`, `approval_decision.zig:170-174`; `applyDraftAction`, `approval_decision.zig:231-246`; `cursorHome`/`cursorEnd`, `approval_amendment.zig:84-92`) |
| `C-p` | `0x10` | a raw byte with no shortcut (`appendRaw`, `terminal_action_decoder.zig:207-212`; `fromControlByte`, `shortcuts.zig:11-29`) | nothing: `approvalActionFromRawByte` (`src/ui/input/runtime.zig:103-108`) makes no action of it (`approvalActionFromByte`, `src/ui/input/runtime.zig:61-73`), so `app_input_runtime.zig:1173-1184` do nothing | nothing, the same |
| `C-n` | `0x0e` | a raw byte, `.history_next` (`shortcuts.zig:17`), which `fromFocusedEditorControlByte` drops (`shortcuts.zig:31-41`; fx's test lists 14 and 16 as excluded, `shortcuts.zig:101-104`) | nothing, the same | nothing, the same |

- **Plain Up and Down still move**, open amendment or not: `.cursor_up` is
  `visual_up` to the draft, which it does not name, so it reaches
  `input_approval_runtime.zig:111-126`, `move_choice` (`apply`,
  `approval_decision.zig:166-169`), which wraps and ends the amendment
  (`moveChoice`, `approval_decision.zig:214-225`); fx's own test drives
  `.history_up` and `.cursor_left` this way
  (`app_input_runtime.zig:5931-5961`). Nothing at the pin makes a
  `.history_up` or `.history_down` from a key: the enum declares them
  (`input_action.zig:71-72`) and no decoder emits either (`fx-src.txt`'s
  last section, a `git grep` of `src/ui` and `src/core/input`); the
  composer's switch reads them (`app_input_runtime.zig:1004-1012`).
- **fx's screens agree** (`fx/`): in the amendment `ab cd`, Shift-Up `x`
  gives `ab cdx`, Super-Up `y` `yab cdx`, `C-p` `w` `ywab cdx` (the probe's
  example), then Shift-Down `v`, Ctrl-Up `u`, Ctrl-Down `t`, Alt-Up `s`,
  Alt-Down `r`, kitty Alt-Up `q`, kitty Alt-Down `p` and `C-n` `o` each type
  at the caret Super-Up left, and Super-Down `n` at the end:
  `ywvutsrqpoab cdxn`, choice 1 marked throughout. Down and Up back to
  choice 1 (`choice-1`), then each of the twelve leaves `❯ 1  Apply once`.
  Enter answers with the card `┃ ywvutsrqpoab cdxn` (`answered`).

## What xfx did at the base

- The decoder gave Ctrl-, Alt- and Super-Up and -Down (kitty's and the
  keypad's included) the plain `Up`/`Down`, and Shift-Up/-Down
  `SelectUp`/`SelectDown` (`cursor` in `src/tui/input.rs`).
- `decide` (`src/tui/shell.rs`) mapped `Up | SelectUp` and `Down | SelectDown`
  to the panel's `Up`/`Down`, and `ControlP`/`HistoryNext` (with no review
  plane up) to the same: every one of the twelve moved the choice and ended
  an open draft.

## The change (`git diff 2593f49c ed5ab8b9 -- src`: +168 / -55 outside the test modules, most of it comments)

- **The decoder** keeps every modifier on Up and Down apart from the arrow,
  in fx's order (`cursor`): Super first, as `Action::DraftStart { shifted }`
  / `DraftEnd { shifted }`; then Shift, as `SelectUp`/`SelectDown`; then Ctrl
  or Alt, as `ComposerUp`/`ComposerDown`; no modifier the table reads, the
  arrow. Kitty's arrows and the keypad's go through `cursor` too.
- **The fold** (`unspelled`, which `folded` calls) gives every surface but
  the approval exactly the key it had: `ComposerUp` and `DraftStart
  { shifted: false }` are `Up`, `DraftStart { shifted: true }` is `SelectUp`
  (Super-Shift-Up was `SelectUp`, the key a question's field binds), and the
  same on Down. The fold test pins each.
- **`decide`**: `Up` and `Down` move the choice, draft or not, as before.
  `DraftStart`/`DraftEnd` while a draft is open are `Edit` -- the editor
  reads them as the start and end of the whole text (`Editor::apply`), fx's
  `cursorHome`/`cursorEnd`. `SelectUp`/`SelectDown`, `ComposerUp`/`Down`,
  `DraftStart`/`End` with no draft, `ControlP` and `HistoryNext` are
  swallowed, with or without a draft, before the panel is asked. On the
  review plane `C-p`/`C-n` still walk the change (unchanged; checked before
  the translation). `approval::edits` names `DraftStart`/`DraftEnd`.
- **Read before the fold, the choice made**: the approval keeps reading the
  event before any fold (`spelled`), as APPROVAL-HORIZONTAL-KEYS chose for
  Alt-b and Alt-f. Why, here: the fold is exactly what erases this unit's
  distinctions -- after it Ctrl-Up, Super-Up and Shift-Up are `Up` again --
  so a `decide` after the fold could not tell them from the arrow; and
  `decide` already names every spelling in the panel's own vocabulary,
  folding only for its draft. Reading after the fold would need the fold to
  skip approvals, a second place that knows which surface is up.
- `docs/parity.md`, the full-screen TUI row: the approval sentence that
  named this unit as owner now says what xfx does, with fx's lines; the
  amendment sentence says the arrows end editing and the rest leave both
  drafts; the composer's Super sentence names `DraftStart`/`DraftEnd`.
  Retired: the "Not changed" clause for the vertical spellings. Kept: `ESC
  ESC [ A`/`[ B`/`[ D` (owner QUESTION-LEFT-SPELLINGS). Added: the paste
  finding below.
- The ledger: four `src/tui/shell.rs` refs moved by the change, re-pointed
  to the lines they cite (`10277` to `10300`, `13498` to `13525`, `16413` to
  `16452`, `39046` to `39347`, mapped line for line against the base);
  `ledger.load(..., repo_root)` accepts the file.

## Approval safety

- **A key that leaves the choice and the amendment as they were does not
  touch the readiness receipt**: it is swallowed before the panel is asked,
  so there is no `Reply`, no frame and no `invalidate`. The new test
  `a_no_op_vertical_spelling_keeps_the_receipt_and_enter_answers_the_choice_shown`
  earns a receipt, sends each spelling (all twelve at a choice; the ten that
  do nothing in an amendment), compares `Readiness` (its `Debug`) before and
  after, and sends Enter in the same read with no frame between: it answers
  `Once`, with `ab cd` when the draft was open. **RED at the base**:
  Shift-Down then Enter answered `Always` under a frame that marked
  `Apply once` (`red.txt`); the base's `Moved` keeps the receipt, so the
  marker's move was answerable before any frame showed it.
- **A key that still changes what the card shows still revokes it**, as
  `invalidate_approval` does today: the plain arrow out of a filled draft
  (`Reshaped`) -- the same test's last case, and the existing
  `leaving_a_filled_draft_revokes_the_receipt_and_the_next_frame_earns_it_back`
  and `opening_a_draft_revokes_a_committed_readiness_receipt`. Super-Up and
  -Down in a draft move the caret, `Moved`, as Home and Left there do today.
- No new string reaches the terminal, and nothing writes a file, runs a
  process or reaches the network.

## RED (`red.txt`, at the base `2593f49c` with only the tests added)

- `the_vertical_spellings_at_an_approval_choice_move_nothing_and_the_arrows_still_do`:
  `shift-down ([27, 91, 49, 59, 50, 66]) moved the choice: "  ❯ 2  Apply + allow …"`.
- `the_vertical_spellings_in_an_approval_amendment_keep_it_open_as_fxs_do`:
  `shift-up ([27, 91, 49, 59, 50, 65]) ended the amendment`.
- `a_no_op_vertical_spelling_keeps_the_receipt_and_enter_answers_the_choice_shown`:
  `open=false shift-down: Enter did not answer the choice the last frame
  showed`, `Always` where `Once` was wanted.
- `up_and_down_move_the_choice_whether_or_not_a_draft_is_open` (rewritten to
  fx's rule): `open=false ControlP: the marker moved`.
- The pair: 24 of 27 differ beyond `ask`'s rows; `scope.py` 3 of 27. The
  scenario alternates Down and Up after the first two keys at the choice, so
  a key that moves leaves the base's marker on 2 or 3 at every snap but the
  last (`ctrl-p`: six steps each way come back to 1).

## GREEN (`green.txt`, at the code head `ed5ab8b9`)

- The pair as above: `scope.py` 27 of 27, `pairs.sh` 26 at the floor and
  `answered` at the next question's grant-scope rows.
- The unit's tests and their neighbours, 404 passed (`green.txt`), among
  them the four RED tests and:
  - `every_modifier_on_the_vertical_arrows_is_kept_apart_from_the_arrow_in_upstreams_order`
    (the decoder: the twelve, two modifiers at once, kitty's and the
    keypad's arrows, and the arrow with no modifier);
  - `super_up_and_down_are_the_whole_drafts_edges` (`approval_amendment`: a
    draft holding a line break, Shift held or not);
  - `the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`,
    with the six new actions;
  - `the_keypad_decodes_as_the_main_row_it_mirrors` and
    `a_sequence_is_a_key_only_in_the_exact_shapes_a_terminal_sends_one`,
    whose Ctrl-Up/Down now expect `ComposerUp`/`ComposerDown`.

## Mutants (`mutants.py`, `mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway of the branch (its remote
removed) under `$TMPDIR/mut/`, one at a time, the file restored after each,
under the suite lock; per mutant `cargo test --locked --lib -- approval
amendment alt_b alt_f the_fold the_full_transcript_holds horizontal
draft_left up_and_down_move the_arrows_leave leaving_a_filled_draft revokes
vertical super_up_and_down question:: input::tests editor::tests`, 900 s
each.

- **Round 1, at `ed5ab8b9`: 12 of 13.** M13 survived: every test that left
  a draft did it with Down first, so an Up that stayed in the draft was
  never sent there. `eb381074` makes the up-and-down test open the draft
  again and leave it with Up (wrapping to choice 3, the draft closed).
- **Round 2, at `eb381074`: 13 of 13 killed**, the clone's tree clean after
  the run (`mutants.txt`, both rounds).

| mutant | killed by (round 2) |
|---|---|
| M1 a modified arrow (Ctrl- or Alt-Up) moves the choice again | the four shell tests |
| M2 `C-p` moves the choice again | the four shell tests |
| M3 Super-Up and -Down in an amendment end it (and move the choice) | the amendment test |
| M4 Shift-Up in an amendment moves the caret (to the line's start) | the amendment test |
| M5 a no-op spelling revokes the readiness receipt | the receipt test |
| M6 a reshape no longer revokes the receipt | `leaving_a_filled_draft_…`, `opening_a_draft_revokes_…`, the receipt test |
| M7 Super-Up decodes as the arrow again | the decoder test, three shell tests |
| M8 Ctrl and Alt on Up and Down decode as the arrows again | three decoder tests, the receipt test |
| M9 Shift wins over Super (Super-Shift-Up is `SelectUp`) | the decoder test |
| M10 the fold gives the question Up for Super-Shift-Up | the fold test |
| M11 the fold leaves `ComposerUp` for the question | the fold test |
| M12 Super-Up in a draft goes to the line's start | the draft-edges test |
| M13 the plain Up no longer ends a draft | the up-and-down test (round 2) |

M1-M6 are the brief's six; M7-M12 the decoder, the fold and the draft's
edges they rest on; M13 the plain arrow the brief says must not change.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only '*approval*' --only composer-keys --only
question-field` at 40x120, `--jobs 4`, this branch's scenarios and ledger,
once with the base's binary (`v0.1.0-2593f49`) and once with the code
head's (`v0.1.0-ed5ab8b`, sha256 `a398ecbd…`), both in one suite-lock slot.
`composer-keys` sends the modified arrows to the composer and
`question-field` the shifted ones to a question's field: a fold that leaked
would show there. `cmp_sweeps.py` (APPROVAL-HORIZONTAL-KEYS', unchanged)
compares them pair by pair (`sweep-compare.txt`):

- **base 20 / 98, head 20 / 98, drops 0.** No pair changed class.
- `p5-approval-vertical-keys`: all 27 pairs stay `unresolved-defect` under
  `approval-grant-scope` alone (no unit can pass them); the rows no cause
  covers go to **0 in every one** at the head, from 2-4 in 24 of them at the
  base.
- `p5-approval-horizontal-keys` keeps 0 uncaused rows in all 29 pairs at
  both: the keys APPROVAL-HORIZONTAL-KEYS copied are unchanged.
- `composer-keys` 8 / 11 and `question-field` 10 / 10 at both.
- The 3 pairs elsewhere with uncaused rows are the same 3, with the same
  counts, at both (`p5-input-approval` `cmd-ask` 15, `edit-ask` 17,
  `after` 13); this unit touches no row of that scenario.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at
`92d087df`, whose `src/` differs from the code head `ed5ab8b9` in comments
only (`gate.txt` carries the `git diff --stat`). Every step exits 0 and the
tree was the same at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5554 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5591 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5554 is APPROVAL-HORIZONTAL-KEYS' gate count, 5548, plus its round-2
revocation test (`git diff 12848e4c 2593f49c` adds one `#[test]`) and this
unit's five (`git diff 2593f49c 92d087df` adds five and removes none: three
shell, one decoder, one draft). No test here builds a
`Runtime`: the shell cases use the shell fixture, the others the decoder,
the drafts and the folds alone. Every commit's added lines were scanned for
local paths before it was made (`git diff --cached`).

After the gate, `eb381074` adds one case inside the test module (M13);
at it `cargo fmt --check` and both clippy runs are clean and the unit's
tests and their neighbours pass, 407 (`gate.txt`, last section).

## Found, not changed

- **`ESC ESC [ A`/`[ B`** still cancel the turn (fx's meta prefix; xfx reads
  Escape, then the arrow). Left out of the scenario for that reason; owner
  QUESTION-LEFT-SPELLINGS (proposed), shown by APPROVAL-HORIZONTAL-KEYS'
  `probe/`.
- **A paste into a draft keeps its line breaks and tabs**; fx's turns each
  into a space (`normalizeApprovalAmendmentPasteInPlace`,
  `input_paste_runtime.zig:19-25`). So xfx's draft can hold several lines,
  and on it Home and End are a line's edges where Super-Up and -Down are the
  whole draft's; on fx, whose draft is one line, they are the same. Recorded
  in `docs/parity.md`; owner proposed (APPROVAL-PASTE-SPACES). This unit
  gives Super-Up and -Down fx's `cursorHome`/`cursorEnd` (0 and the length)
  rather than Home's line start, so they stay right once that lands.
- **`C-p`/`C-n` on the review plane** walk the change, where fx's plane
  scrolls by the wheel (xfx enables no mouse reports): unchanged, recorded
  in `docs/parity.md`.

## Decisions (each reversible)

- **Super-Shift-Up is Super-Up to the approval** (fx's table reads Super
  first), and still `SelectUp` to every other surface: the decoder carries
  the Shift bit in `DraftStart { shifted }` so the fold can give it back.
  The other way -- Shift first, the approval treating it as nothing -- would
  have left a spelling fx moves the caret on doing nothing in an amendment.
- **The approval reads before the fold** (above).
- **Swallowed before the panel**, not answered by the panel as `Ignored`:
  the panel never sees a key it does not bind, which is what keeps the
  receipt untouched by construction rather than by a reply's meaning.
