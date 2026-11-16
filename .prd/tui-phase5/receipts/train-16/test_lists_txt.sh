#!/usr/bin/env bash
# Train 16, round 2 (new; round 1 ran the same calls inline): test-lists.txt
# from test_lists.sh's output and lists -- its lines; per feature set the
# tests per binary, base -> head, and every test the head lists that the
# base does not; the doc test's note; Cargo.toml's two lines at <head>; and
# (round 2) what <head> lists against round 1's head's lists, kept in
# <round1-lists>. Through redacted.py.
# usage: test_lists_txt.sh <repo> <head> <lists-dir> <test_lists.sh output> <round1-lists> <scratch>
set -u
repo=$1 head=$2 L=$3 raw=$4 R1=$5 T=$6
here=$(cd "$(dirname "$0")" && pwd)
{ echo "# test_lists.sh <repo> 866747ff $head <scratch> (REUSE_BASE=1: round 1's base lists, made by this script at the same base; one hold of the suite lock): every test each test binary lists (cargo test --all-targets -- --list), at the base and the head, plain and with fault-injection"
  cat "$raw"
  for fs in plain fault; do
    echo "## $fs: the tests per binary, base -> head"
    python3 -I -B - "$L/list-base-$fs.txt" "$L/list-head-$fs.txt" <<'PY'
import sys, collections
a = collections.Counter(l.split()[0] for l in open(sys.argv[1]))
b = collections.Counter(l.split()[0] for l in open(sys.argv[2]))
for k in sorted(set(a) | set(b)):
    print(f"  {k}: {a[k]} -> {b[k]} ({b[k] - a[k]:+d})")
print(f"  all: {sum(a.values())} -> {sum(b.values())} ({sum(b.values()) - sum(a.values()):+d})")
PY
    echo "## $fs: the $(comm -13 "$L/list-base-$fs.txt" "$L/list-head-$fs.txt" | wc -l | tr -d ' ') the head lists and the base does not"
    comm -13 "$L/list-base-$fs.txt" "$L/list-head-$fs.txt" | sed 's/^/  + /'
  done
  echo "## the doc test: one at both (cargo test --doc -- --list), \`src/agent/types.rs - agent::types::TurnJournal (line 70)\`; TEST-ENV-ISOLATION's Cargo.toml sets \`doctest = false\` (and \`test = false\` on the binary), so the head's gate runs it no more: train 15's gate at 2c575f52 had its \`1 passed\` (../train-15/gate-counts.txt), this train's gate none (gate-counts.txt)"
  echo "## Cargo.toml at $head, the two lines"
  git -C "$repo" show "$head:Cargo.toml" | grep -n "^test = false\|^doctest = false"
  for fs in plain fault; do
    echo "## round 2, $fs: in $head's list and not round 1's head 2b78aab6's: $(comm -13 "$R1/list-head-$fs.txt" "$L/list-head-$fs.txt" | wc -l | tr -d ' '); the reverse: $(comm -23 "$R1/list-head-$fs.txt" "$L/list-head-$fs.txt" | wc -l | tr -d ' ')"
    comm -13 "$R1/list-head-$fs.txt" "$L/list-head-$fs.txt" | sed 's/^/  + /'
  done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/test-lists.txt" "$T=<scratch>" "$repo=<repo>"
