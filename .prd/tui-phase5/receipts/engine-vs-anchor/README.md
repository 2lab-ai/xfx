# P5 ENGINE-VS-ANCHOR receipt: the frame re-anchors after a glyph with a variation selector

The ledger cause `frame-variation-selector-anchor` (owner ENGINE-VS-ANCHOR,
WIDTH-DISPUTED's SHOULD 3). fx v0.0.16's frame writer (`vercel-labs/fx@481104fd`,
commit `95e6c8fd` "Re-anchor the cursor after glyphs with a variation
selector") clears, re-anchors and guards around a display unit whose suffix
holds a variation selector; xfx's painter wrote the run on. The cause is
closed: deleted from the ledger.

- **Base:** `58b463c8`, the train-2 head (WIDTH-DISPUTED in it).
- **Head:** the commit carrying this receipt (the ledger and this folder).
  Its code is `a66155c0`'s, seven commits on the base:
  - `02204cbc`: the rule (grid, frame, check, smoke oracle);
  - `b6522746`: document rows written as they stand (decision 2);
  - `a253f7bf`, `8018daf0`: comments only (fx citations; the comments point
    here rather than at the closed ledger cause);
  - `05752b03`: one test, for the code mutant that survived round 0's run;
  - `b729c221`: **review round 1** (below): a cluster split off its glyph
    by a sequence written where it stands, the grid diff's split cell, and
    the check's bracket narrowed to the painter's shape;
  - `a66155c0`: one more bracket test case, for the mutant that survived
    round 1's first run.
- **Builds.** Debug builds of the commit named:
  - `xfx/`: `a66155c0` (`x𝒇x v0.1.0-a66155c [debug]`);
  - `red/`: `58b463c8` (`v0.1.0-58b463c`);
  - the sweeps: base `58b463c8`, head `a66155c0` (`sweep-head-r2.md`);
    round 0's head sweep was `b6522746` (`sweep-head.md`).
  - `publish.py` (WIDTH-DISPUTED's, `../width-disputed/publish.py`) checked
    each xfx snap whose row 1 is the banner as it copied it.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`,
  `FX_AUTO_UPGRADE=0`; every capture's guard line says it was unchanged.

## fx's rules, read at the pin

The per-cell write of `Grid.diffBand`, `src/core/terminal/engine.zig:2548-2589@481104fd`:

- **Columns are one-based.** `first = c + 1` (`:2529`), `col = first`
  (`:2548`), `last` the last changed column, one-based; `col` steps by the
  cell's width (`:2581`).
- **Which suffixes count.** `hasVariationSelector` (`:3367-3369`): the cell's
  combining suffix holds U+FE0E or U+FE0F, anywhere in it.
  `disputed_width = suffix != null and hasVariationSelector(suffix)` (`:2572`).
- **The guard** (`guard_margin_suffix`, `:2565-2568`): autowrap off in the
  terminal the diff starts from (`!prev.autowrap`), the cell two wide, `col +
  1 == cols` (its second cell is the last column), and **any** suffix, not
  only a selector.
- **The order**, per cell: the SGR transition (`:2555-2558`), the link
  (`:2559-2562`), the `│` anchor (`:2563`, below), `?7h` if guarded
  (`:2571`), `ESC[2X` if disputed and two wide (`:2575`), the codepoint and
  the suffix (`:2576-2579`), `?7l` if guarded (`:2580`), then `col +=
  width` and `ESC[{col}G` if disputed and `col <= last` (`:2586-2588`): the
  column after the unit, one-based, when something else is written on the
  row.
- **When DECAWM is off.** For the whole session: the interactive mode set
  ends `?7l` (`src/ui/terminal/terminal.zig:4-5@481104fd`), written at launch
  (`enableInteractiveTerminalModes`, `src/core/app/app_lifecycle.zig:963-965`,
  `:1214-1220`), and footer rows are fed "with DECAWM disabled ... fx keeps
  autowrap disabled session-wide" (`src/ui/render_engine/frame_surface.zig:256-258`).
  So the guard applies on every frame.
- **Where the diff writes.** Every frame: a full repaint is `writeBandClears`
  and the diff, a partial one the diff
  (`src/ui/render_engine/terminal_diff.zig:618-640@481104fd`). A **document
  append is not**: its rows are written as their own bytes, with autowrap on
  for their length (`appendDocumentMovement`, `:1369-1426`). No anchor there.
- fx's own test: `engine.zig:4056-4085` (`a \u{26A1}\u{FE0E}` →
  `\x1b[2X\u{26A1}\u{FE0E}\x1b[5G`); the guard's: `:4643-4671`.

## What changed

- **`src/tui/grid.rs`**
  - `put_unit`: the one spelling of the rule, upstream's order. It decides on
    the grid cell's grapheme and writes the unit's bytes through the writer
    it is handed. The grid diff (`diff_from`) writes each cell through it.
  - `write_cell` (round 1): the diff's bytes for one cell. Together, unless
    the cell is a one-cell glyph `place_row` joined a cluster to across a
    sequence and the two together measure wider (`☀` U+FE0F: one cell in the
    grid, two together): then split as its row was -- the longest part still
    its width, the pen again (a reset with nothing on, the attributes
    otherwise: it changes nothing), the rest.
  - `write_row_text`: a row written whole, cut into the units `tokenize_row`
    finds and each written through `put_unit`. **Its bytes are the row's, in
    the row's order** (round 1); the rule is decided on the cell `place_row`
    makes, which joins a zero-width cluster after a sequence onto a
    one-cell unit and onto nothing for a two-cell one (its continuation). The
    anchor follows the last cluster joined. Pure ASCII is written as it
    stands.
  - The module doc: `ECH`, `CHA` and the bracket are now in the diff's
    alphabet (it said "No `ECH`", for the smoke oracle's sake).
- **`src/tui/frame.rs`**: the alternate plane's paint (`paint_alternate`: the
  full transcript, the approval and question screens), `restore_primary` and
  `paint_whole` write their rows through `write_row` → `write_row_text`.
  `place`, the document's rows, does not (decision 2).
- **`src/tui/check.rs`** (the output check, `preflight`, decodes every vector
  before it is written):
  - `CHA` (`CSI n G`): a known caret, `1..=cols`, the row kept;
  - `ECH` (`CSI n X`): `n ≥ 1`, a known caret, nothing past the last column,
    the whole of any wide cluster it covers part of (`erase_span`), the caret
    kept; recorded as an erase;
  - the bracket (`Bracket`): only `put_unit`'s shape gives the mode back as
    `?7h` found it -- `?7h`, `ECH 2` exactly when the unit's suffix holds a
    selector, one unit two cells wide with a suffix ending in the last
    column, `?7l` (round 1: it took any one unit at the margin and any
    clears). Anything else and the two are what they say alone, so a frame
    that carries them is refused as a mode it did not declare (as a bare
    trailing `?7h` still is, `frame.rs`'s
    `a_frame_that_turns_a_terminal_mode_back_on_…`).
- **`scripts/smoke-tui.sh`**: the oracle models `CHA` and `ECH` as
  `check.rs` does, with two falsification tests.
- **`scripts/fxcap/parity-ledger.toml`**: `frame-variation-selector-anchor`
  deleted.

### Decisions

1. **An `EL` after the unit counts as something written after it** (the
   grid diff's shortened row). fx's diff never writes one -- it writes the
   blank cells -- so `col <= last` covers them; an `EL` sent from a caret the
   terminal advanced by its own width would start a cell early or late, and
   late leaves a stale cell. Test:
   `the_erase_after_a_selector_glyph_starts_where_the_grid_puts_the_glyphs_end`.
2. **Document rows are written as they stand.** fx's document append is
   outside its diff (above), and an anchor in `place` would skip cells on a
   row erased only behind its text: what the scroll brought up into them
   would stay. Round 1 anchored them (`02204cbc`); `b6522746` takes it out.
   Test: `a_document_row_is_written_as_it_stands_as_upstreams_document_append_is`.
3. **No autowrap flag in the painter.** xfx's session keeps autowrap off as
   fx's does (`src/tui/term.rs`'s `MODE_SET`, `?7l`), and no frame is written
   outside it, so the guard's `!prev.autowrap` is always true here.
4. **`CHA` and `ECH` under an open link are accepted** by the check and the
   oracle, where `CUP`, `ED` and `EL` are refused: both stay on the caret's
   row, the next cell covers what `ECH` blanked, and fx writes both after the
   cell's link is open. Test: `an_anchor_inside_a_link_is_accepted`.
5. **A split cluster's rule is decided on the grid's cell** (round 1). The
   brief allowed reading the joined grapheme; the cell `place_row` makes is
   the one the diff writes, so a full paint and a partial one decide alike:
   `☀`, a colour, U+FE0F is a selector glyph (anchored after the selector),
   `⚡`, a colour, U+FE0E is not (the grid keeps `⚡`, no clear, no anchor).
   `place_row` is unchanged. Tests:
   `a_row_written_whole_keeps_a_split_cluster_behind_its_colour`,
   `a_cluster_split_off_its_glyph_by_a_colour_is_accepted_from_the_diff_and_a_whole_row`.
6. **The diff's split cell carries the pen again between its parts** (round
   1). The grid does not keep the sequence the row had there, so the diff
   writes one that changes nothing: the cell's own attributes, or a reset
   when none are on. Test:
   `a_one_cell_glyph_and_a_selector_a_colour_split_off_are_written_split`.

### Seen, not this unit's

- fx also writes `ESC[{col}G` before every `│` cell (`engine.zig:2563`, at
  `c1db9191` too), and `ESC[{n}X` over a row's span when a wide cell shifted
  under another (`hasShiftedWideCellOverlap`, `:2544-2546`). xfx's painter
  does neither. Neither is in this unit's brief or in the ledger.
- fx writes a document append with autowrap on (`:1414-1420`); xfx's `place`
  writes with it off, so a wide unit with a suffix in a document row's last
  column is written without the bracket. Not in the brief.
- A cluster after a sequence and a **two-cell** glyph is no cell's in the
  grid (`place_row`), where a terminal may join it to the glyph. Unchanged
  here, as the review asked; such a selector neither clears nor anchors.
- The review's other should-fix (a row painted whole writes a `CHA` before
  trailing default spaces where fx's `last` stops) is recorded by the
  dispatcher as no action: the cells are the same.

## RED

**Tests** (`red-test.txt`): the unit's 21 tests on the base's code (the base
files with `a253f7bf`'s test blocks appended, in a throwaway clone): 17 fail,
4 pass. (`05752b03` adds two assertions to one of the 17.)

- The 17 fail on the missing bytes (`left: "\u{1b}[10;1Ha ⚡\u{fe0e} b"`,
  `right: "…\u{1b}[2X⚡\u{fe0e}\u{1b}[5G b"`), or, in the check's, on
  `a control sequence outside the alphabet` where a `CHA`/`ECH` stands;
- the 4 that pass on the base are the guards of what must not change: a
  glyph without a selector, a wide unit without a suffix or short of the
  margin, a bracket around anything else, a document row.

**Captures** (`red.txt`, `check.py … red`, 4 claims hold): `check.py`'s red
mode asserts the difference, so its claims holding **is** the RED. Line 2's
read row and Ctrl-O diff row on the full transcript: fx's cells are red's
with three unstyled blank cells before `x zwj`, both rows; and all four of
those rows are among the rows one side holds alone.

### Review round 1 (`red-test-r2.txt`)

Six tests, `b729c221`'s, appended to round 1's head `f20c28c9` in a
throwaway clone: **all six fail** there.

| finding | test | at `f20c28c9` | on the base `58b463c8` |
|---|---|---|---|
| MUST 1: `한` `ESC[39m` U+0301 and `☀` `ESC[1m` U+FE0F through `enter_alternate` | `frame` `a_cluster_a_colour_split_off_its_glyph_is_written_where_it_stands_on_a_surface` | **refused** ("a cell the vector did not leave as intended", row 4 col 1) | accepted; fails only at its anchor assertion (the base writes none) |
| MUST 1, the bytes | `grid` `a_row_written_whole_keeps_a_split_cluster_behind_its_colour` | `한\u{301}\u{1b}[39m…`: the mark moved ahead of the colour | -- |
| MUST 1, the three readers | `check` `a_cluster_split_off_its_glyph_by_a_colour_is_accepted_from_the_diff_and_a_whole_row` | "`한` `ESC[39m` U+0301 written whole: refused" | -- |
| SHOULD 3: `☀` `ESC[1m` U+FE0F through the band diff | `frame` `a_selector_a_colour_split_off_a_one_cell_glyph_is_written_split_by_the_band_diff` | **refused** (row 24 col 1) | **refused** too (row 24 col 1): the defect is the base's |
| SHOULD 3, the bytes | `grid` `a_one_cell_glyph_and_a_selector_a_colour_split_off_are_written_split` | `☀\u{fe0f}` written together | -- |
| SHOULD 2: the bracket's shape | `check` `an_autowrap_bracket_is_only_the_painters_own_shape` | a one-cell unit with no suffix, at the margin, accepted as a bracket | -- |

The base column is the two `frame.rs` tests on `58b463c8` (same file).
`a66155c0` adds one more shape (a one-cell unit with a suffix) to the last
test, for the mutant that survived round 1's first run (below).

## GREEN

**Tests:** the unit's 27 pass at the head (round 0's 21 and round 1's six).

**Captures** (`green.txt`, `check.py` on `fx/` against `xfx/`), 4 claims, 4
hold:

| claim | result |
|---|---|
| `anchor`: line 2's read row and Ctrl-O diff row, `full` | every cell the same as fx's, text and attributes (`halfﾠ︎   x zwj…`) |
| `aligned`: the whole `full` screen, aligned as sequences | no row one side holds alone has a selector; each such row is a ledger entry's (`ctrl-o-diff-row-clipped`, `filler-modifier-count`) |
| `inline edited` | all 41 rows the same |
| `inline closed` | all 41 rows the same |

**What "identical" covers.** The anchor's rows are identical cell for cell.
The `full` snap as a whole is not, on either build, and its sweep verdict
stays `unresolved-defect`: fx reflows an overlong Ctrl-O diff row and xfx
clips it (`ctrl-o-diff-row-clipped`, DIFF-WRAP's), and fx counts a skin-tone
modifier after no emoji as no cells (`filler-modifier-count`, an approved
divergence) -- every row one side holds alone is one of those two entries'
(the `aligned` claim prints them). `edited` and `closed` are identical but
for the banner.

## Mutants

`mutants.py`, every mutant caught:

- **Code** (`mutants-code.txt`): 24 mutants in a throwaway clone of
  `a66155c0`, each against the unit's 27 tests. Each takes out one rule: the
  anchor, its column (one early), the clear, U+FE0F as a selector, the
  guard, the guard for any suffix, the diff's `EL` as following, the row
  written whole not anchoring after its last unit, each of the three frame
  writers, the document row as it stands, and in the check `CHA` under a
  link, the bracket at all, `ECH`'s whole-cluster span, `ECH` past the
  margin, `CHA` off the screen. Round 1's eight: the joined cluster moved
  ahead of its sequence (round 1's head, restored), the join onto a two-cell
  unit, the diff's split cell written together, split with nothing between,
  and the bracket taking a unit of any width, one with no suffix, a clear of
  any count, a clear whatever the selector.
  - On `8018daf0` one survived (anchoring after a row's last unit);
    `05752b03`'s assertion catches it.
  - On `b729c221` one survived (the bracket taking any width: a one-cell
    unit with no suffix failed on the suffix too); `a66155c0`'s case, a
    one-cell unit with a suffix, catches it.
- **Oracle** (`mutants-oracle.txt`): 4 mutants of `smoke-tui.sh`'s emulator
  (`CHA` or `ECH` unmodelled, `CHA` past the margin accepted, `ECH` leaving
  half a cluster), each caught by its falsification tests.
- **Checker** (`mutants-check.txt`): 3 capture edits (xfx's read row without
  the gap, fx's diff row without it, an `edited` row), each failing the
  claim it names.

## Sweep

`sweep.sh --jobs 2 --only …` at 40x120, fx v0.0.16, base `58b463c8`
against `b6522746` (`sweep-base.md`, `sweep-head.md`), `compare.py`'s
pair-by-pair reading in `compare.txt`.

**Selection.** Every scenario whose captures hold U+FE0E or U+FE0F, read
from the receipts on this branch and from `research/tui-phase5`'s 5912
`.ansi`/`.txt` files: `composer-keys`, `fast-astra`, `fast-opus`,
`fast-override`, `fast-unavailable`, `fast-unconfirmed`, `model-column`,
`model-speed`, `p5-session-early-prefs`, `p5-transcript-diffwide2` (the
`fast` rows carry the hint's ⚡︎, U+26A1 U+FE0E, on the band the grid diff
paints). Then `composer-*`, `p5-input-*`, the rest of `fast-*` and
`model-column*`, and `p5-transcript-diffwide`: 25 scenarios, 124 pairs.

**Result.** Base 93 pass, head 92; no pair gained a differing row the
head's xfx screen caused. Three verdicts moved, all in two timing-driven
scenarios, and **the xfx screen of each was the same in both sweeps; fx's
moved** (fx is one binary in both):

| pair | base r1 | head r1 | base r2 | head r2 | the side that moved |
|---|---|---|---|---|---|
| `model-column-idle` `col-typed` | pass | unresolved | unresolved | unresolved | fx (`unable to load models`) |
| `model-column-reask` `col-down` | pass | unresolved | pass | pass | fx |
| `model-column-reask` `col-back` | unresolved | pass | unresolved | unresolved | fx |

r2 is both scenarios again on each build, one at a time (`sweep-base-2.md`,
`sweep-head-2.md`, `compare-2.txt`): the same verdicts on both builds, and
every screen the same. `model-column-idle.cap`'s header says why: the
catalog is down for the first nine seconds, and fx re-asks only when
something asks for a frame. No screen in either scenario holds a selector.

The other xfx screens that differ between the sweeps: `composer-wrap`
`badge-71`/`-111` (the run's temp path inside an image link), the two
`model-column-idle` snaps above (the same catalog timing, on both sides),
`p5-transcript-diffwide` `full` (timestamps), and `p5-transcript-diffwide2`
`full` (the anchor).

**Review round 1** (`sweep-head-r2.md` at `a66155c0`; `compare-r2.txt`
against the same base sweep, `compare-r0-r2.txt` against round 0's head
sweep). Base 93 pass, round 1's head 92, round 0's head 92.
- Against the base: one verdict moved, `model-column-idle` `col-typed`, the
  fx-timing pair above (fx's screen moved, xfx's did not; `compare.py` exit 0).
- Against round 0's head: two verdicts moved, both `model-column-reask` (the
  same fx timing: fx's screens moved, xfx's did not). The xfx screens that
  differ are `composer-wrap` `badge-71`/`-111` (the temp path),
  `p5-transcript-diffwide` `full` and `p5-transcript-diffwide2` `full`; the
  last differs from round 0's in no row once its timestamps are set aside
  (40 rows each, checked by row).

## Gates

On `a66155c0` (review round 1), with the ledger and this receipt staged.
`TMPDIR` a short scratch directory; each suite alone, one after another.

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 5184 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 5221 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh` | ok (12094 files), ok, ok (12094 files) |
| `scripts/fxcap/test-sweep.sh` (the ledger changed); the ledger loads with its sources checked | 50 + 26 tests OK; 22 divergences, 46 causes |
| `cargo build --locked --release`; `--release --features fault-injection --target-dir target/faulty` | ok |
| `scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx <dir>` | 37 scenarios and the oracle, 1,127 checks (the oracle's two new tests among them), 0 failures |
| `scripts/check-tui-preflight-cost.sh` (the check decodes three more sequences) | ok: 200x300 full 1.30 ms, update 0.92 ms (budget 32 ms) |
| `check.py … xfx` (`green.txt`) / `check.py … red` (`red.txt`) | 4 claims, 4 hold / 4 claims, 4 hold (red mode asserts the gap) |
| `mutants.py` code / oracle / check | 24 / 4 / 3 mutants, every one caught |
| `transcript-wrap/check.py`, `transcript-receipts/check.py`, `width-disputed/check.py` under the new ledger | 34/34, 35/35, 11/11: each `green.txt` reproduced |

`docs/parity.md`: no row touched. The anchor closes a gap no row records,
and nothing here is left divergent from fx.

## Files

- `fx/`, `xfx/`, `red/`: `p5-transcript-diffwide2`'s snaps, redacted.
- `check.py` (the claims above), `mutants.py` (code, oracle and checker
  mutants), `compare.py` (two sweeps, pair by pair).
- `green.txt`, `red.txt`, `red-test.txt`, `red-test-r2.txt`,
  `mutants-code.txt`, `mutants-oracle.txt`, `mutants-check.txt`,
  `sweep-base.md`, `sweep-head.md`, `compare.txt`, `sweep-base-2.md`,
  `sweep-head-2.md`, `compare-2.txt`, `sweep-head-r2.md`, `compare-r2.txt`,
  `compare-r0-r2.txt`. Each went through `scripts/fxcap/redact.py`.

Commands, from the worktree (`R=.prd/tui-phase5/receipts/engine-vs-anchor`):

```
python3 -I -B .prd/tui-phase5/receipts/width-disputed/publish.py . $R p5-transcript-diffwide2 <fx-caps> <head-caps> <base-caps> a66155c 58b463c
python3 -I -B $R/check.py . $R xfx    # green.txt
python3 -I -B $R/check.py . $R red    # red.txt
python3 -I -B $R/mutants.py code <clone of the head> $R/mutants-code.txt
python3 -I -B $R/mutants.py oracle . <scratch>       # mutants-oracle.txt
python3 -I -B $R/mutants.py check . $R <scratch>     # mutants-check.txt
python3 -I -B $R/compare.py <base-sweep> <head-sweep>   # compare.txt
```
