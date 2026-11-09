#!/usr/bin/env python3
"""Why a join test is not run on a side: the test helpers and items it names,
and on which sides each is defined (a count of its defining line in that
side's file).

    python3 -I -B helpers.py <repo>
"""
import subprocess
import sys

repo = sys.argv[1]
SIDES = [("main", "6bb9fd90"), ("compact-notice-late", "96cc7849"), ("at-height", "af2b6137"),
         ("paste-after-end", "a832f4d7"), ("head", "e28e1b15")]
ITEMS = [
    ("join_test_worker.rs", "src/tui/worker.rs", "enum CatalogWait"),
    ("join_test_worker.rs", "src/tui/worker.rs", "fn waiting_in("),
    ("join_test_worker.rs", "src/tui/worker.rs", "fn sealed_on_llmux("),
    ("join_test_worker.rs", "src/tui/compaction.rs", "    Dropped,"),
    ("join_test_paste.rs", "src/tui/event_loop.rs", "struct AtList"),
    ("join_test_paste.rs", "src/tui/event_loop.rs", "fn composer(shell: &Shell)"),
    ("join_test_question.rs", "src/tui/file_picker.rs", "pub(crate) fn chosen("),
]
for test, path, needle in ITEMS:
    cells = []
    for name, commit in SIDES:
        text = subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], capture_output=True, text=True).stdout
        cells.append(f"{name} {text.count(needle)}")
    print(f"{test}: `{needle.strip()}` in {path}: " + ", ".join(cells))
