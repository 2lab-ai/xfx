#!/usr/bin/env python3
"""The quarantine mutants against the lib's clock-free threshold cases.

    python3 -I lib_threshold_mutants.py <clone> <cargo-target-dir>

The same three mutants as quarantine_mutants.py (src/tui/input.rs), on a
throwaway clone, each against the two lib cases that pin the quarantine's
threshold without a clock (src/tui/input.rs's
a_quarantine_ends_only_after_two_empty_checks_a_quiet_period_apart and
a_quarantine_ends_at_the_first_empty_check_at_or_past_the_quiet). Prints one
line per run; prints no path. Exits 1 if the baseline failed or a mutant
survived.
The join with train 16 bounds each cargo run, its build included (TIMEOUT):
a run past it has its process group killed and is printed TIMED OUT.
"""
import os
import signal
import subprocess
import sys

clone, target = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target
TIMEOUT = 1200


def bounded(command):
    """`subprocess.run` with each run bounded by TIMEOUT, its build included:
    past it the run's process group is killed and the returncode is None."""
    proc = subprocess.Popen(command, cwd=clone, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        out, err = proc.communicate(timeout=TIMEOUT)
        code = proc.returncode
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        code = None
    return subprocess.CompletedProcess(command, code, out, err)
QUIET = "Some(since) if whole && now.saturating_duration_since(since) >= Self::PASTE_QUIET => {"
CASES = ["a_quarantine_ends_only_after_two_empty_checks_a_quiet_period_apart",
         "a_quarantine_ends_at_the_first_empty_check_at_or_past_the_quiet"]
MUTANTS = [
    ("baseline", None),
    ("quarantine-ends-at-any-quiet", "Some(_) if whole => {"),
    ("quarantine-quiet-5ms",
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(5) => {"),
    ("quarantine-quiet-20ms",
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(20) => {"),
]
path = os.path.join(clone, "src/tui/input.rs")
bad = 0
for name, new in MUTANTS:
    if new is not None:
        text = open(path).read()
        assert text.count(QUIET) == 1
        open(path, "w").write(text.replace(QUIET, new))
    try:
        done = bounded(["cargo", "test", "--locked", "--lib", "--"] + CASES)
    finally:
        if new is not None:
            subprocess.run(["git", "-C", clone, "checkout", "--", "src/tui/input.rs"], env=env, check=True)
    failed = [l.split()[1] for l in done.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
    passed = [l.split()[1] for l in done.stdout.splitlines() if l.startswith("test ") and l.endswith(" ok")]
    if done.returncode is None:
        print(f"{name}: TIMED OUT after {TIMEOUT} s")
        bad += 1
    elif new is None:
        ok = done.returncode == 0 and len(passed) == 2
        print(f"baseline: {'green' if ok else 'NOT GREEN'}, passed {passed}")
        bad += not ok
    elif failed:
        print(f"{name}: killed by {', '.join(failed)}")
    else:
        print(f"{name}: SURVIVED (exit {done.returncode})")
        bad += 1
sys.exit(1 if bad else 0)
