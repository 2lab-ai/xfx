#!/bin/bash
# usage: loop.sh <tree> <label> <iterations> <outdir> [tests...]
# Runs the affected harness tests (or the tests named) <iterations> times; one
# log per iteration and a per-test pass count in <outdir>/<label>.counts.
set -u
wt=$1 label=$2 n=$3 out=$4
shift 4
mkdir -p "$out/$label"
tests=(
  McpHttp FakeGateway
  Verbs.test_clock Verbs.test_usage_is_in_place_at_launch_and_swapped_on_cue
  Verbs.test_models2_and_modelsdelay Verbs.test_mcphttp_on_cue
  Verbs.test_held_waits_for_the_hold_and_release_lets_it_go
  Verbs.test_a_file_a_verb_changes_mid_run_is_replaced_whole
)
(( $# == 0 )) || tests=("$@")
export TMPDIR=/private/tmp/claude-501/flake-mcpmode/t
mkdir -p "$TMPDIR"
for i in $(seq 1 "$n"); do
  log=$out/$label/run-$i.log
  ( cd "$wt" && timeout 600 python3 -I scripts/fxcap/test_capture.py -v "${tests[@]}" ) > "$log" 2>&1
  echo "run $i rc=$? $(grep -E '^(Ran|OK|FAILED)' "$log" | tr '\n' ' ')"
done
# Per test: the runs in which it passed (ran, and no FAIL/ERROR line names it).
python3 - "$out/$label" "$n" > "$out/$label.counts" <<'PY'
import glob, re, sys, collections
d, n = sys.argv[1], int(sys.argv[2])
ran, bad = collections.Counter(), collections.Counter()
for path in sorted(glob.glob(d + "/run-*.log")):
    text = open(path).read()
    names = set(re.findall(r"^(test_\w+) \(__main__\.\w+\.test_\w+\)", text, re.M))
    failed = set(re.findall(r"^(?:FAIL|ERROR): (test_\w+) ", text, re.M))
    for t in names:
        ran[t] += 1
        bad[t] += t in failed
for t in sorted(ran):
    print("%-75s passed %d/%d" % (t, ran[t] - bad[t], ran[t]))
PY
cat "$out/$label.counts"
