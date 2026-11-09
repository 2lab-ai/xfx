"""AT-FOLDERS round 7's mutants: each rule this round adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit, in their own process group. A mutant the tests fail is
KILLED. (The runner is AT-HEIGHT's, `../../at-height/mutants.py`; rounds 1-6's
are `../mutants.py` and `../round2/` to `../round6/mutants.py`; round 6's
six whose text this round rewrote are re-texted here under their names, or
as `unchanged-access-refreshes` for `unchanged-roots-refresh`.)

usage: python3 -I mutants.py <clone> [<mutant id>...]
The clone must be at the unit's head with no change; each mutant is restored
with `git checkout -- <file>` in the clone before the next. Prints one line per
mutant and `mutants: N run, K killed`.
"""

import os
import signal
import subprocess
import sys

sys.dont_write_bytecode = True

TESTS = [
    "tui::file_picker::tests",
    "tui::file_index::tests",
    "tui::event_loop::tests::the_frame_of_a_keystroke_that_lists_a_folder",
    "tui::event_loop::tests::a_key_the_list_hands_on_mid_resize",
    "tui::event_loop::tests::a_key_spent_mid_resize",
    "tui::shell::tests::a_later_at_puts_the_rows",
    "tui::shell::tests::a_directory_choice",
    "tui::shell::tests::enter_on_a_folder_row",
    "tui::shell::tests::tab_on_the_folders_own_row",
    "tui::shell::tests::a_folder_with_a_space",
    "tui::shell::tests::a_keystroke_that_lists_a_folder",
    "tui::shell::tests::only_a_folder_listing_holds",
    "tui::shell::tests::after_a_slow_folder",
    "tui::shell::tests::enter_puts_the_path",
    "tui::shell::tests::an_unmatched_mention",
    "tui::check::tests::a_title_on_its_own",
    "tui::event_loop::tests::a_listing_still_out_at_8_ms",
    "tui::event_loop::tests::a_frame_held_for_a_folder",
    "tui::event_loop::tests::a_held_tick_that_owes_a_title",
    "tui::event_loop::tests::a_program_status",
    "tui::event_loop::tests::the_launchs_report",
    "tui::event_loop::tests::a_title_its_own_tick",
    "tui::event_loop::tests::owed_bells",
    "tui::shell::tests::under_the_full_transcript_a_folder_listing",
    "tui::shell::tests::a_listing_out_when_ctrl_o_opens",
    "tui::shell::tests::no_listing_starts_while_the_list_is_not_eligible",
    "tui::shell::tests::tab_under_the_full_transcript_asks_for_no_read",
    "tui::shell::tests::an_at_word_typed_under_the_full_transcript",
    "tui::shell::tests::an_at_word_under_a_shown_skills_menu",
    "tui::shell::tests::enter_under_the_full_transcript",
    "tui::shell::tests::a_folder_query_opens_with_its_listing",
    "tui::shell::tests::an_indexed_query_opens_with_one_walk",
    "tui::shell::tests::a_folder_query_edited_into_an_indexed_one",
    "tui::shell::tests::an_episode_opened_under_the_transcript",
    "tui::shell::tests::tab_retries_a_failed_index",
    "tui::shell::tests::tab_under_the_full_transcript",
    "tui::shell::tests::enter_under_a_mention_menu",
    "tui::shell::tests::a_dollar",
    "tui::shell::tests::a_new_episode_during_a_walk",
    "tui::shell::tests::two_requests_during_one_walk",
    "tui::shell::tests::a_failed_walk_with_a_refresh_queued",
    "tui::shell::tests::a_workspace_change_under_an_open_indexed_list",
    "tui::shell::tests::no_listing_starts_while",
    "tui::shell::tests::a_change_of_roots_during_a_walk",
    "tui::shell::tests::a_workspace_change",
    "tui::frame::tests",
]
LIMIT_S = 900

SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"
FI = "src/tui/file_index.rs"
FR = "src/tui/frame.rs"
FP = "src/tui/file_picker.rs"

