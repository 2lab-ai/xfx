# DIFF-WRAP receipt: a Ctrl-O diff row too long for the screen goes on under its gutter, as fx reflows it

The ledger's cause `ctrl-o-diff-row-clipped` (owner DIFF-WRAP) recorded the
gap. fx's Ctrl-O screen reflows a diff row longer than the screen onto
continuation rows, `  │     5678`. xfx's `diff_rows` clipped the row at the
margin, so the tail of a long line was not shown: in
`p5-transcript-diffwide` `full`, line 3's `5678` row was missing, and the
window above it started a row later on fx.

- **Branch:** `feat/p5-ctrlo-diff-wrap`, from the train-2 head `58b463c8`
  (T-U6, WIDTH-DISPUTED, T-U1-SAFE, CALLROW-SAFE).
- **The code:** `755ef861`. Every `xfx/` capture is from a debug build of
  it (banner `v0.1.0-755ef86`). The commit after it adds this receipt, one
  alternative in a ledger selector and one line in WIDTH-DISPUTED's checker
  (both under "The ledger"): `git diff 755ef861 HEAD -- src/ docs/` is
  empty, so the binary is the head's.
- **`red/`:** a debug build of `58b463c8` (banner `v0.1.0-58b463c`).
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`, checked before
  the captures; each run's guard line said it was unchanged.

## fx's rule

At `481104fd`, and the same at `c1db9191` (`reflowDiffRow` and `diffPrefix`
diff to nothing between the two):

- **Which rows.** `reflowDiffBlock`
  (`src/ui/render_engine/transcript_blocks.zig:772-797@481104fd`) reflows a
  row only when `diffPrefix` (`:659-695`) finds `  │ `, spaces, one digit or
  more, a space, `+`/`-`/space and a space, and only when that prefix is
  narrower than the screen (`:783`). Any other row passes through as it is:
  fx's elision row `⋯` has no digits. The Ctrl-O screen calls it for every
  change's diff (`src/ui/full_transcript_screen.zig:5917@481104fd`).
- **The continuation's indent and gutter.** `\x1b[0m\n`, every complete SGR
  the row has had so far (`:744`, `:759`), `  │ `, and `prefix.width - 4`
  spaces (`:758-762`). So the gutter is in the row's colour, the number and
  the sign are not carried (blanks under them), and the text goes on in the
  column it started in.
- **Where it breaks.** By cells, a display unit at a time, never at a word:
  before the first unit for which `shouldWrapAt(col, w, cols)`, that is
  `col + w - 1 > cols` (`src/core/shared/display_width.zig:286-288@481104fd`,
  asked at `transcript_blocks.zig:757`). A unit is what `displayUnitAt`
  returns (`display_width.zig:234-263`): an RGI emoji sequence, a variation
  base with its selector, else one code point. A unit of no cells is written
  before the question is asked and never moves down (`:751-755`): a mark
  joined to the character before it, and every C0 control -- **a tab
  included** (`display_width.zig:241-244`, width 0).
- **A wide character at the margin** moves down whole; the last cell of the
  row it leaves is empty (`shouldWrapAt` with `w = 2`).
- **Escapes.** The row's text holds no escape: `appendTerminalSafeLine`
  spelled every byte a terminal acts on (`\x1b`, `\u{202e}`;
  `src/core/output/diff.zig:1273@481104fd`), so a spelling is printable
  ASCII and is cut where the margin falls like any other text. A real
  escape in the row -- the row's own SGR -- is copied whole
  (`ansiSequenceEnd`, `transcript_blocks.zig:736-746`), never cut, and
  replayed at each continuation.
- **Colours.** The row's: `appendDiffRenderLine` opens it with the line
  style (`diff.zig:1250-1276@481104fd`) and the continuation replays it.
- **After the reflow, the row walker** lays the rows out as it lays out every
  static segment (`walker.append`, `full_transcript_screen.zig:3848`;
  `appendWithSoftWrapPrefix`, `:3972-4031@481104fd`): a tab moves to the
  next stop of eight, at most to the last column (`nextTabStopColumn`,
  `display_width.zig:292-300`, at `full_transcript_screen.zig:4004-4010`),
  and a row still wider than the screen -- the tab's cells, which the reflow
  counted as none -- runs on to the next row with no prefix at all
  (`:4019-4031`).

## What xfx does now

`src/tui/full_transcript.rs`:
- `diff_rows` cuts each numbered row's text with `reflowed`, writes each
  piece behind the row's own `  │ N ± ` or, for a continuation, behind
  `  │ ` and `start - 4` blanks, in the row's style; then draws the tabs
  (`drawn_tabs`) and hands the row to `run_on`, the port of fx's walker that
  `Body::Block` rows already use. A row with no number (the elided run, a
  notice) or a screen narrower than the prefix only runs on, as in fx.
- `reflowed` is fx's loop: break before a cluster whose cells would pass
  the room, a cluster of no cells never moved, a tab counted as none.
- `drawn_tabs` now stops a tab at the last column, as `nextTabStopColumn`
  does.

Decisions, each in the code's doc comments (`reflowed`, `drawn_tabs`), the
first three in `docs/parity.md:238` too:
- **The screen's own measure, not fx's `display_width`.** A cluster's cells
  are `wrap::cluster_width`, the count the frame places and clips a row by
  (WIDTH-DISPUTED). fx counts a skin-tone modifier after no emoji as no
  cells and a terminal draws it two wide; copied, fx's count would run the
  row past the margin, as fx's own row does (the ledger's
  `filler-modifier-count`). The scalars `drawn_wider` lists go the other
  way: xfx's measure counts them fewer cells than fx and a terminal, so a
  row holding one can still run past the margin, as it could before this
  unit (`ctrl-o-drawn-wider-raw`, owner CTRLO-WIDER, unchanged; no scenario
  holds one). This is the rule the tool group's wrapped rows already follow
  (the same `docs/parity.md` row).
- **Cut at grapheme clusters, not at fx's units.** The same as `literal` and
  `run_on`. The two cut apart only in a cluster of two characters fx counts
  a cell or more each (conjoining jamo `U+1100 U+1161`), which fx may cut
  between and xfx keeps whole. No scenario holds one; it is a sentence in
  `docs/parity.md`, not a ledger row.
- **A row without a number runs on.** fx's walker runs such a row on; xfx
  clipped it. The xfx-only notices (`(the text of this file would not
  change)`) now run on at a narrow screen instead of losing their tail.
- **A tab after a full row moves nothing.** *Superseded by DIFF-WRAPb
  (`../ctrlo-diff-wrap-b/`).* This described only the `col >= cols` case,
  and both reviews found the wider one: `drawn_tabs` fixed every tab's cells
  before `run_on` ran the row on, so a tab on a row the walker had run on
  took none (`\t`, 108 `b`, `\tZ` at 120: `bbbbZ` where fx's is
  `bbbb    Z`). fx's screen, captured in `p5-transcript-diffwrap2`, writes
  the character after a tab on a full row over the row's last cell; xfx now
  lays each row out as fx's walker does, in one pass, and copies both.

## RED

- **Unit tests** (`red-test.txt`): the five below, written first and run on
  the base's code, all fail on the missing continuation:
  - `a_changes_line_wider_than_the_screen_goes_on_under_its_gutter_as_fx_reflows_it`
    (the `5678` row, an exact fit, one cell over, three rows of a dim
    context line under a two-digit number column);
  - `a_wide_character_at_the_margin_moves_down_whole_and_leaves_the_last_cell_blank`
    (`漢`, `😀`, and `f` U+0301 in the last column);
  - `a_spelled_escape_at_the_margin_is_cut_like_any_text_and_no_row_carries_a_control`
    (`\x` / `1b[2JEND`, `\u{2` / `02e}END`, and no row holding a character a
    terminal acts on, by the closure of the T-U6 test beside it);
  - `a_tab_is_no_cells_to_the_reflow_and_the_row_walker_runs_its_overflow_on`
    (the walker's `ddddd` row with no gutter; a tab stopped at the last
    column);
  - `a_row_with_no_number_or_no_room_runs_on_as_the_row_walker_lets_it`.
  A sixth, `what_takes_no_cells_stays_on_its_row_even_past_a_character_wider_than_the_room`,
  tests `reflowed` itself, which the base has not; it is the test mutant M7
  fails.
- **Pairs** (`red.txt`, `python3 -I -B check.py <repo> . red`): 6 of 15
  claims fail -- `full` and `rows` of each scenario. The diff block is
  fx 5 / xfx 4 rows (`diffwide`), 10 / 7 (`diffwide2`), 19 / 9 (`diffwrap`).

## GREEN

`green.txt` (`python3 -I -B check.py <repo> .`): 15 of 15 claims hold.

| scenario | `edited` | `full` | `closed` |
|---|---|---|---|
| `p5-transcript-diffwide` | same, 41 rows | **same, 41 rows**; the diff block 5 rows each | same |
| `p5-transcript-diffwide2` | same | 35 rows the same; rows 9 and 23 `frame-variation-selector-anchor`, rows 14, 15, 28, 29 `filler-modifier-count` | same |
| `p5-transcript-diffwrap` (new) | same | **same, 41 rows**; the diff block 19 rows each | same |

- `diffwrap` `full` is fx's cell for cell, including the cases a reading of
  the code alone could get wrong: the walker's `ddddd` row after the tab,
  `… X` / `Y` for the tab a cell short of the last column, and U+0301 after
  the last column's `f`.
- `diffwide2`: lines 3 and 4 run on as fx's do (`  │     5678`,
  `  │     IL5678`). Line 5's rows stand apart, both the read rows and now the
  diff rows: fx counts each U+1F3FD as no cells and its row runs past the
  margin (`🏽🏽🏽 L`, then `  │     5678`); xfx's ends `🏽🏽` and goes on
  `  │     🏽🏽TAIL5678`. Each of xfx's rows is taken in by the divergence's
  `xfx_row`.
- No cell of any xfx `full` is past column 120 (`margin`).

## The ledger

- **`ctrl-o-diff-row-clipped` is deleted**: no row is left for it.
- **`filler-modifier-count`**: line 5's diff rows are its rows now, as its
  read rows were. Its `xfx_row` already took in xfx's two (each holds a lone
  U+1F3FD). For fx's side its `fx_row` gains one alternative,
  `^  │     5678$` -- fx's diff continuation, as `^│  5678$` was its read
  continuation. Without it the head sweep left `p5-transcript-diffwide2/full`
  fx row 29 in "Rows no cause covers" (the aligned reading matches each
  side's text on its own side, `sweep.py` `attribute`). Line 3's
  continuation is the same text and the same on both sides, so nothing has
  to tell the two apart. The `reason` now names the diff rows instead of
  sending their continuation to `ctrl-o-diff-row-clipped`.
- **WIDTH-DISPUTED's `check.py`** reads that divergence from the live ledger
  (by id). Its `full … aligned` claim counted fx's `  │     5678` rows in its
  own captures, where xfx still clipped, as rows only fx has. With the new
  alternative they fall under "rows the divergence covers" as well, and the
  claim failed (10 of 11). The one changed line drops what the live divergence
  covers from the expected list too, as the claim already did for the rows it
  reads (`want_fx`). Re-run against its own captures: `xfx` 11 of 11, `red`
  6 of 11, as its `green.txt` and `red.txt` record. T-U6's `check.py`, run
  against its own captures with this ledger: 35 of 35, as recorded.
- **`frame-variation-selector-anchor`**: the same two rows, the same cells
  on base and head (line 2 is 73 bytes and is not reflowed); only their
  position moved, from 12 and 26 to fx's 9 and 23.

## Sweep

Every `p5-transcript-*` scenario (23, `diffwrap` among them), 40x120,
`--jobs 2`, fx v0.0.16 against two xfx builds:

- **base** (`sweep-base.md`): `58b463c8`'s build (sha256 `253632f6…`),
  scored with `58b463c8`'s ledger (`--rescore` from a `--no-local` clone at
  that commit; ledger sha256 `b1a27fb6…`);
- **head** (`sweep-head.md`): `755ef861`'s build (`bf0800ef…`), scored with
  this branch's head ledger (`--rescore` after the `fx_row` alternative;
  ledger sha256 `eb7bafc1…`).

`sweep-compare.txt` (`python3 -I pairs_diff.py <base pairs.json> <head
pairs.json>`): 104 pairs each side, pass 39 → 42, **0 dropped**. Three
verdicts moved, each up:

| pair | base | head | why |
|---|---|---|---|
| `p5-transcript-diffwide/full` | unresolved-defect | normalized-identical | this unit |
| `p5-transcript-diffwrap/full` | unresolved-defect | normalized-identical | this unit |
| `p5-transcript-records/start` | unresolved-defect | normalized-identical | timing: the snap is at launch (`wait 4`, no tool call yet), and on base xfx's footer still held the transient `Full access enabled: …` hint (`src/tui/permissions.rs:30`); no diff row is on that screen |

`p5-transcript-diffwide2/full` stays unresolved-defect, attributed at head
to `frame-variation-selector-anchor` (ENGINE-VS-ANCHOR) alone, with no row
left unattributed. At base it was `ctrl-o-diff-row-clipped`'s, a whole-snap
cause. "Rows no cause covers" goes from 15 snaps to 13. The two that left are
`diffwrap/full` (base: the new scenario, which no cause named) and
`records/start` (the timing row above). None was added.

## Mutants

`mutants.py`, on a `git clone --no-local` of the branch in the scratchpad
(`GIT_DIR`, `GIT_INDEX_FILE`, `GIT_WORK_TREE` unset for each run), each a
one-place change to `src/tui/full_transcript.rs`, then
`cargo test --locked --lib -- tui::full_transcript`. `mutants.txt`: **7 of
7 killed**.

| mutant | killed by |
|---|---|
| M1 no reflow of a numbered row | 4 tests (the `5678` row, the wide character, the spelled escape, the tab) |
| M2 the old clip in place of the walker | the walker's `ddddd` row; the notice that runs on |
| M3 break one cell early (`>=`) | 5 tests |
| M4 a tab counted four cells by the reflow | the tab test; `reflowed`'s own test |
| M5 a continuation one cell deeper | 4 tests |
| M6 a tab stop past the last column | the tab test (`… X` / `Y`) |
| M7 a unit of no cells may start a row | `reflowed`'s own test |

## Gates

In the worktree, with a short `TMPDIR` (the common brief's), one step at a time,
on `755ef861`'s `src/` (the receipt commit changes none of it):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 15 binaries, 5163 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 15 binaries, 5200 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |
| `scripts/fxcap/test-sweep.sh` (the ledger's own tests, with the head ledger) | 50 + 26 tests, OK |

`fmt`, the three `check-*` scripts and `test-sweep.sh` were run again on the
head commit, with this receipt in it: the same results.

No pty test failed, so none was re-run.

## Files

- `fx/`, `xfx/`, `red/`: the three scenarios' `edited`, `full` and `closed`
  snaps, `.ansi` and `.txt`.
- `check.py` and its output, `green.txt` and `red.txt`.
- `red-test.txt`: the five tests on the base's code.
- `sweep-base.md`, `sweep-head.md`: the scoreboards. `sweep-compare.txt` is
  `pairs_diff.py`'s output over the two `pairs.json`.
- `mutants.py` and `mutants.txt`.
