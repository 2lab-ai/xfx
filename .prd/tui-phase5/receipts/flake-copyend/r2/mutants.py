"""FLAKE-COPYEND's mutants, round 1's (C1-C4, retargeted where round 2
moved their text) and round 2's: each one change to the code or the fake
tool in a throwaway clone, then the test named, which must fail.

usage: python3 -I mutants.py <clone> > mutants.txt

<clone> is a `git clone --no-local` at the commit under test, built in its
own target directory (no CARGO_TARGET_DIR). Each mutant is applied, its test
run with `cargo test --locked --lib <test> -- --exact`, the tree put back
(`git checkout -- .`), and any fake tool still running afterwards (an
orphan whose directory is gone) is counted and killed. KILLED when the test
fails, SURVIVED when it passes, NOT BUILT when the change does not compile
(no verdict: the mutant itself is wrong), with the test's last lines and
how long it took.
"""

import os
import re
import signal
import subprocess
import sys
import time

COPY, CLIP, TRACE = "src/tui/copy.rs", "src/clipboard.rs", "src/tui/trace.rs"
T = "tui::copy::tests::"
TOOL = "clipboard::tests::tool::"
LAND = T + "an_ending_session_lets_a_copy_that_answers_land"
AFTER = T + "an_ending_session_kills_a_copy_whose_answer_comes_after_its_grace"
NEVER = T + "an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread"
LATER = T + "a_copy_is_started_and_answered_later_rather_than_waited_for"
PANIC = T + "an_end_whose_look_at_the_grace_panics_still_stops_and_joins_every_copy"
REPORT = "tui::trace::tests::an_ending_session_joins_a_report_whole_and_gives_its_copy_up"

START_WAITS = (
    COPY,
    "        let done = self.done.clone();\n        self.errands.spawn(THREAD_NAME, move |stop| {\n"
    "            // Nobody left to tell is a session that has ended, and the\n"
    "            // clipboard was written for it all the same.\n"
    "            let _ = done.send(clipboard.copy_until(&reply, stop));\n        })",
    "        let done = self.done.clone();\n        let ok = clipboard.copy(&reply);\n"
    "        let _ = done.send(ok);\n        let _ = &self.errands;\n        true",
)
NO_STOP = (COPY, "        for (_, stop) in &self.0 {\n            stop.stop();\n        }\n", "")

MUTANTS = [
    ("C1 the end gives a copy up before it waits (kills before the answer)", COPY,
     "        let began = Instant::now();\n        while running",
     "        for (_, stop) in &running.0 {\n            stop.stop();\n        }\n"
     "        let began = Instant::now();\n        while running", LAND),
    ("C2 the end waits for nothing: the grace is never asked", COPY,
     "while running.0.iter().any(|(thread, _)| !thread.is_finished()) && !over(began) {",
     "while false && !over(began) {", LAND),
    ("C3 a session's Drop gives no grace", COPY,
     "        self.end(|began| began.elapsed() >= GRACE);", "        self.end(|_| true);", NEVER),
    ("C4 starting a copy waits for it", *START_WAITS, LATER),
    ("C4 starting a copy waits for it (against the landing case)", *START_WAITS, LAND),
    ("C4 starting a copy waits for it (against the after-grace case)", *START_WAITS, AFTER),
    ("C5 the end joins without stopping (against the after-grace case)", *NO_STOP, AFTER),
    ("C5 the end joins without stopping (against /trace's end)", *NO_STOP, REPORT),
    ("P1 an end that panics leaves its threads (the guard does nothing while unwinding)", COPY,
     "    fn drop(&mut self) {\n        for (_, stop) in &self.0 {",
     "    fn drop(&mut self) {\n        if std::thread::panicking() {\n            return;\n        }\n"
     "        for (_, stop) in &self.0 {", PANIC),
    ("G1 the gate never gives up", CLIP,
     '"  [ \\"$n\\" -lt {looks} ] || {{', '"  [ \\"$n\\" -lt {looks} ] || true || {{',
     TOOL + "a_gate_nobody_opens_gives_up_and_says_so"),
    ("G2 the gate does not end with its directory", CLIP,
     '                    "  [ -d \\"${{0%/*}}\\" ] || exit 1\\n",\n', "",
     TOOL + "a_tool_at_its_gate_ends_itself_once_its_case_is_gone"),
    ("G3 the gate gives up without saying so", CLIP,
     ': > \\"${{0%/*}}/gave-up\\"; exit 1;', "exit 1;", TOOL + "a_gate_nobody_opens_gives_up_and_says_so"),
    ("D1 a file copy ignores its deadline", CLIP,
     "        let deadline = deadline.map(|limit| Instant::now() + limit);\n",
     "        let deadline: Option<Instant> = deadline.and(None);\n",
     TOOL + "a_file_copy_that_outlives_its_deadline_is_killed_and_a_failure"),
]


def orphans():
    """This user's fake tools whose parent is gone: `sh <tempdir>/<tool>`."""
    out = subprocess.run(["ps", "-U", str(os.getuid()), "-o", "pid=,ppid=,command="],
                         capture_output=True, text=True).stdout
    found = []
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[1] == "1" and re.search(r"/\.tmp\w+/(pbcopy|xclip|osascript)$", parts[2]):
            found.append(int(parts[0]))
    return found


def main(clone):
    for name, path, old, new, test in MUTANTS:
        full = os.path.join(clone, path)
        text = open(full).read()
        if text.count(old) != 1:
            print("%s: NOT APPLIED (%d matches)" % (name, text.count(old)), flush=True)
            continue
        open(full, "w").write(text.replace(old, new))
        began = time.time()
        r = subprocess.run(["cargo", "test", "--locked", "--lib", test, "--", "--exact"], cwd=clone,
                           capture_output=True, text=True, timeout=1800)
        took = time.time() - began
        lines = [line for line in (r.stdout + r.stderr).strip().split("\n") if line]
        verdict = [line for line in lines if line.startswith("test result:") or "panicked at" in line
                   or line.startswith("error")]
        said = [lines[i + 1] for i, line in enumerate(lines[:-1]) if "panicked at" in line]
        left = orphans()
        for pid in left:
            os.kill(pid, signal.SIGKILL)
        built = "could not compile" not in r.stdout + r.stderr
        print("%s: %s (%s, exit %d, %.0f s: %s%s)%s" % (
            name, ("KILLED" if r.returncode else "SURVIVED") if built else "NOT BUILT", test.split("::")[-1], r.returncode, took,
            " | ".join(verdict[-2:]) if verdict else lines[-1],
            (" | said: " + said[-1].strip()) if said else "",
            (" [killed %d orphaned fake tool(s) after it]" % len(left)) if left else ""), flush=True)
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)


if __name__ == "__main__":
    main(sys.argv[1])
