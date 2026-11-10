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
| `b47953e2` | the change: fx's pruned top set, the facts an entry keeps from the walk, the narrower scratch; AT-RANK's search kept as a test-only reference and the cases against it; the benchmark un-ignored, `scripts/check-at-search-cost.sh`, its CI step and its `test-gate-env.sh` case. **The build the pairs (§4) and the benchmark runs (§5) come from; round 1's mutants' clone (§7).** |
| `a2fdef29` | `docs/parity.md` (the `@` row); in `src/`, fx citations in comments corrected (comments only, §9) |
| `c8f41359` | one case, from round 1's surviving mutant: a crossing at its ceiling still enters (§7). **The gate's head and round 2's clone.** |
| (receipt) | this folder |

Every fx line cited here and in the source lines this unit added is printed
in `fx-src.txt`, which `fx_src.sh` generates (AT-RANK's ranges, which the
reference still cites, and this unit's). `fxcites.py` checks each citation
against it (`fxcites.txt`).

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
- **The order is total**: two entries with the same path (a file and a
  directory of one name, which `merged` keeps) tie on everything fx
  compares; `rank_top` keeps the earlier in the index. AT-RANK left their
  order to `select_nth_unstable_by`.
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
- **The reference is a separate test-only file**,
  `src/tui/file_index/reference.rs`: AT-RANK's ranking section, lines
  527-1152 of `4a5dc3c7:src/tui/file_index.rs`, verbatim from its line 17
  (`size.txt` checks it), sharing nothing with the search it checks.
- **The gate is a release-part script**, as the output check's is
  (`check-tui-preflight-cost.sh`): the test asserts under optimization
  only, alone (§6).
- **Memory**: an entry keeps its lower-cased bytes beside its path, a
  second copy of every path as fx's `lower_buf` is, and its name start and
  mask.

## 3. Equivalence (`unit-head.txt`)

`same_as_at_rank` asserts that `search` and `reference::search` give the
same rows, kinds and bold letters, query by query:

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
  `Résumé`, `ファイル`, `x/`): 139 prefixes on each corpus, 278 searches
  (`prefixes.py`, its line at the end of `unit-head.txt`).
