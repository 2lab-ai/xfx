"""Where the receipts rule's 103 scenarios come from, and what two narrower
readings of it would select. Changes nothing; a proposal's numbers.

usage: python3 -I receipts_rule.py <repo> <base rev> <sweep step log>

For every receipt the change adds or edits (ci_sweep.RECEIPT_PATH) it reads
the scenario names ci_sweep.names_in finds in the whole file (the rule as it
is), then the names in the lines the change adds to it (`git diff -U0`), and
whether the file is new. Times are the head sweep's run-seconds per
scenario, from the step log (fx + xfx).
"""

import collections
import os
import re
import sys

repo, base, log = sys.argv[1:4]
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import capfile  # noqa: E402
import ci_sweep  # noqa: E402

RUN = re.compile(r"^\S+Z \[\d+/\d+\] (?:fx|xfx) (\S+): .* \((\d+)s\)$")
per = collections.defaultdict(int)
in_head = True
with open(log, encoding="utf-8-sig") as f:
    for row in f:
        parts = row.rstrip("\n").split("\t")
        if len(parts) < 3:
            continue
        line = parts[2].lstrip("﻿")
        if "-> <out>/head-retry" in line:
            in_head = False
        m = RUN.match(line)
        if m and in_head:
            per[m.group(1)] += int(m.group(2))

tree = ci_sweep.Tree(os.path.join(repo, "scripts", "fxcap", "scenarios"))
index = ci_sweep.reading_index(capfile.units(tree.scenarios))
core = ci_sweep.read_core(os.path.join(repo, "scripts", "fxcap", "ci-core.txt"))
chosen, _ = ci_sweep.select(tree, base, core)
not_receipt = {n for n, why in chosen.items() if any(not w.startswith("receipt") for w in why)}

paths = [p for p in ci_sweep.changed_paths(tree, base) if ci_sweep.RECEIPT_PATH.match(p)]
added = set(tree.git("diff", "--name-only", "--no-renames", "--diff-filter=A", "-z", base, "HEAD").decode().split("\0"))
whole, plus, new_only = {}, {}, {}
for p in paths:
    names = {index[n] for n in ci_sweep.names_in(p, os.path.join(tree.repo, p)) if n in index}
    diff = tree.git("diff", "-U0", "--no-renames", base, "HEAD", "--", p).decode("utf-8", errors="replace")
    plus_text = "\n".join(l[1:] for l in diff.split("\n") if l.startswith("+") and not l.startswith("+++"))
    found = set()
    for text in (p, plus_text):
        for m in ci_sweep.RECEIPT_NAME.finditer(text):
            found.add(m.group(1) or m.group(2))
    whole[p] = names
    plus[p] = {index[n] for n in found if n in index}
    new_only[p] = names if p in added else set()

def report(title, sel):
    names = set().union(*sel.values()) if sel else set()
    extra = names - not_receipt
    print("%s: %d scenario(s) beyond core/changed/uses, run-seconds %d" % (title, len(extra), sum(per[n] for n in extra)))
    return extra

print("receipts the change adds or edits: %d (%d new)" % (len(paths), sum(1 for p in paths if p in added)))
by_dir = collections.Counter()
for p in paths:
    if whole[p] - not_receipt:
        by_dir[p.split("/receipts/", 1)[1].split("/", 1)[0]] += 1
print("receipt folders that name a receipt-only scenario: %d" % len(by_dir))
for d, n in by_dir.most_common():
    print("  %-34s %3d file(s)" % (d, n))
w = report("\nthe rule as it is (names anywhere in each such file)", whole)
pl = report("names on the lines the change adds", plus)
nw = report("names in receipts the change adds (new files only)", new_only)
edited = {p: plus[p] for p in paths if p not in added}
report("names on the added lines of the %d receipts it edits (not new)" % len(edited), edited)
total = sum(per[n] for n in per if n != "(inventory)")
core_changed = sum(per[n] for n in not_receipt)
print("\nhead run-seconds: all %d; core/changed/uses alone %d" % (total, core_changed))
for title, extra in (("as it is", w), ("added lines", pl), ("new files", nw)):
    s = core_changed + sum(per[n] for n in extra)
    print("  %-12s %3d scenarios, run-seconds %5d, head wall at 1.98/s about %4.0f s" % (title, len(not_receipt) + len(extra), s, s / 1.98))
print("\nadded-lines rule, its scenarios: %s" % " ".join(sorted(pl)))
print("added-lines set == new-files set: %s; added-lines within the whole-file set: %s" % (pl == nw, pl <= w))
