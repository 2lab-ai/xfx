#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP round 2: what 81248004 changes after the gate head
# 401b3fb2 (one line of ci.yml), and the steps that read it, on a clone at
# 81248004, with /bin/bash, in one hold of the suite lock.
#
#   suite-lock.sh head-delta-r2.sh <clone at 81248004>
set -u
cd "$1" || exit 2
echo "# head $(git rev-parse --short HEAD); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION')"
echo '$ git diff --stat 401b3fb2 HEAD'
git diff --stat 401b3fb2 HEAD
echo '$ git diff 401b3fb2 HEAD | grep -E "^[+-][^+-]"'
git diff 401b3fb2 HEAD | grep -E '^[+-][^+-]'
actionlint .github/workflows/ci.yml
echo "\$ actionlint .github/workflows/ci.yml: exit $?"
/bin/bash scripts/test-gate-env.sh </dev/null >"$TMPDIR/test-gate-env.log" 2>&1
echo "\$ scripts/test-gate-env.sh: exit $?, $(tail -n 1 "$TMPDIR/test-gate-env.log")"
grep -E '^test-gate-env: (ok )?(ci-bash32|ci-script-tests-per-os|workflow-steps)' "$TMPDIR/test-gate-env.log" | sed -E 's#/[^ ]*#<path>#g'
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
