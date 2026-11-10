#!/usr/bin/env bash
# Quotes every fx v0.0.16 line this unit's claims rest on, into fx-src.txt.
#
# usage: FX_REPO=<fx checkout> fx_src.sh > fx-src.txt
#
# Each citation is `path start end needle`: the range is printed with its line
# numbers, and the run fails (exit 1) when the range runs past the file's end
# or the needle is not inside it -- a range that drifted is a failed run, not
# a quote of the wrong lines. Read with `git show`; never checked out.
set -euo pipefail
: "${FX_REPO:?FX_REPO names the fx checkout}"
pin=481104fd
fail=0
cite() {
  local path=$1 start=$2 end=$3 needle=$4 file total
  file=$(git -C "$FX_REPO" show "$pin:$path")
  total=$(printf '%s\n' "$file" | wc -l | tr -d ' ')
  echo "### git -C fx-upstream show $pin:$path | sed -n ${start},${end}p"
  if (( start < 1 || end < start || end > total )); then
    echo "RANGE-FAIL $path:$start-$end (file has $total lines)"; fail=1; echo; return
  fi
  local body
  body=$(printf '%s\n' "$file" | awk -v a="$start" -v b="$end" 'NR>=a && NR<=b {printf "%6d  %s\n", NR, $0}')
  printf '%s\n' "$body"
  if [[ $body != *"$needle"* ]]; then
    echo "NEEDLE-FAIL $path:$start-$end lacks: $needle"; fail=1
  fi
  echo
}
section() { echo; echo "## $*"; }
cs=src/core/slash_commands/command_specs.zig

echo "# fx v0.0.16 (pin $pin): every line SLASH-ARG-SKILLS's claims about fx rest on."

section "A1 past a blank the popup's prefix goes on for a name with arguments, and stops for one without"
cite $cs 568 576 "if (!spec.has_args) return null;"

section "A2 the anchored commands: /mcp (its verbs on), /statusline, /sound, /permissions, /workspace, /allowlist's anchor"
cite $cs 636 644 "pub fn argCompletionAnchor"
cite $cs 871 874 "fn mcpVerbsOn"

section "A3 each command's list, in this order: /mcp's run-time list first, then the tables"
cite $cs 610 631 "pub fn nthSlashCompletion"
cite $cs 578 608 "pub fn slashCompletionCount"

section "A4 the label is the line past the command; the tables have no description and no category"
cite $cs 646 671 "pub fn nthSlashCompletionLabel"
cite $cs 673 690 "if (statuslineArgCompletionPrefix(prefix) != null) return null;"

section "A5 the tables' words"
cite $cs 750 776 "const statusline_arg_completions"
cite $cs 778 841 "const allowlist_remove_tool_completions"

section "A6 the filter: the command, then a space or a tab; the four trimmed both ends, /allowlist at the start only; a word matched by its start, case ignored, an empty query matching all"
cite $cs 909 943 "fn argCompletionPrefix"
cite $cs 1031 1034 "fn argCompletionMatches"

section "A7 /allowlist walks its words, and its anchor is each arm's head"
cite $cs 1042 1101 "fn allowlistArgCompletionState"
cite $cs 1108 1204 "fn scopedAllowlistArgCompletionState"
cite $cs 1206 1276 "fn allowlistArgCompletionAnchor"

section "A8 Tab writes the marked line, with a space when more is to come"
cite src/core/app/app_input_runtime.zig 1331 1353 "slashCompletionHasArgs"
cite $cs 692 748 "pub fn slashCompletionHasArgs"

section "A9 Enter runs the marked line in place of what was typed, while the list shows, and the history records the line run"
cite src/core/app/input_submit_runtime.zig 719 729 "const resolved_slash_submission = resolvedSlashSubmission(app, left_trimmed);"
cite src/core/app/input_submit_runtime.zig 1098 1101 "fn resolvedSlashSubmission"
cite src/core/app/input_submit_runtime.zig 1009 1016 "const command_text = resolvedSlashSubmission(app, text)"
cite src/core/app/input_submit_runtime.zig 1059 1064 "recordAcceptedSlashCommandHistory"
cite src/core/app/input_submit_runtime.zig 1818 1823 "fn resolveSlashSubmission"

section "A10 the rows hang under the anchor, past the draft's leading blanks; no header (an anchored prefix has no slash layout); the list picker's rows"
cite src/ui/footer/input_presentation.zig 287 292 "fn slashRawAnchor"
cite src/ui/footer/picker_presentation.zig 710 732 "if (command_specs.argCompletionAnchor(prefix) != 0) return null;"
cite src/ui/footer/paint_plan.zig 911 923 "composeMixedSlashCompletionOptionRow"
cite src/ui/footer/picker_presentation.zig 1446 1486 "pub fn composeSlashCompletionOptionRow"
cite src/ui/footer/surface_frame.zig 659 666 "list_picker_rows"

section "A11 one .slash kind: the popup, the argument lists and the ghost; Escape dismisses the visible kind, which lasts while the kind is the same"
cite src/core/app/input_completion_runtime.zig 97 109 "fn slashCompletionQueryActive"
cite src/core/app/input_completion_runtime.zig 133 169 "fn visibleInlinePickerKind"
cite src/core/input/picker_state.zig 287 296 "pub fn inlinePickerTriggerKind"
cite src/core/input/picker_state.zig 515 532 "fn suppressionAfterEdit"

section "A12 the dim ghost is a mid-line slash word's (or a \$word's), never an argument's"
cite src/core/app/input_completion_runtime.zig 653 667 "pub fn visibleInlineSlashCompletion"
cite src/core/input/picker_state.zig 613 624 "fn findInlineSlashQuery"

section "A13 /mcp's run-time list: the eight verbs, then each server's lines"
cite src/core/app/app_mcp_runtime.zig 459 468 "const verb_completions"
cite src/core/app/app_mcp_runtime.zig 487 513 "fn buildCompletion"
cite $cs 876 907 "fn runtimeArgQuery"

section "S1 (not this unit: the split's second part) an unanchored prefix lists the skills its words find, after the commands"
cite src/ui/footer/picker_presentation.zig 1170 1227 "pub fn mixedSlashCompletionCount"
cite src/core/skills/skill_runtime.zig 1380 1392 "fn skillMatchRank"
cite src/core/app/app_input_runtime.zig 2366 2396 "fn submitSlashPickerSelection"
cite src/core/app/app_input_runtime.zig 2451 2476 "fn bindSelectedSlashSkill"

exit $fail
