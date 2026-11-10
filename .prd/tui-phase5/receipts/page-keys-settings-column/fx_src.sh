#!/usr/bin/env bash
# Quotes every fx v0.0.16 line this receipt cites, from the pin, with its
# number: `fx_src.sh <fx-upstream checkout> > fx-src.txt`. Read-only (`git show`).
set -euo pipefail
fx=$1
rev=481104fd
q() { # <path> <first> <last>
  echo "== $1:$2-$3@$rev"
  git -C "$fx" show "$rev:$1" | awk -v a="$2" -v b="$3" 'NR >= a && NR <= b { printf "%5d  %s\n", NR, $0 }'
}
# The keys: PgUp/PgDn are a page of routeComposerVertical, C-j/C-k a row of
# routePickerControlByte; Home/End are caret moves; C-n is history_next and
# C-p is no composer shortcut; the wheel does nothing at the composer.
q src/ui/input/shortcuts.zig 11 29
q src/ui/input/shortcuts.zig 43 63
q src/ui/input/shortcuts.zig 103 117
q src/core/app/app_input_runtime.zig 298 328
q src/core/app/app_input_runtime.zig 403 407
q src/core/app/app_input_runtime.zig 870 894
q src/core/app/app_input_runtime.zig 1036 1040
q src/core/app/app_input_runtime.zig 1093 1116
q src/core/app/app_input_runtime.zig 1593 1619
q src/core/app/app_input_runtime.zig 1984 1990
# The page and where it goes.
q src/ui/footer/input_presentation.zig 252 255
q src/core/app/input_completion_runtime.zig 171 180
q src/core/app/input_completion_runtime.zig 211 213
q src/core/app/input_completion_runtime.zig 215 292
q src/core/app/input_completion_runtime.zig 390 422
# 1. /settings' menu and its Model row's list.
q src/core/app/input_completion_runtime.zig 431 463
q src/core/config/settings_catalog.zig 225 241
q src/core/config/settings_catalog.zig 261 276
q src/core/config/settings_catalog.zig 408 414
q src/core/app/model_cache_runtime.zig 229 245
# 2. The /model column's three stages.
q src/core/app/input_completion_runtime.zig 629 631
q src/core/app/input_completion_runtime.zig 1072 1106
q src/core/app/input_completion_runtime.zig 1135 1158
q src/core/shared/list_window.zig 1 68
# 3. /mcp's argument completions, a slash list.
q src/core/app/input_completion_runtime.zig 111 131
q src/core/slash_commands/command_specs.zig 568 585
q src/core/slash_commands/command_specs.zig 876 897
q src/core/app/app_mcp_runtime.zig 459 468
q src/core/mcp_host/config.zig 14 18
q src/core/mcp_host/config.zig 186 192
# Round 3: where fx hangs its lists under the composer, and when it opens
# the slash menu.
q src/ui/input/visual_layout.zig 697 713
q src/ui/footer/input_presentation.zig 287 292
q src/ui/footer/input_presentation.zig 340 386
q src/core/slash_commands/command_specs.zig 636 644
q src/core/input/picker_state.zig 287 296
# Round 3: how fx cuts a list's row too long for the screen (the cause
# model-column-clip): every inline list's row, its label cut at the edge.
q src/ui/footer/paint_plan.zig 943 953
q src/ui/footer/picker_presentation.zig 818 858
q src/ui/footer/row_text.zig 23 53
