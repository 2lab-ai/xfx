#!/usr/bin/env python3
"""The three catalog-wait tests under the scratch stall (turn-stall.patch):
the loop's thread held XFX_SCRATCH_TURN_STALL_MS right after each turn
starts. Runs `cargo test --locked --lib` over the three names in <tree>
(a throwaway clone with the patch applied), once per stall, each run
bounded at LIMIT seconds (its process group killed past it), and writes one
summary line per test and run, then each run's whole output, to <out>.

    python3 -I -B stall_runs.py <tree> <label> <out> <stall-ms>...

TMPDIR for the tests is taken from the environment.
"""
import os
import re
import signal
import subprocess
import sys

LIMIT = 600
TESTS = [
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number",
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one",
    "tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop",
]
SHORT = {TESTS[0]: "A next-start", TESTS[1]: "B records-one", TESTS[2]: "C twenty-questions"}


def run(tree, stall):
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        env.pop(name, None)
    env["XFX_SCRATCH_TURN_STALL_MS"] = str(stall)
    args = ["cargo", "test", "--locked", "--lib", "--", "--exact", *TESTS]
    proc = subprocess.Popen(args, cwd=tree, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=LIMIT)
        return proc.returncode, out, False
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, _ = proc.communicate()
        return None, out, True


def main():
    tree, label, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    stalls = [int(s) for s in sys.argv[4:]]
    head = subprocess.run(["git", "-C", tree, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    summary = [f"# {label}: the three tests at {head} with turn-stall.patch applied, "
               f"one run per stall, each bounded at {LIMIT} s"]
    bodies = []
    for stall in stalls:
        rc, out, hung = run(tree, stall)
        for test in TESTS:
            m = re.search(rf"^test {re.escape(test)} \.\.\. (\S+)$", out, re.M)
            verdict = m.group(1) if m else ("killed (limit)" if hung else "not run")
            summary.append(f"stall {stall:>5} ms  {SHORT[test]:<20} {verdict}")
        result = re.findall(r"^test result: .*$", out, re.M)
        summary.append(f"stall {stall:>5} ms  exit {rc}; {result[-1] if result else 'no result'}")
        bodies.append(f"===== stall {stall} ms =====\n{out}")
    with open(out_path, "w") as handle:
        handle.write("\n".join(summary) + "\n\n" + "\n".join(bodies))
    print("\n".join(summary))


main()
