#!/usr/bin/env bash
# Train 8, one hold of the suite lock: join check 1's test on each side alone
# (join_units.sh), join check 4 (net.py: the lib and tui tests through a
# loopback proxy logger), join check 3 (test-capture five times) -- each at
# <head>, in a `git clone --no-local` of its own. train 7's jobs-a.sh.
# usage: suite-lock.sh jobs-a.sh <repo> <head> <scratch> <connect_logger.py>
set -u
repo=$1 head=$2 scratch=$3 logger=$4
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
echo "# jobs-a at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## join check 1 on each side alone"
bash "$here/join_units.sh" "$repo" "$scratch"
echo "## join check 4: the network"
rm -rf "$scratch/net-clone"
git clone -q --no-local "$repo" "$scratch/net-clone" && git -C "$scratch/net-clone" checkout -q "$head"
echo "# net-clone at $(git -C "$scratch/net-clone" rev-parse --short=8 HEAD)"
python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
echo "## join check 3: test-capture x5, at $head"
rm -rf "$scratch/cap-clone"
git clone -q --no-local "$repo" "$scratch/cap-clone" && git -C "$scratch/cap-clone" checkout -q "$head"
for run in 1 2 3 4 5; do
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  start=$SECONDS
  bash "$scratch/cap-clone/scripts/fxcap/test-capture.sh" > "$scratch/cap-$run.log" 2>&1
  rc=$?
  echo "test-capture run $run: exit $rc, $((SECONDS - start)) s, load $load; $(grep -E '^(Ran|OK|FAILED)' "$scratch/cap-$run.log" | tr '\n' ' ')"
  grep -E '^(FAIL|ERROR): ' "$scratch/cap-$run.log" | sed 's/^/    /'
done
echo "# jobs-a done at $(date -u +%FT%TZ)"