- **The facts** (`each_entry_keeps_its_lower_case_its_name_and_its_letters`,
  fx's `buildFromRaw` cases, `file_index.zig:2092-2128`), **the mask**
  (`a_path_without_one_of_the_querys_letters_is_never_placed`), and **a
  refresh and an added directory** searched by their own paths
  (`a_refresh_and_an_added_directory_are_searched_by_their_own_paths`: a
  real walk, a rename, a second walk, then an added directory; every
  adopted entry equal to `Candidate::new` of its own path).

**Result: all identical.** At the head, `tui::file_index` and
`tui::file_picker`: 85 passed, 0 failed (`unit-head.txt`) -- AT-RANK's
79 and its benchmark, which is no longer ignored, and the five above.
**Every AT-RANK case is unchanged**: against the base, the test lines
removed are the `candidates` helper's literal (now `Candidate::new`) and
the benchmark's `#[ignore]`, tree and assertion guard (§6); no other
case's line changed.

## 4. Pairs (`pairs.py`, `pairs.txt`; `xfx/`)

The scenario `p5-at-rank` (13 snaps) and the five snaps of the ledger's old
`at-ranking` cause (`p5-input-at`'s `at-st`, `at-st-down12`, `at-st-up6`,
`at-stat`; `workspace-index`'s `at-plan`) at 40x120, captured on xfx built
at `b47953e2` (row 1: `v0.1.0-b47953e [debug]`, `caps.sh`) and compared
with AT-RANK's fx v0.0.16 captures (`../at-rank/fx/`, the same pin and
binary), styles compared, the run directory written `<tmp>` on both sides
(at-folders round 7's `runid.py redact`):

- **`p5-at-rank`: 13 of 13 identical**, as at AT-RANK's head;
- **the five `at-ranking` snaps: 5 of 5 identical**, as at AT-RANK's head.
  In `at-plan` the run directory's random name decides row 13's bold
  letters (AT-RANK §7, the ledger cause `at-plan-run-directory`); here the
  two names led to the same letters, so the pair is identical, as
  AT-RANK's §4 pair was.

No scenario was added or changed; the later commits change no product
line (`a2fdef29` comments, `c8f41359` a test, §9).

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
  the test fails or if the harness did not run exactly one test (a rename).
- **CI**: `.github/workflows/ci.yml`, step "The @ search still fits inside
  a frame", on the release part of every row, after the output check.
- **`scripts/test-gate-env.sh`** plants the rename under `GREP_OPTIONS=-v`
  and requires the script to refuse it (`at-search-cost`).

**The gate run** (`gates.sh`, `gate.txt`): AT-RANK's script with this
unit's step added, in a `--no-local` clone at `c8f41359`, the whole script
in one hold of the suite lock, TMPDIR a short scratch directory.

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

## 7. Mutants (`mutants.py`, `mutants-r1.txt`, `mutants-r2.txt`)

Each mutant runs on a `git clone --no-local` throwaway with `cargo test
--locked --lib -- tui::file_index tui::file_picker tui::shell`, a short
TMPDIR, each round in one hold of the suite lock, 900 s each.

- **Round 1** (`b47953e2`): 15 run, 14 killed, 0 did not build.
  `crossing-cut-at-its-ceiling` survived: no case had the worst kept at
  the crossing ceiling itself. `c8f41359` adds one (§3).
- **Round 2** (`c8f41359`, the head, every mutant again): **15 run, 15
  killed, 0 did not build**. The clone was clean after each round.

"Killed by" is round 2's, summarised; the log lists every killer.

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
are killed only by the case that pins the predicate itself, and would
otherwise show only as time. Since the facts live in the entry and
`Candidate::new` is the only constructor, the two stale-facts mutants
model the two ways an entry could carry another path's facts: carried over
from the old index on adoption, and kept across the relative-to-absolute
rename of an added directory.

## 8. Size (`size.sh`, `size.txt`)

`size.txt`, base `4a5dc3c7` against the head `c8f41359`:

- **`src/tui/file_index.rs` outside its tests: +298 -113, net +185**
  (56 of the added are comments, 10 blank) -- under the brief's "about 300".
- Inside its tests: +382 -23.
- `src/tui/file_index/reference.rs`, compiled only under `cfg(test)`: 642
  lines, of which 626 are AT-RANK's ranking section, checked identical to
  `4a5dc3c7:src/tui/file_index.rs` lines 527-1152.
- The gate: `scripts/check-at-search-cost.sh` 60 lines, `ci.yml` +9,
  `test-gate-env.sh` +7.

## 9. Citations (`fx_src.sh`, `fx-src.txt`, `fxcites.py`, `fxcites.txt`)

`fx_src.sh` prints AT-RANK's ranges -- the reference cites them, verbatim
-- with fx's benchmark inventory read at `benchmarks/file_index_bench.zig:82-135` (AT-RANK's
`benchmarks/file_index_bench.zig:84-125` left out `candidateKind` and the end of `generateInventory`),
and this unit's: `alphaMask`,
`non_ascii_mask`, `max_search_results` (`file_index.zig:1164-1176`), `prepareQuery`
(`file_index.zig:1208-1215`), the `buildFromRaw` cases (`file_index.zig:2092-2128`). `fxcites.py`
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
  47-109. The repository's own record of the output-cost gate on CI
  (`.prd/tui-phase3/loop.md:581`, R195) has `macos-15-intel` at 6.55-10.32
  ms over six measurements where the other three runners measured 2.8-4.5
  ms and a local M-series machine 1.2 ms, and records the rule: if the
  Intel row's floor exceeds the budget, the choice is the user's (a
  per-runner budget, an optimisation unit, or dropping that archive), not
  a change of estimator. This unit has not run CI (no push); if the new step fails on
  that row, that is the decision. The bar and the estimator are left as
  the brief set them: 8 ms at p95 over every keystroke.
- **CASE-FOLD-SIMPLE** (queued, AT-RANK §10) is untouched: the reference and
  the search fold alike, so the equivalence holds either way, and that
  unit changes both or neither.
- **SWEEP-TEMPNAME-STYLE** (queued, AT-RANK §8): `at-plan` is identical
  here by the luck of two run names (§4).
