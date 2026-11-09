"""Pass / pairs per family in the final sweep (selection-final.txt), in each
sweep given: train 7's families.py with the families of train 9's four units.
usage: families_final.py <name>=<sweep-out>..."""
import json, sys, fnmatch
PASS = ("identical", "normalized-identical")
FAMILIES = [("the stop scenarios (*stop*, interrupt*, compact-cancel, p5-transcript-cancel)",
             ["*stop*", "interrupt*", "compact-cancel", "p5-transcript-cancel"]),
            ("model and catalog (model*, h1-models*, p5-input-model*, settings-model*)",
             ["model*", "h1-models*", "p5-input-model*", "settings-model*"]),
            ("p5-lifecycle-*", ["p5-lifecycle-*"]),
            ("p5-stop-in-catalog-wait", ["p5-stop-in-catalog-wait"]),
            ("the question scenarios (p5-input-question*, question*)", ["p5-input-question*", "question*"]),
            ("the paste scenarios (p5-input-paste*, paste-*, *-paste)", ["p5-input-paste*", "paste-*", "*-paste"]),
            ("provider* and resume* (HISTORY-NOTICE-STOP's)", ["provider*", "resume*", "undo-resume"]),
            ("provider-carry-back-stop", ["provider-carry-back-stop"]),
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
