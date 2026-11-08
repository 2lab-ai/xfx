#!/usr/bin/env python3
"""GRID-ROW-COST: ARGROW-CACHE's apply_probe.py, reading its
probe from that receipt (`../argrow-cache/render_cost_probe.rs`) and unchanged
otherwise. Appends the probe to the end of `src/tui/shell.rs`'s `mod tests` in
the clone given, as train 6 did; never run on a worktree.
usage: apply_ctrlo_probe.py <clone>"""
import pathlib
import sys

clone = pathlib.Path(sys.argv[1])
probe = (pathlib.Path(__file__).parent / ".." / "argrow-cache" / "render_cost_probe.rs").read_text()
shell = clone / "src/tui/shell.rs"
text = shell.read_text().rstrip()
assert text.endswith("}"), "shell.rs does not end its tests module"
assert "fn probe_render_cost_of_a_long_argument" not in text, "probe already in"
shell.write_text(text[:-1].rstrip() + "\n\n" + probe.rstrip() + "\n}\n")
