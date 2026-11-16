#!/bin/bash
# Redoes the join in a throwaway clone and compares its tree with the merge
# commit's: ef595df2 merged with 8be4693b, the conflict resolved by
# resolve_parity.py, the index written as a tree.
#   remerge.sh <repo> <merge commit> <scratch dir>
# Prints no path.
set -u
repo=$1 merge=$2 scratch=$3
rm -rf "$scratch"
git clone -q --no-local --no-checkout "$repo" "$scratch" || exit 2
cd "$scratch" || exit 2
git -c advice.detachedHead=false checkout -q -b remerge ef595df2 || exit 2
git -c user.name=remerge -c user.email=remerge@invalid merge --no-ff --no-commit 8be4693b >merge.out 2>&1
echo "merge exit $? (1 = stopped on a conflict)"
grep '^CONFLICT' merge.out
python3 -I "$repo/.prd/tui-phase5/receipts/ci-pty-wait/join/resolve_parity.py" . || exit 3
git add docs/parity.md
echo "unmerged left: $(git diff --name-only --diff-filter=U | wc -l | tr -d ' ')"
redone=$(git write-tree)
committed=$(git -C "$repo" rev-parse "$merge^{tree}")
echo "redone tree    $redone"
echo "merge's tree   $committed"
[ "$redone" = "$committed" ] && echo "same tree" || { echo "DIFFERENT tree"; exit 1; }
