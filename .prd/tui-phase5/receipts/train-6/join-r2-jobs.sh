#!/usr/bin/env bash
# After 392ca22a (the row-cut join test's modifier case): the mutants again on
# the head's clone, the three join tests on each unit's tip again, and at the
# head clippy both ways and the three join tests -- one hold of the lock.
set -u
S=$1 H=$2
python3 -I "$S/join_mutants.py" "$S/mut-head"
echo "== the units"
bash "$S/join_units.sh" "$S/unit-tu4" "$S/unit-wider"
echo "== the head ($(git -C "$H" rev-parse --short HEAD))"
( cd "$H" && cargo clippy --locked --all-targets -- -D warnings ) > "$S/r2-clippy.log" 2>&1
rc=$?; echo "clippy exit $rc"
( cd "$H" && cargo clippy --locked --all-targets --features fault-injection -- -D warnings ) > "$S/r2-clippy-fault.log" 2>&1
rc=$?; echo "clippy-fault exit $rc"
( cd "$H" && cargo fmt --check ) > "$S/r2-fmt.log" 2>&1
rc=$?; echo "fmt exit $rc"
( cd "$H" && cargo test --locked --lib --no-fail-fast -- tui::toolgroup::tests tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width tui::shell::tests::a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back ) > "$S/r2-tests.log" 2>&1
rc=$?; echo "tests exit $rc: $(grep '^test result' "$S/r2-tests.log")"
