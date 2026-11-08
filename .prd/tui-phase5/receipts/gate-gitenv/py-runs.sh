#!/usr/bin/env bash
# GATE-GITENV's sweep tests, RED and GREEN: test_sweep.py and test_ci_sweep.py
# (what scripts/fxcap/test-sweep.sh runs, one after the other) at 45d99f52
# (the base), 4ac0667e (the two new tests, the sweep unfixed) and 5d33c5df
# (fixed), on a disposable clone. Each module runs on its own, so the second
# runs when the first fails.
#
#   py-runs.sh <clone>
set -uo pipefail
cd "$1" || exit 1
for name in $(compgen -e); do
	case "$name" in
	GIT_*) unset "$name" ;;
	esac
done
printf '== platform\n'; uname -sm; python3 --version; git --version
log="${TMPDIR:-/tmp}/py-run.log"
for rev in 45d99f52 4ac0667e 5d33c5df; do
	git checkout -q "$rev" || exit 1
	for module in test_sweep test_ci_sweep; do
		printf '\n== scripts/fxcap/%s.py at %s\n' "$module" "$(git rev-parse --short HEAD)"
		start=$(date +%s); status=0
		python3 -I "scripts/fxcap/$module.py" >"$log" 2>&1 </dev/null || status=$?
		grep -E '^(FAIL|ERROR):|^[A-Za-z]+(Error|Exception):|^Ran |^OK|^FAILED' "$log" | sort | uniq -c | sed 's/^ *//' | cut -c 1-170
		printf -- '-> exit %d, %d s\n' "$status" $(($(date +%s) - start))
	done
done
git checkout -q -
printf '\n== the clone afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
