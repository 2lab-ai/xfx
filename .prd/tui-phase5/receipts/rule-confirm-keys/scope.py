"""Every fx/ snap of this receipt against the same snap under <side>/, cell by
cell with its styles (scripts/fxcap/cells.py's reading), after every
normalization of the parity ledger (each product's banner, the run's
directory, the session id, a turn's seconds). No ledger entry's rows are set
aside, so the two pairs the cause `rule-confirm-card-after-stop` names stay
DIFF here. Blank rows at the bottom are left out;
everything else must be equal, row for row. A snap the side has no capture
of (its run stopped before it) is MISSING, and counts as different.

usage: python3 -I scope.py <red|xfx>   (reads this directory)

APPROVAL-CTRL-J's scope.py with no entries set aside, for this receipt's three
scenarios.
"""

import importlib.util
import os
import sys

sys.dont_write_bytecode = True
here = os.path.dirname(os.path.realpath(__file__))
root = os.path.join(here, "..", "..", "..", "..")


def module(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(root, "scripts", "fxcap", name + ".py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


cells = module("cells")
ledgermod = module("ledger")
LEDGER = os.path.join(root, "scripts", "fxcap", "parity-ledger.toml")
RULES = {side: ledgermod.rules_for(ledgermod.load(LEDGER, root), side) for side in ("fx", "xfx")}
SCENARIOS = ["p5-rule-confirm-keys-40x120", "p5-rule-confirm-keys-turn-40x120", "p5-rule-confirm-keys-cmd-40x120"]


def kept(path, side):
    """The rows of one capture, normalized: (text, cells)."""
    out = []
    for row in cells.normalize(cells.rows(path, side == "fx"), RULES[side]):
        out.append((cells.text_of(row), row))
    while out and not out[-1][0].strip():
        out.pop()
    return out


def main():
    side = sys.argv[1]
    same = total = 0
    for name in SCENARIOS:
        for f in sorted(os.listdir(os.path.join(here, "fx"))):
            if not (f.startswith("fx-" + name + "-") and f.endswith(".ansi")):
                continue
            # The scenario's own snaps, not those of the -turn and -cmd scenarios its name begins.
            if any(o != name and f.startswith("fx-" + o + "-") for o in SCENARIOS):
                continue
            label = f[len("fx-" + name + "-") : -len(".ansi")]
            what = "%s %s" % (name[: -len("-40x120")], label)
            total += 1
            other = os.path.join(here, side, "xfx-%s-%s.ansi" % (name, label))
            if not os.path.exists(other):
                print("MISSING  %-44s (the run stopped before it)" % what)
                continue
            fx = kept(os.path.join(here, "fx", f), "fx")
            xfx = kept(other, "xfx")
            rows = [i for i in range(max(len(fx), len(xfx))) if (fx[i][1] if i < len(fx) else None) != (xfx[i][1] if i < len(xfx) else None)]
            if not rows:
                same += 1
                print("SAME     %-44s (%d rows compared)" % (what, len(fx)))
                continue
            print("DIFF     %-44s %d rows differ" % (what, len(rows)))
            for i in rows[:3]:
                print("         fx:  %r" % (fx[i][0] if i < len(fx) else None))
                print("         xfx: %r" % (xfx[i][0] if i < len(xfx) else None))
    print("%d of %d pairs identical" % (same, total))
    return 0 if same == total else 1


if __name__ == "__main__":
    sys.exit(main())
