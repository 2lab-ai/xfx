"""Every cell of one row that differs between an fx and an xfx capture, read
as cells.py reads them (the ramp reading of notation.py for a direct grey).

usage: python3 -I row8.py <cells.py dir> <row> <fx.ansi> <xfx.ansi>...

Prints, per pair, the row's text on each side and each differing cell
(column, fx's character and attributes, xfx's), grouped into runs.
"""
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells  # noqa: E402

def ramp(v):
    if isinstance(v, tuple) and len(v) == 3 and v[0] == v[1] == v[2] and (v[0] - 8) % 10 == 0 and 0 <= (v[0] - 8) // 10 <= 23:
        return 232 + (v[0] - 8) // 10
    return v

def norm(cell):
    col, ch, shown = cell
    return col, ch, tuple((k, ramp(v)) if k in ("fg", "bg") else (k, v) for k, v in shown)

row = int(sys.argv[2])
args = sys.argv[3:]
for fxp, xp in zip(args[::2], args[1::2]):
    fx = cells.parse(open(fxp, encoding="utf-8").read())
    xf = cells.parse(open(xp, encoding="utf-8").read())
    a = {c[0]: norm(c) for c in fx[row - 1]}
    b = {c[0]: norm(c) for c in xf[row - 1]}
    name = xp.rsplit("/", 1)[-1]
    print("## %s row %d" % (name, row))
    print("   fx : %r" % "".join(c[1] for c in sorted(a.values())))
    print("   xfx: %r" % "".join(c[1] for c in sorted(b.values())))
    diff = [col for col in sorted(set(a) | set(b)) if a.get(col) != b.get(col)]
    runs, start, prev = [], None, None
    for col in diff:
        if start is None or col != prev + 1:
            if start is not None:
                runs.append((start, prev))
            start = col
        prev = col
    if start is not None:
        runs.append((start, prev))
    for s, e in runs:
        fxs = "".join(a[c][1] if c in a else "·" for c in range(s, e + 1))
        xs = "".join(b[c][1] if c in b else "·" for c in range(s, e + 1))
        attrs = sorted({(a.get(c, (0, "", ()))[2], b.get(c, (0, "", ()))[2]) for c in range(s, e + 1)})
        print("   cols %d-%d: fx %r xfx %r attrs %s" % (s, e, fxs, xs, attrs))
    if not diff:
        print("   (no cell differs)")
