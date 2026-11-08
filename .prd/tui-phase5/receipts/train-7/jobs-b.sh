#!/usr/bin/env bash
# Train 7, one hold of the suite lock: join checks 1 and 2's mutants
# (join_mutants.py on a clone of the head), then join check 6 (the render
# cost probe, DETAILS=1, two rounds, the head and fdc5ac0b alternating).
# usage: suite-lock.sh jobs-b.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
echo "# jobs-b at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## the join mutants"
rm -rf "$scratch/mut-clone"
git clone -q --no-local "$repo" "$scratch/mut-clone" && git -C "$scratch/mut-clone" checkout -q "$head"
python3 -I -B "$here/join_mutants.py" "$scratch/mut-clone" "$scratch/join-mutants.txt"
echo "## the probe"
bash "$here/probe/prep.sh" "$repo" "$head" "$here/probe/render_cost_probe-head.rs" "$scratch/probe-head"
bash "$here/probe/prep.sh" "$repo" fdc5ac0b "$here/probe/render_cost_probe.rs" "$scratch/probe-base"
DETAILS=1 ROUNDS=2 bash "$here/probe/run.sh" "$scratch/probe-base" "$scratch/probe-head"
echo "# jobs-b done at $(date -u +%FT%TZ)"
