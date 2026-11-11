#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN (rounds 1 to 3) on a throwaway clone at edba4758.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut3
export TMPDIR=${MUT_TMP:?}
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = edba4758 ] || { echo "clone not at edba4758"; exit 2; }
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
ST="$ST Sweep.test_a_run_that_failed_is_a_failed_unit_and_its_path_is_not_reused"
# Round 1's.
mutant M1-ignore-FXCAP_RUN $C '447s/if \[\[ -n ${FXCAP_RUN:-} \]\]; then/if false; then/' "$CT"
mutant M2-letters-again $C "456s/.*/    name=\$(basename \"\$(mktemp -u \"\$tmpdir\/fxcap.XXXXXX\")\")/" "$CT"
mutant M3-any-shape $C '448s/.*/    true/' "$CT"
mutant M4-reuse-named $C '449s/mkdir -m 700/mkdir -p -m 700/' "$CT"
mutant M5-random-mode $C '457s/mkdir -m 700/mkdir/' "$CT"
mutant M6-name-per-run $P '173s/env\["FXCAP_RUN"\] = run/env["FXCAP_RUN"] = "fxcap.%06d" % secrets.randbelow(10**6)/' "$ST"
mutant M7-no-env $P '173s/.*/        pass/' "$ST"
mutant M8-root-letters $P '152s/digit_dir(args.tmp, "fxsw.", 8)/__import__("tempfile").mkdtemp(prefix="fxsw.", dir=args.tmp)/' "$ST"
# Round 2's that still apply.
mutant M9-named-mode $C '449s/mkdir -m 700/mkdir/' "$CT"
mutant M10-random-reuse $C '457s/mkdir -m 700/mkdir -p -m 700/' "$CT"
mutant M13-every-failure-retried $C '463s/.*/    true ||/' "$CT"
mutant M14-no-wipe-between-sides $P '204s/.*/            pass/' "$ST"
# Round 3's.
mutant R1-no-wait $C '572s/.*/    true/' "$CT"
mutant R2-no-fail $C '572s/.*/    await_pane || true/' "$CT"
mutant R3-identity-at-cleanup $C '571s/^/    own_pane "$sock"; /' "$CT"
mutant R4-no-record-at-relaunch $C '1276s/.*/        true/' "$CT"
mutant R5-pid-only $C '554s/.*/    if ! ps -p "$pid" > \/dev\/null 2>\&1; then/' "$CT"
mutant R6-wait-too-short $C '553s/i < 60/i < 2/' "$CT"
mutant R7-sweep-reuses-path $P '195s/.*/            if False:/' "$ST"
git checkout -q -- scripts
git status --short
