#!/usr/bin/env bash
# Train 15 (train 14's reproduce.sh, its merges and rules this train's; no
# merge here needed a semantic fix, so semantic.py is not used): every merge
# redone by this folder's scripts in a `git clone --no-local`, and compared
# with the merge commit's tree: each merge's first parent checked out, `git
# merge --no-ff --no-commit` of its unit (conflicts written in diff3 style,
# which blocks.py and byhand.py read), the conflicted files resolved by the
# scripts named, then `git diff --cached --stat <merge>` over the whole tree
# (empty: the same tree). docs/parity.md is put back in git's plain style
# first (`checkout --conflict=merge`), which rowsmerge.py reads. Each
# resolver's own lines (one per block or row) are printed under its merge.
# The join of main (a14df710) is the sixth, redone the same way.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train15@invalid; git config user.name train15
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
m1() { $P/byhand.py 1 src/tui/worker.rs; }
m2() {
  base=afd891af && rows --meet 'full transcript (`C-o`)@4894-4895' \
    && $P/byhand.py 2 src/tui/shell.rs && $P/byhand.py 2 scripts/fxcap/parity-ledger.toml \
    && $P/blocks.py scripts/fxcap/parity-ledger.toml
}
m3() { base=3f2d6e78 && $P/byhand.py 3 scripts/fxcap/parity-ledger.toml && $P/blocks.py scripts/fxcap/parity-ledger.toml && rows; }
m4() {
  base=a37a969e && $P/byhand.py 4 src/tui/approval.rs && $P/byhand.py 4 src/tui/shell.rs \
    && $P/blocks.py tests/tui.rs && $P/blocks.py scripts/fxcap/parity-ledger.toml && rows
}
m5() {
  base=afd891af && $P/byhand.py 5 src/tui/input.rs && $P/blocks.py scripts/fxcap/parity-ledger.toml \
    && rows --inside '`ask_user_question`@3298-3299'
}
one 2ff302c5 72fc9672 m1
one ed92e515 df45025c m2
one b10ca771 fffb3cfe m3
one 88e83a68 7c9dfb2e m4
one 07eefa89 b5cb6f46 m5
# The join of main (#95, #96, #97): main e2fc7e94 into the train at
# bc9ea199; one conflicting file, docs/parity.md.
j1() { base=753b0a72 && rows; }
one a14df710 e2fc7e94 j1
cd / && rm -rf "$dir/repro"
