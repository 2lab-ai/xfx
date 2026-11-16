"""usage: python3 -I tail_check.py <ledger.toml>   (from the repo root)
Asks the ledger's own `covering` which cause covers each draft row of the
p5-approval-paste-spaces deny-* snaps: the rows as captured, and the same
rows with the `a b` tail lost. A lost tail must be covered by no amend-draft
cause (round 3, item D)."""
import sys
sys.dont_write_bytecode = True
sys.path.insert(0, "scripts/fxcap")
import ledger

L = ledger.load(sys.argv[1], ".")
FIRST = "  ❯ 3  Don't apply, "
CONT = " " * 20
cases = [
    ("deny-held", "fx", FIRST + "x" * 99, True),
    ("deny-held", "xfx", CONT + "x" * 93, True),
    ("deny-fits", "fx", FIRST + "x" * 96 + "a b", True),
    ("deny-fits", "xfx", FIRST + "x" * 100, True),
    ("deny-fits", "xfx", CONT + "x" * 100, True),
    ("deny-fits", "xfx", CONT + "x" * 93 + "a b ", True),
    ("deny-fits", "fx", FIRST + "x" * 99, False),
    ("deny-fits", "xfx", CONT + "x" * 93, False),
    ("deny-paste-refused", "fx", FIRST + "x" * 99, False),
    ("deny-paste-refused", "xfx", CONT + "x" * 96 + " ", False),
]
bad = 0
for snap, side, text, want in cases:
    got = [c["id"] for c in ledger.covering(L["causes"], "p5-approval-paste-spaces", snap, 20, side, text)
           if c["id"].startswith("amend-draft")]
    ok = bool(got) == want
    bad += not ok
    shown = text.replace("x" * 10, "x10")
    print("%-4s %-19s %-3s %-44r %s" % ("ok" if ok else "BAD", snap, side, shown, got or "uncaused"))
print("%d cases, %d wrong" % (len(cases), bad))
sys.exit(1 if bad else 0)
