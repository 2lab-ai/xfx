"""T-U1+U2's receipt checker: every claim the receipt makes about a capture,
checked against the captures themselves, cell by cell with their styles.

usage: python3 -I -B check.py <repo> <captures> [--only <scenario>]

<captures> holds `fx/` and `xfx/` (and optionally `red/`) as the receipt
does: `<side>/<scenario>-<size>/<prefix>-<scenario>-<size>-<snap>.ansi`.

Each row of a capture is read as `scripts/fxcap/cells.py` reads it, then the
parity ledger's normalizations for that side are applied (`banner`,
`transcript-timestamp`, `network-ms`, ...). A Ctrl-O snap's record is its
rows above the footer's blank row (36 at 40 rows). Then two causes the
ledger already names are applied, and **only these**, each by an exact
pattern:

  tool-schema     fx's command entry has `│  action: run` and
                  `│  yield_time_ms: <n>` under its heading; xfx's
                  `terminal` has neither (parity-ledger.toml `tool-schema`).
                  Those two rows are taken out of fx's side, counted.
  session-record  fx's `* session:` record has `  system prompt: ready · <n>
                  bytes` and `  skills: loading`, which xfx's has not: taken
                  out of fx's side, counted. Its tool list differs in its
                  words on both sides: each side's list, from `  tools: <n>
                  advertised (` to the first row that ends `)`, is one row
                  `<tools>` (parity-ledger.toml `session-record`).

Nothing else is set aside: no row is excused for where it stands.

The claims (SPEC below):

  stitch  one document state, as windows a key apart (a page, `PgUp`: 36
          rows, less where a page up stopped at the document's top; an
          arrow, `Up`: 3), the last one the tail. Each side's windows
          are checked against each other where they overlap (an arrow's 33
          rows), then joined into the bottom of that side's document. The two
          documents, the causes applied, are compared from the bottom up:
          every row both cover must be the same, cells and styles. A page
          counted from the tail starts as many rows later on fx as fx has rows
          below it that xfx has not, so the rows only one side covers are at
          the top, and their count is reported; with an arrow window above a
          claimed page, every row of that page is covered on both sides.
  top     windows at the same row from the document's top on both sides
          (the reader at the records): compared from the top.
  hold    on each side, two snaps whose records are the same, cell by cell:
          the view held while the page rebuilt.
  insert  on each side, the second snap is the first with one `* network:`
          record put in at one row: the same above it, the first snap's rows
          pushed down under it (the view at the document's top, where the
          record lands inside it).
  row1    on each side, the same first row in two snaps (a width change).
  inside  a window found on each side, whole and once, inside that side's
          joined document of a stitch and within the rows both sides cover
          (so every row of it was compared), and how many rows later fx's starts
          in the document both share: one signed shift. A view pinned at
          another width must start the same number of rows later as the view
          it was pinned from -- the same rows of the same entry.
  inline  an inline snap: every row the same (no causes apply).

Exit 0 when every claim holds; the report says each claim's numbers.
"""

import glob
import os
import re
import sys

repo = os.path.realpath(sys.argv[1])
captures = os.path.realpath(sys.argv[2])
only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

LEDGER = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
RULES = {"fx": ledgermod.rules_for(LEDGER, "fx"), "xfx": ledgermod.rules_for(LEDGER, "xfx")}

FX_ONLY = {
    "tool-schema": re.compile(r"^│  (?:action: run|yield_time_ms: \d+)$"),
    "session-record": re.compile(r"^  (?:system prompt: ready · \d+ bytes|skills: loading)$"),
}
TOOLS_START = re.compile(r"^  tools: \d+ advertised \(")
FOOTER = "┃ full detail"
VIEWPORT = 36
ARROW = 3

text = cells.text_of


def path(side, scenario, size, snap):
    prefix = "fx" if side == "fx" else "xfx"
    found = os.path.join(captures, side, "%s-%s" % (scenario, size), "%s-%s-%s-%s.ansi" % (prefix, scenario, size, snap))
    if not os.path.exists(found):
        raise SystemExit("missing capture: %s" % os.path.relpath(found, captures))
    return found


def rows_of(side, scenario, size, snap):
    raw = cells.parse(open(path(side, scenario, size, snap), encoding="utf-8").read())
    rule_side = "fx" if side == "fx" else "xfx"
    return [cells.normalize_row(r, RULES[rule_side]) for r in raw]


def record(rows):
    """The rows above the footer's blank row."""
    for i, r in enumerate(rows):
        if text(r).startswith(FOOTER):
            return rows[: i - 1]
    raise ValueError("no Ctrl-O footer")


