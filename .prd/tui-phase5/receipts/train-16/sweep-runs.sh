#!/usr/bin/env bash
# Train 16: train 15's sweep-runs.sh, its two sweeps this train's (the head
# 2b78aab6 and the base 866747ff, side by side in two slots of the lock);
# its header line says train 16. Nothing else changed.
# Train 15 (train 14's sweep-runs.sh, its sweeps this train's: one sweep a
# tree over one selection, selection-sweep.txt -- train 14's two selections
# in one file -- the head 2c575f52 and the base, main e2fc7e94):
# sweep-runs.txt, through redacted.py -- each sweep's own lines as sweep.sh
# printed them (each inside one hold of the suite lock, FX the v0.0.16
# binary, SHORT_TMP a short scratch directory), its run.json's start, wall
# time, load and the xfx it ran, the banners its xfx snaps show
# (banners.py), and the base sweep rescored with the head's ledger
# (rescore.sh: sweep.sh --rescore on a copy, ref-base): the pass line
# before and after, and the verdicts the rescore moved, pair by pair; then
# the product version each tree builds (its Cargo.toml).
# usage: sweep-runs.sh <scratch> <head-rev7> <base-rev7> <head-clone>
set -u
T=$1 head=$2 base=$3 hclone=$4
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 16's sweeps, as sweep.sh printed them; then each sweep's run.json, the banners (banners.py) and the base's rescore (sweep.sh --rescore on a copy, with the head's ledger)."
  for s in base head; do
    echo; echo "## $s"
    cat "$T/sw-$s.run"
    python3 -I -B - "$T/sw-$s/run.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
keys = [k for k in ("started", "wall", "start_load", "peak_load", "jobs", "size", "xfx_sha256", "fx_sha256", "repo_head") if k in d]
print("run.json: " + "; ".join(f"{k} {d[k]}" for k in keys))
PY
  done
  echo; echo "## banners"
  python3 -I -B "$here/banners.py" "head=$T/sw-head:$head" "base=$T/sw-base:$base"
  echo; echo "## the base sweep rescored with the head's ledger"
  cat "$T/ref-base.rescore.run"
  echo "ref-base: before $(grep -o 'pass = [0-9]* / [0-9]*' "$T/sw-base/scoreboard.md"); after $(grep -o 'pass = [0-9]* / [0-9]*' "$T/ref-base/scoreboard.md"); ledger $(grep -o 'ledger sha256 `[0-9a-f]\{8\}' "$T/ref-base/scoreboard.md" | tail -c 9) (the head's $(shasum -a 256 "$hclone/scripts/fxcap/parity-ledger.toml" | cut -c1-8))"
  python3 -I -B - "$T/sw-base/pairs.json" "$T/ref-base/pairs.json" <<'PY2'
import json, sys
a, b = ({(p["scenario"], p["label"]): p["verdict"] for p in json.load(open(f))} for f in sys.argv[1:3])
moved = sorted(k for k in a if a[k] != b.get(k))
print(f"pair by pair: {len(a)} pairs before, {len(b)} after; verdicts moved by the rescore: {len(moved)}" + "".join(f"\n    {k[0]} {k[1]}: {a[k]} -> {b.get(k)}" for k in moved))
PY2
  echo; echo "## the product version each tree builds (Cargo.toml)"
  for c in "$base" "$head"; do
    echo "$c $(git -C "$hclone" show "$c:Cargo.toml" | grep -m1 '^version')"
  done
} | python3 -I -B "$here/redacted.py" "$here/sweep-runs.txt" "$T=<scratch>"
