"""Takes the capture's scratch root out of this receipt's captures.

usage: TMPDIR=<the TMPDIR the captures ran under> python3 -I scrub.py <receipt-dir> [--check]

capture.sh runs each binary in `$TMPDIR/fxcap.<run>/` (`work/` the workspace,
`home/` its HOME), and fx's system prompt names that folder, so fx's request
bodies carry it. Every file under fx/, xfx/, red/ and probe/ gets one
byte-exact substitution: `$TMPDIR/fxcap.<run>` -> `<work>`, so
`<root>/fxcap.Yp6FlA/work` reads `<work>/work`. Nothing else in a file
changes. The root is read from `$TMPDIR` when this runs, so the script names
no local path itself.

--check rewrites nothing and exits 1 if any file still holds the root.
"""
import os
import re
import sys

root = os.environ.get("TMPDIR", "").rstrip("/")
if not root:
    sys.exit("scrub.py: TMPDIR is not set")
receipt = sys.argv[1]
check = "--check" in sys.argv[2:]
run = re.compile(re.escape(root.encode()) + rb"/fxcap\.[A-Za-z0-9]+")
held = re.compile(re.escape(root.encode()))
changed, left = [], []
for side in ("fx", "xfx", "red", "probe"):
    for folder, _, names in os.walk(os.path.join(receipt, side)):
        for name in sorted(names):
            path = os.path.join(folder, name)
            data = open(path, "rb").read()
            new, count = run.subn(b"<work>", data)
            if count and not check:
                open(path, "wb").write(new)
                changed.append((os.path.relpath(path, receipt), count))
            if held.search(new if not check else data):
                left.append(os.path.relpath(path, receipt))
for path, count in changed:
    print(f"rewrote {path}: {count}")
print(f"{len(changed)} file(s) rewritten; {len(left)} still hold the root")
for path in left:
    print(f"  still: {path}")
sys.exit(1 if left else 0)
