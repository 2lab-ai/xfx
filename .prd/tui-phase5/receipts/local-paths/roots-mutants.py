"""LOCAL-PATHS round 8: each mutant of scripts/fxcap/roots.py, capture.sh or
parity-ledger.toml, applied to a throwaway clone's copy (never a checkout
anyone works in), must be failed by the tests that pin it. Each line names
the tests that failed and the test file's line of the first failed
assertion -- which assertion killed it -- never a path.

usage: python3 -I roots-mutants.py <clone>     (run it under the suite lock)
"""
import os
import re
import subprocess
import sys

clone = sys.argv[1]
FX = os.path.join(clone, "scripts", "fxcap")
CAPTURE = ["test_capture.py", "RootsRewrite", "RunRoots.test_a_tmpdir_with_a_trailing_slash",
           "RunRoots.test_a_root_each_side_broke_at_another_column_reads_different",
           "RunRoots.test_a_sibling_of_the_harness_tmpdir_is_left_and_refuses_nothing"]
SWEEP = ["test_sweep.py", "Normalize"]
IN_CONTROL = """        in_control = any(
            any(m.start() < end and start < m.end() for start, end in view.controls)
            for m in re.finditer(re.escape(needle) + rb"(?![" + EDGE + rb"])", data)
        )
"""
BROKEN = "        broken = any(_find(view, v, []) for v in _broken_at_gutters(needle))\n"
JUDGE = "        if in_control or broken or _find(view, needle, []):"
# capture-tmpdir as round 7 writes it: the run directory's placeholder, then
# `fxtmp` with a directory under it, the one form the captures hold.
PREFIX = "(<scratch(?::real)?_*>/|fxcap\\.XXXXXX/)"
END = "(?=/)"
# Round 6's: one of a list of delimiters after `fxtmp`.
DELIMS = ["/", "\\s", '"', "'", "\\]", "}", ")", ",", ":", ";", "|", ">"]


def rule(prefix=PREFIX, end=END):
    return "fx = '" + prefix + "fxtmp" + end + "'"


def rule6(end):
    # A literal TOML string cannot hold the `'` round 6's list does.
    return "fx = " + "'" * 3 + PREFIX + "fxtmp" + end + "'" * 3


