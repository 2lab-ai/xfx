#!/usr/bin/env bash
#
# The unit's gate, at the checked-out head, one step after another; each
# step's exit status and its last result lines. Run under the suite lock with
# a short TMPDIR:
#
#   TMPDIR=<short> suite-lock.sh gates.sh > gates.txt
#
# The full log of each cargo test goes to $TMPDIR/gates-<n>.log; this prints
# the lines the receipt rests on.
set -uo pipefail

step() {
  local n=$1
  shift
  local log="${TMPDIR%/}/gates-$n.log"
  "$@" > "$log" 2>&1
  local status=$?
  echo "== $n: $* -> exit $status"
  # Every line of the self-test, whose fixture lines are the receipt; the
  # last 60 of any other step.
  local keep=60
  [ "$n" = env-reads-self-test ] && keep=+1
  grep -E '^test result|^error|^warning: unused|FAILED|panicked|Running |check-|OK|ok$|refused|NOT REFUSED|^accepted|^resolved|NOT RESOLVED|^lexed|NOT LEXED|no case runs' "$log" \
    | grep -vE '^test .* \.\.\. ok$' | tail -n "$keep"
}

main() {
  echo "# gates at $(git rev-parse --short=8 HEAD)"
  step fmt cargo fmt --check
  step clippy cargo clippy --locked --all-targets -- -D warnings
  step clippy-fi cargo clippy --locked --all-targets --features fault-injection -- -D warnings
  step test cargo test --locked --no-fail-fast
  step test-fi cargo test --locked --features fault-injection --no-fail-fast
  step identity scripts/check-xfx-identity.sh
  step stubs scripts/check-no-stubs.sh
  step secrets scripts/check-no-secrets.sh
  step env-reads python3 -I scripts/check-env-reads.py
  step env-reads-self-test python3 -I scripts/check-env-reads.py --self-test
}

main "$@"
