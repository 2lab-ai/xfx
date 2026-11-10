"""SLASH-LEADING-WS's mutants: each takes one of the unit's rules out of the
code head and runs the tests that pin it; a mutant is killed when one fails.

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
SHELL = "src/tui/shell.rs"
SHELL_TESTS = [
    "tui::shell::tests::a_slash_word_past_leading_blanks_opens_the_menu",
    "tui::shell::tests::a_dismissal_lasts_while_the_draft_still_begins_with_a_slash",
    "tui::shell::tests::tab_and_enter_over_leading_blanks_put_the_bare_name_in_the_composer",
    "tui::shell::tests::the_menu_leaves_a_taller_composer_its_rows",
    "tui::shell::tests::a_skill_row_binds_over_the_word_and_keeps_the_blanks_before_it",
]
TESTS = ["tui::picker::"] + SHELL_TESTS

# (id, what it takes out, file, old, new)
MUTANTS = [
    ("M1", "the kind skips no blanks", PICKER,
     "        text.trim_start_matches(BLANKS)\n            .starts_with('/')\n            .then_some(Self::Slash)",
     "        text.starts_with('/').then_some(Self::Slash)"),
    ("M2", "the kind ends at a blank after the word (the base's rule)", PICKER,
     "        text.trim_start_matches(BLANKS)\n            .starts_with('/')\n            .then_some(Self::Slash)",
     "        let word = text.trim_start_matches(BLANKS);\n        (word.starts_with('/') && !word.contains(BLANKS)).then_some(Self::Slash)"),
    ("M3", "the query skips no blanks", PICKER,
     "    let word = text.trim_start_matches(BLANKS);\n    (word.starts_with('/') && !word.contains(BLANKS)).then_some(word)",
     "    let word = text;\n    (word.starts_with('/') && !word.contains(BLANKS)).then_some(word)"),
    ("M4", "the query skips every Unicode blank, not upstream's four", PICKER,
     "    let word = text.trim_start_matches(BLANKS);\n    (word.starts_with('/') && !word.contains(BLANKS)).then_some(word)",
     "    let word = text.trim_start();\n    (word.starts_with('/') && !word.contains(BLANKS)).then_some(word)"),
    ("M5", "the commands are matched against the whole draft", PICKER,
     ".filter_map(|spec| found(spec, word))",
     ".filter_map(|spec| found(spec, query))"),
    ("M6", "the menu is keyed by the word, not the draft", PICKER,
     "            query: query.to_string(),\n            word: query.len() - word.len(),",
     "            query: word.to_string(),\n            word: 0,"),
    ("M7", "the budget ignores the composer's extra rows", PICKER,
     ".saturating_sub(FIXED_ROWS.saturating_add(input_extra))",
     ".saturating_sub(FIXED_ROWS)"),
    ("M8", "the band asks the menu's height with a one-row composer", SHELL,
     "Some(Slot::Menu(picker)) => picker.height(rows, input_rows.saturating_sub(1)),",
     "Some(Slot::Menu(picker)) => picker.height(rows, 0),"),
    ("M9", "the menu is painted for a one-row composer", SHELL,
     "                    self.geometry.input_rows().saturating_sub(1),\n                    self.palette,\n                    self.slash_menu_categories,",
     "                    0,\n                    self.palette,\n                    self.slash_menu_categories,"),
    ("M10", "a skill's token takes the blanks before the word", SHELL,
     "let start = self.picker.take().map_or(0, |picker| picker.word_start());",
     "let start = self.picker.take().map_or(0, |_| 0);"),
    ("M11", "the `/provider ` line keeps a slash dismissal", SHELL,
     "            let _ = self.dismissed.admits(None);",
     "            let _ = self.dismissed.admits(Trigger::of(self.editor.text()));"),
    ("M13", "the header reads the whole draft for a bare slash", PICKER,
     '        let left = if self.word() == "/" {',
     '        let left = if self.query == "/" {'),
    ("M12", "a command is recorded with the blanks before it", SHELL,
     "                text.len() - text.trim_start_matches([' ', '\\t', '\\r', '\\n']).len()\n",
     "                0\n"),
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
