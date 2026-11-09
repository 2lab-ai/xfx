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
q src/ui/footer/input_presentation.zig 252 267
q src/main.zig 173 173
q src/ui/terminal/terminal.zig 36 59
q src/core/app/app_lifecycle.zig 883 883
q src/main.zig 1036 1036
q src/ui/resize_runtime.zig 271 279
q src/core/app/app_session_runtime.zig 1598 1598
q src/core/app/app_input_runtime.zig 310 327
q src/core/app/input_completion_runtime.zig 215 263
q src/core/app/input_completion_runtime.zig 395 463
q src/core/app/input_completion_runtime.zig 465 539
q src/core/app/input_completion_runtime.zig 560 575
q src/core/app/input_completion_runtime.zig 1072 1096
q src/ui/footer/surface_frame.zig 437 441
q src/ui/input/visual_layout.zig 682 688
q src/core/input/effort_menu.zig 53 57
echo "== callers of inputRowLimit / cappedInputRows at $rev"
git -C "$fx" grep -n "inputRowLimit\|cappedInputRows" "$rev" -- src | sed "s/^$rev://"
