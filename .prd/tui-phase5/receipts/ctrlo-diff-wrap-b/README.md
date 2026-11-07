# DIFF-WRAPb receipt: a Ctrl-O diff row's tab goes to the stop of the row fx's walker is on

Both reviews of DIFF-WRAP (`dd0e4e21`) raised the same SHOULD. `drawn_tabs`
fixed every tab's cells before `run_on` ran the row on. So a tab on a row
the walker had already run on took no cells: `\t`, 108 `b`, `\tZ` at 120
columns drew `bbbbZ` where fx draws `bbbb    Z`. The same happened on a
screen narrower than the prefix, where the prefix itself runs on. The opus
review also noted that no fx capture held a diff with two-digit line
numbers.

- **Branch:** `feat/p5-ctrlo-diff-wrap`, on DIFF-WRAP's head `dd0e4e21`.
- **The code:** `378a0839`. Every `xfx/` capture is from a debug build of
  it (banner `v0.1.0-378a083`). The commit after it adds this receipt and
  one paragraph of DIFF-WRAP's README: `git diff 378a0839 HEAD -- src/
  scripts/ docs/` is empty.
- **`red/`:** a debug build of `dd0e4e21` (banner `v0.1.0-dd0e4e2`), built
  in a `--no-local` clone.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`, checked before
  the captures; each run's guard line said it was unchanged.

## fx's walker

`ProjectionRowWalker.appendWithSoftWrapPrefix`
(`src/ui/full_transcript_screen.zig:3972-4031@481104fd`), which lays out the
reflowed diff as a static segment, with no prefix (`:3848`):

- one column, `col`, from 1, for the whole of what it is given;
- a character with `shouldWrapAt(col, w, cols)` starts the next row and
  `col` is 1 again (`:4019-4022`) -- before anything after it is placed;
- a tab sets `col = nextTabStopColumn(col, cols)` (`:4004-4006`): the next
  stop of eight from the row's own column, at most `cols`, and `cols` for a
  column at or past it (`display_width.zig:292-300@481104fd`);
- so a tab after a row the walker filled moves `col` back onto the last
  column, and the next character, for which `shouldWrapAt(cols, 1, cols)` is
  false, is written there.

What fx's screen shows for the last case was not in the code. The new
scenario captures it: `p5-transcript-diffwrap2` `full` line 3 is `…cZ`, its
104th `c` written over, then `W`. Line 13, the same over a wide character's
right half, is `…i Z`: the character's left cell blank and unstyled.

## What xfx does now

`src/tui/full_transcript.rs`: `walked` replaces `drawn_tabs` and `run_on`
for a diff row. It is one pass over the row as fx's walker makes it, a cell
list per row:

- a character that would pass the margin starts the next row at column 1;
- a tab adds blank, unstyled cells to the next stop of the row it is on, at
  most the last column;
- a tab on a full row moves the column back, and the next character that
  fits takes the last cell's place (a wide character's left half becomes a
  blank);
- a mark of no cells joins the cell before it.

`drawn_tabs` is deleted. `run_on` is unchanged and still lays out
`Body::Block` rows. `reflowed` is unchanged: its pieces go to `walked` in
place of `drawn_tabs` + `run_on`.

**One decision: the last cell written over is copied.** fx's Ctrl-O shows
`…cZ`, so one character of the file's line is not on that screen. xfx copies
it, as T-U6b copied fx's lossy spelling on Ctrl-O: Ctrl-O decides nothing,
and the review, the screen a change is decided on, wraps a line whole rather
than cut it (`src/tui/approval_view.rs:504-506`). Stated in `walked`'s doc
comment and `docs/parity.md:238`. It replaces DIFF-WRAP's departure 3,
which now points here.

**Two-digit numbers.** fx pads every number to the widest
(`appendDiffLineNumber`, `src/core/output/diff.zig:1316-1329@481104fd`,
`gutter_width` from the largest at `:1224-1229`), so every row's prefix is
the same width, which is what `under` (`start - 4`) assumes. Captured:
`p5-transcript-diffwrap2` has 14 lines, `  │  1 +` to `  │ 14 +`, and line
11's continuation `  │      gggg` and line 4's `  │      ee     f` are fx's.

## RED

- **Unit tests** (`red-test.txt`), written first and run on `dd0e4e21`'s
  code, both fail:
  - `a_tab_the_walker_meets_on_a_row_it_ran_on_goes_to_that_rows_next_stop`:
    `bbbbZ` for fx's `bbbb    Z` (the opus repro); also has the gpt6 repro
    (`aaaaaa  XY`), the unstyled bytes of the run-on row's tab, and the
    screen narrower than the prefix (`  │ 1 ` / `+    a` / `b`);
  - `a_tab_after_a_row_the_walker_filled_puts_the_next_character_over_its_last_cell`:
    `…c` / `ZW` for fx's `…cZ` / `W`; also has the wide character's blank
    left cell and the wide character after such a tab.
