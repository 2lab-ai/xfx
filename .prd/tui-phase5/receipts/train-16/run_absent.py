#!/usr/bin/env python3
"""Train 16 (new): the test functions a commit holds (tests_kept.py's
reading: a `fn` under `#[test]` or `#[tokio::test]` in a `.rs` file under
`src/` or `tests/`) that a `cargo test` log has no `test ... <result>` line
for, by file and name, and for each whether <base> holds it too. A test
compiled only under a cfg the run did not have (a feature, a target) or
written inside a string is one of them. Answers which tests the `#[test]`
line count holds that the run's passed and ignored counts do not.

    python3 -I -B run_absent.py <repo> <head> <base> <cargo-test-log>
"""
import os
import re
import sys

repo, head, base, log = sys.argv[1:5]
src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "tests_kept.py")).read()
# tests_kept.py's reader and its two patterns, taken whole (the text up to
# its `cache = {}`), with this script's <repo>: one reading for both.
ns = {"__name__": "tests_kept", "re": re, "subprocess": __import__("subprocess"), "repo": repo}
exec(src[src.index("ATTR = "):src.index("\ncache = {}")], ns)
H, B = ns["tests"](head), ns["tests"](base)
text = open(log, encoding="utf-8", errors="replace").read()
ran = set(re.findall(r"^test (?:\S*::)?([A-Za-z0-9_]+) \.\.\. ", text, re.M))
absent = sorted(k for k in H if k[1] not in ran)
print(f"# {head[:8]}: {len(H)} test functions; {len(absent)} with no result line in {os.path.basename(log)}")
for f, n in absent:
    print(f"  {f} {n}: at {base[:8]} too: {'yes' if (f, n) in B else 'no'}")
