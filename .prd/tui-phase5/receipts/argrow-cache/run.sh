#!/usr/bin/env bash
# ARGROW-CACHE: the probe (render_cost_probe.rs, appended to each clone's
# src/tui/shell.rs tests by apply_probe.py) on every clone given -- train 6's
# probe/run.sh, with the short case and the key mode added: release, one case
# a process, the clones alternating case by case, ROUNDS rounds (default 2),
# in each DETAILS mode ("none" -- the call's start only, T-U4's probe -- and
# "1", the call's arguments too) and each KEYS mode ("" and "1",
# XFX_PROBE_KEY=1). Run the whole of it inside one hold of the suite lock.
# usage: [DETAILS="none 1"] [KEYS=" 1"] [ROUNDS=2] suite-lock.sh run.sh <clone>...
set -u
DETAILS=${DETAILS:-"none 1"}
ROUNDS=${ROUNDS:-2}
CASES=${CASES:-"c1 wider short"}
for clone in "$@"; do
  ( cd "$clone" && cargo test --release --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  echo "# $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-70); build exit $rc"
done
for round in $(seq 1 "$ROUNDS"); do
  for key in "" 1; do
    for detail in $DETAILS; do
      [ "$detail" = none ] && [ -n "$key" ] && continue
      for case in $CASES; do
        for clone in "$@"; do
          load=$(sysctl -n vm.loadavg | awk '{print $2}')
          out=$( cd "$clone" && env XFX_PROBE="$case" ${key:+XFX_PROBE_KEY=1} \
            $( [ "$detail" = none ] || echo XFX_PROBE_DETAIL=1 ) \
            cargo test --release --locked --lib -- --ignored --exact \
            tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture 2>&1 )
          rc=$?
          line=$(printf '%s\n' "$out" | grep '^PROBE ' | head -1)
          line3=$(printf '%s\n' "$out" | grep '^PROBE3 ' | head -1)
          echo "round=$round clone=$(basename "$clone") load=$load exit=$rc $line"
          echo "round=$round clone=$(basename "$clone") load=$load exit=$rc $line3"
        done
      done
    done
  done
done
