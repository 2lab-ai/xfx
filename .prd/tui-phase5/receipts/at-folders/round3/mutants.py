"""AT-FOLDERS round 3's mutants: each rule this unit adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit, in their own process group. A mutant the tests fail is
KILLED. (The runner is AT-HEIGHT's, `../../at-height/mutants.py`; round 1's
and round 2's are `../mutants.py` and `../round2/mutants.py`.)

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
    "tui::frame::tests",
]
LIMIT_S = 900

SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"
FI = "src/tui/file_index.rs"
FR = "src/tui/frame.rs"

MUTANTS = [
    # item 1: the bells wait with a held frame
    ("held-tick-rings", EL, "    if frame_held {\n        return Ok(());\n    }\n    commit_bells(shell, out, failures, now)\n", "    commit_bells(shell, out, failures, now)\n"),
    # the title told before the band's first frame
    ("first-frame-retells-title", FR, "            if told.is_some() {\n                self.shown_title = told;\n            }\n", ""),
    # item 2: fx's ineligible lifecycle
    ("ctrl-o-keeps-listing", SH, "            if self.files.abandon_listing() {", "            if false && self.files.abandon_listing() {"),
    ("ctrl-o-never-relists", SH, ".is_some_and(FilePicker::take_lookup_dirty)", ".is_some_and(|_| false)"),
    ("ctrl-o-not-marked", SH, "                    files.mark_lookup_dirty();\n", ""),
    ("abandon-keeps-the-answer", FI, "        let Some(listing) = self.listing.take() else {", "        let Some(listing) = self.listing.as_ref() else {"),
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
