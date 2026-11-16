#!/usr/bin/env bash
# Train 15 (new): the movers' reruns as one job in one hold of the suite lock
# -- a `git clone --no-local` of the head and of the base, each built (debug,
# as the sweeps were), then reruns.sh: each scenario swept 3 times on each
# tree with that tree's own harness, scenarios and ledger. (The sweeps'
# clones were lost to the machine's /tmp cleanup before the first reruns
# finished; this builds them again from the same commits.)
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> \
#        suite-lock.sh rerun-job.sh <repo> <head> <base> <out-dir> <scenario>...
set -u
repo=$1 head=$2 base=$3 out=$4; shift 4
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
for t in head:$head main:$base; do
  n=${t%%:*} c=${t#*:}
  rm -rf "$out/$n"
  git clone -q --no-local "$repo" "$out/$n" && git -C "$out/$n" checkout -q --detach "$c" || exit 2
  ( cd "$out/$n" && cargo build --locked ) > "$out/$n.build.log" 2>&1
  echo "# $n: build at $(git -C "$out/$n" rev-parse --short=8 HEAD), exit $?; xfx sha256 $(shasum -a 256 "$out/$n/target/debug/xfx" | cut -c1-64)"
done
bash "$here/reruns.sh" "$out/head" "$out/main" "$out/rr" "$@"
echo "# reruns.sh exit $?"
