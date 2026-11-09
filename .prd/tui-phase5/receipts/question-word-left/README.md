# QUESTION-WORD-LEFT receipt: Alt-b at a choice goes back a question

The model's question panel (QUESTIONS-20, on `main`) answered fx v0.0.16's
word-left at a choice with nothing; fx goes back a question. QUESTIONS-20's
opus review named the gap (its SHOULD 1). The base is `origin/main` at
`32afd307`.

| commit | what |
|---|---|
| `d45823bc` | RED: the shell test, the scenario `p5-question-word-left`, `fx/`, `red/`, `fx-src.txt`, `pairs.sh`, `red.txt` |
| `8cb952a2` | **the code head**: the decoder's `Action::AltB`, the panel's `Act::AltB`, the shell's fold, the tests, `docs/parity.md` |
| the commits after | this file, `xfx/`, `probe/`, `green.txt`, `mutants.py`, `mutants.txt`, the sweeps, `gate.txt`; no `src/` change |

The xfx captures in `xfx/` and `probe/` come from the binary built at
`8cb952a2` (row 1 `x𝒇x v0.1.0-8cb952a [debug]`, sha256 `ade20524aa2b…`); those
in `red/` from the base (`v0.1.0-32afd30`, `8ca6891a3650…`). fx is the v0.0.16
release, sha256 `8e6cda2e…`, run with `FX_AUTO_UPGRADE=0`; `capture.sh`
checked its bytes after every run. No `gwlog` step, so no request body (and
no scratch path) is in the receipt.

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
  Shift-Left is `composerMove(.character_left)` (`:99-106`), the same.
- **What reaches the switch at a choice.** `questionActionFromShortcut` gives a
  question nothing for a move unless the freeform slot is marked
  (`runtime.zig:178-197`), so the action falls through to the switch
  (`input_question_runtime.zig:146-148`). With the slot marked, every word move
  is `.edit_freeform = .cursor_word_left` and is the draft's (`runtime.zig:155-176`).
  A raw `0x02` (Ctrl-B) is no question action at all (`questionActionFromByte`,
  `runtime.zig:135-153`, `:199-209`; `app_input_runtime.zig:1136-1160`).
- **fx's own screens agree** (`fx/`, the scenario below): from page 4, Alt-b in
  each spelling lands on page 3 (`alt-b`, `kitty-alt-b`, `mok-alt-b`); Ctrl-Left,
  Alt-Left, kitty's Ctrl-Left, Alt-f, Ctrl-Right and Right stay on page 4
  (`ctrl-left`, `alt-left`, `kitty-ctrl-left`, `alt-f`, `ctrl-right`, `right`);
  Alt-b on page 1 stays (`alt-b-first`); in a draft on page 2, `ab cd`, Alt-b,
  `z` gives `ab zcd` on page 2 (`draft-alt-b`). `probe/`: Shift-Left, Ctrl-B and
  `ESC ESC [ D` stay on page 4.

So the brief's lead "the Alt/Option-b or Ctrl-Left binding" is half of it:
Ctrl-Left does **not** go back in fx, and xfx already matched that.

## What xfx did at the base

The decoder gave Alt-b, Ctrl-Left and Alt-Left one action, `Action::WordLeft`
(`ESC b`, `src/tui/input.rs:638`; the keycode table `:1303`; the arrows'
`b'D' if word`, `:987`), and the panel answered `Act::WordLeft` at a choice with
`Answered::Nothing` (`src/tui/question.rs:857`, `:866`). The panel could not
copy fx without telling the spellings apart.

## The change (the code head)

- `Action::AltB` (`src/tui/input.rs:198`): `ESC b` (`:653`) and the keycode
  table's Alt-b (`:1320`) decode to it; the arrows stay `WordLeft`.
- The question panel reads it as `Act::AltB` (`src/tui/question.rs:234`): with
  no draft open the previous question, as Left (`:839`); in a draft the word
  move (`:899`).
