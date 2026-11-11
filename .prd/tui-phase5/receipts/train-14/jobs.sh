#!/usr/bin/env bash
# Train 14: train 13's jobs.sh, its `net` part alone (this train adds no join
# test, so it has no join mutants): one hold of the suite lock, a
# `git clone --no-local` of <head> (never the worktree), net.py there -- the
# lib, tui and sessions tests through a loopback proxy logger.
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
