#!/usr/bin/env python3
"""Mutants of scripts/check-no-local-paths.sh, each run through the self-test.

    mutants.py <throwaway clone> [<mutant name>...]
                                   (a `git clone --no-local`, at the head under test;
                                   names limit the run to those mutants)

Each mutant is one textual change to the clone's check; the clone's
self-test then runs and its failing cases are printed. The check is restored
after each. Run it on a throwaway clone only: a mutant is the check without
one of its guards.
"""
import os
import subprocess
import sys

clone = os.path.abspath(sys.argv[1])
check = os.path.join(clone, "scripts", "check-no-local-paths.sh")
test = os.path.join(clone, "scripts", "test-check-no-local-paths.sh")
pristine = open(check).read()

MUTANTS = [
    ("binary lines read as text",
     '>>"$trace"\n\t\t\tcontinue\n\t\tfi\n\t\t# A mark', '>>"$trace"\n\t\t\t:\n\t\tfi\n\t\t# A mark', 1),
    ("binary only by its first 8000 bytes",
     'probe="$(tr -d -c \'\\000\' <"$root/$path" | wc -c)"',
     'probe="$(head -c 8000 "$root/$path" | tr -d -c \'\\000\' | wc -c)"', 1),
    ("an unaccounted line passes",
     'if [ -z "$hit" ] && [ "$removed" -eq 0 ]; then', 'if false; then', 1),
    ("no look for missing files",
     'if [ -s "$work/deleted" ]; then', 'if false; then', 1),
    ("a pattern the shell cannot compile ignored",
     '\t\tuncompiled="$2"\n', '\t\t:\n', 1),
    ("a placeholder home taken out is not counted",
     '\t\t\tbare="${bare%%"$found"*}${bare#*"$found"}"\n\t\t\tremoved=$((removed + 1))\n',
     '\t\t\tbare="${bare%%"$found"*}${bare#*"$found"}"\n', 1),
    ("a documented home taken out is not counted",
     '\t\t\tbare="${bare%%"$found"*}${after}${bare#*"$found"}"\n\t\t\tremoved=$((removed + 1))\n',
     '\t\t\tbare="${bare%%"$found"*}${after}${bare#*"$found"}"\n', 1),
    ("a line with no number accepted",
     "\t\t'' | *[!0-9]*)\n", "\t\t'' | *[!0-9]*x)\n", 1),
    ("a scan that did not finish passes",
     'if [ "$scan_status" -ne 0 ]; then', 'if false; then', 1),
    # Round 2.
    ("a mark by the regex alone",
     '[[ $text == *"$marker"* ]] && rematch "$text" "$marked_re"', 'rematch "$text" "$marked_re"', 1),
    ("a binary line not counted",
     '\t\t\tbinary_lines=$((binary_lines + 1))\n', '', 1),
    ("one file's lines dropped, the count kept",
     '\t\tgiven=$((given + 1))\n', '\t\tgiven=$((given + 1))\n\t\tif [ "$path" = w3/trace-report.md ]; then continue; fi\n', 1),
    ("the count not checked",
     'if [ "$given" -ne $((refused + binary_lines + marked + placeheld)) ]; then', 'if false; then', 1),
    # Round 3.
    ("the second reading not compared",
     'if ! cmp -s "$tally" "$counted"; then', 'if false; then', 1),
    ("symlinks not looked at",
     '\t\t120000\\ *) ;;\n', '\t\t120000\\ *) continue ;;\n', 1),
    ("a run that stops early passes (the EXIT trap's guard)",
     """trap 'rm -rf "$work"; [ "$finished" = 1 ] || exit 1' EXIT""", """trap 'rm -rf "$work"' EXIT""", 1),
    ("a symlink listing cut inside its last entry accepted",
     """\tif [ -n "$entry" ]; then\n\t\tprintf 'check-no-local-paths: git ls-files ended inside""", """\tif false; then\n\t\tprintf 'check-no-local-paths: git ls-files ended inside""", 1),
    ("a tracked listing cut inside its last entry accepted",
     """if [ -n "$entry" ]; then\n\tfail "git ls-files ended inside""", """if false; then\n\tfail "git ls-files ended inside""", 1),
    ("a stream that ends inside a record accepted",
     '\tif [ -n "$path" ]; then\n\t\tprintf \'check-no-local-paths: git grep ended inside', '\tif false; then\n\t\tprintf \'check-no-local-paths: git grep ended inside', 1),
]

env = dict(os.environ)
only = set(sys.argv[2:])
for name, old, new, count in MUTANTS:
    if only and name not in only:
        continue
    if pristine.count(old) != count:
        print(f"{name}: SKIPPED, the text to change is there {pristine.count(old)} time(s)")
        continue
    with open(check, "w") as f:
        f.write(pristine.replace(old, new))
    run = subprocess.run([test], cwd=clone, env=env, stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    failed = [l.split(": FAIL ", 1)[1].split(":", 1)[0]
              for l in run.stdout.splitlines() if ": FAIL " in l]
    verdict = "killed" if run.returncode != 0 else "SURVIVED"
    print(f"{name}: {verdict} (exit {run.returncode}; failing: {', '.join(failed) or 'none'})")
with open(check, "w") as f:
    f.write(pristine)
