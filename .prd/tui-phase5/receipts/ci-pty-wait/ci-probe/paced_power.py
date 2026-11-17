"""How often the paced case catches a broken quarantine, by its pause.

    python3 -I paced_power.py <clone> <cargo-target-dir> <runs> <pause-ms>... [-- <prefix>...]

On a throwaway clone at the code to test (never the worktree): for each
pause, the paced case's pause is set to it (`tests/tui.rs`, the one call
`hostile_paste_types_nothing(std::time::Duration::from_millis(12), ...)`),
and for each of quarantine_mutants.py's mutants (src/tui/input.rs) and the
baseline the fault-injection tui binary is built, and the paced case alone
runs <runs> times through `cargo test`, with <prefix> words before it if
given (e.g. `taskpolicy -c utility`). Each build is run before the next is
made: the test binary runs the `xfx` its own build made. Each run is put in one class:
green; red on an accepted session (the regression caught); red, every
session paused; red, other. The files are put back with `git checkout`.
Prints one line per run and a summary per binary; prints no path.
"""
import os
import subprocess
import sys

args = sys.argv[1:]
prefix = args[args.index("--") + 1:] if "--" in args else []
args = args[:args.index("--")] if "--" in args else args
clone, target, runs, pauses = args[0], args[1], int(args[2]), [int(p) for p in args[3:]]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["CARGO_TARGET_DIR"] = target
CASE = "a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing"
PACED = "    hostile_paste_types_nothing(std::time::Duration::from_millis(12), &[1, 64, 128, 192]);\n"
QUIET = "Some(since) if whole && now.saturating_duration_since(since) >= Self::PASTE_QUIET => {"
MUTANTS = [
    ("baseline", None),
    ("quarantine-ends-at-any-quiet", "Some(_) if whole => {"),
    ("quarantine-quiet-5ms",
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(5) => {"),
    ("quarantine-quiet-20ms",
     "Some(since) if whole && now.saturating_duration_since(since) >= Duration::from_millis(20) => {"),
]
def edit(path, old, new):
    file = os.path.join(clone, path)
    text = open(file).read()
    assert text.count(old) == 1, (path, old[:40], text.count(old))
    open(file, "w").write(text.replace(old, new))


def classify(out):
    if "test result: ok." in out:
        return "green"
    if "no session tested the paste" in out:
        return "red, every session paused"
    if "the paste's longest pause:" in out:
        return "red on an accepted session"
    return "red, other"


for pause in pauses:
    for name, quiet in MUTANTS:
        label = f"{pause}ms.{name}"
        counts = {}
        try:
            edit("tests/tui.rs", PACED, PACED.replace("from_millis(12)", f"from_millis({pause})"))
            if quiet:
                edit("src/tui/input.rs", QUIET, quiet)
            # The test binary runs the `xfx` this build makes (CARGO_BIN_EXE_xfx,
            # a path fixed at compile time), so each mutant is built and run
            # before the next is built.
            done = subprocess.run(["cargo", "test", "--locked", "--features", "fault-injection",
                                   "--test", "tui", "--no-run"], cwd=clone, env=env,
                                  capture_output=True, text=True)
            assert done.returncode == 0, done.stderr[-2000:]
            for n in range(1, runs + 1):
                done = subprocess.run(prefix + ["cargo", "test", "--locked", "--features",
                                                "fault-injection", "--test", "tui", "--", "--exact",
                                                CASE, "--nocapture"],
                                      cwd=clone, env=env, capture_output=True, text=True, timeout=900)
                out = done.stdout + done.stderr
                verdict = classify(out)
                counts[verdict] = counts.get(verdict, 0) + 1
                said = [l.split(";")[0] for l in out.splitlines()
                        if "longest pause:" in l or "timing-indeterminate" in l]
                print(f"{label} run {n}: {verdict}; {said}", flush=True)
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", "tests/tui.rs", "src/tui/input.rs"],
                           env=env, check=True)
        print(f"{label}: {counts}", flush=True)
