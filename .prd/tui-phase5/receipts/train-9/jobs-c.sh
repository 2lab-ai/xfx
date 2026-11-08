#!/usr/bin/env bash
# Train 9, one hold of the suite lock: join check 4 (net.py: the lib and tui
# tests through a loopback proxy logger) at <head>, in a `git clone
# --no-local` of its own. Train 8's jobs-a.sh's network part.
# usage: suite-lock.sh jobs-c.sh <repo> <head> <scratch> <connect_logger.py>
set -u
repo=$1 head=$2 scratch=$3 logger=$4
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
echo "# jobs-c at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## join check 4: the network"
rm -rf "$scratch/net-clone"
git clone -q --no-local "$repo" "$scratch/net-clone" && git -C "$scratch/net-clone" checkout -q "$head"
echo "# net-clone at $(git -C "$scratch/net-clone" rev-parse --short=8 HEAD)"
python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
echo "# jobs-c done at $(date -u +%FT%TZ)"
