#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: the script tests and the quick gates on the tree
# that merges main 753b0a72, on a throwaway clone, with /bin/bash, in one
# hold of the suite lock; and capture.sh as #94 left it, stopped after its
# trap (verdicts.sh).
#
#   suite-lock.sh merged-scripts.sh <clone> <log-dir>
set -u
clone=$1 logs=$2
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$logs"
cd "$clone" || exit 2
echo "# head $(git rev-parse --short HEAD) (parents $(git log -1 --format=%p HEAD 2>/dev/null)); merges 753b0a72: $(git merge-base --is-ancestor 753b0a72 HEAD && echo yes || echo no); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION'); tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
actionlint .github/workflows/ci.yml
echo "actionlint ci.yml: exit $?"
for s in check-no-secrets check-xfx-identity check-no-stubs check-no-local-paths check-preview-contract \
	test-check-no-local-paths test-check-no-secrets test-gate-env test-preview-contract test-early-stop; do
	start=$SECONDS
	/bin/bash "scripts/$s.sh" </dev/null >"$logs/$s.log" 2>&1
	status=$?
	printf '%-26s exit %d (%ds), %s\n' "$s" "$status" "$((SECONDS - start))" "$(tail -n 1 "$logs/$s.log" | sed -E 's#/[^ ]*#<path>#g' | cut -c 1-120)"
done
grep -E '^test-gate-env: (ok )?(ci-bash32|ci-script-tests-per-os|at-search)' "$logs/test-gate-env.log" | sed -E 's#/[^ ]*#<path>#g' | sed 's/^/    /'
printf '#!/bin/sh\nexit 0\n' >"$logs/fake-xfx"
chmod 755 "$logs/fake-xfx"
echo "# capture.sh at this tree (#94's), stopped right after its EXIT trap (verdicts.sh)"
TMPDIR="$logs" /bin/bash "$here/verdicts.sh" /bin/bash "$clone" "$logs/verdicts" fxcap/capture.sh
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
