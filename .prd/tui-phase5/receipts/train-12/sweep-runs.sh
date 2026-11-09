#!/usr/bin/env bash
# Train 12 (train 11's sweep-runs.sh, its sweeps this train's):
# sweep-runs.txt, through redacted.py -- each sweep's own lines as sweep.sh
# printed them (each inside one hold of the suite lock, FX the v0.0.16
# binary, SHORT_TMP a short scratch directory), the last lines of its log, its
# run.json's start, wall time, load and the xfx it ran, the banners its xfx
# snaps show (banners.py), and main's sweep rescored with the head's ledger
# (sweep.sh --rescore on a copy): the pass line before and after.
# usage: sweep-runs.sh <scratch> <head-rev7> <main-rev7> <head-clone>
set -u
T=$1 head=$2 main=$3 hclone=$4
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 12's sweeps, as sweep.sh printed them; then each sweep's run.json, the banners (banners.py) and main's rescore (sweep.sh --rescore on a copy, with the head's ledger)."
  for s in main head irl41 irl8; do
    echo; echo "## $s"
    grep -v '^exit' "$T/sweep-$s.run"
    grep '^exit' "$T/sweep-$s.run" | sed 's/^/# suite-lock.sh /'
    python3 -I -B - "$T/sw-$s/run.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
keys = [k for k in ("started", "wall", "start_load", "peak_load", "jobs", "size", "xfx_sha256", "fx_sha256", "repo_head") if k in d]
print("run.json: " + "; ".join(f"{k} {d[k]}" for k in keys))
PY
  done
  echo; echo "## banners"
  python3 -I -B "$here/banners.py" "head=$T/sw-head:$head" "main=$T/sw-main:$main" "irl41=$T/sw-irl41:$head" "irl8=$T/sw-irl8:$head"
  echo; echo "## main's sweep rescored with the head's ledger"
  echo "ref-main: before $(grep -o 'pass = [0-9]* / [0-9]*' "$T/sw-main/scoreboard.md"); after $(grep -o 'pass = [0-9]* / [0-9]*' "$T/ref-main/scoreboard.md"); ledger $(grep -o 'ledger sha256 `[0-9a-f]\{8\}' "$T/ref-main/scoreboard.md" | tail -c 9) (the head's $(shasum -a 256 "$hclone/scripts/fxcap/parity-ledger.toml" | cut -c1-8)); run at $(python3 -I -c "import json,sys; print(json.load(open(sys.argv[1]))['repo_head'][:8])" "$T/ref-main/run.json")"
} | python3 -I -B "$here/redacted.py" "$here/sweep-runs.txt" "$T=<scratch>"
