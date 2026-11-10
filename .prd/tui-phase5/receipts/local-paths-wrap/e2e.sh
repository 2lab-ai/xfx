#!/usr/bin/env bash
# LOCAL-PATHS-WRAP: train 12's trial (../train-12/ci-mcp-timing/gutter_check.py)
# as the end-to-end RED and GREEN. p5-mcp-menu-new is swept, fx v0.0.16 and
# xfx, from a copy of <tree>'s scripts/fxcap/scenarios at a path of 118
# bytes, so xfx's approval wraps that directory behind `↳`; the sweep runs
# <tree>'s capture.sh, and with it <tree>'s roots.py. Then, on each side's
# approve-confirm capture: rows.py (what leads each row holding a piece of
# the directory), and gutter_check.py with <tree>'s roots.py --check and,
# given, with <other>'s. Prints no path; the captures stay in <work>.
#   e2e.sh <tree> <fx> <xfx> <work> [<other tree>]     (under the suite lock)
set -u
tree=$1 fx=$2 xfx=$3 work=$4 other=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
gutter=.prd/tui-phase5/receipts/train-12/ci-mcp-timing/gutter_check.py
rm -rf "$work" && mkdir -p "$work/t" "$work/c" || exit 2
base=$work/c/
pad=$((118 - $(printf %s "$base" | LC_ALL=C wc -c)))
scen=$base$(printf "w%.0s" $(seq 1 "$pad"))
cp -R "$tree/scripts/fxcap/scenarios" "$scen" || exit 2
echo "head of <tree> $(git -C "$tree" rev-parse --short=8 HEAD); roots.py there: sha256 $(shasum -a 256 "$tree/scripts/fxcap/roots.py" | cut -c1-12)"
echo "the scenario directory: $(printf %s "$scen" | LC_ALL=C wc -c | tr -d ' ') bytes, outside the run's TMPDIR and HOME"
echo "xfx: sha256 $(shasum -a 256 "$xfx" | cut -c1-12), banner $("$xfx" --version 2>&1 | head -1)"
before=$(shasum -a 256 "$fx" | cut -d' ' -f1)
(cd "$tree" && FX_AUTO_UPGRADE=0 bash scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$xfx" \
  --jobs 2 --tmp "$work/t" --only p5-mcp-menu-new --scenarios "$scen" --out "$work/out" >"$work/sweep.log" 2>&1 </dev/null)
status=$?
after=$(shasum -a 256 "$fx" | cut -d' ' -f1)
echo "sweep exit $status; fx oracle sha256 $([ "$before" = "$pin" ] && [ "$after" = "$pin" ] && echo "the pin, before and after" || echo CHANGED); $(grep -m1 '^pass = ' "$work/sweep.log")"
for side in fx xfx; do
  dir=$work/out/runs/p5-mcp-menu-new/$side
  cap=$dir/$side-p5-mcp-menu-new-40x120-approve-confirm.txt
  echo "== $side: run $(python3 -I -c 'import json,sys; s=json.load(open(sys.argv[1])); print("exit", s.get("exit", s.get("status", "?")))' "$dir/status.json" 2>/dev/null || echo '(no status.json)'); approve-confirm"
  [ -f "$cap" ] || { echo "no capture"; continue; }
  python3 -I "$here/rows.py" "$cap" "$scen"
  echo "-- gutter_check.py, <tree>'s roots.py:"
  python3 -I "$tree/$gutter" "$cap" "$scen" "$tree"
  if [ -n "$other" ]; then
    echo "-- gutter_check.py, <other>'s roots.py ($(git -C "$other" rev-parse --short=8 HEAD); sha256 $(shasum -a 256 "$other/scripts/fxcap/roots.py" | cut -c1-12)):"
    python3 -I "$tree/$gutter" "$cap" "$scen" "$other" | tail -1
  fi
done
