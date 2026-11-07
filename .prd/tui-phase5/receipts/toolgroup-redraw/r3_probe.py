"""Which rows `toolgroup-ctrlo-record-short` covers, read off the ledger
(`ledger._covers`), and the Ctrl-O pairs of two sweeps compared.

usage: python3 -I r3_probe.py <fxcap dir> <ledger> <sweep A pairs.json> <sweep B pairs.json>
"""
import json, sys
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import ledger  # noqa: E402

led = ledger.load(sys.argv[2])
cause = [c for c in led["causes"] if c["id"] == "toolgroup-ctrlo-record-short"][0]
print("## toolgroup-ctrlo-record-short covers (ledger._covers)")
for unit in ("p5-toolgroup-ctrlo", "p5-toolgroup-ctrlo-tc"):
    for row, side, text in ((34, "fx", "  XXXXXXXXXXXXXXXXXXXXXXX KST · Usage"), (35, "fx", "  <wall> (↑4 ↓80)"),
                            (36, "fx", ""), (34, "xfx", ""), (37, "xfx", ""), (35, "xfx", "  any text"),
                            (33, "xfx", ""), (38, "xfx", "")):
        print("%-22s full-light row %d %-3s %-40r %s" % (unit, row, side, text, ledger._covers(cause, unit, "full-light", row, side, text)))

def load(p):
    return {(x["scenario"], x["label"]): x for x in json.load(open(p)) if x["scenario"].startswith("p5-toolgroup-ctrlo")}
a, b = load(sys.argv[3]), load(sys.argv[4])
print()
print("## the Ctrl-O pairs: round 2's head sweep against this one (same fx, same xfx f482d340)")
print("pairs %d / %d, same pairs: %s" % (len(a), len(b), set(a) == set(b)))
moved = 0
def sig(z):
    return (z["verdict"], sorted(z.get("causes") or []), z.get("unattributed") or [],
            [(r["side"], r["row"], r["text"]) for r in z.get("aligned") or []])
for k in sorted(a):
    same = sig(a[k]) == sig(b[k])
    moved += not same
    y = b[k]
    print("%-22s %-16s %-22s causes=%s uncaused=%d aligned=%d  %s" % (
        k[0], k[1], y["verdict"], sorted(y.get("causes") or []), len(y.get("unattributed") or []),
        len(y.get("aligned") or []), "same" if same else "MOVED %s -> %s" % (sig(a[k])[:3], sig(b[k])[:3])))
print("moved: %d" % moved)
sys.exit(1 if moved else 0)
