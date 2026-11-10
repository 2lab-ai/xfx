#!/usr/bin/env python3
"""The narrow sweeps' pairs, classified by the parity sweep's own code
(`sweep.score`: the normalizations, the approved divergences with the
aligned `cascade` reading, and the causes of an unresolved pair), base
against head.

    python3 -I -B narrow_pairs.py <scripts/fxcap> <base out prefix> <head out prefix> <group>...

`<prefix>-<group>` is a `narrow-sweeps.sh` output: its run.json, its captures
under runs/ and the .cap files it ran under scenarios-used/. Each is scored
here rather than read from its pairs.json, because a sweep at 50 columns
stops at fx's inventory before it writes one (narrow-sweeps.sh). Scoring
writes the sweep's diffs/ into the output again; nothing else changes.

One row per pair, in three readings:

  base        the base binary's captures, this tree's ledger
  no-reader   the head binary's captures, this tree's ledger with every
              `wrapped` turned off: the banner rule as round 1 left it
  head        the head binary's captures, this tree's ledger

and the per-group counts. `pass` is identical or normalized-identical, the
sweep's own measure. A pair is DROPPED when its `head` verdict ranks below its
`base` one -- pass, then approved (approved-divergence, its cascade,
replaced-in-next-pin), then unresolved, then failed or timeout -- or when
`head` has no such pair. Exit 1 when any pair dropped.
"""
import collections
import json
import os
import sys

sys.dont_write_bytecode = True
fxcap, base_prefix, head_prefix, *groups = sys.argv[1:]
sys.path.insert(0, os.path.abspath(fxcap))

import capfile  # noqa: E402
import ledger as ledgermod  # noqa: E402
import sweep  # noqa: E402

PASS = ("identical", "normalized-identical")
APPROVED = ("approved-divergence", "approved-divergence-cascade", "replaced-in-next-pin")


def rank(pair):
    if pair is None:
        return -1
    v = pair["verdict"]
    return 3 if v in PASS else 2 if v in APPROVED else 1 if v == "unresolved-defect" else 0


LEDGER = os.path.join(os.path.abspath(fxcap), "parity-ledger.toml")


def verdicts(out, reader=True):
    with open(os.path.join(out, "run.json"), encoding="utf-8") as f:
        run = json.load(f)
    led = ledgermod.load(LEDGER, None)
    for n in led["normalizations"]:
        n["wrapped"] = n["wrapped"] and reader
    pin = ledgermod.pin_named(led, run["fx_sha256"])
    units = [u for u in capfile.units(os.path.join(out, "scenarios-used")) if u.name in set(run["units"])]
    _, pairs, _ = sweep.score(units, out, run["size"], led, pin)
    return run, {(p["scenario"], p["label"]): p for p in pairs}


def short(pair):
    v = pair["verdict"]
    if v == "unresolved-defect":
        causes = pair.get("causes") or []
        extra = (["?"] if pair.get("unattributed") else [])
        return "unresolved (%s)" % ", ".join(causes + extra) if causes or extra else "unresolved"
    return v


dropped, without, agree, total = [], 0, 0, 0
for group in groups:
    run_b, base = verdicts("%s-%s" % (base_prefix, group))
    run_h, head_old = verdicts("%s-%s" % (head_prefix, group), reader=False)
    _, head = verdicts("%s-%s" % (head_prefix, group))
    print("## group %s, %s: base xfx %s, head xfx %s, fx %s" % (
        group, run_b["size"], run_b["xfx_sha256"][:12], run_h["xfx_sha256"][:12], run_b["fx_sha256"][:12]))
    keys = sorted(set(base) | set(head), key=lambda k: (k[0], list(base).index(k) if k in base else 0))
    for key in keys:
        b, ho, h = base.get(key), head_old.get(key), head.get(key)
        total += 1
        without += rank(ho) < rank(b)
        agree += bool(b and h and b["verdict"] == h["verdict"])
        cells = [short(x) if x else "(none)" for x in (b, ho, h)]
        mark = ""
        if rank(h) < rank(b):
            mark = "  DROPPED"
            dropped.append(key)
        elif rank(h) > rank(b):
            mark = "  gained"
        print("%-24s %-22s base: %-32s no-reader: %-32s head: %s%s" % (key[0], key[1], *cells, mark))
    for name, side in (("base", base), ("no-reader", head_old), ("head", head)):
        c = collections.Counter(p["verdict"] for p in side.values())
        print("  %-8s %d pairs, pass %d; %s" % (
            name, len(side), sum(c[v] for v in PASS), ", ".join("%s %d" % kv for kv in sorted(c.items()))))
    print()
print("pairs: %d; the same verdict in base and head: %d; would drop without the reader: %d" % (total, agree, without))
print("dropped: %d%s" % (len(dropped), "".join("\n  %s %s" % k for k in dropped)))
sys.exit(1 if dropped else 0)
