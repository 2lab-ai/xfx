"""Compares two `capture-pane -e` files cell by cell, row 1 (each product's
banner) left out.

usage: python3 -I cells.py <fx.ansi> <xfx.ansi>

Every printed character is compared with its column and the attributes in
force on it, the SGR state carried from row to row as tmux writes it. Blank
cells past a row's last printed character are not compared: tmux trims them,
and a space's foreground and weight do not show -- so where tmux happens to
put the reset that ends a row's run (on that row, or in front of the next)
is not a difference. A space under a background, reverse or underline is
compared like any other character. `/.fx/` in the fx file is read as
`/.xfx/` -- `~/.fx/mcp.json` and `~/.xfx/mcp.json` are each product's
profile -- the same rule as the banner.

That command line is the M1 receipt's (`.prd/tui-phase4/receipts/m1/`), and
its output is unchanged: the receipt's copy of this file runs this one.

The parity sweep (`sweep.py`) uses the same reading as a library, with the
normalizations of `parity-ledger.toml` in place of the two fixed rules above:

  parse(text)            the rows of a capture, each a list of cells
                         (col, ch, shown), every row compared -- row 1 too
  normalize(rows, rules) the rows with each rule's regex applied to a row's
                         printed text, the columns counted again after it
                         (`absorb="after"|"before"`: a widening rule's extra
                         columns taken out of the padding beside it)
  differing(fx, xfx)     the rows that differ, with their texts and the first
                         cell that differs
  unaligned(fx, xfx)     how many rows differ once the two are aligned as
                         sequences, a reading aid beside the positional count

A rule is a dict: `pattern` (a compiled regex), `replace` (a string, or None
to mask), `mask` (the character a masked match is written with) and `attrs`
(`"keep"`: each written cell takes the attributes of the cell it replaces, the
match's last cell for any past its end; `"none"`: no attributes, so a banner's
colours are not compared either). With `wrapped`, `normalize` given the
screen's width first joins the rows a screen that narrow wrapped one such line
onto (`join_wrapped`); without a width it joins nothing.
"""

import re
import sys
import unicodedata

SGR = re.compile(r"\x1b\[([0-9;]*)m")
OTHER = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[A-Za-z]")
SHOWS_ON_A_SPACE = ("bg", "reverse", "underline")


def apply(state, params):
    codes = [int(p) if p else 0 for p in params.split(";")] if params else [0]
    i = 0
    while i < len(codes):
        c = codes[i]
        if c == 0:
            state.clear()
        elif c in (38, 48) and i + 2 < len(codes) and codes[i + 1] == 5:
            state["fg" if c == 38 else "bg"] = codes[i + 2]
            i += 2
        elif c in (38, 48) and i + 4 < len(codes) and codes[i + 1] == 2:
            state["fg" if c == 38 else "bg"] = tuple(codes[i + 2 : i + 5])
            i += 4
        elif c == 39:
            state.pop("fg", None)
        elif c == 49:
            state.pop("bg", None)
        elif c == 22:
            state.pop("bold", None)
            state.pop("dim", None)
        elif c in (23, 24, 27):
            state.pop({23: "italic", 24: "underline", 27: "reverse"}[c], None)
        elif c in (1, 2, 3, 4, 7):
            state[{1: "bold", 2: "dim", 3: "italic", 4: "underline", 7: "reverse"}[c]] = True
        elif 30 <= c <= 37 or 90 <= c <= 97:
            state["fg"] = ("ansi", c)
        elif 40 <= c <= 47 or 100 <= c <= 107:
            state["bg"] = ("ansi", c)
        else:
            state["sgr%d" % c] = True
        i += 1


def width(ch):
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in "WF" else 1


def _shown(ch, shown):
    """What a cell shows: a space's attributes only where they show on one."""
    if ch == " " and not any(k for k, _ in shown if k in SHOWS_ON_A_SPACE):
        return ()
    return shown


def _trim(cells):
    while cells and cells[-1][1] == " " and cells[-1][2] == ():
        cells.pop()
    return cells


