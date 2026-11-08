#!/usr/bin/env bash
# GRID-ROW-COST, after review: the frame probe (frame_cost_probe.rs, appended
# by apply_probe.py) with a theme report before the events timed
# (XFX_PROBE_THEME=1), heavy with 1 and 3 long calls, on every clone given --
# the base, the head the first measurement ran at (01149542) and the head
# with the re-laying fix (0bddf3d8) -- and, without the theme report, every
# long-row case of run.sh's FRAME on the last clone given (the final code).
# Release, one case a process, the clones alternating case by case, ROUNDS
# rounds (default 2), the 1-minute load on every line. One hold of the suite
# lock.
#
# usage: [ROUNDS=2] suite-lock.sh run-theme.sh <clone>...
set -u
ROUNDS=${ROUNDS:-2}
CLONE_LIST="$*"
LAST=${!#}
for clone in $CLONE_LIST; do
  ( cd "$clone" && cargo test --release --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  echo "# $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-70); build exit $rc"
done
probe() {  # <clone> <env>...
  local clone=$1
  shift
  ( cd "$clone" && env "$@" cargo test --release --locked --lib -- --ignored --exact \
      tui::event_loop::tests::probe_frame_cost_of_long_rows --nocapture 2>&1 )
}
for round in $(seq 1 "$ROUNDS"); do
  for rows in 1 3; do
    for clone in $CLONE_LIST; do
      load=$(sysctl -n vm.loadavg | awk '{print $2}')
      out=$(probe "$clone" XFX_PROBE=heavy XFX_PROBE_ROWS="$rows" XFX_PROBE_THEME=1)
      rc=$?
      echo "FRAME round=$round clone=$(basename "$clone") load=$load exit=$rc $(printf '%s\n' "$out" | grep '^FRAMEPROBE ' | head -1)"
    done
  done
  for spec in heavy:1 heavy:3 zw:1 zw:3; do
    case=${spec%%:*} rows=${spec##*:}
    load=$(sysctl -n vm.loadavg | awk '{print $2}')
    out=$(probe "$LAST" XFX_PROBE="$case" XFX_PROBE_ROWS="$rows")
    rc=$?
    echo "FRAME round=$round clone=$(basename "$LAST") load=$load exit=$rc $(printf '%s\n' "$out" | grep '^FRAMEPROBE ' | head -1)"
  done
done
