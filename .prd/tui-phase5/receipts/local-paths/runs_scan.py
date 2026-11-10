"""Counts the capture files under a sweep's runs/ that hold any part of a
run's paths. Prints counts by file kind, never a path's text.

usage: SCAN_TMP=<the TMPDIR the sweep ran under> python3 -I runs_scan.py <runs/>

A file is counted when it holds, anywhere in its bytes:
  root      the TMPDIR whole, or the user's home
  shape     a harness scratch root (`/tmp/claude-<digits>`) or macOS's
            per-user temporary root
  fragment  what a root broken or cut short leaves: `claude-<digits>`, a
            sweep job directory's name (`fxsw.<8>`), or a run directory's
            name (`fxcap.` and six letters or digits)
The sweep's own capture.log and status.json are not captures and are left
out.
"""
import collections
import os
import re
import sys

runs = sys.argv[1]
tmp = os.environ["SCAN_TMP"].encode()
home = os.environ["HOME"].encode()
# Bracketed, so this tracked file does not match its own patterns.
shape = re.compile(rb"/tmp/claud[e]-[0-9]+|/var/folder[s]/")
fragment = re.compile(rb"claud[e]-[0-9]+|fxs[w]\.[A-Za-z0-9_]{8}|fxca[p]\.[A-Za-z0-9]{6}")
total = collections.Counter()
held = {k: collections.Counter() for k in ("root", "shape", "fragment", "any")}
for top, _, names in os.walk(runs):
    for n in names:
        if n in ("capture.log", "status.json"):
            continue
        kind = "body" if "-bodies" in top else (n.rsplit(".", 1)[-1] if "." in n else "(none)")
        data = open(os.path.join(top, n), "rb").read()
        total[kind] += 1
        hits = {"root": tmp in data or home in data, "shape": bool(shape.search(data)), "fragment": bool(fragment.search(data))}
        hits["any"] = any(hits.values())
        for k, v in hits.items():
            held[k][kind] += v
print("%-8s %5s %5s %5s %8s %5s" % ("kind", "files", "root", "shape", "fragment", "any"))
for k in sorted(total) + ["all"]:
    get = (lambda c: sum(c.values())) if k == "all" else (lambda c: c[k])
    print("%-8s %5d %5d %5d %8d %5d" % (k, get(total), get(held["root"]), get(held["shape"]), get(held["fragment"]), get(held["any"])))
