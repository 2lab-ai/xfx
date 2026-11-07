"""FLAKE-HINT4's mutants: each one change to the harness, the check or a
scenario, made on a throwaway copy, and the test that must fail on it.

usage: python3 -I mutants.py <clone> > mutants.txt

<clone> is a `git clone --no-local` of the repository at the commit under
test; each mutant is applied to its working tree, its test run, and the tree
put back (`git checkout -- .`). Prints one line per mutant: KILLED when the
test fails (exit != 0), SURVIVED when it passes, and the test's last line.
"""

import os
import subprocess
import sys

FX = "scripts/fxcap/"
SWEEP_TESTS = ["python3", "-I", FX + "test_sweep.py"]
CAPTURE_TESTS = ["python3", "-I", FX + "test_capture.py"]

MUTANTS = [
    ("M1 records.cap back to `wait 4` / `snap start`", FX + "scenarios/p5-transcript-records.cap",
     "until shown Full access\nwait 4\nuntil gone Full access\nsnap start\n",
     "wait 4\nsnap start\n",
     SWEEP_TESTS + ["Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer"]),
    ("M2 no window: BEFORE = AFTER = 0", FX + "capfile.py",
     "BEFORE = 1.0\nAFTER = 3.0\n", "BEFORE = 0.0\nAFTER = 0.0\n",
     SWEEP_TESTS + ["Capfile.test_a_snap_inside_the_warnings_window_is_refused"]),
    ("M3 an `until` on any row takes the timer down", FX + "capfile.py",
     'names = (verb == "until" and said == timer.text)', 'names = (verb == "until")',
     SWEEP_TESTS + ["Capfile.test_a_snap_inside_the_warnings_window_is_refused"]),
    ("M4 `held` counted as no time", FX + "capfile.py",
     '"held": None,', '"held": 0.0,',
     SWEEP_TESTS + ["Capfile.test_a_snap_inside_the_warnings_window_is_refused"]),
    ("M5 a relaunch does not arm it again", FX + "capfile.py",
     '            if verb == "relaunch":\n                clock, frozen = (0.0, 0.0) if at_launch else None, False\n',
     '            if verb == "relaunch":\n                pass\n',
     SWEEP_TESTS + ["Capfile.test_a_snap_inside_the_warnings_window_is_refused"]),
    ("M6 a seeded profile is not read", FX + "capfile.py",
     'line.startswith(("seed ", "homefile ", "reseed ", "rehomefile "))', 'line.startswith(("nothing ",))',
     SWEEP_TESTS + ["Capfile.test_a_profile_in_full_access_arms_it_too"]),
    ("M7 fx's `full-access` spelling not matched", FX + "capfile.py",
     "PERMISSION_MODE=(?:yolo|full-access)", "PERMISSION_MODE=(?:yolo)",
     SWEEP_TESTS + ["Capfile.test_a_snap_inside_the_warnings_window_is_refused"]),
    ("M8 other keystrokes leave the exit hint up", FX + "capfile.py",
     "        keys=(\"C-c\",),\n        disarms=True,\n", "        keys=(\"C-c\",),\n        disarms=False,\n",
     SWEEP_TESTS + ["Capfile.test_a_ctrl_c_arms_the_exit_hint_and_any_other_keystroke_takes_it_down"]),
    ("M9 the exit hint dropped from TIMERS", FX + "capfile.py",
     '        keys=("C-c",),\n', '        keys=(),\n',
     SWEEP_TESTS + ["Capfile.test_a_ctrl_c_arms_the_exit_hint_and_any_other_keystroke_takes_it_down"]),
    ("M14 a Ctrl-C, an Escape or a menu inside the window not counted", FX + "capfile.py",
     '        pauses=re.compile(r"^key .*\\b(?:C-c|C-o|Escape)\\b|^type /"),\n', "",
     SWEEP_TESTS + ["Capfile.test_a_line_that_takes_the_warnings_row_inside_its_window_is_refused"]),
    ("M15 interrupt-next-cmd's Ctrl-C back inside the window", FX + "scenarios/interrupt-next-cmd.cap",
     "until gone Full access\nkey C-c\n", "key C-c\n",
     SWEEP_TESTS + ["Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer"]),
    ("M16 a line that takes the row before the window stops nothing", FX + "capfile.py",
     "            elif pauses and clock[1] <= timer.seconds - BEFORE:\n                frozen = True\n", "",
     SWEEP_TESTS + ["Capfile.test_a_line_that_takes_the_warnings_row_before_its_window_stops_its_clock"]),
    # Round 2 (the reviews of 9f968a65).
    ("N1 an `until` with a longer text counts (round 1's substring)", FX + "capfile.py",
     'names = (verb == "until" and said == timer.text)', 'names = (verb == "until" and timer.text in said)',
     SWEEP_TESTS + ["Capfile.test_an_until_or_absent_counts_only_with_the_timers_exact_text"]),
    ("N2 `absent` not counted", FX + "capfile.py",
     ' or (verb == "absent" and rest == timer.text)', '',
     SWEEP_TESTS + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("N3 Ctrl-O does not pause the warning", FX + "capfile.py",
     "(?:C-c|C-o|Escape)", "(?:C-c|Escape)",
     SWEEP_TESTS + ["Capfile.test_a_line_that_takes_the_warnings_row_inside_its_window_is_refused"]),
    ("N4 a paste and a `key -H` report take the exit hint down", FX + "capfile.py",
     '(verb == "type" or (verb == "key" and not rest.startswith("-H ")))', '(verb in ("type", "pastex") or verb == "key")',
     SWEEP_TESTS + ["Capfile.test_a_ctrl_c_arms_the_exit_hint_and_any_other_keystroke_takes_it_down"]),
    ("N5 gone while the row is paused counts", FX + "capfile.py",
     "            elif names and not frozen:\n", "            elif names:\n",
     SWEEP_TESTS + ["Capfile.test_absent_says_a_ctrl_c_armed_nothing"]),
    ("N6 an `until shown` nothing counted armed starts at 0", FX + "capfile.py",
     "clock[1] if clock else since_input[1]", "clock[1] if clock else 0.0",
     SWEEP_TESTS + ["Capfile.test_an_until_shown_nothing_counted_armed_reaches_back_to_the_last_input"]),
    ("N7 `reseed`/`rehomefile` not read", FX + "capfile.py",
     'line.startswith(("seed ", "homefile ", "reseed ", "rehomefile "))', 'line.startswith(("seed ", "homefile "))',
     SWEEP_TESTS + ["Capfile.test_a_profile_in_full_access_arms_it_too"]),
    ("N8 an `until` on another row counted as no time", FX + "capfile.py",
     '"until": None}', '"until": 0.0}',
     SWEEP_TESTS + ["Capfile.test_an_until_on_another_row_and_a_signal_take_as_long_as_they_take"]),
    ("N9 `signal` counted as no time", FX + "capfile.py",
     '"signal": None,', '"signal": 0.0,',
     SWEEP_TESTS + ["Capfile.test_an_until_on_another_row_and_a_signal_take_as_long_as_they_take"]),
    ("N10 permissions-midturn without its `absent`", FX + "scenarios/permissions-midturn.cap",
     "absent press ctrl+c again to exit\n", "",
     SWEEP_TESTS + ["Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer"]),
    ("N11 `absent` never fails", FX + "capture.sh",
     '        if [[ $screen == *"$rest"* ]]; then\n', '        if false; then\n',
     CAPTURE_TESTS + ["Verbs.test_absent_passes_a_screen_without_its_text_and_fails_one_with_it"]),
    ("N12 `expect` never fails", FX + "capture.sh",
     '        if ! grep -qF -- "$text" "$frame"; then\n', '        if false; then\n',
     CAPTURE_TESTS + ["Verbs.test_expect_checks_the_frame_a_snap_wrote"]),
    ("N13 `until gone` never says never seen", FX + "capture.sh",
     "$([[ $cond == gone && -z $seen ]] && echo '; never seen')", "",
     CAPTURE_TESTS + ["Verbs.test_until_gone_says_never_seen_when_its_text_was_not_there_at_all"]),
    ("N15 `never seen` counts only the `until gone`'s own looks again", FX + "capture.sh",
     'elif grep -qxF -- "$text" "$run/seen"; then seen=1; fi', 'fi',
     CAPTURE_TESTS + ["Verbs.test_until_gone_says_never_seen_when_its_text_was_not_there_at_all"]),
    ("N14 `until` gives up after 600 looks again", FX + "capture.sh",
     "          if (( SECONDS - began >= 30 )); then\n", "          if (( looks >= 600 )); then\n",
     CAPTURE_TESTS + ["Verbs.test_until_fails_the_run_when_the_row_never_comes"]),
    ("N16 the second catalog at a fixed 5 s from the gateway's start again", FX + "fakegw.ts",
     "(await Bun.file(process.env.FAKEGW_MODELS2).exists())", "Date.now() - started >= 5000",
     CAPTURE_TESTS + ["FakeGateway.test_models2_swaps_the_catalog_with_the_times_logged"]),
    ("N17 a `models2` line puts nothing in place", FX + "capture.sh",
     '      models2) cp "$scenarios/$rest" "$run/models2.tmp"; mv -f "$run/models2.tmp" "$run/models2.json" ;;\n',
     "      models2) ;;\n",
     CAPTURE_TESTS + ["Verbs.test_models2_and_modelsdelay"]),
    ("M10 the sweep does not refuse a race", FX + "sweep.py",
     "    if raced:\n", "    if raced and False:\n",
     SWEEP_TESTS + ["Sweep.test_a_snap_that_races_a_timer_runs_nothing"]),
    ("M11 `until` gives up silently at 30 s", FX + "capture.sh",
     "            printf '%s\\n' \"$screen\" | grep -v '^[[:space:]]*$' | sed 's/^/  | /' >&2 || true\n            exit 1\n",
     "            break\n",
     CAPTURE_TESTS + ["Verbs.test_until_fails_the_run_when_the_row_never_comes"]),
    ("M12 `until gone` waits for shown", FX + "capture.sh",
     "if [[ $cond == shown ]]; then (( on == 0 )) || break; else (( on == 1 )) || break; fi",
     "(( on == 0 )) || break",
     CAPTURE_TESTS + ["Verbs.test_until_waits_for_a_row_to_come_and_then_to_go"]),
    ("M13 `until` matches the text as a glob", FX + "capture.sh",
     '[[ $screen != *"$text"* ]] || on=1', '[[ $screen != *$text* ]] || on=1',
     CAPTURE_TESTS + ["Verbs.test_until_waits_for_a_row_to_come_and_then_to_go"]),
]


def main(clone):
    for name, path, old, new, test in MUTANTS:
        full = os.path.join(clone, path)
        with open(full, encoding="utf-8") as f:
            text = f.read()
        if text.count(old) != 1:
            print("%s: NOT APPLIED (the text is in %s %d times)" % (name, path, text.count(old)))
            continue
        with open(full, "w", encoding="utf-8") as f:
            f.write(text.replace(old, new))
        r = subprocess.run(test, cwd=clone, capture_output=True, text=True, timeout=900)
        last = [line for line in (r.stdout + r.stderr).strip().split("\n") if line][-1]
        print("%s: %s (%s, exit %d: %s)" % (name, "KILLED" if r.returncode else "SURVIVED", test[-1], r.returncode, last), flush=True)
        subprocess.run(["git", "checkout", "--", "."], cwd=clone, check=True)


if __name__ == "__main__":
    main(sys.argv[1])
