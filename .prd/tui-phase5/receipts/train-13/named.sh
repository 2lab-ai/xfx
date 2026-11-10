#!/usr/bin/env bash
# Train 13, the join checks' named tests: each test the README names for a
# join check, run by name at <head> in a `git clone --no-local` (filters are
# cargo's substrings; each line of the output is cargo's own `test ... ok`).
# Single tests and a handful of named ones: no lock (the P5 common brief's
# "Shared machine"); a pty test that fails is run alone three times more
# before it is counted (none was needed if no FAILED line follows).
# usage: named.sh <repo> <head> <scratch>
set -u
repo=$1 head=$2 dir=$3
rm -rf "$dir/named-clone"
git clone -q --no-local "$repo" "$dir/named-clone" && git -C "$dir/named-clone" checkout -q --detach "$head"
cd "$dir/named-clone" || exit 2
echo "# named tests at $(git rev-parse --short=8 HEAD), load $(sysctl -n vm.loadavg | awk '{print $2}')"
run() { # <title> <cargo args...> -- <filters...>
  local title=$1; shift
  echo "## $title"
  cargo test --locked "$@" > "$dir/named.log" 2>&1
  echo "exit $?"
  grep -E '^test .* \.\.\. |^test result' "$dir/named.log" | grep -v ' 0 passed; 0 failed' | sed 's/^/    /'
}
# join check 1, the HISTORY-NOTICES-COPY-FX half: the tests it flipped (no line about a dropped state)
run "HISTORY-NOTICES-COPY-FX, lib (carry_tests)" --lib -- a_switched_session_goes_to_the_gateway_without_the_thinking_llmux_sealed a_model_change_on_llmux_sends_the_steps_another_model_sealed_as_text_and_calls
run "HISTORY-NOTICES-COPY-FX, tests/sessions.rs" --test sessions -- a_legacy_record_is_dropped_without_a_word_on_any_other_wire a_wire_less_responses_state_record_drops_rather_than_replaying a_wire_this_version_does_not_know_drops_rather_than_guessing responses_state_recorded_under_one_authority_is_never_sent_to_another ask_resume_never_adopts_a_model_with_a_control_character_or_prints_one
run "HISTORY-NOTICES-COPY-FX, tests/tui.rs (pty)" --test tui -- a_switch_off_llmux_sends_its_text_and_calls_and_leaves_the_signed_thinking after_a_model_change_on_llmux_no_block_the_earlier_model_signed_is_sent
# join check 1, the QUESTION-RESUME half: the decoder, the hydration and the shell's rows
run "QUESTION-RESUME, lib" --lib -- a_hostile_answer_document_paints_no_sequence_of_its_own a_resumed_answered_batch_of_one_four_or_twenty_replays_its_resolutions a_resumed_batch_that_left_no_answered_document_is_one_asked_row a_resumed_batch_the_stop_cut_off_is_the_turns_cancellation_line_alone a_resumed_step_that_asked_between_two_calls_settles_each_in_a_group_of_its_own a_resumed_step_with_its_ask_last_or_first_keeps_the_reads_beside_it_in_one_group a_saved_answer_document_reads_back_in_order_with_its_escapes a_saved_document_of_one_to_twenty_questions_decodes_and_no_other_count_does a_skipped_value_is_not_checked_as_upstreams_parse_checks_it a_step_that_asked_starts_each_of_its_calls_just_before_its_result an_ask_replays_its_decoded_batch_or_one_asked_row_and_never_a_tool_row anything_but_an_answer_document_is_none_and_never_a_panic the_twenty_first_element_is_refused_at_its_first_byte_unread a_hostile_log_reaches_the_ui_inert_at_launch_and_from_the_picker
# join check 2: QUESTION-WORD-LEFT's tests and APPROVAL-VERTICAL-SPELLINGS' decoder and fold tests
run "QUESTION-WORD-LEFT and APPROVAL-VERTICAL-SPELLINGS, lib" --lib -- alt_b_is_its_own_action_and_the_modified_left_arrows_are_word_left alt_b_goes_back_a_question_at_a_choice_and_a_word_in_a_draft alt_b_at_a_choice_goes_back_a_question_and_the_word_arrows_do_not alt_b_in_an_approval_amendment_is_the_word_move the_fold_maps_the_question_only_keys_to_the_plain_ones_and_leaves_the_rest every_modifier_on_the_vertical_arrows_is_kept_apart_from_the_arrow
# the two join tests
run "train 13's join tests, lib" --lib -- a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning a_question_after_a_resume_that_replayed_a_batch_takes_alt_b_as_before
echo "# the clone clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
