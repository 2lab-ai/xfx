#!/usr/bin/env python3
"""Appends train 7's join-check-1 tests, their text unchanged, to a unit's
clone: each file's section from its `// Train 7, join check 1` banner to the
end of its `mod tests`, taken from the head, put before the last `}` of the
same file in the clone.

    python3 -I -B apply_join.py <repo> <head> <clone>
"""
import subprocess
import sys

FILES = ["src/tui/event_loop.rs", "src/tui/worker.rs", "src/tui/shell.rs"]
repo, head, clone = sys.argv[1:4]
for path in FILES:
    text = subprocess.run(["git", "-C", repo, "show", f"{head}:{path}"], capture_output=True, text=True,
                          check=True).stdout
    banner = text.index("    // -----------------------------------------------------------------------\n"
                        "    // Train 7, join check 1")
    assert text.endswith("}\n")
    section = text[banner:-2]
    target = open(f"{clone}/{path}").read()
    assert target.endswith("}\n") and "Train 7, join check 1" not in target
    open(f"{clone}/{path}", "w").write(target[:-2] + "\n" + section + "}\n")
    print(f"{path}: {section.count(chr(10))} lines appended")
