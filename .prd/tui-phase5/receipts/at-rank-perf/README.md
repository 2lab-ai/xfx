---
title: 0.0.16 delta -- AT-RANK-PERF (the @ search within a frame, fx's pruning)
type: receipt
tags: [xfx, tui, phase5, fx-0.0.16, input, file-picker, performance]
---

# AT-RANK-PERF

Base: `4a5dc3c7` (AT-RANK, closed). AT-RANK copied fx v0.0.16's placement
score (`481104fd`, sha256 `8e6cda2e…`) but placed every path in full on
every keystroke, on the thread that paints: p95 11.0 and 25.1 ms a
keystroke over 100,000 entries (`../at-rank/bench-head.txt`) against the
8 ms `TICK`. This unit copies what fx does for speed, changes no row, kind
or bold letter, and makes the 8 ms bar a gate.

| commit | what |
|---|---|
| `b47953e2` | the change: fx's pruned top set, the facts an entry keeps from the walk, the narrower scratch; AT-RANK's search kept as a test-only reference and the cases against it; the benchmark un-ignored, `scripts/check-at-search-cost.sh`, its CI step and its `test-gate-env.sh` case. **The build the benchmark runs (§5) come from; round 1's mutants' clone (§7).** |
| `a2fdef29` | `docs/parity.md` (the `@` row); in `src/`, fx citations in comments corrected (comments only, §9) |
| `c8f41359` | one case, from round 1's surviving mutant: a crossing at its ceiling still enters (§7). **Round 1's gate head and round 2's mutants' clone.** |
| `7bc1d23c` | round 1's receipt |
| `4db582d8` | **review round 2** (§11): the cost gate requires its measurement; a work count pins each cut; the unpruned oracle replaces `reference.rs`; the same-path, 2048-byte and sigma cases; `Candidate.path` private. **The build the pairs (§4) come from; the head's gate and round 3's mutants' clone.** |
| (receipt r2) | this README, round 2's files |

Every fx line cited here and in the source lines this unit added is printed
in `fx-src.txt`, which `fx_src.sh` generates (AT-RANK's ranges and this
unit's). `fxcites.py` checks each citation against it (`fxcites.txt`).

## 1. What was copied (`src/tui/file_index.rs`)

- **The bounded top set** (`rankTopN`, `file_index.zig:1330-1403`): `rank_top`
  keeps the best `MOST_RESULTS` (32) as it goes. fx keeps `min(out.len, 64)`
  (`file_index.zig:617`, `max_search_results` `file_index.zig:1176`), and the picker's `out` holds 32
  (`file_completion_state.zig:7`). Once the set is full, the worst kept
  decides what an entry may still need (`file_index.zig:1344-1370`):
  - **no folder placement** once the worst is exact or a name match;
  - **no crossing placement** once the worst is exact or a name match
    past `max_crossing_quality` (strictly past, `file_index.zig:1368`);
  - **the length limit**: once the worst is exact, only an exact match is
    placed; once it is the best a name can score (`maxNameQuality`,
    `file_index.zig:1745-1751`) from its first letter, a longer path is placed only if it
    is exact.

  An exact match is always placed in full (`scoreAsciiMatchWith`,
  `file_index.zig:1850-1864`). After a replacement the worst is found again
  (`findWorstSlot`, `file_index.zig:1901-1907`), and the kept rows are sorted at the end.
- **The letter mask** (`file_index.zig:1371-1373`; `alphaMask`, `file_index.zig:1164-1173`;
  `non_ascii_mask`, `file_index.zig:1175`): an ASCII path is placed only if the query is
  ASCII and every letter of it is among the path's; a path with any byte
  past ASCII is always placed (`admits`).
- **What fx works out once, as it builds the index** (`fillProgressive`,
  `file_index.zig:209-222`): each `Candidate` keeps its lower-cased bytes, its name's
  first byte and its mask, made in `Candidate::new`, its only constructor,
  on the walk's worker thread -- the workspace's entries in `merged`, an
  added directory's on their absolute paths in `discover_roots`. A refresh
  builds new entries, so their facts are their own. An ASCII path is placed
  on its kept bytes; any other is folded scalar by scalar as before
  (`scoreFoldedMatchWith`, `file_index.zig:1866-1882`), as fx decodes it per search.
- **The narrower scratch** (`AlignScratch`, `file_index.zig:1427-1438`): the cell tables
  are fx's widths (`scores` i32, `starts` u16, `pred_entries` u16,
  `pred_slots` u8, `columns` u16). They grow to the largest table seen and
  are never cleared: a cell's four scores are reset when the program reaches
  it (`file_index.zig:1634`), and a start or predecessor is read only behind a score
  written with it.
- **fx's subsequence check** (`isSubsequence`, `file_index.zig:1802-1804`, `file_index.zig:1821-1829`)
  is where fx has it, in `choose_placement` once the name has no placement.
  AT-RANK's `holds`, which folded every scalar of every path before any
  scoring, is gone.

## 2. Decisions (each reversible)

- **32 slots, not 64**: fx's `top_cap` is `min(out.len, 64)` and the
  picker's `out` is 32; the set fx keeps for the picker is 32.
- **The order is total, and here it departs from fx**: two entries with
  the same path (a file and a directory of one name, which `merged` keeps)
  tie on everything fx compares; `rank_top` keeps and lists the earlier in
  the index. **This order follows index position where fx's follows the
  slot**: `findWorstSlot` keeps the earliest slot among equals
  (`file_index.zig:1901-1907`) and a replacement fills the evicted slot, so
  fx can evict the earlier-indexed one of the pair. AT-RANK left their order
  to `select_nth_unstable_by`. Pinned by
  `a_path_listed_twice_keeps_its_place_in_the_index` (round 2).
- **`forward`, `backward`, `row_offsets`, `positions` stay `usize`**, where
  fx's are u16: they hold one entry per query letter, not the cell tables'
  bytes, and AT-RANK's `align_best` cases read `positions` as `usize`
  unchanged.
- **An ASCII path's offsets are a static identity table** (`IDENTITY`),
  where fx passes none (`bonusAt`, `file_index.zig:1477-1481`): one `align_best`, generic
  over its text's element, serves both kinds of path.
- **Past `MOST_PATH_BYTES` (2048), no score**, as fx scores no longer name
  (`NameQuery.score`, `file_index.zig:1287`). No index entry is longer (`admissible`), and
  a browsed folder's row that long is dropped after scoring anyway
  (`rank_listing`), so no row changes; it bounds the u16 tables.
- **The oracle is the unpruned search itself** (round 2): `unpruned` in
  the tests places every entry with `score` and every placement, sorts the
  whole order stably and keeps 32 -- no kept set, no cut, no mask, none of
  the kept facts, an ASCII path read scalar by scalar. Round 1's oracle was
  AT-RANK's ranking section copied whole (`reference.rs`, 642 lines, in
  history at `7bc1d23c`). What only that copy covered is the shared code's
  rewrite against AT-RANK's own -- the generic `align_best`, the narrowed
  scratch, `choose_placement` with `isSubsequence`, `exact_fit`. That was
  checked once, identical over 285 searches at `c8f41359` (round 1's
  `unit-head.txt`, at `7bc1d23c`), and is pinned since by AT-RANK's own
  cases: the brute-force search kills both cell-reset mutants, and the
  ranking table, `p5-at-rank`'s rows and the pairs hold. Nothing of the copy
  is kept: it was a second `fold` that CASE-FOLD-SIMPLE would have to
  change too.
