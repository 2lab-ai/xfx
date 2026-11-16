#!/usr/bin/env bash
# Train 16: train 15's jobs.sh with its `cw` part taken out (CATALOG-WAIT-CLOCK
# was train 15's; cw_runs.py is not copied) -- `net` alone; net.py has a
# second part this train, its own head says.
# Train 15: train 14's jobs.sh with a `cw` part (join check 5) beside its
# `net` part, each run inside one hold of the suite lock in a `git clone
# --no-local` of <head> (never the worktree):
#   cw   the head's two lib test binaries (`cargo test --lib --no-run`,
#        plain and `--features fault-injection`), then cw_runs.py: the
#        catalog-wait tests' module, 10 runs on each
#   net  net.py there -- the lib, tui and sessions tests through a loopback
#        proxy logger (train 14's, unchanged)
# usage: suite-lock.sh jobs.sh net <repo> <head> <scratch> <connect_logger.py>
set -u
part=$1 repo=$2 head=$3 scratch=$4 logger=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
clone() { # <name> <commit>
  rm -rf "$scratch/$1"
  git clone -q --no-local "$repo" "$scratch/$1" && git -C "$scratch/$1" checkout -q --detach "$2"
  echo "# $1 at $(git -C "$scratch/$1" rev-parse --short=8 HEAD)"
}
echo "# jobs.sh $part at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
