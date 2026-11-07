"""Copies L-U1+U2's wires into the receipt: fx's, xfx's (the build under the
receipt) and red's (the base's), after checking each xfx wire names the build
it must and no wire holds a local path.

usage: python3 -I -B publish.py <repo> <cap-dir> <receipt-dir> <green-sha7> <red-sha7> <prev-sha7> <prev2-sha7> <prev3-sha7>

<cap-dir> holds `fx/`, `xfx/` and `red/` -- and `prev/`, the first build's
`picker` capture, and `prev2/`, the round-1 build's `autocompactfail`
capture, the scenario each of those builds failed, and `prev3/`, round 2's
build on every scenario, whose launch and resume reports sit where fx's do
not -- each with one
`<scenario>-40x120/` per scenario as `capture.sh` wrote it. Only `.wire`
files are copied: no gateway log, no request body, no run log. A wire is
bytes, not text: copied as it is, and refused if a local root appears in it.
"""
import glob
import os
import re
import shutil
import sys

repo, cap, out = (os.path.realpath(p) for p in sys.argv[1:4])
green_sha, red_sha, prev_sha, prev2_sha, prev3_sha = sys.argv[4:9]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import LEFTOVER  # noqa: E402

SIDES = {"fx": ("fx", None), "xfx": ("xfx", green_sha), "red": ("xfx", red_sha), "prev": ("xfx", prev_sha), "prev2": ("xfx", prev2_sha), "prev3": ("xfx", prev3_sha)}
BANNER = re.compile(rb"v0\.1\.0-([0-9a-f]{7})")
n = 0
for side, (prefix, sha) in SIDES.items():
    shutil.rmtree(os.path.join(out, side), ignore_errors=True)
    for unit_dir in sorted(glob.glob(os.path.join(cap, side, "p5-lifecycle-osc-*-40x120"))):
        unit = os.path.basename(unit_dir)
        wires = sorted(glob.glob(os.path.join(unit_dir, "%s-%s-*.wire" % (prefix, unit))))
        assert wires, (side, unit)
        dest = os.path.join(out, side, unit)
        os.makedirs(dest, exist_ok=True)
        for path in wires:
            data = open(path, "rb").read()
            assert not LEFTOVER.search(data.decode("utf-8", "replace")), path
            if sha:
                found = set(BANNER.findall(data))
                assert found == {sha.encode()}, (path, found, sha)
            shutil.copyfile(path, os.path.join(dest, os.path.basename(path)))
            n += 1
        print("%-4s %-36s %2d wires%s" % (side, unit, len(wires), " banner %s" % sha if sha else ""))
print("published", n, "wires")
