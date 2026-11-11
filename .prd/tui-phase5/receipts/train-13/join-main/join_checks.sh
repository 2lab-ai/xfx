#!/usr/bin/env bash
# The join's own checks, at the code head: (1) what names XFX_BUILD_REVISION
# outside .prd, on each parent and at the head; (2) whether every file p5-mcp-fail's
# list snap is timed by is the same at the head as on each parent and the
# base, and what the train changed under src/mcp and in the MCP menu; (3) the
# `#[test]` lines on the base, each parent and the head.
# usage: join_checks.sh <repo> <train> <main> <head>
set -u
repo=$1 train=$2 main=$3 head=$4
base=$(git -C "$repo" merge-base "$train" "$main")
echo "## 1. XFX_BUILD_REVISION outside .prd (git grep -n)"
for c in "$train" "$main" "$head"; do
  echo "# at $(git -C "$repo" rev-parse --short=8 "$c"): $(git -C "$repo" grep -n XFX_BUILD_REVISION "$c" -- ':!.prd' | wc -l | tr -d ' ') line(s)"
  git -C "$repo" grep -n XFX_BUILD_REVISION "$c" -- ':!.prd' | cut -c1-160 | sed 's/^/    /'
done
echo "# XFX_BUILD_ID outside .prd at the head: $(git -C "$repo" grep -n XFX_BUILD_ID "$head" -- ':!.prd' | wc -l | tr -d ' ') line(s)"
echo
echo "## 2. p5-mcp-fail's list snap: the files that time it, the head against each side"
for f in scripts/fxcap/scenarios/p5-mcp-fail.cap scripts/fxcap/scenarios/p5-mcp-fail-profile.json scripts/fxcap/scenarios/p5-mcp-stdio.ts \
         scripts/fxcap/capture.sh scripts/fxcap/fakegw.ts src/mcp/runtime.rs src/tui/mcp_menu.rs; do
  printf '%s:' "$f"
  for side in main:"$main" train:"$train" base:"$base"; do
    n=${side%%:*} c=${side#*:}
    git -C "$repo" diff --quiet "$c" "$head" -- "$f" && printf ' %s same' "$n" || printf ' %s DIFFERENT' "$n"
  done
  echo
done
echo "# the train's changes (base -> train) under src/mcp, src/tui/mcp_*.rs and the fxcap harness's scripts: $(git -C "$repo" diff --name-only "$base" "$train" -- src/mcp 'src/tui/mcp_*.rs' scripts/fxcap/capture.sh scripts/fxcap/fakegw.ts scripts/fxcap/sweep.sh | wc -l | tr -d ' ') file(s)"
echo
echo "## 3. #[test] lines in tracked .rs files (git grep -F), for the gate's counts against train 13's"
for side in base:"$base" train:"$train" main:"$main" head:"$head"; do
  n=${side%%:*} c=${side#*:}
  echo "# $n $(git -C "$repo" rev-parse --short=8 "$c"): $(git -C "$repo" grep -h -F '#[test]' "$c" -- '*.rs' | wc -l | tr -d ' ')"
done
