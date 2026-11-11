#!/usr/bin/env python3
"""Train 14, ci-08066ac9 (new): run.sh's sweeps read unit by unit. For each
side of each unit: whether its run directory's names hold an `l` (the
sweep's shared `fxsw.` name, the capture's own `fxcap.` name; read from
wrap.sh's scratch log, never printed), whether the folder's absolute path
`<TMPDIR>/<fxsw>/<NNN>/<fxcap>/extra/notes/` holds `p`, `l`, `a`, `n` in
order (lower-cased: the subsequence both products require, trace.txt), and
whether `at-plan`'s row 13 lists the folder (and row 12 its `plan.md`); then the pair's verdict
(pairs.json). Then the counts, and rows 1-20 of `at-plan`, fx's and xfx's
side by side, for one unit of each kind on each tree, each placeholder
written short as `<name~W>`, W its width in columns (so a row fits).

    python3 -I -B read.py <out> <tmp24>
"""
import collections
import glob
import json
import os
import re
import sys

out, tmp = sys.argv[1:3]


def holds(text, query="plan"):
    at = 0
    for ch in text.lower():
        if at < len(query) and ch == query[at]:
            at += 1
    return at == len(query)


def short(row):
    return re.sub(r"<([a-z]+(?::real)?)_*>", lambda m: f"<{m.group(1)}~{len(m.group(0))}>", row)


def rows(path):
    return open(path, encoding="utf-8").read().split("\n") if os.path.exists(path) else []


kinds = collections.Counter()
agree = total = row12 = 0
examples = {}
print("# per unit: tree sweep unit | fxsw holds l | fx: own l, path holds plan, row 13 lists | xfx: the same | verdict")
for tree in ("head", "base"):
    for i in ("1", "2", "3"):
        names = {}
        for line in open(os.path.join(out, f"names-{tree}-{i}.tsv"), encoding="utf-8"):
            side, scen, fxsw, fxcap = line.rstrip("\n").split("\t")
            names[(scen, side)] = (fxsw, fxcap)
        verdicts = {p["scenario"]: p["verdict"] for p in json.load(open(os.path.join(out, f"{tree}-{i}", "pairs.json")))
                    if p["label"] == "at-plan"}
        for scen in sorted(verdicts):
            cells, listed = [], {}
            shared = None
            for side in ("fx", "xfx"):
                fxsw, fxcap = names[(scen, side)]
                shared = "l" in fxsw.lower()
                path = f"{tmp.rstrip('/')}/{fxsw}/000/{fxcap}/extra/notes/"
                r = rows(os.path.join(out, f"{tree}-{i}", "runs", scen, side, f"{side}-{scen}-40x120-at-plan.txt"))
                lists = len(r) > 12 and r[12].rstrip().endswith("/extra/notes/")
                row12 += len(r) > 11 and r[11].rstrip().endswith("/extra/notes/plan.md")
                predicted = holds(path)
                listed[side] = lists
                total += 1
                agree += predicted == lists
                cells.append(f"{'l' if 'l' in fxcap.lower() else '-'} {'yes' if predicted else 'no '} {'lists' if lists else 'blank'}")
            kind = {(True, True): "both list", (True, False): "fx only (CI's frame)",
                    (False, True): "xfx only", (False, False): "neither"}[(listed["fx"], listed["xfx"])]
            kinds[(tree, kind, verdicts[scen])] += 1
            examples.setdefault((tree, kind), (i, scen))
            print(f"{tree} {i} {scen[-2:]} | {'l' if shared else '-'} | fx {cells[0]} | xfx {cells[1]} | {verdicts[scen]}")
print()
print(f"# sides whose row 13 lists the folder exactly when its path holds p, l, a, n in order: {agree} of {total}")
print(f"# sides whose row 12 is the folder's `plan.md` (listed by its name, whatever the path): {row12} of {total}")
for tree in ("head", "base"):
    print(f"# {tree}: " + "; ".join(f"{k[1]} {v} ({k[2]})" for k, v in sorted(kinds.items()) if k[0] == tree))
for (tree, kind), (i, scen) in sorted(examples.items()):
    print()
    print(f"## rows 1-20 of at-plan, {tree} sweep {i} {scen}: {kind} -- fx | xfx")
    fx = rows(os.path.join(out, f"{tree}-{i}", "runs", scen, "fx", f"fx-{scen}-40x120-at-plan.txt"))
    xfx = rows(os.path.join(out, f"{tree}-{i}", "runs", scen, "xfx", f"xfx-{scen}-40x120-at-plan.txt"))
    for n in range(20):
        a = short(fx[n].rstrip()) if n < len(fx) else ""
        b = short(xfx[n].rstrip()) if n < len(xfx) else ""
        print(f"{n + 1:2} {a[:64]:64} | {b[:64]}")
