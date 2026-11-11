#!/usr/bin/env python3
"""Train 14's join of main (new): checks.txt's lines from checks.sh's out
directory -- each harness suite's 'Ran N tests' and its verdict, each cargo
test binary's 'test result' line and their sums; then, for every
`workspace-index` sweep named (checks.sh's `wi24-<i>`, `wi49-<i>` and the
twelve copies, `rep24`, `rep49`; checks_l.sh's `rep24l`), each unit's
verdict for every pair and, for `at-plan`, whether row 13 lists the added
folder (`/extra/notes/`) on each side and whether the two sides' row 13 is
the same text and the same attributes (the `.ansi` row). No run
directory's name is read or written.

    python3 -I -B checks_txt.py <out-dir> <sweep>...
"""
import glob
import json
import os
import re
import sys

PASS = ("identical", "normalized-identical")
out = sys.argv[1]


def lines(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().split("\n")


for name in ("test-sweep", "test-capture"):
    print(f"## {name}: every suite's 'Ran' line and verdict")
    for l in lines(os.path.join(out, name + ".log")):
        if re.match(r"^(Ran [0-9]+ tests|OK|FAILED)", l):
            print("  " + l)
print("## cargo-test: each binary's 'test result' line, then the sums")
p = f = i = n = 0
for l in lines(os.path.join(out, "cargo-test.log")):
    m = re.match(r"^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", l)
    if m:
        print("  " + l)
        p, f, i, n = p + int(m[2]), f + int(m[3]), i + int(m[4]), n + 1
print(f"# sum over {n} binaries: {p} passed, {f} failed, {i} ignored")

total = {"at-plan": [0, 0], "rows": {}}
for sweep in sys.argv[2:]:
    d = os.path.join(out, sweep)
    pairs = json.load(open(os.path.join(d, "pairs.json"), encoding="utf-8"))
    print(f"## {sweep}: {len(pairs)} pairs")
    by = {}
    for q in pairs:
        by.setdefault(q["label"], []).append(q)
    for label, qs in by.items():
        verdicts = {}
        for q in qs:
            verdicts[q["verdict"]] = verdicts.get(q["verdict"], 0) + 1
        ok = sum(q["verdict"] in PASS for q in qs)
        print(f"  {label:20} {len(qs)} unit(s): " + ", ".join(f"{k} {v}" for k, v in sorted(verdicts.items())) + f"; pass {ok}/{len(qs)}")
        if label == "at-plan":
            total["at-plan"][0] += ok
            total["at-plan"][1] += len(qs)
    for q in by.get("at-plan", []):
        scen = q["scenario"]
        row = {}
        for side in ("fx", "xfx"):
            txt = glob.glob(os.path.join(d, "runs", scen, side, f"{side}-{scen}-*-at-plan.txt"))
            ansi = glob.glob(os.path.join(d, "runs", scen, side, f"{side}-{scen}-*-at-plan.ansi"))
            t = lines(txt[0]) if len(txt) == 1 else []
            a = lines(ansi[0]) if len(ansi) == 1 else []
            row[side] = (t[12] if len(t) > 12 else None, a[12] if len(a) > 12 else None)
        listed = [s for s in ("fx", "xfx") if row[s][0] is not None and "/extra/notes/" in row[s][0]]
        kind = "both list" if len(listed) == 2 else "neither" if not listed else listed[0] + " only"
        same_t = row["fx"][0] == row["xfx"][0]
        same_a = row["fx"][1] == row["xfx"][1]
        total["rows"][kind] = total["rows"].get(kind, 0) + 1
        print(f"    {scen} at-plan: {q['verdict']}; row 13: {kind}; text the same: {'yes' if same_t else 'no'}; attributes (.ansi row) the same: {'yes' if same_a else 'no'}")
print(f"# at-plan over every sweep: pass {total['at-plan'][0]} of {total['at-plan'][1]} units; row 13: " + ", ".join(f"{k} {v}" for k, v in sorted(total["rows"].items())))
