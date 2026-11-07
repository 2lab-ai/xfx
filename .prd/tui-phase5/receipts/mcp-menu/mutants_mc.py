"""MCP-Mc (`.prd/tui-phase5/receipts/mcp-menu/README.md`): each mutant removes one guard (or one pinned term) in the
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
    ("one-row-not-led", [
        (MENU, "    let limit = width.saturating_sub(4).max(16);",
         "    let led = rows.len() > 1;\n    let limit = width.saturating_sub(4).max(16);"),
        (MENU, "            out.push(if first {", "            out.push(if first || !led {"),
    ], [T_MENU + "a_one_row_address_never_draws_a_wrapped_piece_as_a_header_row",
        T_MENU + "a_one_row_command_line_never_draws_a_wrapped_piece_as_a_variable_row"], True),
    ("question-undrawn-answered", [
        (MENU, "            if !self.question_drawn.get() {",
         "            if false && !self.question_drawn.get() {"),
    ], [T_MENU + "remove_and_reject_take_no_answer_before_a_frame_painted_the_question",
        T_SHELL + "a_burst_of_enters_in_one_read_answers_no_question_it_never_painted"], True),
    ("question-not-forgotten", [
        (MENU, "        self.question_drawn.set(false);\n    }\n",
         "    }\n"),
    ], [T_MENU + "remove_and_reject_take_no_answer_before_a_frame_painted_the_question"], True),
    ("question-identity-unchecked", [
        (MENU, "            if asked.as_ref() != Some(",
         "            if false && asked.as_ref() != Some("),
    ], [T_MENU + "a_question_whose_server_moved_to_the_other_file_is_cancelled"], True),
    ("held-enter-answers", [
        (SHELL, "            if held && confirming {", "            if false && held && confirming {"),
    ], [T_SHELL + "a_held_enter_after_a_choice_runs_reaches_no_other_question"], True),
    ("menu-keys-under-ctrl-o", [
        (SHELL, "        if self.full.is_open() {\n            return true;\n        }\n        let screen = menu.screen();",
         "        let screen = menu.screen();"),
    ], [T_SHELL + "the_menu_takes_no_key_while_the_full_transcript_covers_it"], True),
    ("resize-keeps-drawn", [
        (SHELL, "        self.forget_mcp_drawn();\n", ""),
    ], [T_SHELL + "an_approval_drawn_before_a_resize_is_not_taken_after_it"], True),
    ("blind-keeps-drawn", [
        (SHELL, "        if blind {\n            menu.forget_drawn();\n        }\n", ""),
    ], [T_SHELL + "an_approval_drawn_before_a_resize_is_not_taken_after_it"], True),
    ("restore-primary-keeps-drawn", [
        (LOOP, "                Err(refusal) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(refusal) => {\n"),
        (LOOP, "                Err(emit) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(emit) => {\n"),
    ], ["tui::event_loop::tests::a_menu_question_the_return_from_the_full_transcript_never_painted_takes_no_answer"], True),
    ("restore-primary-refusal-arm", [
        (LOOP, "                Err(refusal) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(refusal) => {\n"),
    ], ["tui::event_loop::tests::a_menu_question_the_return_from_the_full_transcript_never_painted_takes_no_answer"], True),
    ("restore-primary-emit-arm", [
        (LOOP, "                Err(emit) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(emit) => {\n"),
    ], ["tui::event_loop::tests::a_menu_question_the_return_from_the_full_transcript_never_painted_takes_no_answer"], True),
    ("skills-both", [
        (SHELL, "            && self.mcp_menu.is_none()\n            && self.help_menu.is_none()",
         "            && self.help_menu.is_none()"),
        (SHELL, "            menu.set_filter(self.editor.text());\n            self.picker = None;\n            self.file_picker = None;\n            self.provider_picker = None;\n            self.mcp_args = None;\n            self.skills_menu = None;\n",
         "            menu.set_filter(self.editor.text());\n            self.picker = None;\n            self.file_picker = None;\n            self.provider_picker = None;\n            self.mcp_args = None;\n"),
    ], [T_SHELL + "a_dollar_typed_into_the_menus_filter_opens_no_skills_menu_behind_it"], True),
    ("kept-ignores-digest", [
        (RT, "                    && server.entry.as_ref().map(|entry| entry.digest.as_str())\n                        == shown_entry_digest(&config, &index)\n",
         ""),
    ], [T_SHELL + "a_project_entry_respelled_is_approved_from_the_menu"], True),
    ("expansion-single-quoted", [
        (RT, "    if let Some(quoted) = expanded_word(word) {",
         "    if let Some(quoted) = expanded_word(word).filter(|_| false) {"),
    ], ["mcp::runtime::tests::a_word_xfx_expands_is_shown_as_a_shell_reads_an_expansion"], True),
    ("cut-row-counts-whole", [
        (MENU, "                whole: fits && lines.len()", "                whole: (fits || true) && lines.len()"),
    ], [T_MENU + "a_line_wider_on_the_screen_than_it_was_measured_is_not_drawn_whole"], True),
    ("dollar-opens-skills", [
        (SHELL, "            && self.mcp_menu.is_none()\n            && self.help_menu.is_none()",
         "            && self.help_menu.is_none()"),
    ], [T_SHELL + "a_dollar_typed_into_the_menus_filter_opens_no_skills_menu_behind_it"], True),
    ("reconcile-keeps-skills", [
        (SHELL, "            menu.set_filter(self.editor.text());\n            self.picker = None;\n            self.file_picker = None;\n            self.provider_picker = None;\n            self.mcp_args = None;\n            self.skills_menu = None;\n",
         "            menu.set_filter(self.editor.text());\n            self.picker = None;\n            self.file_picker = None;\n            self.provider_picker = None;\n            self.mcp_args = None;\n"),
    ], [T_SHELL + "a_dollar_typed_into_the_menus_filter_opens_no_skills_menu_behind_it"], False),
    # Pins (item 9).
    ("approval-below-min-cols", [
        (MENU, "&& width >= APPROVAL_MIN_COLS,", "&& (width >= APPROVAL_MIN_COLS || true),"),
    ], [T_MENU + "an_approval_below_twenty_columns_approves_nothing"], True),
    ("drawn-not-reset-on-open", [
        (MENU, "                        self.answer_no = false;\n                        *self.drawn.borrow_mut() = None;\n",
         "                        self.answer_no = false;\n"),
    ], [T_MENU + "opening_an_approval_again_forgets_the_one_drawn_before"], True),
    ("drawn-any-name", [
        (MENU, "drawn.filter(|drawn| drawn.name == row.name)", "drawn.filter(|drawn| true || drawn.name == row.name)"),
    ], [T_MENU + "an_approval_drawn_for_one_server_approves_no_other"], True),
    ("tool-name-unspelled", [
        (MENU, "        out.push_str(&ellipsized(&command::safe_within(&tool.name, 4096), 20));",
         "        out.push_str(&ellipsized(&tool.name, 20));"),
    ], [T_MENU + "every_name_target_error_and_tool_is_spelled_and_a_name_stops_at_its_equals_sign"], True),
    ("remove-without-file", [
        (CMD, "    remove(runtime, name, Some(file), &mut said);",
         "    let _ = file;\n    remove(runtime, name, None, &mut said);"),
    ], [T_SHELL + "remove_in_the_menu_removes_from_the_file_its_question_named_when_both_hold_the_name"], True),
    ("up-down-flip-answer", [
        (MENU, "                    Key::Left if choice.asks_on_row() => self.answer_no = false,",
         "                    Key::Up | Key::Down if choice.asks_on_row() => self.answer_no = !self.answer_no,\n                    Key::Left if choice.asks_on_row() => self.answer_no = false,"),
    ], [T_MENU + "up_and_down_on_a_question_leave_its_answer"], True),
    ("printable-answers-yes", [
        (SHELL, "            Input::Text(_) if confirming => Key::Escape,",
         "            Input::Text(_) if confirming => Key::Enter,"),
    ], [T_SHELL + "remove_and_reject_ask_in_place_and_anything_but_yes_changes_nothing_on_disk"], True),
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
