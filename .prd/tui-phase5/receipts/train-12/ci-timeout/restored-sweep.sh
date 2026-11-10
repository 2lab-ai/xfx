#!/bin/bash
# The restored launch-only p5-panels-badsettings, swept the way ci_sweep.py
# sweeps it: HEAD's scenarios and ledger, once with the head's xfx and once
# with the base's (df46777e), fx v0.0.16 both times, at the runner-length
# TMPDIR; then each side's verdict for `launch` and ci_sweep.py's regression
# rule (`ci_sweep.py:427`: the base passes the pair and the head does not).
# usage: restored-sweep.sh <repo> <fx v0.0.16> <head xfx> <base xfx> <runner-length tmpdir> <--tmp dir> <out>
set -u
repo=$1 fx=$2 head=$3 base=$4 long=$5 rt=$6 out=$7
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
echo "fx sha256 before: $(shasum -a 256 "$fx" | cut -d' ' -f1)"
echo "head xfx: $("$head" --version 2>&1 | head -1); base xfx: $("$base" --version 2>&1 | head -1)"
mkdir -p "$rt"
for side in head base; do
  bin=$head; [ "$side" = base ] && bin=$base
  echo "== sweep.sh --only p5-panels-badsettings, the $side's xfx"
  ( cd "$repo" && TMPDIR=$long FX_AUTO_UPGRADE=0 bash scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$bin" \
      --jobs 2 --tmp "$rt" --only p5-panels-badsettings --out "$out/$side" 2>&1 )
  echo "exit=$?"
done
echo "fx sha256 after:  $(shasum -a 256 "$fx" | cut -d' ' -f1)"
python3 -I - "$out" <<'PY'
import json, os, sys
out = sys.argv[1]
PASS = ("identical", "normalized-identical")
v = {}
for side in ("head", "base"):
    pairs = json.load(open(os.path.join(out, side, "pairs.json")))
    for p in pairs:
        if p["scenario"] == "p5-panels-badsettings":
            print("%s: %s %s, causes %s, rows no cause covers %d"
                  % (side, p["label"], p["verdict"], p.get("causes", "-"), len(p.get("unattributed") or [])))
            v[(side, p["label"])] = p["verdict"]
h, b = v.get(("head", "launch")), v.get(("base", "launch"))
print("regression (head not passing, base passing): %s" % (h not in PASS and b in PASS))
PY
