"""LOCAL-PATHS-WRAP: roots.py's tests (test_capture.py RootsRewrite) against
roots.py at the base (the RED), then against each mutant of the head's
roots.py, then unmutated. Every change is made to a throwaway clone's copy
(`git clone --no-local`), never a checkout anyone works in. Each line names
the tests that failed, how many subtests, and the test file's line of the
first failed assertion -- never a path.

usage: python3 -I mutants.py <clone> <base rev>     (under the suite lock)
"""
import os
import re
import subprocess
import sys

clone, base = sys.argv[1:3]
FX = os.path.join(clone, "scripts", "fxcap")
ROOTS = os.path.join(FX, "roots.py")
TESTS = ["test_capture.py", "RootsRewrite"]
M = [
    ("drop the `↳` mark", 'MARKS = (b"\\xe2\\x86\\xb3", b"\\xe2\\x86\\x91")', 'MARKS = (b"\\xe2\\x86\\x91",)'),
    ("drop the `↑` mark", 'MARKS = (b"\\xe2\\x86\\xb3", b"\\xe2\\x86\\x91")', 'MARKS = (b"\\xe2\\x86\\xb3",)'),
    ("drop the fail-closed tail check", "if in_control or broken or view.tail_behind_mark(needle) or _find(view, needle, []):",
     "if in_control or broken or _find(view, needle, []):"),
    ("tail check: two bytes or more", "for a in self.marked for n in range(1, len(needle)))",
     "for a in self.marked for n in range(2, len(needle)))"),
    ("the marks no gutter run in a root", "|\\x96[\\x80-\\x9f]|\\x86[\\xb3\\x91]))+\")", "|\\x96[\\x80-\\x9f]))+\")"),
]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["PYTHONDONTWRITEBYTECODE"] = "1"
with open(ROOTS) as f:
    orig = f.read()
old = subprocess.run(["git", "-C", clone, "show", "%s:scripts/fxcap/roots.py" % base], capture_output=True, text=True,
                     env=env, check=True).stdout


def run():
    return subprocess.run(["python3", "-I", os.path.join(FX, TESTS[0])] + TESTS[1:], capture_output=True, text=True,
                          env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=900)


def first_assertion(stderr):
    line = None
    for row in stderr.splitlines():
        m = re.search(r'File ".*/%s", line (\d+)' % re.escape(TESTS[0]), row)
        if m:
            line = m.group(1)
        elif re.match(r"\w*(Error|Exception)\b", row) and line:
            return "%s:%s" % (TESTS[0], line)
    return "-"


def report(label, text):
    with open(ROOTS, "w") as f:
        f.write(text)
    r = run()
    fails = [row for row in r.stderr.splitlines() if row.startswith(("FAIL:", "ERROR:"))]
    tests = sorted({row.split("(")[0].split(" ", 1)[1].strip() for row in fails})
    print("%-36s exit %d, %3d failed (sub)tests, first at %s: %s" % (label, r.returncode, len(fails),
                                                                first_assertion(r.stderr), ", ".join(tests) or "none"))
    with open(ROOTS, "w") as f:
        f.write(orig)
    return r


report("roots.py at the base (%s)" % base[:8], old)
for label, a, b in M:
    assert orig.count(a) == 1, (label, orig.count(a))
    report(label, orig.replace(a, b))
r = report("unmutated", orig)
print("%-36s %s" % ("", (r.stderr.strip().splitlines() or ["?"])[-1]))
