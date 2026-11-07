#!/usr/bin/env bash
# SECRETS-ARGMAXc on Linux: the gate-env test at the head, the same test with
# the five gates as they were before their unset (43ae27de), and the sweep.
# Run as an unprivileged user on a disposable copy.
set -uo pipefail
copy=$1
cd "$copy" || exit 1
printf '== platform\nhead %s\n' "$(git rev-parse --short HEAD)"
uname -sm; bash --version | head -1; grep --version | head -1; ruby -v; printf 'uid %s\n' "$(id -u)"
printf '\n== scripts/test-gate-env.sh at the head\n'
s=$(date +%s); ./scripts/test-gate-env.sh </dev/null; printf -- '-> exit %d, %d s\n' "$?" $(($(date +%s) - s))
printf '\n== the same, with the five gates as at 43ae27de (no unset)\n'
for g in check-no-stubs check-preview-contract test-preview-contract check-tui-preflight-cost check-usage-clock; do
	git show "43ae27de:scripts/$g.sh" >"scripts/$g.sh"
done
s=$(date +%s); ./scripts/test-gate-env.sh </dev/null; printf -- '-> exit %d, %d s\n' "$?" $(($(date +%s) - s))
git checkout -q -- scripts
printf '\n== the copy afterwards\n'; if [ -z "$(git status --porcelain)" ]; then echo clean; else git status --porcelain; fi
bash /tools/sweep-env.sh "$copy"
