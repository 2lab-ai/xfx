# QUESTION-WORD-LEFT receipt: Alt-b at a choice goes back a question

The model's question panel (QUESTIONS-20, on `main`) answered fx v0.0.16's
word-left at a choice with nothing; fx goes back a question. QUESTIONS-20's
opus review named the gap (its SHOULD 1). The base is `origin/main` at
`32afd307`.

| commit | what |
|---|---|
| `d45823bc` | RED: the shell test, the scenario `p5-question-word-left`, `fx/`, `red/`, `fx-src.txt`, `pairs.sh`, `red.txt` |
| `8cb952a2` | **the code head**: the decoder's `Action::AltB`, the panel's `Act::AltB`, the shell's fold, the tests, `docs/parity.md` |
| `b3a23c52`, `cedcd9da` | round 1's receipt: `xfx/`, `probe/`, `green.txt`, mutants, sweeps, gate; no `src/` change |
| `3dce75e9` | round 2 RED: Shift-Alt-b/f in the decoder and shell tests, the panel test's marked choice, the amendment test, the fold test's name; the scenario's four new snaps; `fx/`, `red/`, `r1/` recaptured; the approval probe on fx |
| `fc32b83a` | **the code head** (round 2): Shift-Alt-b/f decode to the word moves; the claims about fx in `src/` corrected; `docs/parity.md` |
| the commits after | `xfx/`, `probe/`'s xfx side, `green.txt`, `mutants.py` (M8, M9), this file, then the mutants, the sweeps and the gate; no `src/` change |

