#!/usr/bin/env bash
# Round 2's harness tests: test-capture.sh, then test-sweep.sh, at the
# checkout's head, inside one hold of the suite lock; each one's last lines.
# usage: suite-lock.sh r2-harness.sh <checkout>
set -u
cd "$1" || exit 2
echo "# head $(git rev-parse --short=8 HEAD); tree clean: $([ -z "$(git status --porcelain -- scripts src tests)" ] && echo yes || echo no) (scripts, src, tests)"
for t in test-capture test-sweep; do
  start=$SECONDS
  out=$(scripts/fxcap/$t.sh 2>&1); rc=$?
  echo "## $t: exit $rc, $((SECONDS - start)) s"
  printf '%s\n' "$out" | grep -E '^(Ran [0-9]+ tests|OK|FAILED|FAIL:|ERROR:)'
done
