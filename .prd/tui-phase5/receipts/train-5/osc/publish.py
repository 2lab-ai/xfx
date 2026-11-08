"""Copies train 5's osc wires into the receipt: for each set (`default`,
`tc`, `mcp`), fx's and xfx's, after checking that each xfx wire names the
build it must and that no wire holds a local path (scripts/fxcap/redact.py's
LEFTOVER). Only `.wire` files: no gateway log, no request body, no run log.
A wire is bytes: copied as it is, or, when a local path is in it, not copied
at all and named (the set's osc.py result is in the receipt either way).

usage: python3 -I -B osc/publish.py <repo> <cap-dir> <osc-dir> <head-sha7>
"""
import glob
import os
import re
import shutil
import sys

repo, cap, out = (os.path.realpath(p) for p in sys.argv[1:4])
sha = sys.argv[4]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
from redact import LEFTOVER  # noqa: E402

BANNER = re.compile(rb"v0\.1\.0-([0-9a-f]{7})")
n = refused = 0
for kit in ("default", "tc", "mcp"):
    for side in ("fx", "xfx"):
        shutil.rmtree(os.path.join(out, kit, side), ignore_errors=True)
        for unit_dir in sorted(glob.glob(os.path.join(cap, kit, side, "p5-lifecycle-osc-*-40x120"))):
            unit = os.path.basename(unit_dir)
            wires = sorted(glob.glob(os.path.join(unit_dir, "%s-%s-*.wire" % (side, unit))))
            assert wires, (kit, side, unit)
            dest = os.path.join(out, kit, side, unit)
            kept = 0
            for path in wires:
                data = open(path, "rb").read()
                if side == "xfx":
                    found = set(BANNER.findall(data))
                    assert found == {sha.encode()}, (path, found, sha)
                if LEFTOVER.search(data.decode("utf-8", "replace")):
                    print("not copied (a local path in it): %s/%s/%s/%s" % (kit, side, unit, os.path.basename(path)))
                    refused += 1
                    continue
                os.makedirs(dest, exist_ok=True)
                shutil.copyfile(path, os.path.join(dest, os.path.basename(path)))
                kept += 1
                n += 1
            print("%-7s %-4s %-38s %2d of %2d wires%s" % (kit, side, unit, kept, len(wires), " banner %s" % sha if side == "xfx" else ""))
print("published %d wires, %d not copied" % (n, refused))
