#!/usr/bin/env bash
# The parity sweep's own tests (scripts/fxcap/test_sweep.py): cells.py and the
# M1 receipt's copy of it, the ledger's classification, the redaction, a sha
# mismatch, and failed and timed-out scenarios in the denominator, end to end
# over a fake capture script. Runs no binary and no tmux.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
for script in "$here"/*.sh; do bash -n "$script"; done
exec python3 -I "$here/test_sweep.py" "$@"
