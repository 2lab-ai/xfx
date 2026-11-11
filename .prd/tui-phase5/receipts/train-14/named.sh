#!/usr/bin/env bash
# Train 14 (new; train 13's named.sh ran its named tests again, this one
# reads the gate's own two `cargo test` runs through named.py): each test
# the join checks and the merges' fixes name, its result in each run.
#   J1   INPUT-ROW-LIMIT's page and cap cases the `@` list meets (main)
#   J2   (round 2, opus S5) AT-LAUNCH-WALK's own cases of the launch walk,
#        which tell the head from the base where the sweep's pair cannot
#   J3   the C-j/C-k meeting under the full transcript: CTRLO-AT-KEYS' cases
#        (the `@` list), CTRLO-OTHER-PICKERS' (every other picker, the
#        burst), PAGE-KEYS' (every other list), MCP-C's (`/mcp`'s menu)
#   J4   PAGE-KEYS' page cases (main's INPUT-ROW-LIMIT)
#   M1   merge 1's `event_loop.rs` blocks: main's card-preview and catalog
#        clock cases, the stack's held-frame cases
#   M2   merge 2's and 3's `CatalogDue` fixes, merge 3's `rows_walked`
#        rename (its callers are J4's), merge 4's worker test
# Output: named.txt, through redacted.py.
# usage: named.sh <gate-log-dir>
set -u
logs=$1
here=$(cd "$(dirname "$0")" && pwd)
python3 -I -B "$here/named.py" "$logs" \
  J1:at_41_rows_a_page_of_the_at_list_is_19_paths \
  J1:at_41_rows_the_composer_is_capped_at_19_rows_and_a_page_is_19_of_them \
  J1:on_an_eight_row_screen_a_page_and_the_composers_cap_are_one_row \
  J2:the_launch_starts_the_index_walk_fresh_or_resumed \
  J2:the_launch_walks_a_root_once_and_an_empty_root_never \
  J2:a_fresh_sessions_first_at_after_the_launch_walk_is_whole_shows_its_rows_at_once \
  J3:c_j_under_the_full_transcript_is_a_new_line_and_closes_the_list \
  J3:c_k_under_the_full_transcript_kills_to_the_lines_end_and_keeps_the_list \
  J3:tab_under_the_full_transcript_chooses_nothing_and_keeps_the_list \
  J3:the_arrows_and_page_keys_under_the_full_transcript_are_its_own \
  J3:escape_under_the_full_transcript_closes_it_and_keeps_the_list \
  J3:a_letter_under_the_full_transcript_is_typed_and_looked_up_once_it_closes \
  J3:tab_under_the_full_transcript_over_files_unavailable_walks_nothing \
  J3:c_j_and_c_k_in_ctrl_os_burst_over_the_catalog_edit_its_search_and_keep_its_mark \
  J3:c_j_in_ctrl_os_burst_over_model_session_and_skills_menus_does_nothing_and_c_k_kills_nothing \
  J3:c_j_in_ctrl_os_burst_over_a_dollar_menu_is_a_new_line_after_its_word \
  J3:c_j_and_c_k_in_ctrl_os_burst_over_fork_close_the_menu_and_the_screen_stays \
  J3:c_j_and_c_k_in_ctrl_os_burst_over_effort_close_the_menu_and_the_screen_stays \
  J3:c_j_in_ctrl_os_burst_inside_a_search_word_keeps_a_scrolled_catalogs_mark_and_window \
  J3:c_j_in_ctrl_os_burst_inside_a_search_word_keeps_a_scrolled_settings_mark_and_window \
  J3:c_k_in_ctrl_os_burst_with_text_after_the_caret_kills_it_and_each_menus_search_follows \
  J3:c_k_under_the_full_transcript_over_the_slash_menu_moves_no_mark \
  J3:c_j_under_the_full_transcript_over_the_slash_menu_is_a_new_line \
  J3:c_k_under_the_full_transcript_over_the_provider_column_moves_no_mark \
  J3:c_j_under_the_full_transcript_over_the_provider_column_is_a_new_line \
  J3:c_k_under_the_full_transcript_over_the_mcp_arguments_moves_no_mark \
  J3:c_j_under_the_full_transcript_over_the_mcp_arguments_is_a_new_line \
  J3:under_the_full_transcript_no_other_list_takes_a_row_key \
  J3:under_the_full_transcript_the_mcp_arguments_take_no_row_key \
  J3:under_the_full_transcript_the_model_column_takes_no_row_key \
  J3:under_the_full_transcript_the_model_menu_takes_no_row_key \
  J3:under_the_full_transcript_the_settings_menu_takes_no_row_key \
  J3:the_menu_takes_no_key_while_the_full_transcript_covers_it \
  J4:at_24_rows_a_page_of_the_settings_menu_is_11_settings_and_runs_to_the_far_end \
  J4:at_24_rows_a_page_of_the_settings_model_list_is_11_models \
  J4:at_24_rows_a_page_of_the_model_column_is_11_models \
  J4:at_24_rows_a_page_of_the_effort_and_speed_columns_runs_to_the_far_end \
  J4:at_24_rows_a_page_of_the_mcp_arguments_is_11_servers \
  J4:a_slash_menu_on_a_taller_composer_leaves_the_arrows_and_pages_to_the_caret \
  J4:mcp_arguments_on_a_taller_composer_leave_the_arrows_and_pages_to_the_caret \
  M1:the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame \
  M1:a_card_is_held_from_the_document_while_its_preview_is_up \
  M1:a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one \
  M1:the_model_columns_frame_reads_the_catalogs_clock \
  M1:the_frame_of_a_keystroke_that_lists_a_folder_waits_for_its_rows \
  M1:a_listing_still_out_at_8_ms_lets_the_owed_frame_go_and_its_rows_follow \
  M1:a_frame_held_for_a_folder_holds_its_bell_but_no_report_or_title \
  M1:a_held_tick_that_owes_a_title_writes_the_title_then_the_report \
  M1:a_key_the_list_hands_on_mid_resize_asks_for_the_frame_too \
  M1:a_key_handed_on_under_the_full_transcript_writes_nothing \
  M2:ctrl_o_over_the_model_menu_holds_no_frame_and_its_keys_stay_its \
  M2:a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning \
  2>&1 | python3 -I -B "$here/redacted.py" "$here/named.txt" "$logs=<gate-logs>"
