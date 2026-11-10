#!/usr/bin/env bash
# Quotes every fx v0.0.16 line this unit's claims rest on, into fx-src.txt.
# (SLASH-ARG-SKILLS' fx_src.sh, with this unit's citations.)
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
ranges=0
cite() {
  local path=$1 start=$2 end=$3 needle=$4 file total
  file=$(git -C "$FX_REPO" show "$pin:$path")
  total=$(printf '%s\n' "$file" | wc -l | tr -d ' ')
  ranges=$((ranges + 1))
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
pp=src/ui/footer/picker_presentation.zig
sk=src/core/skills/skill_runtime.zig
ai=src/core/app/app_input_runtime.zig
ic=src/core/app/input_completion_runtime.zig

echo "# fx v0.0.16 (pin $pin): every line SLASH-MIXED-SKILLS's claims about fx rest on."

section "M1 which lines: past a blank the prefix goes on for a word no command has, or one that takes an argument; one that takes none has no prefix"
cite $cs 568 576 "if (!spec.has_args) return null;"
cite src/core/mods/registry.zig 42 55 "pub fn matchExact"
cite src/core/mods/registry.zig 69 78 "const trimmed = std.mem.trimEnd"

section "M2 an anchored prefix (an argument list) adds no skill; any other adds the skills its query finds, after the commands"
cite $pp 1170 1175 "if (command_specs.argCompletionAnchor(prefix) != 0) return command_count;"
cite $pp 1177 1189 "pub fn nthMixedSlashCompletionSkill"
cite $pp 1213 1220 "skillMenuSkillAtQuery(skills, .all, prefix[1..]"
cite $pp 1139 1168 "fillSkillMenuRangeAtQuery"
cite $cs 636 644 "pub fn argCompletionAnchor"
cite $cs 909 915 "if (boundary != ' ' and boundary != '\\t') return null;"

section "M3 the commands past a blank: matched with the whole prefix, so none answers one with a blank in it"
cite $cs 520 526 "fn completionCommandMatchRank"
cite $cs 578 608 "if (matchedCompletionCommand(spec, prefix) != null) count += 1;"

section "M4 the skills: the query trimmed of blanks and slashes, by name start, name, then description, source or path; then the source group, then discovery order"
cite $sk 1380 1392 "fn skillMatchRank"
cite $sk 1330 1376 "if (skillGroupRank(skill.source) != self.source_rank) continue;"
cite $sk 1216 1223 "pub fn skillMenuFilterQueryCount"
cite $sk 1158 1174 ".workspace_shared => \"workspace skills/\""

section "M5 the header: Results while a skill is listed, type to filter only for the bare slash; a skill row's name, description and short source"
cite $pp 1229 1245 "const noun = if (layout.command_count == layout.result_count) \"Commands\" else \"Results\";"
cite $pp 1280 1302 ".metadata = if (include_metadata) skill_runtime.skillSourceShortLabel(skill.source) else \"\","

section "M6 the visible slash count is the mixed one, while the slash kind is the draft's; the model and provider kinds come first"
cite $ic 82 109 "picker_presentation.mixedSlashCompletionCount("
cite src/core/input/picker_state.zig 287 296 "if (self.rawModelPickerQuery(editor) != null) return .model;"

section "M7 Tab: the ghost first, then the marked row -- a skill bound, a command written"
cite $ai 1304 1353 "} else if (completion_rt.visibleInlineCompletion(app) != null) {"
cite $ic 653 667 "pub fn visibleInlineSlashCompletion"

section "M8 Enter: a skill row bound unless the line is a command's name (a blank after it too); then the submit resolves to the marked command, or the line as typed with none"
cite $ai 1391 1400 "if (try submitSlashPickerSelection(app)) return;"
cite $ai 2366 2396 "if (app.slashRegistry().matchExact(prefix) != null) return false;"
cite src/core/app/input_submit_runtime.zig 1098 1101 "fn resolvedSlashSubmission"
cite src/core/app/input_submit_runtime.zig 1818 1823 "const count = command_specs.slashCompletionCount(registry, text);"

section "M9 the bind: the skill's token over the whole prefix, the leading blanks kept"
cite $ai 2451 2476 "const leading_ws = items.len - prefix.len;"

section "M10 one .slash dismissal: Escape dismisses the visible kind, which lasts while the kind is the same"
cite $ic 133 169 "fn visibleInlinePickerKind"
cite src/core/input/picker_state.zig 515 532 "fn suppressionAfterEdit"

section "M11 the registry: each command's name, aliases and whether it takes an argument (has-args.txt compares it with xfx's)"
cite src/builtins/commands.zig 441 481 "pub const slash_specs = [_]SlashSpec{"
cite src/core/input/picker_state.zig 54 61 "pub const provider_picker_prefixes"

echo "# $ranges ranges"
exit $fail
