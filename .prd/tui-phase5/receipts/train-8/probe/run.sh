#!/usr/bin/env bash
# Train 8, join check 2: GRID-ROW-COST's run.sh with its FRAME and CTRLO
# probes (its MEM probe left out), unchanged otherwise, on every clone given
# (prep.sh's: main, GRID-ROW-COST alone, the head), release, one case a
# process, the clones alternating case by case, ROUNDS rounds (default 2).
# Run the whole of it inside one hold of the suite lock.
#
#   FRAME   the whole frame path (frame_cost_probe.rs): heavy and zw with 1 and
#           3 long calls, 20 reads (40 events) each; short, c1 and wider with 1.
#   CTRLO   train 6's Ctrl-O probe as ARGROW-CACHE extended it
#           (render_cost_probe.rs, XFX_PROBE_DETAIL=1): c1, wider.
#
# usage: [ROUNDS=2] suite-lock.sh run.sh <clone>...
set -u
ROUNDS=${ROUNDS:-2}
CLONE_LIST="$*"
for clone in $CLONE_LIST; do
  ( cd "$clone" && cargo test --release --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  echo "# $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-70); build exit $rc"
done
probe() {  # <clone> <test> <env>...
  local clone=$1 test=$2
  shift 2
  ( cd "$clone" && env "$@" cargo test --release --locked --lib -- --ignored --exact "$test" --nocapture 2>&1 )
}
for round in $(seq 1 "$ROUNDS"); do
  for spec in heavy:1 heavy:3 zw:1 zw:3 short:1 c1:1 wider:1; do
    case=${spec%%:*} rows=${spec##*:}
    for clone in $CLONE_LIST; do
      load=$(sysctl -n vm.loadavg | awk '{print $2}')
      out=$(probe "$clone" tui::event_loop::tests::probe_frame_cost_of_long_rows XFX_PROBE="$case" XFX_PROBE_ROWS="$rows")
      rc=$?
      echo "FRAME round=$round clone=$(basename "$clone") load=$load exit=$rc $(printf '%s\n' "$out" | grep '^FRAMEPROBE ' | head -1)"
    done
  done
  for case in c1 wider; do
    for clone in $CLONE_LIST; do
      load=$(sysctl -n vm.loadavg | awk '{print $2}')
      out=$(probe "$clone" tui::shell::tests::probe_render_cost_of_a_long_argument XFX_PROBE="$case" XFX_PROBE_DETAIL=1)
      rc=$?
      echo "CTRLO round=$round clone=$(basename "$clone") load=$load exit=$rc $(printf '%s\n' "$out" | grep '^PROBE3 ' | head -1)"
    done
  done
done