- **`Candidate.path` is private to the module** (round 2): its facts are
  its path's, and no other module read it.
- **The gate is a release-part script**, as the output check's is
  (`check-tui-preflight-cost.sh`): the test asserts under optimization
  only, alone (§6).
- **Memory**: an entry keeps its lower-cased bytes beside its path, a
  second copy of every path as fx's `lower_buf` is, and its name start and
  mask.

## 3. Equivalence (`unit-head.txt`)

`same_as_unpruned` asserts that `search` and `unpruned` (§2) give the same
rows, kinds and bold letters, query by query:

- **fx's own pruning cases** (`file_index.zig:2775-2827`), transcribed in
  their order, which is not byte order, so a later entry has to displace
  the worst kept: `stat`; `s` and `st` over crowded names; `main` once
  every kept row is exact; `vercle` with and without the perfect names.
  Plus one of this unit's: `ab` over forty crossings at the ceiling and a
  shorter one last (§7). 7 searches.
- **Every prefix of 20 queries over two corpora** -- `p5-at-rank`'s
  workspace as walked, and a generated tree: the first 2,400 entries of
  fx's benchmark inventory and crowds of exact names, perfect names of many
  lengths, crossings, folder matches, mixed case, scalars past ASCII and
  absolute paths, each path once, in a fixed shuffle. The queries are fx's
  eight benchmark queries and twelve more (`stat`, `MAIN.zig`,
  `service/main`, `vercle`, `road_map`, `abcd`, `comp`, `scm`, `index`,
  `Résumé`, `ファイル`, `x/`, and since round 2 `σigma` and `Σς` over
  paths with a final `ς` and a capital `Σ`): 146 prefixes on each corpus,
  292 searches (`prefixes.py`, its line at the end of `unit-head.txt`).
