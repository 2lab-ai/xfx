#!/usr/bin/env bash
# Train 12's merge checks (train 11's merge-checks.sh, its merges, sides and
# checks this train's), written into this directory through redacted.py:
# each merge's conflicted and both-changed files checked from both ends
# (rowcheck.py, filecheck.py; merge 4's shell.rs with its semantic fix taken
# out, semantic.py --reverse, in a clone) and every resolution redone in a
# clone (reproduce.sh); the token check and its negative control (train 7's
# token_check.py, byte for byte) with token_control.py's plants on the rows
# both sides changed, and ledger_numbers.py on the entry the token check
# hands back; the remerge-diffs; the five reference scans at the merges'
# head before the re-point and at <head>; the re-points against their code
# (repointed.py); and the overlaps. The brief's path check over every commit
# of the range and the range itself is final-scan.sh's, at the receipt's last commit.
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MAIN=df46777e
MERGES=(d43c8c92 2c640bac 55d993a7 12b56ab8)
CODE_HEAD=12b56ab8
REPOINT=03fb5436
SIDES=(main=$MAIN gate-env-maintlock=a1cbdcaf sweep-wallclock=4bc9ab13 local-paths=5609108f input-row-limit=3d3d56c8)
{
  echo "# Each merge's both-changed files, checked from both ends against its first parent (ours), second (theirs) and merge base."
  echo "## merges 1 (d43c8c92) and 2 (2c640bac): the files both sides changed since their merge base"
  for m in d43c8c92 2c640bac; do
    b=$(git -C "$repo" merge-base "$m^1" "$m^2")
    echo "$m: $(comm -12 <(git -C "$repo" diff --name-only "$b" "$m^1" | sort) <(git -C "$repo" diff --name-only "$b" "$m^2" | sort) | wc -l | tr -d ' ') files"
  done
  echo "## merge 3, 55d993a7 (LOCAL-PATHS): the code and harness files both sides changed (all merged clean)"
  $P/filecheck.py "$repo" ccced1f5 2c640bac 5609108f 55d993a7 scripts/fxcap/capture.sh scripts/fxcap/test_capture.py scripts/test-gate-env.sh src/tui/approval.rs src/tui/shell.rs; echo "# exit $?"
  echo "## merge 3: the ledger with every reference number masked (its three blocks: two LOCAL-PATHS's, one ours' numbers)"
  $P/filecheck.py --numbers-masked "$repo" ccced1f5 2c640bac 5609108f 55d993a7 scripts/fxcap/parity-ledger.toml; echo "# exit $?"
  echo "## merge 4, 12b56ab8 (INPUT-ROW-LIMIT): docs/parity.md's three rows in its two blocks"
  for row in '| `/model` |' '| `/effort` |' '| full-screen TUI |'; do
    $P/rowcheck.py "$repo" 32afd307 55d993a7 3d3d56c8 12b56ab8 docs/parity.md "$row"; echo "# exit $?"
  done
  echo "## merge 4: the ledger with every reference number masked (six numbers-only blocks, ours')"
  $P/filecheck.py --numbers-masked "$repo" 32afd307 55d993a7 3d3d56c8 12b56ab8 scripts/fxcap/parity-ledger.toml; echo "# exit $?"
  echo "## merge 4: the code files both sides changed, at the merge commit (shell.rs holds the semantic fix: False expected)"
  $P/filecheck.py "$repo" 32afd307 55d993a7 3d3d56c8 12b56ab8 src/tui/layout.rs src/tui/shell.rs; echo "# exit $?"
  echo "## merge 4: shell.rs with the semantic fix taken out (semantic.py --reverse, in a clone)"
  rm -rf "$scratch/sem" && git clone -q --no-local "$repo" "$scratch/sem" && git -C "$scratch/sem" checkout -q --detach 12b56ab8
  $P/semantic.py --reverse "$scratch/sem"; echo "# exit $?"
  git -C "$scratch/sem" diff --stat | tail -1
  $P/filecheck.py "$scratch/sem" 32afd307 55d993a7 3d3d56c8 - src/tui/shell.rs; echo "# exit $?"
  rm -rf "$scratch/sem"
  echo "## every resolution redone by this folder's scripts in a clone (reproduce.sh)"
  bash "$here/reproduce.sh" "$repo" "$scratch"
} 2>&1 | rd "$here/resolutions.txt"
{
  echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  $P/token_check.py "$repo" "${MERGES[@]}"; echo "# exit $?"
  echo
  echo "# ledger_numbers.py: the entry token_check.py hands back (its .prd/ reference is outside its pattern), every :N masked"
  $P/ledger_numbers.py "$repo" 55d993a7 "$REPOINT" normalization:tmux-dead-pane; echo "# exit $?"
} | rd "$here/token-check.txt"
{
  for m in "${MERGES[@]}"; do $P/token_check.py "$repo" --self-test "$m"; echo "# exit $?"; done
  echo
  echo "# token_control.py: the two token cases planted on every merged row both sides changed"
  $P/token_control.py "$repo" "${MERGES[@]}"; echo "# exit $?"
} | rd "$here/token-self-test.txt"
i=0
for m in "${MERGES[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
for s in refs refs_all refs_short refs_bare refs_prd; do
  $P/$s.py "$repo" "$CODE_HEAD" "${SIDES[@]}" | rd "$here/$(echo "$s" | tr _ -)-pre.txt"
  $P/$s.py "$repo" "$head" "${SIDES[@]}" | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
$P/repoint.py "$repo" --derive | rd "$here/repoint-plan.txt"
$P/repointed.py "$repo" "$head" "$REPOINT" | rd "$here/repointed.txt"
$P/overlap.py "$repo" "${MERGES[@]}" | rd "$here/overlap.txt"
