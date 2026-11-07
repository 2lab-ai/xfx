# DIFF-WRAPc receipt: a mark right after a tab in a Ctrl-O diff row is not drawn, as on fx

The opus review of DIFF-WRAPb (`373ddb39`) raised SHOULD-FIX 1 as PLAUSIBLE:

- `walked` joined a cluster of no cells to the styled cell before it and
  kept that cell's width.
- After a tab that adds no blanks (a column at or past the last one),
  nothing stood between the two in the row text.
- The frame segments a row again (`wrap::painting`, `frame::clip`; only an
  escape is a boundary) and measures what it finds.

The convergence rule reopened it, for a RED to decide. **It reproduces.**

- **Branch:** `feat/p5-ctrlo-diff-wrap`, on DIFF-WRAPb's head `373ddb39`.
- **The code:** `3a243867`. Every `xfx/` capture is from a debug build of
  it (banner `v0.1.0-3a24386`). The commit after it adds this receipt:
  `git diff 3a243867 HEAD -- src/ scripts/ docs/` is empty.
- **`red/`:** a debug build of `373ddb39` (banner `v0.1.0-373ddb3`), built
  in a `--no-local` clone.
- **fx:** the v0.0.16 reference binary, sha256 `8e6cda2e…`, checked before
  the captures; each run's guard line said it was unchanged.

## RED

- **Unit test** (`red-test.txt`), written first and run on `373ddb39`'s
  code: `a_mark_right_after_a_tab_is_not_drawn_as_fxs_screen_does_not_draw_it`
  fails at its first assertion, through the frame. For `\t`, 102 `k`,
  `1\t` U+FE0F `Z` at 120 columns, `walked` counted the row as 120 cells,
  and `frame::clip(row, 120)` returned it without its `Z` and its reset:
  the frame measured `1` U+FE0F as two cells. This is the reviewer's
  repro, failing as predicted.
