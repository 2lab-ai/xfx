#!/usr/bin/env python3
"""The load check: the three catalog-wait tests, N times each on two lib
test binaries (the base's and the head's) side by side, while one burner
process (burn.c: busy threads, no fork, no exec, its own time limit) keeps
every core busy. Meant to run under `SUITE_LOCK_EXCLUSIVE=1 suite-lock.sh`.

    python3 -I -B load_check.py <burn> <threads> <burn-seconds> <runs> <out> \
        <label>=<test-binary>@<cwd> <label>=<test-binary>@<cwd>

Each iteration starts both binaries at once (each over the three names,
`--exact`), bounded at LIMIT seconds (its process group killed past it), and
records each test's verdict per binary, the run's time and the 1-minute load
when it began. The burner is started once, by Popen (no shell), and killed by
its own PID at the end, whatever happened. TMPDIR comes from the
environment.
"""
import os
import re
import signal
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


def verdicts(out):
    found = []
    for test in TESTS:
        m = re.search(rf"^test {re.escape(test)} \.\.\. (\S+)$", out, re.M)
        found.append(m.group(1) if m else "missing")
    return found


def main():
    burn, threads, burn_seconds, runs, out_path = sys.argv[1:6]
    sets = []
    for spec in sys.argv[6:]:
        label, rest = spec.split("=", 1)
        binary, cwd = rest.split("@", 1)
        sets.append((label, binary, cwd))
    lines = [f"# load check: {runs} iterations; per iteration every binary runs the three tests "
             f"(A, B, C) once, side by side; burner {threads} busy threads in one process, "
             f"limit {burn_seconds} s; each run bounded at {LIMIT} s",
             f"# cores {os.cpu_count()}; load before the burner {os.getloadavg()[0]:.2f}"]
    counts = {label: [0, 0, 0] for label, _, _ in sets}
    fails = {label: [] for label, _, _ in sets}
    burner = subprocess.Popen([burn, threads, burn_seconds], start_new_session=True)
    try:
        lines.append("# burner: " + subprocess.run(
            ["ps", "-o", "pid=,command=", "-p", str(burner.pid)],
            capture_output=True, text=True).stdout.strip().split(" ", 1)[0]
            + " (pid) " + os.path.basename(burn) + f" {threads} {burn_seconds}")
        time.sleep(30)
        lines.append(f"# load 30 s into the burner {os.getloadavg()[0]:.2f}")
        print("\n".join(lines), flush=True)
        for i in range(1, int(runs) + 1):
            if burner.poll() is not None:
                lines.append(f"# the burner ended before iteration {i}: stopping")
                break
            load = os.getloadavg()[0]
            began = time.monotonic()
            procs = []
            for label, binary, cwd in sets:
                proc = subprocess.Popen([binary, "--exact", *TESTS], cwd=cwd, env=env(),
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                        text=True, start_new_session=True)
                procs.append((label, proc))
            row = [f"run {i:>3} load {load:6.2f}"]
            for label, proc in procs:
                try:
                    out, _ = proc.communicate(timeout=max(1, LIMIT - (time.monotonic() - began)))
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    out, _ = proc.communicate()
                found = verdicts(out)
                for n, verdict in enumerate(found):
                    if verdict == "ok":
                        counts[label][n] += 1
                    else:
                        failed = [l for l in out.splitlines()
                                  if l.startswith(("thread '", "  left:", " right:"))]
                        fails[label].append((i, SHORT[n], verdict, failed))
                row.append(f"{label} " + " ".join(f"{s}:{v}" for s, v in zip(SHORT, found)))
            row.append(f"{time.monotonic() - began:5.1f} s")
            lines.append("  ".join(row))
            print(lines[-1], flush=True)
    finally:
        if burner.poll() is None:
            burner.kill()
        burner.wait()
        lines.append(f"# burner {burner.pid} ended, exit {burner.returncode}; "
                     f"load after {os.getloadavg()[0]:.2f}")
    total = int(runs)
    lines.append("")
    for label, _, _ in sets:
        lines.append(f"PASSED {label}: " + ", ".join(
            f"{s} {c}/{total}" for s, c in zip(SHORT, counts[label])))
    for label, _, _ in sets:
        for i, short, verdict, failed in fails[label]:
            lines.append(f"FAIL {label} run {i} {short} {verdict}")
            lines += ["    " + l for l in failed]
    with open(out_path, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print("\n".join(lines[-(2 * len(sets) + 2):]))


main()
