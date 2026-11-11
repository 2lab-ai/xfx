#!/usr/bin/env bash
# Train 14: train 13's pathcheck.sh (the pattern is train 14's brief's, the
# same eight terms as train 13's), with train 13's join-main exception for a
# merge: the brief's path check, run before every commit this train adds.
# For a merge in progress (MERGE_HEAD present) the staged tree is written as a
# probe commit with both parents (`git write-tree`, `git commit-tree`; on no
# branch), and the check reads `git show --format= <probe>` -- exactly what
# `git show` prints of the merge once committed. Otherwise it reads `git diff
# --cached`, the lines `git show --format=` prints of the commit. The pattern
# is the brief's, each part spelt with a bracket class so this file does not
# match itself; it gates on every line, with no exception.
# Then shapes.py's three shapes over the same lines, and over every file of
# this folder as it is on disk. During a merge, an added line that is also an
# added line of the unit's own (`git diff HEAD MERGE_HEAD`, the unit's text
# against the train) is counted apart and listed with its shape; the shapes'
# count that gates is over the rest (train 13's join of main did the same
# for main's lines). Any other commit: a staged line whose run-name or
# uid-tail match already stands in HEAD's text of its file is carried, not
# added, and listed apart (carried.py). Prints the counts and every hit;
# exit 0 only when all three gating counts are 0.
# usage: pathcheck.sh <worktree>
set -u -o pipefail
cd "$1" || exit 2
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
here=$(cd "$(dirname "$0")" && pwd)
if git rev-parse -q --verify MERGE_HEAD > /dev/null; then
  probe=$(git commit-tree "$(git write-tree)" -p HEAD -p MERGE_HEAD -m probe) || exit 2
  added() { git show --format= "$probe" | grep -E '^[+]'; }
  what="the merge's probe commit"
  units=$(mktemp) && git diff HEAD MERGE_HEAD | grep -E '^[+]' | sed 's/^+//' | sort -u > "$units"
  # a combined diff's added line starts with two columns; its text is after them
  text() { sed -E 's/^[+ -]{2}//'; }
  own() { added | text | sort -u | comm -23 - "$units"; }
  theirs() { added | text | sort -u | comm -12 - "$units"; }
else
  added() { git diff --cached | grep -E '^[+]'; }
  what="the staged lines"
  # a line rewritten around a run name or uid tail HEAD's file already
  # holds carries it, and is listed, not counted (carried.py)
  own() { python3 -I -B "$here/carried.py" . 2> /dev/null; }
  theirs() { :; }
  python3 -I -B "$here/carried.py" . 2>&1 > /dev/null | sed 's/^/  /'
fi
n=$(added | grep -cE "$P")
echo "path check over $what: $n"
added | grep -E "$P" | cut -c1-200 | sed 's/^/    hit: /'
if git rev-parse -q --verify MERGE_HEAD > /dev/null; then
  echo "shapes over its lines that are the unit's own added lines: $(theirs | python3 -I -B "$here/shapes.py" lines | head -1)"
  theirs | python3 -I -B -c 'import re, sys
shapes = [("run-name", re.compile(r"fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}")),
          ("uid-tail", re.compile(r"claud[e]-[0-9]+|(?<![^\s\x27\x22`(<>])-[0-9]+/"))]
for line in sys.stdin:
    for name, rx in shapes:
        m = rx.search(line)
        if m:
            print(f"    the unit\x27s ({name}): ...{line[max(0, m.start() - 60):m.end() + 20]}...")'
fi
echo "shapes over $what$(git rev-parse -q --verify MERGE_HEAD > /dev/null && echo ', the rest'): $(own | python3 -I -B "$here/shapes.py" lines | head -1)"
own | python3 -I -B "$here/shapes.py" lines > /dev/null
lines=${PIPESTATUS[1]}
rm -f "${units:-}"
# every file of this folder and its subfolders (join of main: ci-08066ac9/ is the first subfolder)
find "$here" -type f -print0 | xargs -0 python3 -I -B "$here/shapes.py" files | sed '1s/^/shapes over this folder: /'
folder=${PIPESTATUS[1]}
[ "$n" = 0 ] && [ "$lines" = 0 ] && [ "$folder" = 0 ]
