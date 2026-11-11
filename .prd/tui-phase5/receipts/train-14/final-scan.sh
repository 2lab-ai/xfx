#!/usr/bin/env bash
# Train 14: train 13's final-scan.sh, its units, folder and commits this
# train's: the brief's path check at the receipt's last commit --
#   1. each commit this train adds (first-parent, <base>..<head>: the four
#      merges, the join commit, the ledger, re-point and placeholder commits,
#      the receipt's), `git show --format= <c>`'s added lines -- a merge as
#      git show prints it, its combined diff -- every one 0; each merge also
#      read as everything it brings (`git diff <merge>^1 <merge>`), counted;
#      then the range, `git diff <base>..<head>`, 0; and shapes.py's three
#      shapes over the range's added lines, its run-name lines by file;
#   2. midbranch_scan.py: every commit of <base>..<head>, the units' own
#      included, and the range itself, each hit classified (placeholder,
#      SCRATCH, NAME; a NAME or a CREDENTIAL stops the train); first, each
#      unit tip's commits since its base;
#   3. every file of this directory, by the same pattern, and shapes.py's
#      three shapes over it;
#   4. each commit after <code-head>, and the files it touches outside this
#      directory (none: no code after the code head).
# The pattern is the brief's, each part spelt with a bracket class so this
# file does not match itself.
# usage: final-scan.sh <repo> <base> <head> [<code-head>]
set -u
repo=$1 base=$2 head=$3 code=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
echo "## 1. each commit this train adds, first-parent $base..$(git -C "$repo" rev-parse --short=8 "$head"): the brief's count; a merge also as everything it brings"
mine=0
for c in $(git -C "$repo" rev-list --first-parent --reverse "$base..$head"); do
  n=$(git -C "$repo" show --format= "$c" | grep -E '^[+]' | grep -cE "$P")
  extra=""
  if [ "$(git -C "$repo" rev-list --parents -n1 "$c" | wc -w)" -gt 2 ]; then
    extra="; as everything it brings (git diff ^1): $(git -C "$repo" diff "$c^1" "$c" | grep -E '^[+]' | grep -cE "$P")"
  fi
  echo "$(git -C "$repo" rev-parse --short=8 "$c") $n$extra  $(git -C "$repo" log -1 --format=%s "$c" | cut -c1-70)"
  mine=$((mine + n))
done
echo "# the brief's count, summed over this train's commits: $mine"
range=$(git -C "$repo" diff "$base" "$head" | grep -E '^[+]' | grep -cE "$P")
echo "# the brief's count over the range, git diff $base..$(git -C "$repo" rev-parse --short=8 "$head"): $range"
mine=$((mine + range))
d=$(mktemp)
git -C "$repo" diff "$base" "$head" > "$d"
echo "# shapes.py over the range's added lines (not gating; the brief's count is the first): $(grep -E '^[+]' "$d" | python3 -I -B "$here/shapes.py" lines)"
python3 -I -B - "$d" <<'PY'
import re, sys, collections
run = re.compile(r"fxs[w]\.[A-Za-z0-9_]{6}|fxca[p]\.(?!XXXXXX)[A-Za-z0-9]{6}")  # shapes.py's run-name
f, c = None, collections.Counter()
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    if line.startswith("+++ b/"):
        f = line[6:].rstrip("\n")
    elif line.startswith("+") and not line.startswith("+++") and run.search(line):
        c[f] += 1
print("#   the run-name lines by file: " + "; ".join(f"{k} {v}" for k, v in sorted(c.items())))
PY
rm -f "$d"
echo
echo "## 2. every commit of the range, and the range (midbranch_scan.py)"
echo "# commits per unit since its base: $(for pair in af2b6137..4130f4ac cb7c9c18..e7e1bcf6 3d3d56c8..3ea16930 714f6283..bde1ddfa; do printf '%s %s; ' "$pair" "$(git -C "$repo" rev-list --count "$pair")"; done)the range $base..$(git -C "$repo" rev-parse --short=8 "$head") $(git -C "$repo" rev-list --count "$base..$head")"
python3 -I -B "$here/midbranch_scan.py" "$repo" "$base" "$head"
rc=$?
echo "# midbranch_scan.py exit $rc"
echo
f=$(find "$here" -type f -print0 | xargs -0 grep -lE "$P" | wc -l | tr -d ' ')
echo "## 3. files of this directory holding the pattern: $f"
find "$here" -type f -print0 | xargs -0 python3 -I -B "$here/shapes.py" files | sed '1s/^/shapes.py over this directory: /'
sh=${PIPESTATUS[1]}
out=0
if [ -n "$code" ]; then
  echo
  echo "## 4. each commit after the code head $code, and the files it touches outside this directory"
  for c in $(git -C "$repo" rev-list --reverse "$code..$head"); do
    o=$(git -C "$repo" show --format= --name-only "$c" -- . ':!.prd/tui-phase5/receipts/train-14' | grep -c .)
    echo "$(git -C "$repo" rev-parse --short=8 "$c") outside: $o; inside: $(git -C "$repo" show --format= --name-only "$c" -- .prd/tui-phase5/receipts/train-14 | grep -c .) file(s)"
    out=$((out + o))
  done
  echo "# files outside this directory touched after the code head: $out"
fi
exit $(( mine + rc + f + sh + out > 0 ))
