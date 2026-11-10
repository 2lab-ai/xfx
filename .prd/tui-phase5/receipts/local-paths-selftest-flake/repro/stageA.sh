#!/bin/bash
set -euo pipefail
repo_root="${LPSF_REPO:?}"; n="$1"; bad=0
scratch_git() {
	GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -c core.fsmonitor=false "$@"
}

# A new tree at $1 holding the check, its own repository.
new_tree() {
	mkdir -p "$1/scripts"
	cp "$repo_root/scripts/check-no-local-paths.sh" "$1/scripts/"
	scratch_git -C "$1" init -q
}
track() {
	scratch_git -C "$1" add -A -f
}
s='/'
mac="${s}Users${s}alice${s}"
linux="${s}home${s}bob${s}"
harness_mac="${s}private${s}tmp${s}claude-501"
harness_linux="${s}tmp${s}claude-1000"
folders="${s}private${s}var${s}folders${s}ab${s}T${s}"
folders_bare="${s}var${s}folders${s}ab${s}T${s}"
home_shape='a macOS home directory'
linux_shape='a Linux home directory'
harness_shape='a harness scratch root'
folders_shape='a macOS per-user temporary root'
for i in $(seq 1 "$n"); do
work="$(mktemp -d "${TMPDIR:-/tmp}/lpsf-A.XXXXXX")"
t="$work/shapes"
new_tree "$t"
odd="$(printf 'caf\303\251\tname.md')"
printf 'cd %srepo\n' "$mac" >"$t/mac.md"
printf 'HOME=%s\n' "$linux" >"$t/linux.sh"
printf 'TMPDIR = "%s/x"\n' "$harness_mac" >"$t/harness.py"
printf 'one\n%s/x\n' "$harness_linux" >"$t/linux-harness.txt"
printf '\033]8;;file://%sshot.png\033\\x\033]8;;\033\\\n' "$folders" >"$t/link.ansi"
printf '{"cwd": "%sx"}\n' "$folders_bare" >"$t/body.json"
printf '\033[1m\033[38;5;245m%sx\033[39m\n' "$mac" >"$t/colour.ansi"
printf 'see %sx\n' "$mac" >"$t/$odd"
printf '%s\n' "$linux" >"$t/-"
mkdir -p "$t/-x"
printf 'a\nb\n%s' "$harness_mac" >"$t/-x/last.txt"
printf 'PNG\000%s\n' "$mac" >"$t/blob.bin"
printf '%s and %s\n' "${s}home${s}u${s}" "$linux" >"$t/both.rs"
printf '%s // local-path-ok\n%s\n' "$folders" "$folders" >"$t/marked.rs"
printf 'log >%sx\n' "$linux" >"$t/redirect.sh"
printf '<code>%sx</code>\n' "$mac" >"$t/tag.html"
printf 'cd %s\n(%s)\n' "${s}Users${s}alice" "${s}home${s}bob" >"$t/bare.md"
printf '  \033[38;5;250mclaude-501%sfxcap.Ab12Cd%swork\n' "$s" "$s" >"$t/wrapped.ansi"
printf 'a short directory in the harness temp folder (`claude-%s%sp5uc`)\n' 501 "$s" >"$t/ticked.md"
# Known conservative matches, refused on purpose: `@` and a letter past ASCII
# can be part of a name, but the shape's left context is a heuristic (a
# character no ASCII path name of these repositories holds), so these fail
# toward refusal.
printf 'cache@claude-3%sx\n' "$s" >"$t/at-sign.md"
printf '\303\251claude-3%sx\n' "$s" >"$t/accented.md"
mkdir -p "$t/fx-x-40x120-bodies"
printf '{"cwd":"%sx"} // local-path-ok\n' "$linux" >"$t/fx-x-40x120-bodies/001-completion.json"
printf '%sx # local-path-ok\n' "$mac" >"$t/notes.txt"
printf 'see %sx local-path-ok here\n' "$mac" >"$t/midline.md"
mkdir -p "$t/w3"
printf 'Workspace: %swork  # local-path-ok\n' "$linux" >"$t/w3/xfx-trace-40x120-report-1.md"
printf 'cp x %subuntu%sy; cd %su\n' "${s}home${s}" "$s" "${s}home${s}" >"$t/sentinel.sh"
printf 'Workspace: %swork\n' "$linux" >"$t/w3/trace-report.md"
mkdir -p "$t/docs"
printf 'cd %swork  # local-path-ok: an example\n' "$linux" >"$t/docs/fx-guide-40x120-notes.md"
track "$t"
c="$(scratch_git -C "$t" ls-files | wc -l | tr -d ' ')"
w="$(scratch_git -C "$t" ls-files -- w3 | wc -l | tr -d ' ')"
if [ "$c" != 28 ] || [ "$w" != 2 ]; then bad=$((bad + 1)); echo "A-bad iter=$i count=$c w3=$w keep=$work"; else rm -rf "$work"; fi
done
echo "stageA: $bad bad of $n"
