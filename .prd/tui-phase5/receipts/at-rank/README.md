---
title: 0.0.16 delta -- AT-RANK (the @ list's order, fx's placement score)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker]
---

# AT-RANK

Base: `cb7c9c18` (CTRLO-AT-KEYS). At the base, xfx's `@` list still ranked
by c1db919's eight facts. fx v0.0.16 (`481104fd`, sha256 `8e6cda2e…`) ranks
by its placement score. This unit copies the score, the order and the bold
letters. fx's pruning, the part that is only about speed, is a separate
unit, AT-RANK-PERF (§5).

| commit | what |
|---|---|
| `20a42b58` | RED: fx's ranking cases over `search` and `rank_listing`; the scenario `p5-at-rank` and its workspaces; the keystroke benchmark (ignored) |
| `ebe38755` | the fix. **The build the pairs (§4) and the head benchmark (§5) come from.** |
| `bdee36f2`, `ab13a5df` | receipt files; fx citations in `src/tui/file_index.rs` corrected (comments only) |
| `a5b1cb59` | `docs/parity.md` (the `@` row); the ledger's `at-ranking` cause closed; refs (§8) |
| `dad8caa1` | tests that pin what fx's own cases leave free: the camel and punctuation bonuses, the 4096-cell budget, 32 rows written as a number (§6). **The build the sweep (§7) and the first gate run come from.** |
| `e2cba14b` | a test for `workspace-index` `at-plan`'s folder row, and its ledger cause (§7). **The second gate run's head (§9).** |
| `3958fb03`, `1f1962ad` | round 1's receipt; in `src/`, one test comment (two model-only scores taken out) |
| `e7661322` | **round 2** (the reviews): pins for the bold letters' repeated search, for ties, for the first letter; `is_subsequence` deleted; comments; the benchmark fails without its tree; `at-plan-run-directory` narrowed (§6-§8). **The mutants' clone (§6).** |
| (receipt r2) | this README, `at-plan-sweep/`, the re-scored sweep files, round 3's mutants, the third gate run (§9) |

**Size** (`size.sh`, `size.txt`, at `e7661322`): +525 −154 outside the tests,
net +371.
That is past the brief's "about 300". The dispatcher decided to keep it as
one unit: the score, the placement, the order and the bold letters are one
fx rule.

Every fx line cited here is printed in `fx-src.txt`, which `fx_src.sh`
generates. `fxcites.py` checks each citation against it: this README, the
source lines this unit added, the parity row's changed text and the
scenario header (`fxcites.txt`).

## 1. fx's rule (`file_index.zig@481104fd`)

- **Match.** A case-blind subsequence. The query and the path are folded
  (`prepareQuery`, `decodeFoldedPath`; ASCII paths are lower-cased). Case
  and word classes are read from the path's own bytes (`bonusAt`,
  `:1477-1481`).
- **Score of one placement** (`scoreAlignment`, `:1498-1521`), constants
  copied verbatim (`:1410-1423`):
  - 16 per letter, and the first letter's bonus counted twice;
  - a letter after `/` earns 9; after a space or other punctuation, 8; at a
    camel-case or digit start, 7; punctuation itself, 8;
  - a run keeps its first letter's bonus, at least 4 (`consecutiveBonus`,
    `:1493-1496`);
  - a gap costs 3, and 1 more for each position past its first.
- **Best placement** (`alignBest`, `:1527-1720`): the highest score, then
  the earliest start, found by a dynamic program over four run slots. One
  letter is found by a scan (`:1546-1564`). Past 4096 cells, the leftmost
  placement is scored instead (`:1591-1604`).
- **Which placement ranks the path** (`choosePlacement`, `:1776-1819`): the
  best one inside the name, plus 4 (`name_bonus`, `:1722-1727`), against a
  better one that starts in the name's own folder and runs into the name.
  Failing both, the best one anywhere, ranked in the folder tier below
  every name match -- whether or not it happens to end in the name, as
  `abcd` does in `ab/x/cd.ts` (`file_index.zig:2586-2597`).
