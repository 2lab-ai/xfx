#!/usr/bin/env bash
# Train 12: reruns.txt, through redacted.py -- each reruns.sh call's own
# lines (sweep-rerun.sh's: the build, fx's sha256 before and after, the pass
# line), each rerun's verdicts pair by pair (rerun_verdicts.py, with
# model-column-idle's screen states), and the catalog asks of
# model-column-idle in both sweeps and each rerun (asks.py).
# usage: reruns_txt.sh <scratch>   (its reruns-1.out, rr1, reruns-2.out, rr2)
set -u
T=$1
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# Train 12's reruns (reruns.sh, one hold of the suite lock per call, sweep-rerun.sh 3 times a scenario on each tree's own debug build)"
  echo; echo "## call 1: model-column-idle, head and main (the brief's question: did SWEEP-WALLCLOCK-F end catalog-window)"
  cat "$T/reruns-1.out"
  python3 -I -B "$here/rerun_verdicts.py" "$T/rr1" head-model-column-idle main-model-column-idle
  echo "# and in the two sweeps, read as one run each"
  python3 -I -B "$here/rerun_verdicts.py" "$T" "sweep-head-model-column-idle=$T/sw-head"
  python3 -I -B "$here/rerun_verdicts.py" "$T" "sweep-main-model-column-idle=$T/sw-main"
  echo "asks, the head's sweep: $(python3 -I -B "$here/asks.py" "$T/sw-head" model-column-idle | awk '{print $2, $3}' | tr '\n' ' ')"
  echo "asks, main's sweep: $(python3 -I -B "$here/asks.py" "$T/sw-main" model-column-idle | awk '{print $2, $3}' | tr '\n' ' ')"
  for side in head main; do
    for i in 1 2 3; do
      echo "asks, $side rerun $i: $(python3 -I -B "$here/asks.py" "$T/rr1/$side-model-column-idle-$i" model-column-idle | awk '{print $2, $3}' | tr '\n' ' ')"
    done
  done
  echo; echo "## call 2: the sweeps' movers against main (p5-mcp-menu-new, p5-lifecycle-retryok, workspace-index) and the scenario that failed on both (p5-panels-badsettings), head and main"
  cat "$T/reruns-2.out"
  echo "# p5-mcp-menu-new in the two sweeps, read as one run each"
  python3 -I -B "$here/rerun_verdicts.py" "$T" "sweep-head-p5-mcp-menu-new=$T/sw-head" "sweep-main-p5-mcp-menu-new=$T/sw-main"
  python3 -I -B "$here/rerun_verdicts.py" "$T/rr2" head-p5-mcp-menu-new main-p5-mcp-menu-new head-p5-lifecycle-retryok main-p5-lifecycle-retryok \
    head-workspace-index main-workspace-index head-p5-panels-badsettings main-p5-panels-badsettings
} | python3 -I -B "$here/redacted.py" "$here/reruns.txt" "$T=<scratch>"
