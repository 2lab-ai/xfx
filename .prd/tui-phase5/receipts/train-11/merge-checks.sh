#!/usr/bin/env bash
# Train 11's merge checks (train 10's merge-checks.sh, its merges, sides and
# checks this train's), written into this directory through redacted.py: each
# merge's conflicted and both-changed files checked from both ends
# (rowcheck.py, filecheck.py, regroup.py; merge 2's two files with its
# semantic fixes taken out, semantic.py --reverse, in a clone) and every
# resolution redone in a clone (reproduce.sh); the token check and its
# negative control (train 7's token_check.py, byte for byte) with
# token_control.py's plants on the merged rows; the remerge-diffs; the four
# reference scans at the merges' head before the re-point and at <head>; the
# re-points against their code (repointed.py); the stale references; join
# check 3's shared functions and loop order; the helpers each join test
# names; the overlaps; and checks selection.txt.
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MERGES=(4a0b900b 0040fa59 d2ed0274)
CODE_HEAD=d2ed0274
REPOINT=6ae87955
SIDES=(main=32afd307 submit-card-preview=0c203cd3 catalog-refresh=3ffbe0c7 paste-gap=5cc2e42c)
{
  echo "# Each merge's conflicted and both-changed files, checked from both ends against its first parent (ours), second (theirs) and merge base."
  echo "## merge 1, 4a0b900b (SUBMIT-CARD-PREVIEW): docs/parity.md's full-screen TUI row"
  $P/rowcheck.py "$repo" 58c92ba2 32afd307 0c203cd3 4a0b900b docs/parity.md '| full-screen TUI |'; echo "# exit $?"
  echo "## merge 1: the code files both sides changed (event_loop.rs and shell.rs held one conflict each)"
  $P/filecheck.py "$repo" 58c92ba2 32afd307 0c203cd3 4a0b900b src/tui/event_loop.rs src/tui/shell.rs src/tui/transcript.rs; echo "# exit $?"
  echo "## merge 1: the ledger, capfile.py and tests/tui.rs with every reference number masked (their numbers-only blocks kept ours')"
  $P/filecheck.py --numbers-masked "$repo" 58c92ba2 32afd307 0c203cd3 4a0b900b scripts/fxcap/parity-ledger.toml scripts/fxcap/capfile.py tests/tui.rs; echo "# exit $?"
  echo "## merge 2, 0040fa59 (CATALOG-REFRESH): docs/parity.md's /model row"
  $P/rowcheck.py "$repo" e705a5dc 4a0b900b 3ffbe0c7 0040fa59 docs/parity.md '| `/model` |'; echo "# exit $?"
  echo "## merge 2: the code files both sides changed, at the merge commit (shell.rs and worker.rs hold the semantic fixes: False expected)"
  $P/filecheck.py "$repo" e705a5dc 4a0b900b 3ffbe0c7 0040fa59 src/interactive.rs src/tui/bridge.rs src/tui/event_loop.rs src/tui/shell.rs src/tui/worker.rs; echo "# exit $?"
  echo "## merge 2: shell.rs and worker.rs with the four semantic fixes taken out (semantic.py --reverse, in a clone)"
  rm -rf "$scratch/sem" && git clone -q --no-local "$repo" "$scratch/sem" && git -C "$scratch/sem" checkout -q --detach 0040fa59
  $P/semantic.py --reverse "$scratch/sem"; echo "# exit $?"
  git -C "$scratch/sem" diff --stat | tail -1
  $P/filecheck.py "$scratch/sem" e705a5dc 4a0b900b 3ffbe0c7 - src/tui/shell.rs src/tui/worker.rs; echo "# exit $? (worker.rs's False from theirs' end expected: git diff groups ours' appended tests differently, regroup.py below)"
  $P/regroup.py "$scratch/sem" e705a5dc 4a0b900b 3ffbe0c7 - src/tui/worker.rs; echo "# exit $?"
  rm -rf "$scratch/sem"
  echo "## merge 3, d2ed0274 (PASTE-GAP): docs/parity.md's full-screen TUI row"
  $P/rowcheck.py "$repo" a832f4d7 0040fa59 5cc2e42c d2ed0274 docs/parity.md '| full-screen TUI |'; echo "# exit $?"
  echo "## merge 3: the files both sides changed (event_loop.rs held the conflict)"
  $P/filecheck.py "$repo" a832f4d7 0040fa59 5cc2e42c d2ed0274 scripts/smoke-tui.sh src/tui/event_loop.rs src/tui/shell.rs tests/tui.rs; echo "# exit $?"
  echo "## every resolution redone by this folder's scripts in a clone (reproduce.sh)"
  bash "$here/reproduce.sh" "$repo" "$scratch"
} 2>&1 | rd "$here/resolutions.txt"
{
  echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  $P/token_check.py "$repo" "${MERGES[@]}"; echo "# exit $?"
} | rd "$here/token-check.txt"
{
  for m in "${MERGES[@]}"; do $P/token_check.py "$repo" --self-test "$m"; echo "# exit $?"; done
  echo
  echo "# token_control.py: the two token cases planted on the merged row itself"
  $P/token_control.py "$repo" "${MERGES[@]}"; echo "# exit $?"
} | rd "$here/token-self-test.txt"
i=0
for m in "${MERGES[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
for s in refs refs_all refs_short refs_bare; do
  $P/$s.py "$repo" "$CODE_HEAD" "${SIDES[@]}" | rd "$here/$(echo "$s" | tr _ -)-pre.txt"
  $P/$s.py "$repo" "$head" "${SIDES[@]}" | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
$P/repoint.py "$repo" --derive | rd "$here/repoint-plan.txt"
$P/repointed.py "$repo" "$head" "$REPOINT" | rd "$here/repointed.txt"
$P/stale.py "$repo" "$head" | rd "$here/stale.txt"
$P/shared.py "$repo" "$head" main=32afd307 submit-card-preview=0c203cd3:58c92ba2 catalog-refresh=3ffbe0c7:e705a5dc \
  paste-gap=5cc2e42c:a832f4d7 | rd "$here/shared.txt"
$P/loop_order.py "$repo" "$head" 32afd307 0c203cd3 3ffbe0c7 5cc2e42c | rd "$here/loop-order.txt"
$P/helpers.py "$repo" | rd "$here/helpers.txt"
$P/overlap.py "$repo" "${MERGES[@]}" | rd "$here/overlap.txt"
$P/selection.py "$repo" 32afd307 "$head" | cmp -s - "$here/selection.txt" && echo "selection.txt: the same as selection.py writes at $head" || echo "selection.txt: DIFFERENT from what selection.py writes at $head"
