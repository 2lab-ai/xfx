#!/usr/bin/env bash
#
# The three ways the check at d2b2ca75 passes a planted path, forced on demand,
# against whichever check is named:
#
#   forced.sh <path/to/check-no-local-paths.sh>
#
# Each scenario is a throwaway repository under $TMPDIR holding the check and
# one plant; it prints the scenario, the check's exit status and the lines it
# named. The paths are assembled at run time (this file is tracked and
# scanned).
#
#   per-file-empty  a `git` on PATH that gives nothing for the per-file grep
#                   of the plant (`-h -n ... :(literal)plant.md`): the file
#                   the listing named has no lines, the CI failure's shape.
#   per-file-shapeless  the same grep gives a line with no shape in it.
#   late-nul        no shim: a text file whose first 8000 bytes hold no NUL,
#                   with a NUL before the path on the same line.
set -euo pipefail
check="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
work="$(mktemp -d "${TMPDIR:-/tmp}/forced.XXXXXX")"
trap 'rm -rf "$work"' EXIT
real_git="$(command -v git)"
s=/
plant="cd ${s}home${s}bob${s}x"
cat >"$work/git" <<'SH'
#!/bin/sh
case "$FORCED: $* " in
per-file-empty:*' -h -n '*':(literal)plant.md '*) exit 1 ;;
per-file-shapeless:*' -h -n '*':(literal)plant.md '*) printf '1:nothing here\n'; exit 0 ;;
esac
exec "$FORCED_GIT" "$@"
SH
chmod 700 "$work/git"
for scenario in per-file-empty per-file-shapeless late-nul; do
	t="$work/$scenario"
	mkdir -p "$t/scripts"
	cp "$check" "$t/scripts/check-no-local-paths.sh"
	chmod 755 "$t/scripts/check-no-local-paths.sh"
	if [ "$scenario" = late-nul ]; then
		{ printf '%8100s' '' | tr ' ' a; printf '\000 %s\n' "$plant"; } >"$t/plant.md"
	else
		printf '%s\n' "$plant" >"$t/plant.md"
	fi
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" init -q
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$t" add -A -f
	status=0
	(cd "$t" && env PATH="$work:$PATH" FORCED="$scenario" FORCED_GIT="$real_git" ./scripts/check-no-local-paths.sh) >"$work/out" 2>&1 </dev/null || status=$?
	printf '%s: exit %d; named: %s\n' "$scenario" "$status" "$(grep -E '^plant\.md:' "$work/out" | tr '\n' ' ')"
	grep -E '^check-no-local-paths: (ok|git gave)' "$work/out" | sed 's/^/    /' || true
done
