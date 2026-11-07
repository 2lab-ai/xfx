#!/usr/bin/env python3
"""FLAKE-HINT4's timer-window lint (`capfile.races`, the check
test_sweep.py's `Capfile.test_no_snap_of_the_repos_scenarios_races_a_timer`
runs) over every scenario of each commit's tree, and every `models2` line.

    python3 -I -B lint.py <repo> <lint-commit> <commit>...

The lint is <lint-commit>'s `scripts/fxcap/capfile.py`, read out with
`git archive`; each <commit>'s `scripts/fxcap/scenarios` is read out the same
way and given to it, so a commit from before FLAKE-HINT4 is read by the same
check its scenarios meet on the train. Each race is printed when a commit has
races in six scenarios or fewer; past that, the scenarios' names. A `models2` line in the old timed
form (`models2 <catalog.json> <ms>`, which FLAKE-HINT4 refuses before
launch) is counted apart.
"""
import os
import re
import subprocess
import sys
import tempfile


def archive(repo, commit, path, into):
    tar = subprocess.run(["git", "-C", repo, "archive", commit, path], check=True, capture_output=True).stdout
    subprocess.run(["tar", "-x", "-C", into], input=tar, check=True)


def main():
    repo, lint, commits = sys.argv[1], sys.argv[2], sys.argv[3:]
    with tempfile.TemporaryDirectory() as code:
        archive(repo, lint, "scripts/fxcap", code)
        sys.path.insert(0, os.path.join(code, "scripts", "fxcap"))
        import capfile
        print(f"# lint: capfile.py at {lint}")
        bad = 0
        for commit in commits:
            label = subprocess.run(["git", "-C", repo, "log", "-1", "--format=%h %s", commit],
                                   check=True, capture_output=True, text=True).stdout.strip()
            with tempfile.TemporaryDirectory() as d:
                archive(repo, commit, "scripts/fxcap/scenarios", d)
                where = os.path.join(d, "scripts", "fxcap", "scenarios")
                us = list(capfile.units(where))
                raced = [line for u in us for side in ("fx", "xfx") for line in capfile.races(u.reading(side))]
                m2 = []
                for name in sorted(os.listdir(where)):
                    if name.endswith(".cap"):
                        for line in open(os.path.join(where, name), encoding="utf-8").read().splitlines():
                            if re.match(r"(?:fx: |xfx: )?models2\b", line):
                                m2.append((name, line))
                old = [x for x in m2 if re.fullmatch(r"(?:fx: |xfx: )?models2 \S+ \d+", x[1])]
            print(f"== {label[:110]}")
            names = sorted({line.split(" (")[0] for line in raced})
            print(f"  {len(us)} scenarios; {len(raced)} races, in {len(names)} scenarios")
            if len(names) > 6:
                print("    " + ", ".join(names))
            else:
                for line in raced:
                    print("    " + line)
            print(f"  models2 lines: {len(m2)}; in the old timed form: {len(old)}")
            for name, line in m2:
                print(f"    {name}: {line}")
            bad += len(raced) + len(old)
    sys.exit(1 if bad else 0)


main()