- **Round 2's cases**: a file and a directory of one path at the cut-off,
  the pair before and after the crowd, either kind first (4 searches); an
  ASCII path of 2048 bytes placed and one of 2049 not (1).
- **The facts** (`each_entry_keeps_its_lower_case_its_name_and_its_letters`,
  fx's `buildFromRaw` cases, `file_index.zig:2092-2128`), **the mask**
  (`a_path_without_one_of_the_querys_letters_is_never_placed`), and **a
  refresh and an added directory** searched by their own paths
  (`a_refresh_and_an_added_directory_are_searched_by_their_own_paths`: a
  real walk, a rename, a second walk, then an added directory; every
  adopted entry equal to `Candidate::new` of its own path).

**Result: all identical.** At `4db582d8`, `tui::file_index` and
`tui::file_picker`: 88 passed, 0 failed (`unit-head.txt`) -- AT-RANK's
79 and its benchmark, which is no longer ignored, round 1's five, and
round 2's three: the same-path case, the 2048-byte case and the work
count (§6).
**Every AT-RANK case is unchanged**: against the base, the test lines
removed are the `candidates` helper's literal (now `Candidate::new`) and
the benchmark's `#[ignore]`, tree and assertion guard (§6); no other
case's line changed.

## 4. Pairs (`pairs.py`, `pairs.txt`; `xfx/`)

The scenario `p5-at-rank` (13 snaps) and the five snaps of the ledger's old
`at-ranking` cause (`p5-input-at`'s `at-st`, `at-st-down12`, `at-st-up6`,
`at-stat`; `workspace-index`'s `at-plan`) at 40x120, captured on xfx built
at `4db582d8`, round 2's head (row 1: `v0.1.0-4db582d [debug]`, `caps.sh`;
round 1's captures, at `b47953e2`, are in history at `7bc1d23c` and gave
the same verdicts) and compared
with AT-RANK's fx v0.0.16 captures (`../at-rank/fx/`, the same pin and
binary), styles compared, the run directory written `<tmp>` on both sides
(at-folders round 7's `runid.py redact`):

- **`p5-at-rank`: 13 of 13 identical**, as at AT-RANK's head;
- **the five `at-ranking` snaps: 5 of 5 identical**, as at AT-RANK's head.
  In `at-plan` the run directory's random name decides row 13's bold
  letters (AT-RANK §7, the ledger cause `at-plan-run-directory`); here the
  two names led to the same letters, so the pair is identical, as
  AT-RANK's §4 pair was.

No scenario was added or changed.

## 5. Performance (`bench-run1.txt`, `bench-run2.txt`)

The benchmark is AT-RANK's: fx's eight queries, every prefix, 23 passes,
the first 3 dropped, 1,460 searches. Release lib test binaries of the base
(`4a5dc3c7`) and the head (`b47953e2`), run one after another in one hold
of the suite lock, each alone (`bench-all.sh`, `bench.sh`). Two fixtures:
AT-RANK's walked tree (`../at-rank/gen_tree.py`, walked to the 100,000 cap)
and fx's inventory in memory (100,000 entries, `fx_inventory`, what the
gate searches). The machine is shared, 18 cores.

