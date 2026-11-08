#!/usr/bin/env python3
"""Train 8, join check 1: appends join_test.rs (beside this file) to the end of
`src/tui/event_loop.rs`'s `mod tests` in the checkout given, before the file's
last `}`. Used once on the train's branch (the join test's commit) and on a
`git clone --no-local` of each unit's tip (join_units.sh).
usage: apply_join.py <checkout>"""
import pathlib
import sys

checkout = pathlib.Path(sys.argv[1])
test = (pathlib.Path(__file__).parent / "join_test.rs").read_text()
target = checkout / "src/tui/event_loop.rs"
text = target.read_text().rstrip()
assert text.endswith("}"), "event_loop.rs does not end its tests module"
assert "fn a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays" not in text, "already in"
target.write_text(text[:-1].rstrip() + "\n\n" + test.rstrip() + "\n}\n")
