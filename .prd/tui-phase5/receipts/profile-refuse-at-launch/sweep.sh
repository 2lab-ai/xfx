#!/usr/bin/env bash
# The unit's sweep: every scenario named p5-panels*, settings*, startup* or
# config*, and every scenario that seeds a profile before launch (seed_scan.py),
# with HEAD's scenarios and ledger, once with the head's xfx and once with the
# base's, fx v0.0.16 both times; then compare.py over the two pairs.json.
# Run inside one hold of the suite lock: `suite-lock.sh sweep.sh ...`.
#
# usage: sweep.sh <repo> <fx v0.0.16> <head xfx> <base xfx> <out dir> <jobs>
set -u
repo=$1 fx=$2 head=$3 base=$4 out=$5 jobs=${6:-2}
pin=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
cd "$repo" || exit 2
only=(--only 'p5-panels*' --only 'settings*' --only 'startup*' --only 'config*')
while read -r name; do only+=(--only "$name"); done < <(
  python3 -I "$here/seed_scan.py" scripts/fxcap/scenarios | sed -n 's/^\([^ =][^:]*\): .*/\1/p')
echo "fx sha256 before: $(shasum -a 256 "$fx" | cut -c1-12)"
echo "selection: $(( ${#only[@]} / 2 )) globs"
for side in head base; do
  bin=$head; [ "$side" = base ] && bin=$base
  start=$SECONDS
  FX_AUTO_UPGRADE=0 bash scripts/fxcap/sweep.sh --fx "$fx" --pin-sha "$pin" --xfx "$bin" \
    --jobs "$jobs" "${only[@]}" --out "$out/$side" > "$out/$side.log" 2>&1
  echo "$side: exit $? in $((SECONDS - start)) s; $(grep -m1 -E 'pass|Scenarios' "$out/$side/summary.md" 2>/dev/null)"
done
echo "fx sha256 after: $(shasum -a 256 "$fx" | cut -c1-12)"
python3 -I "$here/compare.py" "$out/base/pairs.json" "$out/head/pairs.json"
