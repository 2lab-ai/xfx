"""Copies T-U6's captures into the receipt, redacted, after checking that each
xfx snap whose row 1 is the banner names the build it must.

usage: python3 -I -B publish.py <repo> <cap-dir> <receipt-dir> <green-sha7> <red-sha7>

<cap-dir> holds `fx/`, `xfx/` (the build under the receipt) and `red/` (the
base's build), each as `capture.sh` wrote them: flat, every scenario's files
side by side. Snaps (`.ansi`, `.txt`) and terminal bytes (`.wire`) are
copied, nothing else (no request bodies, no gateway logs, no workspace
listing, and no replayed frame: `frames.py` makes those from the wires). A
wire is bytes, not text: it is copied as it is, and refused if a local root
appears in it.
"""
import glob
import os
import re
import shutil
import sys

repo, cap, out = (os.path.realpath(p) for p in sys.argv[1:4])
green_sha, red_sha = sys.argv[4:6]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import LEFTOVER, Redactor  # noqa: E402

red = Redactor({repo: "<repo>", cap: "<scratch>"})
SCENARIOS = ["p5-transcript-submit", "p5-transcript-submit-multi", "p5-transcript-tallfirst",
             "p5-transcript-diffctl", "p5-transcript-diffodd", "p5-transcript-diffbad", "p5-transcript-diffask"]
SIDES = {"fx": ("fx", "fx", None), "xfx": ("xfx", "xfx", green_sha), "red": ("red", "xfx", red_sha)}
BANNER = re.compile(r"^x𝒇x v0\.1\.0-([0-9a-f]{7}) \[debug\]")
LABEL = re.compile(r"^[a-z0-9.]+(?:-[a-z0-9.]+)*\.(?:ansi|txt|wire)$")
for name in SIDES:
    shutil.rmtree(os.path.join(out, name), ignore_errors=True)
n = 0
for side, (name, prefix, sha) in SIDES.items():
    for scenario in SCENARIOS:
        unit = scenario + "-40x120"
        lead = "%s-%s-" % (prefix, unit)
        files = []
        for path in sorted(glob.glob(os.path.join(cap, side, lead + "*"))):
            label = os.path.basename(path)[len(lead):]
            # This scenario's own snaps: a label is the scenario's, so it
            # holds no `-40x120-` (`p5-transcript-submit-40x120-` is no
            # prefix of `-submit-multi-`'s files, and nor is any other).
            if "40x120" in label or not LABEL.match(label):
                continue
            if label.startswith(("frame", "work-")):
                continue
            files.append(path)
        assert files, (side, unit)
        banners = 0
        dest = os.path.join(out, name, unit)
        os.makedirs(dest, exist_ok=True)
        for path in files:
            target = os.path.join(dest, os.path.basename(path))
            if path.endswith(".wire"):
                data = open(path, "rb").read()
                assert not LEFTOVER.search(data.decode("utf-8", "replace")), path
                open(target, "wb").write(data)
                n += 1
                continue
            text = open(path, encoding="utf-8").read()
            if sha and path.endswith(".txt"):
                m = BANNER.match(text.split("\n", 1)[0])
                if m:
                    assert m.group(1) == sha, (path, m.group(1), sha)
                    banners += 1
            red.write(target, text)
            n += 1
        if sha:
            assert banners, ("no banner seen", side, unit)
            print("%-4s %-34s %2d snaps' row 1 is the banner of %s" % (name, unit, banners, sha))
print("published", n, "files")