def causes_applied(rows, side):
    """The rows with the two causes applied, and what was taken out."""
    out, taken, i = [], {"tool-schema": 0, "session-record": 0, "tools-list": 0}, 0
    while i < len(rows):
        t = text(rows[i])
        if side == "fx":
            hit = next((k for k, p in FX_ONLY.items() if p.search(t)), None)
            if hit:
                taken[hit] += 1
                i += 1
                continue
        if TOOLS_START.search(t):
            j = i
            while j < len(rows) and not text(rows[j]).endswith(")"):
                j += 1
            taken["tools-list"] += j - i + 1
            out.append([(0, "<tools>", ())])
            i = j + 1
            continue
        out.append(rows[i])
        i += 1
    return out, taken


def first_difference(a, b):
    for (ca, cb) in zip(a, b):
        if ca != cb:
            return ca, cb
    return (a[len(b)] if len(a) > len(b) else None), (b[len(a)] if len(b) > len(a) else None)


class Claim:
    def __init__(self, name):
        self.name, self.lines, self.ok = name, [], True

    def say(self, line):
        self.lines.append(line)

    def fail(self, line):
        self.ok = False
        self.lines.append("FAIL " + line)


def joined(side, scenario, size, windows, claim):
    """One side's windows, top first, joined into the bottom of its
    document. `windows` is [(snap, step-to-the-next)], the last the tail."""
    recs = [record(rows_of(side, scenario, size, snap)) for snap, _ in windows]
    doc = []
    for k, ((snap, step), rec) in enumerate(zip(windows, recs)):
        if k == len(recs) - 1:
            # The tail: blank rows past the last row with text are the
            # screen's, when the whole document fits.
            body = list(rec)
            padded = 0
            while body and not text(body[-1]):
                body.pop()
                padded += 1
            if padded and k > 0:
                claim.fail("%s %s: the tail window ends in %d blank rows under windows above it" % (side, snap, padded))
            doc += body
            continue
        nxt = recs[k + 1]
        if step == "page" and text(rec[0]) == "<banner>":
            # At the document's top a page up stops at row 0, so this window
            # and the next can overlap: by as many rows as the longest run
            # its end and the next window's start share.
            overlap = max(n for n in range(0, VIEWPORT) if n == 0 or rec[VIEWPORT - n :] == nxt[:n])
            claim.say("%s %s: at the top; it overlaps the next window by %d rows" % (side, snap, overlap))
            doc += rec[: VIEWPORT - overlap]
        elif step == "page":
            doc += rec
        elif step == "arrow":
            if rec[ARROW:] != nxt[: VIEWPORT - ARROW]:
                claim.fail("%s %s: its last %d rows are not the next window's first" % (side, snap, VIEWPORT - ARROW))
            doc += rec[:ARROW]
        else:
            raise ValueError(step)
    return doc


def check_stitch(scenario, size, windows, expect):
    claim = Claim("stitch %s %s [%s]" % (scenario, size, " ".join(s for s, _ in windows)))
    docs = {}
    for side in ("fx", "xfx"):
        doc = joined(side, scenario, size, windows, claim)
        applied, taken = causes_applied(doc, side)
        docs[side] = applied
        claim.say("%-3s %3d rows joined; taken out: %s" % (side, len(doc), ", ".join("%s %d" % kv for kv in taken.items() if kv[1])))
        if side == "fx":
            for cause, want in expect.items():
                if taken[cause] != want:
                    claim.fail("fx: %d %s rows taken out, the claim is %d" % (taken[cause], cause, want))
    a, b = docs["fx"], docs["xfx"]
    n = min(len(a), len(b))
    if a[len(a) - n :] != b[len(b) - n :]:
        ca, cb = first_difference(list(reversed(a[len(a) - n :])), list(reversed(b[len(b) - n :])))
        claim.fail("bottom-up, fx %r / xfx %r" % (text(ca) if ca else None, text(cb) if cb else None))
    claim.say("compared bottom-up: %d rows the same; above them only fx covers %d, only xfx %d" % (n, len(a) - n, len(b) - n))
    docs["compared"] = n
    return claim, docs


def check_top(scenario, size, snaps):
    claim = Claim("top %s %s [%s]" % (scenario, size, " ".join(snaps)))
    for snap in snaps:
        a, ta = causes_applied(record(rows_of("fx", scenario, size, snap)), "fx")
        b, tb = causes_applied(record(rows_of("xfx", scenario, size, snap)), "xfx")
        # Both start at the same row of the document: what is on the screen
        # is compared from the top, as far as both go.
        n = min(len(a), len(b))
        same = a[:n] == b[:n]
        if not same:
            ca, cb = first_difference(a[:n], b[:n])
            claim.fail("%s: fx %r / xfx %r" % (snap, text(ca) if ca else None, text(cb) if cb else None))
        claim.say("%s: %d rows the same from the top (fx took out %d session-record rows)" % (snap, n if same else 0, ta["session-record"]))
    return claim


