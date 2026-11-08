"""Pass / pairs per family the brief names, in each sweep given (train 7's
families.py, its families train 8's).
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch, collections, os
HERE = os.path.dirname(os.path.abspath(__file__))
SEL = [l.strip() for l in open(os.path.join(HERE, "selection.txt")) if l.strip() and not l.startswith("#")]
GRID = SEL[:16]
F48 = SEL[17:]
assert SEL[16] == "*ctrlo*" and len(F48) == 48, (SEL[16], len(F48))
PASS = ("identical", "normalized-identical")
FAMILIES = [("Ctrl-O (*ctrlo*, fulltranscript*)", ["*ctrlo*", "fulltranscript*"]),
            ("p5-transcript-*", ["p5-transcript-*"]),
            ("tool groups (toolgroup*, p5-toolgroup-*)", ["toolgroup*", "p5-toolgroup-*"]),
            ("the cancel scenarios (interrupt*, question*, compact-cancel)", ["interrupt*", "question*", "compact-cancel"]),
            ("the wraps, the resize, p5-inline-code-*", ["prose-wrap", "table-wrap", "card-wrap", "cmd-wrap", "composer-wrap", "notice-wrap", "h1-resize", "p5-inline-code-*"]),
            ("GRID-ROW-COST's selection (75)", GRID),
            ("FLAKE-MCPMODE's 48 (scenarios.tsv)", F48),
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
