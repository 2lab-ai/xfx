#!/usr/bin/env bash
# Train 6, merge 5 (ARGROW-CACHE), one hold of the suite lock:
#   suite-lock.sh merge5-jobs.sh <head worktree> <scratch> <clone>...
# the probe (probe/run.sh, DETAILS=1, two rounds) on the clones given, then at
# the head: fmt, clippy both ways, the Ctrl-O join tests by name, the lib's
# full_transcript and toolgroup tests, and the tui test binary. Every exit is
# taken from $? on the line after its command.
set -u
H=$1 S=$2; shift 2
R=$H/.prd/tui-phase5/receipts/train-6
echo "== the probe"
DETAILS=1 ROUNDS=2 bash "$R/probe/run.sh" "$@"
echo "== the head ($(git -C "$H" rev-parse --short HEAD)); tree clean: $([ -z "$(git -C "$H" status --porcelain)" ] && echo yes || echo no)"
cd "$H" || exit 2
for c in "cargo fmt --check" \
         "cargo clippy --locked --all-targets -- -D warnings" \
         "cargo clippy --locked --all-targets --features fault-injection -- -D warnings"; do
  $c > "$S/m5.log" 2>&1
  rc=$?
  echo "\$ $c -> exit $rc"
done
NAMED=(
  tui::shell::tests::ctrl_o_with_arguments_through_its_caches_is_the_session_laid_out_from_nothing_after_every_event
  tui::full_transcript::tests::ctrl_o_wraps_a_calls_arguments_once_for_a_width_and_not_again_for_a_join_or_a_theme
  tui::full_transcript::tests::rows_kept_unpainted_are_the_rows_railed_painted_at_once
  tui::full_transcript::tests::ctrl_o_builds_a_groups_block_once_and_lays_each_heading_out_once_for_what_it_shows
  tui::full_transcript::tests::a_value_the_heading_had_to_spell_is_never_shown_by_it_already
  tui::full_transcript::tests::a_calls_heading_is_spelled_as_its_transcript_row_is_at_any_width
  tui::full_transcript::tests::a_long_heading_wraps_under_its_rail_instead_of_being_cut
  tui::full_transcript::tests::a_heading_is_cut_only_where_fxs_own_bound_on_its_phrase_cuts_it
  tui::full_transcript::tests::arguments_beside_a_heading_wider_than_the_screen_keep_their_rows
  tui::shell::tests::ctrl_o_names_what_a_terminal_draws_wider_and_its_caches_follow_the_palette_and_the_width
  tui::shell::tests::a_theme_report_while_ctrl_o_is_open_recolours_a_named_label_and_a_cancelled_line_and_back
  tui::toolgroup::tests::the_transcripts_row_keeps_what_a_terminal_draws_wider_whole_at_every_width
  tui::shell::tests::a_code_panels_label_names_what_a_terminal_draws_wider
  tui::shell::tests::a_turn_stopped_over_its_answer_leaves_its_cancelled_line_on_ctrl_o_too
  tui::shell::tests::a_theme_report_recolours_ctrl_os_stopped_turn_line_and_refused_batch_row
)
cargo test --locked --lib --no-fail-fast -- --exact "${NAMED[@]}" > "$S/m5-named.log" 2>&1
rc=$?
echo "\$ cargo test --locked --lib --no-fail-fast -- --exact <${#NAMED[@]} named tests> -> exit $rc; $(grep '^test result' "$S/m5-named.log")"
grep -E '^test ' "$S/m5-named.log" | sort
cargo test --locked --lib --no-fail-fast -- tui::full_transcript tui::toolgroup tui::wrap > "$S/m5-ft.log" 2>&1
rc=$?
echo "\$ cargo test --locked --lib --no-fail-fast -- tui::full_transcript tui::toolgroup tui::wrap -> exit $rc; $(grep '^test result' "$S/m5-ft.log")"
cargo test --locked --test tui --no-fail-fast > "$S/m5-tui.log" 2>&1
rc=$?
echo "\$ cargo test --locked --test tui --no-fail-fast -> exit $rc; $(grep '^test result' "$S/m5-tui.log")"
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