- **Pairs** (`red.txt`): 18 of 20 claims hold; `p5-transcript-diffwrap2`'s
  `full` and `rows` fail on the four new lines whose mark the base draws:
  - line 15: `…k1️` (the `Z` is gone) where fx shows `…k1Z`;
  - line 17: `…méZ` where fx shows `…meZ`;
  - lines 18 and 19: the mark drawn on the tab's last blank
    (`p      ́q`, `r      ️s`) where fx shows `p      q`, `r      s`.
  Line 16 (the mark over a full row's last cell) was already fx's: the
  overwrite pops the joined cell.

## fx's screen

The reviewer could not say what fx draws for a mark after a tab. The
capture is the oracle, so `p5-transcript-diffwrap2` gained lines 15-20:

| line | text | fx's screen |
|---|---|---|
| 15 | `\t`, 102 `k`, `1\t` U+FE0F `Z` | `…k1Z` |
| 16 | `\t`, 103 `l`, `1\t` U+FE0F `ZW` | `…lZ`, then `W` |
| 17 | `\t`, 102 `m`, `e\t` U+0301 `Z` | `…meZ` |
| 18 | `p\t` U+0301 `q` | `p      q` |
| 19 | `r\t` U+FE0F `s` | `r      s` |
| 20 | `t` U+0301 `\tu` | `t́      u` |

So neither a U+FE0F nor a U+0301 right after a tab is drawn, whether the tab
moved the column (18, 19) or not (15, 17); one before a tab is (20). fx's
walker writes a unit of no cells where its column is
(`full_transcript_screen.zig:4017-4035@481104fd`; `width > 0` guards only
the wrap). The capture says the screen leaves nothing of it after a tab
[추정: the terminal has no character cell to join it to after the tab;
the capture is the evidence, not this reason].

## What xfx does now

- **`walked`** (`src/tui/full_transcript.rs`) remembers whether the last
  cluster was a tab, and drops a cluster of no cells that comes right after
  one.
- **It joins nothing.** A control ends a grapheme cluster (UAX #29 GB5), so
  a cluster of no cells stands alone only right after a tab. Anywhere else
  it is already part of the cluster before it (`wrap::first_cluster`). The
  join branch, and its comment "as the painter joins it", are gone.
- **The one branch left** keeps such a cluster as a cell of its own only if
  it were the row's first cluster, which no diff row has: every row opens
  with the gutter.

The doc comment and `docs/parity.md:238` say this. A mark is dropped, not a
character, and fx's screen drops it too, so no ledger row is needed.

## GREEN

`green.txt`: 20 of 20 claims hold.

- `p5-transcript-diffwrap2` `full` is the same as fx's on every row, its
  29-row diff block too. Its `.ansi` rows 20-30 are byte-identical to fx's,
  including the unstyled tab blanks.
- `diffwrap` and `diffwide` are the same as fx's on every row.
- `diffwide2` differs only in the rows of the named causes, as before.
- No cell is past column 120.

## Sweep

Every `p5-transcript-*` scenario (24), 40x120, `--jobs 2`, fx v0.0.16. The
ledger is the same on both sides, since this unit changes none of it
(sha256 `eb7bafc1…`):

- **base** (`sweep-base.md`): `373ddb39`'s build (sha256 `596daf6f…`);
- **head** (`sweep-head.md`): `3a243867`'s build (`8caa1f2b…`).

`sweep-compare.txt`: 107 pairs each side, pass 44 → 45, **0 dropped**. Two
verdicts moved, both up:

- `p5-transcript-diffwrap2/full`, unresolved-defect → normalized-identical
  (this unit);
- `p5-transcript-submit/submit-0.15`, normalized-identical → identical.

"Rows no cause covers" goes from 14 snaps to 13: `diffwrap2/full`'s row left.
`records/start`, DIFF-WRAPb's coin flip
(`../ctrlo-diff-wrap-b/records-reruns.txt`), came out normalized-identical on
both builds this time.

## Mutants

`mutants.py` (DIFF-WRAPb's runner, with three mutants for the mark), on a
`git clone --no-local` moved to `3a243867` in the scratchpad (`GIT_DIR`,
`GIT_INDEX_FILE`, `GIT_WORK_TREE` unset for each run), each a one-place
change, then `cargo test --locked --lib -- tui::full_transcript`.
`mutants.txt`: **10 of 10 killed**.

| mutant | killed by |
|---|---|
| C1 a mark after a tab kept as a cell of its own | the new test (`frame::clip` cuts the `Z`) |
| C2 no tab ever counted as the last cluster | the new test |
| C3 DIFF-WRAPb's join restored | the new test |
| W1-W7, DIFF-WRAPb's walker mutants | as in `../ctrlo-diff-wrap-b/mutants.txt`, each still killed |

## Gates

In the worktree, with a short `TMPDIR` (the common brief's), one step at a
time, on `3a243867` (the receipt commit changes no `src/`):

| gate | result |
|---|---|
| `cargo fmt --check` | ok |
| `cargo clippy --locked --all-targets -- -D warnings` | ok |
| `cargo clippy --locked --all-targets --features fault-injection -- -D warnings` | ok |
| `cargo test --locked --no-fail-fast` | 15 binaries, 5166 passed, 0 failed, 7 ignored |
| `cargo test --locked --features fault-injection --no-fail-fast` | 15 binaries, 5203 passed, 0 failed, 7 ignored |
| `scripts/check-xfx-identity.sh` | ok |
| `scripts/check-no-stubs.sh` | ok |
| `scripts/check-no-secrets.sh` | ok |

`fmt`, the three `check-*` scripts and `scripts/fxcap/test-sweep.sh` were
run again on the head commit, with this receipt in it. No pty test failed.

## Files

- `fx/`, `xfx/`, `red/`: the four scenarios' `edited`, `full` and `closed`
  snaps, `.ansi` only (what `check.py` reads; `capture-pane -e` keeps the
  text in it). The `.txt` copies are left out: with them,
  `scripts/check-no-secrets.sh` hands `grep` every tracked path at once and
  passed the system's argument limit (`Argument list too long`, 12 294 files);
  without them it scans 12 258 and passes.
- `check.py` (DIFF-WRAPb's with `diffwrap2`'s new block), `green.txt`,
  `red.txt`, `red-test.txt`.
- `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`.
- `mutants.py`, `mutants.txt`.
