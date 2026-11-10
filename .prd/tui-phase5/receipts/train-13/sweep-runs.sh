#!/usr/bin/env bash
# Train 13 (train 12's sweep-runs.sh, its sweeps this train's: the head and
# the base f53606d1, train 12's head): sweep-runs.txt, through redacted.py --
# each sweep's own lines as sweep.sh printed them (each inside one hold of the
# suite lock, FX the v0.0.16 binary, SHORT_TMP a short scratch directory), its
# run.json's start, wall time, load and the xfx it ran, the banners its xfx
# snaps show (banners.py), and the base's sweep rescored with the head's ledger
# (sweep.sh --rescore on a copy, ref-base): the pass line before and after,
# and the verdicts the rescore moved, pair by pair. The same for the extra
# selection's two sweeps (base-x, head-x; selection-extra.txt), and round 2's
# (base-s2, head-s2; selection-s2.txt).
# usage: sweep-runs.sh <scratch> <head-rev7> <base-rev7> <head-clone>
set -u
T=$1 head=$2 base=$3 hclone=$4
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 13's sweeps, as sweep.sh printed them; then each sweep's run.json, the banners (banners.py) and the base's rescore (sweep.sh --rescore on a copy, with the head's ledger)."
  for s in base head base-x head-x base-s2 head-s2; do
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
  python3 -I -B "$here/banners.py" "head=$T/sw-head:$head" "base=$T/sw-base:$base" "head-x=$T/sw-head-x:$head" "base-x=$T/sw-base-x:$base" "head-s2=$T/sw-head-s2:$head" "base-s2=$T/sw-base-s2:$base"
  for x in "" -x -s2; do
  echo; echo "## the base$x sweep rescored with the head's ledger"
  cat "$T/rescore$x.run"
  echo "ref-base$x: before $(grep -o 'pass = [0-9]* / [0-9]*' "$T/sw-base$x/scoreboard.md"); after $(grep -o 'pass = [0-9]* / [0-9]*' "$T/ref-base$x/scoreboard.md"); ledger $(grep -o 'ledger sha256 `[0-9a-f]\{8\}' "$T/ref-base$x/scoreboard.md" | tail -c 9) (the head's $(shasum -a 256 "$hclone/scripts/fxcap/parity-ledger.toml" | cut -c1-8))"
  python3 -I -B - "$T/sw-base$x/pairs.json" "$T/ref-base$x/pairs.json" <<'PY2'
import json, sys
a, b = ({(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(f))} for f in sys.argv[1:3])
moved = sorted(k for k in a if a[k] != b.get(k))
print(f"pair by pair: {len(a)} pairs before, {len(b)} after; verdicts moved by the rescore: {len(moved)}" + "".join(f"\n    {k[0]} {k[1]}: {a[k]} -> {b.get(k)}" for k in moved))
PY2
  done
} | python3 -I -B "$here/redacted.py" "$here/sweep-runs.txt" "$T=<scratch>"
