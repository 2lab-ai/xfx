#!/usr/bin/env bash
# The join of main (train 13's pathcheck.sh, read over `git diff --cached`
# in every state, a merge's included): the brief's path check, run before
# every commit the join adds. During the merge, `git diff --cached` is the
# index against the train's head -- everything main brings and every
# resolution -- which is the brief's command as written. Then train 13's
# shapes.py over the same lines, and over every file of this folder as it is
# on disk. During the merge, a staged line that is also an added line of
# `git diff HEAD MERGE_HEAD` (main's own text, on main already) is counted
# apart and listed; the shapes' count that gates is over the rest. The pattern is the brief's, each part spelt with a bracket class
# so this file does not match itself. Prints the counts and every hit; exit 0
# only when all three are 0.
# usage: pathcheck.sh <worktree>
set -u -o pipefail
cd "$1" || exit 2
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
here=$(cd "$(dirname "$0")" && pwd)
added() { git diff --cached | grep -E '^[+]'; }
echo "staged lines: $(added | wc -l | tr -d ' ')$(git rev-parse -q --verify MERGE_HEAD > /dev/null && echo ' (a merge in progress: the index against HEAD)')"
n=$(added | grep -cE "$P")
echo "path check over the staged lines: $n"
added | grep -E "$P" | cut -c1-200 | sed 's/^/    hit: /'
if git rev-parse -q --verify MERGE_HEAD > /dev/null; then
  mains=$(mktemp) && git diff HEAD MERGE_HEAD | grep -E '^[+]' | sort -u > "$mains"
  own() { added | sort -u | comm -23 - "$mains"; }
  echo "shapes over the staged lines that are main's own added lines: $(added | sort -u | comm -12 - "$mains" | python3 -I -B "$here/../shapes.py" lines | head -1)"
  added | sort -u | comm -12 - "$mains" | python3 -I -B -c 'import re, sys
rx = re.compile(r"fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}|claud[e]-[0-9]+|(?<![^\s\x27\x22`(<>])-[0-9]+/")
for line in sys.stdin:
    if rx.search(line):
        print("    main\x27s: " + line.rstrip("\n")[:160])'
else
  own() { added; }
fi
echo "shapes over the staged lines$(git rev-parse -q --verify MERGE_HEAD > /dev/null && echo ' that are not main'"'"'s'): $(own | python3 -I -B "$here/../shapes.py" lines | head -1)"
own | python3 -I -B "$here/../shapes.py" lines > /dev/null
lines=${PIPESTATUS[1]}
rm -f "${mains:-}"
python3 -I -B "$here/../shapes.py" files "$here"/* | sed '1s/^/shapes over this folder: /'
folder=${PIPESTATUS[0]}
[ "$n" = 0 ] && [ "$lines" = 0 ] && [ "$folder" = 0 ]
