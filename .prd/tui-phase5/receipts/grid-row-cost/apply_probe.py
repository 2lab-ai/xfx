#!/usr/bin/env python3
"""Appends the probe (frame_cost_probe.rs, beside this file) to the end of
`src/tui/event_loop.rs`'s `mod tests` in the clone given, as train 6 appended
its probe to `shell.rs`'s; never run on a worktree. usage: apply_probe.py <clone>"""
import pathlib
import sys

clone = pathlib.Path(sys.argv[1])
probe = (pathlib.Path(__file__).parent / "frame_cost_probe.rs").read_text()
target = clone / "src/tui/event_loop.rs"
text = target.read_text().rstrip()
assert text.endswith("}"), "event_loop.rs does not end its tests module"
assert "fn probe_frame_cost_of_long_rows" not in text, "probe already in"
target.write_text(text[:-1].rstrip() + "\n\n" + probe.rstrip() + "\n}\n")
