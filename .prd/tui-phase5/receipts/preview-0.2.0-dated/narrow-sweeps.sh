#!/bin/bash
# usage: narrow-sweeps.sh <xfx binary> <out prefix> [<group>...]
#
# Every scenario with a snap narrower than the widest 0.2.0 banner (74 cells;
# narrow-snaps.txt), through the tree's own sweep.sh against the pinned fx
# v0.0.16 oracle (its sha256 checked by sweep.py before and after, and by
# capture.sh around each run; capture.sh launches fx with FX_AUTO_UPGRADE=0
# last in its environment). One sweep per run size, the sizes the receipts
# take these scenarios at:
#
#   120  40x120  h1-resize (resizes to 20x60), p5-transcript-question (40x60)
#   50   24x50   p5-panels-status-short (P-status)
#   60   40x60   p5-mcp-menu-new (MCP-M)
#   u50  40x50   p5-usage-cached, -budget, -many, -nokey, -turns (#62)
#
# At 50 columns fx's help wraps a slash row the sweep's inventory refuses, so
# the 24x50 and 40x50 sweeps take every capture and then stop without a
# scoreboard; narrow_pairs.py classifies every group's captures the same way.
# A group's exit status is printed, not fatal.
set -uo pipefail
xfx="$1" out="$2"
shift 2
groups=("$@")
[ "${#groups[@]}" -gt 0 ] || groups=(120 50 60 u50)
tree="${NARROW_TREE:?the xfx checkout whose sweep.sh runs}"
fx="${NARROW_FX:?the fx v0.0.16 executable}"
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
cd "$tree" || exit 1
for group in "${groups[@]}"; do
	case "$group" in
	120) size=40x120 only=(--only h1-resize --only p5-transcript-question) ;;
	50) size=24x50 only=(--only p5-panels-status-short) ;;
	60) size=40x60 only=(--only p5-mcp-menu-new) ;;
	u50) size=40x50 only=(--only p5-usage-cached --only p5-usage-budget --only p5-usage-many \
		--only p5-usage-nokey --only p5-usage-turns) ;;
	*) echo "unknown group $group" >&2; exit 2 ;;
	esac
	rm -rf "$out-$group"
	./scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$xfx" --jobs 1 \
		--size "$size" "${only[@]}" --out "$out-$group"
	echo "group $group ($size): sweep.sh exit $?"
done
shasum -a 256 "$fx" "$xfx"
