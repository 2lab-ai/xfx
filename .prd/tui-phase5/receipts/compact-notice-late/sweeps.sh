#!/bin/bash
# The compaction scenarios on fx v0.0.16 and on xfx, base then head.
set -u
export TMPDIR=<tmp>
cd <repo>
for side in base head; do
  echo "## $side $(date -u +%FT%TZ)"
  scripts/fxcap/sweep.sh --fx <fx-bin> --pin-sha $(cat <scratch>/fx.sha) \
    --xfx <tmp>/bin-$side/xfx --jobs 2 --only 'compact*' --only 'p5-lifecycle-*compact*' \
    --out <scratch>/sweep-$side 2>&1 | tail -15
  echo "exit ${PIPESTATUS[0]}"
done
