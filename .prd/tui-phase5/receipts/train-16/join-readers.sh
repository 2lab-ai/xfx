#!/usr/bin/env bash
# Train 16, round 3 (new): every reader of ci.yml and every script main
# changed, at the joined code head, from the gate's own step logs there (the
# gate ran each where ci.yml runs it, on the macos-15 row's terms: the pinned
# toolchain, /bin/bash first on PATH): each step's exit, its verdict lines,
# and for test-gate-env.sh every `ci-*` and `workflow-*` case; then which
# bash the scripts ran. Through redacted.py.
# usage: join-readers.sh <gate-dir> <head>
set -u
G=$1 head=$2
here=$(cd "$(dirname "$0")" && pwd)
L=$G/logs
log() { ls "$L"/[0-9][0-9]-"$1".log 2>/dev/null | head -1; }
{ echo "# the readers at $head, from the gate's logs (gates.txt names each step's exit)"
  for s in pinned-toolchain bash32 which-bash; do
    echo "## $s: $(grep -E "^[0-9]{2} $s " "$G/gates.out" | awk '{print "exit", $4}')"
    sed 's/^/  /' "$(log "$s")"
  done
  echo "## check-env-reads.py: $(grep -E '^[0-9]{2} check-env-reads ' "$G/gates.out" | awk '{print "exit", $4}')"
  tail -2 "$(log check-env-reads)" | sed 's/^/  /'
  echo "## check-env-reads.py --self-test: $(grep -E '^[0-9]{2} check-env-reads-self-test ' "$G/gates.out" | awk '{print "exit", $4}')"
  tail -1 "$(log check-env-reads-self-test)" | sed 's/^/  /'
  echo "## test-gate-env.sh: $(grep -E '^[0-9]{2} test-gate-env ' "$G/gates.out" | awk '{print "exit", $4}'); its ci.yml and workflow cases, and its last line"
  grep -E '^test-gate-env: (ok|FAIL|not ok)? *(ci-|workflow-)' "$(log test-gate-env)" | sed 's/^/  /'
  echo "  (round 4: ci-script-tests-per-os reads its own five -- test-gate-env, test-preview-contract,"
  echo "  test-check-no-secrets (reached through test-gate-env's nested run, no step of its own),"
  echo "  test-check-no-local-paths, test-early-stop -- not ci.yml's five scheduled script tests: it"
  echo "  does not read check-env-reads.py --self-test, whose rows ci.yml's \`if: matrix.script_tests\`"
  echo "  on its step sets: join-steps.txt, its last lines)"
  echo "  its other test-gate-env lines (a note or a problem; a problem ends the run non-zero):"
  grep -E -v '^test-gate-env: ok ' "$(log test-gate-env)" | grep -E '^test-gate-env: ' | sed 's/^/    /'
  tail -1 "$(log test-gate-env)" | sed 's/^/  /'
  for s in test-early-stop test-check-no-secrets test-preview-contract check-preview-contract test-check-no-local-paths check-no-secrets check-xfx-identity smoke; do
    f=$(log "$s")
    echo "## $s: $(grep -E "^[0-9]{2} $s " "$G/gates.out" | awk '{print "exit", $4}'); its last lines"
    tail -3 "$f" | sed 's/^/  /'
  done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/join-readers.txt" "$G/repo=<repo>" "$G=<gate>"