def check_hold(scenario, size, a_snap, b_snap):
    claim = Claim("hold %s %s %s == %s" % (scenario, size, a_snap, b_snap))
    for side in ("fx", "xfx"):
        a = record(rows_of(side, scenario, size, a_snap))
        b = record(rows_of(side, scenario, size, b_snap))
        if a != b:
            ca, cb = first_difference(a, b)
            claim.fail("%s: %r / %r" % (side, text(ca) if ca else None, text(cb) if cb else None))
        else:
            claim.say("%-3s all %d rows the same; row 1 %r" % (side, len(a), text(a[0])))
    return claim


def check_insert(scenario, size, a_snap, b_snap):
    """On each side, `b_snap`'s record is `a_snap`'s with one full-detail
    record put in at one row: the rows above it the same, then a `· Notice`
    header and its `* network:` row, then `a_snap`'s rows from there on,
    pushed down by the record's height."""
    claim = Claim("insert %s %s %s -> %s" % (scenario, size, a_snap, b_snap))
    for side in ("fx", "xfx"):
        a = record(rows_of(side, scenario, size, a_snap))
        b = record(rows_of(side, scenario, size, b_snap))
        k = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), None)
        if k is None:
            claim.fail("%s: nothing was put in" % side)
            continue
        if not (text(b[k]).endswith(" · Notice") and k + 1 < len(b) and text(b[k + 1]).startswith("* network: ")):
            claim.fail("%s: row %d is %r, not a record's header over `* network:`" % (side, k + 1, text(b[k])))
            continue
        m = next((m for m in range(2, len(b) - k) if b[k + m :] == a[k : len(a) - m]), None)
        if m is None:
            claim.fail("%s: the rows under the record are not the ones that were there" % side)
            continue
        claim.say("%-3s rows 1-%d the same; a %d-row record at row %d; the %d rows under it are the ones that were at row %d on" % (side, k, m, k + 1, len(b) - k - m, k + 1))
    return claim


def check_row1(scenario, size, a_snap, b_snap):
    claim = Claim("row1 %s %s %s, %s" % (scenario, size, a_snap, b_snap))
    for side in ("fx", "xfx"):
        a = record(rows_of(side, scenario, size, a_snap))[0]
        b = record(rows_of(side, scenario, size, b_snap))[0]
        if a != b:
            claim.fail("%s: %r / %r" % (side, text(a), text(b)))
        else:
            claim.say("%-3s row 1 %r in both" % (side, text(a)))
    return claim


def check_inside(scenario, size, snap, docs, group, shifts, same_as):
    claim = Claim("inside %s %s %s, in %s" % (scenario, size, snap, group))
    found = {}
    for side in ("fx", "xfx"):
        win, _ = causes_applied(record(rows_of(side, scenario, size, snap)), side)
        doc = docs[side]
        compared = docs["compared"]
        at = [i for i in range(len(doc) - len(win) + 1) if doc[i : i + len(win)] == win]
        if len(at) != 1:
            claim.fail("%s: the window is found %d times in its document" % (side, len(at)))
            return claim
        found[side] = len(doc) - at[0]
        claim.say("%-3s found whole, %d rows from the document's end" % (side, found[side]))
        if found[side] > compared:
            claim.fail("%s: the window reaches above the %d rows both sides cover" % (side, compared))
    # How many rows later fx's window starts in the document both sides
    # share, counted from the end with the causes applied: one signed shift.
    shift = found["xfx"] - found["fx"]
    shifts[snap] = shift
    if same_as is None:
        claim.say("fx's window starts %d rows later than xfx's" % shift)
    elif shift != shifts[same_as]:
        claim.fail("fx's window starts %d rows later than xfx's; at %s it was %d" % (shift, same_as, shifts[same_as]))
    else:
        claim.say("fx's window starts %d rows later than xfx's, as at %s: the same rows of the same entry" % (shift, same_as))
    return claim


def check_inline(scenario, size, snap):
    claim = Claim("inline %s %s %s" % (scenario, size, snap))
    a = rows_of("fx", scenario, size, snap)
    b = rows_of("xfx", scenario, size, snap)
    diff = [i + 1 for i in range(max(len(a), len(b))) if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]
    if diff:
        claim.fail("rows %s differ" % diff)
    else:
        claim.say("all %d rows the same" % len(a))
    return claim


