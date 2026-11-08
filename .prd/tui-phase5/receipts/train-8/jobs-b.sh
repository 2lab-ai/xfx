#!/usr/bin/env bash
# Train 8, one hold of the suite lock: join check 1's mutants
# (join_mutants.py) on a `git clone --no-local` of <head>, the clone clean
# before and after.
# usage: suite-lock.sh jobs-b.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
echo "# jobs-b at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
rm -rf "$scratch/mut-clone"
git clone -q --no-local "$repo" "$scratch/mut-clone" && git -C "$scratch/mut-clone" checkout -q "$head"
echo "# mut-clone at $(git -C "$scratch/mut-clone" rev-parse --short=8 HEAD)"
python3 -I -B "$here/join_mutants.py" "$scratch/mut-clone"
echo "# jobs-b done at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
