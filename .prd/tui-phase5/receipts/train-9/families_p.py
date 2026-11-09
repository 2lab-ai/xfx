"""Pass / pairs per family in merge 3's paste sweep (selection-p.txt), in each
sweep given: families.py's table with the paste and question families.
usage: families_p.py <name>=<sweep-out>..."""
import json, sys, fnmatch
PASS = ("identical", "normalized-identical")
FAMILIES = [("the paste scenarios (p5-input-paste*, paste-*, *-paste)", ["p5-input-paste*", "paste-*", "*-paste"]),
            ("the question scenarios (p5-input-question*, question*)", ["p5-input-question*", "question*"]),
            ("p5-input-question20, -21", ["p5-input-question20", "p5-input-question21"]),
            ("the approvals (approvals*, p5-input-approval)", ["approvals*", "p5-input-approval"]),
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
