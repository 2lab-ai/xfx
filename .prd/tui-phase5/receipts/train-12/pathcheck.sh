#!/usr/bin/env bash
# Train 12: the brief's path check, run before every commit this train adds.
# For a merge in progress (MERGE_HEAD present) the staged tree is written as a
# probe commit with both parents (`git write-tree`, `git commit-tree`; on no
# branch), and the check reads `git show --format= <probe>` -- exactly what
# `git show` prints of the merge once committed. Otherwise it reads `git diff
# --cached`, the lines `git show --format=` prints of the commit. The pattern
# is the brief's, each part spelt with a bracket class so this file does not
# match itself. Prints the count, and every line the last term (`claude-<n>/`,
# which a model path also matches) holds; exit 0 only when the count is 0.
# Round 2 adds, through shapes.py, a run directory's name and the harness
# root's uid tail over the same lines, and all three shapes over every file
# of this folder as it is on disk; each must be 0 too.
# usage: pathcheck.sh <worktree>
set -u -o pipefail
cd "$1" || exit 2
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/'
if git rev-parse -q --verify MERGE_HEAD > /dev/null; then
  probe=$(git commit-tree "$(git write-tree)" -p HEAD -p MERGE_HEAD -m probe) || exit 2
  added() { git show --format= "$probe" | grep -E '^[+]'; }
  what="the merge's probe commit"
else
  added() { git diff --cached | grep -E '^[+]'; }
  what="the staged lines"
fi
n=$(added | grep -cE "$P")
echo "path check over $what: $n"
added | grep -E "$P" | cut -c1-200 | sed 's/^/    hit: /'
here=$(cd "$(dirname "$0")" && pwd)
echo "shapes over $what: $(added | python3 -I -B "$here/shapes.py" lines)"
added | python3 -I -B "$here/shapes.py" lines > /dev/null
lines=${PIPESTATUS[1]}
python3 -I -B "$here/shapes.py" files "$here"/* | sed '1s/^/shapes over this folder: /'
folder=${PIPESTATUS[0]}
[ "$n" = 0 ] && [ "$lines" = 0 ] && [ "$folder" = 0 ]
