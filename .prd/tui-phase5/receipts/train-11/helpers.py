#!/usr/bin/env python3
"""Train 11 (train 10's helpers.py, its items and sides this train's): why a
join test is not run as it stands on a side -- the test helpers and items it
names, and on which sides each is defined (a count of its defining line in
that side's file; the head's count, 1, is the sample that the needle
matches).

    python3 -I -B helpers.py <repo>
"""
import subprocess
import sys

repo = sys.argv[1]
SIDES = [("main", "32afd307"), ("merge-1", "4a0b900b"), ("submit-card-preview", "0c203cd3"),
         ("catalog-refresh", "3ffbe0c7"), ("paste-gap", "5cc2e42c"), ("head", "820a8f15")]
ITEMS = [
    ("both", "src/tui/event_loop.rs", "    fn loop_tick("),
    ("both", "src/tui/event_loop.rs", "    fn card_once("),
    ("both", "src/tui/event_loop.rs", "    struct Terminal {"),
    ("both", "src/tui/shell.rs", "    pub(crate) fn previewing("),
    ("join_test_paste.rs", "src/tui/event_loop.rs", "    struct Readable {"),
    ("join_test_paste.rs", "src/tui/input.rs", "    pub(crate) const PASTE_QUIET"),
    ("join_test_paste.rs", "src/tui/shell.rs", "    pub(crate) fn end_delivery_at("),
    ("join_test_paste.rs", "src/tui/shell.rs", "    pub(crate) fn settle_input("),
    ("join_test_catalog.rs", "src/tui/event_loop.rs", "    fn full_with_draft("),
    ("join_test_catalog.rs", "src/tui/event_loop.rs", "    fn catalog_due("),
    ("join_test_catalog.rs", "src/tui/event_loop.rs", "    fn catalog_due_ago("),
    ("join_test_catalog.rs", "src/provider/catalog_clock.rs", "pub(crate) struct CatalogDue"),
]
for test, path, needle in ITEMS:
    cells = []
    for name, commit in SIDES:
        r = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True)
        cells.append(f"{name} {r.stdout.count(needle) if r.returncode == 0 else 'no file'}")
    print(f"{test}: `{needle.strip()}` in {path}: " + ", ".join(cells))
