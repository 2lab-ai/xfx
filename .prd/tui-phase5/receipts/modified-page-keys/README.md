# MODIFIED-PAGE-KEYS receipt: a page key held with a modifier, as fx reads it

fx v0.0.16 decodes a page key held with a modifier to a composer move to a
page, not to its `.page_up`/`.page_down`, and its full transcript pages on
the bare key alone. xfx read every held page key as the bare one, so it
paged the full transcript where fx's stays put and moves the composer's caret
behind the screen instead. TILDE-EMPTY-MODIFIER (closed at `fab7a217`, this
unit's base) found the gap, widened it to its new spellings, and named this
unit the owner (its README, "Left apart" 2; its `probe/frames.txt`).

| commit | what |
|---|---|
| `4c3ab703` | RED: the two variants `Action::ComposerPageUp`/`ComposerPageDown` with the arms the exhaustive matches need and **no producer and no fold**; the decoder pins of a held page key flipped; the per-surface tests; `delete(62)`/`delete(63)`; the scenario `p5-modified-page-keys` |
| `0c10d6ee` | **the code head**: `tilde_key` and `kitty_key` make the two, `folded` folds them back, the docs of `Action::PageUp`, `tilde_key` and `kitty_key`; three test comments name the file of a bare citation (no assertion change) |
| `60043e55` | `docs/parity.md`, the ledger's refs, this folder but its mutant, gate and citation-check records; no `src/` change. **The gate ran here** |
| `8b488ca4` | `mutants.txt`, `gate.txt`, `green.txt`, `fx-src-check.txt`, this file's mutant and gate sections; no `src/` change. Round 1's head |
| `f5e9ea72` | **round 2's code head**: tests only (the modal drafts, the page size behind the screen) and the scenario pressing each held key on its own; no non-test change |
| the commits after | round 2's record: `docs/parity.md` (the held-arrow gap, the `/mcp` exception), the ledger's `fulltranscript-response-header` cause, this folder (`round2.txt`); then its mutants and gate |

The source change outside the test modules is +63/-19 lines over the three
files (`git diff -U0 fab7a217 0c10d6ee -- src`, lines above each file's
`mod tests`).

## fx's rule, surface by surface (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The decoder.** The tilde page keys: one field is `.page_up`/`.page_down`
(`escape_parser.zig:632-633`); two fields with no modifier (`0`, `1`, empty)
the same (`escape_parser.zig:696`, `:715-716`); with any bit of the field
held -- a lock bit and a bit past eight counted (`escape_parser.zig:699`) --
`composerMove(.page_up/.page_down, modifiers)` (`escape_parser.zig:708-709`),
whose `extend_selection` is Shift (`escape_parser.zig:66-71`). The keypad's
(`57421`/`57422`): the report's modifiers lose the lock bits first
(`escape_parser.zig:186-187`); with only Shift, Alt, Ctrl or Super among them
(`escape_parser.zig:119`, `:191`) the key is `pageKeyAction`
(`escape_parser.zig:169-173`, reached at `:198-199`): none is the bare key,
any is the composer move; Hyper or Meta is no keypad key, and nothing else
of the report names the code (`escape_parser.zig:265`). A tilde's third
field is the same report (`escape_parser.zig:746-752`).

**What each is to the composer.** The bare key's composer shortcut is the
same move with no selection (`shortcuts.zig:53-54`, `:7-9`): the two differ
only in the decoded action, and in Shift's `extend_selection`.

| surface | the bare key | held with a modifier |
|---|---|---|
| the full transcript | pages (`keyForAction`, `input_full_transcript_runtime.zig:165-166`; `routeKey`, `:192-195`) | not its key (`input_full_transcript_runtime.zig:167-170`, `routeAction`, `:47-52`): the event goes on (`app_input_runtime.zig:930`) to the composer behind the screen (`app_input_runtime.zig:997-1001`) |
| sign-in or key entry | swallowed (`app_auth_runtime.zig:730-743`, `else => true`) | the same |
| a question | nothing: no choice move and no field edit for a page move (`src/ui/input/runtime.zig:178-197`), and the switch's `else` (`input_question_runtime.zig:181-200`) | the same |
| an approval, at its choice and in its amendment | nothing: no draft edit for a page move (`src/ui/input/runtime.zig:75-96`), and the switch's `else` (`input_approval_runtime.zig:90-128`) | the same |
| `/statusline`, the info panel, `/usage`, `/workspace` | nothing (`routeCompactCommandMenuEscapeAction`, `app_input_runtime.zig:2259-2263`) | the same |
| the lists (`/mcp`, `/settings`, `/help`, the model, effort, skills, resume and fork menus, the sign-in picker, the file, model and provider columns, the slash menu) | a page of the composer's row cap (`routeComposerVertical`, `input_completion_runtime.zig:215-233`, `:255-260`; `routeNonSlashPickerMove`, `:395-422`) | the same move (`app_input_runtime.zig:310-327`) |
| the composer's caret | a page of rows, never a history step (`input_completion_runtime.zig:235-270`) | the same, and with Shift held the selection extends (`applyTargetWithSelection`, `input_completion_runtime.zig:262-268`) |

**The oracle** (`fx/`, the scenario below), each held key on its own: in
fx's full transcript `full == full-ctrl-pgup` (`CSI 5;5~`), the bare
`CSI 5~` pages it, and after Escape an `X` typed lands on the draft's first
row (`typed`, `┃ oneX`): the tilde key moved the caret behind the screen up
a page. Then `full2 == full2-kp-ctrl-pgdn` (`CSI 57422;5u`), and a `Y`
typed after Escape lands on the last row (`typed2`, `┃ fourY`): the keypad
key moved it down a page.

## What xfx does now (`git diff 4c3ab703 0c10d6ee -- src`)

- `tilde_key`: `5`/`6` with any bit of the field held are
  `ComposerPageUp`/`ComposerPageDown`; with none, `PageUp`/`PageDown`.
- `kitty_key`: the keypad page keys with Shift, Alt, Ctrl or Super held are
  the same two; a lock bit alone is masked off first and stays the bare key;
  Hyper and Meta were already nothing and are unchanged (pinned).
- `folded` (`src/tui/shell.rs`) makes the two `PageUp`/`PageDown`. The full
  transcript reads the event before the fold (`unspelled`), so its `key`
  answers `Unbound` and the event goes on, as fx's does; the question and
  the approval read it before the fold too and swallow it, as they swallow
  the bare key; every list and the composer read the folded key, so they
  page exactly as before. `act` takes the two as the page keys too, the way
  it takes the other kept-apart spellings, for a route that skipped the fold.
- **Decision (reversible): no Shift in the variants.** No xfx surface reads
  it: the composer has a selection (`Super+A`) but no move extends it, which
  `docs/parity.md` already records for every selection-extending move, a
  Shift on a page key named (owner COMPOSER-SELECTION-EXTEND).
  `Action::ComposerLeft` drops Shift the same way.

## RED (`red.txt`, `red-pairs.txt`)

- The tests at `4c3ab703` over the base's code: **7 fail**, each on the
  base's reading (`red.txt`): the new decoder test and the four flipped
  decoder pins (`[Action(PageUp)]` where `ComposerPageUp` is wanted), the
  fold table (`ComposerPageUp` not folded) and the full-transcript test
  (`CSI 5;5~` paged the screen). The other 112 of the run pass, among them
  the unit's guards: the question and approval test, the nine `at_41_rows`
  page tests with their held spellings, and the CSI-bound test with
  `delete(62)` and `delete(63)` (`red.txt`'s last block).
- The scenario on the base's binary (`v0.1.0-fab7a21`, `red/`, round 2's
  scenario): `full` and `full-ctrl-pgup` differ (the tilde key paged the
  screen), `typed` is `┃ fourX` where fx's is `┃ oneX`, and `typed2` is
  `┃ fourXY` where fx's is `┃ fourY` (the keypad key moved neither the
  screen, already at its tail, nor the caret): **3 of 10** pairs identical
  (`red-pairs.txt`).
