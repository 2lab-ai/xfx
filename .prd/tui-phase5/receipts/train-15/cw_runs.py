#!/usr/bin/env python3
"""Train 15 (new), join check 5: CATALOG-WAIT-CLOCK x everything. The
catalog-wait tests in their own module, `tui::worker::tests`, run whole --
every test of the module beside them, as the gate runs them -- N times on
the head's plain lib test binary and N times on its fault-injection one,
one run after the other in one normal slot of the suite lock, at the shared
machine's own load (no burner), the 1-minute load read when each run
begins. Each run is bounded at LIMIT seconds (its process group killed past
it). Writes one line per run: the binary, the load, the module's `test
result` line, and each named test's result; then per binary the pass count
of each named test and the load range; then every failure's assertion
lines. TMPDIR comes from the environment.

The named tests are A to D of the unit's README at the head (D under main's
name, `..._tells_it_once_before_the_turn_that_runs`; E went with
IMAGE-LOSS-COPY-FX in train 14, merge 1) and its round 4's F and G.

    suite-lock.sh python3 -I -B cw_runs.py <runs> <out> <label>=<test-binary>@<cwd>...
"""
import os
import re
import signal
import subprocess
import sys
import time

LIMIT = 900
MODULE = "tui::worker::tests::"
TESTS = [
    ("A", "a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number"),
    ("B", "a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one"),
    ("C", "a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop"),
    ("D", "a_stop_through_a_dropped_compaction_tells_it_once_before_the_turn_that_runs"),
    ("F", "the_place_watcher_ends_when_the_run_it_watches_panics"),
    ("G", "a_late_watcher_moves_the_stop_to_end_reading_and_fails_it_only_past_the_bound"),
]

runs, out = int(sys.argv[1]), sys.argv[2]
bins = []
for spec in sys.argv[3:]:
    label, rest = spec.split("=", 1)
    path, cwd = rest.split("@", 1)
    bins.append((label, path, cwd))
env = {k: v for k, v in os.environ.items() if k not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE")}
lines, fails = [], []
tally = {label: {s: 0 for s, _ in TESTS} for label, _, _ in bins}
loads = {label: [] for label, _, _ in bins}
module_ok = {label: 0 for label, _, _ in bins}
lines.append(f"# join check 5: {runs} runs of {MODULE.rstrip(':')} on each binary, one after the other, "
             f"in one normal slot, the machine's own load; each run bounded at {LIMIT} s; cores {os.cpu_count()}")
for label, path, cwd in bins:
    for i in range(1, runs + 1):
        load = os.getloadavg()[0]
        loads[label].append(load)
        start = time.monotonic()
        p = subprocess.Popen([path, MODULE, "--test-threads", str(os.cpu_count())], cwd=cwd, env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
        try:
            text, _ = p.communicate(timeout=LIMIT)
            rc = p.returncode
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            text, _ = p.communicate()
            rc = "timeout"
        took = time.monotonic() - start
        result = re.findall(r"^test result: (.*)$", text, re.M)
        res = []
        for short, name in TESTS:
            m = re.findall(r"^test " + re.escape(MODULE + name) + r" \.\.\. (\w+)", text, re.M)
            r = m[0] if len(m) == 1 else "missing"
            res.append(f"{short}:{r}")
            if r == "ok":
                tally[label][short] += 1
            else:
                fails.append((label, i, short, re.findall(r"^.*" + re.escape(name) + r".*$\n(?:^(?!test ).*$\n){0,12}", text, re.M)))
        if rc == 0:
            module_ok[label] += 1
        lines.append(f"{label:5} run {i:2} load {load:5.2f} exit {rc} {took:6.1f} s  {' '.join(res)}  "
                     f"| {result[0] if result else 'no test result line'}")
lines.append("")
for label, _, _ in bins:
    ls = sorted(loads[label])
    lines.append(f"{label}: " + " ".join(f"{s} {n}/{runs}" for s, n in tally[label].items())
                 + f"; module exit 0 in {module_ok[label]}/{runs}; load at the runs' start {ls[0]:.2f}-{ls[-1]:.2f}, "
                 f"median {ls[len(ls) // 2]:.2f}")
for label, i, short, ctx in fails:
    lines.append(f"FAIL {label} run {i} {short}:")
    lines.extend("    " + x for c in ctx for x in c.split("\n")[:14])
open(out, "w").write("\n".join(lines) + "\n")
print("\n".join(lines[-len(bins) - len(fails) - 1:]))
sys.exit(0 if all(n == runs for t in tally.values() for n in t.values()) else 1)
