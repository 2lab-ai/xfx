# TILDE-EMPTY-MODIFIER receipt: the rest of the tilde grammar, as fx reads it

fx v0.0.16 reads every field of `CSI ... ~` as a number. xfx's decoder read
each field only in the one spelling a terminal sends, so it refused `; 1`, an
empty field, `0`, a leading zero, a modifier past 256 and a third field under
anything but `27`, and it took the lock bits out of Delete's mask, where fx
reads them. DECODER-TILDE-HOME-END (closed at `ebb04642`, this unit's base)
recorded the gap and named this unit its owner.

| commit | what |
|---|---|
| `074f5113` | RED: one test per spelling class that changes, the lock-bit test renamed and flipped, the old pins of the strict reading flipped, the scenario `p5-tilde-empty-modifier` |
| `ce4f9c6b` | **the code head**: `tilde_report`, `tilde_field`, `tilde_key` and `forward_delete` in `src/tui/input.rs`, and the docs of `csi` and the module; no test change |
| the commits after | this folder, `docs/parity.md`, DECODER-TILDE-HOME-END's README (one word); no `src/` change |

The source change outside the test modules is +107/-88 lines
(`git diff 074f5113 ce4f9c6b --stat -- src`); every hunk is above `mod tests`.

## fx's tilde grammar (`fx-src.txt`, written by `fx_src.sh`: every line read with `git show 481104fd:<path>`)

A digit after `CSI` starts the first field (`escape_parser.zig:583-589`), and
every digit of every field goes through `appendCsiDigitSaturating`
(`escape_parser.zig:19-23`): a `u16` that stops at 65535, a leading zero
nothing. A `;` hands the field on and starts the next at `0`
(`escape_parser.zig:606-611`, `:668-674`), so an empty field is `0`.

- **One field**, `~` (`escape_parser.zig:620-635`): the paste markers when the
  field is three digits and 200 or 201 (`:625-626`), else `1, 7 => .home`,
  `3 => .delete_next`, `4, 8 => .end`, `5 => .page_up`, `6 => .page_down`,
  anything else nothing.
- **Two fields**, `~` (`escape_parser.zig:694-718`): `modifiers` is one less
  than the second field, `0` for `0` or empty (`:696`). Delete hands them,
  every bit, to `forwardDeleteKeyAction` (`:698`; `:176-180`: Super kills to
  the line's end, Alt or Ctrl deletes the word to the right, none is Delete,
  anything else nothing). With any bit set the others are a composer move:
  the line's edge, the draft's under Ctrl (`:699-707`), a page (`:708-709`);
  with none, the bare key (`:712-718`).
- **Three fields**, `~` (`escape_parser.zig:746-752`):
  `kittyUnicodeKeyAction(third, second - 1)`, the first field overwritten;
  that function masks the modifiers to their low six bits (`:186-187`). A
  fourth field is nothing (`:754`).

### The table: each spelling class against each key code

`1/7`, `4/8` and `5/6` are one arm each upstream, so one column each. "Bare"
means the key's one-field action (Home, Delete, End, PageUp, PageDown).

| class | spelling | fx (escape_parser.zig) | xfx at `ebb04642` | xfx at `ce4f9c6b` |
|---|---|---|---|---|
| `; 1` | `CSI k ; 1 ~` | bare, all seven (`escape_parser.zig:696`, `:698`, `:712-718`) | nothing, all seven (`mask == 0`) | **bare** |
| empty | `CSI k ; ~` | bare, all seven (`escape_parser.zig:606-611`, `:696`) | nothing (`key_modifiers`) | **bare** |
| `0` | `CSI k ; 0 ~` | bare, all seven (`escape_parser.zig:696`) | nothing (`decimal`) | **bare** |
| leading zero, key | `CSI 0k ~`, `CSI 0k ; m ~` | the number (`escape_parser.zig:600-602`), so the key | nothing (byte-matched codes) | **the key** |
| leading zero, modifier | `CSI k ; 0m ~` | the number (`escape_parser.zig:663-665`) | nothing (`decimal`) | **the number** |
| past 256 | `CSI k ; 257..65535 ~` (and past 16 bits: 65535) | 1/7 4/8 5/6: the move (`escape_parser.zig:699-709`); 3: its bits (`:698`) | nothing, all seven (`key_modifiers`) | 1/7 4/8 5/6: **the key**; 3: **its bits** |
| lock bits alone | `CSI k ; 65`/`129`/`193 ~` | 1/7 4/8 5/6: the move (`escape_parser.zig:699-709`); 3: nothing (`:176-180`) | 1/7 4/8 5/6: the key; 3: Delete (`& 0x3f`) | 1/7 4/8 5/6: the key; 3: **nothing** |
| a third field | `CSI k ; m ; c ~` | the key report `c` under `m`, whatever `k` holds (`escape_parser.zig:746-752`) | nothing unless `k` is exactly `27` and `m`, `c` canonical | **the key report** |

Every changed cell is pinned by a test of `src/tui/input.rs`:
`a_modifier_field_of_1_is_the_bare_tilde_key`,
`an_empty_modifier_field_is_the_bare_tilde_key`,
`a_modifier_field_of_0_is_the_bare_tilde_key`,
`a_leading_zero_in_a_tilde_field_is_the_number`,
`a_modifier_field_past_256_is_read_as_upstreams_sixteen_bits`,
`delete_with_a_lock_bit_alone_is_nothing_as_upstream_reads_it` and
`a_third_tilde_field_is_a_key_report_whatever_the_first_holds`.

"The key" for a move is Home, End or the page key, as it was for every
modifier from `2` to `256` at the base: xfx has no draft-edge move and no
selection (DECODER-TILDE-HOME-END's records, `docs/parity.md`). The `5`/`6`
cells of the last three rows are PageUp and PageDown here and a composer move
to a page in fx; the two differ on the full transcript only (below).

**The oracle.** tmux sends each spelling as bytes (`key -H`), so the fx
binary was run on every class (`fx/`, `captures.txt`): `; 1`, empty and `0`
on Home, End and Delete, a leading zero on the key and on the modifier, past
256 on Home, End and Delete (`CSI 3 ; 99999 ~` kills to the line's end: 65535
saturated, Super among its bits), Caps Lock alone on Delete (nothing), and a
third field under `4` and `7` (Ctrl-E, Ctrl-A). The page keys are left to the
tests and to the probe below.

## What xfx does now (`git diff ebb04642 ce4f9c6b -- src`)

- `csi`'s `~` arm matches the paste markers as bytes, `200` and `201`, and
  hands every other parameter string to `tilde_report`. The five literal key
  arms are gone: one route reads every tilde key. `CSI 0200 ~` is the number
  200, no key, as fx's `digit_count == 3` makes it (`escape_parser.zig:625-626`).
- `tilde_report` splits on `;`: an empty first field or a fourth field is
  nothing; each field is `tilde_field` (fx's accumulator: digits only,
  saturating `u16`, empty `0`); the modifiers are the second field less one,
  saturating at `0`. Three fields go to `kitty_key` with the low byte of the
  modifiers (`kitty_key` masks to six bits, as fx does). One or two go to
  `tilde_key`.
- `tilde_key`: `1`/`7` and `4`/`8` through `cursor`'s `H` and `F` rows (Home,
  End), `3` through `forward_delete` with every bit, `5`/`6` PageUp and
  PageDown, anything else nothing.
- `forward_delete` takes a `u16`: the tilde spelling hands it the whole field,
  so `CSI 3 ; 257 ~` (256, a bit past eight) is nothing, not Delete; the
  keypad's call passes its six-bit mask, as fx's does (`escape_parser.zig:187`, `:205`).
- `kitty_key` and `report` (kitty's `u` spelling) are unchanged: their fields
  keep the canonical grammar (`docs/parity.md`).

## Left apart, each recorded in `docs/parity.md` (the full-screen TUI row)

1. **A tilde sequence past 32 parameter bytes** is discarded here, as every
   `CSI` is (`MOST_BYTES`), where fx's accumulators have no bound. Only a run
   of leading zeros, or digits past where fx's number stops, makes one.
2. **A page key held with a modifier pages the full transcript here, not in
   fx** (found by this unit's lock-bit check; older than this unit). fx's
   modified page key, tilde (`escape_parser.zig:708-709`) or keypad
   (`escape_parser.zig:169-173`), is a composer move to a page, and the full
   transcript takes only the bare `.page_up` (`input_full_transcript_runtime.zig:156-178`;
   routing `app_input_runtime.zig:930`, `app_input_runtime.zig:997-1001`).
   The probe (`probe/`) shows it on both binaries: in fx's frames
   `full == full-ctrl-pgup == full-caps-pgup` and the page moves on
   `CSI 5 ; 1 ~` and `CSI 5 ~`; in xfx's, `CSI 5 ; 5 ~` already moved it
   (`probe/frames.txt`). The parity row's sentence that said fx pages the
   full transcript on a modified page key now says this. Owner proposed:
   MODIFIED-PAGE-KEYS (a surface change: an action of its own for a modified
   page key, folded back to the page key after the full transcript reads it).
   The lock-bit page key reads as every other modified page key does, on
   both sides.

## RED (`red.txt`, `red-pairs.txt`)

- The tests at `074f5113` over the base's code: **15 fail**, each on the
  base's reading (`red.txt`, first failing assertion each): the seven new
  or renamed class tests, the four modified-tilde tests' `; 1` case, the
  field walk at `0`, the page-keys test's `05~`, `28;10;122~` and `27;5;099~`.
- The pair: **1 of 15** snaps identical (`typed`, before any key); in every
  other the base's letters all land at the end (`┃ abc defABC...`).

## GREEN (`green-pairs.txt`, `green.txt`)

- **15 of 15 pairs identical** (`cells.py`, row 1 left out), the last
  `┃ MN` on both. Banners: fx `v0.0.16`, xfx `v0.1.0-ce4f9c6`
  (`green-pairs.txt`), the red side `v0.1.0-ebb0464` (`red-pairs.txt`).
- `green.txt`: the unit's tests in the gate's run.

## Mutants (`mutants.py`, `mutants.txt`)

On a `git clone --no-local` of the branch at `ce4f9c6b` (its remote removed)
under `$TMPDIR/mut/`, one at a time, the file restored after each, under the
suite lock: see `mutants.txt` for the verdicts and the clone's state after
the run.

## Gate (`gate.txt`)

See `gate.txt`.

## The ledger's refs (`refs.py`, `refs.txt`)

This unit changes `src/tui/input.rs` alone, which no ledger ref names.
DECODER-TILDE-HOME-END's `refs.py` (docstring only changed) reads 16 checked,
0 wrong against `ebb04642` (`refs.txt`).

## `docs/parity.md`

The full-screen TUI row: the grammar sentence now says "everywhere but the
tilde keys" and drops "looser in one place only"; the tilde Home and End take
"any field"; the four-shape sentence is replaced by the tilde grammar as fx's,
with the 32-byte bound left apart; and the page-key sentence says the full
transcript pages on the bare key only upstream, with the probe and the
proposed owner.

## Carried from DECODER-TILDE-HOME-END's reviews

- The Home test's comment says "through `cursor`'s H row: Home"
  (`home_with_a_modifier_csi_1_mod_tilde_is_home`).
- "Six tilde codes" are seven: `modified_tilde`'s comment is gone with the
  function, and the predecessor's README says seven (1, 3, 4, 5, 6, 7, 8).
  Its `round2.txt` keeps "six": a record of what round 2 wrote.
- The `; 1` guard and its comment are gone; `tilde_key`'s doc says what each
  field means instead.
- The pinned divergence test is now
  `delete_with_a_lock_bit_alone_is_nothing_as_upstream_reads_it`.
