#!/usr/bin/env bash
# LOCAL-PATHS-WRAP: fix_tails.py over <tree>, with mcp-menu's remote_only.py
# (its pair verdicts for p5-mcp-menu-new at 40x60, the size it touches)
# before and after: its summary line only, the rows it prints holding paths.
#   fix-tails.sh <tree>     (once: it rewrites committed captures)
set -u
set -o pipefail
tree=$1
m=$tree/.prd/tui-phase5/receipts/mcp-menu
cap=$tree/scripts/fxcap/scenarios/p5-mcp-menu-new.cap
verdict() { python3 -I "$m/remote_only.py" "$m/fx" "$m/xfx-head" p5-mcp-menu-new 40x60 "$cap" | tail -1; }
echo "before: $(verdict)"
python3 -I "$(dirname "$0")/fix_tails.py" "$tree" || exit 1
echo "after:  $(verdict)"
