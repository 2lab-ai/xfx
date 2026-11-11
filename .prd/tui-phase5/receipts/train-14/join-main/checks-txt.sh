#!/usr/bin/env bash
# Train 14's join of main (new): writes checks.txt through redacted.py --
# the files outside .prd that changed from the gate's code head b3dffdad to
# <head> (the join's harness and ledger, no src), then checks.sh's and
# checks_l.sh's own lines as they printed them, then checks_txt.py's reading
# of their step logs and sweeps.
# usage: checks-txt.sh <repo> <head> <checks.out> <checks-l.out> <out-dir>
set -u
repo=$1 head=$2 c1=$3 c2=$4 out=$5
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# The checks after the join, at $(git -C "$repo" rev-parse --short=8 "$head"): the code since the gate, then checks.sh's own lines (one hold of the suite lock), checks_l.sh's (another), and checks_txt.py's reading of their step logs and sweeps."
  echo "## git diff --stat b3dffdad $head, outside .prd"
  git -C "$repo" diff --stat=100 b3dffdad "$head" -- . ':!.prd'
  echo "## checks.sh"
  grep -v '^exit ' "$c1"
  echo "# checks.sh, under suite-lock.sh: $(grep '^exit ' "$c1")"
  echo "## checks_l.sh"
  grep -v '^exit ' "$c2"
  echo "# checks_l.sh, under suite-lock.sh: $(grep '^exit ' "$c2")"
  python3 -I -B "$here/checks_txt.py" "$out" wi24-1 wi24-2 wi24-3 wi49-1 wi49-2 wi49-3 rep24 rep49 rep24l
  echo "# checks_txt.py exit $?"
} | python3 -I -B "$here/../redacted.py" "$here/checks.txt" "$out=<out>" "$repo=<repo>"