- Round 2's tests over the base's code (`round2.txt`): the full-transcript
  test, now on a sixteen-row draft, fails at its first held key, as it did;
  the modal-draft cases pass there (each pins what the base already did).

## GREEN (`green-pairs.txt`, `green.txt`)

- The scenario on round 2's code head's binary (`v0.1.0-f5e9ea7`, `xfx/`;
  its non-test code is `0c10d6ee`'s):
  - **the five inline snaps identical**, `answered`, `draft`, `closed`,
    `typed` (`┃ oneX`) and `typed2` (`┃ fourY`), cell by cell (`cells.py`,
    row 1 left out);
  - the full-transcript snaps differ across the sides before any key: each
    side's own record times, which `cells.py` does not mask, and a
    `· Response` header fx's tail page has and xfx's document does not, so
    the page stands one row apart (the sweep's 2 uncaused rows, Sweep
    below); at the head, `full-pgup` differs as the ledger's
    `session-record` cause says. So they are read within each side: on
    both, `full == full-ctrl-pgup == full2 == full2-kp-ctrl-pgdn` and
    `full-pgup` moved, and the differing-row list of each held-key snap is
    `full`'s (rows 2-32 and 35). On the base side the tilde key moved the
    page, and the list gained rows 33 and 36.
- `green.txt`: the unit's tests in the gate's run.

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` (its remote removed) under `$TMPDIR/mut/`, one
at a time, the file restored after each, under the suite lock, each run the
unit's tests (`tui::input::tests` and the shell tests above). Round 1, at
`0c10d6ee`: **9 of 9 killed**. Round 2, at `f5e9ea72`, the nine again and
three new: **12 of 12 killed**, the clone's tree clean after the run
(`mutants.txt`, round 2's block first; each names the first six failing
tests):

| mutant | killed by |
|---|---|
| M1 a held tilde page key is the page key (the base's reading) | the decoder's held-key test, the four flipped pins, the full-transcript test |
| M2 a held keypad page key is the page key (the base's reading) | the decoder's held-key test, the page-keys table, the full-transcript test |
| M3 the keypad counts a lock bit alone as held | the decoder's held-key test (`;65u`, `;129u`, `;193u`), the full-transcript test (`57422;65u`) |
| M4 the tilde page key reads only the field's low byte | the past-256 test, the held-key test, the full-transcript test (`6;257~`) |
| M5 the tilde page key masks the lock bits off | the past-256 test, the held-key test, the lock-bit test, the full-transcript test (`5;65~`) |
| M6 no fold | the fold table and the `at_41_rows` list tests, 9 failing in all (the held key paged no list; the composer's page test passes, `act` pages it) |
| M7 the fold makes a held page key the arrow | the fold table and the `at_41_rows` tests, 11 failing in all |
| M8 the fold runs before the full transcript reads the event | the fold table, the full-transcript test |
| M9 the full transcript pages on the held key too | the full-transcript test |
| M10 (round 2) a question's freeform field takes a held page key as Home | the question and approval test alone (its freeform case) |
| M11 (round 2) an approval's amendment takes a held page key as Home | the question and approval test alone (its amendment case) |
| M12 (round 2) a page behind the screen is one row short of the composer's cap | the full-transcript test (its sixteen-row draft) and `at_41_rows_the_composer_is_capped_at_19_rows_and_a_page_is_19_of_them` |

Not mutated: `act`'s two arms that take the held keys as the page keys. They
are unreachable -- `act`'s one caller outside the tests reads the event after
`folded` -- and they are there because `act`'s match is exhaustive and names
every action on purpose, delegating each kept-apart spelling to the key it
folds to, the convention its other such arms follow (`src/tui/shell.rs`,
the comment over `Action::SelectUp | Action::ComposerUp | ...` in `act`).

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only 'fulltranscript*' --only 'p5-page-keys*'
--only 'composer*' --only 'p5-tilde*' --only p5-modified-page-keys` at
40x120, `--jobs 2`, this branch's scenarios, once with the base's binary
(`v0.1.0-fab7a21`) and once with the code head's (`v0.1.0-0c10d6e`), both in
one suite-lock slot, then both rescored (`--rescore`) with this branch's
ledger once its refs were re-pointed (below). Six scenarios, 64 pairs:
`composer-keys`, `composer-wrap`, `fulltranscript`, `fulltranscript-notice`,
`p5-tilde-empty-modifier` and this unit's. **`p5-page-keys*` selects nothing
at `fab7a217`**: that family is on `fix/p5-page-keys-settings-column`, which
this branch does not stack on. APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py`
(unchanged, run from its folder) compares them pair by pair:

- **base 44 / 64, head 45 / 64, drops 0.**
- The three that change are this unit's scenario's: `typed` from 4
  uncaused rows (`┃ fourX` against `┃ oneX`) to identical, and the two
  held-key snaps from 26 uncaused rows (the base paged the screen to the
  head, where fx's session record stands taller) to the 2 of `full` itself.
- Those 2, on `full` at both and on the held-key snaps at the head, are
  older than this unit and not its: fx's tail page shows a second
  `· Response` header (row 32) that xfx's document does not have, so the
  page stands one row apart. The sweep finds the same unmatched header on
  `p5-transcript-md`'s `full` (`train-12/sweep-head.md`, that scenario's
  row); the ledger names no cause for it.
- No other pair changes class, causes or uncaused-row count.

**Round 2** (`sweep-r2-base.md`, `sweep-r2-head.md`, `sweep-r2-compare.txt`):
round 2's scenario with `p5-transcript-md` and `-light`, the same two
binaries' kind (base `v0.1.0-fab7a21`, head `v0.1.0-f5e9ea7`), round 2's
ledger, one slot: **base 9 / 20, head 11 / 20, drops 0**. `typed` and
`typed2` are identical at the head (4 uncaused rows each at the base), and
`full-ctrl-pgup` drops the base's 21 uncaused rows. The head's three pairs
left with uncaused rows are the `full-pgup` snaps, the document's head:
xfx's page reaches one row further (`A list that starts at nine:`), under
the `session-record` cause, not the `· Response` header.

## Gate (`gate.txt`)

**Round 2**, at `ceab580d`, whose `src/` is round 2's code head
`f5e9ea72`'s, in one normal slot (`gate.txt`'s first block): every step
exits 0; test 5705 passed, 0 failed, 7 ignored; test-fi 5742 passed, 0
failed, 7 ignored (round 2 extends tests and adds no `#[test]`). No
catalog-wait failure in this run. `green.txt`'s first block: the unit's 18
tests in both suites, 36 of 36 ok.

Round 1: the brief's gate, once, under the suite lock with a short `TMPDIR`, at
`60043e55`, whose `src/` is the code head's (`gate.txt` carries the empty
`git diff --stat`). Every step exits 0 and the tree was the same at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5705 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5742 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh`, `check-no-local-paths.sh` | exit 0 each |

TILDE-EMPTY-MODIFIER's round-2 gate read 5699 passed and 3 failed (5702)
and 5739; this unit adds three `#[test]`s (the decoder's held-key test, the
full-transcript test, the question and approval test) and removes none.
`green.txt`: the unit's 18 tests in both suites of this run, 36 of 36 ok.

