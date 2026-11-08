#!/usr/bin/env bash
# Train 8, join check 2: a probe clone -- a `git clone --no-local` at <rev>,
# the frame probe appended to src/tui/event_loop.rs's `mod tests` and the
# Ctrl-O probe to src/tui/shell.rs's (each before the file's last `}`), and
# nothing else. <variant> is "" (the probes as their units wrote them, for
# GRID-ROW-COST's tip) or "-head" (`start_turn()` for a tree with STOP-NEXT).
# usage: prep.sh <repo> <rev> <variant> <clone>
set -eu
repo=$1 rev=$2 variant=$3 clone=$4
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$clone"
git clone -q --no-local "$repo" "$clone"
git -C "$clone" checkout -q "$rev"
python3 -I -B - "$clone" "$here/frame_cost_probe$variant.rs" "$here/render_cost_probe$variant.rs" <<'PY'
import sys
clone, frame, ctrlo = sys.argv[1:4]
for path, probe, name in ((f"{clone}/src/tui/event_loop.rs", frame, "fn probe_frame_cost_of_long_rows"),
                          (f"{clone}/src/tui/shell.rs", ctrlo, "fn probe_render_cost_of_a_long_argument")):
    text = open(path).read()
    assert text.endswith("}\n") and name not in text
    open(path, "w").write(text[:-2] + "\n" + open(probe).read().rstrip("\n") + "\n}\n")
PY
echo "$(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-60); probes$variant appended"
