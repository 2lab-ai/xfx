#!/usr/bin/env python3
"""The repeat check: the three catalog-wait tests, N times each on two lib
test binaries (the base's and the head's), one after the other in every
iteration, at the shared machine's own load -- no burner. Meant to run in
one normal slot of the suite lock (`suite-lock.sh python3 load_check.py ...`).

    python3 -I -B load_check.py <runs> <out> \
        <label>=<test-binary>@<cwd> <label>=<test-binary>@<cwd>

Each run is one binary over the three names (`--exact`), bounded at LIMIT
seconds (its process group killed past it); the 1-minute load average is
read when it begins. Writes one line per iteration, then per binary the
pass count of each test with the load range its runs began under, then
every failure's assertion lines. TMPDIR comes from the environment.
"""
import os
import re
import signal
import statistics
import subprocess
import sys
import time

LIMIT = 300
TESTS = [
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number",
    "tui::worker::tests::a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one",
    "tui::worker::tests::a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop",
]
SHORT = ["A", "B", "C"]


def env():
    out = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        out.pop(name, None)
    return out


def run(binary, cwd):
    proc = subprocess.Popen([binary, "--exact", *TESTS], cwd=cwd, env=env(),
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=LIMIT)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, _ = proc.communicate()
    found = []
    for test in TESTS:
        m = re.search(rf"^test {re.escape(test)} \.\.\. (\S+)$", out, re.M)
        found.append(m.group(1) if m else "missing")
    return found, out


def main():
    runs, out_path = int(sys.argv[1]), sys.argv[2]
    sets = []
    for spec in sys.argv[3:]:
        label, rest = spec.split("=", 1)
        binary, cwd = rest.split("@", 1)
        sets.append((label, binary, cwd))
    lines = [f"# repeat check: {runs} iterations; in each, every binary runs the three tests "
             f"(A, B, C) once, one binary after the other; no burner, the machine's own load; "
             f"one normal slot of the suite lock; each run bounded at {LIMIT} s; "
             f"cores {os.cpu_count()}"]
    counts = {label: [0, 0, 0] for label, _, _ in sets}
    loads = {label: [] for label, _, _ in sets}
    fails = {label: [] for label, _, _ in sets}
    print(lines[0], flush=True)
    for i in range(1, runs + 1):
        row = [f"run {i:>3}"]
        for label, binary, cwd in sets:
            load = os.getloadavg()[0]
            began = time.monotonic()
            found, out = run(binary, cwd)
            loads[label].append(load)
            for n, verdict in enumerate(found):
                if verdict == "ok":
                    counts[label][n] += 1
                else:
                    failed = [l for l in out.splitlines()
                              if l.startswith(("thread '", "  left:", " right:"))]
                    fails[label].append((i, SHORT[n], verdict, load, failed))
            row.append(f"{label} load {load:5.2f} " + " ".join(
                f"{s}:{v}" for s, v in zip(SHORT, found)) + f" {time.monotonic() - began:5.1f} s")
        lines.append("  ".join(row))
        print(lines[-1], flush=True)
    lines.append("")
    for label, _, _ in sets:
        ls = loads[label]
        lines.append(f"PASSED {label}: " + ", ".join(
            f"{s} {c}/{runs}" for s, c in zip(SHORT, counts[label]))
            + f"  (1-min load at its runs' start: min {min(ls):.2f}, median "
              f"{statistics.median(ls):.2f}, max {max(ls):.2f})")
    for label, _, _ in sets:
        for i, short, verdict, load, failed in fails[label]:
            lines.append(f"FAIL {label} run {i} {short} {verdict} (load {load:.2f})")
            lines += ["    " + l for l in failed]
    with open(out_path, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print("\n".join(l for l in lines if l.startswith(("PASSED", "FAIL"))))


main()
