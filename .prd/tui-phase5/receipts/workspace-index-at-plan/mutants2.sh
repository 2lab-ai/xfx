#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN (rounds 1 and 2) on a throwaway clone at f718e3c1.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut2
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = f718e3c1 ] || { echo "clone not at f718e3c1"; exit 2; }
CT="scripts/fxcap/test_capture.py RunDirectory"
ST="scripts/fxcap/test_sweep.py Sweep.test_a_units_two_runs_share_one_run_directory_named_by_digits Sweep.test_a_units_second_run_finds_nothing_its_first_left"
mutant() { # id file sed-expr test...
  local id=$1 file=$2 expr=$3; shift 3
  git checkout -q -- scripts
  sed -i '' "$expr" "$file"
  if git diff --quiet -- scripts; then echo "$id: NOT APPLIED"; return; fi
  local diffline; diffline=$(git diff -U0 -- scripts | grep -E '^[-+][^-+]' | tr '\n' ' ' | cut -c1-220)
  local killed=0 t
  for t in "$@"; do
    if ! timeout 400 python3 -I $t > "$TMPDIR/$id.log" 2>&1; then killed=1; break; fi
  done
  if [ $killed = 1 ]; then
    echo "$id: KILLED by ${t%% *} ($(grep -m1 -E 'AssertionError|Error' "$TMPDIR/$id.log" | cut -c1-120))"
  else
    echo "$id: SURVIVED"
  fi
  echo "    diff: $diffline"
  echo "    failing: $(grep -E '^(FAIL|ERROR):' "$TMPDIR/$id.log" | sed -e 's/ (__main__[^)]*)//' -e 's/^[A-Z]*: //' | sort -u | tr '\n' ' ')"
}
C=scripts/fxcap/capture.sh P=scripts/fxcap/sweep.py
# Round 1's eight.
mutant M1-ignore-FXCAP_RUN $C '445s/if \[\[ -n ${FXCAP_RUN:-} \]\]; then/if false; then/' "$CT"
mutant M2-letters-again $C "454s/.*/    name=\$(basename \"\$(mktemp -u \"\$tmpdir\/fxcap.XXXXXX\")\")/" "$CT"
mutant M3-any-shape $C '446s/.*/    true/' "$CT"
mutant M4-reuse-named $C '447s/mkdir -m 700/mkdir -p -m 700/' "$CT"
mutant M5-random-mode $C '455s/mkdir -m 700/mkdir/' "$CT"
mutant M6-name-per-run $P '173s/env\["FXCAP_RUN"\] = run/env["FXCAP_RUN"] = "fxcap.%06d" % secrets.randbelow(10**6)/' "$ST"
mutant M7-no-env $P '173s/.*/        pass/' "$ST"
mutant M8-root-letters $P '152s/digit_dir(args.tmp, "fxsw.", 8)/__import__("tempfile").mkdtemp(prefix="fxsw.", dir=args.tmp)/' "$ST"
# Round 2's.
mutant M9-named-mode $C '447s/mkdir -m 700/mkdir/' "$CT"
mutant M10-random-reuse $C '455s/mkdir -m 700/mkdir -p -m 700/' "$CT"
mutant M11-no-reap $C '552s/.*/    true/' "$CT"
mutant M12-reap-no-kill $C '534s/.*/  true/' "$CT"
mutant M13-every-failure-retried $C '461s/.*/    true ||/' "$CT"
mutant M14-no-wipe-between-sides $P '194s/.*/            pass/' "$ST"
git checkout -q -- scripts
git status --short
