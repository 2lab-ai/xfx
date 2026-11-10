#!/bin/bash
# What one snap's `publish` costs since LOCAL-PATHS: capture.sh runs roots.py
# once per file a snap writes (.txt and .ansi), before the next step
# (capture.sh:455-469). This times roots.py on a real snap's two files, 20
# times each, with the root arguments a run of p5-mcp-menu-new uses (fake
# roots of the same widths under a scratch directory).
# usage: publish_cost.sh <repo> <a snap's .txt> <scratch dir>
set -u
repo=$1 txt=$2 s=$3/publish-cost
rm -rf "$s" && mkdir -p "$s/run/work" "$s/scen" "$s/home"
ansi=${txt%.txt}.ansi
roots=(--suffix fxcap.AAAAAA --root "$s/run/work" work --root "$s/run" scratch --root "$s/scen" scenarios --root "$s/home" home)
start=$(python3 -c 'import time; print(time.time())')
for i in $(seq 1 20); do
  for f in "$txt" "$ansi"; do
    cp "$f" "$s/f"
    python3 -I "$repo/scripts/fxcap/roots.py" "${roots[@]}" -- "$s/f" > /dev/null || { echo "roots.py exit $?"; exit 1; }
  done
done
end=$(python3 -c 'import time; print(time.time())')
python3 -c "import sys; t=float(sys.argv[2])-float(sys.argv[1]); print('20 snaps x 2 files: %.2f s; per snap %.3f s' % (t, t/20))" "$start" "$end"
