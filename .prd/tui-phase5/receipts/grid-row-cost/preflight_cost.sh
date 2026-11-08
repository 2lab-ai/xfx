#!/usr/bin/env bash
# The coordinator's question on PR #82's x86_64-apple-darwin job: what
# `scripts/check-tui-preflight-cost.sh` costs at main, the base and the head,
# and whether this unit shrinks it. Each clone given is a clean
# `git clone --no-local` (no probe appended) with its own, empty target
# directory; the script is run there twice, one after the other: cold (its two
# release `cargo test --lib` builds from nothing, as CI's runner, which keeps
# no cache, builds them) and warm (the same command again: the two test
# binaries already built, so what is left is the two tests). Wall seconds,
# exit, and the script's own measurement lines. The script and its bar are
# each tree's own and unchanged. Run the whole of it inside one hold of the
# suite lock.
#
# usage: suite-lock.sh preflight_cost.sh <clone>...
set -u
for clone in "$@"; do
  for run in cold warm; do
    load=$(sysctl -n vm.loadavg | awk '{print $2}')
    start=$SECONDS
    out=$( cd "$clone" && ./scripts/check-tui-preflight-cost.sh 2>&1 )
    rc=$?
    echo "# $(basename "$clone") $(git -C "$clone" rev-parse --short HEAD) $run: exit $rc, $((SECONDS - start)) s, load $load"
    printf '%s\n' "$out" | grep -E '^(preflight|replay) |check-tui-preflight-cost: ok' | sed 's/^/    /'
  done
done
