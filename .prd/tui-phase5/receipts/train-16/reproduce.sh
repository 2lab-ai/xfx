#!/usr/bin/env bash
# Train 16 (train 15's reproduce.sh, its merges and rules this train's, and
# train 14's semantic.py back for merge 3): every merge redone by this
# folder's scripts in a `git clone --no-local`, and compared with the merge
# commit's tree: each merge's first parent checked out, `git merge --no-ff
# --no-commit` of its unit (conflicts written in diff3 style, which
# blocks.py and byhand.py read), the conflicted files resolved by the
# scripts named, then `git diff --cached --stat <merge>` over the whole tree
# (empty: the same tree). docs/parity.md is put back in git's plain style
# first (`checkout --conflict=merge`), which rowsmerge.py reads. Merge 3's
# semantic fixes (semantic.py) are applied after its conflicts, as they were
# made. Each resolver's own lines (one per block, row or fix) are printed
# under its merge.
# usage: reproduce.sh <repo> <scratch-dir>
set -u
repo=$1 dir=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$dir/repro"
git clone -q --no-local "$repo" "$dir/repro" || exit 2
cd "$dir/repro" || exit 2
git config user.email train16@invalid; git config user.name train16
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
m1() {
  base=3943d667 && $P/blocks.py scripts/fxcap/parity-ledger.toml && $P/blocks.py scripts/fxcap/scenarios/p5-panels-badsettings.cap \
    && $P/blocks.py tests/sessions.rs && $P/blocks.py tests/tui.rs && rows
}
m2() {
  base=afd891af && $P/byhand.py 2 src/tui/shell.rs && $P/blocks.py scripts/fxcap/parity-ledger.toml && rows
}
m3() {
  $P/byhand.py 3 CONTRIBUTING.md && $P/byhand.py 3 src/tui/worker.rs && $P/byhand.py 3 tests/support/mod.rs \
    && $P/semantic.py 3 "$PWD"
}
m4() { $P/byhand.py 4 scripts/fxcap/roots.py; }
m5() {
  base=df45025c && $P/byhand.py 5 src/tui/approval_amendment.rs && $P/blocks.py scripts/fxcap/parity-ledger.toml && rows
}
m6() {
  base=df45025c && $P/blocks.py scripts/fxcap/parity-ledger.toml \
    && rows --drop '`ask_user_question`=ours@3666-3667' --drop 'full-screen TUI=ours@28340-28341'
}
one b2d044c9 bacb1052 m1
one e8b004a5 32eb858f m2
one 849f5017 7c3f6c1e m3
one dd2ac93d dd9f5f0a m4
one 71787971 ca496c10 m5
one 1bce5348 6d94fbc0 m6
cd / && rm -rf "$dir/repro"
