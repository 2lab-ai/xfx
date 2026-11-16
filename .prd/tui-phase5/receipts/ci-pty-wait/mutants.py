#!/usr/bin/env python3
"""CI-PTY-WAIT's mutants, each run on a throwaway clone, never the worktree.

    python3 -I mutants.py <clone> <cargo-target-dir>

First the named tests run unmutated (they must pass: the GREEN). Then each
mutant replaces one exact text (which must occur once), runs the tests that
should catch it, and puts the file back with `git checkout`. A mutant is
killed when those tests fail; the killing tests are the mutant's RED. Prints
one line per run; exits 1 if the baseline failed or any mutant survived.
Reads $TMPDIR for the tests; prints no path.

The join with train 16 added a bound on each cargo run, its build included
(TIMEOUT): a run past it has its process group killed and is printed TIMED
OUT -- a baseline that times out fails, a mutant that does is counted apart
from the kills -- and the two `loop-*` mutants, with the join test in UNIT.
The join review added two more `loop-*` orders, and UNIT takes every
`an_escape_pair_` case (the settled one, the hand-driven pending one and the
loop-driven pending one).
"""
import os
import re
import signal
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target

UNIT = ["cargo", "test", "--locked", "--lib", "--",
        "a_turn_read_whole_in_one_drain", "a_turn_start_owes_its_frame_exactly",
        "keys_between_a_split", "under_a_sustained_stream", "the_drain_reads_past_a_start",
        "the_drain_stops_behind_a_start_only", "an_escape_pair_"]
HARNESS = ["cargo", "test", "--locked", "--test", "tui", "--",
           "the_harness_measures", "an_unpaced_case", "a_skills_sandbox_path",
           "the_last_bytes_of_a_stream"]

DRAIN_STOP = ("                if starts && shell.turn_start_owes_its_frame() {\n"
              "                    return false;\n"
              "                }\n")
KEYED = "        if self.status_moved_by_input || (self.status_held && working) {\n"
SETTLE = "    shell.settle_input(Instant::now());\n"
SETTLE_THEN_DRAIN = (SETTLE +
                     "    // What the runtime produced since the last turn. Text, not terminal\n"
                     "    // state, and therefore outside the reconciles for the same reason.\n"
                     "    take_ui_events(shell, events);\n")
TIMEOUT = 1200
# The pass from the input's settle through the band's, and from the read
# through the drain, as loop_turn has them: each order mutant moves one line
# within one of them, so it is one replacement.
BAND_LINE = "    shell.settle_band(Instant::now());\n"
DRAIN_LINE = "    take_ui_events(shell, events);\n"
PASS = (SETTLE_THEN_DRAIN +
        "    // And what neither the bytes nor the events settled: how deep the\n"
        "    // submission queue is now, and whether an armed Escape has gone stale.\n"
        "    // Both are answers only the clock and the other thread produce, so like\n"
        "    // `settle_input` this is what makes them a tick rather than a wait for\n"
        "    // the next keystroke.\n" + BAND_LINE)
READ_TO_DRAIN = (
    "    read(shell)?;\n"
    "    // What the bytes could not settle by themselves. A lone `ESC` is both a\n"
    "    // key and the first byte of every sequence, so it is the Escape key only\n"
    "    // once it has been alone for `input::Decoder::ESC_TIMEOUT`; asking here,\n"
    "    // every turn, is what makes that a 50 ms wait rather than a wait for the\n"
    "    // next keystroke. It reads no terminal and writes none, which is why it\n"
    "    // is outside the two reconciles rather than between them.\n" + SETTLE_THEN_DRAIN)

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
     "longest = longest.max(gap);", "let _ = gap;", HARNESS),
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
    # Round 3.
    ("last-bytes-cut-anywhere", "tests/tui.rs",
     "    while !text.is_char_boundary(from) {\n        from += 1;\n    }\n", "", HARNESS),
    # The join with train 16: the loop's turn settles the input once, after
    # the read and before the drain.
    ("loop-never-settles-input", "src/tui/event_loop.rs", SETTLE_THEN_DRAIN,
     SETTLE_THEN_DRAIN.replace(SETTLE, ""), UNIT),
    ("loop-settles-input-after-the-drain", "src/tui/event_loop.rs", SETTLE_THEN_DRAIN,
     SETTLE_THEN_DRAIN.replace(SETTLE, "") + SETTLE, UNIT),
    # The join review (SHOULD 2): two more orders of the pass, which the
    # loop-driven pending case (25203a2c) kills and the hand-driven one
    # cannot (join-review/order_red.py runs the three cases apart).
    ("loop-settles-band-first", "src/tui/event_loop.rs", PASS,
     "    shell.settle_band(Instant::now());\n" + PASS.replace(BAND_LINE, ""), UNIT),
    ("loop-drains-before-the-read", "src/tui/event_loop.rs", READ_TO_DRAIN,
     "    take_ui_events(shell, events);\n" + READ_TO_DRAIN.replace(DRAIN_LINE, ""), UNIT),
]


def run(command):
    proc = subprocess.Popen(command, cwd=clone, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=TIMEOUT)
        code = proc.returncode
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        code = None
    done = subprocess.CompletedProcess(command, code, out, err)
    failed = [l.split()[1] for l in done.stdout.splitlines()
              if l.startswith("test ") and l.endswith("FAILED")]
    passed = sum(int(n) for n in re.findall(r"test result: \w+\. (\d+) passed", done.stdout))
    return done, failed, passed


bad = 0
for name, command in (("unit", UNIT), ("harness", HARNESS)):
    done, failed, passed = run(command)
    ok = done.returncode == 0 and not failed
    if done.returncode is None:
        print(f"baseline {name}: TIMED OUT after {TIMEOUT} s")
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
    if done.returncode is None:
        print(f"{name}: TIMED OUT after {TIMEOUT} s, failed so far {failed}")
    elif done.returncode != 0 and failed:
        print(f"{name}: killed by {', '.join(failed)}")
    elif done.returncode != 0:
        tail = done.stderr.strip().splitlines()[-1:]
        print(f"{name}: BUILD OR RUN ERROR (exit {done.returncode}): {tail}")
        bad += 1
    else:
        print(f"{name}: SURVIVED")
        bad += 1
sys.exit(1 if bad else 0)
