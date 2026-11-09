#!/usr/bin/env python3
"""Train 9, join check 5 (PASTE-AFTER-END x QUESTIONS-20): appends
join_test_paste.rs (beside this file) to the end of `tests/tui.rs` in the
checkout given. Used once on the train's branch and on a `git clone
--no-local` of each unit's tip (join_units_paste.sh). apply_join.py's
pattern.
usage: apply_join_paste.py <checkout>"""
import pathlib
import sys

checkout = pathlib.Path(sys.argv[1])
tui = checkout / "tests/tui.rs"
text = tui.read_text()
assert "fn a_paste_with_enter_behind_its_marker_on_page_ten_of_twenty" not in text, "already in"
tui.write_text(text.rstrip() + "\n" + (pathlib.Path(__file__).parent / "join_test_paste.rs").read_text())
