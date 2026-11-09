"""Pass / pairs per family the brief names, in each sweep given (train 7's
families.py, with train 9's families).
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch
PASS = ("identical", "normalized-identical")
STOP_UNIT = ["model-*", "p5-lifecycle-osc-stopnext", "p5-input-model*", "p5-stop-in-catalog-wait"]
QUESTIONS_UNIT = ["*question*", "*approval*"]
FAMILIES = [("the stop scenarios (*stop*, interrupt*, compact-cancel, p5-transcript-cancel)",
             ["*stop*", "interrupt*", "compact-cancel", "p5-transcript-cancel"]),
            ("p5-lifecycle-* (STOP-NEXT-2's OSC 7501 and lifecycle)", ["p5-lifecycle-*"]),
            ("model and catalog (model*, h1-models*, p5-input-model*, settings-model*)",
             ["model*", "h1-models*", "p5-input-model*", "settings-model*"]),
            ("p5-input-question*", ["p5-input-question*"]),
            ("p5-stop-in-catalog-wait", ["p5-stop-in-catalog-wait"]),
            ("STOP-IN-CATALOG-WAIT's selection (its sweep's 18)", STOP_UNIT),
            ("QUESTIONS-20's selection (its sweep's 13)", QUESTIONS_UNIT),
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
