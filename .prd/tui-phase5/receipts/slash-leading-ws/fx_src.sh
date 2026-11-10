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

echo "# fx v0.0.16 (pin $pin): every line SLASH-LEADING-WS's claims about fx rest on."

section "F1 the trigger kind: a slash word at the caret, else a draft whose first byte past spaces, tabs, CR and LF is /"
cite src/core/input/picker_state.zig 287 296 'trimmed[0] == '"'"'/'"'"''
cite src/core/input/picker_state.zig 279 285 'tokenMatchesAt(command_text, 0, "/mcp")'

section "F2 findInlineSlashQuery: the last word, / and one byte more, with non-blank text before it (a ghost, not the popup)"
cite src/core/input/picker_state.zig 580 582 "fn isFilePickerTerminator"
cite src/core/input/picker_state.zig 613 624 "fn findInlineSlashQuery"
cite src/core/app/input_completion_runtime.zig 653 661 "activeInlineSlashQuery"

section "F3 the popup's query is the trimmed draft; a space after a no-args name ends it; the popup asks for the .slash kind"
cite src/core/slash_commands/command_specs.zig 568 576 "pub fn slashCompletionPrefix"
cite src/ui/footer/input_presentation.zig 269 276 "pub fn slashCompletionPickerPrefix"
cite src/core/app/input_completion_runtime.zig 97 109 "fn slashCompletionQueryActive"

section "F4 the /model and /provider kinds skip spaces and tabs only"
cite src/core/input/picker_state.zig 584 588 "fn leadingWhitespaceLen"
cite src/core/input/picker_state.zig 298 309 "fn rawModelPickerQuery"
cite src/core/input/picker_state.zig 336 340 "fn rawProviderPickerQuery"

section "F5 Escape's latch lasts while the kind is the same; every edit zeroes the mark; a recalled slash line is closed until edited"
cite src/core/input/picker_state.zig 203 213 "pub fn dismissInlinePicker"
cite src/core/input/picker_state.zig 228 236 "pub fn reconcileInlinePickerAfterEdit"
cite src/core/input/picker_state.zig 515 532 "fn suppressionAfterEdit"
cite src/core/app/input_completion_runtime.zig 133 140 "pub fn dismissVisibleInlinePicker"

section "F6 Tab: a command row replaces the whole composer; a skill row binds over the slash word and keeps the blanks before it"
cite src/core/app/app_input_runtime.zig 1331 1353 "textReplacementState().replace"
cite src/core/input/composer_text_replacement.zig 63 94 "self.edit.swapInput"
cite src/core/app/app_input_runtime.zig 2451 2476 "const leading_ws = items.len - prefix.len"

section "F7 Enter: a skill row binds; a command line is left-trimmed, resolved to the marked row, run, and recorded as resolved"
cite src/core/app/app_input_runtime.zig 2366 2396 "fn submitSlashPickerSelection"
cite src/core/app/input_submit_runtime.zig 719 729 "const left_trimmed = std.mem.trimStart"
cite src/core/app/input_submit_runtime.zig 1009 1016 "const command_text = resolvedSlashSubmission(app, text)"
cite src/core/app/input_submit_runtime.zig 1059 1064 "recordAcceptedSlashCommandHistory"
cite src/core/app/input_submit_runtime.zig 1098 1101 "fn resolvedSlashSubmission"

section "F8 the popup's row budget leaves the composer's rows past its first to it"
cite src/ui/footer/picker_presentation.zig 754 771 "inlinePickerRowBudget(terminal_rows, input_extra, banner_rows)"
cite src/ui/footer/picker_presentation.zig 793 803 "const fixed_rows: u16 = 5 +| input_extra +| banner_rows"
cite src/ui/footer/surface_frame.zig 543 551 "geometry.input_extra"

section "F9 what xfx does not copy: past a blank the prefix goes on, for argument completion (/mcp's among it, under the same .slash latch) and for the skills its words find"
cite src/core/slash_commands/command_specs.zig 636 644 "pub fn argCompletionAnchor"
cite src/ui/footer/picker_presentation.zig 1170 1175 "skillMenuFilterQueryCount(skills, .all, prefix[1..])"
cite src/core/app/input_completion_runtime.zig 142 169 "fn visibleInlinePickerKind"

if (( fail )); then echo "fx_src.sh: a citation failed its range check" >&2; exit 1; fi
echo "# every range above is inside its file and holds its needle"
