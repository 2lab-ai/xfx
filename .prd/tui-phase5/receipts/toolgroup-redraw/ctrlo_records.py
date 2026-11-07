"""Every row `truecolor-grey-notation` names by pattern on the Ctrl-O pages
of `p5-toolgroup-ctrlo-tc` -- an entry's heading, the session and network
records, the prompt, the group's header and call, a command's output rail,
the turn's summary, the footer and the statusline -- compared between fx and
xfx cell by cell, a direct grey read as the index it spells (notation.py's
ramp).

usage: python3 -I ctrlo_records.py <cells.py dir> <fx.ansi> <xfx.ansi>

Each fx row of those shapes is paired with the xfx row of the same text in
the same order, the clock (`YYYY-MM-DD hh:mm:ss.mmm`) and a network record's
latency (`· Nms`) masked on both sides first. A paired row's cells are
compared character and attributes alike, every attribute kept (bold, the
background, ...); a masked cell by its attributes alone. A row of those
shapes on one side with no row of its text on the other is listed as such:
its text differs between the products (`tool-schema`, ...), so its colours
are not compared here.
Exit 1 if a paired row differs.
"""
import re
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import cells  # noqa: E402

# `truecolor-grey-notation`'s fx_row/xfx_row (parity-ledger.toml), each
# alternative as it reads before the sweep masks the clock (`X+`) and the
# turn's wall time (`<wall>`).
SHAPES = re.compile(
    r"^┃|^  \d[\dhms ]*s \(|^(?:auto|full access) · model-a$|^● |^[├└] |^[▪■] Cancelled |^~ Running \("
    r"|^  \d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} [A-Z]+ · |^\* (?:session|network): |^  (?:finish|tokens|mcp): |^│ "
)
MASKS = (re.compile(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}"), re.compile(r"(?<=· )\d+(?=ms$)"))


def ramp(v):
    if isinstance(v, tuple) and len(v) == 3 and v[0] == v[1] == v[2] and (v[0] - 8) % 10 == 0 and 0 <= (v[0] - 8) // 10 <= 23:
        return 232 + (v[0] - 8) // 10
    return v


def attrs(shown):
    return tuple((k, ramp(v)) if k in ("fg", "bg") else (k, v) for k, v in shown)


def segments(row):
    """The row as runs: ("text", cells) for what is compared cell by cell and
    ("masked", cells) for a clock or a latency, whose characters differ from
    run to run; and the row's key, each masked run written `#`."""
    text = "".join(c[1] for c in row).rstrip()
    spans = sorted((m.start(), m.end()) for rx in MASKS for m in rx.finditer(text))
    runs, key, at = [], "", 0
    for start, end in spans:
        runs.append(("text", row[at:start]))
        runs.append(("masked", row[start:end]))
        key += text[at:start] + "#"
        at = end
    runs.append(("text", [c for c in row[at:] if c[1] != " " or c[2]]))
    key += text[at:]
    return key, runs


def differs(a_runs, b_runs):
    """The first few cells that differ, comparing run by run: a text run cell
    by cell, character and every attribute; a masked run by the attributes
    its cells carry."""
    out = []
    for (kind, a), (_, b) in zip(a_runs, b_runs):
        if kind == "masked":
            if {attrs(c[2]) for c in a} != {attrs(c[2]) for c in b}:
                out.append(("masked", a[:1], b[:1]))
            continue
        for x, y in zip(a, b):
            if x[1] != y[1] or attrs(x[2]) != attrs(y[2]):
                out.append((x, y))
        if len(a) != len(b):
            out.append(("length", len(a), len(b)))
    return out[:3]


fx = cells.parse(open(sys.argv[2], encoding="utf-8").read())
xf = cells.parse(open(sys.argv[3], encoding="utf-8").read())
pool = {}
for row in xf:
    key, runs = segments(row)
    pool.setdefault(key, []).append(("".join(c[1] for c in row).rstrip(), runs))
bad = 0
for row in fx:
    text = "".join(c[1] for c in row).rstrip()
    if not SHAPES.search(text):
        continue
    key, runs = segments(row)
    candidates = pool.get(key) or []
    if not candidates:
        print("no xfx row of this text  %r" % key)
        continue
    diff = differs(runs, candidates.pop(0)[1])
    if diff:
        bad += 1
        print("DIFFERS  %r: %s" % (key, diff))
    else:
        print("same     %r" % key)
for key, left in pool.items():
    for text, _ in left:
        if SHAPES.search(text):
            print("no fx row of this text   %r" % key)
sys.exit(1 if bad else 0)
