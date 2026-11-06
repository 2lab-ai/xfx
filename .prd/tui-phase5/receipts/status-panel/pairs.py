"""The status-panel receipt's verdicts: each fx/xfx snap pair in this
directory classified exactly as the parity sweep classifies it, against the
v0.0.16 pin of `scripts/fxcap/parity-ledger.toml` -- its normalizations, its
approved divergences (with the aligned `cascade` reading) and, for a pair left
unresolved, the causes that name why. No reading of its own.

usage: python3 -I pairs.py [<dir>]   (default: this directory)

One line per pair: `<scenario>-<size>-<label>  <verdict>  [ids]`, where the
ids are the divergences a pair was approved under, or the causes of an
unresolved one (`?` for a row no cause names). The banner row is the
`banner` normalization's, as in the sweep.
"""

import glob
import os
import sys

sys.dont_write_bytecode = True

HERE = os.path.dirname(os.path.realpath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", "..", ".."))
sys.path.insert(0, os.path.join(REPO, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402
import sweep  # noqa: E402

PIN_SHA = "8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2"


def verdict(led, divs, scenario, label, fx_text, xfx_text):
    rules = {side: ledgermod.rules_for(led, side) for side in ("fx", "xfx")}
    a, b = cells.parse(fx_text), cells.parse(xfx_text)
    na, nb = cells.normalize(a, rules["fx"]), cells.normalize(b, rules["xfx"])
    readings = [cells.normalize(a, rules["fx"], absorb=way) for way in ("after", "before")]
    diff = cells.differing(na, nb, readings)
    found, matched = ledgermod.classify(a == b, diff, scenario, label, divs)
    ids = sorted({e for _, hit in matched for e in hit})
    if found == "unresolved-defect":
        promoted, left = ledgermod.cascade(
            sweep.aligned_fx(na, readings, nb), nb, scenario, label, divs, cells.text_of
        )
        if promoted:
            return promoted + " (aligned)", sorted({e for *_, hit in left for e in hit})
        causes, unattributed = sweep.attribute(left, scenario, label, led["causes"])
        return found, causes + (["?"] if unattributed else [])
    return found, ids


def main():
    where = sys.argv[1] if len(sys.argv) > 1 else HERE
    led = ledgermod.load(os.path.join(REPO, "scripts", "fxcap", "parity-ledger.toml"), REPO)
    pin = ledgermod.pin_named(led, PIN_SHA)
    divs = ledgermod.active(led, pin["name"])
    bad = 0
    for fx_path in sorted(glob.glob(os.path.join(where, "fx-*.ansi"))):
        name = os.path.basename(fx_path)[len("fx-") : -len(".ansi")]
        xfx_path = os.path.join(where, "xfx-%s.ansi" % name)
        scenario, size, label = split(name)
        if not os.path.isfile(xfx_path):
            print("%-52s missing xfx capture" % name)
            bad += 1
            continue
        with open(fx_path, encoding="utf-8") as f, open(xfx_path, encoding="utf-8") as g:
            v, ids = verdict(led, divs, scenario, label, f.read(), g.read())
        bad += v == "unresolved-defect" and "?" in ids
        print("%-52s %-34s %s" % (name, v, ",".join(ids) or "-"))
    return 1 if bad else 0


def split(name):
    """`p5-panels-status-short-14x120-down-end` -> (scenario, size, label)."""
    parts = name.split("-")
    at = next(i for i, p in enumerate(parts) if "x" in p and p.replace("x", "").isdigit())
    return "-".join(parts[:at]), parts[at], "-".join(parts[at + 1 :])


if __name__ == "__main__":
    sys.exit(main())
