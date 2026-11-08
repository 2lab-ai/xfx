#!/usr/bin/env bash
#
# GATE-GITENV's sweep: each git-reading gate, run with each of git's
# environment variables (and the attribute files a clone can hold) pointed at
# a decoy, against a fixture holding one violation it must refuse.
#
#   gitenv-sweep.sh <copy>       (a disposable `git clone --no-local`)
#
# Every row records: the gate's exit status, the line it refused the
# violation with (or its `ok`), and whether any file under the decoy
# repository's .git changed. A correct row is exit 1 with the violation
# named and the decoy unchanged.

set -uo pipefail

copy=$1
cd "$copy" || exit 1
for name in $(compgen -e); do
	case "$name" in
	GIT_*) unset "$name" ;;
	esac
done
unset GREP_OPTIONS
s="$(mktemp -d "${TMPDIR:-/tmp}/xfx-gitenv.XXXXXX")"
trap 'rm -rf "$s"' EXIT

section() {
	printf '\n== %s\n' "$1"
}

section "platform"
printf 'head %s\n' "$(git rev-parse --short HEAD)"
uname -sm
bash --version | head -1
git --version

retired="f""x""r"
key="s""k-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

# --- the fixtures -------------------------------------------------------------

# identity, a tracked path that carries the retired name (content clean)
mkdir -p "$s/ipath/scripts" "$s/ipath/docs"
cp scripts/check-xfx-identity.sh "$s/ipath/scripts/"
printf 'Clean text.\n' >"$s/ipath/docs/${retired}rename-notes.md"
git -C "$s/ipath" init -q && git -C "$s/ipath" add -A

# identity, the retired name inside a tracked Cargo.lock
mkdir -p "$s/ilock/scripts"
cp scripts/check-xfx-identity.sh "$s/ilock/scripts/"
printf 'name = "%s-core"\n' "$retired" >"$s/ilock/Cargo.lock"
git -C "$s/ilock" init -q && git -C "$s/ilock" add -A

# no-secrets, a key in b.txt
mkdir -p "$s/keyed/scripts"
cp scripts/check-no-secrets.sh "$s/keyed/scripts/"
printf 'clean\n' >"$s/keyed/a.txt"
printf '%s\n' "$key" >"$s/keyed/b.txt"
git -C "$s/keyed" init -q && git -C "$s/keyed" add -A

