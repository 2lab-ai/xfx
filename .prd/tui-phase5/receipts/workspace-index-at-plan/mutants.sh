#!/bin/bash
# Mutants of WORKSPACE-INDEX-AT-PLAN's harness change, on a throwaway clone at the fix commit.
# Each: one edit, the targeted tests, the clone restored. KILLED = a test fails.
set -u
M=${SCRATCH:?}/mut
export TMPDIR=${GATE_TMP:?}/mut
mkdir -p "$TMPDIR"
cd "$M" || exit 2
[ "$(git rev-parse --short HEAD)" = 2a721846 ] || { echo "clone not at 2a721846"; exit 2; }
CT="scripts/fxcap/test_capture.py RunDirectory"
ST="scripts/fxcap/test_sweep.py Sweep.test_a_units_two_runs_share_one_run_directory_named_by_digits"
mutant() { # id file line-number sed-expr test...
  local id=$1 file=$2 expr=$3; shift 3
  git checkout -q -- scripts
  sed -i '' "$expr" "$file"
  if git diff --quiet -- scripts; then echo "$id: NOT APPLIED"; return; fi
  local diffline; diffline=$(git diff -U0 -- scripts | grep -E '^[-+][^-+]' | tr '\n' ' ' | cut -c1-200)
  local killed=0 t
  for t in "$@"; do
    if ! timeout 300 python3 -I $t > "$TMPDIR/$id.log" 2>&1; then killed=1; break; fi
  done
  if [ $killed = 1 ]; then
    echo "$id: KILLED by ${t##* } ($(grep -m1 -E 'AssertionError|Error' "$TMPDIR/$id.log" | cut -c1-120))"
  else
    echo "$id: SURVIVED"
  fi
  echo "    diff: $diffline"
}
mutant M1-ignore-FXCAP_RUN scripts/fxcap/capture.sh '442s/if \[\[ -n ${FXCAP_RUN:-} \]\]; then/if false; then/' "$CT"
mutant M2-letters-again scripts/fxcap/capture.sh "451s/.*/    name=\$(basename \"\$(mktemp -u \"\$tmpdir\/fxcap.XXXXXX\")\")/" "$CT"
mutant M3-any-shape scripts/fxcap/capture.sh '443s/.*/    true/' "$CT"
mutant M4-reuse-existing scripts/fxcap/capture.sh '444s/mkdir -m 700/mkdir -p -m 700/' "$CT"
mutant M5-default-mode scripts/fxcap/capture.sh '452s/mkdir -m 700/mkdir/' "$CT"
mutant M6-name-per-run scripts/fxcap/sweep.py '173s/env\["FXCAP_RUN"\] = run/env["FXCAP_RUN"] = "fxcap.%06d" % secrets.randbelow(10**6)/' "$ST"
mutant M7-no-env scripts/fxcap/sweep.py '173s/.*/        pass/' "$ST"
mutant M8-root-letters scripts/fxcap/sweep.py '152s/digit_dir(args.tmp, "fxsw.", 8)/__import__("tempfile").mkdtemp(prefix="fxsw.", dir=args.tmp)/' "$ST"
git checkout -q -- scripts
git status --short
