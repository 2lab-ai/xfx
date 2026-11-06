"""One fx/xfx snap pair, compared cell by cell after the parity ledger's
normalizations, with the rows two named [[cause]] entries explain set aside.

usage: [PAGE_OFFSET=k] python3 -I -B pair.py <repo> <fx.ansi> <xfx.ansi> [cause-id ...]

Prints: the positional verdict (every row, normalized), then the sequence
alignment (ledger.aligned) and, for every row left without a match, the
cause that covers it on its own side or UNEXPLAINED. Exit 0 when no row is
UNEXPLAINED.
"""

import os
import re
import sys

repo = os.path.realpath(sys.argv[1])
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

fx_path, xfx_path = sys.argv[2], sys.argv[3]
cause_ids = sys.argv[4:]
led = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
fx_rules = ledgermod.rules_for(led, "fx")
xfx_rules = ledgermod.rules_for(led, "xfx")

causes = []
toml_text = open(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), encoding="utf-8").read()
for block in toml_text.split("[[cause]]")[1:]:
    m = re.search(r'^id = "([^"]+)"', block, re.M)
    if not m or m.group(1) not in cause_ids:
        continue
    fx_row = re.search(r"^fx_row = '(.*)'$", block, re.M)
    xfx_row = re.search(r"^xfx_row = '(.*)'$", block, re.M)
    causes.append(
        (
            m.group(1),
            re.compile(fx_row.group(1)) if fx_row else None,
            re.compile(xfx_row.group(1)) if xfx_row else None,
        )
    )
missing = set(cause_ids) - {c[0] for c in causes}
assert not missing, missing


def load(path, rules):
    rows = cells.parse(open(path, encoding="utf-8").read())
    return [cells.normalize_row(r, rules) for r in rows]


fx = load(fx_path, fx_rules)
xfx = load(xfx_path, xfx_rules)
text = cells.text_of

positional = [i + 1 for i in range(max(len(fx), len(xfx))) if (fx[i] if i < len(fx) else []) != (xfx[i] if i < len(xfx) else [])]
print("positional: %d of %d rows differ%s" % (len(positional), max(len(fx), len(xfx)), (" " + str(positional)) if positional else ""))

left = ledgermod.aligned(fx, xfx, text)


def viewport_end(rows):
    """The last row of the record on a Ctrl-O screen: the one above the
    footer's blank row, or every row when there is no footer."""
    for i, r in enumerate(rows):
        if text(r).startswith("\u2503 full detail"):
            return i - 1
    return len(rows)


# A page that sits a few rows apart on the two sides (rows below the window
# that only one side has move every window above them): the run of unmatched
# rows at the top of one side and at the bottom of the other's record.
# PAGE_OFFSET=k: at most k such rows at each edge of each side -- the number
# of rows below the window that one side has and the other has not.
unmatched = {(side, row) for _, side, row, _ in left}
limit = int(os.environ.get("PAGE_OFFSET", "0"))
edge = set()
for side, rows in (("fx", fx), ("xfx", xfx)):
    top = 0
    while (side, top + 1) in unmatched:
        top += 1
    if top <= limit:
        edge |= {(side, r) for r in range(1, top + 1)}
    end = viewport_end(rows)
    bottom = end
    while bottom > top and (side, bottom) in unmatched:
        bottom -= 1
    if end - bottom <= limit:
        edge |= {(side, r) for r in range(bottom + 1, end + 1)}
unexplained = 0
for block, side, row, row_text in left:
    covered = None
    for cid, fx_re, xfx_re in causes:
        pattern = fx_re if side == "fx" else xfx_re
        if pattern is not None and pattern.search(row_text):
            covered = cid
            break
    if covered is None and row_text == "":
        rows = fx if side == "fx" else xfx
        rest = [text(r) for r in rows[row:]]
        footer = next((i for i, t in enumerate(rest) if t), None)
        if footer is not None and rest[footer].startswith("\u2503 full detail"):
            # The screen's blank rows above its footer: a page whose record
            # is shorter by the covered rows has that many more of them.
            covered = "(viewport padding)"
        elif [t for b, s, _, t in left if b == block and s == side and t]:
            covered = "(blank row beside a covered run)"
    if covered is None and (side, row) in edge:
        covered = "(page offset)"
    if covered is None:
        unexplained += 1
    print("  %-3s row %2d  %-28s %s" % (side, row, covered or "UNEXPLAINED", row_text))
print("aligned: %d rows without a match, %d unexplained" % (len(left), unexplained))
sys.exit(1 if unexplained else 0)
