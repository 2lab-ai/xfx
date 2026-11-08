#!/usr/bin/env bash
#
# GATE-GITENV round 2's probes: the entry points outside the three gates, the
# identity scan's failure handling, and the config that survives the GIT_*
# drop. Each row: exit status, what the gate said, and what it left behind.
#
#   r2-probes.sh <copy> [quick]      (a disposable clone; "quick" skips the two
#                                     slow rows that run whole test scripts)

set -uo pipefail
copy=$1
quick=${2:-}
cd "$copy" || exit 1
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-r2.XXXXXX")"
trap 'chmod -R u+rw "$s" 2>/dev/null; rm -rf "$s"' EXIT
sg() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c user.name=p -c user.email=p@example.invalid -c commit.gpgsign=false "$@"; }

printf '== platform\nhead %s\n' "$(git rev-parse --short HEAD)"
uname -sm; bash --version | head -1; git --version; rustc --version 2>/dev/null || echo "rustc: none"

retired="f""x""r"
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

row() {
	printf '%-58s exit %-3s %s\n' "$1" "$2" "$3"
}
said() {
	grep -m1 -E 'still in tracked|possible OpenAI|could not|cannot|failed|this scan is not working|: ok|checked out|gate is running|fatal|error' "$1" | cut -c 1-90
}

new_decoy() {
	rm -rf "$s/decoy"; mkdir -p "$s/decoy"
	sg -C "$s/decoy" init -q
	printf 'decoy\n' >"$s/decoy/d.txt"
	sg -C "$s/decoy" add d.txt
	sg -C "$s/decoy" commit -q -m decoy
}

printf '\n== ci.yml: the checkout assertion step, run as extracted, in a scratch checkout\n'
ruby -ryaml -e '
	doc = YAML.safe_load(File.read(".github/workflows/ci.yml"), aliases: true)
	step = doc["jobs"].values.flat_map { |j| j["steps"] || [] }.find { |st| st["name"] == "Confirm the checkout is the commit under test" }
	abort "no such step" unless step
	print step["run"]
' >"$s/assertion.sh" && echo "extracted $(wc -l <"$s/assertion.sh" | tr -d ' ') lines"
mkdir -p "$s/checkout"; sg -C "$s/checkout" init -q; printf 'x\n' >"$s/checkout/x"; sg -C "$s/checkout" add x; sg -C "$s/checkout" commit -q -m checkout
new_decoy
decoy_head="$(sg -C "$s/decoy" rev-parse HEAD)"
st=0; (cd "$s/checkout" && env COMMIT_UNDER_TEST="$decoy_head" bash -e "$s/assertion.sh") >"$s/out" 2>&1 || st=$?
row "plain, COMMIT_UNDER_TEST=<the decoy's HEAD>" "$st" "$(said "$s/out")"
st=0; (cd "$s/checkout" && env COMMIT_UNDER_TEST="$decoy_head" GIT_DIR="$s/decoy/.git" GIT_TRACE="$s/trace-ci" bash -e "$s/assertion.sh") >"$s/out" 2>&1 || st=$?
row "GIT_DIR=decoy GIT_TRACE=<file>, COMMIT_UNDER_TEST=<decoy HEAD>" "$st" "$(said "$s/out"); trace file: $([ -e "$s/trace-ci" ] && echo WRITTEN || echo absent)"

printf '\n== build.rs: compiled alone, run in a scratch checkout\n'
edition="$(sed -n 's/^edition *= *"\([0-9]*\)".*/\1/p' Cargo.toml | head -1)"
if rustc --edition "$edition" -O build.rs -o "$s/build-script" >"$s/rustc.out" 2>&1; then
	checkout_head="$(sg -C "$s/checkout" rev-parse HEAD)"
	for env_line in "" "GIT_DIR=$s/decoy/.git" "GIT_TRACE=$s/trace-build"; do
		st=0
		(cd "$s/checkout" && env -u XFX_BUILD_REVISION -u XFX_BUILD_CHANNEL PROFILE=debug OUT_DIR="$s" ${env_line:+"$env_line"} "$s/build-script") >"$s/out" 2>&1 || st=$?
		rev="$(sed -n 's/^cargo:rustc-env=XFX_BUILD_REVISION=//p' "$s/out")"
		if [ "$rev" = "$checkout_head" ]; then which="the checkout's HEAD"; elif [ "$rev" = "$decoy_head" ]; then which="THE DECOY'S HEAD"; else which="'$rev'"; fi
		label="${env_line%%=*}"
		row "${label:-plain}" "$st" "revision = $which; trace file: $([ -e "$s/trace-build" ] && echo WRITTEN || echo absent)"
	done
else
	row "rustc build.rs" "-" "$(head -3 "$s/rustc.out")"
fi

printf '\n== check-xfx-identity.sh: the checkout carries the failure, its controls do not\n'
ilock() {
	rm -rf "$s/ilock"; mkdir -p "$s/ilock/scripts"
	cp scripts/check-xfx-identity.sh "$s/ilock/scripts/"
	printf 'name = "%s-core"\n' "$retired" >"$s/ilock/Cargo.lock"
	sg -C "$s/ilock" init -q; sg -C "$s/ilock" add -A
}
for kv in grep.patternType=bogus grep.extendedRegexp=notabool grep.threads=x core.quotePath=notabool; do
	ilock; sg -C "$s/ilock" config "${kv%%=*}" "${kv#*=}"
	st=0; "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
	row ".git/config $kv" "$st" "$(said "$s/out")"
