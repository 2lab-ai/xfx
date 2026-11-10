"""Every fx/ snap of this receipt against the same snap under <side>/, cell by
cell with its styles (scripts/fxcap/cells.py's reading), after every
normalization of the parity ledger (each product's banner, the run's
directory, the session id, a turn's seconds, the /status panel's build), with
the rows of three ledger entries set aside on each side:

  status-panel-sandbox, status-panel-updates, status-panel-step-limit
                          (approved divergences) /status rows one side has or
                          words differently

Exactly the rows those entries' `fx_row`/`xfx_row` match, read from
scripts/fxcap/parity-ledger.toml. Blank rows at the bottom are left out;
everything else must be equal, row for row. The panel scenario is the only one
those rows can match.

usage: python3 -I pairs.py <red|xfx>   (reads this directory)

APPROVAL-CTRL-J's scope.py, with this receipt's two scenarios and no approval
entries (a model question has no grant scope and no tool comment row).
"""

import importlib.util
import os
import re
import sys
import tomllib

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
raw = tomllib.load(open(LEDGER, "rb"))
ASIDE = ["status-panel-sandbox", "status-panel-updates", "status-panel-step-limit"]
entries = [e for e in raw["cause"] + raw["divergence"] if e["id"] in ASIDE]
assert sorted(e["id"] for e in entries) == sorted(ASIDE), "an entry is missing from the ledger"
SCOPE = {side: [re.compile(e[side + "_row"]) for e in entries if side + "_row" in e] for side in ("fx", "xfx")}
SCENARIOS = ["p5-question-ctrl-j-40x120", "p5-question-ctrl-j-panel-40x120"]


def kept(path, side):
    """The rows of one capture, normalized, with the entries' rows set aside: (text, cells)."""
    out = []
    for row in cells.normalize(cells.rows(path, side == "fx"), RULES[side]):
        text = cells.text_of(row)
        if any(p.search(text) for p in SCOPE[side]):
            continue
        out.append((text, row))
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
            label = f[len("fx-" + name + "-") : -len(".ansi")]
            fx = kept(os.path.join(here, "fx", f), "fx")
            xfx = kept(os.path.join(here, side, "xfx-%s-%s.ansi" % (name, label)), "xfx")
            total += 1
            rows = [i for i in range(max(len(fx), len(xfx))) if (fx[i][1] if i < len(fx) else None) != (xfx[i][1] if i < len(xfx) else None)]
            what = "%s %s" % (name[: -len("-40x120")], label)
            if not rows:
                same += 1
                print("SAME  %-48s (%d rows compared)" % (what, len(fx)))
                continue
            print("DIFF  %-48s %d rows differ" % (what, len(rows)))
            for i in rows[:3]:
                print("      fx:  %r" % (fx[i][0] if i < len(fx) else None))
                print("      xfx: %r" % (xfx[i][0] if i < len(xfx) else None))
    print("%d of %d pairs identical once the entries' rows are set aside" % (same, total))
    return 0 if same == total else 1


if __name__ == "__main__":
    sys.exit(main())
