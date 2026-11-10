---
title: 0.0.16 delta -- AT-RANK (the @ list's order, fx's placement score)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-RANK

Base: `cb7c9c18` (CTRLO-AT-KEYS). xfx's `@` list still ranked with c1db919's
eight facts; fx v0.0.16 (`481104fd`, sha256 `8e6cda2e…`) ranks by its
placement score. This unit copies the score, the order and the bold letters.

| commit | what |
|---|---|
| `20a42b58` | RED: fx's ranking cases over `search` and `rank_listing`, the scenario `p5-at-rank` and its workspaces, the keystroke benchmark (ignored) |
| `ebe38755` | the fix -- **the build the captures and the head benchmark come from** |
| (receipt) | this folder; fx citations in `src/tui/file_index.rs` corrected (comments only) |

**Status: stopped at the brief's size rule.** The source change outside the
tests is +516/-145 lines, the ranking section 322 → 692 lines (`git diff
--stat cb7c9c18 -- src/`), past "about 300", and it is the score alone: fx's
pruning is not in it. Mutants, the sweep, the full gate, the parity row and
the ledger are not done. §6 proposes the split.

Every fx line cited here is printed in `fx-src.txt` (`fx_src.sh`); `fxcites.py`
(CTRLO-AT-KEYS', with this folder and scenario) checks every citation in this
README, in the source lines this unit added and in the scenario header
(`fxcites.txt`).

## 1. fx's rule (`file_index.zig@481104fd`)

- **Match.** A case-blind subsequence (query and path folded, `prepareQuery`,
  `decodeFoldedPath`; ASCII paths lowered), case and word classes read from
  the path's own bytes (`bonusAt`, `:1477-1481`).
- **Score of one placement** (`scoreAlignment`, `:1498-1521`), constants
  verbatim (`:1410-1423`): 16 a letter; the first letter's bonus doubled;
  a letter after `/` 9, after a space or other punctuation 8, a camel or digit
  start 7, punctuation itself 8; a run keeps its first bonus, at least 4
  (`consecutiveBonus`, `:1493-1496`); a gap costs 3, and 1 more for each
  further position.
- **Best placement** (`alignBest`, `:1527-1720`): highest score, then the
  earliest start, by a dynamic program over four run slots; one letter by a
  scan (`:1546-1564`); past 4096 cells, the leftmost placement
  (`:1591-1604`).
- **Which placement ranks the path** (`choosePlacement`, `:1776-1819`): the
  best inside the name plus 4 (`name_bonus`, `:1722-1727`), or a better one
  from the name's own folder into the name; failing both, the best anywhere,
  which ends in a folder.
- **Order** (`scoreBetter`, `:1884-1890`; `rankedCandidateBetter`,
  `:1892-1899`): exact (the path, the name, or the name before a dot,
  `exactFit`, `:1753-1758`), then ends in the name, then the score, then the
  earlier start, then the shorter path, then bytes. Files and directories
  have no priority (`:2829-2850`).
- **Rows kept:** the 32 best (`capacity`, `file_completion_state.zig:7`;
  `rankTopN` keeps `min(out, 64)`, `file_index.zig:617`). An empty query is
  the index's order (`:601-611`). Each keystroke searches the whole index
  again (`app_workspace_runtime.zig:39-50`).
- **Added directories** are entries by their absolute paths
  (`file_index.zig:1086-1105`), scored like any other.
- **Bold letters** are the placement that set the rank
  (`reconstructMatchSpans`, `:1921-1983`). A browsed folder ranks its names
  by the same score (`NameQuery`, `path_completion.zig:56`, `:93`, `:234`).

## 2. The copy (`src/tui/file_index.rs`, ranking section)

`Score` is fx's four tiers; `score_alignment`, `align_best` (fx's transition
order kept, so equal-score ties land on the same letters), `choose_placement`,
the exact rule and `spans` are fx's functions. One `Scratch` per search: no
allocation per path. A one-pass subsequence check comes before any scoring.
Not copied: fx's pruning (`Needed`, `length_limit`) and its a-z bitmap
(`rankTopN`, `file_index.zig:1344-1373`); fx's tests say they change no
result (`file_index.zig:2775-2827`).

## 3. RED → GREEN (`red-unit.txt`, `green-unit.txt`)

