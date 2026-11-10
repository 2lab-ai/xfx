#!/usr/bin/env bash
# The join of main, redone in a clone by this folder's blocks.py (train 13's
# reproduce.sh, for one merge): the train's head merged with main's in diff3
# style, every conflicted file resolved by blocks.py, the index's tree
# compared with the merge commit's. Prints blocks.py's line for each block
# and "no difference" or the files that differ; exit 0 only on no difference.
# usage: reproduce.sh <repo> <scratch>
set -u -o pipefail
repo=$1 scratch=$2
here=$(cd "$(dirname "$0")" && pwd)
TRAIN=afd891af MAIN=d2b2ca75 MERGE=8adc57df
c="$scratch/reproduce"
rm -rf "$c" && git clone -q --no-local "$repo" "$c" && git -C "$c" checkout -q --detach "$TRAIN" || exit 2
git -C "$c" -c merge.conflictStyle=diff3 -c user.name=r -c user.email=r@localhost merge -q --no-ff --no-commit "$MAIN" > /dev/null 2>&1
files=$(git -C "$c" diff --name-only --diff-filter=U)
echo "conflicted: $(echo $files)"
for f in $files; do
  python3 -I -B "$here/blocks.py" "$c/$f" "$f" || exit 1
  git -C "$c" add "$f"
done
tree=$(git -C "$c" write-tree)
want=$(git -C "$repo" rev-parse "$MERGE^{tree}")
if [ "$tree" = "$want" ]; then
  echo "the redone merge's tree is $MERGE's: no difference"
  rc=0
else
  echo "DIFFERENT from $MERGE's tree:"; git -C "$c" diff --stat "$MERGE" | tail -5
  rc=1
fi
rm -rf "$c"
exit $rc
