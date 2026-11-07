#!/usr/bin/env bash
#
# SECRETS-ARGMATd's probes for the receipt's remaining sentences: the script
# ci.yml runs from a block `run:`, and the two reasons the receipt gives for
# check-xfx-identity.sh and smoke.sh.
#
#   d-probes.sh <copy>      (a disposable `git clone --no-local`)

set -uo pipefail
cd "$1" || exit 1
unset GREP_OPTIONS
scratch="$(mktemp -d "${TMPDIR:-/tmp}/xfx-probes.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

section() {
	printf '\n== %s\n' "$1"
}

section "platform"
printf 'head %s\n' "$(git rev-parse --short HEAD)"
uname -sm
bash --version | head -1
grep --version 2>&1 | head -1
git --version

section "ci.yml's block run: steps that name a script (the sweep read only one-line run: steps)"
grep -n -E '^ +\./scripts/' .github/workflows/ci.yml

section "scripts/smoke-tui.sh: every line with a grep command word"
grep -n -E '(^|[^[:alnum:]_-])grep( |$)' scripts/smoke-tui.sh | cut -c 1-140

section "BSD grep's --exclude: a file operand, standard input, a here-string"
printf 'needle\n' >"$scratch/a.md"
printf 'file operand:  '; GREP_OPTIONS='--exclude=*.md' grep -c needle "$scratch/a.md"; printf -- '-> exit %d\n' "$?"
printf 'stdin:         '; printf 'needle\n' | GREP_OPTIONS='--exclude=*' grep -c needle; printf -- '-> exit %d\n' "$?"
printf 'here-string:   '; GREP_OPTIONS='--exclude=*' grep -c needle <<<"needle"; printf -- '-> exit %d\n' "$?"

section "git grep and GREP_OPTIONS"
repo="$scratch/repo"
mkdir -p "$repo"
git -C "$repo" init -q
printf 'needle here\nnothing\n' >"$repo/notes.md"
git -C "$repo" add notes.md
plain="$(git -C "$repo" grep -n -i -e needle -- || true)"
steered="$(GREP_OPTIONS='-v --exclude=*.md' git -C "$repo" grep -n -i -e needle -- || true)"
printf 'plain:                            %s\n' "$plain"
printf "GREP_OPTIONS='-v --exclude=*.md': %s\n" "$steered"

section "smoke.sh's expect_absent at :617, under GREP_OPTIONS=--exclude=*content.err"
sed -n 617p scripts/smoke.sh
awk '/^expect_absent\(\) \{/,/^\}/' scripts/smoke.sh >"$scratch/expect.sh"
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
printf 'debug: %s\n' "$key" >"$scratch/content.err"
for value in '' '--exclude=*content.err'; do
	GREP_OPTIONS="$value" bash -c '
		. "$1"
		pass() { printf "pass"; }
		fail() { printf "FAIL"; }
		expect_absent "$2" "$3" leak
	' _ "$scratch/expect.sh" "$scratch/content.err" "$key"
	printf "   (GREP_OPTIONS='%s'; content.err holds the key)\n" "$value"
done
printf 'expect_contains reading content.err: %s\n' "$(grep -c -E 'expect_contains "\$evidence/content\.err"' scripts/smoke.sh)"

section "smoke.sh's credential walk, the verdict that does not use grep"
grep -n -E '^python3 - "\$evidence" "\$FAKE_KEY"|^for directory, subdirectories, files in os\.walk\(root\):|^PYTHON$|^leak_status=' scripts/smoke.sh
