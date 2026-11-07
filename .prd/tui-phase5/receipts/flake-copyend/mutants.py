"""FLAKE-COPYEND's mutants: each one change to src/tui/copy.rs in a
throwaway clone, then the test named, which must fail.

usage: python3 -I mutants.py <clone> > mutants.txt

<clone> is a `git clone --no-local` at the commit under test. Each mutant is
applied, its test run with `cargo test --locked --lib <test> -- --exact`, and
the tree put back (`git checkout -- .`). KILLED when the test fails, SURVIVED
when it passes, with the test's last line.
"""

import subprocess
import sys

COPY = "src/tui/copy.rs"
T = "tui::copy::tests::"

MUTANTS = [
    ("C1 the end gives a copy up before it waits (kills before the answer)",
     "        let began = Instant::now();\n        while running",
     "        for (_, stop) in &running {\n            stop.stop();\n        }\n        let began = Instant::now();\n        while running",
     T + "an_ending_session_lets_a_copy_that_answers_land"),
    ("C2 the end waits for nothing: the grace is never asked",
     "while running.iter().any(|(thread, _)| !thread.is_finished()) && !over(began) {",
     "while false && !over(began) {",
     T + "an_ending_session_lets_a_copy_that_answers_land"),
    ("C3 a session's Drop gives no grace",
     "        self.end(|began| began.elapsed() >= GRACE);",
     "        self.end(|_| true);",
     T + "an_ending_session_kills_a_copy_whose_tool_never_exits_and_joins_its_thread"),
    ("C4 starting a copy waits for it",
     "        let done = self.done.clone();\n        self.errands.spawn(THREAD_NAME, move |stop| {\n            // Nobody left to tell is a session that has ended, and the\n            // clipboard was written for it all the same.\n            let _ = done.send(clipboard.copy_until(&reply, stop));\n        })",
     "        let done = self.done.clone();\n        let ok = clipboard.copy(&reply);\n        let _ = done.send(ok);\n        let _ = &self.errands;\n        true",
     T + "a_copy_is_started_and_answered_later_rather_than_waited_for"),
]


def main(clone):
    for name, old, new, test in MUTANTS:
        path = clone + "/" + COPY
        text = open(path).read()
        if text.count(old) != 1:
            print("%s: NOT APPLIED (%d matches)" % (name, text.count(old)), flush=True)
            continue
        open(path, "w").write(text.replace(old, new))
        r = subprocess.run(["cargo", "test", "--locked", "--lib", test, "--", "--exact"], cwd=clone,
                           capture_output=True, text=True, timeout=1800)
        lines = [line for line in (r.stdout + r.stderr).strip().split("\n") if line]
        verdict = [line for line in lines if line.startswith("test result:") or "panicked at" in line or line.startswith("error")]
        print("%s: %s (%s, exit %d: %s)" % (name, "KILLED" if r.returncode else "SURVIVED", test.split("::")[-1], r.returncode,
                                            " | ".join(verdict[-2:]) if verdict else lines[-1]), flush=True)
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)


if __name__ == "__main__":
    main(sys.argv[1])
