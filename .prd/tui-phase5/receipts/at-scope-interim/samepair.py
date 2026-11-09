"""Two captures of one scenario from two sweeps, cell by cell (cells.py's
parser), row 1 (the banner) left out, with the names each run made for
itself -- the sweep's directory (`fxsw.` and 8 characters) and the run's
(`fxcap.` and 6) -- written as X. Their styles are kept: a cell inside such a
name that one capture paints bold and the other does not still differs.

usage: python3 -I samepair.py <cells.py> <a.ansi> <b.ansi>
"""
import importlib.util
import re
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("cells", sys.argv[1])
cells = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cells)
NAMES = re.compile(r"fxsw\.([a-z0-9]{8})|fxcap\.([A-Za-z0-9]{6})")


def masked(path):
    out = []
    for row in cells.rows(path, False):
        text = "".join(ch for _, ch, _ in row)
        hide = set()
        for m in NAMES.finditer(text):
            g = 1 if m.group(1) else 2
            hide.update(range(m.start(g), m.end(g)))
        out.append([(c, "X" if i in hide else ch, s) for i, (c, ch, s) in enumerate(row)])
    return out


a, b = masked(sys.argv[2]), masked(sys.argv[3])
differ = 0
for i in range(1, max(len(a), len(b))):
    p = a[i] if i < len(a) else []
    q = b[i] if i < len(b) else []
    if p != q:
        differ += 1
        first = next((j for j in range(max(len(p), len(q))) if (p[j] if j < len(p) else None) != (q[j] if j < len(q) else None)), None)
        print("row %d differs; first differing cell index %s: %r / %r" % (i + 1, first, p[first] if first is not None and first < len(p) else None, q[first] if first is not None and first < len(q) else None))
print("%d rows differ (row 1 not compared; the run's names masked, styles kept)" % differ)
