"""LOCAL-PATHS: shows that each receipt whose evidence scrub.py rewrote reads
as it did, pair by pair.

usage: python3 -I .prd/tui-phase5/receipts/local-paths/verify.py <base-rev> <manifest.tsv>

For every rewritten file under `.prd/` that has a counterpart on the other
side -- `fx-<x>` and `xfx-<x>` in one directory, or `fx/fx-<x>` and
`xfx/xfx-<x>` (`xfx-base/` too) -- it compares the two sides at <base-rev>
and in the working tree:

  lines      the line numbers at which the two sides differ, by position:
             the same set before and after, or the pair reads otherwise
  cells      for an .ansi pair, scripts/fxcap/cells.py's own reading (row 1
             left out, `/.fx/` read as `/.xfx/`): its exit status and the rows
             it names, before and after
  sweep      for an .ansi pair, the rows that still differ once the parity
             ledger's normalizations are applied, as the sweep compares a
             pair, before and after

A pair whose lines or cells differ in fewer rows after is checked once more
with the run directory's random suffix masked at <base-rev> (the ledger's
run-directory rule): if the rows then agree, the suffix -- different on each
side, gone with the placeholder -- was all the rewrite took away.

It prints row numbers and counts, never a row's text: the text at <base-rev>
holds the paths this receipt removed. A rewritten file with no counterpart is
listed as such: its evidence was rewritten and nothing pairs it.
"""

import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
ROOT = os.path.realpath(os.path.join(HERE, "..", "..", "..", ".."))
FXCAP = os.path.join(ROOT, "scripts", "fxcap")
sys.dont_write_bytecode = True
sys.path.insert(0, FXCAP)

import cells  # noqa: E402

SUFFIX = re.compile(rb"(?<=fxcap\.)[A-Za-z0-9]{6}")
import ledger as ledgermod  # noqa: E402


def git_env():
    return {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}


def at(rev, rel):
    r = subprocess.run(["git", "-C", ROOT, "show", "%s:%s" % (rev, rel)], capture_output=True, env=git_env())
    return r.stdout if r.returncode == 0 else None


def now(rel):
    with open(os.path.join(ROOT, rel), "rb") as f:
        return f.read()


def pairs_of(rel):
    """The (fx, xfx) pairs a file is one side of, by name."""
    d, b = os.path.split(rel)
    parent, last = os.path.split(d)
    if b.startswith("fx-"):
        if last == "fx":
            return [(rel, os.path.join(parent, s, "x" + b)) for s in ("xfx", "xfx-base")]
        return [(rel, os.path.join(d, "x" + b))]
    if b.startswith("xfx-"):
        if last in ("xfx", "xfx-base"):
            return [(os.path.join(parent, "fx", b[1:]), rel)]
        return [(os.path.join(d, b[1:]), rel)]
    # A request body: <side>-<scenario>-<size>-bodies/NNN-<kind>.json.
    if last.startswith("fx-") and last.endswith("-bodies"):
        return [(rel, os.path.join(parent, "x" + last, b))]
    if last.startswith("xfx-") and last.endswith("-bodies"):
        return [(os.path.join(parent, last[1:], b), rel)]
    return []


def differing_lines(a, b):
    la, lb = a.split(b"\n"), b.split(b"\n")
    return sorted(i + 1 for i in range(max(len(la), len(lb))) if (la[i] if i < len(la) else None) != (lb[i] if i < len(lb) else None))


def cells_reading(fx, xfx):
    a = cells.parse(fx.decode("utf-8", "replace").replace("/.fx/", "/.xfx/"))
    b = cells.parse(xfx.decode("utf-8", "replace"))
    rows = [i + 1 for i in range(1, max(len(a), len(b))) if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]
    return (1 if rows else 0), rows


def sweep_reading(fx, xfx, rules):
    a = cells.parse(fx.decode("utf-8", "replace"))
    b = cells.parse(xfx.decode("utf-8", "replace"))
    na, nb = cells.normalize(a, rules["fx"]), cells.normalize(b, rules["xfx"])
    readings = [cells.normalize(a, rules["fx"], absorb=way) for way in ("after", "before")]
    return [i + 1 for i, *_ in cells.differing(na, nb, readings)]


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    base, manifest = sys.argv[1], sys.argv[2]
    led = ledgermod.load(os.path.join(FXCAP, "parity-ledger.toml"), ROOT)
    rules = {side: ledgermod.rules_for(led, side) for side in ("fx", "xfx")}
    rewritten = []
    with open(manifest, encoding="utf-8") as f:
        for line in f.read().splitlines()[1:]:
            rel = line.split("\t")[0]
            if rel.startswith(".prd/") and not rel.endswith(".md") and not rel.startswith("TOTAL"):
                rewritten.append(rel)
    pairs, alone = [], []
    for rel in rewritten:
        found = [p for p in pairs_of(rel) if all(os.path.isfile(os.path.join(ROOT, q)) and at(base, q) is not None for q in p)]
        if not found:
            alone.append(rel)
        pairs += [p for p in found if p not in pairs]
    same = other = suffix_only = 0
    print("pair\tlines before\tlines after\tcells before\tcells after\tsweep before\tsweep after\tverdict")
    for fx, xfx in pairs:
        fb, xb, fn, xn = at(base, fx), at(base, xfx), now(fx), now(xfx)
        row = [differing_lines(fb, xb), differing_lines(fn, xn)]
        if fx.endswith(".ansi"):
            row += [cells_reading(fb, xb), cells_reading(fn, xn), sweep_reading(fb, xb, rules), sweep_reading(fn, xn, rules)]
        else:
            row += ["-", "-", "-", "-"]
        ok = row[0] == row[1] and row[2] == row[3] and row[4] == row[5]
        verdict = "reads the same"
        if not ok:
            # The run directory's random suffix, masked on both sides at
            # <base-rev> as the ledger's run-directory rule masks it: if the
            # two sides then differ where they differ now, the suffix was the
            # whole of what the rewrite took away.
            mb, mx = SUFFIX.sub(b"fxcap.XXXXXX", fb), SUFFIX.sub(b"fxcap.XXXXXX", xb)
            masked = [differing_lines(mb, mx), row[1]]
            if fx.endswith(".ansi"):
                masked += [cells_reading(mb, mx), row[3]]
            if masked[0] == masked[1] and masked[2:3] == masked[3:4] and row[4] == row[5]:
                verdict = "reads the same, but for the run directory's random suffix"
                suffix_only += 1
            else:
                verdict = "READS OTHERWISE"
                other += 1
        else:
            same += 1
        print("%s | %s\t%s\t%s" % (fx, os.path.basename(os.path.dirname(xfx)) + "/" + os.path.basename(xfx), "\t".join(str(x) for x in row), verdict))
    print()
    print("%d pairs read the same, %d the same but for the run directory's random suffix, %d otherwise" % (same, suffix_only, other))
    print("rewritten, with no pair to read against (%d):" % len(alone))
    for rel in alone:
        print("  " + rel)
    return 1 if other else 0


if __name__ == "__main__":
    sys.exit(main())
