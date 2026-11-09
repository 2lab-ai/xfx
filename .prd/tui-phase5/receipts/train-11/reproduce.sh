#!/usr/bin/env bash
# Train 11: every merge's resolution redone by this folder's scripts in a
# `git clone --no-local`, and compared with the merge commit's tree: each
# merge's first parent checked out, `git merge --no-ff --no-commit` of its
# unit (conflicts written in diff3 style, which blocks.py reads), the
# conflicted files resolved by the scripts named, then `git diff --cached
# --stat <merge>` over the whole tree (empty: the same tree). docs/parity.md
# is put back in git's plain style first (`checkout --conflict=merge`), which
# train 10's rowmerge.py reads.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train11@invalid; git config user.name train11
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
row() { git checkout --conflict=merge docs/parity.md && $P/rowmerge.py --apply "$PWD" "$1" HEAD "$2" docs/parity.md "$3"; }
m1() {
  $P/blocks.py scripts/fxcap/parity-ledger.toml && $P/blocks.py scripts/fxcap/capfile.py && $P/blocks.py tests/tui.rs &&
    $P/blocks.py --append src/tui/event_loop.rs && $P/blocks.py --append src/tui/shell.rs &&
    row 58c92ba2 0c203cd3 '| full-screen TUI |'
}
m2() { row e705a5dc 3ffbe0c7 '| `/model` |' && $P/semantic.py "$PWD"; }
m3() { $P/blocks.py --append src/tui/event_loop.rs && row a832f4d7 5cc2e42c '| full-screen TUI |'; }
one 4a0b900b 0c203cd3 m1
one 0040fa59 3ffbe0c7 m2
one d2ed0274 5cc2e42c m3
cd / && rm -rf "$dir/repro"