MUTANTS = [
    # item 1: a walk that starts takes the queued refresh
    ("start-keeps-the-pending", FI, "        // A walk of the current roots is what any queued refresh asks for.\n        self.pending_refresh = false;\n", ""),
    ("superseded-never-set", FI, "            walk.stop.store(true, Atomic::Relaxed);\n            self.superseded = true;\n", "            walk.stop.store(true, Atomic::Relaxed);\n"),
    ("superseded-never-cleared", FI, "            // A walk a change of roots stopped is over by now.\n            self.superseded = false;\n", ""),
    # item 2: fx's install
    ("no-install-refresh", SH, "                files.invalidate();\n            }\n            self.files.refresh_index();\n", "                files.invalidate();\n            }\n"),
    ("no-invalidation", SH, "            if let Some(files) = self.file_picker.as_mut() {\n                files.invalidate();\n            }\n", ""),
    ("trigger-on-the-roots-only", SH, "        let changed = self.workspace_access != access;", "        let changed = index_roots(&self.workspace, &self.workspace_access) != index_roots(&self.workspace, &access);"),
    ("listing-not-stopped", SH, "            self.files.abandon_listing();\n            self.files.begin_episode();\n", "            self.files.begin_episode();\n"),
    ("folder-not-read-again", SH, "            self.files.abandon_listing();\n            self.files.begin_episode();\n", "            self.files.abandon_listing();\n"),
    ("invalidate-keeps-the-bar", FP, "    pub(crate) fn invalidate(&mut self) {\n        self.selected = 0;\n", "    pub(crate) fn invalidate(&mut self) {\n"),
    ("invalidate-keeps-the-window", FP, "        self.selected = 0;\n        self.window_start = 0;\n        self.trusted = false;", "        self.selected = 0;\n        self.trusted = false;"),
    ("invalidate-keeps-trust", FP, "        self.window_start = 0;\n        self.trusted = false;\n        self.refresh_requested = true;\n", "        self.window_start = 0;\n        self.refresh_requested = true;\n"),
    # round 6's six whose text round 7 rewrote, re-texted
    ("drop-the-pending-refresh", FI, "        if self.walk.is_some() || self.superseded {\n            self.pending_refresh = true;\n        } else {", "        if self.walk.is_some() || self.superseded {\n        } else {"),
    ("never-coalesce", FI, "        if self.walk.is_some() || self.superseded {\n            self.pending_refresh = true;\n        } else {", "        if (self.walk.is_some() || self.superseded) && !self.pending_refresh {\n            self.pending_refresh = true;\n        } else {"),
    ("pending-never-cleared", FI, "            if self.pending_refresh {\n                self.start_walk();", "            if self.pending_refresh {\n                self.start_walk();\n                self.pending_refresh = true;"),
    ("workspace-asks-no-refresh", FP, "        self.trusted = false;\n        self.refresh_requested = true;\n    }", "        self.trusted = false;\n    }"),
    ("workspace-looks-nothing-up", SH, "            self.files.refresh_index();\n            self.refresh_file_picker();\n        }\n        if self.workspace_menu.is_some() {", "            self.files.refresh_index();\n        }\n        if self.workspace_menu.is_some() {"),
    ("unchanged-access-refreshes", SH, "        self.workspace_access = access;\n        if changed {", "        self.workspace_access = access;\n        if true || changed {"),
    # item 3: the list's work stays behind eligibility
    ("prepare-index-from-adopt", SH, "            self.files.refresh_index();\n            self.refresh_file_picker();\n        }\n        if self.workspace_menu.is_some() {", "            self.files.refresh_index();\n            if let Some(files) = self.file_picker.as_mut() {\n                prepare_index(&mut self.files, files);\n            }\n            self.refresh_file_picker();\n        }\n        if self.workspace_menu.is_some() {"),
]


def run(clone, args, **kw):
    return subprocess.run(args, cwd=clone, **kw)


def main():
    clone = os.path.realpath(sys.argv[1])
    wanted = set(sys.argv[2:])
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    status = run(clone, ["git", "status", "--porcelain"], capture_output=True, text=True, env=env)
    if status.stdout.strip():
        sys.exit(f"the clone is not clean:\n{status.stdout}")
    ran = killed = 0
    for mid, path, old, new in MUTANTS:
        if wanted and mid not in wanted:
            continue
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        if text.count(old) != 1:
            print(f"BROKEN   {mid}: the text to mutate is in {path} {text.count(old)} times")
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
        ran += 1
        try:
            cmd = ["cargo", "test", "--locked", "--lib", "--", *TESTS]
            # Its own process group, so a run past the limit is stopped with
            # every process it started, and nothing else.
            child = subprocess.Popen(
                cmd, cwd=clone, env=env, text=True, start_new_session=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            try:
                out, _ = child.communicate(timeout=LIMIT_S)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.communicate()
                raise
            done = subprocess.CompletedProcess(cmd, child.returncode)
            if done.returncode == 0:
                verdict = "SURVIVED"
            else:
                killed += 1
                verdict = "KILLED"
            failing = [line.split()[1] for line in out.splitlines() if line.startswith("test ") and line.endswith("FAILED")]
            if done.returncode != 0 and not failing:
                failing = ["(no test ran: " + next((l for l in out.splitlines() if l.startswith("error")), "build failed") + ")"]
            print(f"{verdict:8} {mid}: {', '.join(failing) or 'every test passed'}", flush=True)
        except subprocess.TimeoutExpired:
            killed += 1
            print(f"KILLED   {mid}: timed out after {LIMIT_S} s", flush=True)
        finally:
            run(clone, ["git", "checkout", "--", path], env=env, check=True)
    print(f"mutants: {ran} run, {killed} killed")


if __name__ == "__main__":
    main()