| run | build | fixture | load (1 min) before -> after | p50 / p95 per keystroke | whole queries p95 | 8 ms |
|---|---|---|---|---|---|---|
| 1 | base `4a5dc3c7` | walked tree | 102.9 -> 106.6 | 4.65 / **14.12** ms | 16.88 ms | fails |
| 1 | head `b47953e2` | walked tree | 106.6 -> 108.7 | 0.37 / **4.23** ms | 4.33 ms | holds |
| 1 | head `b47953e2` | fx's inventory | 108.7 -> 108.7 | 0.55 / **5.19** ms | 6.85 ms | holds |
| 2 | base `4a5dc3c7` | walked tree | 42.1 -> 47.3 | 3.05 / **9.56** ms | 10.67 ms | fails |
| 2 | head `b47953e2` | walked tree | 47.3 -> 48.5 | 0.25 / **3.48** ms | 3.66 ms | holds |
| 2 | head `b47953e2` | fx's inventory | 48.5 -> 48.5 | 0.26 / **3.41** ms | 3.53 ms | holds |
| gate | head `c8f41359` | fx's inventory | 73.4 -> 71.0 | 0.33 / **4.32** ms | 4.65 ms | holds |

- **The head is 12 to 13 times the base's speed at p50 and 2.7 to 3.3
  times at p95**, on the same tree, run after the base in the same hold.
  The base fails its own bar in both runs; the head holds it in all five,
  at loads from 47 to 109 on 18 cores. fx's own gate, 16 ms over whole
  queries (`benchmarks/file_index_bench.zig:16`), is met with room.
- **The slowest queries are `exact`, `low-selectivity` and `long-gap`**
  (each run's per-query lines: p95 4.5-7.4 ms on the head in run 1,
  3.5-4.2 ms in run 2); `no-match` and `unicode` stay under 0.8 ms. What
  is left is fx's own algorithm (§1); nothing past it is copied.
- The base's two runs here (14.1 and 9.6 ms) and AT-RANK's two runs of
  its fix (11.0 and 25.1 ms at `ebe38755`, `../at-rank/bench-head.txt`)
  measure the same full search at different loads.

## 6. The gate

- **The test** `a_keystroke_over_a_hundred_thousand_entries_costs_less_than_a_frame`
  is no longer ignored. Without `XFX_AT_BENCH_TREE` it builds fx's
  inventory in memory (`fx_inventory`, `benchmarks/file_index_bench.zig:82-135`).
  Under optimization it times every prefix and fails past 8 ms p95; in a
  debug build -- the ordinary `cargo test` -- it searches each whole query
  once and asserts nothing.