# preview-contract, a README without the install command
mkdir -p "$s/readme/.github/workflows"
cp .github/workflows/*.yml "$s/readme/.github/workflows/"
cp .github/preview-epoch "$s/readme/.github/preview-epoch"
grep -v -F -e 'brew install' README.md >"$s/readme/README.md"

# indexes that lack the violation
GIT_INDEX_FILE="$s/ipath.index" git -C "$s/ipath" add scripts/check-xfx-identity.sh
GIT_INDEX_FILE="$s/ilock.index" git -C "$s/ilock" add scripts/check-xfx-identity.sh
GIT_INDEX_FILE="$s/keyed.index" git -C "$s/keyed" add a.txt scripts/check-no-secrets.sh

# attributes that call a .lock binary, and config files that name them
printf '*.lock -diff\n' >"$s/attributes"
printf '[core]\n\tattributesFile = %s\n' "$s/attributes" >"$s/attr.config"

# a tree whose .gitattributes says the same, for GIT_ATTR_SOURCE
attr_blob="$(printf '*.lock -diff\n' | git -C "$s/ilock" hash-object -w --stdin)"
attr_tree="$(printf '100644 blob %s\t.gitattributes\n' "$attr_blob" | git -C "$s/ilock" mktree)"

# a program that records it was run, for GIT_EXTERNAL_DIFF
printf '#!/bin/sh\n: >>"%s/external-diff-ran"\n' "$s" >"$s/external-diff"
chmod 755 "$s/external-diff"

new_decoy() {
	rm -rf "$s/decoy"
	mkdir -p "$s/decoy"
	git -C "$s/decoy" init -q
	printf 'decoy\n' >"$s/decoy/d.txt"
	git -C "$s/decoy" add d.txt
	git -C "$s/decoy" -c user.name=d -c user.email=d@example.invalid commit -q -m decoy
}

fingerprint() {
	(cd "$s/decoy/.git" && find . -type f -exec cksum {} + | sort)
}

# probe <gate> <label> [NAME=value...]: runs the gate on its fixture.
probe() {
	local gate="$1" label="$2" status=0 line changed
	shift 2
	new_decoy
	rm -f "$s/external-diff-ran"
	fingerprint >"$s/before"
	case "$gate" in
	ipath) env "$@" "$s/ipath/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || status=$? ;;
	ilock) env "$@" "$s/ilock/scripts/check-xfx-identity.sh" >"$s/out" 2>&1 </dev/null || status=$? ;;
	keyed) env "$@" "$s/keyed/scripts/check-no-secrets.sh" >"$s/out" 2>&1 </dev/null || status=$? ;;
	readme) env "$@" scripts/check-preview-contract.sh "$s/readme/.github/workflows/preview.yml" "$s/readme" >"$s/out" 2>&1 </dev/null || status=$? ;;
	esac
	fingerprint >"$s/after"
	if cmp -s "$s/before" "$s/after"; then changed="unchanged"; else changed="CHANGED"; fi
	[ -e "$s/external-diff-ran" ] && changed="$changed, external diff RAN"
	line="$(grep -m1 -E 'is still in tracked|possible OpenAI|does not document the qualified|this scan is not working|flags upstream|cover|: ok|cannot be read|not a git|fatal|error' "$s/out" | cut -c 1-80)"
	printf '%-7s %-34s exit %d  decoy %-9s  %s\n' "$gate" "$label" "$status" "$changed" "$line"
}

section "each gate on its fixture, under each variable"
for gate in ipath ilock keyed readme; do
	case "$gate" in
	ipath) index="$s/ipath.index" ;;
	ilock) index="$s/ilock.index" ;;
	keyed) index="$s/keyed.index" ;;
	readme) index="$s/keyed.index" ;;
	esac
	probe "$gate" "(none)"
	probe "$gate" "GIT_DIR=decoy" GIT_DIR="$s/decoy/.git"
	probe "$gate" "GIT_WORK_TREE=decoy" GIT_WORK_TREE="$s/decoy"
	probe "$gate" "GIT_DIR+GIT_WORK_TREE=decoy" GIT_DIR="$s/decoy/.git" GIT_WORK_TREE="$s/decoy"
	probe "$gate" "GIT_INDEX_FILE=<without it>" GIT_INDEX_FILE="$index"
	probe "$gate" "GIT_OBJECT_DIRECTORY=decoy" GIT_OBJECT_DIRECTORY="$s/decoy/.git/objects"
	probe "$gate" "GIT_ALTERNATE_OBJECT_DIRECTORIES" GIT_ALTERNATE_OBJECT_DIRECTORIES="$s/decoy/.git/objects"
	probe "$gate" "GIT_COMMON_DIR=decoy" GIT_COMMON_DIR="$s/decoy/.git"
	probe "$gate" "GIT_NAMESPACE=elsewhere" GIT_NAMESPACE=elsewhere
	probe "$gate" "GIT_CEILING_DIRECTORIES=<parent>" GIT_CEILING_DIRECTORIES="$s"
	probe "$gate" "GIT_CONFIG_GLOBAL=<attributes>" GIT_CONFIG_GLOBAL="$s/attr.config"
	probe "$gate" "GIT_CONFIG_SYSTEM=<attributes>" GIT_CONFIG_SYSTEM="$s/attr.config"
	probe "$gate" "GIT_CONFIG_NOSYSTEM=1" GIT_CONFIG_NOSYSTEM=1
	probe "$gate" "GIT_CONFIG_COUNT=<attributes>" GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.attributesFile GIT_CONFIG_VALUE_0="$s/attributes"
	probe "$gate" "GIT_CONFIG_PARAMETERS=<attributes>" GIT_CONFIG_PARAMETERS="'core.attributesfile'='$s/attributes'"
	probe "$gate" "GIT_ATTR_SOURCE=<tree>" GIT_ATTR_SOURCE="$attr_tree"
	probe "$gate" "GIT_EXTERNAL_DIFF=<recorder>" GIT_EXTERNAL_DIFF="$s/external-diff" GIT_DIFF_OPTS=-u9
done

section "identity: attributes a clone holds, not the environment"
cp "$s/attributes" "$s/ilock/.git/info/attributes"
probe ilock ".git/info/attributes"
rm -f "$s/ilock/.git/info/attributes"
git -C "$s/ilock" config core.attributesFile "$s/attributes"
probe ilock ".git/config core.attributesFile"
git -C "$s/ilock" config --unset core.attributesFile
cp "$s/attributes" "$s/ilock/.gitattributes"
probe ilock "an untracked .gitattributes"
rm -f "$s/ilock/.gitattributes"
git -C "$s/ilock" config color.grep always
probe ilock ".git/config color.grep=always"
git -C "$s/ilock" config --unset color.grep
