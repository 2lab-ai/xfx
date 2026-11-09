"""Every fx/xfx capture pair of this receipt, compared two ways.

usage: python3 -I compare.py <fx dir> <xfx dir>

raw   `cells.py`'s comparison (scripts/fxcap/cells.py, the M1 receipt's):
      every cell and its attributes from row 2 on.
norm  the same after the ledger's normalizations, the sweep's reading
      (scripts/fxcap/sweep.py `score`): each product's banner and name,
      a turn summary's wall seconds, and the rest of
      scripts/fxcap/parity-ledger.toml's [[normalization]] list. The rule ids
      that changed a differing row are named.

Run from the repository root's receipt directory; it finds scripts/fxcap
from its own path.
"""

import glob
import os
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
FXCAP = os.path.join(HERE, "..", "..", "..", "..", "scripts", "fxcap")
sys.path.insert(0, FXCAP)
import cells  # noqa: E402
import ledger  # noqa: E402

REPO = os.path.join(FXCAP, "..", "..")


def main(fx_dir, xfx_dir):
    led = ledger.load(os.path.join(FXCAP, "parity-ledger.toml"), REPO)
    rules = {side: ledger.rules_for(led, side) for side in ("fx", "xfx")}
    for fx_path in sorted(glob.glob(os.path.join(fx_dir, "fx-*.ansi"))):
        name = os.path.basename(fx_path)[len("fx-") :]
        xfx_path = os.path.join(xfx_dir, "xfx-" + name)
        if not os.path.isfile(xfx_path):
            print("MISSING %s" % name)
            continue
        a = cells.parse(open(fx_path, encoding="utf-8").read())
        b = cells.parse(open(xfx_path, encoding="utf-8").read())
        raw = cells.differing(a[1:], b[1:])
        na, nb = cells.normalize(a, rules["fx"]), cells.normalize(b, rules["xfx"])
        readings = [cells.normalize(a, rules["fx"], absorb=way) for way in ("after", "before")]
        norm = cells.differing(na, nb, readings)
        if not raw:
            print("SAME raw %s" % name)
        elif not norm:
            used = []
            for i, *_ in raw:
                for rule in rules["fx"] + rules["xfx"]:
                    texts = (cells.text_of(a[i + 1]), cells.text_of(b[i + 1]))
                    if any(rule["pattern"].search(t) for t in texts) and rule["id"] not in used:
                        used.append(rule["id"])
            print("SAME normalized %s: %d raw row(s) differ, equal after %s" % (name, len(raw), ", ".join(used)))
        else:
            print("DIFF %s: %d row(s) differ after normalizing" % (name, len(norm)))
            for i, ft, xt, _, same in norm[:4]:
                print("    row %d: fx  %r" % (i + 1, ft))
                print("    row %d: xfx %r%s" % (i + 1, xt, " (attributes only)" if same else ""))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