def parse(text):
    """Every row of a `capture-pane -e` text, as lists of (col, ch, shown)."""
    state, out = {}, []
    for line in text.split("\n"):
        cells, col, pos = [], 0, 0
        while pos < len(line):
            m = SGR.match(line, pos)
            if m:
                apply(state, m.group(1))
                pos = m.end()
                continue
            m = OTHER.match(line, pos)
            if m:
                pos = m.end()
                continue
            ch = line[pos]
            pos += 1
            shown = tuple(sorted(state.items(), key=str))
            if ch == " " and not any(k in state for k in SHOWS_ON_A_SPACE):
                shown = ()
            cells.append((col, ch, shown))
            col += width(ch)
        out.append(_trim(cells))
    return out


def rows(path, product_dir):
    text = open(path, encoding="utf-8").read()
    if product_dir:
        text = text.replace("/.fx/", "/.xfx/")
    return parse(text)


def text_of(cells):
    return "".join(ch for _, ch, _ in cells)


def _recount(pairs):
    """(ch, shown) pairs back into cells, each column counted again."""
    cells, col = [], 0
    for ch, shown in pairs:
        cells.append((col, ch, _shown(ch, shown)))
        col += width(ch)
    return _trim(cells)


def normalize_row(cells, rules, absorb=None):
    """One row with every rule applied in order. With `absorb` ("after" or
    "before"), a rule that says `absorb` and writes wider than it matched
    takes the columns it adds out of the nearest run of two or more blank
    cells after it (`/usage  show local fx tokens …  Account`: a padded
    column stands where it stood) or before it (a right-aligned warning ends
    where it ended); without, what follows moves right."""
    pairs = [(ch, shown) for _, ch, shown in cells]
    for rule in rules:
        text = "".join(ch for ch, _ in pairs)
        out, at, grown = [], 0, []
        for m in rule["pattern"].finditer(text):
            start, end = m.span()
            if start == end:
                continue
            out.extend(pairs[at:start])
            if rule.get("replace") is None:
                written = rule.get("mask", "X") * (end - start)
            else:
                written = m.expand(rule["replace"])
            for k, ch in enumerate(written):
                shown = () if rule.get("attrs") == "none" else pairs[min(start + k, end - 1)][1]
                out.append((ch, shown))
            if len(written) > end - start:
                grown.append((len(out), len(written) - (end - start)))
            at = end
        out.extend(pairs[at:])
        if absorb and rule.get("absorb"):
            for after, extra in reversed(grown):
                _take_padding(out, after, extra, absorb)
        pairs = out
    return _recount(pairs)


def _take_padding(pairs, after, extra, direction):
    """Deletes `extra` blank cells from the nearest run of more than `extra`
    of them after index `after` (or, "before", ending before the written
    text, which starts `extra` or more cells before `after`), if there is one."""
    runs, i = [], 0
    while i < len(pairs):
        if pairs[i] == (" ", ()):
            j = i
            while j < len(pairs) and pairs[j] == (" ", ()):
                j += 1
            if j - i > extra:
                runs.append((i, j))
            i = j
        else:
            i += 1
    if direction == "after":
        found = next((r for r in runs if r[0] >= after), None)
    else:
        found = next((r for r in reversed(runs) if r[1] <= after - extra - 1), None)
    if found:
        del pairs[found[0] : found[0] + extra]


def row_width(cells):
    """The columns a row's printed cells take, its trimmed blanks aside."""
    return cells[-1][0] + width(cells[-1][1]) if cells else 0


def _wraps_to(part, gap, cols):
    """Whether a screen `cols` wide, wrapping one line, leaves exactly these
    rows: no row wider than the screen, and at each break the row could not
    have held more. A word wrap (`gap` a space) breaks where the next word
    would not fit: the row, the space and the next row's first word are wider
    than the screen. A word longer than a row (`gap` empty) breaks inside it,
    so the row it breaks is full."""
    if any(row_width(row) > cols for row in part):
        return False
    for row, after in zip(part, part[1:]):
        if gap:
            word = text_of(after).split(" ", 1)[0]
            if row_width(row) + 1 + sum(width(ch) for ch in word) <= cols:
                return False
        elif row_width(row) != cols:
            return False
    return True


