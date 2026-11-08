#!/usr/bin/env bash
# The train head's three join tests, their text unchanged, on each unit's own
# tip (a `git clone --no-local` with the tests appended): which pass alone.
# usage: suite-lock.sh join_units.sh <clone>...
set -u
TESTS=(tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width
       tui::shell::tests::a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back
       tui::toolgroup::tests::the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width)
for clone in "$@"; do
  echo "## $(basename "$clone"): $(git -C "$clone" log --oneline -1 | cut -c1-90), the tests appended (git diff --stat: $(git -C "$clone" diff --shortstat))"
  ( cd "$clone" && cargo test --locked --lib --no-fail-fast -- --exact "${TESTS[@]}" ) > "$clone.log" 2>&1
  rc=$?
  grep -E '^test |^test result|^error(\[|:)|panicked at' "$clone.log" | head -40
  for t in "${TESTS[@]}"; do
    grep -A12 -- "---- $t stdout ----" "$clone.log" | grep -v '^$' | head -12
  done
  echo "exit $rc"
done
