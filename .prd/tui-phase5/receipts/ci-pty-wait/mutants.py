#!/usr/bin/env python3
"""CI-PTY-WAIT's mutants, each run on a throwaway clone, never the worktree.

    python3 -I mutants.py <clone> <cargo-target-dir>

First the named tests run unmutated (they must pass: the GREEN). Then each
mutant replaces one exact text (which must occur once), runs the tests that
should catch it, and puts the file back with `git checkout`. A mutant is
killed when those tests fail; the killing tests are the mutant's RED. Prints
one line per run; exits 1 if the baseline failed or any mutant survived.
Reads $TMPDIR for the tests; prints no path.
"""
import os
import re
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target

UNIT = ["cargo", "test", "--locked", "--lib", "--",
        "a_turn_read_whole_in_one_drain", "a_turn_start_owes_its_frame_exactly",
        "keys_between_a_split", "under_a_sustained_stream", "the_drain_reads_past_a_start",
        "the_drain_stops_behind_a_start_only"]
HARNESS = ["cargo", "test", "--locked", "--test", "tui", "--",
           "the_harness_measures", "an_unpaced_case", "a_skills_sandbox_path"]

DRAIN_STOP = ("                if starts && shell.turn_start_owes_its_frame() {\n"
              "                    return false;\n"
              "                }\n")
KEYED = "        if self.status_moved_by_input || (self.status_held && working) {\n"

MUTANTS = [
    # The round-1 set, re-aimed at the round-2 code where it moved.
    ("drain-never-stops", "src/tui/event_loop.rs", DRAIN_STOP, "", UNIT),
    ("drain-stops-after-any-event", "src/tui/event_loop.rs",
     "if starts && shell.turn_start_owes_its_frame() {",
     "if shell.turn_start_owes_its_frame() {", UNIT),
    ("owed-only-when-held", "src/tui/shell.rs", KEYED,
     "        if self.status_held && working {\n", UNIT),
    ("owed-only-when-keyed", "src/tui/shell.rs", KEYED,
     "        if self.status_moved_by_input {\n", UNIT),
    ("measure-not-carried", "tests/support/pty.rs",
     "previous.replace(started)", "previous.take()", HARNESS),
    ("measure-never-grows", "tests/support/pty.rs",
     "longest = longest.max(before.elapsed());", "let _ = before;", HARNESS),
    ("retry-gives-up-quietly", "tests/tui.rs",
     "            Err(rejected) => paused.push(rejected),\n",
     "            Err(rejected) => {\n                paused.push(rejected);\n                return;\n            }\n",
     HARNESS),
    ("retry-runs-once", "tests/tui.rs",
     "    for _ in 0..UNPACED_ATTEMPTS {\n        match case() {",
     "    for _ in 0..1 {\n        match case() {", HARNESS),
    ("search-check-case-sensitive", "tests/tui.rs",
     "        let text = text.to_ascii_lowercase();\n        searched",
     "        searched", HARNESS),
    # Round 2.
    ("predicate-by-flags-alone", "src/tui/shell.rs",
     "        self.keyed() == Keyed::AfterFrame\n",
     "        self.status_held || self.status_moved_by_input\n", UNIT),
    ("drain-stops-at-every-start", "src/tui/event_loop.rs",
     "if starts && shell.turn_start_owes_its_frame() {",
     "if starts {", UNIT),
    ("drain-stops-after-every-event", "src/tui/event_loop.rs",
     "if starts && shell.turn_start_owes_its_frame() {",
     "if true {", UNIT),
    ("search-check-root-name-only", "tests/tui.rs",
     '    let found = ["home", "workspace"]\n        .iter()\n        .any(|below| holds(&root.join(below).to_string_lossy()));\n',
     "    let found = root\n        .file_name()\n        .is_some_and(|name| holds(&name.to_string_lossy()));\n",
     HARNESS),
    ("rejected-screens-unsaid", "tests/tui.rs",
     'rejected.screen.join(" | ")', "String::new()", HARNESS),
]


def run(command):
    done = subprocess.run(command, cwd=clone, env=env, capture_output=True, text=True)
    failed = [l.split()[1] for l in done.stdout.splitlines()
              if l.startswith("test ") and l.endswith("FAILED")]
    passed = sum(int(n) for n in re.findall(r"test result: \w+\. (\d+) passed", done.stdout))
    return done, failed, passed


bad = 0
for name, command in (("unit", UNIT), ("harness", HARNESS)):
    done, failed, passed = run(command)
    ok = done.returncode == 0 and not failed
    print(f"baseline {name}: {'green' if ok else 'NOT GREEN'}, {passed} passed, failed {failed}")
    bad += not ok
for name, path, old, new, command in MUTANTS:
    file = os.path.join(clone, path)
    text = open(file).read()
    if text.count(old) != 1:
        print(f"{name}: NOT APPLIED ({text.count(old)} matches)")
        bad += 1
        continue
    open(file, "w").write(text.replace(old, new))
    try:
        done, failed, _ = run(command)
    finally:
        subprocess.run(["git", "-C", clone, "checkout", "--", path], env=env, check=True)
    if done.returncode != 0 and failed:
        print(f"{name}: killed by {', '.join(failed)}")
    elif done.returncode != 0:
        tail = done.stderr.strip().splitlines()[-1:]
        print(f"{name}: BUILD OR RUN ERROR (exit {done.returncode}): {tail}")
        bad += 1
    else:
        print(f"{name}: SURVIVED")
        bad += 1
sys.exit(1 if bad else 0)
