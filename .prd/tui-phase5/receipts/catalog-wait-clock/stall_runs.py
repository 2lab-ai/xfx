#!/usr/bin/env python3
"""The catalog-wait tests under the scratch stall points (stall_patch.py:
TURN, SELECT, START). Runs `cargo test --locked --lib` over the five names
in <tree> (a throwaway clone with the points planted), once per stall spec,
each run bounded at LIMIT seconds (its process group killed past it), and
writes one summary line per test and run, then each run's whole output, to
<out>. A spec is `none`, or comma-joined `point=ms` (`turn=1200`,
`select=1200,start=1200`).

    python3 -I -B stall_runs.py <tree> <label> <out> <spec>...

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
    "tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs",
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn",
]
SHORT = {TESTS[0]: "A next-start", TESTS[1]: "B records-one", TESTS[2]: "C twenty-questions",
         TESTS[3]: "D compaction-join", TESTS[4]: "E history-notice"}
VARS = {"turn": "XFX_SCRATCH_TURN_STALL_MS", "select": "XFX_SCRATCH_SELECT_STALL_MS",
        "start": "XFX_SCRATCH_START_STALL_MS"}


def run(tree, spec):
    env = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", *VARS.values()):
        env.pop(name, None)
    if spec != "none":
        for part in spec.split(","):
            point, ms = part.split("=")
            env[VARS[point]] = str(int(ms))
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
    stalls = sys.argv[4:]
    head = subprocess.run(["git", "-C", tree, "rev-parse", "--short=8", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    summary = [f"# {label}: the five tests at {head} with stall_patch.py's points planted, "
               f"one run per stall spec, each bounded at {LIMIT} s"]
    bodies = []
    for stall in stalls:
        rc, out, hung = run(tree, stall)
        for test in TESTS:
            m = re.search(rf"^test {re.escape(test)} \.\.\. (\S+)$", out, re.M)
            verdict = m.group(1) if m else ("killed (limit)" if hung else "not run")
            summary.append(f"{stall:<34} {SHORT[test]:<20} {verdict}")
        result = re.findall(r"^test result: .*$", out, re.M)
        summary.append(f"{stall:<34} exit {rc}; {result[-1] if result else 'no result'}")
        bodies.append(f"===== stall {stall} =====\n{out}")
    with open(out_path, "w") as handle:
        handle.write("\n".join(summary) + "\n\n" + "\n".join(bodies))
    print("\n".join(summary))


main()
