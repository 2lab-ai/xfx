#!/bin/bash
# Prints every fx v0.0.16 line CTRLO-OTHER-PICKERS cites, each range under its
# path:first-last@rev, read with `git show` (the checkout is never touched).
# `fxcites.py` checks every citation in this folder's README, in the parity
# rows' text this unit changed, in the source lines it added and in its
# scenarios' headers against these `q` ranges, so each names its file in full.
# usage: fx_src.sh <fx-upstream checkout>   (writes to stdout; fx-src.txt is its output)
set -euo pipefail
fx=$1
P=481104fd
q() { # path first last what
  printf '\n== %s:%s-%s@%s -- %s\n' "$1" "$2" "$3" "$P" "$4"
  git -C "$fx" show "$P:$1" | awk -v a="$2" -v b="$3" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
# 1. The frame closes the full transcript over seven menus (group A)
q src/core/app/app_render_runtime.zig 1132 1142 "reconcileBeforeFrameRender: what the frame settles first"
q src/core/app/app_render_runtime.zig 1206 1215 "while the settings, help, skills, model, session, fork or effort menu is up, the full transcript is closed"
q src/core/app/app_render_runtime.zig 1252 1258 "the frame calls it (1254) before it renders anything"
q src/core/app/app_render_runtime.zig 2001 2035 "the seven tests: skills menuVisible (2002), model (2007), session (2012), fork (2017), help (2022), effort (2027), settings (2032)"
q src/core/app/app_lifecycle.zig 1277 1286 "closeFullTranscriptIfActive"
q src/core/app/app_lifecycle.zig 1387 1400 "closeFullTranscript: the trace line close_full_transcript restore=..."
q src/ui/input/escape_parser.zig 268 274 "Ctrl-O (15) is toggle_full_transcript"
q src/core/app/app_input_runtime.zig 896 1002 "a decoded key: Escape to the transcript first (915-919), the keys it binds (930), Ctrl-O through routeComposerAction (997); the composer shortcut after (999-1002)"
q src/core/app/input_full_transcript_runtime.zig 26 197 "screenOwnsInput, routeByte, routeAction, routeComposerAction (54-71); transitionScreen opens (95-111) and traces depth_transition (121-126); closeScreen traces close_screen (129-145); keyForByte (147-154); keyForAction (156-178); routeKey (181-197)"
q src/core/shared/debug_trace.zig 413 434 "FX_TRACE_LOG and FX_TRACE_SCOPES: the trace the help scenario keeps"
q src/core/app/app_input_runtime.zig 658 677 "flushPendingEscape: a lone Escape goes to resolveEscape"
q src/core/app/app_input_runtime.zig 2687 2688 "resolveEscape: the full transcript first"
q src/core/app/app_input_runtime.zig 2761 2769 "then a menu's Escape closes it (2765)"
# 2. Under the screen, C-j and C-k are the composer's (group B)
q src/ui/input/terminal_action_decoder.zig 207 212 "appendRaw: a plain byte carries its composer shortcut"
q src/ui/input/shortcuts.zig 11 29 "fromControlByte: C-k (11) delete_to_line_end (19), C-j insert_newline (26)"
q src/core/app/app_input_runtime.zig 840 894 "a raw byte: the transcript's bytes (872), a modal (873), the picker control bytes (879), then the composer (881-893)"
q src/core/app/app_input_runtime.zig 1093 1116 "routePickerControlByte: no picker takes C-j/C-k while the transcript is up (composerPickerSurfaceVisible, 1110-1116)"
q src/core/app/app_input_runtime.zig 1593 1619 "isComposerEditingByte; activeCatalogMenuOwnsByte"
q src/core/app/app_input_runtime.zig 1984 1996 "dismissActiveMenusForComposerEdit: the fork and effort menus closed by an edit"
q src/core/app/app_input_runtime.zig 244 246 "routeComposerShortcutAction"
q src/core/app/app_input_runtime.zig 479 488 "delete_to_line_end: a kill to the line's end"
q src/core/app/app_input_runtime.zig 513 523 "insert_newline: a new line (nothing while the /skills, /model or session menu is up, 514)"
q src/core/app/app_input_runtime.zig 1290 1354 "handleComposerByte: Tab, the same with the screen up or not (1304-1353)"
q src/core/app/app_input_runtime.zig 1392 1446 "Enter (1392-1441), the same with the screen up or not; a byte with a composer shortcut (1442-1446)"
q src/core/app/app_input_runtime.zig 1713 1744 "the menus' tests on the input side"
q src/core/app/app_mcp_runtime.zig 459 485 "/mcp 's argument completions: the slash completion's verb_completions (459-468), refreshCompletion"
q src/core/app/input_completion_runtime.zig 175 180 "routeSlashCompletionMove"
q src/core/app/input_completion_runtime.zig 390 422 "routeVisiblePickerMove, routeNonSlashPickerMove: each menu's move, the file query's (407-410), the provider column's (415-418)"
q src/core/app/input_completion_runtime.zig 775 786 "filePickerEligible: the one picker that asks about the transcript itself (783)"
# 3. Shift-Up/Down under the screen over the @ list (CTRLO-AT-KEYS opus NIT)
q src/ui/input/escape_parser.zig 66 71 "composerMove: a modified arrow is a composer move, extend_selection on Shift"
q src/ui/input/escape_parser.zig 99 107 "Shift-Up/Down: visual_up/visual_down"
q src/ui/input/shortcuts.zig 43 65 "fromEscapeAction: a composer_shortcut action carries its shortcut (61)"
q src/core/app/app_input_runtime.zig 310 330 "the .move arm: visual_up/visual_down -> routeComposerVertical"
q src/core/app/input_completion_runtime.zig 211 233 "routeComposerVertical: routeNonSlashPickerMove first (230)"
# 4. The column's anchor off the caret's row (provider-ctrlo cj)
q src/ui/input/visual_layout.zig 705 713 "projectedAnchorColumn: the anchor's column on the caret's row, the caret's row's start otherwise"
# 5. Round 2: the burst, the search's words, the column's anchor
q src/ui/event_loop.zig 88 122 "the loop reads up to max_input_reads_per_fact_collection reads (94) before one frame (110, 119)"
q src/core/slash_commands/command_specs.zig 457 470 "helpCatalogSpecMatches: the search's words, split at spaces, tabs and line breaks (460)"
q src/ui/footer/input_presentation.zig 383 386 "picker_start_col: projectedAnchorColumn for the model, provider, file and slash columns"
