"""WIDTH-DISPUTED's checker: `p5-transcript-diffwide` on fx v0.0.16 against xfx.

usage: python3 -I -B check.py <repo> <receipt-dir> [xfx|red]

Reads `fx/` and `xfx/` (the build under the receipt) or `red/` (the base's),
each `<prefix>-p5-transcript-diffwide-40x120-<snap>.ansi`. The claims:

  bytes   every Ctrl-O diff row (`  │ N ± text`) of `full` is the same bytes
          on both sides, as the capture holds them (`capture-pane -e`);
  margin  line 3 -- 104 `w`s, four soft hyphens and `TAIL5678`, 116 cells by
          fx's count -- has a first row ending `TAIL` on both sides: 112
          characters, the screen's 120 columns less the gutter, nothing written
          past the margin;
  full    the whole `full` screen, the two sides aligned as sequences (the
          ledger's normalizations applied): every row the same but the rows
          the claim names, which are fx's alone -- fx reflows line 3's rest
          onto a row of its own (`  │     5678`) where xfx clips it (the
          ledger's cause `ctrl-o-diff-row-clipped`), so fx's window starts a
          row later; the rows only one side shows are printed;
  inline  `edited` and `closed`: every row the same, the ledger's
          normalizations applied (the banner).

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
UNIT = "p5-transcript-diffwide-40x120"
ROW = re.compile(r"^  │ (\d+) ([ +-]) (.*)$")
results = []


def path(who, snap):
    prefix = "fx" if who == "fx" else "xfx"
    return os.path.join(receipt, who, UNIT, "%s-%s-%s.ansi" % (prefix, UNIT, snap))


def diff_rows(who):
    lines = open(path(who, "full"), encoding="utf-8").read().split("\n")
    rows = cells.parse("\n".join(lines))
    out = []
    for i, r in enumerate(rows):
        m = ROW.match(cells.text_of(r))
        if m:
            out.append(((int(m.group(1)), m.group(2)), m.group(3), lines[i]))
    return out


def claim(name, ok, lines):
    results.append(ok)
    print(("ok   " if ok else "FAIL ") + name)
    for line in lines:
        print("       " + line)


a, b = diff_rows("fx"), diff_rows(side)
same = [k for (k, _, x), (_, _, y) in zip(a, b) if x == y]
bad = [(k, x, y) for (k, _, x), (_, _, y) in zip(a, b) if x != y]
ok = [k for k, _, _ in a] == [k for k, _, _ in b] and len(a) == 4 and not bad
claim("bytes %s full (fx / %s)" % (UNIT, side), ok,
      ["%d %s  %r" % (k[0], k[1], x) for k, _, x in a if k in same] + ["FAIL %d %s: fx %r / %s %r" % (k[0], k[1], x, side, y) for k, x, y in bad])
ends = {who: next((t for k, t, _ in rows if k == (3, " ")), "") for who, rows in (("fx", a), (side, b))}
claim("margin %s full line 3" % UNIT, all(t.endswith("­" * 4 + "TAIL") and len(t) == 112 for t in ends.values()),
      ["%s ends %r, %d characters (%d cells by fx's count)" % (who, t[-8:], len(t), len(t)) for who, t in ends.items()])
def normalized(who, snap):
    rules = RULES["fx" if who == "fx" else "xfx"]
    return [cells.normalize_row(r, rules) for r in cells.parse(open(path(who, snap), encoding="utf-8").read())]


import difflib  # noqa: E402

fa, fb = normalized("fx", "full"), normalized(side, "full")
key = lambda rows: [repr(r) for r in rows]  # noqa: E731
ops = difflib.SequenceMatcher(None, key(fa), key(fb), autojunk=False).get_opcodes()
only_fx = [cells.text_of(fa[i]) for tag, i1, i2, _, _ in ops if tag in ("delete", "replace") for i in range(i1, i2)]
only_xfx = [cells.text_of(fb[j]) for tag, _, _, j1, j2 in ops if tag in ("insert", "replace") for j in range(j1, j2)]
matched = sum(i2 - i1 for tag, i1, i2, _, _ in ops if tag == "equal")
# What may stand on one side only: fx's continuation row, the top row fx's
# later window pushes out of xfx's view, and the screen's last blank rows.
CONTINUATION = "  │     5678"
expect_fx = [CONTINUATION]
ok = [t for t in only_fx if t] == expect_fx and len([t for t in only_xfx if t]) <= 1 and matched >= 36
claim("full %s (fx / %s), aligned" % (UNIT, side), ok,
      ["%d rows the same; only fx: %r; only %s: %r" % (matched, only_fx, side, only_xfx)])
for snap in ("edited", "closed"):
    fa = [cells.normalize_row(r, RULES["fx"]) for r in cells.parse(open(path("fx", snap), encoding="utf-8").read())]
    fb = [cells.normalize_row(r, RULES["xfx"]) for r in cells.parse(open(path(side, snap), encoding="utf-8").read())]
    diff = [i + 1 for i in range(max(len(fa), len(fb))) if (fa[i] if i < len(fa) else []) != (fb[i] if i < len(fb) else [])]
    claim("inline %s %s" % (UNIT, snap), not diff, ["rows %s differ" % diff] if diff else ["all %d rows the same" % len(fa)])
print("%d claims, %d hold, %d fail" % (len(results), sum(results), len(results) - sum(results)))
sys.exit(0 if all(results) else 1)
