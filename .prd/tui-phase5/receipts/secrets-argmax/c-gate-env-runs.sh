#!/usr/bin/env bash
# SECRETS-ARGMAXc's RED and GREEN for scripts/test-gate-env.sh: the test at
# 43ae27de (the test alone, before the five gates unset GREP_OPTIONS) and at
# 64f5f820 (after), each on a disposable clone checked out at that commit.
#
#   gate-env-runs.sh <clone>
set -uo pipefail
cd "$1" || exit 1
unset GREP_OPTIONS
printf '== platform\n'; uname -sm; bash --version | head -1; grep --version | head -1; ruby -v
for rev in 43ae27de 64f5f820; do
	git checkout -q "$rev" || exit 1
	printf '\n== scripts/test-gate-env.sh at %s\n' "$(git rev-parse --short HEAD)"
	s=$(date +%s); ./scripts/test-gate-env.sh </dev/null 2>&1; printf -- '-> exit %d, %d s\n' "$?" $(($(date +%s) - s))
done
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
