#!/usr/bin/env bash
# Train 11: sweep-runs.txt, through redacted.py -- each sweep's own lines as
# sweep.sh printed them (each inside one hold of the suite lock, FX the
# v0.0.16 binary, SHORT_TMP a short scratch directory), the last lines of its
# log, its run.json's start, wall time, load and the xfx it ran, the banners
# its xfx snaps show (banners.py), and each reference sweep's rescore with the
# head's ledger (sweep.sh --rescore on a copy): the pass line before and after.
# usage: sweep-runs.sh <scratch> <head-rev7> <main-rev7>
set -u
T=$1 head=$2 main=$3
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 11's sweeps, as sweep.sh printed them; then each sweep's run.json, the banners (banners.py) and the rescores of the reference sweeps (sweep.sh --rescore on a copy, with the head's ledger)."
  for s in main head; do
    echo; echo "## $s"
    grep -v '^exit' "$T/sweep-$s.run"
    python3 -I -B - "$T/sw-$s/run.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
keys = [k for k in ("started", "wall", "start_load", "peak_load", "jobs", "size", "xfx_sha256", "fx_sha256", "repo_head") if k in d]
print("run.json: " + "; ".join(f"{k} {d[k]}" for k in keys))
PY
  done
  echo; echo "## banners"
  python3 -I -B "$here/banners.py" "head=$T/sw-head:$head" "main=$T/sw-main:$main"
  echo; echo "## rescores with the head's ledger"
  repo=$(cd "$here/../../../.." && pwd)
  for r in main scp cr pg; do
    case $r in
      main) before=$(grep -o 'pass = [0-9]* / [0-9]*' "$T/sw-main/scoreboard.md") ;;
      scp) before="$(git -C "$repo" show 0c203cd3:.prd/tui-phase5/receipts/submit-card-preview/sweep-head.md | grep -o 'pass = [0-9]* / [0-9]*') (its committed sweep-head.md)" ;;
      cr) before="$(git -C "$repo" show 3ffbe0c7:.prd/tui-phase5/receipts/catalog-refresh/sweep-head.md | grep -o 'pass = [0-9]* / [0-9]*') (its committed sweep-head.md)" ;;
      pg) before="$(git -C "$repo" show 5cc2e42c:.prd/tui-phase5/receipts/paste-gap/sweep-head.md | grep -o 'pass = [0-9]* / [0-9]*') (its committed sweep-head.md)" ;;
    esac
    echo "ref-$r: before $before; after $(grep -o 'pass = [0-9]* / [0-9]*' "$T/ref-$r/scoreboard.md"); ledger $(grep -o 'ledger sha256 `[0-9a-f]\{8\}' "$T/ref-$r/scoreboard.md" | tail -c 9); xfx $(grep -o 'xfx: .*sha256 `[0-9a-f]\{8\}' "$T/ref-$r/scoreboard.md" | grep -o '[0-9a-f]\{8\}$'); run at $(python3 -I -c "import json,sys; print(json.load(open(sys.argv[1]))['repo_head'][:8])" "$T/ref-$r/run.json")"
  done
} | python3 -I -B "$here/redacted.py" "$here/sweep-runs.txt" "$T=<scratch>"
