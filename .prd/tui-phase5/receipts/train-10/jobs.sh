#!/usr/bin/env bash
# Train 10, one hold of the suite lock per call, each part in a `git clone
# --no-local` of its own (never the worktree):
#   named   -- at <head>: the four join tests and the units' tests on each
#              meeting, by name (lib, then the pty tests);
#   alone   -- each join test on the sides it joins, alone: the unit's tip (or
#              main) with only that test put in (apply_join.py's way), its
#              verdict and its first failing line;
#   mutants -- join_mutants.py at <head>;
#   net     -- join check 5: net.py (the lib and tui tests through a loopback
#              proxy logger) at <head>.
# Train 9's jobs-a.sh / jobs-d.sh, in parts.
# usage: suite-lock.sh jobs.sh <part> <repo> <head> <scratch> [<connect_logger.py>]
set -u
part=$1 repo=$2 head=$3 scratch=$4 logger=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
clone() { # <name> <commit>
  rm -rf "$scratch/$1"
  git clone -q --no-local "$repo" "$scratch/$1" && git -C "$scratch/$1" checkout -q --detach "$2"
  echo "# $1 at $(git -C "$scratch/$1" rev-parse --short=8 HEAD)"
}
LIB=(
  # the join tests
  tui::shell::tests::a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id
  tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs
  tui::event_loop::tests::a_paste_into_the_at_word_with_an_enter_behind_its_marker_is_kept_trusted_and_sends_nothing
  tui::shell::tests::a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter
  # join check 1's hops: CATALOG-BOUNDS' categories, the selector keeping rows, MODEL-FUZZY's ranks and mark
  provider::model::tests::a_gateway_catalog_one_byte_past_16_mib_fails_and_is_not_cut_to_fit
  provider::model::tests::a_gateway_catalog_past_its_deadline_is_a_failure_asked_again_about
  tui::model_menu::tests::better_matches_list_first_and_the_mark_keeps_its_model_through_a_revalidation
  tui::shell::tests::a_catalog_that_arrives_or_fails_is_what_the_open_menu_shows
  # join check 2: HISTORY-NOTICE-STOP's and COMPACT-NOTICE-LATE's own
  tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn
  tui::worker::tests::the_history_notice_ledger_goes_with_its_conversation
  tui::shell::tests::a_history_notice_the_screen_takes_is_painted_after_a_stop_and_dropped_only_with_its_conversation
  tui::worker::tests::a_compaction_a_stop_drops_before_it_began_is_told_how_it_ended
  tui::worker::tests::a_compaction_dropped_with_no_room_for_its_word_is_told_once_before_the_reset_after_it
  tui::worker::tests::a_compaction_dropped_in_a_turns_race_with_no_room_is_told_after_that_turn_and_before_more_work
  tui::event_loop::tests::a_stopped_compactions_sentence_the_next_line_took_away_stays_away
  tui::event_loop::tests::a_line_sent_while_a_compaction_stops_takes_the_sentence_only_of_one_that_never_began
  tui::event_loop::tests::a_compact_typed_while_a_reset_drains_answers_as_upstreams_would
  # join check 3: AT-HEIGHT's gate, PASTE-AFTER-END's paste, the question's dismissal
  tui::event_loop::tests::a_new_at_words_arrows_wait_for_the_frame_that_shows_its_rows
  tui::event_loop::tests::a_key_spent_mid_resize_asks_for_the_frame_that_shows_the_list_again
  tui::shell::tests::a_paste_with_input_behind_its_end_marker_reaches_the_answer_and_answers_nothing
  tui::event_loop::tests::a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled
  tui::shell::tests::a_question_dismisses_the_file_list
  tui::shell::tests::a_question_dismisses_the_menu_and_leaves_the_approval_panel_alone
)
TUI=(
  a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing
  an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too
  a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty_is_kept_and_answers_nothing
  twenty_questions_are_one_panel_and_their_answers_reach_the_next_request_in_order
  ctrl_c_stops_a_compaction_that_is_waiting_for_its_notes
)
echo "# jobs.sh $part at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
named)
  clone named-clone "$head"
  ( cd "$scratch/named-clone" && cargo test --locked --lib --no-fail-fast -- --exact "${LIB[@]}" ) > "$scratch/named-lib.log" 2>&1
  echo "lib: exit $?"
  grep -E '^test |^test result' "$scratch/named-lib.log" | sed 's/^/    /'
  ( cd "$scratch/named-clone" && cargo test --locked --test tui --no-fail-fast -- --exact "${TUI[@]}" ) > "$scratch/named-tui.log" 2>&1
  echo "tui: exit $?"
  grep -E '^test |^test result' "$scratch/named-tui.log" | sed 's/^/    /'
  ;;
alone)
  clone alone-clone "$head"
  c="$scratch/alone-clone"
  one() { # <side> <commit> <test file> <target> <test path>
    git -C "$c" checkout -q --detach "$2" && git -C "$c" clean -qfd
    python3 -I -B - "$c" "$here/$3" "$4" <<'PY'
import sys
clone, test, target = sys.argv[1:4]
path = f"{clone}/{target}"
text = open(path, encoding="utf-8").read()
assert text.endswith("    }\n}\n")
open(path, "w", encoding="utf-8").write(text[:-2] + open(test, encoding="utf-8").read() + "}\n")
PY
    ( cd "$c" && cargo test --locked --lib --no-fail-fast -- --exact "$5" ) > "$scratch/alone-$1-$3.log" 2>&1
    local rc=$?
    local verdict
    verdict=$(grep -E "^test $5 \.\.\. " "$scratch/alone-$1-$3.log" | sed 's/.* \.\.\. //')
    local why
    why=$(grep -E "panicked at|^error(\[|:)" "$scratch/alone-$1-$3.log" | head -1)
    local said
    said=$(grep -A3 'panicked at' "$scratch/alone-$1-$3.log" | sed -n 2,4p | tr '\n' ' ' | cut -c1-240)
    echo "$1 $(git -C "$c" rev-parse --short=8 HEAD), $3: exit $rc, ${verdict:-no verdict}; ${why:0:160} ${said}"
    git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
  }
  one catalog-bounds e705a5dc join_test_catalog.rs src/tui/shell.rs tui::shell::tests::a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id
  one model-fuzzy 8d9ab0a0 join_test_catalog.rs src/tui/shell.rs tui::shell::tests::a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id
  one main 6bb9fd90 join_test_worker.rs src/tui/worker.rs tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs
  one at-height af2b6137 join_test_paste.rs src/tui/event_loop.rs tui::event_loop::tests::a_paste_into_the_at_word_with_an_enter_behind_its_marker_is_kept_trusted_and_sends_nothing
  one at-height af2b6137 join_test_question.rs src/tui/shell.rs tui::shell::tests::a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter
  one main 6bb9fd90 join_test_question.rs src/tui/shell.rs tui::shell::tests::a_twenty_question_panel_over_an_open_at_list_has_its_tab_arrows_and_enter
  echo "# alone-clone clean after: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
  ;;
mutants)
  clone mutants-clone "$head"
  python3 -I -B "$here/join_mutants.py" "$scratch/mutants-clone"
  ;;
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