def join_wrapped(all_rows, pattern, cols, most=3):
    """The rows with every line a screen `cols` wide wrapped onto two or
    three rows read back as one row, and the rows under it moved up into the
    places they would hold had the line fit.

    Rows join only when the screen wrapped them: the joined line is wider
    than the screen, it is a whole match of `pattern` (`fullmatch`) where the
    first row alone is not, and each break is where the wrap puts one
    (`_wraps_to`). A word wrap takes away the space it breaks on, so the rows
    are joined with one; a word too long for the row breaks inside it, with
    nothing taken, so the rows are tried with none too. A line that fits the
    screen is never wrapped, so the same text on two rows of a wide screen --
    a hard line break -- stays two rows, as does a row that merely starts like
    the line or two rows that make a longer one."""
    out, i = [], 0
    while i < len(all_rows):
        row, joined = all_rows[i], None
        if row and not pattern.fullmatch(text_of(row)):
            for k in range(2, most + 1):
                part = all_rows[i : i + k]
                if len(part) < k or not all(part):
                    break
                for gap in (" ", ""):
                    pairs = []
                    for n, cells in enumerate(part):
                        if n and gap:
                            pairs.append((gap, ()))
                        pairs.extend((ch, shown) for _, ch, shown in cells)
                    line = _recount(pairs)
                    if (
                        row_width(line) > cols
                        and pattern.fullmatch(text_of(line))
                        and _wraps_to(part, gap, cols)
                    ):
                        joined = (k, line)
                        break
                if joined:
                    break
        if joined:
            out.append(joined[1])
            i += joined[0]
        else:
            out.append(row)
            i += 1
    return out


def normalize(all_rows, rules, absorb=None, cols=None):
    """`normalize_row` over every row; with `cols`, the screen's width, a
    `wrapped` rule's line wrapped onto several rows is first read as one."""
    if cols:
        for rule in rules:
            if rule.get("wrapped"):
                all_rows = join_wrapped(all_rows, rule["pattern"], cols)
    return [normalize_row(cells, rules, absorb) for cells in all_rows]


def differing(fx, xfx, fx_readings=()):
    """[(row index, fx text, xfx text, first differing cell index, attrs only)].

    `fx_readings` are fx's rows normalized another way (`absorb`): a row
    equal to xfx's in any reading of fx is the same."""
    found = []
    for i in range(max(len(fx), len(xfx))):
        a = fx[i] if i < len(fx) else []
        b = xfx[i] if i < len(xfx) else []
        if any(i < len(r) and r[i] == b for r in fx_readings):
            continue
        if a != b:
            at = next((n for n, (p, q) in enumerate(zip(a, b)) if p != q), min(len(a), len(b)))
            found.append((i, text_of(a), text_of(b), at, text_of(a) == text_of(b)))
    return found


def unaligned(fx, xfx):
    """How many rows of each side have no equal row on the other once the
    two are aligned as sequences (difflib): a row one side has and the other
    has not moves every row under it, so a snap can differ in twenty rows by
    position and in one by alignment. A reading aid for the diffs; the
    verdict is positional."""
    import difflib

    key = lambda rows: [tuple(r) for r in rows]  # noqa: E731
    m = difflib.SequenceMatcher(None, key(fx), key(xfx), autojunk=False)
    same = sum(b.size for b in m.get_matching_blocks())
    return len(fx) - same, len(xfx) - same


def main():
    fx, xfx = rows(sys.argv[1], True), rows(sys.argv[2], False)
    differ = 0
    for i in range(1, max(len(fx), len(xfx))):
        a = fx[i] if i < len(fx) else []
        b = xfx[i] if i < len(xfx) else []
        if a != b:
            differ += 1
            at = next((n for n, (p, q) in enumerate(zip(a, b)) if p != q), min(len(a), len(b)))
            print("row %d: fx %r" % (i + 1, a[at : at + 4]))
            print("row %d: xfx %r" % (i + 1, b[at : at + 4]))
    print("%d rows differ in their cells (row 1 not compared)" % differ)
    return 1 if differ else 0


if __name__ == "__main__":
    sys.exit(main())
