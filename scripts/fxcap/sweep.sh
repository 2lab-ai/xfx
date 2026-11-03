#!/usr/bin/env bash
# The parity sweep: every scenario in scenarios/ on fx and on xfx, every snap
# pair compared cell by cell against parity-ledger.toml, a scoreboard in
# --out. sweep.py's docstring is the manual; this only runs it isolated
# (`python3 -I`: no environment, no user site, no script directory on the
# import path but the one sweep.py puts there itself).
#
# usage: scripts/fxcap/sweep.sh --fx <bin> --pin-sha <sha256> [--xfx <bin>]
#          [--size 40x120] [--jobs N] [--only <glob>]... --out <dir>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 -I "$here/sweep.py" "$@"
