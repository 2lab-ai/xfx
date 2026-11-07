#!/usr/bin/env bash
#
# SECRETS-ARGMAXc's sweep: which gate scripts CI runs, what in them reads its
# options from the environment, and the probes behind each "not steerable".
#
#   sweep-env.sh <copy>
#
# Run on a DISPOSABLE copy (a `git clone --no-local`, or a container's copy):
# it builds scratch repositories beside it, and reads the copy's scripts.

set -uo pipefail

copy=$1
cd "$copy" || exit 1
unset GREP_OPTIONS
scratch="$(mktemp -d "${TMPDIR:-/tmp}/xfx-sweep.XXXXXX")"
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

section "the scripts ci.yml runs, and the test scripts"
{
	sed -n 's/^ *run: \.\/\(scripts\/[^ ]*\).*/\1/p' .github/workflows/ci.yml
	ls scripts/test-*.sh
} | sort -u | while read -r script; do
	greps=$(grep -c -E '(^|[^[:alnum:]_-])grep( |$)' "$script" || true)
	unset_line=$(grep -n -m1 '^unset GREP_OPTIONS' "$script" | cut -d: -f1)
	tools=$(grep -o -w -E 'git|sed|awk|sort|find|ruby|jq|cargo|python3|perl|actionlint|gh' "$script" | sort -u | tr '\n' ' ')
	printf '%-36s grep lines %-3s unset GREP_OPTIONS at :%-4s also: %s\n' "$script" "$greps" "${unset_line:--}" "$tools"
done

section "--color in any of them (GREP_COLOR/GREP_COLORS act only with it)"
grep -n -E -e '--colou?r' scripts/check-*.sh scripts/test-*.sh scripts/smoke.sh scripts/smoke-tui.sh || echo "none"

section "GREP_COLOR and GREP_COLORS without --color"
printf 'a key here\nnone\n' >"$scratch/colour.txt"
grep -n -e key "$scratch/colour.txt" >"$scratch/plain.out"
GREP_COLOR='01;31' GREP_COLORS='mt=01;31:sl=1' grep -n -e key "$scratch/colour.txt" >"$scratch/colour.out"
if cmp -s "$scratch/plain.out" "$scratch/colour.out"; then echo "identical output"; else echo "DIFFERENT output"; fi

section "check-xfx-identity.sh: a tracked path with the retired name, under each GREP_OPTIONS"
retired="f""x""r"
ident="$scratch/identity"
mkdir -p "$ident/scripts" "$ident/docs"
cp scripts/check-xfx-identity.sh "$ident/scripts/"
printf 'Clean text.\n' >"$ident/docs/${retired}rename-notes.md"
git -C "$ident" init -q
git -C "$ident" add -A
for value in '' -w -x -q -v -n -c -r -F -o '-e docs' '-m 1'; do
	status=0
	out="$(GREP_OPTIONS="$value" "$ident/scripts/check-xfx-identity.sh" 2>&1 </dev/null)" || status=$?
	first="$(printf '%s\n' "$out" | grep -m1 -v -E '^(docs/|check-xfx-identity: [0-9]+ problem)' || true)"
	printf 'GREP_OPTIONS=%-8s exit %d  %s\n' "'$value'" "$status" "$(printf '%s' "$first" | cut -c 1-120)"
done

section "check-xfx-identity.sh: a retired name in a Cargo.lock that git config marks -diff"
lock="$scratch/lock"
mkdir -p "$lock/scripts"
cp scripts/check-xfx-identity.sh "$lock/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$lock/Cargo.lock"
git -C "$lock" init -q
git -C "$lock" add -A
printf '*.lock -diff\n' >"$scratch/attributes"
status=0
"$lock/scripts/check-xfx-identity.sh" >"$scratch/lock-plain.out" 2>&1 </dev/null || status=$?
printf 'plain:                                   exit %d  %s\n' "$status" "$(grep -m1 'check-xfx-identity:' "$scratch/lock-plain.out" | cut -c 1-100)"
status=0
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.attributesFile GIT_CONFIG_VALUE_0="$scratch/attributes" \
	"$lock/scripts/check-xfx-identity.sh" >"$scratch/lock-attr.out" 2>&1 </dev/null || status=$?
