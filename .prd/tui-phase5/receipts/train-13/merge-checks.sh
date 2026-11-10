#!/usr/bin/env bash
# Train 13's merge checks (train 12's merge-checks.sh, its merges, sides and
# checks this train's), written into this directory through redacted.py:
# each merge's conflicted and both-changed files checked from both ends
# (rowcheck.py, filecheck.py, its joined-region allowance with a negative
# control) and every resolution redone in a clone (reproduce.sh); the token
# check and its negative control (train 7's token_check.py, byte for byte)
# with token_control.py's plants on the rows both sides changed, and
# ledger_numbers.py on the entry the token check hands back; the
# remerge-diffs; the five reference scans at the merges' head before the
# re-point and at <head>; refs_content.py (the content-comparing check) and
# APPROVAL-VERTICAL-SPELLINGS' refs.py at the unit's head, the merges' head
# and <head>; the re-points against their code (repointed.py); the overlaps;
# and join check 3's functions (shared.py); round 2: blocks-m3.txt and an
# input.rs negative control. The brief's path check over every
# commit of the range is final-scan.sh's, at the receipt's last commit.
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
BASE=f53606d1
MERGES=(e776a6b0 9acb47a0 bbdde09c)
CODE_HEAD=bbdde09c
JOIN=ea35d548
REPOINT=e1e15efd
SIDES=(base=$BASE history-notices=714f6283 question-resume=d0e08d8c approval-vs=a37a969e)
# Round 2 (opus M1): merge 3's ledger blocks in both conflict styles and
# blocks.py's line for each diff3 block, kept (blocks-m3.txt); the header
# below reads its counts from that file.
bash "$here/blocks_m3.sh" "$repo" "$scratch" 2>&1 | rd "$here/blocks-m3.txt"
{
  echo "# Each merge's both-changed files, checked from both ends against its first parent (ours), second (theirs) and merge base 32afd307."
  for m in "${MERGES[@]}"; do
    b=$(git -C "$repo" merge-base "$m^1" "$m^2")
    echo "## $m: base $(git -C "$repo" rev-parse --short=8 "$b"); the files both sides changed:" \
      "$(comm -12 <(git -C "$repo" diff --name-only "$b" "$m^1" | sort) <(git -C "$repo" diff --name-only "$b" "$m^2" | sort) | tr '\n' ' ')"
  done
  echo "## merge 1, e776a6b0 (HISTORY-NOTICES-COPY-FX): docs/parity.md's three rows in its one block"
  for row in '| `/model` |' '| `/effort` |' '| `/provider` |'; do
    $P/rowcheck.py "$repo" 32afd307 e776a6b0^1 e776a6b0^2 e776a6b0 docs/parity.md "$row"; echo "# exit $?"
  done
  echo "## merge 1: the files both sides changed that merged clean"
  $P/filecheck.py "$repo" 32afd307 e776a6b0^1 e776a6b0^2 e776a6b0 src/interactive.rs src/provider/mod.rs src/tui/shell.rs src/tui/worker.rs tests/tui.rs scripts/fxcap/parity-ledger.toml; echo "# exit $?"
  echo "## merge 2, 9acb47a0 (QUESTION-RESUME): docs/parity.md's full-screen TUI row, its one block"
  $P/rowcheck.py "$repo" 32afd307 9acb47a0^1 9acb47a0^2 9acb47a0 docs/parity.md '| full-screen TUI |'; echo "# exit $?"
  echo "## merge 2: the files both sides changed that merged clean"
  $P/filecheck.py "$repo" 32afd307 9acb47a0^1 9acb47a0^2 9acb47a0 src/tui/bridge.rs src/tui/shell.rs src/tui/worker.rs scripts/fxcap/parity-ledger.toml; echo "# exit $?"
  echo "## merge 3, bbdde09c (APPROVAL-VERTICAL-SPELLINGS and the two under it): docs/parity.md's two rows, two blocks"
  for row in '| `ask_user_question` |' '| full-screen TUI |'; do
    $P/rowcheck.py "$repo" 32afd307 bbdde09c^1 bbdde09c^2 bbdde09c docs/parity.md "$row"; echo "# exit $?"
  done
  echo "## merge 3: the ledger with every reference number masked (blocks-m3.txt: $(grep -o 'diff3 style: [0-9]* blocks' "$here/blocks-m3.txt"), $(grep -c ': numbers -- ' "$here/blocks-m3.txt") numbers, ours'; $(grep -c ': ours -- ' "$here/blocks-m3.txt") ours, the new rule; $(grep -o 'merge style: [0-9]* blocks' "$here/blocks-m3.txt"))"
  $P/filecheck.py --numbers-masked "$repo" 32afd307 bbdde09c^1 bbdde09c^2 bbdde09c scripts/fxcap/parity-ledger.toml; echo "# exit $?"
  echo "## merge 3: the code files both sides changed (all merged clean)"
  $P/filecheck.py "$repo" 32afd307 bbdde09c^1 bbdde09c^2 bbdde09c src/tui/approval.rs src/tui/input.rs src/tui/question.rs src/tui/shell.rs; echo "# exit $?"
  echo "## the joined-region allowance's negative control: merge 3's shell.rs with one line of the unit's joined insertion changed (\"Apply once, y \" -> \"Apply twice, y \"), in a clone; False expected on that end"
  rm -rf "$scratch/neg" && git clone -q --no-local "$repo" "$scratch/neg" && git -C "$scratch/neg" checkout -q --detach bbdde09c
  n=$(grep -n 'band(&shell).contains("Apply once, y ")' "$scratch/neg/src/tui/shell.rs" | head -1 | cut -d: -f1)
  sed -i '' "${n}s/Apply once, y /Apply twice, y /" "$scratch/neg/src/tui/shell.rs"
  echo "changed line $n: $(git -C "$scratch/neg" diff --stat | tail -1)"
  $P/filecheck.py "$scratch/neg" 32afd307 bbdde09c^1 bbdde09c^2 - src/tui/shell.rs; echo "# exit $? (1 expected)"
  rm -rf "$scratch/neg"
  echo "## round 2 (opus N2): the same control on input.rs, its other joined match: one line of ours' insertion there changed (\"let dropped = behind.saturating_add(owed);\" -> \"... saturating_sub(owed);\"), in a clone; False expected"
  rm -rf "$scratch/neg" && git clone -q --no-local "$repo" "$scratch/neg" && git -C "$scratch/neg" checkout -q --detach bbdde09c
  n=$(grep -n 'let dropped = behind.saturating_add(owed);' "$scratch/neg/src/tui/input.rs" | head -1 | cut -d: -f1)
  sed -i '' "${n}s/saturating_add(owed)/saturating_sub(owed)/" "$scratch/neg/src/tui/input.rs"
  echo "changed line $n: $(git -C "$scratch/neg" diff --stat | tail -1)"
  $P/filecheck.py "$scratch/neg" 32afd307 bbdde09c^1 bbdde09c^2 - src/tui/input.rs; echo "# exit $? (1 expected)"
  rm -rf "$scratch/neg"
  echo "## every resolution redone by this folder's scripts in a clone (reproduce.sh)"
  bash "$here/reproduce.sh" "$repo" "$scratch"
} 2>&1 | rd "$here/resolutions.txt"
{
  echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  $P/token_check.py "$repo" "${MERGES[@]}"; echo "# exit $?"
  echo
  echo "# ledger_numbers.py: the entry token_check.py hands back (its bare :N after a path is outside its pattern), every :N masked"
  $P/ledger_numbers.py "$repo" bbdde09c "$REPOINT" cause:amendment-card-after-stop; echo "# exit $?"
} 2>&1 | rd "$here/token-check.txt"
{
  for m in "${MERGES[@]}"; do $P/token_check.py "$repo" --self-test "$m"; echo "# exit $?"; done
  echo
  echo "# token_control.py: the two token cases planted on every merged row both sides changed"
  $P/token_control.py "$repo" "${MERGES[@]}"; echo "# exit $?"
} 2>&1 | rd "$here/token-self-test.txt"
i=0
for m in "${MERGES[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
for s in refs refs_all refs_short refs_bare refs_prd; do
  $P/$s.py "$repo" "$CODE_HEAD" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-pre.txt"
  $P/$s.py "$repo" "$head" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
