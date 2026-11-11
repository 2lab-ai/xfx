"""AT-FOLDERS round 2's mutants: each rule this unit adds, broken one at a time in a
throwaway clone (`git clone --no-local`), and the unit's tests run against it
with a time limit, in their own process group. A mutant the tests fail is
KILLED. (The runner is AT-HEIGHT's, `../../at-height/mutants.py`; round 1's
mutants are `../mutants.py`.)

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
    "tui::event_loop::tests::a_program_status",
    "tui::event_loop::tests::the_launchs_report",
    "tui::event_loop::tests::a_title_its_own_tick",
    "tui::event_loop::tests::owed_bells",
    "tui::shell::tests::under_the_full_transcript_a_folder_listing",
]
LIMIT_S = 900

FP = "src/tui/file_picker.rs"
SH = "src/tui/shell.rs"
EL = "src/tui/event_loop.rs"
CK = "src/tui/check.rs"

MUTANTS = [
    # item 1: only the frame is held
    ("whole-tick-held", EL, "    if !held {\n        shell.settle_column(now);\n    }\n", "    if held {\n        return Ok(());\n    }\n    shell.settle_column(now);\n"),
    ("held-writes-the-frame", EL, "    if held {\n        let primary = !band.on_alternate()", "    if false && held {\n        let primary = !band.on_alternate()"),
    ("held-title-not-written", EL, "        return if primary {\n            write_title_alone(band, out, failures, now)\n", "        return if primary {\n            Ok(())\n"),
    ("title-alone-not-recorded", EL, "    band.title_landed();\n", ""),
    ("title-intent-unchecked", CK, "        Intent::Title(title) => Metadata {\n            shown_title: Some((*title).to_string()),\n", "        Intent::Title(_) => Metadata {\n            shown_title: seed.shown_title.clone(),\n"),
    # round 1's whose text round 2 moved (`../mutants.py` reports them BROKEN at this head)
    ("loop-never-holds", EL, "    let held = shell.hold_frame_for_file_listing(now);\n", "    let held = shell.hold_frame_for_file_listing(now) && false;\n"),
    ("hold-ignores-indexed", SH, "        let in_flight = !files.indexed()\n", "        let in_flight = true\n"),
    ("hold-ignores-loading", SH, "            && files.shown() == file_picker::Shown::Loading\n", "            && true\n"),
    # item 2: no hold under the full transcript
    ("ctrl-o-holds", SH, "            && files.shown() == file_picker::Shown::Loading\n            && !self.full.is_open();\n", "            && files.shown() == file_picker::Shown::Loading;\n"),
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
