"""Pass / pairs per family, in each sweep given: train 10's families.py
with the families of train 11's three units and its ledger entries.
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch
PASS = ("identical", "normalized-identical")
FAMILIES = [("the submit scenarios (p5-submit-preview-*, *submit*)", ["p5-submit-preview-*", "*submit*"]),
            ("p5-submit-preview-* (SUBMIT-CARD-PREVIEW's)", ["p5-submit-preview-*"]),
            ("the other p5-transcript-* (SUBMIT-CARD-PREVIEW's sweep's)", ["p5-transcript-*"]),
            ("model and catalog (model*, p5-input-model*, p5-catalog-*, h1-models*, settings-model*)",
             ["model*", "p5-input-model*", "p5-catalog-*", "h1-models*", "settings-model*"]),
            ("the approvals (approvals*)", ["approvals*"]),
            ("the pastes (paste*, p5-paste*, p5-input-paste*)", ["paste*", "p5-paste*", "p5-input-paste*"]),
            ("the other p5-input-* (PASTE-GAP's sweep's)", ["p5-input-*"]),
            ("PASTE-GAP's sweep's others (card-wrap, composer-wrap, compact-toolarge, image-*, question*, skills-paste, p5-effort-fork-*, p5-panels-keys, p5-session-early-prefs)",
             ["card-wrap", "composer-wrap", "compact-toolarge", "image-composer", "image-inline", "image-undo",
              "question", "question-field", "question-other", "skills-paste", "p5-effort-fork-*", "p5-panels-keys",
              "p5-session-early-prefs"]),
            ("the changed ledger entries' others (interrupt2, model-parked, allowlist2, fulltranscript*, links, read-failures, resume-tools, timeout, p5-ctrlo-wider-inline-fam, h1-resize, p5-toolgroup-ctrlo*)",
             ["interrupt2", "model-parked", "allowlist2", "fulltranscript*", "links", "read-failures", "resume-tools",
              "timeout", "p5-ctrlo-wider-inline-fam", "h1-resize", "p5-toolgroup-ctrlo*"]),
            ("every scenario swept", ["*"])]
sweeps = [a.split("=", 1) for a in sys.argv[1:]]
data = {n: json.load(open(f"{d}/pairs.json")) for n, d in sweeps}
print("| family | " + " | ".join(n for n, _ in sweeps) + " |")
print("|---|" + "---|" * len(sweeps))
for name, globs in FAMILIES:
    cells = []
    for n, _ in sweeps:
        ps = [p for p in data[n] if any(fnmatch.fnmatch(p["scenario"], g) for g in globs)]
        if name.startswith("the other p5-input-*"):
            ps = [p for p in ps if not any(fnmatch.fnmatch(p["scenario"], g) for g in
                                           ["p5-input-model*", "p5-input-paste*"])]
        if name.startswith("the other p5-transcript-*"):
            ps = [p for p in ps if not fnmatch.fnmatch(p["scenario"], "*submit*")]
        cells.append(f"{sum(p['verdict'] in PASS for p in ps)} / {len(ps)}" if ps else "-")
    print(f"| {name} | " + " | ".join(cells) + " |")
