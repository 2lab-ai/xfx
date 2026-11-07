"""ENGINE-VS-ANCHOR's checker: `p5-transcript-diffwide2` on fx v0.0.16 against
xfx, the one capture the ledger's `frame-variation-selector-anchor` named.

usage: python3 -I -B check.py <repo> <receipt-dir> [xfx|red]

Reads `fx/` and `xfx/` (the build under the receipt) or `red/` (the train
head before it), each `<prefix>-p5-transcript-diffwide2-40x120-<snap>.ansi`.

  anchor   line 2 of the file -- `half` U+FFA0 U+FE0E, then `x` -- as the full
           transcript writes it twice, its read row (`│  2    ...`) and its
           Ctrl-O diff row (`  │ 2   ...`). fx v0.0.16 writes `ESC[42G` after
           the pair and tmux leaves three unstyled cells blank before `x`.
           xfx: every cell the same as fx's, text and attributes. red: fx's
           cells are red's with those three blank cells before `x zwj`.
  aligned  the whole `full` screen, the two sides aligned as sequences (the
           ledger's normalizations applied). xfx: no row one side holds alone
           has a variation selector in it, and every such row that is not
           blank is one a ledger entry covers for its side (printed). red: the
           line's two rows are among those one side holds alone.
  inline   `edited` and `closed`: every row the same.

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

SCENARIO = "p5-transcript-diffwide2"
LEDGER = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
RULES = {"fx": ledgermod.rules_for(LEDGER, "fx"), "xfx": ledgermod.rules_for(LEDGER, "xfx")}
ENTRIES = LEDGER["divergences"] + LEDGER["causes"]
READ = re.compile(r"^│ +2 {4}")
DIFF = re.compile(r"^  │ 2 {3}")
SELECTOR = re.compile("[︎️]")
BEFORE = "x zwj"
GAP = 3
results = []


def path(who, snap):
    prefix = "fx" if who == "fx" else "xfx"
    unit = SCENARIO + "-40x120"
    return os.path.join(receipt, who, unit, "%s-%s-%s.ansi" % (prefix, unit, snap))


def parsed(who, snap):
    return cells.parse(open(path(who, snap), encoding="utf-8").read())


def normalized(who, snap):
    rules = RULES["fx" if who == "fx" else "xfx"]
    return [cells.normalize_row(r, rules) for r in parsed(who, snap)]


def claim(name, ok, lines):
    results.append(ok)
    print(("ok   " if ok else "FAIL ") + name)
    for line in lines:
        print("       " + line)


def line_two(rows, pattern):
    found = [r for r in rows if pattern.match(cells.text_of(r))]
    return found[0] if len(found) == 1 else None


fx_full, side_full = parsed("fx", "full"), parsed(side, "full")
lines, ok = [], True
for name, pattern in (("read row", READ), ("diff row", DIFF)):
    cx, cy = line_two(fx_full, pattern), line_two(side_full, pattern)
    if cx is None or cy is None:
        ok = False
        lines.append("FAIL %s: not one such row on each side" % name)
        continue
    fx_cells = [(c, sh) for _, c, sh in cx]
    side_cells = [(c, sh) for _, c, sh in cy]
    if side == "xfx":
        told = fx_cells == side_cells
        why = "every cell the same"
    else:
        at = next((i for i in range(len(cy)) if cells.text_of(cy[i:]).startswith(BEFORE)), None)
        told = at is not None and fx_cells == side_cells[:at] + [(" ", ())] * GAP + side_cells[at:]
        why = "fx's cells are red's with %d unstyled blank cells before %r" % (GAP, BEFORE)
    ok &= told
    lines.append("%s: fx %r / %s %r -- %s%s" % (name, cells.text_of(cx)[-44:], side, cells.text_of(cy)[-44:], why,
                                                "" if told else ": FAIL, not so"))
claim("anchor %s full (fx / %s)" % (SCENARIO, side), ok, lines)

fa, fb = normalized("fx", "full"), normalized(side, "full")
key = lambda rows: [repr(r) for r in rows]  # noqa: E731
ops = difflib.SequenceMatcher(None, key(fa), key(fb), autojunk=False).get_opcodes()
only = [("fx", i, cells.text_of(fa[i])) for tag, i1, i2, _, _ in ops if tag in ("delete", "replace") for i in range(i1, i2)]
only += [(side, j, cells.text_of(fb[j])) for tag, _, _, j1, j2 in ops if tag in ("insert", "replace") for j in range(j1, j2)]
matched = sum(i2 - i1 for tag, i1, i2, _, _ in ops if tag == "equal")
lines, ok = ["%d rows the same; %d rows one side holds alone" % (matched, len(only))], True
anchored = [t for _, _, t in only if SELECTOR.search(t)]
if side == "xfx":
    if anchored:
        ok = False
        lines.append("FAIL rows with a variation selector one side holds alone: %r" % anchored)
    for who, i, text in only:
        if not text.strip():
            lines.append("%s row %d: blank" % (who, i + 1))
            continue
        ledger_side = "fx" if who == "fx" else "xfx"
        hit = [d["id"] for d in ledgermod.covering(ENTRIES, SCENARIO, "full", i + 1, ledger_side, text)]
        ok &= bool(hit)
        lines.append("%s row %d %r: %s" % (who, i + 1, text[-40:], ", ".join(hit) if hit else "FAIL, no entry covers it"))
else:
    told = len(anchored) == 4
    ok &= told
    lines.append("the line's read and diff rows, on each side, among them: %d of 4%s" % (len(anchored), "" if told else ": FAIL"))
claim("aligned %s full (fx / %s)" % (SCENARIO, side), ok, lines)

for snap in ("edited", "closed"):
    ea, eb = normalized("fx", snap), normalized(side, snap)
    diff = [i + 1 for i in range(max(len(ea), len(eb))) if (ea[i] if i < len(ea) else []) != (eb[i] if i < len(eb) else [])]
    claim("inline %s %s" % (SCENARIO, snap), not diff, ["rows %s differ" % diff] if diff else ["all %d rows the same" % len(ea)])
print("%d claims, %d hold, %d fail" % (len(results), sum(results), len(results) - sum(results)))
sys.exit(0 if all(results) else 1)
