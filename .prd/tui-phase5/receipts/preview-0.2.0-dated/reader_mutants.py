#!/usr/bin/env python3
"""Each bound of `cells.join_wrapped` taken out in turn, in memory, and the
reader's three tests in `test_sweep.py` run against it: a bound the tests
need fails one of them.

    cd scripts/fxcap && python3 -I -B <this file>

Prints one line per mutant: its name and the tests that fail, or "all pass".
"""
import os
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, os.getcwd())

import cells  # noqa: E402
import test_sweep  # noqa: E402

SRC = open("cells.py", encoding="utf-8").read()
WIDTH = ("row_width(line) > cols\n                        and ", "")
BREAK = ("\n                        and _wraps_to(part, gap, cols)", "")
MUTANTS = {
    "none (as committed)": [],
    "no width bound": [WIDTH],
    "no break rule": [BREAK],
    "neither": [WIDTH, BREAK],
    "search, not fullmatch": [("pattern.fullmatch(text_of(line))", "pattern.search(text_of(line))")],
}
TESTS = (
    "test_a_banner_the_screen_wrapped_is_read_as_the_banner",
    "test_a_banner_broken_where_the_screen_would_not_wrap_it_stays_broken",
    "test_only_a_whole_banner_is_joined",
)
for name, edits in MUTANTS.items():
    src = SRC
    for old, new in edits:
        assert src.count(old) == 1, (name, old)
        src = src.replace(old, new)
    exec(compile(src, "cells.py", "exec"), cells.__dict__)
    suite = unittest.TestSuite(test_sweep.Normalize(t) for t in TESTS)
    with open(os.devnull, "w") as quiet:
        result = unittest.TextTestRunner(stream=quiet).run(suite)
    failed = sorted(t.id().rsplit(".", 1)[1] for t, _ in result.failures + result.errors)
    print("%-24s %s" % (name, ", ".join(failed) or "all pass"))
