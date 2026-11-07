"""Every differing cell of a pair, grouped by (fx attrs, xfx attrs) with a count,
row 1 left out. usage: python3 -I cellpairs.py <cells.py dir> <fx.ansi> <xfx.ansi>"""
import collections, sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells
a = cells.parse(open(sys.argv[2], encoding="utf-8").read())
b = cells.parse(open(sys.argv[3], encoding="utf-8").read())
c = collections.Counter()
for r in range(1, max(len(a), len(b))):
    ra = a[r] if r < len(a) else []
    rb = b[r] if r < len(b) else []
    for i in range(max(len(ra), len(rb))):
        x = ra[i] if i < len(ra) else None
        y = rb[i] if i < len(rb) else None
        if x != y:
            c[(x[2] if x else None, y[2] if y else None, (x[1] if x else None) == (y[1] if y else None))] += 1
for k, v in sorted(c.items(), key=str):
    print(v, "fx", k[0], "xfx", k[1], "same-text" if k[2] else "TEXT-DIFFERS")