- **Order** (`scoreBetter`, `:1884-1890`; `rankedCandidateBetter`,
  `:1892-1899`):
  1. an exact match: the path, the name, or the name before a dot
     (`exactFit`, `:1753-1758`);
  2. the name tier: a placement inside the name, or from its own folder
     into it;
  3. the higher score;
  4. the earlier start;
  5. the shorter path;
  6. byte order.

  Files and directories have no priority over each other (`:2829-2850`).
- **Rows kept:** the 32 best (`capacity`, `file_completion_state.zig:7`;
  `rankTopN` keeps `min(out, 64)`, `file_index.zig:617`). An empty query
  shows the index in its own order (`:601-611`). Each keystroke searches
  the whole index again (`app_workspace_runtime.zig:39-50`).
- **Added directories** are entries by their absolute paths
  (`file_index.zig:1086-1105`), scored like any other entry.
- **Bold letters** are the placement that set the rank
  (`reconstructMatchSpans`, `:1921-1983`). A browsed folder ranks its names
  by the same score (`NameQuery`, `path_completion.zig:56`, `:93`, `:234`).

## 2. The copy (`src/tui/file_index.rs`, ranking section)

`Score` holds fx's four tiers. `score_alignment`, `align_best`,
`choose_placement`, the exact rule and `spans` are fx's functions.
`align_best` keeps fx's transition order -- the diagonal before the gap,
the last row read first to last, each strict -- so placements equal in
score and start are bold on the same letters, and two cases pin it
(`of_two_placements_as_good_and_as_early_the_dynamic_programs_order_picks_the_bold_letters`, §6).

Each search reuses one `Scratch`, a grow-on-demand workspace: its vectors
grow to the longest path and the largest table seen and are not freed
between paths. It is not allocation-free: each row found allocates its
bold letters' offsets, and the list of matches reserves room for every
entry (fx keeps a bounded top 64). A one-pass subsequence check (`holds`)
runs before any scoring and stands for fx's `isSubsequence` in
`choosePlacement` (`file_index.zig:1804`), which is why xfx has no second
check there.

Not copied: fx's pruning (`Needed`, `length_limit`) and its a–z bitmap
(`rankTopN`, `file_index.zig:1344-1373`). fx's tests show that they change
no result (`file_index.zig:2775-2827`). They are AT-RANK-PERF's (§5).

## 3. RED → GREEN (`red-unit.txt`, `green-unit.txt`)

