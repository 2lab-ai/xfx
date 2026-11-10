#!/usr/bin/env bash
# Train 12, round 2: what check-no-local-paths.sh does with one of the 62
# captures of the head's sweep that keep a cut-short scenario directory
# (capture-scan.txt): p5-mcp-menu-new's server-proj snap, fx's, copied under
# its capture name into a throwaway `git clone --no-local` at <head> and
# staged (`git add`: the check reads the index), then the check run there,
# its exit from `$?` on the next line. The check prints a hit's file, line
# and shape, never its text. The clone is removed after.
# usage: mcp_capture_check.sh <repo> <head> <sweep-out> <scratch>
set -u
repo=$1 head=$2 out=$3 scratch=$4
name=fx-p5-mcp-menu-new-40x120-server-proj.txt
c=$scratch/mcp-check
rm -rf "$c"
git clone -q --no-local "$repo" "$c" && git -C "$c" checkout -q --detach "$head" || exit 2
mkdir -p "$c/.prd/tui-phase5/receipts/mcp-check"
cp "$out/runs/p5-mcp-menu-new/fx/$name" "$c/.prd/tui-phase5/receipts/mcp-check/$name"
git -C "$c" add .prd/tui-phase5/receipts/mcp-check
echo "# check-no-local-paths.sh in a clone at $(git -C "$c" rev-parse --short=8 HEAD), with $name staged under .prd/tui-phase5/receipts/mcp-check/"
( cd "$c" && ./scripts/check-no-local-paths.sh ) 2>&1
echo "# exit $?"
rm -rf "$c"
