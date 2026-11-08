"""Pass / pairs per family the brief names, in each sweep given (train 6's,
its families train 7's).
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch, collections
PASS = ("identical", "normalized-identical")
FAMILIES = [("p5-lifecycle-*", ["p5-lifecycle-*"]),
            ("model and catalog (model*, h1-models*, p5-input-model*)", ["model*", "h1-models*", "p5-input-model*"]),
            ("Ctrl-O (*ctrlo*, fulltranscript*)", ["*ctrlo*", "fulltranscript*"]),
            ("p5-transcript-*", ["p5-transcript-*"]),
            ("the cancel scenarios (interrupt*, question*)", ["interrupt*", "question*"]),
            ("LAUNCH-SETTLE-F's (p5-esc-clear, p5-mcp-fail*, p5-mcp-menu-old, h1-models2)", ["p5-esc-clear", "p5-mcp-fail*", "p5-mcp-menu-old", "h1-models2"]),
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
