"""LOCAL-PATHS round 3: each mutant of scripts/fxcap/roots.py or capture.sh,
applied to a throwaway clone's copy (never a checkout anyone works in), must
be failed by the harness tests that pin it.

usage: python3 -I roots-mutants.py <clone>     (run it under the suite lock)
"""
import os
import subprocess
import sys

clone = sys.argv[1]
FX = os.path.join(clone, "scripts", "fxcap")
TESTS = ["RootsRewrite", "RunRoots.test_a_tmpdir_with_a_trailing_slash",
         "RunRoots.test_a_root_each_side_broke_at_another_column_reads_different",
         "RunRoots.test_a_sibling_of_the_harness_tmpdir_is_left_and_refuses_nothing"]
M = [
    # item 1: the placeholder over the root's own columns
    ("roots.py", "placeholder on the first span only", "        edits.append((s, e, ph[at : at + w]))", "        edits.append((s, e, ph if at == 0 else b\"\"))"),
    ("roots.py", "placeholder not as wide as its root", "    if width >= len(full):\n        return full[:-1] + FILL * (width - len(full)) + \">\"", "    if True:\n        return full"),
    ("roots.py", "no short form", "    if width >= 4:\n", "    if False:\n"),
    # item 3: survivors by the rewrite's rule
    ("roots.py", "survivors by substring", "        whole_bytes = re.search(re.escape(needle) + rb\"(?![\" + EDGE + rb\"])\", data)\n        if whole_bytes or _find(view, needle, []):", "        if needle in data or needle in view.text:"),
    # item 5: a trailing-slash TMPDIR
    ("capture.sh", "no trailing-slash strip", "  while [[ $tmpdir == */ && $tmpdir != / ]]; do tmpdir=${tmpdir%/}; done", "  :"),
    ("roots.py", "no normpath spelling", "        forms = [(given, False), (os.path.normpath(given), False)]", "        forms = [(given, False)]"),
    # NITs and NTHs
    ("roots.py", "no nF escape", "|\\x1b[\\x20-\\x2f]*[\\x30-\\x7e]\")", "|\\x1b[\\x20-\\x7e]\")"),
    ("roots.py", "a backslash escape", "        edits.append((end, end, b\"!\"))", "        edits.append((end, end, b\"\\\\\"))"),
]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
for name in ("roots.py", "capture.sh"):
    with open(os.path.join(FX, name)) as f:
        globals()["orig_" + name.replace(".", "_")] = f.read()


def run():
    return subprocess.run(["python3", "-I", os.path.join(FX, "test_capture.py")] + TESTS, capture_output=True, text=True,
                          env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=900)


for target, label, old, new in M:
    path = os.path.join(FX, target)
    orig = globals()["orig_" + target.replace(".", "_")]
    assert orig.count(old) == 1, (label, orig.count(old))
    with open(path, "w") as f:
        f.write(orig.replace(old, new))
    r = run()
    failed = sorted({line.split("(")[0].split(" ", 1)[1].strip() for line in r.stderr.splitlines() if line.startswith(("FAIL:", "ERROR:"))})
    print("%-36s exit %d  failed: %s" % (label, r.returncode, ", ".join(failed) or "none"))
    with open(path, "w") as f:
        f.write(orig)
r = run()
print("%-36s exit %d  %s" % ("unmutated", r.returncode, (r.stderr.strip().splitlines() or ["?"])[-1]))