- **At `20a42b58`:** 6 failed, 35 passed in `tui::file_index`. The
  failures are the ranking table, name prefix and whole word, the
  own-folder and folder-ending matches, the bold letters, a browsed
  folder's names, and `p5-at-rank`'s rows. The pins (ties, case, an added
  directory's paths) pass.
- **At `ebe38755`:** 45 passed. Added with the fix: fx's brute-force
  equivalence check (over 600,000 placements), the cell-budget case, the
  score tiers and fx's many-row cases.
- **Later commits:** `dad8caa1` and `e2cba14b` add three more cases, 48 in
  all (§6, §7); round 2's `e7661322` three more, 51.
- One base test pinned c1db919's order for `main.rs`. It now pins fx's
  order (`main-rs` in §4).

## 4. Pairs (`pairs.py`, `pairs.txt`; `fx/`, `xfx/`, `xfx-base/`)

The scenario `p5-at-rank` at 40x120, captured on fx v0.0.16, on the base
(`v0.1.0-cb7c9c1`) and on the head (`v0.1.0-ebe3875`), styles compared:

- **base:** 3 of 13 snaps identical;
- **head:** 13 of 13 identical.

The ledger's `at-ranking` snaps are `p5-input-at`'s `at-st`,
`at-st-down12`, `at-st-up6` and `at-stat`, and `workspace-index`'s
`at-plan`. On the head, 5 of 5 are identical, with the run directories
masked. Run directories are written `<tmp>` on every side. (In this
`at-plan` pair the two run names led to the same bold letters; §7 shows a
pair where they did not.)

## 5. Performance (`bench-base.txt`, `bench-head.txt`, `gen_tree.py`)

The fixture is fx's benchmark inventory (`benchmarks/file_index_bench.zig:
84-125`) written to disk as a tree and walked to the 100,000-entry cap. The
benchmark runs fx's eight queries and times every prefix, 1,460 searches.
Release build, run alone under the suite lock, on a shared 18-core machine.

| run | load | p50 / p95 per keystroke | whole queries p95 |
|---|---|---|---|
| base (`20a42b58`) | 39.8–40.2 | 32.7 / 46.7 ms | 51.8 ms |
| head (`ebe38755`), run 1 | 54.8–75.7 | 3.3 / 11.0 ms | 12.0 ms |
| head (`ebe38755`), run 2 | 36.8–43.5 | 5.6 / 25.1 ms | 20.7 ms |

- **The bar: 8 ms p95 per keystroke, not met yet.** It is the event loop's
  `TICK` (`src/tui/event_loop.rs`); the lookup runs on the thread that
  paints. AT-RANK-PERF owns closing it.
- **fx's own gate is 16 ms p95** (`benchmarks/file_index_bench.zig:16`).
  The head meets it in run 1 and misses it in run 2.
- **The head is 6 to 10 times the base's speed at p50**, but load moves
  the tail: run 2's p50 is 1.7 times run 1's.
- **The slow keys** are the low-selectivity ones (`ae`, `ashr`, `scwfi`).
  fx's pruning cuts exactly those.
- **The benchmark is `#[ignore]`d**, so no gate runs it today, and since
  `e7661322` it fails rather than passes when `XFX_AT_BENCH_TREE` is unset.
  AT-RANK-PERF makes it a gate. It is that unit's RED (`bench-head.txt`).
- **AT-RANK-PERF's scope** (`src/tui/file_index.rs` at `e7661322`):
  - **(a) fx's pruning:** `Needed`, `length_limit` and the a–z bitmap
    (`rankTopN`, `file_index.zig:1344-1373`), with a bounded top set.
  - **(b) work fx does once, at index time:** fx stores each path's
    lower-cased bytes, name start and a–z mask as the index is built
    (`fillProgressive`, `file_index.zig:209`, `:216-220`, `:222`). xfx
    redoes it for every path on every keystroke: `holds` folds every
    scalar (`file_index.rs:1011-1015`), `Scratch::load` folds again into
    two vectors (`file_index.rs:715-723`), and `loaded` finds the name's start
    (`file_index.rs:1023-1030`). The no-match query's floor, 2.9-5.0 ms
    (`bench-head.txt`), is about this cost alone.
  - **(c) a narrower scratch:** `Align`'s tables are `usize`
    (`file_index.rs:700-712`) where fx's are u16 and u8
    (`AlignScratch`, `file_index.zig:1427-1438`); and every table call
    clears and refills `starts`, `pred_entries` and `pred_slots`
    (`file_index.rs:834-839`), which are written before they are read,
    where fx resets only the score slots of a cell it visits
    (`file_index.zig:1634`).

## 6. Mutants (`mutants.py`, `mutants-r1.txt`, `mutants-r2.txt`, `mutants-r3.txt`, `equivalent-mutants.txt`)

Each mutant runs on a `git clone --no-local` throwaway with `cargo test
--locked --lib -- tui::file_index tui::file_picker tui::shell`, a short
TMPDIR, each round in one hold of the suite lock, 900 s each. None failed
to build; the clone was clean after.

- **Round 1** (`ab13a5df`, the fix's tests): 24 run, 17 killed.
- **Round 2** (`dad8caa1`, round 1's seven survivors): 6 killed.
- **Round 3** (`e7661322`, every mutant, round 2's review adding three):
  **27 run, 26 killed, 1 equivalent** (`mutants-r3.txt`). This is the
  head's result; rounds 1 and 2 are kept as the history of the pins.

"Killed by" is round 3's, summarised; the log lists every killer.

| mutant | killed by (round 3) |
|---|---|
| `per-letter-dropped` (16 → 0) | **survives, equivalent**: every placement of a query has the same number of letters, so the score drops by 16·m everywhere (`equivalent-mutants.txt`) |
| `first-letter-not-doubled` | `the_first_letters_bonus_counts_twice`, and `p5-at-rank` |
| `after-slash-bonus-dropped` (9) | own folder, a browsed folder, name prefix and whole word, and more |
| `word-start-bonus-dropped` (8 after space and punctuation) | a browsed folder, own folder, name prefix and whole word, and more |
| `camel-digit-bonus-dropped` (7), `punctuation-bonus-dropped` (8) | `a_camel_start_and_a_matched_punctuation_earn_their_bonuses` |
| `run-floor-dropped` (4) | the brute-force equivalence, and one `tui::shell` case |
| `gap-cost-dropped`, `gap-extension-dropped` | the folder row bold through the whole path, a browsed folder, name prefix, and more |
| `name-bonus-off` (+4) | own folder, the bold letters of the name's placement, `p5-at-rank` |
| `folder-placement-off` | own folder, `main.rs`, `p5-at-rank` |
| `exact-dot-rule-dropped` | name prefix and whole word, `p5-at-rank` |
| `exact-reversed`, `name-reversed`, `quality-reversed`, `first-reversed` | the score tiers, the ranking table, `p5-at-rank`, and more |
| `shorter-path-reversed`, `bytes-reversed` | the ties cases, the ranking table, the listing cases, and more |
| `earlier-start-tie-reversed` (inside the DP) | the brute-force equivalence |
| `cap-31`, `cap-33` | `only_the_best_thirty_two_are_kept` |
| `cells-4095`, `cells-4097`, `greedy-fallback-gone` | `the_cell_budget_is_four_thousand_and_ninety_six_cells` |
| `spans-no-rerun` (the bold letters from whichever search ran last) | `the_bold_letters_are_the_names_placement_when_the_folders_loses` |
| `dp-gap-before-diagonal` (the gap offered first) | `of_two_placements_as_good_and_as_early_the_dynamic_programs_order_picks_the_bold_letters` |
| `final-pick-ge` (the last tying cell of the last row) | the same case, and one `tui::shell` case |

## 7. Sweep (`sweeps.sh`; `sweep-base.md`, `sweep-head.md`, `sweep-compare.txt`, `at-plan-sweep/`)

`sweep.sh` compares fx v0.0.16 (sha checked before and after each run)
against the base (`v0.1.0-cb7c9c1`) and the head (`v0.1.0-dad8caa`).
Settings: 40x120, `--jobs 2`, `--only 'at*' 'p5-input-at*' 'filepicker*'
'p5-ctrlo*' 'p5-at-rank' 'workspace-index'` (20 scenarios, 99 pairs).
Both sweeps ran in one hold of the suite lock. Each was then re-scored
(`--rescore`) with the ledger at `e7661322`. No other scenario types an `@`
query; `p5-mcp-*` type `@BUN@` and `@SCENARIOS@` as words.

- **Base 64/99 → head 78/99: 14 moved to passing, 0 dropped, 0 other
  verdict changes.** The 14 are 10 `p5-at-rank` snaps and the 4
  `p5-input-at` `at-st`/`at-stat` snaps.
- **The head's 21 pairs that do not pass are all owned:**
  - 15 `p5-ctrlo-wider-*` snaps: 11 approved divergences, 3 owned by
    `ri-run-pairing` and 1 by `resize-document-repaint`;
  - 5 `p5-input-at-resize` snaps (`at-resize-band-at-the-bottom`);
  - `workspace-index` `at-plan`, row 13, attributes only. That row is the
    added folder `<tmp>/fxcap.<run>/extra/notes`, which `@plan` matches only
    through its whole path. Its bold letters therefore depend on the run
    directory's random name. fx's run `fxcap.qnVERa` bolded the run name's
    `n`; xfx's `fxcap.YTZTDF`, which has no `n`, bolded the `n` of `notes`
    (`at-plan-sweep/bold.txt`, from the two captures there, redacted by
    `at-plan-sweep/redact.py`; the sweep's own diff line is
    `at-plan-sweep/at-plan.diff`). Each is the best placement in its own
    path. `e2cba14b` pins xfx's rule on both shapes and adds the ledger
    cause `at-plan-run-directory`; `e7661322` narrows it (§8).
- With that cause, the head has **no row that no cause covers**
  (`sweep-head.md`). It covers 1 row of 1 snap there and nothing else. On
  the base sweep it covers the same row (`sweep-base.md`), whose
  difference there is the old ranking's: a selector of row and text cannot
  tell the two apart, since both differ only in attributes. The base's
  other ranking rows are listed as uncaused, as they should be once
  `at-ranking` is closed.

## 8. The parity row, the ledger, refs

- **`docs/parity.md`, the `@` file picker row:** "ranked on its eight
  facts" is replaced by fx's placement score. The new text gives the
  score's terms, the placement, the order, the bold letters and a browsed
  folder's names, each cited at the pin. It names the case-folding
  difference as unit **CASE-FOLD-SIMPLE, queued**. The receipt list points
  here.
- **The ledger:** `at-ranking` is closed. A comment in its place says that
  AT-RANK made its five snaps identical. `at-plan-run-directory` (§7)
  covers row 13 only (`rows = [13]`), both regexes anchored to the whole
  normalized row (`^  /\S+/fxcap\.XXXXXX/extra/notes/$`), and is owned by
  **SWEEP-TEMPNAME-STYLE** (queued): capture.sh's run directory under a
  fixed name, the same for fx's run and xfx's.
- **Refs:** `refs.py` (CTRLO-AT-KEYS' version, base `cb7c9c18`) checked 17,
  0 wrong (`refs.txt`). No ref names `file_index.rs`.

## 9. Gate (`gates.sh`, `gate.txt`)

`gates.sh` (CTRLO-AT-KEYS' version) ran in a `--no-local` clone, the whole
script in one hold of the suite lock, TMPDIR a short scratch directory.
`gate.txt` has both runs. Before them, `units.txt` recorded fmt, both
clippys and the touched modules' 1147 tests at `bdee36f2`.

- **At `dad8caa1`: 20 steps, 0 failed.** `cargo test`: 5560 passed, 0
  failed. With `fault-injection`: 5597 passed, 0 failed. smoke-tui: 37
  scenarios and the oracle, 1168 checks, 0 failures.
- **At `e2cba14b`: 20 steps, 0 failed.** `cargo test`: 5561 passed, 0
  failed. With `fault-injection`: 5598 passed, 0 failed (the one case
  `e2cba14b` added). smoke-tui: 1168 checks, 0 failures. The machine's
  load was 27–114 over the run.
- Since `e2cba14b`, `src/` has changed in one test comment only.

## 10. Queue items and open questions

- **AT-RANK-PERF** (§5): fx's pruning and a bounded top set; the per-path
  work fx does at index time; a narrower scratch; the 8 ms bar as a gate.
- **CASE-FOLD-SIMPLE:** xfx folds case with `to_lowercase`; fx uses Unicode
  simple case folding. So `σ` and `ς` match each other in fx and not in
  xfx (`file_index.zig:2909-2944`). fx's test of it is not transcribed
  here. Not fixed here; `fold`'s comment says so.
- **SWEEP-TEMPNAME-STYLE** (§7, §8): the run directory's name fixed, so a
  path through it is placed alike on both sides.
- No user decision is needed.
