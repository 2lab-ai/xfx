#!/usr/bin/env python3
"""Appends the memory probe (memory_probe.rs, beside this file) to the end of
`src/tui/shell.rs`'s `mod tests` in the clone given, as train 6 appended its
probe; never run on a worktree. usage: apply_memory_probe.py <clone>"""
import pathlib
import sys

clone = pathlib.Path(sys.argv[1])
probe = (pathlib.Path(__file__).parent / "memory_probe.rs").read_text()
shell = clone / "src/tui/shell.rs"
text = shell.read_text().rstrip()
assert text.endswith("}"), "shell.rs does not end its tests module"
assert "fn probe_memory_of_a_long_argument" not in text, "probe already in"
shell.write_text(text[:-1].rstrip() + "\n\n" + probe.rstrip() + "\n}\n")