- **`scripts/check-at-search-cost.sh`** runs it alone in a release build
  (`cargo test --locked --release --lib -- --exact --test-threads=1`),
  prints the load average before and after and the figures, and fails if
  the test fails, if the harness did not run exactly one test (a rename),
  or -- since round 2 -- if the line `every keystroke: 1460 searches` is
  missing: a build that kept debug assertions searches once per query,
  asserts nothing and passed before (`r2-gate-script-red-green.txt`: the
  case's command against round 1's script exits 0, against round 2's 1).
- **CI**: `.github/workflows/ci.yml`, step "The @ search still fits inside
  a frame", on the release part of every row, after the output check.
- **`scripts/test-gate-env.sh`** plants the rename under `GREP_OPTIONS=-v`
  and requires the script to refuse it (`at-search-cost`), and since round
  2 a run that passed and measured nothing (`at-search-measured`,
  `XFX_TEST_CARGO_RAN=1 XFX_TEST_CARGO_LINES=0`).
- **The work each cut saves, counted** (round 2,
  `a_keystroke_places_no_more_than_its_cuts_allow`, in the ordinary debug
  suite on every row): a `cfg(test)` count of `align_best`'s searches over
  every prefix of fx's eight queries on its 100,000-entry inventory, with a
  ceiling per query at today's count (607,118 for `exact` ... 7 for
  `ärf`). It needs no clock. Switching a cut off passes a ceiling
  (`r2-counts.txt`, `count.sh`): without the folder cut, six of the eight
  queries (`Klv` 58,773 to 88,760); without the crossing cut, `scwfi`
  (38,703 to 40,827) and `api-route` (189,720 to 190,141); without the
  length limit, five (`scwfi` 38,703 to 192,991).

**Round 1's gate run** (`gates.sh`, `gate.txt`): AT-RANK's script with this
unit's step added, in a `--no-local` clone at `c8f41359`, the whole script
in one hold of the suite lock, TMPDIR a short scratch directory. Round 2's
is §11.

- **21 steps, 0 failed**; the tree clean before and after; load 37-106.
- `cargo test`: 5570 passed, 0 failed (AT-RANK's 5564 at `e7661322`, and
  six: the five cases of §3 and the cost test, no longer ignored; in this
  debug run it searched once per query and asserted nothing). With
  `fault-injection`: 5607 passed, 0 failed.
- `check-at-search-cost.sh`: ok, p95 4.32 ms a keystroke, load 73.4 before
  and 71.0 after (the table above; its whole output is in `gate.txt`). It
  ran in 2 s: the release lib test binary was the one the output check's
  step had built.
- `test-gate-env.sh`: 63 cases, `at-search-cost` among them.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.

## 7. Mutants (`mutants.py`, `mutants-r1.txt`, `mutants-r2.txt`, `mutants-r3.txt`)

Each mutant runs on a `git clone --no-local` throwaway with `cargo test
--locked --lib -- tui::file_index tui::file_picker tui::shell`, a short
TMPDIR, each round in one hold of the suite lock, 900 s each.

- **Round 1** (`b47953e2`): 15 run, 14 killed, 0 did not build.
  `crossing-cut-at-its-ceiling` survived: no case had the worst kept at
  the crossing ceiling itself. `c8f41359` adds one (§3).
- **Round 2** (`c8f41359`, every mutant again): 15 run, 15 killed, 0 did
  not build.
