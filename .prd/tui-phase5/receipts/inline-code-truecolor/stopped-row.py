"""Row 8 of the truecolor stopped-call pairs -- the stopped call's own line --
cell by cell: each cell whose foreground differs, with xfx's direct grey read
as the xterm-ramp index it spells (notation.py's reading). The product name
the row carries (`fx`/`xfx`) shifts the cells after it; those have no
foreground on either side and are left out.

usage: python3 -I stopped-row.py <cells.py dir> <sweep --out>
"""

import sys

sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells  # noqa: E402

RUNS = sys.argv[2] + "/runs"
UNITS = {
    "p5-toolstop-retint-tc": ("dark", "light", "dark-again"),
    "p5-toolstop-retint-light-tc": ("light", "dark", "light-again"),
}


def fg(shown):
    return dict(shown).get("fg")


def ramp(v):
    if isinstance(v, tuple) and len(v) == 3 and v[0] == v[1] == v[2] and (v[0] - 8) % 10 == 0:
        k = (v[0] - 8) // 10
        if 0 <= k <= 23:
            return 232 + k
    return v


for unit, labels in UNITS.items():
    for label in labels:
        side = {}
        for who in ("fx", "xfx"):
            path = "%s/%s/%s/%s-%s-40x120-%s.ansi" % (RUNS, unit, who, who, unit, label)
            side[who] = {c: (ch, s) for c, ch, s in cells.parse(open(path, encoding="utf-8").read())[7]}
        runs = []
        for col in sorted(set(side["fx"]) | set(side["xfx"])):
            a, b = side["fx"].get(col), side["xfx"].get(col)
            fa, fb = fg(a[1]) if a else None, fg(b[1]) if b else None
            if fa == fb:
                continue
            if runs and runs[-1]["key"] == (fa, fb) and runs[-1]["to"] == col - 1:
                runs[-1]["to"] = col
                runs[-1]["fx"] += a[0]
                runs[-1]["xfx"] += b[0]
            else:
                runs.append({"key": (fa, fb), "from": col, "to": col, "fx": a[0], "xfx": b[0]})
        out = [
            "cols %d-%d %r/%r fx %s xfx %s reads %s" % (r["from"], r["to"], r["fx"], r["xfx"], r["key"][0], r["key"][1], ramp(r["key"][1]))
            for r in runs
        ]
        print("%s %s: %s" % (unit, label, "; ".join(out)))
