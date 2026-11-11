#!/usr/bin/env bash
# Train 14 (new): the join checks that capture, on the head's debug build
# (the head sweep's clone, built by sweep.sh), with that tree's harness,
# scenarios and ledger, fx v0.0.16 at the pin (sha256 checked before and
# after each run, FX_AUTO_UPGRADE=0). Run each part inside one hold of the
# suite lock.
#
#   tmpdir <tree> <out>  join check 1: `p5-input-row-limit*`, `p5-input-at*`
#                        and `workspace*` swept with TMPDIR (and so the
#                        sweep's run root, `--tmp`'s default) a directory of
#                        24 bytes and then one of 49 (CI's runner's length,
#                        train 12's ci-socket receipt), each `/tmp/claude-<uid>/`
#                        through the `/tmp` link, a name, and a `/`
#   walk <tree> <out>    join check 2: `p5-input-row-limit` swept five times,
#                        one after another (sweep-rerun.sh), the load at each
#                        start printed; then each run's `at` snap, row 5 on
#                        each side (walk_rows.py)
#
# usage: FX=<fx v0.0.16> joins.sh <part> <tree> <out>
set -u
part=$1 tree=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
FX=${FX:?the fx v0.0.16 binary}
PIN=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
export FX_AUTO_UPGRADE=0
mkdir -p "$out"
case "$part" in
tmpdir)
  root=/tmp/claude-$(id -u)/
  for want in 24 49; do
    pad=$((want - ${#root} - 1))
    name=$(printf 'j%0*d' $((pad - 1)) 0)
    dir="$root$name/"
    mkdir -p "$dir"
    echo "# TMPDIR of ${#dir} bytes (wanted $want); load $(sysctl -n vm.loadavg | awk '{print $2}'); fx sha256 before $(shasum -a 256 "$FX" | cut -c1-64)"
    TMPDIR=$dir bash "$tree/scripts/fxcap/sweep.sh" --fx "$FX" --pin-sha "$PIN" --xfx "$tree/target/debug/xfx" --jobs 2 \
      --size 40x120 --only 'p5-input-row-limit*' --only 'p5-input-at*' --only 'workspace*' --out "$out/j1-$want" > "$out/j1-$want.log" 2>&1
    echo "# exit $?; fx sha256 after $(shasum -a 256 "$FX" | cut -c1-64); $(grep -E '^pass =' "$out/j1-$want.log")"
  done
  ;;
walk)
  SIZE=40x120 SHORT_TMP=$out bash "$here/sweep-rerun.sh" walk "$tree" p5-input-row-limit 5 "$out/j2"
  python3 -I -B "$here/walk_rows.py" "$out"/j2-1 "$out"/j2-2 "$out"/j2-3 "$out"/j2-4 "$out"/j2-5
  ;;
esac
