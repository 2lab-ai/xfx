"""SLASH-ARG-SKILLS' mutants: each takes one of the unit's rules out of the
code head and runs the tests that pin it; a mutant is killed when one fails.
(SLASH-LEADING-WS' mutants.py, with this unit's rules.)

Run on a throwaway `git clone --no-local` checked out at the code head, never
on the worktree: the clone's source is edited in place and restored after
each mutant.

usage: python3 -I mutants.py <clone> [ID...]   (TMPDIR set; cargo on PATH)
Prints one line per mutant and `killed N of M`; exits 1 when one survives.
"""
import os
import subprocess
import sys

LISTS = "src/tui/arg_lists.rs"
ARGS = "src/tui/mcp_args.rs"
SHELL = "src/tui/shell.rs"
SHELL_TESTS = [
    "tui::shell::tests::statusline_lists_its_items_under_the_argument_and_enter_runs_the_marked_one",
    "tui::shell::tests::sound_lists_fxs_levels_and_enter_runs_the_marked_one",
    "tui::shell::tests::permissions_lists_its_six_forms_and_tab_writes_the_marked_one",
    "tui::shell::tests::workspace_lists_its_verbs_and_tab_leaves_room_for_a_path",
    "tui::shell::tests::allowlist_walks_its_words_under_the_one_being_typed",
    "tui::shell::tests::one_dismissal_closes_the_slash_menu_and_every_argument_list_for_the_line",
    "tui::shell::tests::a_dismissal_lasts_across_a_mcp_line_whose_kind_is_still_slash",
    "tui::shell::tests::tab_writes_the_marked_line_and_escape_closes_it_for_the_line",
    "tui::shell::tests::enter_on_a_mcp_line_submits_the_marked_completion",
    "tui::shell::tests::a_recalled_mcp_line_opens_no_completions_until_it_is_edited",
    "tui::shell::tests::a_mcp_line_opens_its_completions_under_the_argument_and_keeps_the_hint",
    # Round 2: an older case the list's Enter rewrote (`/permissions full`),
    # and the two new ones.
    "tui::shell::tests::an_argument_nobody_understands_is_the_usage_line_and_changes_nothing",
    "tui::shell::tests::a_caret_move_keeps_a_tables_mark_and_an_edit_puts_it_back_on_the_first_row",
    "tui::shell::tests::under_the_full_transcript_tab_and_enter_take_the_lists_marked_row_as_fxs_do",
]
TESTS = ["tui::arg_lists::", "tui::mcp_args::"] + SHELL_TESTS

# (id, what it takes out, file, old, new)
MUTANTS = [
    ("A1", "a table line opens no list (the base's rule)", SHELL,
     "        self.mcp_args = McpArgs::fixed(text, Instant::now());\n",
     "        self.mcp_args = None;\n"),
    ("A2", "a word is matched with its case", LISTS,
     "                    .is_some_and(|head| head.eq_ignore_ascii_case(self.query))",
     "                    .is_some_and(|head| head == self.query)"),
    ("A3", "the four tables trim the argument's start only", LISTS,
     "        let query = argument(prefix, command)?.trim_end_matches(SPACES);",
     "        let query = argument(prefix, command)?;"),
    ("A4", "a tab is not a boundary", LISTS,
     "const SPACES: [char; 2] = [' ', '\\t'];",
     "const SPACES: [char; 2] = [' ', ' '];"),
    ("A5", "the rows hang past the head but not the draft's blanks", ARGS,
     "            token_start: text.len() - prefix.len() + list.head.len(),",
     "            token_start: list.head.len(),"),
    ("A6", "/allowlist's scopes are not walked", LISTS,
     ".find(|scope| top && is(scope))",
     ".find(|_| false)"),
    ("A7", "/allowlist's `tool ` step is not walked", LISTS,
     "(tool, Some(name)) if tool.eq_ignore_ascii_case(\"tool\") =>",
     "(tool, Some(name)) if tool.is_empty() && name.is_empty() =>"),
    ("A8", "Tab writes no space after a line that takes more", ARGS,
     "            .is_some_and(|spaced| spaced[self.matching[self.selected]]);",
     "            .is_some_and(|_| false);"),
    ("A9", "Enter submits what was typed, not the marked line", SHELL,
     "                if let Some(line) = args.chosen().map(str::to_string) {\n                    if line != self.editor.text() {",
     "                if let Some(line) = args.chosen().filter(|_| false).map(str::to_string) {\n                    if line != self.editor.text() {"),
    ("A10", "Escape over a list keeps a latch of its own, not the slash kind's", SHELL,
     "                self.mcp_args = None;\n                self.dismissed.dismiss(Trigger::Slash);",
     "                self.mcp_args = None;"),
    ("A11", "a `/mcp ` list ignores the slash dismissal", SHELL,
     "        if !self.dismissed.admits(Trigger::of(text)) || self.recalled_slash {",
     "        if self.recalled_slash {"),
    ("A12", "a table is read again as `/mcp `'s servers are", ARGS,
     "        self.spaced.is_none() && now.saturating_duration_since(self.read_at) >= REFRESH",
     "        now.saturating_duration_since(self.read_at) >= REFRESH"),
    ("A13", "a recalled table line opens its list", SHELL,
     "        if self.recalled_slash {\n            self.picker = None;\n            self.mcp_args = None;\n            return;\n        }",
     "        if self.recalled_slash && McpArgs::fixed(self.editor.text(), Instant::now()).is_none() {\n            self.picker = None;\n            self.mcp_args = None;\n            return;\n        }"),
    ("A14", "a row shows its whole line, not the label", ARGS,
     "            label_at: list.head.len(),",
     "            label_at: 0,"),
    ("A15", "a table is rebuilt on every keystroke, a caret move included", SHELL,
     "            .is_some_and(|open| open.is_table() && open.text() == text)",
     "            .is_some_and(|_| false)"),
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
