#!/usr/bin/env bash
# Join check 2: the probe (render_cost_probe.rs, appended to each clone's
# src/tui/shell.rs tests) on every clone given, both cases, in each mode in
# DETAILS ("none" -- the call's start only, T-U4's probe -- and "1",
# XFX_PROBE_DETAIL=1, the call's arguments too; default both), release, one
# case a process, the clones alternating case by case, ROUNDS rounds
# (default 2). Run the whole of it inside one hold of the suite lock.
# usage: [DETAILS="1"] [ROUNDS=2] suite-lock.sh run.sh <clone>...
set -u
DETAILS=${DETAILS:-"none 1"}
ROUNDS=${ROUNDS:-2}
for clone in "$@"; do
  ( cd "$clone" && cargo test --release --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  echo "# $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-70); build exit $rc"
done
for round in $(seq 1 "$ROUNDS"); do
  for detail in $DETAILS; do
    for case in c1 wider; do
      for clone in "$@"; do
        load=$(sysctl -n vm.loadavg | awk '{print $2}')
        if [ "$detail" = none ]; then
          out=$( cd "$clone" && env XFX_PROBE="$case" cargo test --release --locked --lib -- --ignored --exact tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture 2>&1 )
        else
          out=$( cd "$clone" && env XFX_PROBE="$case" XFX_PROBE_DETAIL=1 cargo test --release --locked --lib -- --ignored --exact tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture 2>&1 )
        fi
        rc=$?
        line=$(printf '%s\n' "$out" | grep '^PROBE ' | head -1)
        echo "round=$round clone=$(basename "$clone") load=$load exit=$rc $line"
      done
    done
  done
done
