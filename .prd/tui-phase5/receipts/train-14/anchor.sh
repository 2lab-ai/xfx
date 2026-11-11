#!/usr/bin/env bash
# Train 14 (new): before the code head is fixed, whether two ledger causes
# the merges touched still cover anything: `at-ranking-anchor` (PAGE-KEYS-
# SETTINGS-COLUMN's, `p5-page-keys-anchor` / `file-one`, owner I-U2, which
# AT-RANK closed) and the row `at-plan-run-directory` covered (`workspace-index`
# / `at-plan` row 13, the cause deleted in 8f622949). The commit <rev> is
# cloned and built (debug), and each scenario swept once with that tree's
# harness and ledger (sweep-rerun.sh, n = 1). Run it inside one hold of the
# suite lock.
# usage: FX=<fx v0.0.16> SHORT_TMP=<a short scratch dir> anchor.sh <repo> <rev> <dir>
set -u
repo=$1 rev=$2 dir=$3
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/anchor" && git clone -q --no-local "$repo" "$dir/anchor" && git -C "$dir/anchor" checkout -q --detach "$rev" || exit 2
( cd "$dir/anchor" && cargo build --locked ) > "$dir/anchor.build.log" 2>&1
echo "# build at $(git -C "$dir/anchor" rev-parse --short=8 HEAD): exit $?"
for s in p5-page-keys-anchor workspace-index; do
  bash "$here/sweep-rerun.sh" "anchor-$s" "$dir/anchor" "$s" 1 "$dir/anchor-$s"
done
