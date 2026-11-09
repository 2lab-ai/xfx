#!/usr/bin/env bash
# Train 9, one hold of the suite lock, at <head> in a `git clone --no-local`
# of its own: join check 7's named tests (HISTORY-NOTICE-STOP's five and the
# pty pin of a switch's notice said once), with join checks 1, 2, 5 and 6's
# again at the head with all four merges; then join check 4 (net.py: the lib
# and tui tests through a loopback proxy logger). Train 8's jobs-a.sh.
# usage: suite-lock.sh jobs-d.sh <repo> <head> <scratch> <connect_logger.py>
set -u
repo=$1 head=$2 scratch=$3 logger=$4
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
LIB=(
  tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn
  tui::worker::tests::the_history_notice_ledger_goes_with_its_conversation
  tui::shell::tests::a_history_notice_the_screen_takes_is_painted_after_a_stop_and_dropped_only_with_its_conversation
  interactive::tests::the_line_shell_says_each_history_notice_once_and_only_once_it_is_written
  interactive::tests::the_line_shell_leaves_unsaid_only_the_line_it_could_not_write
  agent::machine::tests::a_turn_stopped_before_it_began_is_asked_and_stopped_and_measures_nothing
  tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one
  tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number
  tui::event_loop::tests::a_groups_long_rows_stay_what_their_text_lays_through_scrolls_themes_and_a_resize
  tui::event_loop::tests::a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays
  tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop
  tui::event_loop::tests::a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled
)
TUI=(
  a_switch_off_llmux_sends_its_text_and_calls_and_leaves_the_signed_thinking_behind
  a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt
  a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing
  a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing
  an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too
  twenty_questions_are_one_panel_and_their_answers_reach_the_next_request_in_order
)
echo "# jobs-d at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## join check 7 and the others' named tests at $head"
rm -rf "$scratch/named-clone"
git clone -q --no-local "$repo" "$scratch/named-clone" && git -C "$scratch/named-clone" checkout -q "$head"
echo "# named-clone at $(git -C "$scratch/named-clone" rev-parse --short=8 HEAD)"
( cd "$scratch/named-clone" && cargo test --locked --lib --no-fail-fast -- --exact "${LIB[@]}" ) > "$scratch/named-lib.log" 2>&1
echo "lib: exit $?"
grep -E '^test |^test result' "$scratch/named-lib.log" | sed 's/^/    /'
( cd "$scratch/named-clone" && cargo test --locked --test tui --no-fail-fast -- --exact "${TUI[@]}" ) > "$scratch/named-tui.log" 2>&1
echo "tui: exit $?"
grep -E '^test |^test result' "$scratch/named-tui.log" | sed 's/^/    /'
echo "## join check 4: the network"
rm -rf "$scratch/net-clone"
git clone -q --no-local "$repo" "$scratch/net-clone" && git -C "$scratch/net-clone" checkout -q "$head"
echo "# net-clone at $(git -C "$scratch/net-clone" rev-parse --short=8 HEAD)"
python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
echo "# jobs-d done at $(date -u +%FT%TZ)"
