#!/usr/bin/env python3
"""The repeat check: the five catalog-wait tests, N times each on two lib
test binaries (the base's and the head's), one after the other in every
iteration, at the shared machine's own load -- no burner. Meant to run in
one normal slot of the suite lock (`suite-lock.sh python3 load_check.py ...`).

    python3 -I -B load_check.py <runs> <out> \
        <label>=<test-binary>@<cwd> <label>=<test-binary>@<cwd>

Each run is one binary over the five names (`--exact`), bounded at LIMIT
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
    "tui::worker::tests::a_stop_through_a_dropped_compaction_tells_it_once_and_leaves_the_history_notice_to_the_turn_that_runs",
    "tui::worker::tests::a_turn_stopped_before_its_request_leaves_the_history_notice_to_the_next_turn",
]
SHORT = ["A", "B", "C", "D", "E"]


def env():
    out = dict(os.environ)
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE"):
        out.pop(name, None)
    return out


def run(binary, cwd):
    proc = subprocess.Popen([binary, "--exact", "--nocapture", *TESTS], cwd=cwd, env=env(),
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=LIMIT)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, _ = proc.communicate()
    segments = [int(v) for v in re.findall(r"^stop-to-end-us (\d+)$", out, re.M)]
    found = []
    for test in TESTS:
        m = re.search(rf"^test {re.escape(test)} \.\.\. (\S+)$", out, re.M)
        found.append(m.group(1) if m else "missing")
    return found, out, segments


def main():
    runs, out_path = int(sys.argv[1]), sys.argv[2]
    sets = []
    for spec in sys.argv[3:]:
        label, rest = spec.split("=", 1)
        binary, cwd = rest.split("@", 1)
        sets.append((label, binary, cwd))
    lines = [f"# repeat check: {runs} iterations; in each, every binary runs the three tests "
             f"(A to E) once, one binary after the other; no burner, the machine's own load; "
             f"one normal slot of the suite lock; each run bounded at {LIMIT} s; "
             f"cores {os.cpu_count()}"]
    counts = {label: [0] * len(TESTS) for label, _, _ in sets}
    loads = {label: [] for label, _, _ in sets}
    fails = {label: [] for label, _, _ in sets}
    seg = {label: [] for label, _, _ in sets}
    print(lines[0], flush=True)
    for i in range(1, runs + 1):
        row = [f"run {i:>3}"]
        for label, binary, cwd in sets:
            load = os.getloadavg()[0]
            began = time.monotonic()
            found, out, segments = run(binary, cwd)
            seg[label] += [(s, load) for s in segments]
            loads[label].append(load)
            for n, verdict in enumerate(found):
                if verdict == "ok":
                    counts[label][n] += 1
                else:
                    failed = [l for l in out.splitlines()
                              if l.startswith(("thread '", "  left:", " right:"))]
                    fails[label].append((i, SHORT[n], verdict, load, failed))
            row.append(f"{label} load {load:5.2f} " + " ".join(
                f"{s}:{v}" for s, v in zip(SHORT, found)) + f" {time.monotonic() - began:5.1f} s"
                + (f" stop-to-end max {max(segments) / 1000:.1f} ms (n={len(segments)})" if segments else ""))
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
        if seg[label]:
            values = sorted(s for s, _ in seg[label])
            top, top_load = max(seg[label])
            lines.append(f"STOP-TO-END {label}: {len(values)} segments (the harness's stopped runs, "
                         f"stop sent to the first turn's place given back, wall clock): min "
                         f"{values[0] / 1000:.1f} ms, median {statistics.median(values) / 1000:.1f} ms, "
                         f"p99 {values[int(len(values) * 0.99) - 1] / 1000:.1f} ms, max {top / 1000:.1f} ms "
                         f"(its run began at load {top_load:.2f})")
    for label, _, _ in sets:
        for i, short, verdict, load, failed in fails[label]:
            lines.append(f"FAIL {label} run {i} {short} {verdict} (load {load:.2f})")
            lines += ["    " + l for l in failed]
    with open(out_path, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print("\n".join(l for l in lines if l.startswith(("PASSED", "FAIL", "STOP-TO-END"))))


main()
