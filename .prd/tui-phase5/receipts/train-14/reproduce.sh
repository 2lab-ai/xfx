#!/usr/bin/env bash
# Train 14 (train 13's reproduce.sh, its merges and rules this train's):
# every merge redone by this folder's scripts in a `git clone --no-local`,
# and compared with the merge commit's tree: each merge's first parent checked
# out, `git merge --no-ff --no-commit` of its unit (conflicts written in diff3
# style, which blocks.py, byhand.py and eventloop.py read), the conflicted
# files resolved by the scripts named, the merge's semantic fixes applied
# (semantic.py), then `git diff --cached --stat <merge>` over the whole tree
# (empty: the same tree). docs/parity.md is put back in git's plain style
# first (`checkout --conflict=merge`), which rowsmerge.py reads. Each
# resolver's own lines (one per block or row) are printed under its merge.
# The join of main (583cc13a) is the fifth, redone the same way.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train14@invalid; git config user.name train14
P="python3 -I -B $here"
one() { # <merge> <unit> <resolve function>
  local m=$1 u=$2 f=$3
  git checkout -q --detach "$m^1"
  git -c merge.conflictStyle=diff3 merge --no-ff --no-commit "$u" > /dev/null 2>&1
  local said
  said=$("$f" 2>&1)
  local rc=$?
  git add -A
  local stat
  stat=$(git diff --cached --stat "$m" | tail -1)
  echo "$(git rev-parse --short=8 "$m") (${u:0:8}): resolve exit $rc; against the merge commit's tree: ${stat:-no difference}"
  printf '%s\n' "$said" | grep -v '^Recreated' | sed "s#$PWD/##; s/^/    /"
  git merge --abort 2>/dev/null || git reset -q --hard
}
rows() { git checkout --conflict=merge docs/parity.md && $P/rowsmerge.py "$@" "$PWD" "$base" HEAD MERGE_HEAD docs/parity.md; }
m1() { base=af2b6137 && rows && $P/eventloop.py src/tui/event_loop.rs && $P/blocks.py scripts/fxcap/parity-ledger.toml; }
m2() { base=cb7c9c18 && rows && $P/blocks.py scripts/fxcap/parity-ledger.toml && $P/blocks.py --append src/tui/event_loop.rs && $P/semantic.py 2 "$PWD"; }
m3() {
  base=3d3d56c8 && $P/byhand.py src/tui/shell.rs && $P/byhand.py scripts/fxcap/parity-ledger.toml \
    && $P/blocks.py scripts/fxcap/parity-ledger.toml && $P/blocks.py scripts/fxcap/capfile.py \
    && rows --drop '`/settings`=ours@221-221' && $P/semantic.py 3 "$PWD"
}
m4() { $P/blocks.py scripts/fxcap/parity-ledger.toml && $P/semantic.py 4 "$PWD"; }
one d548c078 4130f4ac m1
one ecad8518 e7e1bcf6 m2
one 7a1773d9 3ea16930 m3
one fb6dfdb8 bde1ddfa m4
# The join of main (PR #94, WORKSPACE-INDEX-AT-PLAN), added with it: main
# 3570a182 into the train at 08066ac9; one conflicting file, the ledger.
j1() { $P/blocks.py scripts/fxcap/parity-ledger.toml; }
one 583cc13a 3570a182 j1
cd / && rm -rf "$dir/repro"
