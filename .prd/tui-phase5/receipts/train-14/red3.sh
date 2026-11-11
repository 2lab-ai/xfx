#!/usr/bin/env bash
# Train 14 (new): why merge 3 needs its behaviour fix (semantic.py 3). In a
# `git clone --no-local`, merge 3 is redone as reproduce.sh redoes it but
# with semantic.py's build fixes alone (`--only build`), and three tests run
# by exact name: CTRLO-AT-KEYS' two C-j/C-k-under-Ctrl-O cases over the `@`
# list and PAGE-KEYS-SETTINGS-COLUMN's own check of the same keys over every
# other list. Then the behaviour fix is applied and the three run again; then
# the same three at the train's commit <head>. Run it inside one hold of the
# suite lock. Each run's `test` lines, panics and `test result` line are
# kept; nothing else of cargo's output.
# usage: red3.sh <repo> <scratch-dir> <head>
set -u
repo=$1 dir=$2 head=$3
here=$(cd "$(dirname "$0")" && pwd)
P="python3 -I -B $here"
TESTS="tui::shell::tests::c_j_under_the_full_transcript_is_a_new_line_and_closes_the_list tui::shell::tests::c_k_under_the_full_transcript_kills_to_the_lines_end_and_keeps_the_list tui::shell::tests::under_the_full_transcript_no_other_list_takes_a_row_key"
rm -rf "$dir/red3"
git clone -q --no-local "$repo" "$dir/red3" || exit 2
cd "$dir/red3" || exit 2
git config user.email train14@invalid; git config user.name train14
run() { # <label>
  echo "## $1 ($(git rev-parse --short=8 HEAD) + the working tree)"
  # shellcheck disable=SC2086
  cargo test --locked --lib -- --exact $TESTS > "$dir/red3.$2.log" 2>&1
  echo "cargo test exit $?"
  grep -E '^test |^test result|panicked at|^  left:|^ right:|^thread ' "$dir/red3.$2.log" | grep -v '^test result: ok. 0 passed'
}
git checkout -q --detach 7a1773d9^1
git -c merge.conflictStyle=diff3 merge --no-ff --no-commit 3ea16930 > /dev/null 2>&1
$P/byhand.py src/tui/shell.rs > /dev/null && $P/byhand.py scripts/fxcap/parity-ledger.toml > /dev/null \
  && $P/blocks.py scripts/fxcap/parity-ledger.toml > /dev/null && $P/blocks.py scripts/fxcap/capfile.py > /dev/null \
  && git checkout --conflict=merge docs/parity.md \
  && $P/rowsmerge.py --drop '`/settings`=ours@221-221' "$PWD" 3d3d56c8 HEAD MERGE_HEAD docs/parity.md 2> /dev/null \
  && $P/semantic.py --only build 3 "$PWD" || exit 3
run "merge 3 with its build fixes alone" build
$P/semantic.py --only behaviour 3 "$PWD" || exit 3
git add -A
echo "## the tree now against merge 3's commit: $(git diff --cached --stat 7a1773d9 | tail -1)"
run "merge 3 with its behaviour fix too" fixed
git merge --abort 2> /dev/null || git reset -q --hard
git checkout -q --detach "$head"
run "the train at $head" head
cd / && rm -rf "$dir/red3"