- **Pairs** (`red.txt`, `python3 -I -B check.py <repo> . red`): 2 of 20
  claims fail, `full` and `rows` of `p5-transcript-diffwrap2`. DIFF-WRAP's
  three scenarios hold on the base, as they did at DIFF-WRAP. The rows that
  differ are lines 1 and 2 (`bbbbZ`, `aaaaaaX` for fx's `bbbb    Z`,
  `aaaaaa  X`), line 3 (`…c` / `ZW` for `…cZ` / `W`) and line 13 (`…i漢` /
  `ZW` for `…i Z` / `W`). Lines 4, 11, 12 and 14 and every two-digit
  prefix were already fx's on the base.

## GREEN

`green.txt` (`python3 -I -B check.py <repo> .`): 20 of 20 claims hold.

| scenario | `edited` | `full` | `closed` |
|---|---|---|---|
| `p5-transcript-diffwrap2` (new) | same | **same, every row**; the diff block 22 rows each | same |
| `p5-transcript-diffwrap` | same | same, every row; 19 rows each | same |
| `p5-transcript-diffwide` | same | same, every row; 5 rows each | same |
| `p5-transcript-diffwide2` | same | 35 rows the same; rows 9 and 23 `frame-variation-selector-anchor`, 14, 15, 28 and 29 `filler-modifier-count`, as at DIFF-WRAP | same |

No cell of any xfx `full` is past column 120 (`margin`), including the
overwrite rows.

## Sweep

Every `p5-transcript-*` scenario (24, `diffwrap2` among them), 40x120,
`--jobs 2`, fx v0.0.16. The ledger is the same on both sides, since this
unit changes none of it (sha256 `eb7bafc1…`):

- **base** (`sweep-base.md`): `dd0e4e21`'s build (sha256 `c719d242…`);
- **head** (`sweep-head.md`): `378a0839`'s build (`5800cffc…`).

`sweep-compare.txt`: 107 pairs each side, pass 44 and 44. Six verdicts moved:

| pair | base | head | why |
|---|---|---|---|
| `p5-transcript-diffwrap2/full` | unresolved-defect | normalized-identical | this unit |
| `p5-transcript-submit/filled`, `/second` | normalized-identical | identical | not a drop; the screens were the same on both runs |
| `p5-transcript-submit-multi/filled`, `/second` | normalized-identical | identical | the same |
| `p5-transcript-records/start` | normalized-identical | **unresolved-defect** | **timing, not this unit**; see below |

**`records/start` is a coin flip on both builds.** The snap is taken at launch,
after `wait 4` and before any tool call, so no diff row is on that screen.
The one row that differs is the footer's `Full access enabled: xfx
permission checks disabled` hint, which stays up for
`WARNING_VISIBLE` = 4000 ms (`src/tui/permissions.rs:38`): the snap and the
hint end at the same moment. `records-reruns.txt` runs the scenario alone three
times on each build: the base is unresolved 2 times in 3, the head 2 times
in 3. At DIFF-WRAP the same pair went the other way
(`../ctrlo-diff-wrap/sweep-compare.txt`). So no pair dropped because of this
unit. The scenario's wait against the hint's timer is a nondeterminism I
left as it is: it is neither a diff row nor this unit's scenario.

"Rows no cause covers": `diffwrap2/full`'s row leaves it, and
`records/start`'s comes back. Nothing else changed.

## Mutants

`mutants.py` (the DIFF-WRAP runner with `walked`'s mutants), on a
`git clone --no-local` moved to `378a0839` in the scratchpad (`GIT_DIR`,
`GIT_INDEX_FILE`, `GIT_WORK_TREE` unset for each run), each a one-place
change, then `cargo test --locked --lib -- tui::full_transcript`.
`mutants.txt`: **7 of 7 killed**.

| mutant | killed by |
|---|---|
| W1 the column kept past a row the walker runs on (the reviewed defect) | 4 tests, the run-on-row tab test among them |
| W2 a tab stop past the last column | 2 tests |
| W3 a tab on a full row moves nothing (DIFF-WRAP's departure 3) | the overwrite test |
| W4 nothing written over the last cell | the overwrite test |
| W5 no blank for a wide character's left cell | the overwrite test |
| W6 a tab's cells styled | 4 tests, T-U6's byte test among them |
| W7 a character that reaches the last column run on | 7 tests |

## Gates

In the worktree, with a short `TMPDIR` (the common brief's), one step at a
time, on `378a0839` (the receipt commit changes no `src/`):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 15 binaries, 5165 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 15 binaries, 5202 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |

`fmt`, the three `check-*` scripts and `scripts/fxcap/test-sweep.sh` were
run again on the head commit, with this receipt in it. No pty test failed.

## Files

- `fx/`, `xfx/`, `red/`: the four scenarios' `edited`, `full` and `closed`
  snaps, `.ansi` and `.txt`.
- `check.py`, `green.txt`, `red.txt`, `red-test.txt`.
- `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt` (DIFF-WRAP's
  `pairs_diff.py`), `records-reruns.txt`.
- `mutants.py`, `mutants.txt`.
