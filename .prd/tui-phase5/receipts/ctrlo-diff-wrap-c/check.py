"""DIFF-WRAPc's checker: DIFF-WRAPb's (`../ctrlo-diff-wrap-b/check.py`) over
the same four scenarios, `p5-transcript-diffwrap2` now with its lines 15-20
(a U+FE0F or a U+0301 right after a tab, and one before a tab).

usage: python3 -I -B check.py <repo> <receipt-dir> [xfx|red]

Reads `fx/` and `xfx/` (the build under the receipt) or `red/` (the build
before it), each `<prefix>-<scenario>-40x120-<snap>.ansi`, and the ledger at
<repo>. Per scenario:

  full    the whole `full` screen, position by position, the ledger's
          normalizations applied: every row the same -- cells, text and
          attributes, as the capture holds them -- but a row an approved
          divergence or a named cause of the ledger covers, each such row
          printed with what covers it, and only the ids the claim names;
  rows    the diff block: every row from the first `  │ N ± ` row to the
          blank row under it, fx's and this side's, the same count and
          each pair the same cells but where the claim names the line;
  margin  no cell of this side's `full` past the screen's 120 columns;
  inline  `edited` and `closed`: every row the same.

Exit 0 when every claim holds.
"""
import os
import re
import sys

repo, receipt = (os.path.realpath(p) for p in sys.argv[1:3])
side = sys.argv[3] if len(sys.argv) > 3 else "xfx"
assert side in ("xfx", "red"), side
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

LEDGER = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
RULES = {"fx": ledgermod.rules_for(LEDGER, "fx"), "xfx": ledgermod.rules_for(LEDGER, "xfx")}
# By id, not by `active`, so the claim reads the same after an entry's date.
ENTRIES = LEDGER["divergences"] + LEDGER["causes"]
FIRST = re.compile(r"^  │ +\d+ [ +-] ")
COLS = 120
SPEC = {
    # Line 3's tail, `5678`, on a row of its own under the gutter.
    "p5-transcript-diffwide": {"covered": set(), "rows": 5},
    # Line 2's read and diff rows: fx re-anchors after U+FFA0 U+FE0E
    # (`frame-variation-selector-anchor`); line 5's read and diff rows,
    # first and continuation: a modifier after no emoji, fx none, xfx two
    # cells (`filler-modifier-count`). Lines 3 and 4 run on as fx's do.
    "p5-transcript-diffwide2": {
        "covered": {"frame-variation-selector-anchor", "filler-modifier-count"},
        "rows": 10,
        "apart": {2, 5},
    },
    # The margin cases: a CJK and an emoji character, `\x1b` and
    # `\u{202e}` cut, a tab before an overflow and one a cell short of the
    # last column, a joined mark, an exact fit, three rows.
    "p5-transcript-diffwrap": {"covered": set(), "rows": 19},
    # Lines 1 and 2: a tab on the walker's second row, `bbbb    Z` and
    # `aaaaaa  X`; line 3: a tab on a full row, `Z` over the last `c`, `W`
    # on; line 4: a tab on a reflowed row; line 12: two tabs at the margin;
    # line 13: `Z` over a wide character's right half, its left cell blank;
    # line 14: a wide character after such a tab, on the next row; line 11:
    # a two-digit line under the gutter, `  │      gggg`; lines 15-19: a
    # mark right after a tab, not drawn (`…k1Z`, `…lZ`/`W`, `…meZ`,
    # `p      q`, `r      s`); line 20: one before a tab, kept.
    "p5-transcript-diffwrap2": {"covered": set(), "rows": 29},
}
results = []


def path(who, scenario, snap):
    prefix = "fx" if who == "fx" else "xfx"
    unit = scenario + "-40x120"
    return os.path.join(receipt, who, unit, "%s-%s-%s.ansi" % (prefix, unit, snap))


def normalized(who, scenario, snap):
    rules = RULES["fx" if who == "fx" else "xfx"]
    raw = cells.parse(open(path(who, scenario, snap), encoding="utf-8").read())
    return raw, [cells.normalize_row(r, rules) for r in raw]


def block(rows):
    """The diff block's rows: from the first numbered diff row to the blank
    row under the last, with the number of the line each row belongs to."""
    texts = [cells.text_of(r) for r in rows]
    at = next(i for i, t in enumerate(texts) if FIRST.match(t))
    out, line = [], None
    for i in range(at, len(texts)):
        if not texts[i]:
            break
        m = FIRST.match(texts[i])
        if m:
            line = int(texts[i].split()[1])
        out.append((line, rows[i]))
    return out


def claim(name, ok, lines):
    results.append(ok)
    print(("ok   " if ok else "FAIL ") + name)
    for line in lines:
        print("       " + line)


for scenario, spec in SPEC.items():
    (_, fa), (rb, fb) = normalized("fx", scenario, "full"), normalized(side, scenario, "full")
    lines, ok, seen = [], len(fa) == len(fb), set()
    for i in range(max(len(fa), len(fb))):
        a, b = (fa[i] if i < len(fa) else []), (fb[i] if i < len(fb) else [])
        if a == b:
            continue
        ta, tb = cells.text_of(a), cells.text_of(b)
        by = {d["id"] for d in ledgermod.covering(ENTRIES, scenario, "full", i + 1, "fx", ta)}
        by |= {d["id"] for d in ledgermod.covering(ENTRIES, scenario, "full", i + 1, "xfx", tb)}
        by &= spec["covered"]
        seen |= by
        ok &= bool(by)
        lines.append("row %d %s: fx %r / %s %r" % (i + 1, sorted(by) or "FAIL, nothing the claim names covers it", ta[-40:], side, tb[-40:]))
    ok &= seen == spec["covered"]
    lines.append("%d rows, %d the same" % (len(fa), sum(1 for i in range(min(len(fa), len(fb))) if fa[i] == fb[i])))
    claim("full %s (fx / %s)" % (scenario, side), ok, lines)

    ba, bb = block(fa), block(fb)
    lines = ["fx %d rows / %s %d rows" % (len(ba), side, len(bb))]
    ok = len(ba) == len(bb) == spec["rows"]
    for (la, a), (lb, b) in zip(ba, bb):
        same = la == lb and a == b
        apart = la in spec.get("apart", ()) and la == lb
        ok &= same or apart
        if not same:
            lines.append("line %s %s: fx %r / %s %r" % (la, "apart (named)" if apart else "FAIL", cells.text_of(a)[-30:], side, cells.text_of(b)[-30:]))
    claim("rows %s (fx / %s)" % (scenario, side), ok, lines)

    wide = [(i + 1, r[-1][0] + cells.width(r[-1][1])) for i, r in enumerate(rb) if r and r[-1][0] + cells.width(r[-1][1]) > COLS]
    claim("margin %s %s" % (scenario, side), not wide, ["rows past column %d: %s" % (COLS, wide)] if wide else ["no cell past column %d" % COLS])

    for snap in ("edited", "closed"):
        (_, ea), (_, eb) = normalized("fx", scenario, snap), normalized(side, scenario, snap)
        diff = [i + 1 for i in range(max(len(ea), len(eb))) if (ea[i] if i < len(ea) else []) != (eb[i] if i < len(eb) else [])]
        claim("inline %s %s" % (scenario, snap), not diff, ["rows %s differ" % diff] if diff else ["all %d rows the same" % len(ea)])
print("%d claims, %d hold, %d fail" % (len(results), sum(results), len(results) - sum(results)))
sys.exit(0 if all(results) else 1)
