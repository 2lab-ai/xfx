#!/usr/bin/env python3
"""CI-PTY-WAIT's mutants, each run on a throwaway clone, never the worktree.

    python3 -I mutants.py <clone> <cargo-target-dir>

Each mutant replaces one exact text (which must occur once), runs the tests
that should catch it, and puts the file back with `git checkout`. A mutant
is killed when those tests fail. Prints one line per mutant; exits 1 if any
survived. Reads $TMPDIR for the tests; prints no path.
"""
import os
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target

UNIT = ["cargo", "test", "--locked", "--lib", "--",
        "a_turn_read_whole_in_one_drain", "a_turn_start_owes_its_frame_exactly"]
HARNESS = ["cargo", "test", "--locked", "--test", "tui", "--",
           "the_harness_measures", "an_unpaced_case", "a_skills_sandbox_root"]

MUTANTS = [
    ("drain-never-stops", "src/tui/event_loop.rs",
     "                if starts && shell.turn_start_owes_its_frame() {\n                    return false;\n                }\n",
     "", UNIT),
    ("drain-stops-after-any-event", "src/tui/event_loop.rs",
     "if starts && shell.turn_start_owes_its_frame() {",
     "if shell.turn_start_owes_its_frame() {", UNIT),
    ("owed-only-when-held", "src/tui/shell.rs",
     "        self.status_held || self.status_moved_by_input\n",
     "        self.status_held\n", UNIT),
    ("owed-only-when-keyed", "src/tui/shell.rs",
     "        self.status_held || self.status_moved_by_input\n",
     "        self.status_moved_by_input\n", UNIT),
    ("measure-not-carried", "tests/support/pty.rs",
     "previous.replace(started)", "previous.take()", HARNESS),
    ("measure-never-grows", "tests/support/pty.rs",
     "longest = longest.max(before.elapsed());", "let _ = before;", HARNESS),
    ("retry-gives-up-quietly", "tests/tui.rs",
     "            Err(gap) => paused.push(gap),\n",
     "            Err(gap) => {\n                paused.push(gap);\n                return;\n            }\n", HARNESS),
    ("retry-runs-once", "tests/tui.rs",
     "    for _ in 0..UNPACED_ATTEMPTS {\n        match case() {",
     "    for _ in 0..1 {\n        match case() {", HARNESS),
    ("name-check-case-sensitive", "tests/tui.rs",
     "    let name = name.to_ascii_lowercase();\n    searched",
     "    searched", HARNESS),
]

survived = 0
for name, path, old, new, command in MUTANTS:
    file = os.path.join(clone, path)
    text = open(file).read()
    if text.count(old) != 1:
        print(f"{name}: NOT APPLIED ({text.count(old)} matches)")
        survived += 1
        continue
    open(file, "w").write(text.replace(old, new))
    try:
        run = subprocess.run(command, cwd=clone, env=env, capture_output=True, text=True)
    finally:
        subprocess.run(["git", "-C", clone, "checkout", "--", path], env=env, check=True)
    failed = [l for l in run.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
    if run.returncode != 0 and failed:
        print(f"{name}: killed by {', '.join(l.split()[1] for l in failed)}")
    elif run.returncode != 0:
        print(f"{name}: BUILD OR RUN ERROR (exit {run.returncode}): {run.stderr.strip().splitlines()[-1:]}")
        survived += 1
    else:
        print(f"{name}: SURVIVED")
        survived += 1
sys.exit(1 if survived else 0)
