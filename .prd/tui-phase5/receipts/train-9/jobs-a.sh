#!/usr/bin/env bash
# Train 9, one hold of the suite lock, each at <head> in a `git clone
# --no-local` of its own: join check 1's named tests (STOP-IN-CATALOG-WAIT's
# catalog-wait tests, train 7's join tests, GRID-ROW-COST's per-tick session
# test, train 8's join test) and join check 2's with the units' own; join
# check 2's tests on each side alone (join_units.sh); join check 4 (net.py:
# the lib and tui tests through a loopback proxy logger). Train 8's jobs-a.sh.
# usage: suite-lock.sh jobs-a.sh <repo> <head> <scratch> <connect_logger.py>
set -u
repo=$1 head=$2 scratch=$3 logger=$4
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
)
TUI=(
  a_stop_at_a_twenty_question_panel_is_one_cancelled_row_and_the_next_turn_carries_its_prompt
  twenty_questions_are_one_panel_and_their_answers_reach_the_next_request_in_order
  six_questions_open_the_panel_and_twenty_one_or_none_are_refused_in_upstreams_words
  the_request_after_a_cancelled_question_answers_it_aborted_as_upstreams_does
)
echo "# jobs-a at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
echo "## join checks 1 and 2: the named tests at $head"
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
echo "## join check 4: the network"
rm -rf "$scratch/net-clone"
git clone -q --no-local "$repo" "$scratch/net-clone" && git -C "$scratch/net-clone" checkout -q "$head"
echo "# net-clone at $(git -C "$scratch/net-clone" rev-parse --short=8 HEAD)"
python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
echo "# jobs-a done at $(date -u +%FT%TZ)"