done
ilock; chmod 000 "$s/ilock/Cargo.lock"
st=0; "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
chmod 644 "$s/ilock/Cargo.lock"
row "Cargo.lock (the residue) unreadable, mode 000" "$st" "$(said "$s/out")"
ilock
mkdir -p "$s/elsewhere/scripts"; printf 'name = "clean-core"\n' >"$s/elsewhere/Cargo.lock"; cp scripts/check-xfx-identity.sh "$s/elsewhere/scripts/"
sg -C "$s/ilock" config core.worktree "$s/elsewhere"
st=0; "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row ".git/config core.worktree=<a clean tree, same names>" "$st" "$(said "$s/out")"
for bit in skip-worktree assume-unchanged; do
	ilock; sg -C "$s/ilock" update-index "--$bit" Cargo.lock; printf 'bin\000ary\n' >"$s/ilock/Cargo.lock"
	st=0; "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
	row "Cargo.lock marked $bit, a working copy with a NUL" "$st" "$(said "$s/out")"
done
ilock; printf 'DIRC' >"$s/ilock/.git/index"
st=0; "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row ".git/index cut to 4 bytes" "$st" "$(said "$s/out")"

printf '\n== config from HOME that runs a program: core.fsmonitor and core.hooksPath\n'
home="$s/home"; mkdir -p "$home/hooks"
for hook in pre-commit commit-msg post-commit post-checkout; do
	printf '#!/bin/sh\n: >>"%s/hook-ran-%s"\n' "$s" "$hook" >"$home/hooks/$hook"; chmod 755 "$home/hooks/$hook"
done
printf '#!/bin/sh\n: >>"%s/fsmonitor-ran"\nexit 1\n' "$s" >"$home/fsmonitor"; chmod 755 "$home/fsmonitor"
printf '[core]\n\tfsmonitor = %s\n\thooksPath = %s\n[user]\n\tname = h\n\temail = h@example.invalid\n' "$home/fsmonitor" "$home/hooks" >"$home/.gitconfig"
ran() {
	local list
	list="$(cd "$s" && ls hook-ran-* fsmonitor-ran 2>/dev/null | tr '\n' ' ')"
	rm -f "$s"/hook-ran-* "$s/fsmonitor-ran"
	printf '%s' "${list:-nothing ran}"
}
mkdir -p "$s/keyed/scripts"; cp scripts/check-no-secrets.sh "$s/keyed/scripts/"
printf 'clean\n' >"$s/keyed/a.txt"; printf '%s\n' "$key" >"$s/keyed/b.txt"; sg -C "$s/keyed" init -q; sg -C "$s/keyed" add -A
mkdir -p "$s/readme/.github/workflows"; cp .github/workflows/*.yml "$s/readme/.github/workflows/"
cp .github/preview-epoch "$s/readme/.github/"; grep -v -F -e 'brew install' README.md >"$s/readme/README.md"
ilock
st=0; HOME="$home" "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "check-xfx-identity.sh, HOME=<that config>" "$st" "$(said "$s/out"); ran: $(ran)"
st=0; HOME="$home" "$s/keyed/scripts/check-no-secrets.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "check-no-secrets.sh, HOME=<that config>" "$st" "$(said "$s/out"); ran: $(ran)"
st=0; HOME="$home" scripts/check-preview-contract.sh "$s/readme/.github/workflows/preview.yml" "$s/readme" >"$s/out" 2>&1 </dev/null || st=$?
row "check-preview-contract.sh, HOME=<that config>" "$st" "$(grep -m1 'does not document' "$s/out" | cut -c 1-60); ran: $(ran)"
if [ -z "$quick" ]; then
	st=0; HOME="$home" ./scripts/test-gate-env.sh >"$s/out" 2>&1 </dev/null || st=$?
	row "test-gate-env.sh, HOME=<that config>" "$st" "$(tail -1 "$s/out" | cut -c 1-60); ran: $(ran)"
	st=0; HOME="$home" GIT_TRACE="$s/trace-tcns" ./scripts/test-check-no-secrets.sh >"$s/out" 2>&1 </dev/null || st=$?
	row "test-check-no-secrets.sh, HOME=<that config> GIT_TRACE=<file>" "$st" "$(tail -1 "$s/out" | cut -c 1-50); ran: $(ran); trace file: $([ -e "$s/trace-tcns" ] && echo WRITTEN || echo absent)"
fi

printf '\n== check-no-secrets.sh: GIT_DIR at a decoy that tracks the fixture'"'"'s clean files\n'
rm -rf "$s/decoy2"; mkdir -p "$s/decoy2/scripts"
printf 'clean\n' >"$s/decoy2/a.txt"; cp scripts/check-no-secrets.sh "$s/decoy2/scripts/"
sg -C "$s/decoy2" init -q; sg -C "$s/decoy2" add -A; sg -C "$s/decoy2" commit -q -m decoy
st=0; GIT_DIR="$s/decoy2/.git" "$s/keyed/scripts/check-no-secrets.sh" >"$s/out" 2>&1 </dev/null || st=$?
row "GIT_DIR=<decoy tracking a.txt and the script>" "$st" "$(said "$s/out")"

printf '\n== check-no-secrets.sh: a checkout-local core.worktree naming a clean tree with the same names\n'
mkdir -p "$s/else/scripts"; cp scripts/check-no-secrets.sh "$s/else/scripts/"
printf 'clean\n' >"$s/else/a.txt"; printf 'clean\n' >"$s/else/b.txt"
sg -C "$s/keyed" config core.worktree "$s/else"
st=0; "$s/keyed/scripts/check-no-secrets.sh" >"$s/out" 2>&1 </dev/null || st=$?
row ".git/config core.worktree=<a clean tree, same names>" "$st" "$(said "$s/out")"
sg -C "$s/keyed" config --unset core.worktree
