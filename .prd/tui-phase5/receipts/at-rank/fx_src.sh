#!/bin/bash
# Prints every fx v0.0.16 line AT-RANK cites, each range under its
# path:first-last@rev, read with `git show` (the checkout is never touched).
# `fxcites.py` checks every fx citation in this folder's README, in the
# source lines this unit added and in the scenario's header against these
# `q` ranges.
# usage: fx_src.sh <fx-upstream checkout>   (writes to stdout; fx-src.txt is its output)
set -euo pipefail
fx=$1
P=481104fd
q() { # path first last what
  printf '\n== %s:%s-%s@%s -- %s\n' "$1" "$2" "$3" "$P" "$4"
  git -C "$fx" show "$P:$1" | awk -v a="$2" -v b="$3" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
# What the list holds and how many rows a query keeps
q src/core/workspace/file_index.zig 1 9 "the module: ranked by its best query placement (alignBest), highlights from the same placement"
q src/core/workspace/file_index.zig 23 24 "max_indexed_files (100,000), max_path_len"
q src/core/workspace/file_index.zig 585 645 "searchAtRevision: an empty query is the index's order (601-611); rankTopN keeps min(out, 64) (617); the spans from the ranked placement (621-643)"
q src/core/workspace/file_index.zig 1086 1105 "appendDiscoveredCandidate: an added root's entries by their absolute paths"
q src/core/workspace/file_index.zig 1176 1176 "max_search_results = 64"
q src/core/input/file_completion_state.zig 7 7 "capacity = 32: the picker's out buffer"
q src/core/app/input_completion_runtime.zig 895 899 "the lookup's results[file_picker_completion_cap]"
q src/core/app/app_workspace_runtime.zig 39 50 "fileCompletionsAtRevision -> searchAtRevision"
# The score
q src/core/workspace/file_index.zig 1254 1309 "MatchScore (1254-1265); NameQuery: score (1286-1300), better (1302-1309)"
q src/core/workspace/file_index.zig 1330 1403 "rankTopN: the pruning (Needed, length_limit, 1344-1370), the alphaMask prefilter (1371-1373), insertion of the kept (1389-1398), sortRanked (1400)"
q src/core/workspace/file_index.zig 1405 1438 "fzf's v2 path-scheme score: the constants (1410-1423), AlignScratch (1427-1438)"
q src/core/workspace/file_index.zig 1440 1525 "Alignment; CharClass, charClassOf (1445-1457); bonusFor (1459-1475); bonusAt (1477-1481); runSlot (1483-1491); consecutiveBonus (1493-1496); scoreAlignment (1498-1521); betterCandidate (1523-1525)"
q src/core/workspace/file_index.zig 1527 1720 "alignBest: one letter (1546-1564), the greedy scans (1566-1589), one placement or past the cell budget (1591-1604), the sparse rows (1606-1617), the DP (1619-1688), the best and its path back (1690-1720)"
q src/core/workspace/file_index.zig 1722 1774 "name_bonus (1722-1727); maxCrossingQuality (1736-1743); maxNameQuality (1745-1751); exactFit (1753-1758); Needed, Placement"
q src/core/workspace/file_index.zig 1776 1829 "choosePlacement (1776-1819); isSubsequence (1821-1829)"
q src/core/workspace/file_index.zig 1831 1882 "scoreOf; scoreAsciiMatchWith, scoreFoldedMatchWith (1850-1882)"
q src/core/workspace/file_index.zig 1884 1918 "scoreBetter (1884-1890); rankedCandidateBetter (1892-1899); findWorstSlot; sortRanked"
q src/core/workspace/file_index.zig 1921 1983 "reconstructMatchSpans: the chosen search again (1934-1935); spansFromMatchedOffsets"
# A browsed folder
q src/core/workspace/path_completion.zig 44 131 "completeCancellable: each name scored by NameQuery (56, 93), inserted by better (106)"
q src/core/workspace/path_completion.zig 226 252 "insertCandidate: matcher.better (234)"
q src/core/workspace/path_completion.zig 497 516 "test: path completion fuzzy ranking retains the best bounded result"
# fx's own tests, transcribed
q src/core/workspace/file_index.zig 2033 2055 "TestSearchBuffer; expectFirstSearchPath"
q src/core/workspace/file_index.zig 2373 2442 "subsequence scoring tests; search returns best matches first; abbreviated subsequences (FIIDX)"
q src/core/workspace/file_index.zig 2444 2630 "the ranking table; ties; score tiers; name prefix, whole word, before an extension, vercle, abcd, comp, own folder only; highlights"
q src/core/workspace/file_index.zig 2633 2740 "bruteForceAlignment; best alignment equals a brute-force search; the greedy placement past the cell budget"
q src/core/workspace/file_index.zig 2775 2850 "pruned ranking keeps the results of scoring every path (4 cases); file and directory have no ranking priority"
q src/core/workspace/file_index.zig 2885 2907 "search is case-insensitive"
q src/core/workspace/file_index.zig 2909 2944 "search uses Unicode 17 simple folding (not transcribed: xfx folds with to_lowercase)"
q src/core/workspace/file_index.zig 3124 3164 "mixed Unicode basename and full-path scores; empty query in index order; no a-z letters"
# fx's latency gate
q benchmarks/file_index_bench.zig 1 31 "the shipping latency gate: 100,000 candidates, 32 results, p95 under 16 ms (16), the eight queries (24-31)"
q benchmarks/file_index_bench.zig 84 125 "generateInventory: the inventory this receipt's fixture tree is made from"
q benchmarks/file_index_bench.zig 283 289 "the gate: overall p95 < latency_budget_ns"
