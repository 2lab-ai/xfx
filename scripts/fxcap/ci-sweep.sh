#!/usr/bin/env bash
# The parity sweep in CI, per pull request: ci_sweep.py's docstring is the
# manual; this only runs it isolated, as sweep.sh runs sweep.py.
#
# usage: scripts/fxcap/ci-sweep.sh --base <rev> --fx <bin> --pin-sha <sha256>
#          --pin <name> --xfx <bin> --base-xfx <bin> --out <dir> [--jobs N]
#        scripts/fxcap/ci-sweep.sh --select --base <rev>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
exec python3 -I "$here/ci_sweep.py" "$@"
