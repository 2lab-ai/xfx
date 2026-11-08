"""LAUNCH-SETTLE's mutants: each one change to the harness, the check or the
sweep, made on a throwaway copy, and the test that must fail on it.

usage: python3 -I mutants.py <clone> > mutants.txt

<clone> is a `git clone --no-local` of the repository at the commit under
test; each mutant is applied to its working tree, its test run, and the tree
put back (`git checkout -- .`). Prints one line per mutant: KILLED when the
test fails (exit != 0), SURVIVED when it passes, NOT APPLIED when the text to
change is not in the file exactly once; and the test's last line.
"""

import os
import subprocess
import sys

FX = "scripts/fxcap/"
SWEEP = ["python3", "-I", FX + "test_sweep.py"]
CAPTURE = ["python3", "-I", FX + "test_capture.py"]
CAP = FX + "capture.sh"
CHECK = FX + "capfile.py"

MUTANTS = [
    ("L1 no launch wait at launch", CAP,
     '  else\n    await_up "$sock" "$run" "$keep"\n  fi\n\n  while IFS= read -r raw; do',
     '  else\n    : > "$run/launched"\n  fi\n\n  while IFS= read -r raw; do',
     CAPTURE + ["LaunchWait.test_the_first_line_waits_for_the_first_frame"]),
    ("L2 no launch wait at a relaunch", CAP,
     '        else\n          await_up "$sock" "$run" "$keep"\n        fi\n',
     '        else\n          : > "$run/launched"\n        fi\n',
     CAPTURE + ["LaunchWait.test_a_relaunch_waits_for_its_first_frame_too"]),
    ("L3 a blank screen counts as a frame", CAP,
     '    if [[ $screen == *[![:space:]]* ]]; then\n      echo "up: a first frame',
     '    if true; then\n      echo "up: a first frame',
     CAPTURE + ["LaunchWait.test_the_first_line_waits_for_the_first_frame"]),
    ("L4 the 30 s bound gives up silently", CAP,
     'the binary never drew a frame" >&2\n      exit 1\n',
     'the binary never drew a frame" >&2\n      break\n',
     CAPTURE + ["LaunchWait.test_a_binary_that_never_draws_fails_the_run"]),
    ("L5 a dead pane under keep fails the run", CAP,
     '      [[ -n $keep ]] || { echo "up: the pane is dead',
     '      false || { echo "up: the pane is dead',
     CAPTURE + ["LaunchWait.test_a_binary_gone_at_once_is_final_under_keep_and_fails_the_run_without"]),
    ("L6 a gone pane is taken for up", CAP,
     'the binary ended before its first frame, and no \\`keep\\` held the pane" >&2; exit 1; }',
     'the binary ended before its first frame, and no \\`keep\\` held the pane" >&2; break; }',
     CAPTURE + ["LaunchWait.test_a_binary_gone_at_once_is_final_under_keep_and_fails_the_run_without"]),
    # The launch's moment, not the first frame's: `launched` takes the mtime
    # `up` was stamped with when the wait began. (A first form stamped
    # `launched` there too, and the stamp after the loop overwrote it: a no-op
    # mutant, which survived; mutants.txt keeps that line.)
    ("L7 `since launch` counts from the launch", CAP,
     '  done\n  : > "$run/launched"\n}\n',
     '  done\n  touch -r "$run/up" "$run/launched"\n}\n',
     CAPTURE + ["LaunchWait.test_since_launch_counts_from_the_first_frame"]),
    ("L8 nodraw not honoured", CAP,
     '  if [[ -n $nodraw ]]; then\n    echo "up: not waited for (nodraw)"',
     '  if false; then\n    echo "up: not waited for (nodraw)"',
     CAPTURE + ["LaunchWait.test_nodraw_runs_a_stub_that_draws_nothing_at_once"]),
    ("L9 `seen` kept over a relaunch", CAP,
     '        # nothing of what this one showed.\n        : > "$run/seen"\n',
     '        # nothing of what this one showed.\n',
     CAPTURE + ["Verbs.test_never_seen_is_of_the_binary_since_its_last_launch"]),
    ("L10 `expect` reads any frame of the name", CAP,
     '        grep -qxF -- "$label" "$run/snapped" && [[ -f $frame ]] ||',
     '        [[ -f $frame ]] ||',
     CAPTURE + ["Verbs.test_expect_reads_only_a_frame_this_run_wrote"]),
    ("L11 a snapfull label is not recorded as <label>.full", CAP,
     """        printf '%s\\n' "$rest.full" >> "$run/snapped"\n""",
     """        printf '%s\\n' "$rest" >> "$run/snapped"\n""",
     CAPTURE + ["Verbs.test_expect_reads_only_a_frame_this_run_wrote"]),
    ("C1 `absent` counts over a launch-armed timer (the base rule)", CHECK,
     '(seen or (by_key and clock[0] >= timer.settle))',
     '(verb != "absent" or clock[0] >= (timer.settle or 3.0))',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C2 an `until shown` does not count as seen", CHECK,
     '                clock, frozen, seen = (0.0, clock[1] if clock else since_input[1]), False, True\n',
     '                clock, frozen, seen = (0.0, clock[1] if clock else since_input[1]), False, False\n',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C3 no settle after a key", CHECK,
     '(by_key and clock[0] >= timer.settle)', '(by_key)',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C4 a relaunch keeps `seen`", CHECK,
     '                clock, frozen, seen, by_key = (0.0, 0.0) if at_launch else None, False, False, False\n',
     '                clock, frozen, by_key = (0.0, 0.0) if at_launch else None, False, False\n',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C5 a paused row's `absent` counts once seen", CHECK,
     'elif names and not frozen and (seen or',
     'elif names and (not frozen or seen) and (seen or',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C6 the warning keeps a settle", CHECK,
     '        pauses=re.compile(r"^key .*\\b(?:C-c|C-o|Escape)\\b|^type /"),\n    ),',
     '        pauses=re.compile(r"^key .*\\b(?:C-c|C-o|Escape)\\b|^type /"),\n        settle=3.0,\n    ),',
     SWEEP + ["Capfile.test_settle_is_an_allowance_for_a_key_only"]),
    # Round 2 (the reviews of a0f7edef).
    ("C7 `until gone` counts over an unseen timer (round 1's rule)", CHECK,
     '(seen or (by_key and clock[0] >= timer.settle))',
     '(verb != "absent" or seen or (by_key and clock[0] >= timer.settle))',
     SWEEP + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("C8 `harness_only` names nothing", CHECK,
     '        if line.partition(" ")[0] in HARNESS_ONLY\n',
     '        if False\n',
     SWEEP + ["Capfile.test_nodraw_is_the_harness_tests_own"]),
    ("C9 the sweep runs a scenario with `nodraw`", FX + "sweep.py",
     '    if stubs:\n        raise Refusal(',
     '    if False:\n        raise Refusal(',
     SWEEP + ["Sweep.test_a_scenario_with_nodraw_runs_nothing"]),
]


def main(clone):
    for name, path, old, new, test in MUTANTS:
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        if text.count(old) != 1:
            print("%s: NOT APPLIED (the text is in %s %d times)" % (name, path, text.count(old)), flush=True)
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
        r = subprocess.run(test, cwd=clone, capture_output=True, text=True, timeout=900)
        last = [line for line in (r.stdout + r.stderr).strip().split("\n") if line][-1]
        print("%s: %s (%s, exit %d: %s)" % (name, "KILLED" if r.returncode else "SURVIVED", test[-1], r.returncode, last), flush=True)
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)


if __name__ == "__main__":
    main(sys.argv[1])
