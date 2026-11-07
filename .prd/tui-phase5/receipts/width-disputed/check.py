"""WIDTH-DISPUTED's checker: `p5-transcript-diffwide` and its cluster form,
`p5-transcript-diffwide2`, on fx v0.0.16 against xfx.

usage: python3 -I -B check.py <repo> <receipt-dir> [xfx|red]

Reads `fx/` and `xfx/` (the build under the receipt) or `red/` (the build
before it), each `<prefix>-<scenario>-40x120-<snap>.ansi`. Per scenario:

  bytes   every Ctrl-O diff row (`  │ N ± text`) of `full` is the same cells
          on both sides, text and attributes, as the capture holds them
          (`capture-pane -e`; its bytes are printed, and are the same but where
          a row's first SGR follows a row only one side has) -- but a row the
          claim names, and what tells it apart is printed, and a row that
          stands apart (the ledger's `filler-modifier-count`), each side's
          text as the claim holds it;
  margin  each margin line's first row ends as fx's does, the line's text
          cut where fx's count reaches the screen's 112 cells of text: nothing
          written past the margin;
  whole   each line read whole: that side's read rows of it, joined, hold the
          line as the capture keeps it, where fx's lose some of it;
  full    the whole `full` screen, the two sides aligned as sequences (the
          ledger's normalizations applied): every row the same but the rows
          the claim names on each side and the rows the ledger's
          `filler-modifier-count` covers, printed;
  inline  `edited` and `closed`: every row the same.

Exit 0 when every claim holds.
"""
import difflib
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
ROW = re.compile(r"^  │ (\d+) ([ +-]) (.*)$")
READ = re.compile(r"^│ +(\d+) {4}(.*)$")
# The divergence's rows, as a selector only: by id, not `active`, so the
# claim reads the same after the entry's `expires` date.
APART = [d for d in LEDGER["divergences"] if d["id"] == "filler-modifier-count"]
assert len(APART) == 1, "the ledger holds no filler-modifier-count"
W104 = "w" * 104
# fx v0.0.16 re-anchors the cursor after a glyph with a variation selector
# (`ESC[nG`): on line 2 of the cluster form tmux then leaves three unstyled
# cells blank before `x`, in the diff row and in the read row above it (the
# ledger's cause `frame-variation-selector-anchor`).
GAP = 3
SPEC = {
    "p5-transcript-diffwide": {
        "rows": [(1, "-"), (1, "+"), (2, " "), (3, " ")],
        "named": {},
        "apart": {},
        "whole": {},
        "anchored": None,
        "margins": {3: W104 + "\u00ad" * 4 + "TAIL"},
        # fx's continuation of line 3 (ctrl-o-diff-row-clipped).
        "only_fx": ["  │     5678"],
        # The rows the two sides hold alike, at least: the screen's 40 less
        # the rows one side holds alone.
        "matched": 34,
    },
    "p5-transcript-diffwide2": {
        "rows": [(1, "-"), (1, "+"), (2, " "), (3, " "), (4, " "), (5, " "), (6, " ")],
        "named": {(2, " "): "x zwj"},
        # Line 5, four U+1160 U+1F3FD: fx counts the modifier as no cells and
        # its row runs past the margin, the fourth modifier and `TAI` lost;
        # xfx's ends where the margin is (the capture keeps no cell for
        # U+1160). The ledger's divergence `filler-modifier-count`.
        "apart": {(5, " "): (W104 + "\U0001f3fd" * 3 + " L", W104 + "\U0001f3fd" * 2)},
        "whole": {5: W104 + "\U0001f3fd" * 4 + "TAIL5678"},
        "anchored": "half\uffa0\ufe0e",
        "margins": {3: W104 + "\uffa0\u0301" * 4 + "TAIL", 4: W104 + "\u115f\u1160" * 2 + "TA"},
        # fx's continuations of lines 3, 4 and 5 (ctrl-o-diff-row-clipped).
        "only_fx": ["  │     5678", "  │     5678", "  │     IL5678"],
        "matched": 33,
    },
}
results = []


def path(who, scenario, snap):
    prefix = "fx" if who == "fx" else "xfx"
    unit = scenario + "-40x120"
    return os.path.join(receipt, who, unit, "%s-%s-%s.ansi" % (prefix, unit, snap))


def diff_rows(who, scenario):
    lines = open(path(who, scenario, "full"), encoding="utf-8").read().split("\n")
    rows = cells.parse("\n".join(lines))
    out = []
    for i, r in enumerate(rows):
        m = ROW.match(cells.text_of(r))
        if m:
            out.append(((int(m.group(1)), m.group(2)), m.group(3), lines[i], r))
    return out


def normalized(who, scenario, snap):
    rules = RULES["fx" if who == "fx" else "xfx"]
    return [cells.normalize_row(r, rules) for r in cells.parse(open(path(who, scenario, snap), encoding="utf-8").read())]


def claim(name, ok, lines):
    results.append(ok)
    print(("ok   " if ok else "FAIL ") + name)
    for line in lines:
        print("       " + line)


