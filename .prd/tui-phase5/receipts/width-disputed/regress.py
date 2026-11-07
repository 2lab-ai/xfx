"""Every snap of the regression scenarios, the base's build against the fix's,
cell by cell with styles, the ledger's normalizations for xfx applied to both.

usage: python3 -I -B regress.py <repo> <base-dir> <new-dir>

Both dirs are as `capture.sh` writes them, flat. Prints one line per snap
(`identical`, or the rows that differ and the first difference) and a tally.
Wires are not compared: they are the binary's writes in time, not a screen.
"""
import glob
import os
import sys

repo, base, new = (os.path.realpath(p) for p in sys.argv[1:4])
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

RULES = ledgermod.rules_for(ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo), "xfx")


def rows(p):
    return [cells.normalize_row(r, RULES) for r in cells.parse(open(p, encoding="utf-8").read())]


names = sorted(os.path.basename(p) for p in glob.glob(os.path.join(base, "xfx-*.ansi")))
only_new = sorted(set(os.path.basename(p) for p in glob.glob(os.path.join(new, "xfx-*.ansi"))) - set(names))
same = differ = missing = 0
for name in names:
    other = os.path.join(new, name)
    label = name[len("xfx-"):-len(".ansi")]
    if not os.path.exists(other):
        print("%-70s MISSING in the fix's run" % label)
        missing += 1
        continue
    a, b = rows(os.path.join(base, name)), rows(other)
    diff = [i for i in range(max(len(a), len(b))) if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]
    if not diff:
        print("%-70s identical" % label)
        same += 1
    else:
        i = diff[0]
        print("%-70s rows %s differ; row %d: base %r / fix %r" % (label, [d + 1 for d in diff], i + 1,
              cells.text_of(a[i]) if i < len(a) else None, cells.text_of(b[i]) if i < len(b) else None))
        differ += 1
for name in only_new:
    print("%-70s only in the fix's run" % name)
print("%d snaps: %d identical, %d differ, %d missing, %d only in the fix's run" % (len(names), same, differ, missing, len(only_new)))
