#!/bin/bash
# Runs each test that asserts a short literal is absent from runtime output
# with TMPDIR named after that literal, against test binaries built with
# XFX_BUILD_REVISION=1049997a7b7c (a revision spelling 1049, 997 and 7).
# A test whose output embeds a temporary path or the revision fails here.
# Usage: sweep.sh <target/debug/deps>
set -u
deps=$1
# The library's unit tests, not the binary's: the one that lists this test.
for lib in $(ls "$deps"/xfx-* | grep -E '/xfx-[0-9a-f]+$'); do
  "$lib" --list 2>/dev/null | grep -q '^output::tests::the_panels_session' && break
done
bin() { ls "$deps/$1"-* | grep -E "/$1-[0-9a-f]+$" | head -1; }
# Each directory is under the caller's TMPDIR, which must hold none of the
# literals itself (the receipt's runs used one that holds none).
run() { # literal-suffix binary test...
  local suffix=$1 binary=$2 dir="${TMPDIR:-/tmp}/sw-$1"; shift 2
  mkdir -p "$dir"
  for t in "$@"; do
    r=$(TMPDIR="$dir" timeout 300 "$binary" --exact "$t" --test-threads=1 2>&1 |
      grep -E '^test result' | sed -E 's/;.*//; s/^test result: //')
    printf '%-18s %-12s %s\n' "sw-$suffix" "$r" "$t"
  done
}
run 's1'      "$lib" output::tests::the_panels_session_and_ultrafast_change_nothing_the_status_command_prints
run 'T3'      "$lib" agent::compact::tests::the_newest_exchanges_stay_and_the_older_ones_of_their_turn_are_compacted
run '[2J-next' "$lib" credits::tests::an_unreadable_provider_asks_nobody_and_says_so_in_the_turns_words
run 'six'     "$lib" interactive::tests::the_unknown_command_refusal_counts_the_registry_rather_than_a_literal
run 'evil'    "$lib" provider::model::tests::a_catalog_id_with_a_control_character_is_refused_when_a_query_resolves_to_it tui::shell::tests::the_band_never_announces_or_sends_a_model_id_xfx_would_not_have_written
run 'pw@'     "$lib" trace::tests::a_daemon_url_is_reported_without_its_credentials
run '#: '     "$lib" tui::blocks::tests::table_wraps_long_cells_inside_the_grid_instead_of_stacking_fields
run 'sk-'     "$lib" tui::check::tests::a_reject_carries_no_row_text
run '7-accepted' "$lib" tui::diagnostic::tests::a_marked_partial_reports_its_reason_without_the_original_sentence
run 'opus-ask-yolo' "$lib" tui::hint::tests::a_notice_overrides_the_left_side_without_losing_the_right tui::hint::tests::every_mode_is_said_in_the_word_upstreams_row_says_it_in tui::shell::tests::left_steps_back_to_the_model_column_with_the_mark_on_the_model
run 'of 2-row0' "$lib" tui::question::tests::one_question_is_visible_at_a_time_with_ordinals_and_a_freeform_slot tui::question::tests::an_answer_taller_than_its_share_of_the_screen_keeps_the_caret_row_shown
run '1049'    "$lib" tui::shell::tests::a_question_the_other_plane_owns_is_painted_there_and_not_in_the_band
run 'high'    "$lib" tui::shell::tests::effort_with_a_word_applies_it_or_says_which_levels_there_are
run ';3[3[4'  "$lib" tui::usage_menu::tests::the_current_model_is_bold_and_colourless
run '%'       "$lib" tui::shell::tests::usage_and_max_context_drive_one_context_meter tui::shell::tests::a_turn_that_published_no_usage_leaves_the_meter_off tui::shell::tests::a_new_session_clears_the_meter_it_measured_and_keeps_the_window tui::hint::tests::the_meter_is_the_first_segment_a_narrow_screen_gives_up tui::hint::tests::a_window_nobody_published_leaves_the_numerator_on_its_own
run 'xfx:'    "$(bin interactive)" an_empty_line_asks_nothing_and_prompts_again
run 'one'     "$(bin tool_loop)" read_file_honors_an_explicit_line_range
run '997'     "$(bin tui)" a_palette_the_user_named_is_not_overruled_by_the_terminal
