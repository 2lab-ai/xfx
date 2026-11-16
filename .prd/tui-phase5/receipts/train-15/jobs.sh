#!/usr/bin/env bash
# Train 15: train 14's jobs.sh with a `cw` part (join check 5) beside its
# `net` part, each run inside one hold of the suite lock in a `git clone
# --no-local` of <head> (never the worktree):
#   cw   the head's two lib test binaries (`cargo test --lib --no-run`,
#        plain and `--features fault-injection`), then cw_runs.py: the
#        catalog-wait tests' module, 10 runs on each
#   net  net.py there -- the lib, tui and sessions tests through a loopback
#        proxy logger (train 14's, unchanged)
# usage: suite-lock.sh jobs.sh <cw|net> <repo> <head> <scratch> [<connect_logger.py>]
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
cw)
  clone cw-clone "$head"
  bin() { # <features...>: the lib test binary cargo builds for them
    ( cd "$scratch/cw-clone" && cargo test --locked --lib --no-run --message-format=json "$@" 2> /dev/null ) \
      | python3 -I -B -c 'import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["kind"] == ["lib"]:
        print(m["executable"])'
  }
  plain=$(bin) fault=$(bin --features fault-injection)
  echo "# built: plain $([ -x "$plain" ] && echo yes || echo no), fault-injection $([ -x "$fault" ] && echo yes || echo no), not the same binary: $([ "$plain" != "$fault" ] && echo yes || echo no)"
  python3 -I -B "$here/cw_runs.py" 10 "$scratch/cw-runs.txt" plain="$plain@$scratch/cw-clone" fault="$fault@$scratch/cw-clone"
  echo "# cw_runs.py exit $?"
  echo "# tree clean after: $([ -z "$(git -C "$scratch/cw-clone" status --porcelain)" ] && echo yes || echo no)"
  ;;
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
