#!/usr/bin/env bash
# Join check 2: the probe (render_cost_probe.rs) on two clones, T-U4's tip and
# the train head, both cases, with and without the call's arguments: release,
# one case a process, the whole run in one hold of the suite lock.
# usage: suite-lock.sh run.sh <clone-tu4> <clone-head>
set -u
for clone in "$@"; do
  ( cd "$clone" && cargo test --release --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  echo "# $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-70); build exit $rc"
done
for round in 1 2; do
  for detail in "" 1; do
    for case in c1 wider; do
      for clone in "$@"; do
        load=$(sysctl -n vm.loadavg | awk '{print $2}')
        out=$( cd "$clone" && env XFX_PROBE="$case" ${detail:+XFX_PROBE_DETAIL=1} cargo test --release --locked --lib -- --ignored --exact tui::shell::tests::probe_render_cost_of_a_long_argument --nocapture 2>&1 )
        rc=$?
        line=$(printf '%s\n' "$out" | grep '^PROBE ' | head -1)
        echo "round=$round clone=$(basename "$clone") load=$load exit=$rc $line"
      done
    done
  done
done
