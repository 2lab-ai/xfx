#!/usr/bin/env bash
# CTRLO-OTHER-PICKERS' sweep (CTRLO-AT-KEYS', with this unit's --only list): the base's xfx and the head's against fx v0.0.16,
# one after the other, inside one hold of the suite lock
# (`suite-lock.sh sweeps.sh ...`), the same scenarios, ledger and size.
# sweep.py checks fx's sha256 before and after each run. The list: this
# unit's scenarios and every scenario of each picker it touches -- the
# catalog (`help*`), `/model` (`model*`), the session picker (`resume*`; no
# scenario is named `sessions*`), `/fork` and `/effort`
# (`p5-effort-fork-*`), `/skills` (`skills*`), `/settings` (`settings*`),
# the slash menu (`slash*`), `/provider` (`provider*`), `/mcp`'s menu and
# argument completions (`p5-mcp-menu*`, `p5-mcp-verbs`) -- the `p5-ctrlo*`
# family and the full transcript's own (`fulltranscript*`).
#
# usage: suite-lock.sh sweeps.sh <repo> <fx> <xfx base> <xfx head> <out dir>
set -euo pipefail
repo=$1 fx=$2 base=$3 head=$4 out=$5
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
only=(--only 'p5-ctrlo*' --only 'help*' --only 'model*' --only 'resume*' --only 'p5-effort-fork-*' --only 'skills*' --only 'settings*' --only 'slash*' --only 'provider*' --only 'p5-mcp-menu*' --only 'p5-mcp-verbs' --only 'fulltranscript*')
for side in base head; do
  bin=$base
  [ "$side" = head ] && bin=$head
  start=$SECONDS
  "$repo/scripts/fxcap/sweep.sh" --fx "$fx" --pin-sha "$pin" --xfx "$bin" --size 40x120 --jobs 2 "${only[@]}" --out "$out/$side" > "$out/$side.log" 2>&1 || echo "# $side sweep exit $?"
  echo "# $side sweep: $((SECONDS - start)) s"
done
