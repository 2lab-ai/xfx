"""The join of main: each code unit a conflict block sat in, read whole in
the merge against both parents' versions (and the base's).

usage: python3 -I -B functions.py <repo> <base> <ours> <theirs> <merged> <path>:<header>...

<header> is the unit's first line, stripped (a Rust `fn` or a Python `def`).
The unit runs from that line to the first later line, at its indentation or
less, that closes it: a Rust `}` at the header's indentation, or the next
non-blank line a Python `def` holds at its indentation or less. For each
side, prints every line by which the merged unit differs from that side's
(difflib), and whether the two are the same once every `:N` / `:N-M` is
masked. A unit the merge took from one side whole reads no line against it.
"""
import difflib
import re
import subprocess
import sys

repo, base, ours, theirs, merged = sys.argv[1:6]
NUMS = re.compile(r":\d+(?:-\d+)?")


def show(commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True, capture_output=True, text=True).stdout.split("\n")


def unit(lines, header):
    starts = [i for i, x in enumerate(lines) if x.strip() == header]
    if len(starts) != 1:
        sys.exit(f"{header!r} on {len(starts)} lines")
    i = starts[0]
    indent = len(lines[i]) - len(lines[i].lstrip())
    rust = header.endswith("{")
    for j in range(i + 1, len(lines)):
        x = lines[j]
        if not x.strip():
            continue
        k = len(x) - len(x.lstrip())
        if rust and k == indent and x.strip() == "}":
            return i + 1, lines[i:j + 1]
        if not rust and k <= indent:
            return i + 1, lines[i:j]
    return i + 1, lines[i:]


bad = 0
for spec in sys.argv[6:]:
    path, header = spec.split(":", 1)
    at, m = unit(show(merged, path), header)
    print(f"## {path}: `{header}` -- merged :{at}-{at + len(m) - 1}, {len(m)} lines")
    for name, commit in (("ours", ours), ("theirs", theirs), ("base", base)):
        sat, s = unit(show(commit, path), header)
        diff = [d for d in difflib.unified_diff(s, m, lineterm="", n=0) if d[:1] in "+-" and d[:3] not in ("+++", "---")]
        same_masked = [NUMS.sub(":N", x) for x in s] == [NUMS.sub(":N", x) for x in m]
        print(f"   against {name} ({commit[:8]}, :{sat}-{sat + len(s) - 1}): {len(diff)} line(s) differ; the same once numbers are masked: {same_masked}")
        for d in diff:
            print("      " + d.strip()[:160])
        bad += not same_masked
print(f"# {bad} (unit, side) readings that differ beyond their numbers")
sys.exit(1 if bad else 0)
