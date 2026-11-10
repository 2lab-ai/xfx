#!/bin/bash
# The surface p5-panels-badsettings was written for, swept on its own: the
# scenario that captures it the right way, and H1's `rehomefile` one, with
# fx v0.0.16 and the head's xfx, at the runner-length TMPDIR, the oracle's
# sha256 read before and after.
# usage: surface-sweep.sh <repo> <fx v0.0.16> <xfx> <runner-length tmpdir> <--tmp dir> <out>
set -u
repo=$1 fx=$2 xfx=$3 long=$4 rt=$5 out=$6
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
echo "fx sha256 before: $(shasum -a 256 "$fx" | cut -d' ' -f1)"
mkdir -p "$rt"
( cd "$repo" && TMPDIR=$long FX_AUTO_UPGRADE=0 bash scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$xfx" \
    --jobs 2 --tmp "$rt" --only p5-panels-status-badsettings --only h1-rehomefile --out "$out" 2>&1 )
echo "exit=$?"
echo "fx sha256 after:  $(shasum -a 256 "$fx" | cut -d' ' -f1)"
