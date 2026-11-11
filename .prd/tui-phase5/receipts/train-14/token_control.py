#!/usr/bin/env python3
# Train 14: train 13's token_control.py, copied; what changed for this train, if anything, is said below this line.
# Train 13: train 12's token_control.py, unchanged but for this line.
"""Train 12 (train 10's and 11's token_control.py; it now plants on every row
both sides of a merge changed, and says so for a merge with none -- train 12's
merges 1 and 2 change no docs/parity.md row on the units' side, where train
11's form stopped on a KeyError). Train 10: the two token cases of
token_check.py's negative control, planted
where they can land. token_check.py's `self_test` picks the first
docs/parity.md row a side changed and plants on that side's text of it
(`pm.replace(side, row, 1)`); when both sides changed that row -- merges 1
and 2 here, the `/model` row -- the merged row is neither side's text, the
replace finds nothing, and the case reads MISSED without having planted
anything. This script says, for each merge, whether that is so, and plants
the same two cases on the merged row itself, for each side that changed it:
a token the side added dropped from the merged row, and a token the side
deleted put back; each must be found by token_check.py's own check_parity.

    python3 -I -B token_control.py <repo> <merge>...

token_check.py is read as it is (byte for byte train 7's) and run without its
last line, `main()`, so its functions can be called; nothing of it changes.
"""
import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, "token_check.py"), encoding="utf-8").read()
assert src.endswith("\n\nmain()\n"), "token_check.py does not end in main()"
tc = {"__name__": "token_check", "__file__": os.path.join(here, "token_check.py")}
exec(compile(src[: -len("main()\n")], "token_check.py", "exec"), tc)

repo, merges = sys.argv[1], sys.argv[2:]
total = found = 0
for merge in merges:
    ours, theirs = tc["git"](repo, "rev-parse", f"{merge}^1", f"{merge}^2").split()
    base = tc["git"](repo, "merge-base", ours, theirs).strip()
    pb, po, pt, pm = (tc["show"](repo, c, tc["PARITY"]) for c in (base, ours, theirs, merge))
    pb, po, pt, pm = tc["masked_repoints"](pb, po, pt, pm)
    b, o, t, m = (tc["rows"](x) for x in (pb, po, pt, pm))
    keys = [k for k in b if k[0] == "row" and o.get(k) not in (None, b[k]) and t.get(k) not in (None, b[k])]
    if not keys:
        print(f"merge {merge[:8]}: no docs/parity.md row both sides changed; nothing to plant")
        continue
    for key in keys:
        print(f"merge {merge[:8]}: row {key[1]!r}, changed by both sides; "
              f"self_test's plant on a side's text of it {'lands' if (o[key] in pm or t[key] in pm) else 'finds nothing to replace (the merged row is neither side text)'}")
        row = m[key]
        for name, sx in (("ours", o.get(key)), ("theirs", t.get(key))):
            added = sorted(tc["tokens"](sx) - tc["tokens"](b[key]))
            gone = sorted(tc["tokens"](b[key]) - tc["tokens"](sx))
            cases = []
            tok = next((a for a in added if (" " + a) in row), None)
            if tok:
                cases.append((f"a token {name} added ({tok[:30]!r}) dropped from the merged row", row.replace(" " + tok, "", 1)))
            if gone:
                cases.append((f"a token {name} deleted ({gone[0][:30]!r}) put back in the merged row", row + " " + gone[0]))
            for what, planted in cases:
                assert planted != row and row in pm
                out = []
                bad = tc["check_parity"](pb, po, pt, pm.replace(row, planted, 1), out)
                print(f"  {what}: {'found' if bad else 'MISSED'}")
                total += 1
                found += bool(bad)
print(f"# {found} of {total} found")
sys.exit(0 if found == total else 1)
