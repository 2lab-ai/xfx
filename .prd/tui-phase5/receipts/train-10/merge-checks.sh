#!/usr/bin/env bash
# Train 10's merge checks, written into this directory through redacted.py:
# each merge's resolution checked from both ends (rowcheck.py, filecheck.py)
# and redone by this folder's scripts in a clone (reproduce.sh); the token
# check and its negative control over the merges (train 7's token_check.py,
# byte for byte) with token_control.py's plants on the merged rows; the
# remerge-diffs; the four reference scans at the code head before the
# re-point and at <head>; the re-points against their code (repointed.py);
# the stale references; the paste invariant; the shared functions; the
# overlaps. Train 9's merge-checks.sh with this train's merges and sides.
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MERGES=(13e7c9a0 33e476fa 5a98001a 64873172)
CODE_HEAD=64873172
REPOINT=589248ac
SIDES=(main=6bb9fd90 catalog-bounds=e705a5dc model-fuzzy=8d9ab0a0 compact-notice-late=96cc7849 at-height=af2b6137)
{
  echo "# Each merge's conflicted files, checked from both ends against its first parent (ours), second (theirs) and merge base."
  echo "## merge 1, 13e7c9a0 (CATALOG-BOUNDS): docs/parity.md's /model row"
  $P/rowcheck.py "$repo" ccced1f5 6bb9fd90 e705a5dc 13e7c9a0 docs/parity.md '| `/model` |'; echo "# exit $?"
  echo "## merge 2, 33e476fa (MODEL-FUZZY): docs/parity.md's /model row"
  $P/rowcheck.py "$repo" ccced1f5 13e7c9a0 8d9ab0a0 33e476fa docs/parity.md '| `/model` |'; echo "# exit $?"
  echo "## merge 3, 5a98001a (COMPACT-NOTICE-LATE): every file both sides changed"
  $P/filecheck.py "$repo" 58c92ba2 33e476fa 96cc7849 5a98001a src/tui/event_loop.rs src/tui/worker.rs src/tui/shell.rs docs/parity.md; echo "# exit $?"
  echo "## merge 4, 64873172 (AT-HEIGHT): every file both sides changed (the ledger is not ours + theirs: its nine blocks kept ours' numbers, numbers_only.py)"
  $P/filecheck.py "$repo" 712dd8d1 5a98001a af2b6137 64873172 src/tui/event_loop.rs src/tui/shell.rs docs/parity.md scripts/fxcap/parity-ledger.toml; echo "# exit $?"
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
$P/invariant.py "$repo" a832f4d7 "$head" | rd "$here/invariant.txt"
$P/shared.py "$repo" "$head" main=6bb9fd90 history-notice-stop=62d96c98:05a0a857 compact-notice-late=96cc7849:58c92ba2 \
  catalog-bounds=e705a5dc:ccced1f5 model-fuzzy=8d9ab0a0:ccced1f5 at-height=af2b6137:712dd8d1 | rd "$here/shared.txt"
$P/overlap.py "$repo" "${MERGES[@]}" | rd "$here/overlap.txt"
$P/selection.py "$repo" 6bb9fd90 "$head" | cmp -s - "$here/selection.txt" && echo "selection.txt: the same as selection.py writes at $head" || echo "selection.txt: DIFFERENT from what selection.py writes at $head"