PAGE, UP = "page", "arrow"
SCHEMA2 = {"tool-schema": 2}
BOTH = {"tool-schema": 2, "session-record": 2}
SPEC = {
    "p5-transcript-tools": [
        ("stitch", "40x120", "doc", [("full-running", None)], BOTH),
        ("hold", "40x120", "full-running", "full-pgup"),
        ("stitch", "40x80", "doc80", [("full-running", None)], BOTH),
        ("hold", "40x80", "full-running", "full-pgup"),
        ("inline", "40x120", "running"),
        ("inline", "40x80", "running"),
    ],
    "p5-transcript-anchormid": [
        ("stitch", "40x120", "running", [("full-pgup-up", UP), ("full-pgup", PAGE), ("full-running", None)], SCHEMA2),
        ("inside", "40x120", "full-pgup", "running", None),
        ("hold", "40x120", "full-pgup", "full-pgup-back"),
        ("hold", "40x120", "full-pgup", "full-pgup-after"),
        ("stitch", "40x120", "settled", [("full-end", None)], SCHEMA2),
        ("inline", "40x120", "running"),
        ("inline", "40x120", "end"),
    ],
    "p5-transcript-anchor": [
        ("stitch", "40x120", "running", [("full-pgup2", PAGE), ("full-pgup", PAGE), ("full-running", None)], BOTH),
        # At the top a `* network:` record lands inside the view, under the
        # ones there: the view holds above it, and pushes down under it.
        ("insert", "40x120", "full-pgup2", "full-pgup2-after"),
        ("inline", "40x120", "running"),
        ("inline", "40x120", "end"),
    ],
    "p5-transcript-resize": [
        ("stitch", "40x120", "running-120", [("full-pgup-up", UP), ("full-pgup", PAGE), ("full-running", None)], SCHEMA2),
        ("inside", "40x120", "full-pgup", "running-120", None),
        ("hold", "40x120", "full-pgup", "full-pgup-back"),
        ("row1", "40x120", "full-pgup", "full-pgup-80"),
        ("stitch", "40x120", "running-80", [("full-tail-80-up", UP), ("full-tail-80-pgup", PAGE), ("full-tail-80", None)], SCHEMA2),
        ("inside", "40x120", "full-pgup-80", "running-80", "full-pgup"),
        ("inside", "40x120", "full-tail-80-pgup", "running-80", None),
        ("row1", "40x120", "full-tail-80-pgup", "full-pgup-120"),
        ("stitch", "40x120", "running-120b", [("full-tail-120-up", UP), ("full-tail-120-pgup", PAGE), ("full-tail-120", None)], SCHEMA2),
        ("inside", "40x120", "full-pgup-120", "running-120b", "full-tail-80-pgup"),
        ("hold", "40x120", "full-tail-120-pgup", "full-pgup-after"),
    ],
    "p5-transcript-records": [
        ("top", "40x120", ["full-records", "full-records-80", "full-records-120"]),
        ("row1", "40x120", "full-records", "full-records-80"),
        ("row1", "40x120", "full-records", "full-records-120"),
    ],
    "p5-transcript-question": [
        ("stitch", "40x120", "120", [("full-120", None)], {"session-record": 2}),
        ("stitch", "40x120", "60", [("full-60", None)], {"session-record": 2}),
        ("inline", "40x120", "answered"),
    ],
}


def main():
    claims = []
    for scenario, items in SPEC.items():
        if only and scenario != only:
            continue
        docs, shifts = {}, {}
        for item in items:
            kind = item[0]
            if kind == "stitch":
                claim, joined_docs = check_stitch(scenario, item[1], item[3], item[4])
                docs[item[2]] = joined_docs
            elif kind == "top":
                claim = check_top(scenario, item[1], item[2])
            elif kind == "hold":
                claim = check_hold(scenario, item[1], item[2], item[3])
            elif kind == "insert":
                claim = check_insert(scenario, item[1], item[2], item[3])
            elif kind == "row1":
                claim = check_row1(scenario, item[1], item[2], item[3])
            elif kind == "inside":
                claim = check_inside(scenario, item[1], item[2], docs[item[3]], item[3], shifts, item[4])
            elif kind == "inline":
                claim = check_inline(scenario, item[1], item[2])
            claims.append(claim)
    bad = 0
    for claim in claims:
        print(("ok   " if claim.ok else "FAIL ") + claim.name)
        for line in claim.lines:
            print("       " + line)
        bad += not claim.ok
    print("%d claims, %d hold, %d fail" % (len(claims), len(claims) - bad, bad))
    sys.exit(1 if bad else 0)


main()
