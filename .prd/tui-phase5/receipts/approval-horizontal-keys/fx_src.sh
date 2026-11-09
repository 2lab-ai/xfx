#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  src/core/app/input_approval_runtime.zig:71-130
  src/core/permissions/approval_decision.zig:100-176
  src/core/permissions/approval_decision.zig:214-229
  src/core/permissions/approval_decision.zig:249-271
  src/core/permissions/approval_decision.zig:285-303
  src/core/permissions/approval_amendment.zig:20-22
  src/core/permissions/approval_amendment.zig:170-172
  src/ui/footer/approval_ui.zig:1173-1225
  src/ui/footer/approval_ui.zig:1413-1418
  src/ui/footer/approval_ui.zig:1688-1694
  src/ui/input/runtime.zig:61-133
  src/ui/input/shortcuts.zig:11-65
  src/ui/input/escape_parser.zig:38-39
  src/ui/input/escape_parser.zig:66-109
  src/ui/input/escape_parser.zig:146-167
  src/ui/input/escape_parser.zig:185-217
  src/ui/input/escape_parser.zig:235-250
  src/ui/input/escape_parser.zig:528-555
  src/ui/input/escape_parser.zig:561-568
  src/ui/input/escape_parser.zig:721-737
  src/ui/input/terminal_action_decoder.zig:207-224
  src/core/app/app_input_runtime.zig:955-971
  src/core/app/app_input_runtime.zig:1161-1185
  src/core/app/app_input_runtime.zig:5931-5981
)
{
  echo "# fx v0.0.16 ($pin), every line this receipt cites, as git show $pin:<path> prints it, numbered."
  for r in "${ranges[@]}"; do
    path=${r%%:*} span=${r##*:}
    echo
    echo "## $r"
    git -C "$fx" show "$pin:$path" | awk -v a="${span%-*}" -v b="${span#*-}" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
  done
} > "$here/fx-src.txt"
