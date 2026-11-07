"""Copies one of WIDTH-DISPUTED's scenarios' captures into the receipt,
redacted, after checking each xfx snap whose row 1 is the banner names the
build it must.

usage: python3 -I -B publish.py <repo> <receipt-dir> <scenario> <fx-dir> <xfx-dir> <red-dir> <xfx-sha7> <red-sha7>

Each dir is flat, as `capture.sh` writes it; only the scenario's `.ansi` and
`.txt` snaps are copied, into `<side>/<scenario>-40x120/`.
"""
import glob
import os
import re
import shutil
import sys

repo, out = (os.path.realpath(p) for p in sys.argv[1:3])
scenario = sys.argv[3]
fx, xfx, red = (os.path.realpath(p) for p in sys.argv[4:7])
xfx_sha, red_sha = sys.argv[7:9]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import Redactor  # noqa: E402

UNIT = scenario + "-40x120"
BANNER = re.compile(r"^x𝒇x v0\.1\.0-([0-9a-f]{7}) \[debug\]")
red_redactor = Redactor({repo: "<repo>"})
n = 0
for name, src, prefix, sha in (("fx", fx, "fx", None), ("xfx", xfx, "xfx", xfx_sha), ("red", red, "xfx", red_sha)):
    dest = os.path.join(out, name, UNIT)
    shutil.rmtree(dest, ignore_errors=True)
    os.makedirs(dest)
    seen = 0
    for path in sorted(glob.glob(os.path.join(src, "%s-%s-*" % (prefix, UNIT)))):
        if not path.endswith((".ansi", ".txt")):
            continue
        text = open(path, encoding="utf-8").read()
        m = BANNER.match(text.split("\n", 1)[0])
        if sha and m:
            assert m.group(1) == sha, (path, m.group(1), sha)
            seen += 1
        red_redactor.write(os.path.join(dest, os.path.basename(path)), text)
        n += 1
    assert not sha or seen, ("no banner", name)
    if sha:
        print("%-3s %s %d snaps' row 1 is the banner of %s" % (name, UNIT, seen, sha))
print("published", n, "files")
