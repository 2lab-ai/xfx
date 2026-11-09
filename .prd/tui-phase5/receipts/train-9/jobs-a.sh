#!/usr/bin/env bash
# Train 9, one hold of the suite lock, each at <head> in a `git clone
# --no-local` of its own: join check 1's named tests (STOP-IN-CATALOG-WAIT's
# catalog-wait tests, train 7's join tests, GRID-ROW-COST's per-tick session
# test, train 8's join test), join check 2's and join check 5's with the
# units' own, and join check 6's (PASTE-AFTER-END's carry-over test and its
# two pty tests); join checks 2 and 5 on each side alone (join_units.sh,
# join_units_paste.sh). Train 8's jobs-a.sh; the network (join check 4) is
# jobs-c.sh's, at the final head.
# usage: suite-lock.sh jobs-a.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
LIB=(
  agent::machine::tests::a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing
  agent::machine::tests::a_compaction_the_first_request_needed_that_failed_leaves_no_turn_and_a_stopped_one_does
  tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one
  tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number
  tui::event_loop::tests::a_catalog_answer_read_in_a_stops_window_reaches_the_band_and_leaves_the_stop_as_it_was
  tui::shell::tests::catalog_asks_around_a_stop_leave_its_count_and_the_next_start_its_own
  tui::event_loop::tests::a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize
  tui::event_loop::tests::a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays
  tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop
  tui::event_loop::tests::a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled
  tui::shell::tests::a_paste_with_input_behind_its_end_marker_reaches_the_answer_and_answers_nothing
)
TUI=(
  a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt
  twenty_questions_are_one_panel_and_their_answers_reach_the_next_request_in_order
  six_questions_open_the_panel_and_twenty_one_or_none_are_refused_in_upstreams_words
  the_request_after_a_cancelled_question_answers_it_aborted_as_upstreams_does
  a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing
  a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing
  an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too
)
echo "# jobs-a at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## join checks 1, 2, 5 and 6: the named tests at $head"
rm -rf "$scratch/named-clone"
git clone -q --no-local "$repo" "$scratch/named-clone" && git -C "$scratch/named-clone" checkout -q "$head"
echo "# named-clone at $(git -C "$scratch/named-clone" rev-parse --short=8 HEAD)"
( cd "$scratch/named-clone" && cargo test --locked --lib --no-fail-fast -- --exact "${LIB[@]}" ) > "$scratch/named-lib.log" 2>&1
echo "lib: exit $?"
grep -E '^test |^test result' "$scratch/named-lib.log" | sed 's/^/    /'
( cd "$scratch/named-clone" && cargo test --locked --test tui --no-fail-fast -- --exact "${TUI[@]}" ) > "$scratch/named-tui.log" 2>&1
echo "tui: exit $?"
grep -E '^test |^test result' "$scratch/named-tui.log" | sed 's/^/    /'
echo "## join check 2 on each side alone"
bash "$here/join_units.sh" "$repo" "$head" "$scratch"
echo "## join check 5 on each side alone"
bash "$here/join_units_paste.sh" "$repo" "$scratch"
echo "# jobs-a done at $(date -u +%FT%TZ)"