printf 'GIT_CONFIG_* core.attributesFile=*.lock -diff: exit %d  %s\n' "$status" "$(grep -m1 'check-xfx-identity:' "$scratch/lock-attr.out" | cut -c 1-100)"

section "smoke.sh: expect_contains and expect_absent, run under each GREP_OPTIONS"
awk '/^(expect_contains|expect_absent)\(\) \{/,/^\}/' scripts/smoke.sh >"$scratch/expect.sh"
grep -c -E '^(expect_contains|expect_absent)\(\) \{' "$scratch/expect.sh" | sed 's/^/functions extracted: /'
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
printf 'event: {"tool":"edit_file"} Authorization: Bearer %s\nanother line\n' "$key" >"$scratch/evidence.txt"
for value in '' -v -x -w -q -c -n -o '-m 0'; do
	GREP_OPTIONS="$value" bash -c '
		. "$1"
		pass() { printf "pass"; }
		fail() { printf "FAIL"; }
		printf "absent(the key, which is there): "; expect_absent "$2" "$3" leak
		printf "   contains(\"tool\":\"edit_file\", which is there): "; expect_contains "$2" "\"tool\":\"edit_file\"" ran
		printf "\n"
	' _ "$scratch/expect.sh" "$scratch/evidence.txt" "$key" 2>/dev/null | sed "s/^/GREP_OPTIONS=$(printf '%-6s' "'$value'") /"
done
echo "smoke.sh:666 and :768 are the same \`grep -q(F) <needle> <file>\` shape, in the must-contain sense."

# --- git reads its environment too: leads, not fixed in this unit ------------

section "check-no-secrets.sh: GIT_INDEX_FILE naming an index without the file that holds a key"
keyed="$scratch/keyed"
mkdir -p "$keyed/scripts"
cp scripts/check-no-secrets.sh "$keyed/scripts/"
printf 'clean\n' >"$keyed/a.txt"
printf '%s\n' "$key" >"$keyed/b.txt"
git -C "$keyed" init -q
git -C "$keyed" add -A
GIT_INDEX_FILE="$scratch/partial-index" git -C "$keyed" add a.txt scripts/check-no-secrets.sh
status=0
"$keyed/scripts/check-no-secrets.sh" >"$scratch/keyed-plain.out" 2>&1 </dev/null || status=$?
printf 'plain:                         exit %d  %s\n' "$status" "$(grep -m1 -E 'possible|ok \(' "$scratch/keyed-plain.out" | cut -c 1-90)"
status=0
GIT_INDEX_FILE="$scratch/partial-index" "$keyed/scripts/check-no-secrets.sh" >"$scratch/keyed-index.out" 2>&1 </dev/null || status=$?
printf 'GIT_INDEX_FILE=<a.txt only>:   exit %d  %s\n' "$status" "$(grep -m1 -E 'possible|ok \(' "$scratch/keyed-index.out" | cut -c 1-90)"

section "check-xfx-identity.sh and check-preview-contract.sh: GIT_DIR exported, pointing at a decoy repository"
decoy="$scratch/decoy"
mkdir -p "$decoy"
git -C "$decoy" init -q
printf 'decoy\n' >"$decoy/d.txt"
git -C "$decoy" add d.txt
before="$(git --git-dir="$decoy/.git" ls-files | wc -l | tr -d ' ')"
GIT_DIR="$decoy/.git" "$ident/scripts/check-xfx-identity.sh" >/dev/null 2>&1 </dev/null || true
after="$(git --git-dir="$decoy/.git" ls-files | wc -l | tr -d ' ')"
printf 'check-xfx-identity.sh:   decoy index entries %s -> %s (%s)\n' "$before" "$after" \
	"$(git --git-dir="$decoy/.git" ls-files | grep -c -E '^(prose|env|README)\.md$' | sed 's/$/ of its control files among them/')"
before="$(git --git-dir="$decoy/.git" rev-list --all 2>/dev/null | wc -l | tr -d ' ')"
GIT_DIR="$decoy/.git" scripts/check-preview-contract.sh >/dev/null 2>&1 </dev/null || true
after="$(git --git-dir="$decoy/.git" rev-list --all 2>/dev/null | wc -l | tr -d ' ')"
printf 'check-preview-contract.sh: decoy commits %s -> %s\n' "$before" "$after"