TMPDIR_RULE = rule()
M = [
    # round 3, item 1: the placeholder over the root's own columns
    ("roots.py", CAPTURE, "placeholder on the first span only", "        edits.append((s, e, ph[at : at + w]))", "        edits.append((s, e, ph if at == 0 else b\"\"))"),
    ("roots.py", CAPTURE, "placeholder not as wide as its root", "    if width >= len(full):\n        return full[:-1] + FILL * (width - len(full)) + \">\"", "    if True:\n        return full"),
    ("roots.py", CAPTURE, "no short form", "    if width >= 4:\n", "    if False:\n"),
    # round 3, item 3 (retargeted at round 5's code): survivors by substring
    ("roots.py", CAPTURE, "survivors by substring", IN_CONTROL + BROKEN + JUDGE, "        if needle in data or needle in view.text:"),
    # round 4, item 1: which raw bytes count
    ("roots.py", CAPTURE, "bytes read everywhere (round 3)", IN_CONTROL, "        in_control = re.search(re.escape(needle) + rb\"(?![\" + EDGE + rb\"])\", data)\n"),
    ("roots.py", CAPTURE, "no byte reading at all", IN_CONTROL, "        in_control = False\n"),
    ("roots.py", CAPTURE, "bytes in an OSC only", "            any(m.start() < end and start < m.end() for start, end in view.controls)",
     "            any(m.start() < end and start < m.end() for start, end in view.controls if data.startswith(b\"\\x1b]\", start))"),
    # round 5, item 2: a root read with any of its gutter runs taken out
    ("roots.py", CAPTURE, "no reading at its gutters (round 4)", BROKEN, "        broken = False\n"),
    ("roots.py", CAPTURE, "every gutter out at once (bb5f53ed)", BROKEN,
     "        broken = _find(view, GUTTER_RUN.sub(b\"\", needle), []) if GUTTER_RUN.search(needle) else False\n"),
    ("roots.py", CAPTURE, "one run out at a time only", "        if all(keep):\n", "        if all(keep) or list(keep).count(False) != 1:\n"),
    # round 6, item 2: the rewrite refuses a broken root before any edit
    ("roots.py", CAPTURE, "no refusal in the rewrite (round 5)",
     "        if any(_find(view, v, []) for v in _broken_at_gutters(needle)):\n            raise Refused(",
     "        if False:\n            raise Refused("),
    ("roots.py", CAPTURE, "no bound on the runs", "            if len(GUTTER_RUN.findall(os.fsencode(form))) > MAX_GUTTER_RUNS:\n", "            if False:\n"),
    # round 3, item 5: a trailing-slash TMPDIR (round 4: killed by the HOME as given)
    ("capture.sh", CAPTURE, "no trailing-slash strip", "  while [[ $tmpdir == */ && $tmpdir != / ]]; do tmpdir=${tmpdir%/}; done", "  :"),
    ("roots.py", CAPTURE, "no normpath spelling", "        forms = [(given, False), (os.path.normpath(given), False)]", "        forms = [(given, False)]"),
    # round 3, NITs and NTHs
    ("roots.py", CAPTURE, "no nF escape", "|\\x1b[\\x20-\\x2f]*[\\x30-\\x7e]\")", "|\\x1b[\\x20-\\x7e]\")"),
    ("roots.py", CAPTURE, "a backslash escape", "        edits.append((end, end, b\"!\"))", "        edits.append((end, end, b\"\\\\\"))"),
    # round 4, item 2: capture-tmpdir right after the run directory only
    ("parity-ledger.toml", SWEEP, "any `_>/` before fxtmp (round 3)", TMPDIR_RULE, rule(prefix="(<scratch(?::real)?_*>/|fxcap\\.XXXXXX/|(?<=_>/))")),
    ("parity-ledger.toml", SWEEP, "any placeholder before fxtmp", TMPDIR_RULE, rule(prefix="(<!*[a-z:]+_*>/|fxcap\\.XXXXXX/)")),
    ("parity-ledger.toml", SWEEP, "the short form read too", TMPDIR_RULE, rule(prefix="(<(?:scratch(?::real)?_*|[sS]_+)>/|fxcap\\.XXXXXX/)")),
    ("parity-ledger.toml", SWEEP, "no zero-fill <scratch>", TMPDIR_RULE, rule(prefix="(<scratch(?::real)?_+>/|fxcap\\.XXXXXX/)")),
    # rounds 5 to 7, item 1: the name itself, with a directory under it
    ("parity-ledger.toml", SWEEP, "a word boundary again (round 4)", TMPDIR_RULE, rule(end="\\b")),
    ("parity-ledger.toml", SWEEP, "no end at all", TMPDIR_RULE, rule(end="")),
    ("parity-ledger.toml", SWEEP, "EDGE's complement again (round 5)", TMPDIR_RULE, rule(end="(?![A-Za-z0-9._~+-])")),
    ("parity-ledger.toml", SWEEP, "a delimiter list again (round 6)", TMPDIR_RULE, rule6("(?=[" + "".join(DELIMS) + "]|$)")),
    ("parity-ledger.toml", SWEEP, "`/` or the row's end", TMPDIR_RULE, rule(end="(?=/|$)")),
    ("parity-ledger.toml", SWEEP, "`/` or white space", TMPDIR_RULE, rule(end="(?=[/\\s])")),
    # round 7, item 3: no `dir` beside the binaries' TMPDIRs
    ("capture.sh", ["test_capture.py", "Refusals.test_a_dir_never_takes_a_binarys_tmpdir_name"], "dir takes fxtmp names (round 6)",
     "*/*|[fF][xX][tT][mM][pP]*|[xX][fF][xX][tT][mM][pP]*)", "*/*)"),
    # round 8, item 1: in any case
    ("capture.sh", ["test_capture.py", "Refusals.test_a_dir_never_takes_a_binarys_tmpdir_name"], "dir refuses lower case only (round 7)",
     "*/*|[fF][xX][tT][mM][pP]*|[xX][fF][xX][tT][mM][pP]*)", "*/*|fxtmp*|xfxtmp*)"),
]
env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
env["PYTHONDONTWRITEBYTECODE"] = "1"
orig = {}
for name in ("roots.py", "capture.sh", "parity-ledger.toml"):
    with open(os.path.join(FX, name)) as f:
        orig[name] = f.read()


def run(tests):
    return subprocess.run(["python3", "-I", os.path.join(FX, tests[0])] + tests[1:], capture_output=True, text=True,
                          env=env, cwd=clone, stdin=subprocess.DEVNULL, timeout=900)


def first_subtest(stderr):
    """The parameters of the first failing subTest, where there is one."""
    for row in stderr.splitlines():
        if row.startswith(("FAIL:", "ERROR:")) and ") (" in row:
            return row.rsplit(") (", 1)[1].rstrip(")")[:60]
    return ""


def first_assertion(stderr, test_file):
    """The test file's line of the first failed assertion: the last frame in
    that file before the first `AssertionError` or other error line."""
    line = None
    for row in stderr.splitlines():
        m = re.search(r'File ".*/%s", line (\d+)' % re.escape(test_file), row)
        if m:
            line = m.group(1)
        elif re.match(r"\w*(Error|Exception)\b", row) and line:
            return "%s:%s" % (test_file, line)
    return "?"


for target, tests, label, old, new in M:
    path = os.path.join(FX, target)
    assert orig[target].count(old) == 1, (label, orig[target].count(old))
    with open(path, "w") as f:
        f.write(orig[target].replace(old, new))
    r = run(tests)
    failed = sorted({line.split("(")[0].split(" ", 1)[1].strip() for line in r.stderr.splitlines() if line.startswith(("FAIL:", "ERROR:"))})
    sub = first_subtest(r.stderr)
    print("%-36s exit %d  first at %s%s  failed: %s" % (label, r.returncode, first_assertion(r.stderr, tests[0]),
                                                      " [%s]" % sub if sub else "", ", ".join(failed) or "none"))
    with open(path, "w") as f:
        f.write(orig[target])
for tests in (CAPTURE, SWEEP):
    r = run(tests)
    print("%-36s exit %d  %s: %s" % ("unmutated", r.returncode, tests[0], (r.stderr.strip().splitlines() or ["?"])[-1]))