## The citations (`fx-src-check.txt`)

`fx_src.sh` over this file and the unit's added lines (`src`, `tests`, the
scenario, and `docs/parity.md`'s added words), the command in the file:
every citation falls inside a range of `fx-src.txt`, in round 2 as in
round 1 (round 2's block first).

## The ledger's refs (`refs.py`, `refs.txt`)

This unit adds lines to `src/tui/shell.rs` above nine of the ledger's refs
into it (`session-record`, `transcript-page-offset`,
`toolgroup-ctrlo-record-short`, `safe-page-edge`, `cancel-page-edge`,
`full-transcript-shift` at `"  tools: {} advertised ({},",`;
`answer-drawn-wider-named`; `at-resize-band-at-the-bottom` and
`resize-document-repaint` at `fn resize`). TILDE-EMPTY-MODIFIER's `refs.py`
(docstring only changed) read 16 checked, 9 wrong against the working tree;
`--fix` re-pointed them, and it now reads **16 checked, 0 wrong**
(`refs.txt`). `docs/parity.md`'s rows stay on lines 237 and 238, where the
ledger's `source` cites them.

## `docs/parity.md`

- The full-screen TUI row: the page-key sentence says the full transcript
  pages on the bare key only, a held one (any bit on the tilde key, Shift,
  Alt, Ctrl or Super on the keypad's) is fx's composer move and goes on to
  the composer behind it, here as there, through `ComposerPageUp`/
  `ComposerPageDown` and `folded`; the lists and the caret take the two
  alike; the question and the approval take neither. MODIFIED-PAGE-KEYS is
  no longer named as an owner.
- The full transcript row: the page keys are the bare ones, and a held page
  key is among the keys that go to the composer behind the screen.
- Round 2 (opus NIT 3): the full-screen TUI row's "here as there" names
  `/mcp`'s menu as the exception: under the full transcript it takes no key
  here (`offer_to_mcp`, xfx's recorded choice), where fx's pages its list
  behind the screen (`routeMcpHostMenuMove`, `input_completion_runtime.zig:396`).
- Round 2 (opus SHOULD 1): the held-arrow gap, recorded and not fixed, owner
  MODIFIED-ARROWS-SCREENS (queued). fx's full transcript binds the plain
  arrows only (`keyForAction`, `input_full_transcript_runtime.zig:156-176`);
  every Super, Alt, Ctrl and Shift arrow is a composer move
  (`modifiedArrowAction`, `escape_parser.zig:73-108`), and Ctrl-B and Ctrl-F
  are bytes its screen does not bind (`keyForByte`,
  `input_full_transcript_runtime.zig:147-154`; `app_input_runtime.zig:872`),
  so all of them move the caret behind the screen. xfx's screen reads the
  event `unspelled`, so Ctrl-, Alt- and Super-Up and -Down scroll it, and
  Shift- and Super-Left and -Right, Ctrl-B and Ctrl-F are held (the full
  transcript row). xfx's question reads it `unspelled` too: at a choice
  Ctrl-, Alt- and Super-Up and -Down move the choice and Shift- and
  Super-Left go back a question, where fx's question binds a composer move
  at a choice only with Shift on Up and Down (`src/ui/input/runtime.zig:178-197`)
  and leaves the rest to its switch's `else`
  (`input_question_runtime.zig:181-200`) (the `ask_user_question` row).

## Carried from TILDE-EMPTY-MODIFIER's review (gpt6 r2 NIT)

`a_tilde_sequence_past_the_csi_bound_is_read_as_every_csi_is` adds
`delete(62)` → `[Ignore]` and `delete(63)` → `[Ignore, Text('~')]`: 63 and
64 parameter bytes, the two neighbours of the discard's release. Both pass
at the base too (`red.txt`): coverage, not a behaviour change.

## The scenario `p5-modified-page-keys` (`scripts/fxcap/scenarios/`)

p5-transcript-md's answer (a document taller than the screen) under
`TZ=UTC`; a four-row draft typed with `C-j`. Round 2 presses each held key
on its own (opus NIT 2): Ctrl-O; `key -H 1b 5b 35 3b 35 7e` (Ctrl-PgUp,
tilde); the bare PgUp; Escape; `X`; Ctrl-O; `key -H 1b 5b 35 37 34 32 32 3b
35 75` (Ctrl-PgDn, keypad); Escape; `Y`. Shift is left out: fx's would leave
a selection on screen that xfx's composer does not extend (above). The three
capture runs (`captures.txt`): fx v0.0.16 by its sha guard, the base's
binary and round 2's code head's, each built with a clean `src/` at its
commit. Round 1's captures, of the scenario before round 2, are replaced.

## Round 2 (`round2.txt`)

The reviews of `8b488ca4` (gpt6 no MUST; opus APPROVE), item by item; no
behaviour change.

1. **The held arrows** (opus SHOULD 1): recorded in `docs/parity.md`'s full
   transcript row and the `ask_user_question` row, owner
   MODIFIED-ARROWS-SCREENS (above); `fx_src.sh` reads
   `escape_parser.zig:73-108`, `input_full_transcript_runtime.zig:147-154`
   and `app_input_runtime.zig:840-894` for it.
2. **The `· Response` header** (opus SHOULD 2): the ledger's cause
   `fulltranscript-response-header`, kind `unit`, owner
   FULLTRANSCRIPT-RESPONSE-HEADER (queued), over the tail-page snaps of
   `p5-modified-page-keys` and `full` of `p5-transcript-md` and `-light`:
   fx's second header row and xfx's blank first row. Checked
   (`response-cause-check.txt`): round 2's sweeps rescored with and without
   it; it loads, and at the head it takes those two rows on the six
   tail-page snaps (`p5-modified-page-keys` `full`, `full-ctrl-pgup`,
   `full2`, `full2-kp-ctrl-pgdn`; `p5-transcript-md` and `-light` `full`)
   and nothing else. At the base it also takes four blank fx rows of the
   header's block on `full-ctrl-pgup`, which the base had paged: the
   ledger gives a blank row its block's cause.
3. **Tests** (gpt6 SHOULD; opus NITs 2, 4, 5): the question and approval
   test opens a two-line draft in the amendment and in the freeform field,
   asserts the caret (`cursor()`) after each key, and types `Z` after them
   all: the `Z` ends the draft each answer carries. The full-transcript test
   pages a sixteen-row draft at 24 rows, so the page is pinned at eleven
   rows (`inputRowLimit`, `input_presentation.zig:252-255`). The scenario
   presses each held key on its own.
4. **Wording** (opus NITs 1, 3): `act`'s arms (Mutants), and `/mcp` as the
   exception to "here as there" (`docs/parity.md`).

