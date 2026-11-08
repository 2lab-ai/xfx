#!/usr/bin/env python3
"""Train 9, join check 2: puts the two join tests (beside this file) into the
checkout given -- join_test_worker.rs at the end of `src/tui/worker.rs`'s
`mod tests`, before the file's last `}`, and join_test_tui.rs at the end of
`tests/tui.rs`. Used once on the train's branch (the join tests' commit) and
on a `git clone --no-local` of each unit's tip (join_units.sh). Train 8's
apply_join.py, for two files.
usage: apply_join.py <checkout>"""
import pathlib
import sys

checkout = pathlib.Path(sys.argv[1])
here = pathlib.Path(__file__).parent

worker = checkout / "src/tui/worker.rs"
text = worker.read_text().rstrip()
assert text.endswith("}"), "worker.rs does not end its tests module"
assert "fn a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions" not in text, "already in"
test = (here / "join_test_worker.rs").read_text()
worker.write_text(text[:-1].rstrip() + "\n\n" + test.rstrip() + "\n}\n")

tui = checkout / "tests/tui.rs"
text = tui.read_text()
assert "fn a_stop_at_a_twenty_question_panel_is_one_cancelled_row" not in text, "already in"
tui.write_text(text.rstrip() + "\n" + (here / "join_test_tui.rs").read_text())
