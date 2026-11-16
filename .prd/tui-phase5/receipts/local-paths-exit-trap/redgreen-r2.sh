#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP round 2's RED for its two fixes, on a throwaway clone
# at the code head, with /bin/bash: each fixed file put back as it was
# before its fix (`git show <rev>:<file>`), the host that holds it run, the
# file put back. Run it on a clone only: a reverted file is the file without
# its guard.
#
#   redgreen-r2.sh <clone> <log-dir>
#
#   check-no-secrets.sh at 967581c9 (no listing or scan marker), held by
#     test-check-no-secrets.sh: `skipped-listing`, `skipped-scan`;
#   ci.yml at 967581c9 (no bash32), held by test-gate-env.sh: `ci-bash32`.
# One summary line per run, then the host's lines for those cases (paths
# written <path>).
set -u
clone=$1 logs=$2
mkdir -p "$logs"
cd "$clone" || exit 2
echo "# head $(git rev-parse --short HEAD); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION')"
for row in "scripts/check-no-secrets.sh|test-check-no-secrets.sh|skipped-" ".github/workflows/ci.yml|test-gate-env.sh|ci-bash32"; do
	IFS='|' read -r file host cases <<<"$row"
	git show "967581c9:$file" >"$file"
	label="red-$(basename "$file")"
	start=$SECONDS
	/bin/bash "scripts/$host" </dev/null >"$logs/$label.log" 2>&1
	status=$?
	git checkout -q -- "$file"
	printf '%s at 967581c9: %s exit %d (%ds), %s\n' "$file" "$host" "$status" "$((SECONDS - start))" \
		"$(tail -n 1 "$logs/$label.log" | sed -E 's#/[^ ]*#<path>#g')"
	grep -E "^test-[a-z-]+: (ok )?$cases" "$logs/$label.log" | sed -E 's#/[^ ]*#<path>#g; s/^/    /' | cut -c 1-200
	grep -E '^MISSING' "$logs/$label.log" | sed 's/^/      /'
	if [ "$host" = test-gate-env.sh ]; then
		sed -n '/^test-gate-env: ci-bash32/,/^test-gate-env: /p' "$logs/$label.log" | sed '1d;$d' | sed 's/^/      /'
	fi
done
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
