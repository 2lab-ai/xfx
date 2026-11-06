"""MCP-Me (`.prd/tui-phase5/receipts/mcp-menu/README.md`): each mutant removes one guard (or one pinned term) in the
throwaway clone, runs the tests that should catch it, records whether they
failed, and restores the files. Never run on the worktree."""
import os
import subprocess
import sys

root = sys.argv[1]
assert "/scratchpad/" in root, root
MENU = "src/tui/mcp_menu.rs"
SHELL = "src/tui/shell.rs"
LOOP = "src/tui/event_loop.rs"
RT = "src/mcp/runtime.rs"
CMD = "src/mcp/command.rs"
T_MENU = "tui::mcp_menu::tests::"
T_SHELL = "tui::shell::tests::"

M = [
    ("swallow-does-not-count", [
        (SHELL, "        self.last = Some(now);\n        if !held {",
         "        if !held {\n            self.last = Some(now);\n        }\n        if !held {"),
    ], [T_SHELL + "a_held_enter_after_a_choice_runs_reaches_no_other_question",
        T_SHELL + "a_swallowed_enter_keeps_the_window_open_and_is_said_once"], True),
    ("default-control-as-piece", [
        (RT, "            out.quoted(default);", "            out.text(default);"),
    ], ["mcp::runtime::tests::a_control_character_beside_an_expansion_is_a_piece_of_its_own"], True),
    ("ellipsis-alone-whole", [
        (MENU, "if left >= 2 || (left >= 1 && cells(&name) <= left) {", "if left >= 1 {"),
    ], [T_MENU + "a_name_cut_to_its_ellipsis_alone_is_not_a_question_drawn_whole"], True),
    ("short-name-cut", [
        (MENU, "if left >= 2 || (left >= 1 && cells(&name) <= left) {", "if left >= 2 {"),
    ], [T_MENU + "a_name_cut_to_its_ellipsis_alone_is_not_a_question_drawn_whole"], True),
    ("skills-word-zz", [
        (SHELL, '        const WORD: &str = "z:";', '        const WORD: &str = "zz";'),
    ], [T_SHELL + "a_space_ends_a_dollar_search_and_a_word_nothing_matches_shows_no_menu"], True),
]

only = set(sys.argv[2:])
for name, edits, tests, exact in M:
    if only and name not in only:
        continue
    saved = {}
    try:
        for path, old, new in edits:
            f = os.path.join(root, path)
            src = saved.get(f) or open(f).read()
            saved.setdefault(f, src)
            cur = open(f).read()
            assert cur.count(old) == 1, (name, path, cur.count(old))
            open(f, "w").write(cur.replace(old, new))
        cmd = ["cargo", "test", "--locked", "--lib", "--"] + (["--exact"] if exact else []) + tests
        p = subprocess.run(cmd, cwd=root, capture_output=True, text=True, timeout=1200)
        out = p.stdout + p.stderr
        compiled = "error[" not in out and "could not compile" not in out
        failed = sorted({line.split()[1] for line in out.splitlines()
                         if line.startswith("test ") and line.endswith("... FAILED")})
        verdict = "CAUGHT" if (compiled and p.returncode != 0 and failed) else (
            "COMPILE-ERROR" if not compiled else "MISSED")
        print(f"{name}: {verdict} exit={p.returncode} failed={failed}", flush=True)
        if not compiled:
            print(out[-3000:], flush=True)
    finally:
        for f, src in saved.items():
            open(f, "w").write(src)
