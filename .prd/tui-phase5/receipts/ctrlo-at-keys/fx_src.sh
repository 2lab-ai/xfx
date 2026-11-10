#!/bin/bash
# Prints every fx v0.0.16 line CTRLO-AT-KEYS cites, each range under its
# path:first-last@rev, read with `git show` (the checkout is never touched).
# `fxcites.py` checks every citation in this folder's README, in the parity
# row's text this unit changed and in the source lines it added against
# these `q` ranges, so each names its file in full.
# usage: fx_src.sh <fx-upstream checkout>   (writes to stdout; fx-src.txt is its output)
set -euo pipefail
fx=$1
P=481104fd
q() { # path first last what
  printf '\n== %s:%s-%s@%s -- %s\n' "$1" "$2" "$3" "$P" "$4"
  git -C "$fx" show "$P:$1" | awk -v a="$2" -v b="$3" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
# How a byte reaches the composer
q src/ui/input/terminal_action_decoder.zig 207 212 "appendRaw: a plain byte carries its composer shortcut"
q src/ui/input/shortcuts.zig 11 29 "fromControlByte: C-k (11) is delete_to_line_end (19), C-j is insert_newline (26); Tab and Enter have none"
q src/core/app/app_input_runtime.zig 586 600 "every key ends with reconcileFilePicker (597), which distrusts an ineligible list"
q src/core/app/app_input_runtime.zig 840 894 "a raw byte: the transcript's bytes (872), a modal (873), the picker control bytes (879), then the composer (881-893)"
q src/core/app/app_input_runtime.zig 896 931 "a decoded key: Escape to the transcript first (915-919), then every other key it binds (930)"
q src/core/app/app_input_runtime.zig 1093 1116 "routePickerControlByte: no picker takes C-j/C-k while the transcript is up (composerPickerSurfaceVisible, 1110-1116)"
q src/core/app/app_input_runtime.zig 1290 1321 "handleComposerByte: Tab with a file query -> autocompleteFilePickerSelection, and a frame (1304-1321)"
q src/core/app/app_input_runtime.zig 1392 1411 "Enter: the file picker's answer when it owns the surface (1405-1410), else on to submit"
q src/core/app/app_input_runtime.zig 1435 1481 "submit (1440); a byte with a composer shortcut (1442-1445); a printable byte typed (1447-1478)"
q src/core/app/app_input_runtime.zig 244 246 "routeComposerShortcutAction"
q src/core/app/app_input_runtime.zig 479 488 "delete_to_line_end: a kill to the line's end"
q src/core/app/app_input_runtime.zig 513 523 "insert_newline: a new line in the composer"
# The full transcript's keys
q src/core/app/input_full_transcript_runtime.zig 26 52 "screenOwnsInput, routeByte, routeAction"
q src/core/app/input_full_transcript_runtime.zig 129 197 "closeScreen (129-145); keyForByte (147-154); keyForAction (156-178): Up/Down/page keys scroll, Escape closes (163); routeKey (181-197)"
# The list's side
q src/core/app/input_completion_runtime.zig 390 422 "routeVisiblePickerMove, routeNonSlashPickerMove: a catalog menu's move first, then the file query takes it (407-410)"
q src/core/app/input_completion_runtime.zig 775 809 "filePickerEligible (775-786): not under the full transcript (783); reconcileFilePicker distrusts an ineligible list (798-799)"
q src/core/app/input_completion_runtime.zig 848 866 "prepareFilePicker: nothing while not eligible (853), the index refresh after it (854-866)"
q src/core/app/input_completion_runtime.zig 939 965 "navigateFilePicker; autocompleteFilePickerSelection: the eligibility check (949) before the row (950) and the retry (953)"
q src/core/app/input_completion_runtime.zig 1034 1056 "submitFilePickerOnEnter: nothing unless the list owns the surface (1037); filePickerOwnsSurface (1054-1056)"
q src/core/input/file_completion_state.zig 230 233 "retry: the index's refresh asked for"