for scenario, spec in SPEC.items():
    a, b = diff_rows("fx", scenario), diff_rows(side, scenario)
    lines, ok = [], [k for k, *_ in a] == spec["rows"] == [k for k, *_ in b]
    if not ok:
        lines.append("FAIL the diff rows are fx %s / %s %s" % ([k for k, *_ in a], side, [k for k, *_ in b]))
    for (k, tx, x, cx), (_, ty, y, cy) in zip(a, b):
        if k in spec["apart"]:
            want_fx, want_side = spec["apart"][k]
            told = tx == want_fx and ty == want_side
            ok &= told
            lines.append("%d %s  apart: fx ends %r / %s ends %r%s" % (k[0], k[1], tx[-8:], side, ty[-8:], "" if told else
                         ": FAIL, not fx %r / %s %r" % (want_fx[-8:], side, want_side[-8:])))
        elif k in spec["named"]:
            # fx's cells are xfx's with GAP blank, unstyled cells before
            # `before`; every other cell the same, attributes included.
            before = spec["named"][k]
            at = next((i for i in range(len(cy)) if cells.text_of(cy[i:]).startswith(before)), None)
            told = at is not None and [(c, sh) for _, c, sh in cx] == (
                [(c, sh) for _, c, sh in cy[:at]] + [(" ", ())] * GAP + [(c, sh) for _, c, sh in cy[at:]])
            ok &= told
            lines.append("%d %s  named: fx %r / %s %r -- fx's cells are %s's with %d unstyled blank cells before %r%s"
                         % (k[0], k[1], x[-60:], side, y[-60:], side, GAP, before, "" if told else ": FAIL, not so"))
        elif cx == cy:
            lines.append("%d %s  %r%s" % (k[0], k[1], x[-60:], "" if x == y else "  (%s's first bytes %r)" % (side, y[:12])))
        else:
            ok = False
            lines.append("FAIL %d %s: fx %r / %s %r" % (k[0], k[1], x[-60:], side, y[-60:]))
    claim("bytes %s full (fx / %s)" % (scenario, side), ok, lines)

    lines, ok = [], True
    for number, want in spec["margins"].items():
        for who, rows in (("fx", a), (side, b)):
            text = next((t for k, t, *_ in rows if k == (number, " ")), "")
            good = text == want
            ok &= good
            lines.append("%s line %d ends %r%s" % (who, number, text[-12:], "" if good else ", not %r" % want[-12:]))
    claim("margin %s full" % scenario, ok, lines)

    if spec["whole"]:
        lines, ok = [], True
        for number, want in spec["whole"].items():
            got = {}
            for who in ("fx", side):
                rows = [cells.text_of(r) for r in cells.parse(open(path(who, scenario, "full"), encoding="utf-8").read())]
                at = next(i for i, t in enumerate(rows) if (READ.match(t) or [None, None])[1] == str(number))
                text = READ.match(rows[at]).group(2)
                for t in rows[at + 1:]:
                    if not t.startswith("│  ") or READ.match(t):
                        break
                    text += t[3:]
                got[who] = text
            good = got[side] == want and got["fx"] != want
            ok &= good
            ops = difflib.SequenceMatcher(None, want, got["fx"], autojunk=False).get_opcodes()
            lost = "".join(want[i1:i2] for tag, i1, i2, _, _ in ops if tag in ("delete", "replace"))
            lines.append("line %d read whole: fx %r / %s %r -- %s's rows hold the line; fx's lose %r%s"
                         % (number, got["fx"][-14:], side, got[side][-14:], side, lost, "" if good else ": FAIL, not so"))
        claim("whole %s full" % scenario, ok, lines)

    fa, fb = normalized("fx", scenario, "full"), normalized(side, scenario, "full")
    key = lambda rows: [repr(r) for r in rows]  # noqa: E731
    ops = difflib.SequenceMatcher(None, key(fa), key(fb), autojunk=False).get_opcodes()
    only_fx = [cells.text_of(fa[i]) for tag, i1, i2, _, _ in ops if tag in ("delete", "replace") for i in range(i1, i2)]
    only_xfx = [cells.text_of(fb[j]) for tag, _, _, j1, j2 in ops if tag in ("insert", "replace") for j in range(j1, j2)]
    matched = sum(i2 - i1 for tag, i1, i2, _, _ in ops if tag == "equal")
    # The rows fx re-anchors -- the diff row and the read row of the line
    # with the variation selector -- are named; the rest must be the claim's.
    anchored = spec["anchored"]
    # The rows the ledger's `filler-modifier-count` covers, each for its
    # own side's text, stand apart too.
    apart = lambda who, t: bool(ledgermod.covering(APART, scenario, "full", 0, who, t))  # noqa: E731
    named_fx = [t for t in only_fx if t and not (anchored and anchored in t) and not apart("fx", t)]
    named_xfx = [t for t in only_xfx if t and not (anchored and anchored in t) and not apart("xfx", t)]
    # What may stand on one side only: fx's continuation rows, the named
    # rows, and the top rows fx's later window pushes out of xfx's view.
    ok = sorted(named_fx) == sorted(spec["only_fx"]) and len(named_xfx) <= len(spec["only_fx"]) and matched >= spec["matched"]
    claim("full %s (fx / %s), aligned" % (scenario, side), ok,
          ["%d rows the same; only fx: %r; only %s: %r" % (matched, only_fx, side, only_xfx)])

    for snap in ("edited", "closed"):
        ea, eb = normalized("fx", scenario, snap), normalized(side, scenario, snap)
        diff = [i + 1 for i in range(max(len(ea), len(eb))) if (ea[i] if i < len(ea) else []) != (eb[i] if i < len(eb) else [])]
        claim("inline %s %s" % (scenario, snap), not diff, ["rows %s differ" % diff] if diff else ["all %d rows the same" % len(ea)])
print("%d claims, %d hold, %d fail" % (len(results), sum(results), len(results) - sum(results)))
sys.exit(0 if all(results) else 1)
