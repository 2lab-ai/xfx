#!/usr/bin/env bash
# RED: this unit's five shell cases and their two helpers (`mixed`,
# `slash_rows`), and picker.rs' new query case, as committed at <code>, put
# onto the base's src/tui/shell.rs and src/tui/picker.rs in a throwaway
# `git clone --no-local`, each after the case it follows at the head, and run
# there. picker.rs' other new case, `enter_on_a_skill_row_…`, names what the
# base has not got (`names_exactly`, `PickerOutcome::Typed`): it cannot be
# put there.
# usage: red.sh <repo> <base> <code> <clone dir>   (TMPDIR set; cargo on PATH)
set -euo pipefail
repo=${1:?repo}; base=${2:?base}; code=${3:?code}; clone=${4:?clone dir}
rm -rf "$clone"
git clone -q --no-local "$repo" "$clone"
git -C "$clone" checkout -q --detach "$base"
git -C "$repo" show "$code:src/tui/shell.rs" > "$clone/head-shell.rs"
git -C "$repo" show "$code:src/tui/picker.rs" > "$clone/head-picker.rs"
python3 -I - "$clone/src/tui/shell.rs" "$clone/head-shell.rs" "$clone/src/tui/picker.rs" "$clone/head-picker.rs" <<'PY'
import sys
base_path, head_path = sys.argv[1], sys.argv[2]
head = open(head_path, encoding="utf-8").read()
start = head.index("    /// [`skilled`] with the four skills")
last = head.index("    fn escape_over_the_skills_past_a_blank_lasts_while_the_line_is_a_slash_line")
end = head.index("\n    }\n", last) + len("\n    }\n")
block = head[start:end]
base = open(base_path, encoding="utf-8").read()
anchor = base.index("    fn a_skill_row_binds_over_the_word_and_keeps_the_blanks_before_it")
after = base.index("\n    }\n", anchor) + len("\n    }\n")
base = base[:after] + "\n" + block + base[after:]
open(base_path, "w", encoding="utf-8").write(base)
print(f"put {block.count(chr(10))} lines onto the base's shell.rs")
picker_path, head_picker_path = sys.argv[3], sys.argv[4]
head = open(head_picker_path, encoding="utf-8").read()
start = head.index("    #[test]\n    fn past_a_blank_the_query_goes_on_for_a_name_nobody_has_or_one_with_an_argument")
end = head.index("\n    }\n", start) + len("\n    }\n")
block = head[start:end]
base = open(picker_path, encoding="utf-8").read()
anchor = base.index("    fn a_slash_word_past_spaces_tabs_and_line_breaks_is_the_query")
after = base.index("\n    }\n", anchor) + len("\n    }\n")
base = base[:after] + "\n" + block + base[after:]
open(picker_path, "w", encoding="utf-8").write(base)
print(f"put {block.count(chr(10))} lines onto the base's picker.rs")
PY
rm "$clone/head-shell.rs" "$clone/head-picker.rs"
cd "$clone"
cargo test --locked --lib -- --exact \
  tui::picker::tests::past_a_blank_the_query_goes_on_for_a_name_nobody_has_or_one_with_an_argument \
  tui::shell::tests::past_the_blank_of_a_name_nobody_has_the_menu_lists_the_skills_the_line_finds \
  tui::shell::tests::a_name_with_no_argument_lists_nothing_past_its_blank_and_a_table_lists_its_own \
  tui::shell::tests::tab_and_enter_bind_the_skill_over_the_whole_line_but_a_names_blank_submits_it \
  tui::shell::tests::tab_takes_a_slash_words_ghost_before_the_skills_listed_past_the_blank \
  tui::shell::tests::escape_over_the_skills_past_a_blank_lasts_while_the_line_is_a_slash_line \
  2>&1 | grep -E '^test |^test result|panicked at|^  left|^ right|assertion' || true