- Every other surface sees `WordLeft`: the shell's fold, `folded` (renamed from
  `unshifted`, which already folded Shift on Up and Down for the same reason),
  maps it back before the approvals, the menus and the composer read the event
  (`src/tui/shell.rs:13724-13731`). The composer and the editor also take a
  stray `AltB` as the word move (`shell.rs:12398`, `editor.rs:440`, `:465`), as
  they do a stray `SelectUp`. The full transcript, which reads events before the
  fold, binds neither (`full_transcript.rs`'s `key`: `_ => Reply::Unbound`).
- `input.rs`'s herdr-receipt table and its ESC-spelling table now expect
  `AltB` for `alt+b` and `ESC b`: the bytes are the receipt's, the action is
  the one xfx has for them now.

## RED (`red.txt`)

- The test `tui::shell::tests::alt_b_at_a_choice_goes_back_a_question_and_the_word_arrows_do_not`
  routes the real bytes through the shell at the base: the five word keys pass
  (still on question 2), then `[27, 98] did not go back a question`.
- The pair: 3 of 12 differ, `alt-b`, `kitty-alt-b` and `mok-alt-b` (fx on page
  3, the base on page 4); the 9 others are already identical.

## GREEN (`green.txt`)

- **12 of 12 pairs identical** (`pairs.sh xfx`: `.prd/tui-phase4/receipts/m1/cells.py`,
  row 1 left out), every one at the page fx is on.
- The four new tests pass at the head: the shell test above; the panel's
  `alt_b_goes_back_a_question_at_a_choice_and_a_word_in_a_draft` (page 4 → 3,
  the page's own marked choice, page 1 stays, the arrows, Alt-f and Right do
  nothing, the draft's `ab zcd`); the decoder's
  `alt_b_is_its_own_action_and_the_modified_left_arrows_are_word_left`; the
  fold's `alt_b_is_the_word_move_to_every_surface_below_a_question`.

## The keys checked at a choice (fx, xfx head)

| key | fx v0.0.16 | xfx head | where |
|---|---|---|---|
| Left | back a question | same | `q20-left` (QUESTIONS-20) |
| Alt-b: `ESC b`, `CSI 98;3u`, `CSI 27;3;98~` | back a question | same (**changed**) | `alt-b`, `kitty-alt-b`, `mok-alt-b` |
| Alt-b on the first question | nothing | same | `alt-b-first` |
| Ctrl-Left, Alt-Left, kitty Ctrl-Left | nothing | same | `ctrl-left`, `alt-left`, `kitty-ctrl-left` |
| Alt-f, Ctrl-Right, Right | nothing | same | `alt-f`, `ctrl-right`, `right` |
| Alt-b in a draft | word left, same page | same | `draft-alt-b` |
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

fx's approval panel binds the same split (`.cursor_left, .word_left` and
`.cursor_right, .word_right` move its choice, `input_approval_runtime.zig:90-110`);
whether xfx's approval panel matches it for Alt-b against Ctrl-Left was not
checked here. This unit leaves the approval panel's behaviour as it was: it
reads the folded `WordLeft`, as before.

## Mutants (`mutants.txt`)

`mutants.py` on a `git clone --no-local` throwaway of the branch at `8cb952a2`
(no remote) under `$TMPDIR/mut/`, one at a time, the file restored after each,
under the suite lock; per mutant `cargo test --locked --lib -- alt_b question::
input::tests` with a 900 s timeout. M2 reads the brief's "a retreat by one
instead of a page" as fx's page is one question: Alt-b moving the marked
choice up one instead of the question back.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-diff.txt`)

`scripts/fxcap/sweep.sh --only '*question*' --only '*approval*' --only
'filepicker-keys'` at 40x120 with this branch's scenarios and ledger, once with
the base binary and once with the head's, each under the suite lock.
`filepicker-keys` is the one other scenario that sends Alt-b (in the composer,
over the file picker).

## Gate (`gate.txt`)

The brief's gate, run once at the receipt's last code-identical commit, under
the suite lock, with a short `TMPDIR`.
