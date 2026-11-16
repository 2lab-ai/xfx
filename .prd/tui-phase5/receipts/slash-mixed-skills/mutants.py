"""SLASH-MIXED-SKILLS' mutants: each takes one of the unit's rules out of the
code head and runs the tests that pin it; a mutant is killed when one fails.
(SLASH-ARG-SKILLS' mutants.py, with this unit's rules.)

Run on a throwaway `git clone --no-local` checked out at the code head, never
on the worktree: the clone's source is edited in place and restored after
each mutant.

usage: python3 -I mutants.py <clone> [ID...]   (TMPDIR set; cargo on PATH)
Prints one line per mutant and `killed N of M`; exits 1 when one survives.
"""
import os
import subprocess
import sys

PICKER = "src/tui/picker.rs"
SKILLS = "src/tui/skills_menu.rs"
SHELL = "src/tui/shell.rs"
SHELL_TESTS = [
    "tui::shell::tests::past_the_blank_of_a_name_nobody_has_the_menu_lists_the_skills_the_line_finds",
    "tui::shell::tests::a_name_with_no_argument_lists_nothing_past_its_blank_and_a_table_lists_its_own",
    "tui::shell::tests::tab_and_enter_bind_the_skill_over_the_whole_line_but_a_names_blank_submits_it",
    "tui::shell::tests::tab_takes_a_slash_words_ghost_before_the_skills_listed_past_the_blank",
    "tui::shell::tests::escape_over_the_skills_past_a_blank_lasts_while_the_line_is_a_slash_line",
    "tui::shell::tests::a_skill_row_binds_over_the_word_and_keeps_the_blanks_before_it",
    # Round 2: a moved mark and Tab on a slash word alone, and the column lines.
    "tui::shell::tests::on_a_slash_word_alone_tab_takes_the_marked_row_for_no_ghost_is_there",
    "tui::shell::tests::a_column_line_opens_no_slash_menu_whatever_skill_its_line_would_find",
]
TESTS = ["tui::picker::", "tui::arg_lists::", "tui::skills_menu::"] + SHELL_TESTS

ANCHORED = "        Some(spec) => (spec.has_args && !super::arg_lists::anchored(prefix)).then_some(prefix),"

# (id, what it takes out, file, old, new)
MUTANTS = [
    ("M1", "past a blank a name nobody has lists nothing (the base's rule)", PICKER,
     "        None => Some(prefix),\n",
     "        None => None,\n"),
    ("M2", "past a blank a name with an argument lists nothing", PICKER,
     ANCHORED,
     "        Some(spec) => (spec.has_args && !super::arg_lists::anchored(prefix) && prefix.is_empty()).then_some(prefix),"),
    ("M3", "past a blank a name with no argument lists the skills too", PICKER,
     ANCHORED,
     "        Some(_) => (!super::arg_lists::anchored(prefix)).then_some(prefix),"),
    ("M4", "an anchored line is the slash menu's query too", PICKER,
     ANCHORED,
     "        Some(spec) => spec.has_args.then_some(prefix),"),
    ("M5", "the name is looked up with case ignored", PICKER,
     "        .find(|spec| spec.name == name || spec.aliases.contains(&name))",
     "        .find(|spec| spec.name.eq_ignore_ascii_case(name) || spec.aliases.contains(&name))"),
    ("M6", "an alias is not looked up", PICKER,
     "        .find(|spec| spec.name == name || spec.aliases.contains(&name))",
     "        .find(|spec| spec.name == name)"),
    ("M7", "the commands are matched with the word before the blank, not the line", PICKER,
     "            .filter_map(|spec| found(spec, word))",
     "            .filter_map(|spec| found(spec, word.split(BLANKS).next().unwrap_or(word)))"),
    ("M8", "the skills' query keeps its blanks (the $ menu's trim)", SKILLS,
     "    let query = query.trim_matches([' ', '\\t', '\\r', '\\n', '/']);",
     "    let query = query.trim_start_matches('/');"),
    ("M9", "a name and a blank is not a command's name to Enter", PICKER,
     "    names(prefix) || (prefix.ends_with([' ', '\\t']) && names(prefix.trim_end_matches([' ', '\\t'])))",
     "    names(prefix)"),
    ("M10", "Enter on a name and a blank is spent, the line not submitted", SHELL,
     "            PickerOutcome::Typed => false,",
     "            PickerOutcome::Typed => true,"),
    ("M11", "Enter on a skill row binds it on a command's name too", PICKER,
     "                Entry::Skill(_) if self.exact => match self.commands {",
     "                Entry::Skill(_) if self.exact && false => match self.commands {"),
    ("M12", "the bind does not keep the blanks before the line", SHELL,
     "                let start = self.picker.take().map_or(0, |picker| picker.word_start());",
     "                let start = self.picker.take().map_or(0, |_| 0);"),
    ("M13", "Tab takes the list's row before a ghost", SHELL,
     "        if request == PickerAction::Complete && self.picker.is_some() && self.ghost().is_some() {",
     "        if false && request == PickerAction::Complete && self.picker.is_some() && self.ghost().is_some() {"),
    # Round 2 (opus S1): a `/model` or `/effort` line is the slash kind.
    ("M14", "a `/model` or `/effort` line is the slash kind, not its column's", SHELL,
     "        if model_column::shaped_line(text) || provider_picker::is_query(text) {",
     "        if provider_picker::is_query(text) {"),
]


def main():
    clone = sys.argv[1]
    only = set(sys.argv[2:])
    killed = total = 0
    for mid, what, path, old, new in MUTANTS:
        if only and mid not in only:
            continue
        total += 1
        full = os.path.join(clone, path)
        text = open(full, encoding="utf-8").read()
        if text.count(old) != 1:
            print(f"{mid} NOT-APPLIED ({what}): the text to change is there {text.count(old)} times")
            continue
        open(full, "w", encoding="utf-8").write(text.replace(old, new))
        try:
            run = subprocess.run(
                ["cargo", "test", "--locked", "--lib", "--", *TESTS],
                cwd=clone, capture_output=True, text=True,
            )
        finally:
            open(full, "w", encoding="utf-8").write(text)
        out = run.stdout + run.stderr
        failed = sorted({line.split()[1] for line in out.splitlines()
                         if line.startswith("test ") and line.rstrip().endswith("FAILED")})
        if "error[E" in out and not failed:
            print(f"{mid} KILLED-BY-COMPILE ({what})")
            killed += 1
        elif failed:
            killed += 1
            print(f"{mid} KILLED ({what}): " + ", ".join(name.split("::")[-1] for name in failed))
        else:
            print(f"{mid} SURVIVED ({what})")
    print(f"killed {killed} of {total}")
    sys.exit(0 if killed == total else 1)


if __name__ == "__main__":
    main()
