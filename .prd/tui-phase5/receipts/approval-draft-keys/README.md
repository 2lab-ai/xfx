# APPROVAL-DRAFT-KEYS receipt: the approval draft takes fx v0.0.16's keys

xfx's permission-question draft (the feedback typed after Tab on choice 1 or
3) undid, redid and yanked; fx v0.0.16's does none of the three. So C-w then
C-y sent `ab cd` where fx sends `ab ` (APPROVAL-READINESS-SETTLED's
`red.txt:50`, its opus review S-1). This unit reads fx's draft key set at the
pin, takes the three out, and finds one more key that did not follow the
readiness rule: Tab in an open draft. The base is APPROVAL-READINESS-SETTLED's
head `b3a19ede` (branch `fix/p5-approval-readiness-settled`, not yet on
`main`).

| commit | what |
|---|---|
| `5fadcf22` | RED: three new shell tests, one shell case flipped (C-w then C-y sends `ab `), the approval Tab case and the draft's undo test rewritten, the scenario `p5-approval-draft-keys`; tests only in `src/` |
| `09077f5f` | **the code head**: `approval::edits` without `Undo`, `Redo` and `Yank`; Tab in a draft `Moved`; the draft's undo history deleted (`src/tui/approval_amendment.rs`); `docs/parity.md`'s approval sentence; the ledger's moved refs |
| the commit after | this receipt, and in the parity sentence "the arrows" narrowed to "Left and Right in each spelling" (Up and Down move the choice); no `src/` change |

