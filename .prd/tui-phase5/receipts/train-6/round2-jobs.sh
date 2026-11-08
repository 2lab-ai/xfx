#!/usr/bin/env bash
# Train 6, round 2, one hold of the suite lock (suite-lock.sh round2-jobs.sh
# <scratch> <head worktree>): the probe on four builds (main, CTRLO-WIDER alone,
# T-U4 alone, the train) with the call's arguments; JM8 against round 1's
# caches test and the eight mutants against round 2's; the join tests on each
# unit alone; at the head fmt, clippy both ways, the touched tests, and the
# token check with its negative control. Every exit from $? on the line after
# its command.
set -u
S=$1 H=$2
R=$H/.prd/tui-phase5/receipts/train-6
echo "== the probe"
DETAILS=1 ROUNDS=2 bash "$R/probe/run.sh" "$S/probe-main" "$S/probe-wider" "$S/probe-tu4" "$S/probe-head3"
echo "== JM8 against round 1's caches test (392ca22a's src/tui/shell.rs) on the head's clone"
M=$S/mut-head
( cd "$M" && git checkout -q 392ca22a -- src/tui/shell.rs \
  && sed -i '' 's/        if self.follow_tail || self.bookmark_pending {/        if self.bookmark_pending {/' src/tui/full_transcript/position.rs \
  && git diff --stat | tail -1 \
  && cargo test --locked --lib -- --exact tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width ) > "$S/jm8-r1.log" 2>&1
rc=$?
echo "round 1's caches test with JM8: exit $rc; $(grep -E '^test |^test result' "$S/jm8-r1.log" | tr '\n' ' ')"
git -C "$M" checkout -q HEAD -- src/tui/shell.rs src/tui/full_transcript/position.rs
echo "clone clean after: $([ -z "$(git -C "$M" status --porcelain)" ] && echo yes || echo no)"
echo "== the mutants, round 2's tests"
python3 -I "$R/join_mutants.py" "$M"
echo "== the units"
bash "$R/join_units.sh" "$S/unit-tu4" "$S/unit-wider"
echo "== the head ($(git -C "$H" rev-parse --short HEAD))"
cd "$H" || exit 2
for c in "cargo fmt --check" \
         "cargo clippy --locked --all-targets -- -D warnings" \
         "cargo clippy --locked --all-targets --features fault-injection -- -D warnings"; do
  $c > "$S/r2h.log" 2>&1
  rc=$?
  echo "\$ $c -> exit $rc"
done
cargo test --locked --lib --no-fail-fast -- tui::toolgroup::tests tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width tui::shell::tests::a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back > "$S/r2h-tests.log" 2>&1
rc=$?
echo "\$ cargo test --locked --lib --no-fail-fast -- tui::toolgroup::tests <the two Ctrl-O join tests> -> exit $rc; $(grep '^test result' "$S/r2h-tests.log")"
python3 -I -B "$R/token_check.py" . a09e39c1 7fa4cc27 d55e2317 5f601851 > "$R/token-check.txt" 2>&1
rc=$?
echo "\$ token_check.py . <the four merges> -> exit $rc; $(grep -c '^merge .* -- OK' "$R/token-check.txt") of 4 OK"
: > "$R/token-self-test.txt"
for m in a09e39c1 7fa4cc27 d55e2317 5f601851; do
  python3 -I -B "$R/token_check.py" . --self-test "$m" >> "$R/token-self-test.txt" 2>&1
  rc=$?
  echo "\$ token_check.py . --self-test $m -> exit $rc; $(grep "^self-test ${m:0:8}" "$R/token-self-test.txt")"
done