- **Round 3** (`4db582d8`, the head, review round 2's four added): **19
  run, 19 killed, 0 did not build**. The clone was clean after each round.
  - `folder-cut-never` (`folder: true`), `crossing-cut-never`
    (`crossing: true`), `length-limit-never` (both `limit = None`): each cut
    switched off changes no row, so each survived every case until round
    2; now each is killed by the work count (§6).
  - `ascii-guard-at-2048` (`>=` for `>` on the ASCII path's length): killed
    by the 2048-byte case.

"Killed by" below is round 2's for the first fifteen, summarised; round 3
killed them all again, with the oracle in place of the copy, and adds the
work count to the killers of `best-name-without-its-bonus` and the three
mask mutants. Each log lists every killer.

| mutant | what it breaks | killed by (round 2) |
|---|---|---|
| `folder-cut-whenever-full` | no folder placement once the set is full | the equivalence over the generated tree |
| `crossing-cut-whenever-full` | no crossing once the set is full | fx's pruning cases, the equivalence |
| `crossing-cut-at-its-ceiling` | the crossing cut at `max_crossing_quality`, `>=` for fx's `>` | the ceiling case (`c8f41359`) |
| `length-limit-cuts-an-equal-length` | `>=` for `>`: a path as long as the worst is cut | fx's pruning cases, the equivalence |
| `length-limit-cuts-an-exact-match` | the limit cuts exact matches too | fx's pruning cases, the equivalence, the kept-rows case |
| `best-name-without-its-bonus` | `maxNameQuality` without `name_bonus`: the limit never or wrongly set | the equivalence |
| `worst-not-found-again` | the worst not found again after a replacement | fx's pruning cases, the equivalence |
| `mask-any-shared-letter` | the mask admits a path with one of the query's letters | the mask case |
| `mask-gone` | every ASCII path admitted for an ASCII query | the mask case |
| `mask-non-ascii-bit-ignored` | a path past ASCII judged by its a-z letters | the mask case, the Unicode case, the equivalence |
| `stale-facts-after-a-refresh` | an adopted refresh keeps the old index's facts, position by position | the refresh case, one `tui::shell` case |
| `stale-facts-on-an-added-directory` | an added directory's entry keeps its relative path's facts | the refresh case |
| `name-start-not-kept` | every name taken to start at byte 0 | the facts case, and AT-RANK's own-folder, name-prefix and added-directory cases |
| `cell-reset-gone` | a cell's scores never reset | the brute-force case, `p5-at-rank`'s, and more |
| `cell-reset-after-the-first-row` | the first row's cells not reset | the brute-force case, `p5-at-rank`'s, the equivalence |

**A mask that admits more changes no row** -- the dynamic program refuses
what the mask let through -- so `mask-any-shared-letter` and `mask-gone`
are killed by the case that pins the predicate itself and, since round 2,
by the work count, and would otherwise show only as time. Since the facts live in the entry and
`Candidate::new` is the only constructor, the two stale-facts mutants
model the two ways an entry could carry another path's facts: carried over
from the old index on adoption, and kept across the relative-to-absolute
rename of an added directory.

## 8. Size (`size.sh`, `size.txt`)

`size.txt`, base `4a5dc3c7` against the head `4db582d8`:

- **`src/tui/file_index.rs` outside its tests: +313 -114, net +199**
  (63 of the added are comments, 10 blank) -- under the brief's "about
  300". Round 1 was +298 -113.
- Inside its tests: +500 -23. `reference.rs` (642 lines at `7bc1d23c`) is
  gone.
- The gate: `scripts/check-at-search-cost.sh` 70 lines, `ci.yml` +9,
  `test-gate-env.sh` +12.

## 9. Citations (`fx_src.sh`, `fx-src.txt`, `fxcites.py`, `fxcites.txt`)

`fx_src.sh` prints AT-RANK's ranges, with fx's benchmark inventory read
at `benchmarks/file_index_bench.zig:82-135` (AT-RANK's
`benchmarks/file_index_bench.zig:84-125` left out `candidateKind` and the
end of `generateInventory`), and this unit's: `alphaMask`,
`non_ascii_mask`, `max_search_results` (`file_index.zig:1164-1176`),
`prepareQuery` (`file_index.zig:1208-1215`), the `buildFromRaw` cases
(`file_index.zig:2092-2128`), and since round 2 the picker's cap
(`file_picker_completion_cap`, `input_completion_runtime.zig:44` at the
pin; a reviewer read it two lines up, in a later tree) and the inventory's
arrays (`benchmarks/file_index_bench.zig:34-70`). `fxcites.py`
(AT-RANK's, for this folder) checks every `file.zig:N` in this README, in
the parity row's changed text and in every source line this unit added
(`fxcites.txt`). `a2fdef29` corrected what it found in `b47953e2`'s
comments: `prepareQuery` cited at lines 1197-1204 (that is `QueryScratch`),
the inventory at lines 84-125, and two bare `:N`. That commit changes no
other source line (`git diff -U0 b47953e2 a2fdef29 -- src`: every changed
line a comment).

## 10. Open question and queue

- **Open, for a user decision once CI has run: the 8 ms bar on the
  x86_64 macOS runner.** This machine measures p95 3.4-5.2 ms at load
  47-109. The repository's own record of the output-cost gate on CI:
  - `.prd/tui-phase3/loop.md:581` (R195): `macos-15-intel` at 6.55-10.32
    ms over six measurements where the other three runners measured
    2.8-4.5 ms and a local M-series machine 1.2 ms;
  - `.prd/tui-phase3/loop.md:585` (R197): on that runner even the fastest
    single pass was over the bar, 8.09 and 8.69 ms -- its floor, not its
    noise, about 7.5 times the local machine.

  **x86_64-apple-darwin is a shipped archive** (`.github/workflows/release.yml:41-42`
  builds it, `.github/workflows/release.yml:146-150` requires it), so a miss on that runner is evidence about the product on
  Intel Macs, not about runner noise. loop.md:581 records that such a miss
  is the user's choice and not a change of estimator; the order of
  preference the reviews put to it is: an optimisation unit first, then a
  documented miss for Intel Macs, and a per-row bar last, since that one
  hides the miss. The dispatcher runs CI on the branch after this round;
  this unit has not (no push). The bar and the estimator are left as the
  brief set them: 8 ms at p95 over every keystroke.
- **CASE-FOLD-SIMPLE** (queued, AT-RANK §10) is untouched: the oracle and
  the search share `fold`, so that unit changes both at once; the corpus's
  sigma paths (§3) will show its change.
- **SWEEP-TEMPNAME-STYLE** (queued, AT-RANK §8): `at-plan` is identical
  here by the luck of two run names (§4).

## 11. Review round 2 (`4db582d8`)

Reviews of `7bc1d23c` (opus, gpt6), both APPROVE-WITH-FIXES; both found the
pruning fx's and no row able to differ from AT-RANK's.

- **MUST (opus): the cost gate passed a run that measured nothing.** Fixed
  (§6): `check-at-search-cost.sh` requires `every keystroke: 1460
  searches`; `test-gate-env.sh`'s `at-search-measured` plants the run. RED
  on round 1's script, GREEN on round 2's (`r2-gate-script-red-green.txt`).
  The other MUST of both reviews, CI on the runners, is the dispatcher's.
- **SHOULD, done:** the work count with three cut mutants (§6, §7); the
  unpruned oracle in place of `reference.rs`, sigma paths in the corpus
  (§2, §3); the same-path case and fx's slot order said in §2;
  `Candidate.path` private (§2); the two fx citations (§9); §10.
- **NIT, done:** the 2048-byte case and its mutant (§3, §7); the three bare
  citations in `file.zig:N` form; a `debug_assert!` on the u16 column
  positions.
- **Product lines changed in round 2** (`file_index.rs` above its tests,
  not comments): `path` made private, the `cfg(test)` search count and its
  increment, and the `debug_assert!`. None runs in a release build, and
  none can move a row.

**Round 2's gate run** (`gates.sh`, `gate-r2.txt`): the same script in a
`--no-local` clone at `4db582d8`, one hold of the suite lock.

- **21 steps, 0 failed**; the tree clean before and after; load 4.9-17.1
  (the machine was quieter than in round 1).
- `cargo test`: 5573 passed, 0 failed (round 1's 5570 and round 2's
  three); with `fault-injection` 5610 passed, 0 failed. The work count
  passed in the debug suite.
- `check-at-search-cost.sh`: ok, p95 3.79 ms a keystroke, load 14.4 before
  and 13.9 after, its `every keystroke: 1460 searches` line present. This
  build counts `align_best`'s searches (`cfg(test)`), so this is the
  head's figure with the count in it.
- `test-gate-env.sh`: 64 cases, `at-search-cost` and `at-search-measured`
  among them.
- smoke-tui: 37 scenarios and the oracle, 1168 checks, 0 failures.
- Pairs at `4db582d8`: 13 of 13 and 5 of 5 identical (§4).