Captures, all `p5-approval-draft-keys` at 40x120: `fx/` is fx v0.0.16
(sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`; `capture.sh` checked its bytes after
the run), `red/` the base's binary (row 1 `x𝒇x v0.1.0-b3a19ed [debug]`, built
at `b3a19ede` before any commit), `xfx/` the code head's (row 1
`x𝒇x v0.1.0-09077f5 [debug]`). The hashes and banners: `binaries.txt`. The
request bodies stay out (their system prompt names the run's directory);
`sent.txt` is what each run sent as feedback, read from them by `sent.py`.

## fx's draft keys (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**How a key reaches the draft.** A byte read with no sequence open is a raw
byte (`appendRaw`, `terminal_action_decoder.zig:207-212`; C-o and C-p are
taken first as features, `terminal_action_decoder.zig:118-122`,
`escape_parser.zig:268-274`). `withApprovalInput` makes it an approval
action, a focused-editor key first and else `approvalActionFromByte`
(`src/ui/input/runtime.zig:115-120`, `:103-108`, `:98-101`, `:61-73`), and the
question applies it (`app_input_runtime.zig:1161-1184`). A decoded sequence
carries a composer shortcut (`appendAction`, `terminal_action_decoder.zig:214-224`;
`fromEscapeAction`, `shortcuts.zig:43-65`) that
`approvalDraftActionFromShortcut` turns into a draft action
(`src/ui/input/runtime.zig:121-129`, `:75-96`); a key report that stands for
a byte is replayed through the raw route (`app_input_runtime.zig:950-953`,
`:647-650`, `:746-756`). While a draft is open a decoded draft action edits
it and asks for a frame whatever it changed (`input_approval_runtime.zig:79-89`),
and a raw one answers `.redraw` (`approval_decision.zig:170-174`), which asks
for it too (`input_approval_runtime.zig:59-61`).

**The raw-byte route.** `fromFocusedEditorControlByte` keeps the moves and
three deletes of `fromControlByte` (`shortcuts.zig:31-41`, `:11-29`).

| byte | fx in an open draft | xfx base | xfx head |
|---|---|---|---|
| `0x01` C-a, `0x05` C-e | caret to the draft's start, end (`src/ui/input/runtime.zig:82-83`; `approval_amendment.zig:84-92`) | same | same |
| `0x02` C-b, `0x06` C-f | caret one step (`src/ui/input/runtime.zig:80-81`; `approval_amendment.zig:70-82`) | same | same |
| `0x0b` C-k | delete to the line's end (`src/ui/input/runtime.zig:93`; `approval_amendment.zig:140-148`) | same | same |
| `0x15` C-u | delete to the line's start (`src/ui/input/runtime.zig:92`; `approval_amendment.zig:134-138`) | same | same |
| `0x17` C-w | delete the whitespace word to the left (`src/ui/input/runtime.zig:90`; `approval_amendment.zig:117-126`) | same | same |
| `0x08`, `0x7f` Backspace | delete one character (`src/ui/input/runtime.zig:66`; `approval_decision.zig:152-155`; `approval_amendment.zig:60-68`) | same | same |
| `0x20`-`0x7e`, digits included | inserted (`src/ui/input/runtime.zig:67-71`; `approval_decision.zig:157-165`, `:196-212`) | same | same |
| a UTF-8 scalar | inserted, a modal frame (`app_input_runtime.zig:1162-1164`, `:1251-1255`) | same | same |
| `0x09` Tab | changes nothing; `.redraw`, so it asks for the frame (`src/ui/input/runtime.zig:65`; `approval_decision.zig:142-143`) | changes nothing, **asks for no frame** (`Reply::Ignored`) | **changes nothing, asks for the frame** |
| `0x0d` Enter | answers the marked choice (`src/ui/input/runtime.zig:64`; `approval_decision.zig:141`) | same | same |
| `0x0a` C-j | answers as Enter does (`src/ui/input/runtime.zig:64`) | nothing | nothing (below) |
| `0x03` C-c | refuses (`src/ui/input/runtime.zig:63`; `approval_decision.zig:137-140`) | same | same |
| `0x19` C-y | nothing: out of the focused set (`shortcuts.zig:22`, `:31-41`), no approval byte (`src/ui/input/runtime.zig:61-73`) | **yank** | **nothing** |
| `0x1f` C-_ | nothing, the same way (`shortcuts.zig:23`) | **undo** | **nothing** |
| `0x04` C-d, `0x0c` C-l, `0x0e` C-n | nothing, the same way (`shortcuts.zig:18`, `:24`, `:17`) | nothing | nothing |

**The decoded route.**

| key | fx in an open draft | xfx base | xfx head |
|---|---|---|---|
| Left, Right (`CSI D/C`, `SS3 D/C`; `escape_parser.zig:564-567`, `:640-643`) | caret one step (`shortcuts.zig:47-48`) | same | same |
| Shift-Left, -Right (`escape_parser.zig:99-103`) | caret one step | same | same |
| Ctrl-, Alt-Left and -Right (`escape_parser.zig:83-92`) | word moves (`src/ui/input/runtime.zig:84-85`; `approval_amendment.zig:94-102`) | same | same |
| Alt-b, Alt-f (`ESC b/f`, `escape_parser.zig:528-539`; kitty, `escape_parser.zig:243-250`) | word moves (`shortcuts.zig:49-50`) | same | same |
| Super-Left, -Right (`escape_parser.zig:77-78`) | caret to the draft's start, end (`src/ui/input/runtime.zig:82-83`) | one step | one step (`docs/parity.md`, COMPOSER-SUPER-ARROWS) |
| Super-Up, -Down (`escape_parser.zig:74-76`) | caret to the draft's start, end | same | same |
| Up, Down | the choice moves, the draft closes (`input_approval_runtime.zig:111-126`; `approval_decision.zig:214-225`) | same | same |
| Shift-, Ctrl-, Alt-Up and -Down (`escape_parser.zig:83-101`) | nothing: no draft action (`src/ui/input/runtime.zig:86`), the switch's `else` (`input_approval_runtime.zig:128`) | same | same |
| Home, End (`CSI H/F`, `CSI 1~/4~`, `SS3 H/F`; `escape_parser.zig:568`, `:629-631`, `:644`), and with any modifier as `CSI 1;<m>H/F` (`escape_parser.zig:73-107`) | caret to the draft's start, end (`shortcuts.zig:51-52`) | same | same |
| `CSI 7~`, `CSI 8~`; `CSI 1;<m>~`, `4;<m>~`, `7;<m>~`, `8;<m>~` (`escape_parser.zig:629-631`, `:699-707`) | caret to the draft's start, end | **nothing** (the decoder's `Ignore`) | nothing (below) |
| Page Up, Down (`escape_parser.zig:632-633`) | nothing (`src/ui/input/runtime.zig:86`) | same | same |
| Delete (`CSI 3~`, `escape_parser.zig:630`) | delete the next character (`shortcuts.zig:55`; `approval_amendment.zig:104-109`) | same | same |
| Ctrl-, Alt-Delete; Super-Delete (`escape_parser.zig:176-180`, `:698`) | delete the word to the right; to the line's end (`approval_amendment.zig:128-132`, `:140-148`) | same | same |
| Alt-Backspace (`ESC DEL`, `escape_parser.zig:546-550`; kitty, `escape_parser.zig:261`) | delete the word to the left (`approval_amendment.zig:111-115`) | same | same |
| Super-Backspace (kitty, `escape_parser.zig:260`) | delete to the line's start | same | same |
| Alt-d (`ESC d`, `escape_parser.zig:540-544`; kitty, `escape_parser.zig:251`) | delete the word to the right | same | same |
| kitty or xterm Ctrl+letter (`escape_parser.zig:255-259`) | the letter's byte, replayed: the table above | same; Ctrl+Y a **yank**, Ctrl+_ an **undo** | same; both **nothing** |
| Ctrl+Shift+A, B, E, F (kitty, `escape_parser.zig:235-242`) | caret to the start, one step left, to the end, one step right | **nothing** (the decoder's `Ignore`) | nothing (`docs/parity.md`, COMPOSER-SELECTION-EXTEND) |
| Super+Z, Super+Shift+Z (`escape_parser.zig:232-234`) | nothing: a composer undo and redo, no draft action (`src/ui/input/runtime.zig:94`), the switch's `else` | **undo**, **redo** | **nothing** |
| Super+A, C, X (`escape_parser.zig:223-231`); Alt- and Shift-Enter (`escape_parser.zig:515-519`, `:219-221`); Shift-Tab (`escape_parser.zig:569`, `:254`) | nothing, the same way | nothing | nothing |

**Paste.** A bracketed paste while a draft is open is the draft's
(`beginPaste`, `input_paste_runtime.zig:105-127`): its line breaks and tabs
become spaces (`input_paste_runtime.zig:19-25`, `:242-245`), text that is not
model-safe is refused (`input_paste_runtime.zig:250-266`), and the rest is
inserted with a modal frame (`input_paste_runtime.zig:308-334`). xfx's draft
takes it too and keeps the line breaks and tabs (`docs/parity.md`,
APPROVAL-PASTE-SPACES).

**What each delete deletes**, which the kept-keys test checks from `ab `:
`previousWordStart`, `nextWordEnd`, `nextWordDeleteEnd` and the line bounds
(`text_boundaries.zig:34-89`), and the whitespace word
(`text_boundaries.zig:149-167`). xfx's draft gives fx's answer in every case
of that test, at the base already (`red.txt`: it passes there).

## The change (`git diff --stat b3a19ede 09077f5f -- src`: 3 files, +291 / -114; outside the `mod tests` blocks +41 / -74: comments and the deleted history)

- **`approval::edits` no longer names `Undo`, `Redo` or `Yank`**
  (`src/tui/approval.rs`). Every spelling of the three reaches
  `Shell::decide`'s `_ => return` in its draft arm, before the panel: no
  edit, no `approval_changed()`, no frame, as fx's ask for no frame. So a C-y
  read with an Enter answers at once with the draft as it was, on the band
  and on the review plane.
- **Tab in an open draft is `Reply::Moved`** (`approval::keyed`). It still
  changes nothing; now it asks for the frame and holds an Enter read with it,
  as fx's `.redraw` does. This is the brief's rule ("a key that changes
  nothing still holds it") checked against the key set: Tab was the one key
  that fx's draft answers with a frame and xfx's did not.
- **The draft's undo history is deleted** (`src/tui/approval_amendment.rs`).
  With the three keys gone, nothing read it. A draft is an `Editor`, and a
  kill only deletes, as in fx's amendment, which has no history and no kill
  ring (`approval_amendment.zig:9-148`).
- Nothing else in the draft's key set changes: every key the tables mark
  "same" goes through code this unit did not touch, and the kept-keys test
  pins the draft's edit keys among them.

## Decisions (each reversible)

- **A removed key holds nothing back.** It is swallowed before the panel, so
  the receipt stands and an Enter read with it answers at once, as fx's key
  asks for no frame. Mutant M11 is the stricter way (a removed key marks a
  change); the removed-keys test kills it.
- **Tab's hold applies at every approval**, as APPROVAL-READINESS-SETTLED's
  decision applies the condition to every approval.
- **Not taken here, recorded in `docs/parity.md`** with a proposed owner:
  - C-j answers fx's question as Enter does, at a choice and in a draft
    (`src/ui/input/runtime.zig:64`), and does nothing in xfx's. It is a key
    that grants, and its surface is the question's submit, not the draft's
    edit set: APPROVAL-CTRL-J.
  - `CSI 7~`/`8~` and the modified `CSI 1;<m>~`/`4;<m>~` Home and End are a
    decoder gap that reaches the composer too: DECODER-TILDE-HOME-END.
  - Super-Left/-Right as a step, Ctrl+Shift+A/B/E/F binding nothing and a
    paste keeping its line breaks were recorded already (COMPOSER-SUPER-ARROWS,
    COMPOSER-SELECTION-EXTEND, APPROVAL-PASTE-SPACES). The parity sentence now
    names the draft half of the second.

## RED (`red.txt`)

At `5fadcf22` (`git diff b3a19ede 5fadcf22 -- src` adds tests only), five
tests fail and one passes:
- `undo_redo_and_yank_do_nothing_in_a_draft_as_fxs_do`: all nine spellings
  (C-y, kitty and xterm Ctrl+Y, C-_, kitty Ctrl+_, and Super+Z and
  Super+Shift+Z in both CSI-u forms), on both planes, touched the receipt,
  and the Enter after or with each was held.
- `an_edit_and_enter_in_one_read_send_nothing_until_a_frame_shows_the_draft`:
  "ctrl-w then ctrl-y" sent `ab cd`, not `ab `.
- `tab_in_a_draft_changes_nothing_and_holds_the_enter_until_a_frame_as_fxs_does`:
  Tab and Enter in one read answered `Once` with `ab ` before a frame.
- `tab_enters_the_draft_of_an_eligible_choice_and_cycles_past_an_ineligible_one`:
  Tab in a draft was `Ignored`, not `Moved`.
- `a_draft_has_no_undo_redo_or_yank_as_fxs_has_none`: `Undo changed the draft`.
- `every_key_fxs_draft_takes_still_edits_it_and_holds_the_enter_until_a_frame`
  passes. It pins the 38 kept-key cases, and mutants M5-M10 are what it
  guards against.

The pair at the base (`pairs.txt`, `scope.py red`): **5 of 10** identical.
`edit-ctrl-y`, `edit-super-z` and `edit-kitty-ctrl-y` show `ab cd` where fx
shows `ab `. At `edit-tab-enter` the base had answered (the Enter read with
Tab), so it and `write-ask` are a question ahead. `edit-ctrl-underscore` and
`edit-super-shift-z` match at the base only because of the scenario's order:
its C-_ undid the yank before it, and its Super+Shift+Z redid the undo of the
C-w, both back to `ab `. `sent.txt`: the base sent `ab cd`.

## GREEN (`green.txt`, `pairs.txt`, `sent.txt`)

- At the code head, the unit's tests and their neighbours (APPROVAL-READINESS-SETTLED's
  filter with this unit's three test names added): 479 passed, 0 failed, 1
  ignored (a release-only timing receipt) -- that filter's 476 at
  APPROVAL-READINESS-SETTLED's round 2 plus this unit's three new tests.
  `cargo test --locked --test parity`: 26 passed.
- **The pairs** (`scope.py xfx`: the ledger's `approval-grant-scope` rows set
  aside on each side, row 1 left out, styles compared): **10 of 10
  identical**: `edit-ask`, `edit-typed`, `edit-ctrl-w`, `edit-ctrl-y`,
  `edit-ctrl-underscore`, `edit-super-z`, `edit-super-shift-z`,
  `edit-kitty-ctrl-y`, `edit-tab-enter` (the question still up, `ab `
  unchanged), and `write-ask` (the next Enter answered).
- **The text sent** (`sent.txt`): fx `["ab "]`, the head `["ab "]`, the base
  `["ab cd"]`.

## Mutants (`mutants.py`, `mutants.txt`): 11 of 11 killed

On a fresh `git clone --no-local` throwaway of the branch at `09077f5f`
(remote removed) under `$TMPDIR/mut/`, one at a time, the file restored after
each, under the suite lock. The clone's tree was clean after the run.

| mutant | killed by |
|---|---|
| M1 Undo put back in `approval::edits` | the removed-keys test |
| M2 Redo put back | the removed-keys test |
| M3 Yank put back | the removed-keys test |
| M4 Tab in a draft `Ignored` again | the shell Tab test and the approval Tab case |
| M5 C-u and Super-Backspace removed (`KillToStart`) | the kept-keys test and the edit test |
| M6 Delete removed | the kept-keys test |
| M7 Home, C-a and their spellings removed | the kept-keys test |
| M8 Alt-Backspace removed (`DeleteWordBackward`) | the kept-keys test |
| M9 Super-Up and -Down in a draft swallowed | the kept-keys test and the two vertical-spelling tests |
| M10 a draft edit holds no Enter (`Ignored`) | the kept-keys test, the edit test and the one-read test |
| M11 a removed key holds the Enter (marks a change) | the removed-keys test |

With the history deleted, putting a removed key back in `edits` makes it a
no-op that holds the Enter; putting back its old behaviour as well is the
base, which RED shows.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'approvals*' --only 'p5-approval*' --only
'p5-input-approval*'` at 40x120, `--jobs 4`, this branch's scenarios and
ledger, once with the base's binary (`v0.1.0-b3a19ed`) and once with the
code head's (`v0.1.0-09077f5`), both in one suite-lock slot; compared pair by
pair with APPROVAL-READINESS-SETTLED's `cmp_sweeps.py`, unchanged.

- **base 3 / 98, head 3 / 98, drops 0.** Pairs with rows no cause covers: 8
  at the base, 3 at the head.
- `p5-approval-draft-keys`: at the head every one of its 10 pairs has 0 rows
  no cause covers, `approval-grant-scope` alone. At the base `edit-ctrl-y`,
  `edit-super-z` and `edit-kitty-ctrl-y` had 2 each (the draft's row),
  `edit-tab-enter` 19 and `write-ask` 23 (a question ahead).
- No other pair changed class, cause or count: `approvals`,
  `approvals-amend-limit`, `approvals-big`, `p5-approval-horizontal-keys`,
  `p5-approval-vertical-keys` and `p5-approval-readiness-settled` are the
  same on both sides. The 3 pairs with uncaused rows at the head are the
  same 3 as at the base, with the same counts (`p5-input-approval` `cmd-ask`
  15, `edit-ask` 17, `after` 13), as at APPROVAL-READINESS-SETTLED; this unit
  touches no row of that scenario.

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at the
code head `09077f5f` (`src/` and `tests/` as committed; this receipt and the
parity sentence's one-phrase edit were in the working tree). Every step
exits 0, and `src/` and `tests/` were unchanged at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5569 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5606 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

5569 is APPROVAL-READINESS-SETTLED's round-1 gate count, 5564, plus its round
2's two tests and this unit's three (`git diff 67e40758 b3a19ede -- src tests`
and `git diff b3a19ede 09077f5f -- src tests` add two and three `#[test]`s
and remove none); 5606 is its 5601 plus the same five. No test here builds a
`Runtime`. Every commit's added lines were scanned for local paths before it
was made (`git diff --cached`).

## Parity (`docs/parity.md`) and the ledger

- The full-screen TUI row's approval sentence: "the two drafts are
  independent down to their undo histories and kill slots" is now "the two
  drafts are independent"; the draft edits that ask for a frame no longer
  list a yank; the yank clause is replaced by Tab's frame; and a new clause,
  **"The draft's keys are fx v0.0.16's"**, gives the set by route, the three
  removed and why, and what is still apart (Ctrl+Shift+A/B/E/F in a draft,
  DECODER-TILDE-HOME-END, APPROVAL-CTRL-J). `fx_src.sh` checks those
  citations together with the unit's added lines (run as its header says;
  `fx-src-check.txt`).
- The ledger's refs into `src/tui/shell.rs`, checked by content with
  APPROVAL-VERTICAL-SPELLINGS' `refs.py` (`python3 -I refs.py b3a19ede`, from
  the repo root): 17 checked, 0 wrong (`refs.txt`). The six `tools: {}
  advertised` refs moved 224 lines with the new tests, and `--fix`
  re-pointed them.

## Safety

No new string reaches the terminal. Nothing writes a file, runs a process or
reaches the network. The change takes keys away from the draft and adds one
hold. The one affirmative taken now that the base refused is an Enter read
with a removed key, and it answers the draft the last frame showed: the
removed key changes nothing, so that frame is current. A refusal stays
answerable at every moment.
