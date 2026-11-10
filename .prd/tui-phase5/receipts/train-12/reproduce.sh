#!/usr/bin/env bash
# Train 12 (train 11's reproduce.sh, its merges and rules this train's):
# every merge redone by this folder's scripts in a `git clone --no-local`,
# and compared with the merge commit's tree: each merge's first parent checked
# out, `git merge --no-ff --no-commit` of its unit (conflicts written in diff3
# style, which blocks.py reads), the conflicted files resolved by the scripts
# named, then `git diff --cached --stat <merge>` over the whole tree (empty:
# the same tree). docs/parity.md is put back in git's plain style first
# (`checkout --conflict=merge`), which rowsmerge.py reads.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train12@invalid; git config user.name train12
P="python3 -I -B $here"
one() { # <merge> <unit> <resolve function>
  local m=$1 u=$2 f=$3
  git checkout -q --detach "$m^1"
  git -c merge.conflictStyle=diff3 merge --no-ff --no-commit "$u" > /dev/null 2>&1
  "$f" > /dev/null 2>&1
  local rc=$?
  git add -A
  local stat
  stat=$(git diff --cached --stat "$m" | tail -1)
  echo "$(git rev-parse --short=8 "$m") (${u:0:8}): resolve exit $rc; against the merge commit's tree: ${stat:-no difference}"
  git merge --abort 2>/dev/null || git reset -q --hard
}
none() { true; }
m3() { $P/blocks.py scripts/fxcap/parity-ledger.toml; }
m4() {
  $P/blocks.py scripts/fxcap/parity-ledger.toml &&
    git checkout --conflict=merge docs/parity.md && $P/rowsmerge.py "$PWD" 32afd307 HEAD MERGE_HEAD docs/parity.md &&
    $P/semantic.py "$PWD"
}
one d43c8c92 a1cbdcaf none
one 2c640bac 4bc9ab13 none
one 55d993a7 5609108f m3
one 12b56ab8 3d3d56c8 m4
cd / && rm -rf "$dir/repro"
