"""MCP-Md (`.prd/tui-phase5/receipts/mcp-menu/README.md`): each mutant removes one guard (or one pinned term) in the
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

CFG = "src/mcp/config.rs"
CMDT = "src/mcp/command.rs"
M = [
    ("one-row-early-return", [
        (MENU, "        if window == 0 {\n            return out;\n        }\n", ""),
    ], [T_MENU + "a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer"], True),
    ("one-row-rows-forget", [
        (MENU, "        if out.len() > count {\n            self.forget_drawn();\n        }\n", ""),
    ], [T_MENU + "a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer"], True),
    ("one-row-both", [
        (MENU, "        if window == 0 {\n            return out;\n        }\n", ""),
        (MENU, "        if out.len() > count {\n            self.forget_drawn();\n        }\n", ""),
    ], [T_MENU + "a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer"], True),
    ("one-row-saturating", [
        (MENU, "            lines = lines.saturating_sub(1);", "            lines -= 1;"),
    ], [T_MENU + "a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer"], True),
    ("one-row-all", [
        (MENU, "        if window == 0 {\n            return out;\n        }\n", ""),
        (MENU, "        if out.len() > count {\n            self.forget_drawn();\n        }\n", ""),
        (MENU, "            lines = lines.saturating_sub(1);", "            lines -= 1;"),
    ], [T_MENU + "a_panel_of_the_title_alone_draws_no_question_and_takes_no_answer"], True),
    ("cut-question-counts-whole", [
        (MENU, "        return (ellipsized(&spelled(&question(row, choice)), room), false);",
         "        return (ellipsized(&spelled(&question(row, choice)), room), true);"),
    ], [T_MENU + "remove_keeps_the_file_it_acts_on_whole_and_shortens_a_long_name"], True),
    ("name-not-shortened", [
        (MENU, "            let name = ellipsized(&spelled(&command::typed(&row.name)), room - fixed);\n            return (format!(\"{head}{name}{tail}\"), true);",
         "            let _ = fixed;\n            return (ellipsized(&spelled(&question(row, choice)), room), true);"),
    ], [T_MENU + "remove_keeps_the_file_it_acts_on_whole_and_shortens_a_long_name"], True),
    ("control-inside-quotes", [
        (RT, "            if character.is_control() {\n                if self.quoted {",
         "            if false && character.is_control() {\n                if self.quoted {"),
    ], ["mcp::runtime::tests::a_control_character_beside_an_expansion_is_a_piece_of_its_own"], True),
    ("control-in-default-quoted", [
        (RT, "        if default.is_some_and(|default| default.chars().any(char::is_control)) {",
         "        if false && default.is_some_and(|default| default.chars().any(char::is_control)) {"),
    ], ["mcp::runtime::tests::a_control_character_beside_an_expansion_is_a_piece_of_its_own"], True),
    ("full-transcript-keeps-drawn", [
        (SHELL, "        // while it is up, nor after it closes until a frame paints it (opus-Y).\n        self.forget_mcp_drawn();\n",
         "        // while it is up, nor after it closes until a frame paints it (opus-Y).\n"),
    ], [T_SHELL + "the_full_transcript_over_a_question_leaves_it_unpainted_until_a_frame"], True),
    ("panel-keeps-drawn", [
        (SHELL, "        // until a frame paints it again (opus-Y).\n        self.forget_mcp_drawn();\n",
         "        // until a frame paints it again (opus-Y).\n"),
    ], [T_SHELL + "a_model_question_over_the_menu_leaves_its_question_unpainted_until_a_frame"], True),
    ("other-surfaces-not-counted", [
        (SHELL, "&& self.mcp_menu.is_some())",
         "&& self.mcp_menu.is_some() && self.ask.is_none() && !self.asking())"),
    ], [T_SHELL + "an_enter_another_surface_took_counts_for_the_held_rule"], True),
    ("tap-restarts-window", [
        (SHELL, "        if !enter.repeat {", "        if false && !enter.repeat {"),
    ], [T_SHELL + "a_tap_the_menu_swallowed_keeps_no_window_open_and_is_said_once"], True),
    ("repeat-restarts-nothing", [
        (SHELL, "        if !enter.repeat {", "        if true || !enter.repeat {"),
    ], [T_SHELL + "a_held_enter_after_a_choice_runs_reaches_no_other_question"], True),
    ("swallow-said-every-time", [
        (SHELL, "        !std::mem::replace(&mut self.said, true)", "        let _ = &mut self.said;\n        true"),
    ], [T_SHELL + "a_tap_the_menu_swallowed_keeps_no_window_open_and_is_said_once"], True),
    ("swallow-unsaid", [
        (SHELL, "        !std::mem::replace(&mut self.said, true)", "        let _ = &mut self.said;\n        false"),
    ], [T_SHELL + "a_tap_the_menu_swallowed_keeps_no_window_open_and_is_said_once",
        T_SHELL + "an_enter_another_surface_took_counts_for_the_held_rule"], True),
    ("asking-keeps-drawn", [
        (MENU, "                        self.question = Some((row.name.clone(), row.file()));\n                        self.question_drawn.set(Asked::NotDrawn);\n",
         "                        self.question = Some((row.name.clone(), row.file()));\n"),
    ], [T_MENU + "a_question_asked_again_waits_for_a_frame_of_its_own"], True),
    ("continued-fits", [
        (MENU, "            ConfirmLine::Continued(text) => 2 + cells(&format!(\"\\u{21b3} {text}\")) <= width,",
         "            ConfirmLine::Continued(_) => true,"),
    ], [T_MENU + "a_wrapped_piece_wider_on_the_screen_than_it_was_measured_is_not_drawn_whole"], True),
    ("workspace-unwritten", [
        (CFG, "                config.written = Some(Written::of(raw));\n", "                let _ = raw;\n"),
    ], ["mcp::command::tests::an_approved_project_servers_variables_are_shown_as_written",
        T_SHELL + "the_menu_shows_an_approved_project_servers_variables_as_written"], True),
    ("menu-url-live", [
        (MENU, "                        Transport::Http | Transport::Sse => config.shown_url().to_string(),",
         "                        Transport::Http | Transport::Sse => config.url.clone(),"),
    ], [T_SHELL + "the_menu_shows_an_approved_project_servers_variables_as_written"], True),
    ("show-url-live", [
        (CMDT, "safe(config.shown_url())", "safe(&config.url)"),
    ], ["mcp::command::tests::an_approved_project_servers_variables_are_shown_as_written"], True),
    ("approve-live", [
        (CMDT, "                Transport::Http | Transport::Sse => (\"connects to\", config.shown_url().to_string()),",
         "                Transport::Http | Transport::Sse => (\"connects to\", config.url.clone()),"),
    ], ["mcp::command::tests::an_approved_project_servers_variables_are_shown_as_written"], True),
    ("menu-command-live", [
        (MENU, "                            let (program, args) = config.shown_command();",
         "                            let (program, args) = (&config.command, &config.args);"),
    ], [T_SHELL + "the_menu_shows_a_profile_servers_variable_as_written",
        T_SHELL + "the_menu_shows_an_approved_project_servers_variables_as_written"], True),
    ("restore-primary-keeps-drawn", [
        (LOOP, "                Err(refusal) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(refusal) => {\n"),
        (LOOP, "                Err(emit) => {\n                    shell.forget_mcp_drawn();\n",
         "                Err(emit) => {\n"),
    ], ["tui::event_loop::tests::a_menu_question_the_return_from_the_full_transcript_never_painted_takes_no_answer"], True),
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
