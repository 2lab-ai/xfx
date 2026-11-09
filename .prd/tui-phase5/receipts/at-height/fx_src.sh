#!/bin/bash
# Prints every fx line this receipt cites, at the pin (and the old rule at
# c1db9191), each range under its path:lines@rev. usage: fx_src.sh <fx-upstream checkout>
set -euo pipefail
fx=$1
show() { # rev path start end what
  printf '\n== %s:%s-%s@%s -- %s\n' "$2" "$3" "$4" "$1" "$5"
  git -C "$fx" show "$1:$2" | awk -v a="$3" -v b="$4" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
P=481104fd
show $P src/ui/footer/picker_presentation.zig 647 672 "the list picker's height, the @ list's height, the @ list's item rows"
show $P src/ui/footer/picker_presentation.zig 1588 1597 "fx's own height table"
show $P src/core/shared/list_window.zig 1 3 "six rows: default_max_picker_rows"
show $P src/core/shared/list_window.zig 38 68 "advanceSelectionWithin and updateEdgeStart: wrap, edge-scroll"
show $P src/core/shared/list_window.zig 70 101 "fx's scroll tests"
show $P src/core/input/file_completion_state.zig 70 78 "the receipt carries the painted rows (default six)"
show $P src/core/input/file_completion_state.zig 104 110 "the state keeps them (default six; reset only by deinit)"
show $P src/core/input/file_completion_state.zig 239 249 "navigate scrolls within visible_rows"
show $P src/core/input/file_completion_state.zig 251 277 "view hands the stored window to the frame"
show $P src/core/input/file_completion_state.zig 279 296 "acknowledge: the window as given, the painted rows taken"
show $P src/core/input/file_completion_state.zig 319 350 "fx's navigation test"
show $P src/core/app/app_render_runtime.zig 1854 1859 "only a committed frame that showed the file picker sets the receipt"
show $P src/core/app/app_render_runtime.zig 2355 2361 "paintedFilePickerReceipt"
show $P src/core/app/app_render_runtime.zig 647 654 "the status line over rows: only Selection unavailable (stale)"
show $P src/ui/footer/paint_plan.zig 925 942 "the frame re-fits the window from the stored start, over the item rows"
show $P src/ui/footer/surface_frame.zig 526 542 "banner_rows and the list picker's height"
show $P src/ui/footer/surface_frame.zig 659 666 "the @ list's band rows"
show $P src/ui/footer/render_input.zig 540 551 "banner rows: the message rows and one composer gap"
show $P src/ui/footer/surface_frame.zig 356 369 "the banner is clamped to the screen"
show $P src/core/app/input_completion_runtime.zig 215 233 "PgUp/PgDn: a page is inputRowLimit(content_bottom) rows, then the same navigate"
show $P src/core/app/input_completion_runtime.zig 939 944 "navigateFilePicker"
show $P src/core/app/input_completion_runtime.zig 788 798 "a changed @ query resets the bar and the window to 0, and keeps visible_rows"
show $P src/core/input/picker_state.zig 249 252 "resetFilePickerIndex"
show $P src/core/input/file_completion_state.zig 121 143 "deinit resets every field; invalidate and distrust clear trusted, and leave visible_rows"
show $P src/core/input/file_completion_state.zig 147 196 "reconcile: a changed query clears trusted (:175) and leaves visible_rows"
show $P src/core/input/file_completion_state.zig 204 237 "stage leaves trusted; current is the presented snapshot; selected and acceptedEmpty need trusted"
show $P src/core/app/app_render_runtime.zig 1002 1026 "a frame not committed distrusts; a committed one acknowledges with a receipt, distrusts without"
show $P src/core/app/input_completion_runtime.zig 775 809 "filePickerEligible; reconcile distrusts while ineligible, resize-pending, reset-pending or dimensions invalid"
show $P src/core/app/input_completion_runtime.zig 946 966 "Tab: no selection chooses nothing (retry only for unavailable or stale)"
show $P src/core/app/input_completion_runtime.zig 1034 1056 "Enter: no selection is spent unless acceptedEmpty"
show $P src/core/input/picker_state.zig 196 213 "invalidate on a new inline-picker episode and on dismiss"
show $P src/ui/resize_runtime.zig 657 662 "dimensions invalid: set on a resize no layout fits"
show c1db9191 src/ui/footer/picker_presentation.zig 634 639 "the old height: the list picker's, at most six"
show c1db9191 src/ui/footer/surface_frame.zig 636 641 "the old @ band rows: show_picker, so list_picker_rows"
