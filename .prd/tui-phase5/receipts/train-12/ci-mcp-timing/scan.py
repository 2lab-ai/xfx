"""Which snaps of a sweep show an MCP server as `connecting` or `retrying`.

usage: python3 -I scan.py <sweep out>...

For every capture (.txt) of every scenario run, each row holding one of the
two words, as `<scenario> <label> <side>: <row>`, and then per scenario how
many snaps show one and which server names stand before the word on its row.
Those are the snaps whose text depends on where a server is in its try and
backoff cycle.
"""
import collections
import glob
import os
import re
import sys

WORD = re.compile(r"\b(connecting|retrying)\b")
NAME = re.compile(r"^\W*([A-Za-z0-9_.-]+)\b")

per = collections.defaultdict(lambda: {"snaps": set(), "names": collections.Counter()})
for out in sys.argv[1:]:
    for path in sorted(glob.glob(os.path.join(out, "runs", "*", "*", "*.txt"))):
        side = os.path.basename(os.path.dirname(path))
        scen = os.path.basename(os.path.dirname(os.path.dirname(path)))
        m = re.match(r"^x?fx-%s-\d+x\d+-(.+)\.txt$" % re.escape(scen), os.path.basename(path))
        if not m:
            continue
        label = m.group(1)
        with open(path, encoding="utf-8", errors="replace") as f:
            for row in f.read().split("\n"):
                w = WORD.search(row)
                if not w:
                    continue
                n = NAME.match(row)
                per[scen]["snaps"].add(label)
                per[scen]["names"][(n.group(1) if n else "?", w.group(1))] += 1
                print("%s %s %s: %s" % (scen, label, side, re.sub(r"\s+", " ", row.strip())[:110]))
print()
for scen, d in sorted(per.items()):
    print("%-30s %2d snap(s) show a status word; row names %s" % (scen, len(d["snaps"]), dict(d["names"])))
