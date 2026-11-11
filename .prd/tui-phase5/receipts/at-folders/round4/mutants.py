"""AT-FOLDERS round 4's mutants: each rule this round adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit, in their own process group. A mutant the tests fail is
KILLED. (The runner is AT-HEIGHT's, `../../at-height/mutants.py`; rounds 1-3's
are `../mutants.py`, `../round2/mutants.py`, `../round3/mutants.py`. Round 2's
`ctrl-o-holds` and round 3's `ctrl-o-never-relists` are re-texted here as
`hold-ignores-eligibility` and `eligible-never-relists`; round 3's
`first-frame-retells-title` is dropped with the change it broke.)

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
    "tui::frame::tests",
]
LIMIT_S = 900

SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"
FI = "src/tui/file_index.rs"
FR = "src/tui/frame.rs"

MUTANTS = [
    # item 1: one eligibility predicate, asked at every acquisition site
    ("poll-site-ungated", SH, "        if !eligible {\n            files.mark_lookup_dirty();\n            return;\n        }\n", ""),
    ("tab-site-ungated", SH, "None if eligible && files.shown() == file_picker::Shown::Unavailable => {", "None if files.shown() == file_picker::Shown::Unavailable => {"),
    ("open-site-ungated", SH, "            let mut files = FilePicker::open(query);\n            if self.file_picker_eligible() {", "            let mut files = FilePicker::open(query);\n            if true || self.file_picker_eligible() {"),
    ("episode-not-owed", SH, "                if episode {\n                    files.owe_episode();\n                }\n", ""),
    ("episode-not-begun", SH, "            if episode {\n                self.files.begin_episode();\n            }\n            if episode || dirty {", "            if episode || dirty {"),
    ("eligible-never-relists", SH, "            if episode || dirty {\n                self.refresh_file_picker();", "            if false {\n                self.refresh_file_picker();"),
    ("hold-ignores-eligibility", SH, "            && files.shown() == file_picker::Shown::Loading\n            && self.file_picker_eligible();\n", "            && files.shown() == file_picker::Shown::Loading;\n"),
    ("abandon-ignores-eligibility", SH, "        if !self.file_picker_eligible() {\n            if self.files.abandon_listing() {", "        if false {\n            if self.files.abandon_listing() {"),
    ("predicate-drops-transcript", SH, "            && !self.full.is_open()\n            && self.help_menu.is_none()", "            && self.help_menu.is_none()"),
    ("predicate-drops-skills", SH, "            && !self.skills_menu.as_ref().is_some_and(SkillsMenu::visible)\n", ""),
    ("enter-ignores-eligibility", SH, "Action::Submit if !eligible => return false,", "Action::Submit if false => return false,"),
    # item 2: cooperative cancel
    ("cancel-not-wired", FI, "    list_directory_until(directory, || stop.load(Atomic::Relaxed))", "    list_directory_until(directory, || false)"),
    ("cancel-not-between-entries", FI, "    for entry in entries.flatten() {\n        if stopped() {\n            return Err(());\n        }\n", "    for entry in entries.flatten() {\n"),
    ("abandon-does-not-stop", FI, "        let Some(listing) = self.listing.take() else {\n            return false;\n        };\n        listing.stop.store(true, Atomic::Relaxed);\n", "        let Some(_listing) = self.listing.take() else {\n            return false;\n        };\n"),
    # item 4: the held turn's bell rings once
    ("bells-never-drained", EL, "    let rings = shell.take_bells();\n    if rings == 0 {\n        return Ok(());\n    }\n", "    let rings = shell.take_bells();\n    if rings == 0 {\n        return Ok(());\n    }\n    shell.restore_bells(rings);\n"),
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
