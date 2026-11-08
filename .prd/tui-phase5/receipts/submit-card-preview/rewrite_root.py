#!/usr/bin/env python3
"""Takes the capture harness's scratch root out of this receipt's captures,
the same way on every side, after capture.

    TMPDIR=<the harness temp folder the captures ran under> \
        python3 -I -B rewrite_root.py <receipt-dir>

The image scenarios' card badges link to the run's file, so their captures
hold `<root>/fxcap.<run>/work/shot.png` as an OSC 8 target, a wire byte
that no cell shows. In every file under `fx/`, `red/` and `xfx/`, each
`<root>/fxcap.<run>/` becomes `<work>/`, where `<root>` is $TMPDIR without
its trailing slash and `<run>` is the run's letters and digits. Nothing is
written into this file: the root is read from the environment when it runs.
The rewrite works on bytes, so every other byte of a file stays as it was.
It prints each file it changed with its count, and fails if the root is
still anywhere after it.
"""
import os
import re
import sys

receipt = sys.argv[1]
root = os.environ.get("TMPDIR", "").rstrip("/")
if not root.startswith("/"):
    sys.exit("set TMPDIR to the harness temp folder the captures ran under")
RUN = re.compile(re.escape(root.encode()) + rb"/fxcap\.[A-Za-z0-9]+/")
total = 0
for side in ("fx", "red", "xfx"):
    for name in sorted(os.listdir(os.path.join(receipt, side))):
        path = os.path.join(receipt, side, name)
        data = open(path, "rb").read()
        new, n = RUN.subn(b"<work>/", data)
        if n:
            open(path, "wb").write(new)
            total += n
            print("%3d %s/%s" % (n, side, name))
left = [os.path.join(side, name) for side in ("fx", "red", "xfx")
        for name in sorted(os.listdir(os.path.join(receipt, side)))
        if root.encode() in open(os.path.join(receipt, side, name), "rb").read()]
print("%d links rewritten; %d files still hold the root%s" % (total, len(left), (": " + ", ".join(left)) if left else ""))
sys.exit(1 if left else 0)