for c in a37a969e "$BASE" "$CODE_HEAD"; do
  { for f in scripts/fxcap/parity-ledger.toml docs/parity.md scripts/fxcap/capfile.py scripts/smoke-tui.sh src/tui/shell.rs; do
      $P/refs_content.py "$repo" "$c" "$f"; echo "# exit $?"
    done; } 2>&1 | rd "$here/content-$c.txt"
done
{ for f in scripts/fxcap/parity-ledger.toml docs/parity.md scripts/fxcap/capfile.py scripts/smoke-tui.sh src/tui/shell.rs; do
    echo "## $f, blame ignoring the re-point (each re-pointed ref read against the frame that wrote its numbers)"
    $P/refs_content.py "$repo" "$head" "$f" --ignore "$REPOINT"; echo "# exit $?"
    echo "## $f, blame as it is"
    $P/refs_content.py "$repo" "$head" "$f"; echo "# exit $?"
  done; } 2>&1 | rd "$here/content-head.txt"
{ for c in a37a969e "$BASE" "$CODE_HEAD" "$head"; do
    rm -rf "$scratch/avs" && git clone -q --no-local "$repo" "$scratch/avs" && git -C "$scratch/avs" checkout -q --detach "$c"
    cp "$repo/.prd/tui-phase5/receipts/approval-vertical-spellings/refs.py" "$scratch/avs-refs.py" 2>/dev/null || git -C "$repo" show a37a969e:.prd/tui-phase5/receipts/approval-vertical-spellings/refs.py > "$scratch/avs-refs.py"
    echo "## APPROVAL-VERTICAL-SPELLINGS' refs.py (a37a969e's, unchanged) at $(git -C "$scratch/avs" rev-parse --short=8 HEAD), its base argument 2593f49c"
    (cd "$scratch/avs" && python3 -I -B "$scratch/avs-refs.py" 2593f49c); echo "# exit $?"
  done; rm -rf "$scratch/avs" "$scratch/avs-refs.py"; } 2>&1 | rd "$here/avs-refs.txt"
$P/repoint.py "$repo" --derive 2>&1 | rd "$here/repoint-plan.txt"
$P/repointed.py "$repo" "$head" "$REPOINT" 2>&1 | rd "$here/repointed.txt"
$P/overlap.py "$repo" "${MERGES[@]}" 2>&1 | rd "$here/overlap.txt"
{ $P/shared.py "$repo" 32afd307 "$head" src/tui/shell.rs history-notices=714f6283 question-resume=d0e08d8c approval-stack=a37a969e; echo "# exit $?"
  $P/shared.py "$repo" 32afd307 "$head" src/tui/question.rs question-resume=d0e08d8c approval-stack=a37a969e; echo "# exit $?"
  $P/shared.py "$repo" 32afd307 "$head" src/tui/worker.rs history-notices=714f6283 question-resume=d0e08d8c; echo "# exit $?"; } 2>&1 | rd "$here/shared.txt"
echo "merge-checks: done"