At `20a42b58`: **6 failed, 35 passed** in `tui::file_index` (the ranking
table, name prefix and whole word, own-folder and folder-ending matches, the
bold letters, a browsed folder's names, `p5-at-rank`'s rows); the pins
(ties, case, an added directory's paths) pass. At `ebe38755`: **45 passed**,
with fx's brute-force equivalence (more than 600,000 placements), the
cell-budget case, the score tiers and fx's many-rows cases added. One base
test pinned c1db919's order for `main.rs`; it now pins fx's (`main-rs`
below).

## 4. Pairs (`pairs.py`, `pairs.txt`; `fx/`, `xfx/`, `xfx-base/`)

`p5-at-rank` (40x120): fx v0.0.16, the base (`v0.1.0-cb7c9c1`), the head
(`v0.1.0-ebe3875`). **Base 3/13 identical, head 13/13**, styles compared.
The ledger's `at-ranking` snaps (`p5-input-at` `at-st`, `at-st-down12`,
`at-st-up6`, `at-stat`; `workspace-index` `at-plan`): **head 5/5
identical**. Run directories are `<tmp>` on every side.

## 5. Performance (`bench-base.txt`, `bench-head.txt`, `gen_tree.py`)

The tree is fx's benchmark inventory (`benchmarks/file_index_bench.zig:
84-125`) written to disk and walked to the 100,000-entry cap; fx's eight
queries, every prefix timed (1,460 searches), release build, under the suite
lock, the machine at load 40-75 on 18 cores.

| | base (load 40) | head run 1 (load 76) | head run 2 (load 37-43) |
|---|---|---|---|
| every keystroke p50 / p95 | 32.7 / 46.7 ms | 3.3 / 11.0 ms | 5.6 / 25.1 ms |
| whole queries p95 (fx's gate shape) | 51.8 ms | 12.0 ms | 20.7 ms |

**The number held to: 8 ms p95 a keystroke**, the event loop's `TICK`
(`src/tui/event_loop.rs`): the lookup runs on the thread that paints. fx's
own gate is 16 ms p95 (`benchmarks/file_index_bench.zig:16`); the assertion
is one constant in the benchmark if fx's number is preferred. The head is
about ten times the base's speed at p50 and **misses 8 ms in both runs**,
and fx's 16 ms in the second; the shared machine's load moves the tail
(run 2's p50 is 1.7 times run 1's). The slow keys are the low-selectivity
ones (`ae`, `ashr`, `scwfi`), which fx's pruning cuts. The benchmark is
`#[ignore]`d: no gate runs it.

## 6. The split proposed

- **A (this branch):** AT-RANK as committed: the score, order and bold
  letters, +371 net outside the tests (`size.txt`: +516 -145; 437 code
  lines added). It does not split further: the score, the placement and the
  order are one rule, and the bold letters are that placement's. Then
  mutants, the sweep, the gate, the parity row and the ledger's `at-ranking`
  cause closed.
- **B, AT-RANK-PERF:** fx's pruning (`rankTopN`, `file_index.zig:1344-1373`:
  `Needed`, `length_limit`, the bitmap), not written [estimate: 60-80
  lines], held to 8 ms by the benchmark already in the tree, which is its
  RED today (`bench-head.txt`).
- Not proposed: order without the bold letters (wrong letters on screen
  between the two PRs), or the engine first with its wiring later (dead code
  under `-D warnings`).

## 7. Open questions

- The split (§6): one bit -- go on with A at +371 lines (mutants, sweep,
  gate, parity row, ledger), or not.
- Not run: the mutants (so "would the tests fail if reverted" is
  unanswered), the sweep (`at*`, `p5-input-at*` -- a listing with a query
  such as `p5-input-at-folders` `sto-query` has a unit test here but no
  screen -- `filepicker*`, `p5-ctrlo*`, `p5-input-at-resize`) and the full
  gate. Run (`units.txt`): fmt, both clippys, and `tui::file_index`, `tui::file_picker`,
  `tui::shell`, `tui::event_loop` (`units.txt`: 1147 passed).
- Not this unit's: xfx folds with `to_lowercase` where fx uses Unicode simple
  folding (`σ` and `ς` match each other in fx; `file_index.zig:2909-2944`); fx's test of
  it is not transcribed here.