Round 2 answers the reviews of `cedcd9da` (gpt6 and opus, both
APPROVE-WITH-FIXES, and the dispatcher's round-2 brief).

The xfx captures in `xfx/` and `probe/` come from the binary built at the code
head `fc32b83a` (row 1 `x𝒇x v0.1.0-fc32b83 [debug]`, sha256 `13374687a383…`);
those in `r1/` from round 1's code head `8cb952a2` (`v0.1.0-8cb952a`,
`ade20524aa2b…`); those in `red/` from the base (`v0.1.0-32afd30`,
`8ca6891a3650…`). fx is the v0.0.16 release, sha256 `8e6cda2e…`, run with
`FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after every run. No `gwlog`
step, so no request body (and no scratch path) is in the receipt.

## What fx v0.0.16 does (`fx-src.txt`, every line read with `git show 481104fd:<path>`)

- **At a choice** the panel's keys are routed by the decoded action
  (`routeQuestionEscapeAction`, `input_question_runtime.zig:139-201`):
  `.cursor_left, .word_left => retreatToPrevious` (`:190-195`) and
  `.cursor_right, .word_right =>` nothing, "Enter advances" (`:196-199`).
  A page is one question of the batch (`cycleQuestionPage`,
  `question_prompt.zig:670-676`; the test named "backward navigation preserves
  the page being left", `:987`); `retreatToPrevious` steps back one and returns
  `.none` on the first (`:609-615`).
- **Which keys are `.word_left`.** Only Alt-b's spellings: `ESC b`
  (`escape_parser.zig:528-533`), kitty's `CSI 98;3u` and xterm's `CSI 27;3;98~`
  (the keycode table, `:243-246`; fx's tests `runtime.zig:2273`, `:2293`).
  Ctrl-Left, Alt-Left, kitty's Ctrl-Left and a meta-prefixed `ESC ESC [ D` are
  `composerMove(.word_left)` (`:83-92`; tests `runtime.zig:2160`, `:2201`,
  `:2271`), a `.composer_shortcut` action: the switch above has no arm for it.
  Shift-Left is `composerMove(.character_left)` (`:99-106`), the same, and so
  are Shift-Alt-b and Shift-Alt-f, `composerMove(.word_left/.word_right)` with
  the selection extended (`:244`, `:248`).
- **What reaches the switch at a choice.** `questionActionFromShortcut` gives a
  question nothing for a move unless the freeform slot is marked
  (`runtime.zig:178-197`), so the action falls through to the switch
  (`input_question_runtime.zig:146-148`). With the slot marked, every word move
  is `.edit_freeform = .cursor_word_left` (or `_right`) and is the draft's
  (`runtime.zig:155-176`): `questionFreeformActionFromShortcut` reads only the
  move's kind, so a Shift that extends a selection is dropped there.
  A raw `0x02` (Ctrl-B) is no question action at all (`questionActionFromByte`,
  `runtime.zig:135-153`, `:199-209`; `app_input_runtime.zig:1136-1160`).
- **fx's own screens agree** (`fx/`, the scenario below): from page 4, Alt-b in
  each spelling lands on page 3 (`alt-b`, `kitty-alt-b`, `mok-alt-b`); Ctrl-Left,
  Alt-Left, kitty's Ctrl-Left, Alt-f, Ctrl-Right and Right stay on page 4
  (`ctrl-left`, `alt-left`, `kitty-ctrl-left`, `alt-f`, `ctrl-right`, `right`);
  Alt-b on page 1 stays (`alt-b-first`); in a draft on page 2, `ab cd`, Alt-b,
  `z` gives `ab zcd` on page 2 (`draft-alt-b`); Shift-Alt-b and Shift-Alt-f
  stay on page 4 (`shift-alt-b`, `shift-alt-f`) and in the draft are the word
  moves: `ab yzcd`, then `ab yzcdx` (`draft-shift-alt-b`, `draft-shift-alt-f`).
  `probe/`: Shift-Left, Ctrl-B and `ESC ESC [ D` stay on page 4.
- **The approval panel tells the two apart too.** With no amendment open,
  `.cursor_left, .word_left` mark the previous choice and `.cursor_right,
  .word_right` the next (`input_approval_runtime.zig:90-110`); Up and Down the
  same (`:111-126`); anything else, Ctrl-Left and Ctrl-Right included, nothing
  (`:128`). In an amendment every word move is the draft's
  (`approvalDraftActionFromShortcut`, `runtime.zig:75-96`; `:79-88`).

So the brief's lead "the Alt/Option-b or Ctrl-Left binding" is half of it:
Ctrl-Left does **not** go back in fx, and xfx already matched that.

## What xfx did at the base

The decoder gave Alt-b, Ctrl-Left and Alt-Left one action, `Action::WordLeft`
(`ESC b`, `src/tui/input.rs:638`; the keycode table `:1303`; the arrows'
`b'D' if word`, `:987`), and the panel answered `Act::WordLeft` at a choice with
`Answered::Nothing` (`src/tui/question.rs:857`, `:866`). The panel could not
copy fx without telling the spellings apart.

Shift-Alt-b and Shift-Alt-f decoded to `Ignore` (`kitty_key`'s `if mods & SHIFT
!= 0 { return ignore; }`), so in a question's draft they did nothing where fx
moves by word.

## The change (the code head `fc32b83a`)

- `Action::AltB` (`src/tui/input.rs:202`): `ESC b` (`:657`) and the keycode
  table's Alt-b (`:1333`) decode to it; the arrows stay `WordLeft`.
- Shift-Alt-b and Shift-Alt-f decode to `WordLeft` and `WordRight` (`:1323-1329`,
  round 2): at a choice they do nothing and in a draft they are the word moves,
  as in fx; in the composer they are the plain word moves, the Shift dropped as
  it is from the arrows, until COMPOSER-SELECTION-EXTEND (queued) gives the
  composer a selection to extend (`docs/parity.md`'s full-screen TUI row).
- The question panel reads `AltB` as `Act::AltB` (`src/tui/question.rs:235`):
  with no draft open the previous question, as Left (`:840`); in a draft the
  word move (`:900`).
- The shell's fold, `folded` (renamed from `unshifted`, which already folded
  Shift on Up and Down for the same reason), maps `AltB` back to `WordLeft`
  (`src/tui/shell.rs:13727-13734`). It runs after the question (`answer`) and
  the full transcript (`:12003`, which binds neither key: `full_transcript.rs`'s
  `key`, `_ => Reply::Unbound`) have read the event, and before the approvals,
  the menus and the composer do (`:12026`). The composer and the editor also take
  a stray `AltB` as the word move (`shell.rs:12398`, `editor.rs:440`, `:465`), as
  they do a stray `SelectUp`.
- Round 2 corrected three comments in `src/` that said a question is the only
  surface that tells Alt-b from the arrows: fx's approval choice does too. They
  now say a question is the only **xfx** surface that reads `AltB` today
  (`input.rs`'s `AltB` doc and the decoder test, `folded`'s doc).
- `input.rs`'s herdr-receipt table and its ESC-spelling table expect `AltB` for
  `alt+b` and `ESC b`: the bytes are the receipt's, the action is the one xfx
  has for them now. `CSI 98;4u` and `CSI 102;4u` left the table of keys xfx
  ignores.

## RED (`red.txt`)

- Round 1, at the base: the test
  `tui::shell::tests::alt_b_at_a_choice_goes_back_a_question_and_the_word_arrows_do_not`
  routes the real bytes through the shell: the word keys pass (still on
  question 2), then `[27, 98] did not go back a question`.
- The base's pair, on the round-2 scenario: 5 of 16 differ, `alt-b`,
  `kitty-alt-b` and `mok-alt-b` (fx on page 3, the base on page 4) and
  `draft-shift-alt-b`, `draft-shift-alt-f` (fx's draft moved by word, the
  base's did not).
- Round 2, at round 1's code (`cedcd9da`) with round 2's tests: the decoder test
  fails on `CSI 98;4u` (`Ignore`, wanted `WordLeft`) and the shell test on the
  draft (`ab zycd`, wanted `ab yzcd`). Round 1's binary (`r1/`) differs from fx
  on `draft-shift-alt-b` and `draft-shift-alt-f` only: 14 of 16.

## GREEN (`green.txt`)

- **16 of 16 pairs identical** (`pairs.sh xfx`: `.prd/tui-phase4/receipts/m1/cells.py`,
  row 1 left out), every one at the page fx is on.
- The new tests pass at the head:
  - the shell test above, now with Shift-Alt-b and Shift-Alt-f at a choice and
    in the draft;
  - the panel's `alt_b_goes_back_a_question_at_a_choice_and_a_word_in_a_draft`:
    page 3 is left with choice 2 marked, Alt-b from page 4 lands on page 3 with
    choice 2 still marked and choice 1 not; page 1 stays; the arrows, Alt-f and
    Right do nothing; the draft's `ab zcd`;
  - the decoder's `alt_b_is_its_own_action_and_the_modified_left_arrows_are_word_left`
    (with Shift-Alt-b/f in both spellings);
  - `alt_b_in_an_approval_amendment_is_the_word_move`: Alt-b in an amendment
    draft moves by word, and the answer carries `ab zcd`. The approval reads
    the event after the fold, so an `AltB` the fold let through would be a key
    it swallows (M7);
  - `the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest`,
    `folded`'s own contract (renamed from round 1's
    `alt_b_is_the_word_move_to_every_surface_below_a_question`, which called
    only the helper).

## The keys checked at a choice (fx, xfx head)

| key | fx v0.0.16 | xfx head | where |
|---|---|---|---|
| Left | back a question | same | `q20-left` (QUESTIONS-20) |
| Alt-b: `ESC b`, `CSI 98;3u`, `CSI 27;3;98~` | back a question | same (**changed**) | `alt-b`, `kitty-alt-b`, `mok-alt-b` |
| Alt-b on the first question | nothing | same | `alt-b-first` |
| Ctrl-Left, Alt-Left, kitty Ctrl-Left | nothing | same | `ctrl-left`, `alt-left`, `kitty-ctrl-left` |
| Alt-f, Ctrl-Right, Right | nothing | same | `alt-f`, `ctrl-right`, `right` |
| Alt-b in a draft | word left, same page | same | `draft-alt-b` |
| Shift-Alt-b, Shift-Alt-f | nothing | same | `shift-alt-b`, `shift-alt-f` |
| Shift-Alt-b, Shift-Alt-f in a draft | word left, word right | same (**changed**, round 2) | `draft-shift-alt-b`, `draft-shift-alt-f` |
| Tab | next question, wrapping | same | `q20-tab10`, `q20-wrap` (QUESTIONS-20) |
| Shift-Left | nothing | **back a question** | `probe/…-shift-left` |
| Ctrl-B | nothing | **back a question** | `probe/…-ctrl-b` |
| `ESC ESC [ D` | nothing | **cancels the batch** | `probe/…-esc-esc-left` |

The last three differ the other way (xfx moves where fx does not), so this unit
does not change them: each is a decoder spelling xfx folds into a key fx keeps
apart -- Shift dropped from Left (`cursor`, `src/tui/input.rs`), `0x02` as Left
(`:882` at the base), no `meta_prefixed` (`kitty_key`'s doc). They are on the
`ask_user_question` row of `docs/parity.md` with owner unit
QUESTION-LEFT-SPELLINGS (proposed). `probe/p5-question-word-left-other.cap` is
the scenario that took them; it is not a parity scenario.

**The approval panel does not match fx, and this unit leaves it as it was.**
At an approval choice with no amendment open, fx moves the choice on Left and
Alt-b (previous) and Right and Alt-f (next), and leaves it on Ctrl-Left and
Ctrl-Right (`input_approval_runtime.zig:90-128@481104fd`, above). xfx's `decide`
binds none of the four: they fall through to its swallow
(`Input::Action(_) | Input::PasteByte(_) => return`, `src/tui/shell.rs:9488`).
`probe/p5-approval-horizontal-keys` shows it on both at an edit's question
(`green.txt`): from choice 2, fx's Left marks 1, Right 2, Alt-f 3 and Alt-b 2,
and Ctrl-Right and Ctrl-Left leave 2; xfx stays on 2 after every key. In an
amendment both take Alt-b as the word move (`amend-alt-b`). Copying it needs
Alt-f kept apart from Ctrl-Right in the decoder as Alt-b now is, so it is its
own unit with its own captures: **APPROVAL-HORIZONTAL-KEYS** (queued), on the
full-screen TUI row of `docs/parity.md`.

## Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway of the branch at the code
head `fc32b83a` (no remote) under `$TMPDIR/mut/`, one at a time, the file
restored after each, under the suite lock; per mutant `cargo test --locked
--lib -- alt_b the_fold question:: input::tests` with a 900 s timeout. M2 reads
the brief's "a retreat by one instead of a page" as fx's page is one question:
Alt-b moving the marked choice up one instead of the question back.
`mutants.txt` has the round-2 run (the table below is filled from it):

| mutant | killed by |
|---|---|
| M1 Alt-b at a choice does nothing again (the base's behaviour) | the panel test, the shell test |
| M2 Alt-b moves the marked choice up one instead of the question back | the panel test, the shell test |
| M3 Ctrl-Left and Alt-Left go back a question too | the panel test, the shell test |
| M4 Alt-b in a draft is not the word move | the panel test, the shell test |
| M5 the decoder reads `ESC b` as `WordLeft` again | the decoder test, the ESC-spelling table, the shell test |
| M6 the decoder reads kitty/xterm Alt-b as `WordLeft` again | the decoder test, the herdr table, the shell test |
| M7 the fold lets `AltB` through to the other surfaces | the fold test |

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-diff.txt`)

`scripts/fxcap/sweep.sh --only '*question*' --only '*approval*' --only
'filepicker-keys'` at 40x120 with this branch's scenarios and ledger, once with
the base binary and once with the head's, each under the suite lock.
`filepicker-keys` is the one other scenario that sends Alt-b (in the composer,
over the file picker). `sweep-base.md` and `sweep-head.md` are the scoreboards
(redacted by the sweep); `sweep-diff.txt` is every pair's verdict at both,
compared:

- **base 67 / 95, head 70 / 95.** The three that moved are this unit's targets,
  `p5-question-word-left`'s `alt-b`, `kitty-alt-b` and `mok-alt-b`, from
  unresolved to normalized-identical.
- **None dropped.** `filepicker-keys`' seven pairs, Alt-b in the composer
  among them, are normalized-identical at both.
- The 25 unresolved at the head are the same 25 as at the base: the approval
  scenarios' rows, `p5-input-question20`'s `resumed` and `resumed.full`
  (`question-resume-replay`) and `p5-transcript-question`'s four; this unit
  touches none of those scenarios.

## Gate (`gate.txt`)

The brief's gate, run once at `b3a23c52` (`src/` and `tests/` identical to the
code head `8cb952a2`), under the suite lock, with a short `TMPDIR`. Every step
exits 0:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5543 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5580 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

No test in this unit builds a `Runtime`: the shell tests use the shell fixture
(`shell(rows, cols)`), the others the panel, the decoder and the fold alone.
