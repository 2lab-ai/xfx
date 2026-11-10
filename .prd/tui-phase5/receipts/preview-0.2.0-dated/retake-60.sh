#!/bin/bash
# usage: retake-60.sh <base xfx> <head xfx> <out prefix> <n>
# p5-mcp-menu-new at 40x60, <n> more times on each binary, alternating base
# and head so both see the same machine load: `<prefix>-base-<i>-60` and
# `<prefix>-head-<i>-60`, each a narrow-sweeps.sh group-60 output.
set -uo pipefail
base="$1" head="$2" out="$3" n="$4"
here="$(cd "$(dirname "$0")" && pwd)"
for i in $(seq 1 "$n"); do
	bash "$here/narrow-sweeps.sh" "$base" "$out-base-$i" 60
	bash "$here/narrow-sweeps.sh" "$head" "$out-head-$i" 60
done
