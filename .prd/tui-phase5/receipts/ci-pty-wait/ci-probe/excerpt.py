"""Keeps a masked CI log's tests/tui.rs part: from its "Running tests/tui.rs"
line to the end. A header names the whole log by its name, line count and
sha256, so the excerpt can be checked against it.

    python3 -I excerpt.py <masked-log> <out>
"""
import hashlib
import os
import sys

source, out = sys.argv[1], sys.argv[2]
data = open(source, "rb").read()
lines = data.decode("utf-8", "replace").splitlines()
start = max(i for i, line in enumerate(lines) if "Running tests/tui.rs" in line)
with open(out, "w") as sink:
    sink.write(
        f"# excerpt of {os.path.basename(source)}: lines {start + 1}-{len(lines)} of {len(lines)}, "
        f"the tests/tui.rs part; the whole log's sha256 {hashlib.sha256(data).hexdigest()}\n"
    )
    sink.write("\n".join(lines[start:]) + "\n")
print(f"{os.path.basename(out)}: lines {start + 1}-{len(lines)} of {len(lines)}")
