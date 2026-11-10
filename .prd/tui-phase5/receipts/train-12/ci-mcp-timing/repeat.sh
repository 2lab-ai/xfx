#!/bin/bash
# One scenario swept N times on each of two xfx builds, with this checkout's
# harness, scenarios and ledger (as ci-sweep runs both builds), fx v0.0.16,
# `--jobs 2`, at the TMPDIR it is given. The oracle's sha256 is read before
# and after every sweep. Each sweep's out directory is kept for the readers:
# verdicts.py, words.py, timeline.py, scan.py, content_diff.py, verbs_rows.py.
# usage: repeat.sh <repo> <fx v0.0.16> <head xfx> <base xfx> <tmpdir> <--tmp dir> <out> <runs> <scenario>...
# SCENARIOS_DIR, when set, is the scenario directory instead of the repo's
# (a scratch copy with a change under trial); the ledger is the repo's.
set -u
repo=$1 fx=$2 head=$3 base=$4 tmp=$5 rt=$6 out=$7 n=$8; shift 8
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
only=(); for s in "$@"; do only+=(--only "$s"); done
[ -n "${SCENARIOS_DIR:-}" ] && only+=(--scenarios "$SCENARIOS_DIR")
mkdir -p "$out" "$rt"
for i in $(seq 1 "$n"); do
  for side in head base; do
    bin=$head; [ "$side" = base ] && bin=$base
    o=$out/$side-$i
    rm -rf "$o"
    before=$(shasum -a 256 "$fx" | cut -d' ' -f1)
    ( cd "$repo" && TMPDIR=$tmp FX_AUTO_UPGRADE=0 bash scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$bin" \
        --jobs 2 --tmp "$rt" "${only[@]}" --out "$o" > "$o.log" 2>&1 )
    status=$?
    after=$(shasum -a 256 "$fx" | cut -d' ' -f1)
    echo "$side-$i exit=$status fx-sha-same=$([ "$before" = "$pin" ] && [ "$after" = "$pin" ] && echo yes || echo NO) $(grep -m1 '^pass = ' "$o.log")"
  done
done
