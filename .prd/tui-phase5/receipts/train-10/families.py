"""Pass / pairs per family, in each sweep given: train 9's families_final.py
with the families of train 10's four units and its ledger entries.
usage: families.py <name>=<sweep-out>..."""
import json, sys, fnmatch
PASS = ("identical", "normalized-identical")
FAMILIES = [("model and catalog (model*, p5-input-model*, p5-catalog-bounds-*, h1-models*, settings-model*)",
             ["model*", "p5-input-model*", "p5-catalog-bounds-*", "h1-models*", "settings-model*"]),
            ("p5-catalog-bounds-* (CATALOG-BOUNDS')", ["p5-catalog-bounds-*"]),
            ("p5-input-model-rank, -swap (MODEL-FUZZY's)", ["p5-input-model-rank", "p5-input-model-swap"]),
            ("*effort* (MODEL-FUZZY's)", ["*effort*"]),
            ("the @ list (p5-input-at*, filepicker*, workspace-index)", ["p5-input-at*", "filepicker*", "workspace-index"]),
            ("p5-input-at-height, -resize, -gate (AT-HEIGHT's)", ["p5-input-at-height", "p5-input-at-resize", "p5-input-at-gate"]),
            ("the other p5-input-* (AT-HEIGHT's)", ["p5-input-*"]),
            ("the compaction scenarios (compact*, *compact*)", ["compact*", "*compact*"]),
            ("the stop scenarios (*stop*, interrupt*, p5-transcript-cancel)", ["*stop*", "interrupt*", "p5-transcript-cancel"]),
            ("the band's (composer*, startup, h1-resize, model-column, settings-scrollback, p5-mcp-*)",
             ["composer*", "startup", "h1-resize", "model-column", "settings-scrollback", "p5-mcp-config", "p5-mcp-verbs"]),
            ("the re-pointed ledger entries' (fulltranscript*, p5-transcript-*, p5-toolgroup-ctrlo*, p5-ctrlo-wider-inline-fam, links, read-failures, resume-tools, timeout, approvals, model-parked)",
             ["fulltranscript*", "p5-transcript-*", "p5-toolgroup-ctrlo*", "p5-ctrlo-wider-inline-fam", "links",
              "read-failures", "resume-tools", "timeout", "approvals", "model-parked"]),
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
                                           ["p5-input-model*", "p5-input-at*"])]
        cells.append(f"{sum(p['verdict'] in PASS for p in ps)} / {len(ps)}" if ps else "-")
    print(f"| {name} | " + " | ".join(cells) + " |")
