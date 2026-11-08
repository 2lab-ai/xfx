"""Pass / pairs per family the brief names, in each sweep given.
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch, collections
PASS = ("identical", "normalized-identical")
FAMILIES = [("p5-ctrlo-wider-*", ["p5-ctrlo-wider-*"]),
            ("h1-resize", ["h1-resize"]),
            ("the cancel scenarios (p5-transcript-cancel, interrupt*, question*)", ["p5-transcript-cancel", "interrupt*", "question*"]),
            ("p5-transcript-* (all)", ["p5-transcript-*"]),
            ("every scenario swept", ["*"])]
sweeps = [a.split("=", 1) for a in sys.argv[1:]]
data = {n: json.load(open(f"{d}/pairs.json")) for n, d in sweeps}
print("| family | " + " | ".join(n for n, _ in sweeps) + " |")
print("|---|" + "---|" * len(sweeps))
for name, globs in FAMILIES:
    cells = []
    for n, _ in sweeps:
        ps = [p for p in data[n] if any(fnmatch.fnmatch(p["scenario"], g) for g in globs)]
        cells.append(f"{sum(p['verdict'] in PASS for p in ps)} / {len(ps)}" if ps else "-")
    print(f"| {name} | " + " | ".join(cells) + " |")
