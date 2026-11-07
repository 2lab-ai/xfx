"""The tool group's rows of a pair, found by their text, compared cell by cell.

usage: python3 -I toolrows.py <cells.py dir> <fx.ansi> <xfx.ansi>

A full-transcript (Ctrl-O) screen differs from fx's in rows this unit does
not own (its session notice, a command's arguments, the clock), so the rows
are not compared by position: each row of fx's that opens a tool group's
header (`●` + ` N tool call`) or call (`├ `/`└ ` + a verb) is matched with
xfx's row of the same printed text, and their cells compared (the ramp
reading of notation.py applied, so a truecolor pair compares colours).
Prints one line per row; exit 1 if any row differs or is missing.
"""
import re, sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells  # noqa: E402

def ramp(v):
    if isinstance(v, tuple) and len(v) == 3 and v[0] == v[1] == v[2] and (v[0] - 8) % 10 == 0 and 0 <= (v[0] - 8) // 10 <= 23:
        return 232 + (v[0] - 8) // 10
    return v

def norm(row):
    return [(c, ch, tuple((k, ramp(v)) if k in ("fg", "bg") else (k, v) for k, v in shown)) for c, ch, shown in row]

def text(row):
    return "".join(ch for _, ch, _ in row).rstrip()

TOOL = re.compile(r"^(● \d+ tool call|[├└] (Ran|Running|Cancelled|Exited|Failed|Read|Matched) )")
fx = cells.parse(open(sys.argv[2], encoding="utf-8").read())
xfx = cells.parse(open(sys.argv[3], encoding="utf-8").read())
xfx_by_text = {}
for row in xfx:
    xfx_by_text.setdefault(text(row), row)
bad = 0
for row in fx:
    t = text(row)
    if not TOOL.match(t):
        continue
    other = xfx_by_text.get(t)
    if other is None:
        print(f"MISSING {t!r}"); bad += 1; continue
    a, b = norm(row), norm(other)
    a = [c for c in a if c[1] != " " or c[2]]
    b = [c for c in b if c[1] != " " or c[2]]
    if a == b:
        print(f"same    {t!r}")
    else:
        diff = [(x, y) for x, y in zip(a, b) if x != y][:3]
        print(f"DIFFERS {t!r}: {diff}"); bad += 1
sys.exit(1 if bad else 0)
