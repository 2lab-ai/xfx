"""What the parity sweep would say about this receipt's pairs, through the
sweep's own code (`scripts/fxcap/sweep.py`'s classification, the ledger as it
is in this checkout, pinned to v0.0.16): each pair's verdict, the causes its
rows are attributed to, and the rows no cause covers.

usage: python3 -I -B sweep-view.py <repo> <captures> [<scenario>-<size>/<snap> ...]

With no pair named, every Ctrl-O (`full*`) pair of this receipt's scenarios
under <captures>/fx and <captures>/xfx. Exit 0 when no row is left without a cause.
"""

import glob
import os
import sys

repo = os.path.realpath(sys.argv[1])
captures = os.path.realpath(sys.argv[2])
named = sys.argv[3:]
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402
import sweep  # noqa: E402

led = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
divs = ledgermod.active(led, "v0.0.16")
rules = {side: ledgermod.rules_for(led, side) for side in ("fx", "xfx")}


def pairs():
    if named:
        for item in named:
            unit, snap = item.split("/")
            yield unit, snap
        return
    for path in sorted(glob.glob(os.path.join(captures, "xfx", "*", "xfx-*-full*.ansi"))):
        unit = os.path.basename(os.path.dirname(path))
        if not unit.startswith(UNITS):
            continue
        snap = os.path.basename(path)[len("xfx-%s-" % unit) : -len(".ansi")]
        yield unit, snap


# This receipt's scenarios.
UNITS = tuple("p5-transcript-%s-" % name for name in ("tools", "anchor", "anchormid", "resize", "records", "question"))
loose_total = 0
for unit, snap in pairs():
    scenario, size = unit.rsplit("-", 1)
    fxp = os.path.join(captures, "fx", unit, "fx-%s-%s.ansi" % (unit, snap))
    xp = os.path.join(captures, "xfx", unit, "xfx-%s-%s.ansi" % (unit, snap))
    a = cells.parse(open(fxp, encoding="utf-8").read())
    b = cells.parse(open(xp, encoding="utf-8").read())
    na, nb = cells.normalize(a, rules["fx"]), cells.normalize(b, rules["xfx"])
    readings = [cells.normalize(a, rules["fx"], absorb=way) for way in ("after", "before")]
    diff = cells.differing(na, nb, readings)
    verdict, _ = ledgermod.classify(a == b, diff, scenario, snap, divs)
    caused, loose = [], []
    if verdict == "unresolved-defect":
        promoted, left = ledgermod.cascade(sweep.aligned_fx(na, readings, nb), nb, scenario, snap, divs, cells.text_of)
        if promoted:
            verdict = promoted
        else:
            caused, loose = sweep.attribute(left, scenario, snap, led["causes"])
    loose_total += len(loose)
    print("%-40s %-20s %-22s causes=%s" % (unit, snap, verdict, ",".join(caused) or "-"))
    for side, row, text in loose:
        print("    NO CAUSE %-3s row %2d  %s" % (side, row, text))
print("rows no cause covers: %d" % loose_total)
sys.exit(1 if loose_total else 0)
