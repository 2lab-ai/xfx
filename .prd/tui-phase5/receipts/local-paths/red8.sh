#!/usr/bin/env bash
# LOCAL-PATHS round 8: capture.sh's `dir` and the binaries' TMPDIR names in
# any case, on a tree's own capture.sh, under the bash given (macOS's 3.2 by
# default). One line of verdict each; no path printed.
#   red8.sh <tree> [bash]
set -u
tree=$1 sh=${2:-/bin/bash}
work=$(mktemp -d "${TMPDIR:-/tmp}/lp-red8.XXXXXX")
trap 'rm -rf "$work"' EXIT
printf '%-34s %s\n' "the volume:" "$(d=$work/ci; mkdir -p "$d/fxtmp"; [ -d "$d/FXTMP" ] && echo "case-insensitive (FXTMP is fxtmp)" || echo "case-sensitive")"
printf '%-34s %s\n' "the bash:" "$("$sh" -c 'echo $BASH_VERSION')"
i=0
for name in FXTMP FxTmp XFXTMP Fxtmp-old xFxTmP.bak fxtmp Notes tmp xfx; do
	# Numbered: on a case-insensitive volume FXTMP and FxTmp are one directory.
	i=$((i + 1))
	d=$work/$i
	mkdir -p "$d/scen" "$d/bin"
	ln -s /bin/bash "$d/bin/it"
	printf 'dir %s\nwait 1\nsnap a\n' "$name" >"$d/scen/t.cap"
	status=0
	env -u TMUX -u TMUX_PANE FXCAP_SCENARIOS="$d/scen" TMPDIR="$d" FX_BIN="$d/bin/it" \
		"$sh" "$tree/scripts/fxcap/capture.sh" fx t 30x200 "$d/out" >"$d/log" 2>&1 </dev/null || status=$?
	for sock in "$d"/fxcap.*/s; do [ -S "$sock" ] && tmux -f /dev/null -S "$sock" kill-server 2>/dev/null; done
	if [ "$status" -eq 2 ] && grep -q 'dir takes a plain name' "$d/log"; then
		verdict="refused, exit 2"
	else
		verdict="run, exit $status"
	fi
	case $name in
	Notes | tmp | xfx) [ "$status" -eq 0 ] || verdict="$verdict (RED)" ;;
	*) [ "$status" -eq 2 ] || verdict="$verdict (RED)" ;;
	esac
	printf '%-34s %s\n' "dir $name:" "$verdict"
done
