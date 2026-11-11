#!/usr/bin/env bash
# Train 13, join check 2 (the decoder half): what the question surface reads
# for each of 443 spellings, at QUESTION-WORD-LEFT's head f686b736 and at the
# train head -- a cross-commit measurement, so a probe, never committed as a
# test. In a `git clone --no-local` of each, probe-test.rs.in is appended to
# src/tui/input.rs's test module (PROBE_FOLD the identity at f686b736, the
# shell's `unspelled` at the head, reached through a probe-only
# `probe_unspelled` appended to src/tui/shell.rs) and run alone; probe_table.py
# then lines the outputs up. No lock: one lib test each.
# usage: probe.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 dir=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$dir"
for side in qwl:f686b736 head:$head; do
  n=${side%%:*} c=${side#*:}
  rm -rf "$dir/probe-$n"
  git clone -q --no-local "$repo" "$dir/probe-$n" && git -C "$dir/probe-$n" checkout -q --detach "$c"
  python3 -I -B - "$dir/probe-$n" "$here/probe-test.rs.in" "$n" <<'PY'
import sys
clone, probe, n = sys.argv[1:4]
fold = "|e: Input| e" if n == "qwl" else "super::super::shell::probe_unspelled"
p = f"{clone}/src/tui/input.rs"
s = open(p).read()
assert s.endswith("    }\n}\n")
open(p, "w").write(s[:-2] + open(probe).read().replace("PROBE_FOLD", fold) + "}\n")
if n != "qwl":
    p = f"{clone}/src/tui/shell.rs"
    open(p, "a").write("\n/// Train 13's probe only (never committed).\npub(crate) fn probe_unspelled(event: Input) -> Input {\n    unspelled(event)\n}\n")
PY
  ( cd "$dir/probe-$n" && cargo test --locked --lib -- --nocapture train13_probe_what_the_question_reads ) > "$dir/probe-$n.out" 2>&1
  rc=$?
  echo "# $n at $(git -C "$dir/probe-$n" rev-parse --short=8 HEAD): exit $rc; $(grep '^test result' "$dir/probe-$n.out")"
done
python3 -I -B "$here/probe_table.py" "$dir/probe-qwl.out" "$dir/probe-head.out"
