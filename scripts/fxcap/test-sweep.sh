#!/usr/bin/env bash
# The parity sweep's own tests (scripts/fxcap/test_sweep.py): cells.py and the
# M1 receipt's copy of it, the ledger's classification, the redaction, a sha
# mismatch, and failed and timed-out scenarios in the denominator, end to end
# over a fake capture script; then the CI driver's (test_ci_sweep.py): the
# selection, the base comparison and the re-runs on both builds, over a scratch
# repository; ci-fetch.sh's refusals, with a stub curl; and the sweep
# workflows' invariants (read-only token, no secret, no fork-privileged
# trigger, no expression in a shell) and their base and build steps, run.
# Runs no binary, no tmux and no network; reads the workflows with ruby.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
for script in "$here"/*.sh; do bash -n "$script"; done
python3 -I "$here/test_sweep.py" "$@"
exec python3 -I "$here/test_ci_sweep.py" "$@"
