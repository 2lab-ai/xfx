# DECODER-TILDE-HOME-END receipt: rxvt's `CSI 7 ~`/`CSI 8 ~` and the modified tilde Home and End

fx v0.0.16 reads `CSI 7 ~` and `CSI 8 ~` (rxvt's Home and End) as Home and
End, and `CSI 1`, `4`, `7` or `8 ; <mod> ~` as a move to the line's edge (the
draft's under Ctrl, extending a selection under Shift). xfx's decoder read all
of them as nothing (`Action::Ignore`), in the composer, a question's field and
an approval's draft alike. APPROVAL-DRAFT-KEYS found the gap. Base `afd891af`
(train 13's head, branch `train/p5-1110b`).

| commit | what |
|---|---|
| `4140e1ae` | RED: the six decoder tests and the shell test, four strict shapes added to `a_sequence_is_a_key_only_in_the_exact_shapes_a_terminal_sends_one`, the scenario `p5-decoder-tilde-home-end`, `fx/`, `red/`, `fx_src.sh`, `fx-src.txt`, `pairs.sh`, `red-pairs.txt`, `red.txt` |
| `e90b1bba` | **the code head**: the decoder (`csi`, `modified_tilde` in `src/tui/input.rs`), the two entries of `what_upstream_resolves_to_a_key_xfx_does_not_have_is_ignored` that named `1;5~` and `4;3~` out, `cargo fmt` of the shell test, `docs/parity.md`, the ledger's moved refs, `refs.py`, `refs.txt` |
| `893999fb` | `xfx/`, `green-pairs.txt`, `mutants.py`; no `src/` change. **The gate ran here** |
| `e7beb46f` | `gate.txt`, `green.txt`, `mutants.txt`, `fx_src.sh` given the meta prefix's and stage 6's lines (and `fx-src.txt` again), one word of the parity sentence, this file; no `src/` change |
| `56a69778` | the sweeps (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`), this file's sweep section and table, and the post-gate scan line in `gate.txt`; no `src/` change |
| `cd18efdf` | **round 2** (the reviews of `56a69778`, no MUST): `modified_tilde`'s doc made true (below), the lock-bit Delete divergence recorded and pinned by a test, a test over every modifier field `2..=256`, `docs/parity.md`, `fx_src.sh`'s ranges (Delete's lines, `input_full_transcript_runtime.zig` to `:177`), `mutants.py` M9-M11; no behaviour change |
| the commit after | round 2's receipt: `round2.txt`, `mutants.txt`, `gate.txt`, `green.txt` (each with round 2 first), this file |

The source change outside the test modules is +19/-7 lines, four of them
code (the rest its doc comments): `git diff afd891af e90b1bba -- src`.

Captures: `fx/` is fx v0.0.16 (sha256 `8e6cda2e…`, `FX_AUTO_UPGRADE=0`;
`capture.sh` checked its bytes after the run); `red/` the base's binary (row
1 `x𝒇x v0.1.0-afd891a [debug]`, sha256 `8f236c03…`, built at `afd891af` with
a clean tree); `xfx/` the code head's (row 1 `x𝒇x v0.1.0-e90b1bb [debug]`,
sha256 `255c76c7…`, built after that commit). No `gwlog` step, so no request
body is in the receipt.

## What fx v0.0.16 does (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

**The decoder.** A digit after `CSI` starts the tilde stage
(`escape_parser.zig:583-588`), whose `~` reads the number (`:620-635`); a `;`
moves to the stage after it (`:606-611`), whose `~` reads the key and the
modifier, one more than the mask (`:694-719`; the bits, `:61-64`).

| spelling | bytes | fx decodes it | xfx at the base | xfx at the head |
|---|---|---|---|---|
| Home | `CSI 1 ~` | `.home` (`escape_parser.zig:628-629`) | Home | Home |
| rxvt's Home | `CSI 7 ~` | `.home` (`:629`) | nothing | **Home** |
| End | `CSI 4 ~` | `.end` (`:631`) | End | End |
| rxvt's End | `CSI 8 ~` | `.end` (`:631`) | nothing | **End** |
| Home with a modifier | `CSI 1`/`7 ; <m> ~`, `m` ≥ 2 | `composerMove(.draft_start` under Ctrl, else `.line_start`, extending under Shift`)` (`:699-703`; `composerMove`, `:66-71`) | nothing | **Home** |
| End with a modifier | `CSI 4`/`8 ; <m> ~`, `m` ≥ 2 | the same with `.draft_end`/`.line_end` (`:704-707`) | nothing | **End** |
| the modifier field empty, `0` or `1` | `CSI 1 ; 1 ~`, `CSI 7 ; ~`, ... | the bare key (`:712-714`; `if (param.* > 0) param.* - 1 else 0`, `:696`) | nothing | nothing (kept, below) |

fx's own test pins the bare five (`src/ui/input/runtime.zig:4774-4795`).

**What a modifier changes, surface by surface.** Every decoded action
carries a composer shortcut (`appendAction`, `terminal_action_decoder.zig:214-224`):
`.home` is `move(.line_start)` and `.end` `move(.line_end)`
(`shortcuts.zig:51-52`), and a composer move is itself (`:61`). So a bare
Home, an Alt- or Super-tilde Home (`.line_start`, no Shift) are one move
everywhere; Ctrl's is `.draft_start`, and Shift's extends a selection.

- **The composer** takes the shortcut (`app_input_runtime.zig:999-1002`):
  `.line_start` is the caret line's start, `.draft_start` offset 0
  (`moveInputCursor`, `:296-309`; `horizontal_navigation.zig:29-34`).
  `.home`/`.end` are `unreachable` in its switch (`app_input_runtime.zig:1025-1026`)
  because the shortcut took them. **Only here does the modifier matter.**
- **A question's field**: `.line_start, .draft_start => .cursor_home` and the
  same for End (`src/ui/input/runtime.zig:162-163`); Shift matters only on
  `.visual_up`/`.visual_down` (`:183-190`). Every spelling is the field's Home.
- **An approval's draft**: the same fold (`src/ui/input/runtime.zig:82-83`),
  taken while the amendment is open (`input_approval_runtime.zig:79-88`); at a
  choice neither `.home` nor a move is a case (`:128`).
- **The full transcript** names neither (`keyForAction`,
  `input_full_transcript_runtime.zig:156-177`), so both reach the composer;
  **the compact menus** return on both (`app_input_runtime.zig:2259-2263`).
- No other reader: the `git grep` at the end of `fx-src.txt` finds `.home`
  and `.end` read only by the decoder, `fromEscapeAction`, the composer's
  `unreachable` arm and fx's test of a question's field (the other hits are a
  different enum or a selection's `.end`).

## What xfx does now (`git diff afd891af e90b1bba -- src`)

- `csi`'s tilde arm reads `7` as Home and `8` as End beside `1` and `4`.
- `modified_tilde` reads `1`/`7` with any modifier field from `2` to `256` as
  `cursor(b'H', mods)` and `4`/`8` as `cursor(b'F', mods)`: Home or End
  whatever is held, as `cursor`'s `H` and `F` rows read it. (Round 2: this is
  not "the key `CSI 1 ; <mod> H` is" -- `CSI 1 ; 1 H` is Home and `CSI 1 ; 1 ~`
  nothing, `CSI 1 ; 17 H` nothing and `CSI 1 ; 17 ~` Home, `modifier` taking
  `1..=16` and `key_modifiers` `1..=256` -- and the two spellings are not one
  reading in fx either: under Super its `H`/`F` go to the draft's edge,
  `escape_parser.zig:79-80`, and its tilde to the line's, `:701`.)
- No new action, and no surface changes: the decoder now emits the
  `Action::Home`/`Action::End` every surface already handles (the composer's
  `act`, a question's field, the approval's draft, the menus' bound keys in
  `src/tui/shell.rs`), where it emitted `Action::Ignore`. `src/tui/shell.rs`
  gains a test and nothing else.

**Kept apart from fx, each recorded in `docs/parity.md` (the full-screen TUI row):**

1. **Ctrl with Home or End is the caret line's edge here, the draft's in
   fx's composer.** Only a draft of more than one line shows it. The same
   was already true, and unrecorded, of `CSI 1 ; 5 H`/`F`, kitty's and the
   keypad's Ctrl-Home, and Super-Home in the `H`/`F` and kitty spellings
   (`escape_parser.zig:79-80`, `:95-96`, `:134-167`); in the tilde spelling
   Super is the line's edge (`:701`, `:705`). Owner proposed:
   COMPOSER-SUPER-ARROWS, which needs the same draft-edge move.
2. **Shift does not extend a selection**: the composer has none
   (COMPOSER-SELECTION-EXTEND); the cursor-table sentence now names the tilde
   spellings.
3. **The modifier field `1` (also empty or `0`) is no key here**, as for
   Delete and the page keys, where fx reads the bare key (`:696`, `:698`,
   `:712-718`). The refusal is xfx's own, not borrowed from fx (fx refuses
   `CSI 1 ; 1 Z`, `:723`, but not the tilde keys), and the same decoder reads
   `CSI 1 ; 1 H` as Home; it is kept so the seven tilde codes (1, 3, 4, 5, 6,
   7, 8) follow one rule until TILDE-EMPTY-MODIFIER. It is pinned by
   `a_sequence_is_a_key_only_in_the_exact_shapes_a_terminal_sends_one`
   (`CSI 1 ; 1 ~`) and by every modified test's `; 1` case, and mutant M7
   (fx's reading of `; 1`) is killed by them.
4. **Delete with Caps Lock or Num Lock alone** (`CSI 3 ; 65 ~`, `; 129 ~`,
   `; 193 ~`) is Delete here and nothing in fx, which hands
   `forwardDeleteKeyAction` the raw modifiers (`:694-698`, `:176-180`).
   Older than this unit (round 2 found it; the parity row said "not looser
   anywhere"); recorded, pinned by
   `delete_with_a_lock_bit_alone_is_delete_here_and_nothing_upstream`, owner
   TILDE-EMPTY-MODIFIER. No behaviour change here.

## Other tilde spellings fx accepts that xfx's decoder lacks

fx's `~` finals are the three stages above (`:620-635`, `:694-719`) and
`CSI <a> ; <m> ; <k> ~` (`:746-752`). Against xfx's decoder:

| spelling | fx | xfx | scope |
|---|---|---|---|
| `CSI 3`/`5`/`6 ; 1 ~` (and `; ~`, `; 0 ~`) | Delete, Page Up, Page Down (`:698`, `:715-716`) | nothing | not Home/End: the `; 1` rule above; TILDE-EMPTY-MODIFIER (proposed), if fx's reading is wanted. Taking out `modified_tilde`'s `mask == 0` guard (mutant M7) gives fx's reading for `; 1` only: an empty field and `0` stop at `key_modifiers`/`decimal` first, which that unit would change too |
| a code with a leading zero (`CSI 07 ~`, `CSI 03 ; 5 ~`) | the number (`:600-602`) | nothing | recorded already ("one canonical decimal spelling per field"); this unit adds `07~`, `08~`, `8;05~` to the pinned shapes |
| a modifier past 256 (`CSI 1 ; 300 ~`) | a move (the field is a `u16`, `:696`, `:699`) | nothing (`key_modifiers`, `1..=256`) | the same grammar rule, now in `docs/parity.md` too (round 2); no terminal sends one |
| Delete with a lock bit alone (`CSI 3 ; 65 ~`) | nothing (`:694-698`, `:176-180`) | Delete | the one place xfx is looser; kept 4 above |
| `CSI <a> ; <m> ; <k> ~` with `a` not `27` | the key `k` (`:746-752`; the first field is overwritten) | nothing | recorded in `tilde_report`'s doc already |
| behind a meta prefix (`ESC ESC [ 7 ~`) | the same key: `ESC ESC` sets the meta bit (`:552-553`) and neither tilde stage reads it (`:620-635`, `:694-719`) | Escape, then Home | META-PREFIX-DECODE's (proposed by APPROVAL-HORIZONTAL-KEYS) |
| `CSI 2 ~` (Insert) | nothing (`:634`) | nothing | -- |

## RED (`red.txt`, `red-pairs.txt`)

- At the base with only the tests added: the six decoder tests and the shell
  test fail, each on `[Action(Ignore)]` where Home or End was wanted, and the
  shell test on `"abc defX"` where `"Xabc def"` was (`CSI 7 ~ was not Home`).
  The two existing tests in the run pass (the strict shapes were nothing at
  the base too).
- The pair: 8 of 9 snaps differ, each in row 3, the composer (the base's
  letters all land at the end: `┃ abc defABCDEFGH`); `typed`, before any key,
  is identical.

## GREEN (`green-pairs.txt`, `green.txt`)

- **9 of 9 pairs identical** (`cells.py`, row 1 left out), the last
  `┃ GECAabc defBDFH` on both: Home and End bare (`7~`, `8~`), Ctrl-Home
  `1;5~`, Alt-End `4;3~`, Super-Home `7;9~`, Ctrl-End `8;5~`, Alt-Home
  `1;3~`, Ctrl-Super-End `4;13~`. The draft is one line, where fx's line and
  draft edges are one place; Shift is left out because fx's selection would
  be replaced by the next letter. The multi-line Ctrl case is recorded
  (above), not captured.
- `green.txt`: the unit's tests in the gate's run at `893999fb`.

**Can a scenario inject `ESC [ 7 ~`?** Yes: `key -H 1b 5b 37 7e`. The
scenario's `key` line is `tmux send-keys -t cap <rest>` (`scripts/fxcap/capture.sh`),
and `-H` sends each hex byte as given, as `p5-approval-vertical-keys` already
does. The RED pair shows the bytes arrive unchanged: fx moved on them and the
base's xfx read them as nothing; tmux's own named keys never send `7~`/`8~`.

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` of the branch at `e90b1bba` (its remote removed)
under `$TMPDIR/mut/`, one at a time, the file restored after each, under the
suite lock. **8 of 8 killed**, the clone's tree clean after the run:

| mutant | killed by |
|---|---|
| M1 `CSI 7 ~` is not Home | `rxvts_home_csi_7_tilde_is_home`, the shell test |
| M2 `CSI 8 ~` is not End | `rxvts_end_csi_8_tilde_is_end`, the shell test |
| M3 `CSI 1 ; <m> ~` is not Home | `home_with_a_modifier_csi_1_mod_tilde_is_home`, the shell test |
| M4 `CSI 4 ; <m> ~` is not End | `end_with_a_modifier_csi_4_mod_tilde_is_end` |
| M5 `CSI 7 ; <m> ~` is not Home | `rxvts_home_with_a_modifier_csi_7_mod_tilde_is_home` |
| M6 `CSI 8 ; <m> ~` is not End | `rxvts_end_with_a_modifier_csi_8_mod_tilde_is_end`, the shell test |
| M7 the modifier field `1` is the bare key (fx's reading of `; 1`; empty and `0` stay refused) | the strict-shapes test, `ctrl_o_and_the_page_keys_…`, the modified tests' `; 1` cases |
| M8 Home and End swapped under a modifier | the four modified tests |
| M9 (round 2) Home and End only for the fields `2..=16`, the `H`/`F` table's range | the new loop and three modified tests (the sample's 17 and 65); the list is cut at four |
| M10 (round 2) Home and End only for the fields `2..=255` (256 dropped) | `every_modifier_field_from_2_to_256_on_the_tilde_home_and_end_is_the_key` alone: no sample names 256 |
| M11 (round 2) Delete with a lock bit alone is nothing (fx's reading) | `delete_with_a_lock_bit_alone_is_delete_here_and_nothing_upstream` |

**Round 2, at `cd18efdf`: 11 of 11 killed** on a fresh clone, the clone's tree clean after the run (`mutants.txt`'s first block); M1-M8 again, with the new loop among the killers of M3-M6. M10 is the range mutant only the loop kills.

## Sweep (`sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`)

`scripts/fxcap/sweep.sh --only composer-keys --only composer-wrap --only
filepicker-keys --only question-field --only 'p5-approval-*' --only
p5-decoder-tilde-home-end` at 40x120, `--jobs 4`, this branch's scenarios and
ledger, once with the base's binary (`v0.1.0-afd891a`) and once with the code
head's (`v0.1.0-e90b1bb`), both in one suite-lock slot: every scenario that
sends Home, End, `C-a`/`C-e` or the approval's and question's edit keys
through the decoder. APPROVAL-VERTICAL-SPELLINGS' `cmp_sweeps.py`
(unchanged, run from its folder) compares them pair by pair:

- **base 37 / 104, head 45 / 104, drops 0.**
- The eight that change are this unit's scenario's eight key snaps, from
  `unresolved-defect` with 2 uncaused rows each to `normalized-identical`
  (`typed` passes at both).
- No other pair changes class, causes or uncaused-row count; the head has 0
  pairs with uncaused rows (the 59 unresolved are the ledger's causes:
  `approval-grant-scope` on the approval scenarios, and composer-keys' 3).

## Gate (`gate.txt`)

The brief's gate, once, under the suite lock with a short `TMPDIR`, at
`893999fb`, whose `src/` is the code head's (`gate.txt` carries the empty
`git diff --stat`). Every step exits 0 and the tree was the same at the end:

| step | result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | clean |
| `cargo test --locked --no-fail-fast` | 5692 passed, 0 failed, 7 ignored (15 result lines) |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5729 passed, 0 failed, 7 ignored (15 result lines) |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | exit 0 each |

**Round 2, at `cd18efdf`** (`gate.txt`'s first block): every step exits 0
again: test 5694 passed, 0 failed, and test-fi 5731 passed, 0 failed, 7
ignored each -- round 1's 5692 and 5729 plus the two new tests
(`git diff 56a69778 cd18efdf -- src` adds two `#[test]`s and removes none).

`git diff afd891af e90b1bba -- src` adds seven `#[test]`s and removes none
(the six decoder tests and the shell test). `green.txt` lists the unit's
tests from both runs, with the two decoder tests it edits and one of M7's
killers beside them.

## The ledger's refs (`refs.py`, `refs.txt`)

The shell test sits above the line six ledger entries cite
(`"  tools: {} advertised ({},",`), which moved from `src/tui/shell.rs:42431`
to `:42464`. `refs.py` is APPROVAL-VERTICAL-SPELLINGS' (content-compared,
keyed by entry), unchanged but for its docstring; `--fix` re-pointed the six,
and `refs.txt`, its output at the code head, reads 16 checked, 0 wrong.

## `docs/parity.md`

The full-screen TUI row: the decoder sentence after "The grammar is stricter
than upstream's" now says the tilde Home and End are fx's -- bare `1`/`7`
and `4`/`8`, and with a modifier the key `CSI 1 ; <mod> H`/`F` is -- on every
surface the bare key reaches, and names the two shapes still refused; the cursor-table sentence names the modified tilde spellings;
and a new sentence after COMPOSER-SUPER-ARROWS records Ctrl's draft edge.

**Join note**: APPROVAL-DRAFT-KEYS' parity text (train 14, not at this base)
says xfx's decoder reads these spellings as nothing and names this unit as
the proposed owner; that clause is retired when the two meet.
