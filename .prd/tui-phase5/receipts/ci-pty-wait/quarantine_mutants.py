#!/usr/bin/env python3
"""Can a broken paste quarantine go green only through the harness's retry?

    python3 -I quarantine_mutants.py <clone> <cargo-target-dir> <runs>

Each mutant breaks the product's quarantine (src/tui/input.rs) on a throwaway
clone -- never the worktree -- and runs the three hostile-paste cases `runs`
times each (the two unpaced ones, and the one whose terminal pauses 20 ms
between parts, under the quiet that ends the quarantine), one at a time, with --nocapture so each session's verdict is in
the output: "the paste's longest pause: ..." for a session the writer did not
pause, "a timing-indeterminate session" for one it did. Every run is put in
one class:
  green                      -- the case passed;
  red on an unpaced session  -- it failed after a session that was not
                                rejected (the regression caught there);
  red, every session paused  -- the retry gave up, saying so;
  red, other                 -- anything else.
The baseline (no mutant) is run the same way. A mutant the retry could hide
would show green runs, or only "every session paused" ones. Prints one line
per run and a summary per mutant; prints no path. Exits 1 if a mutant run
was green.
The join with train 16 bounds each cargo run, its build included (TIMEOUT):
a run past it has its process group killed and is printed TIMED OUT.
"""
import os
import signal
import subprocess
import sys

clone, target, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
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
CASES = [
    "a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing",
    "a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval",
    "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing",
]
QUIET = "Some(since) if whole && now.saturating_duration_since(since) >= Self::PASTE_QUIET => {"
MUTANTS = [
    ("baseline", None, None),
    # The quarantine ends at the second check that finds nothing, however
    # soon: every refill gap ends it.
    ("quarantine-ends-at-any-quiet", QUIET, "Some(_) if whole => {"),
    # Timing-dependent: it ends after 5 ms or 20 ms of quiet instead of 50,
    # so it shows only in a session whose refill gaps reach that -- every
    # such session the harness accepts, since it rejects only at 50 ms.
    ("quarantine-quiet-5ms", QUIET,
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(5) => {"),
    ("quarantine-quiet-20ms", QUIET,
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(20) => {"),
]
PATH = "src/tui/input.rs"


def classify(out):
    if "test result: ok." in out:
        return "green"
    if "no session tested the paste" in out:
        return "red, every session paused"
    if "the paste's longest pause:" in out:
        return "red on an unpaced session"
    return "red, other"


green_mutant = False
for name, old, new in MUTANTS:
    file = os.path.join(clone, PATH)
    if old is not None:
        text = open(file).read()
        assert text.count(old) == 1, name
        open(file, "w").write(text.replace(old, new))
    try:
        build = bounded(["cargo", "test", "--locked", "--test", "tui", "--no-run"])
        if build.returncode != 0:
            print(f"{name}: BUILD ERROR {build.stderr.strip().splitlines()[-1:]}")
            green_mutant = True
            continue
        tally = {}
        for case in CASES:
            for n in range(1, runs + 1):
                done = bounded(
                    ["cargo", "test", "--locked", "--test", "tui", "--", "--exact", case,
                     "--nocapture"])
                out = done.stdout + done.stderr
                verdict = "TIMED OUT" if done.returncode is None else classify(out)
                pauses = [l.split(":", 1)[1].strip() for l in out.splitlines()
                          if l.startswith("the paste's longest pause:")]
                rejected = out.count("a timing-indeterminate session")
                first = next((l for l in out.splitlines() if "panicked at" in l), "")
                print(f"{name} {case} run {n}: {verdict}; unpaced sessions' pauses {pauses}; "
                      f"rejected sessions {rejected}; {first.split(' panicked at ')[-1][:60]}")
                tally[verdict] = tally.get(verdict, 0) + 1
                if old is not None and verdict == "green":
                    green_mutant = True
        print(f"{name}: {tally}")
    finally:
        if old is not None:
            subprocess.run(["git", "-C", clone, "checkout", "--", PATH], env=env, check=True)
sys.exit(1 if green_mutant else 0)
