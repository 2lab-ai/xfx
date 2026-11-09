#!/usr/bin/env bash
# Train 10: every merge's resolution redone by this folder's scripts in a
# `git clone --no-local`, and compared with the merge commit's tree: each
# merge's first parent checked out, `git merge --no-ff --no-commit` of its
# unit, the conflicted files resolved by the script named, then `git diff
# --stat <merge>` over the whole tree (empty: the same tree). append_both.py
# was rewritten after merge 3 was committed (to keep git's merge outside the
# span for merge 4); this shows the rewritten script gives merge 3's tree too.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train10@invalid; git config user.name train10
one() { # <merge> <unit> <resolve...>
  local m=$1 u=$2; shift 2
  git checkout -q --detach "$m^1"
  git merge --no-ff --no-commit "$u" > /dev/null 2>&1
  "$@" > /dev/null 2>&1
  local rc=$?
  git add -A
  local stat
  stat=$(git diff --cached --stat "$m" | tail -1)
  echo "$(git rev-parse --short=8 "$m") (${u:0:8}): resolve exit $rc; against the merge commit's tree: ${stat:-no difference}"
  git merge --abort 2>/dev/null || git reset -q --hard
}
P="python3 -I -B $here"
one 13e7c9a0 e705a5dc $P/rowmerge.py --apply "$PWD" ccced1f5 HEAD e705a5dc docs/parity.md '| `/model` |'
one 33e476fa 8d9ab0a0 $P/rowmerge.py --apply "$PWD" ccced1f5 HEAD 8d9ab0a0 docs/parity.md '| `/model` |'
one 5a98001a 96cc7849 $P/append_both.py "$PWD" 58c92ba2 HEAD 96cc7849 src/tui/event_loop.rs 15841
both() { $P/append_both.py "$PWD" 712dd8d1 HEAD af2b6137 src/tui/event_loop.rs 15841 && $P/numbers_only.py scripts/fxcap/parity-ledger.toml; }
one 64873172 af2b6137 both
cd / && rm -rf "$dir/repro"
