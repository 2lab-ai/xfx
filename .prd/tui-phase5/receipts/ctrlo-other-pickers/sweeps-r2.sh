#!/usr/bin/env bash
# CTRLO-OTHER-PICKERS round 2's sweep (sweeps.sh's, with round 2's sides and
# list): round 1's head build (`v0.1.0-91c4673`, the code of 69491c9a) and
# round 2's, against fx v0.0.16, one after the other, inside one hold of the
# suite lock, the same scenarios, ledger and size. The list: round 1's, and
# the `@` list's families (`filepicker*`, `p5-input-at*`), which round 2's
# merged anchor and its one C-j/C-k check reach.
#
# usage: suite-lock.sh sweeps-r2.sh <repo> <fx> <xfx round 1> <xfx round 2> <out dir>
set -euo pipefail
repo=$1 fx=$2 base=$3 head=$4 out=$5
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
only=(--only 'p5-ctrlo*' --only 'help*' --only 'model*' --only 'resume*' --only 'p5-effort-fork-*' --only 'skills*' --only 'settings*' --only 'slash*' --only 'provider*' --only 'p5-mcp-menu*' --only 'p5-mcp-verbs' --only 'fulltranscript*' --only 'filepicker*' --only 'p5-input-at*')
for side in base head; do
  bin=$base
  [ "$side" = head ] && bin=$head
  start=$SECONDS
  "$repo/scripts/fxcap/sweep.sh" --fx "$fx" --pin-sha "$pin" --xfx "$bin" --size 40x120 --jobs 2 "${only[@]}" --out "$out/$side" > "$out/$side.log" 2>&1 || echo "# $side sweep exit $?"
  echo "# $side sweep: $((SECONDS - start)) s"
done
