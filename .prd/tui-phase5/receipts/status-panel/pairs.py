"""The status-panel receipt's reading of an fx/xfx snap pair.

usage: python3 -I pairs.py <fx.ansi> <xfx.ansi>

Prints two counts, each over every row but the banner:

  sweep     the rows that differ cell by cell once the parity ledger's
            normalizations are applied to each side
            (`scripts/fxcap/parity-ledger.toml`, read by `ledger.py` as the
            sweep reads it: the run directory, the product name, the turn's
            clock and the activity row's pulse)
  readings  the rows that still differ once this unit's recorded
            differences are read too, each by its id:
              updates     fx's `Updates` row, which xfx omits
              sandbox     xfx's `Sandbox` row, which fx has not
              session-id  the Session row's id: minted per run, and xfx's is
                          `<ms>-<16 hex>` (`docs/parity.md`, sessions)
              step-limit  the Step limit row's value: xfx compiles in 25
                          where fx compiles in unbounded (UPSTREAM.md:92)
              build       the Build row's value: each product's own build
              auth-help   the Auth help row and the rows it wraps onto:
                          fx's points at `/login`, the gated sign-in
            A dropped row takes a blank one from the bottom of the screen,
            so the rows under it are compared where they now stand.

A banner the screen wrapped onto two rows (xfx's debug banner at 50 columns)
is still the banner: both of its rows are left out, and the rows under it are
compared with fx's under its one.

Exit status 0 when `readings` is 0. Each remaining row is printed with both
texts, so a defect names itself.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", "..", ".."))
sys.path.insert(0, os.path.join(REPO, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

BANNER = {
    "fx": re.compile(r"^𝒇x v\d+\.\d+\.\d+ · Run /help for commands$"),
    "xfx": re.compile(r"^x𝒇x v\S+(?: \[[a-z]+\])? · Run /help for commands$"),
}

# The panel's value column on every screen these scenarios use: two columns of
# indent, `Session grants`, four of gap.
VALUE = 20


def label(name):
    return r"^  %s {%d}" % (re.escape(name), VALUE - 2 - len(name))


DROPPED = {
    "fx": [("updates", re.compile(label("Updates") + r"\S"))],
    "xfx": [("sandbox", re.compile(label("Sandbox") + r"\S"))],
}
WRAPPED = [("auth-help", re.compile(label("Auth help") + r"\S"))]
CONTINUATION = re.compile(r"^ {%d}\S" % VALUE)

# The ledger's `session-id` rule already writes xfx's id `<session-id>`
# wherever it stands; fx's twelve characters it reads only after `--resume `.
MASKED = {
    "fx": [
        ("session-id", re.compile(r"(?<=%s)[A-Za-z0-9_-]{12}(?= · )" % label("Session")), "<session-id>"),
    ],
    "xfx": [
        ("session-id", re.compile(r"(?<=%s)<session-id>(?= · )" % label("Session")), "<session-id>"),
    ],
}
BOTH_MASKED = [
    ("step-limit", re.compile(r"(?<=%s)(?:unlimited|\d+)$" % label("Step limit")), "<steps>"),
    ("build", re.compile(r"(?<=%s).+$" % label("Build")), "<build>"),
]


def unbannered(rows, side):
    """`rows` without the banner, on one row or wrapped onto two."""
    first = cells.text_of(rows[0]) if rows else ""
    if BANNER[side].match(first):
        return rows[1:], 1
    if len(rows) > 1 and BANNER[side].match(first + " " + cells.text_of(rows[1])):
        return rows[2:], 2
    return rows, 0


def read(rows, side, used):
    out = []
    skipping = False
    for row in rows:
        text = cells.text_of(row)
        if skipping and CONTINUATION.match(text):
            continue
        skipping = False
        hit = next((i for i, p in DROPPED[side] + WRAPPED if p.match(text)), None)
        if hit:
            used.add(hit)
            skipping = hit == "auth-help"
            continue
        out.append(row)
    rules = []
    for rid, pattern, replace in MASKED[side] + BOTH_MASKED:
        # A masked value's cells carry no attributes: the two values are
        # different texts, so cell k of one has no cell k on the other to
        # take its colour from. The label before it is compared as painted.
        rules.append({"pattern": pattern, "replace": replace, "mask": None, "attrs": "none", "absorb": False})
    masked = []
    for row in out:
        before = cells.text_of(row)
        after = cells.normalize_row(row, rules)
        if cells.text_of(after) != before:
            for rid, pattern, _ in MASKED[side] + BOTH_MASKED:
                if pattern.search(before):
                    used.add(rid)
        elif side == "xfx" and "<session-id>" in before:
            used.add("session-id")
        masked.append(after)
    return masked


def padded(rows, n):
    return rows + [[] for _ in range(n - len(rows))]


def main():
    fx_path, xfx_path = sys.argv[1], sys.argv[2]
    led = ledgermod.load(os.path.join(REPO, "scripts", "fxcap", "parity-ledger.toml"))
    rules = {side: ledgermod.rules_for(led, side) for side in ("fx", "xfx")}
    raw = {
        "fx": cells.parse(open(fx_path, encoding="utf-8").read()),
        "xfx": cells.parse(open(xfx_path, encoding="utf-8").read()),
    }
    body, banner = {}, {}
    for side in ("fx", "xfx"):
        body[side], banner[side] = unbannered(raw[side], side)
        body[side] = cells.normalize(body[side], rules[side])
    n = max(len(body["fx"]), len(body["xfx"]))
    swept = cells.differing(padded(body["fx"], n), padded(body["xfx"], n))
    used = set()
    read_rows = {side: read(body[side], side, used) for side in ("fx", "xfx")}
    left = cells.differing(padded(read_rows["fx"], n), padded(read_rows["xfx"], n))
    for i, a, b, at, attrs_only in left:
        print("row %d: fx  %r" % (i + 1 + banner["fx"], a))
        print("row %d: xfx %r%s" % (i + 1 + banner["xfx"], b, " (attributes only)" if attrs_only else ""))
    wrapped = " (xfx's banner on 2 rows)" if banner["xfx"] == 2 else ""
    print(
        "sweep %d · readings %d [%s]%s"
        % (len(swept), len(left), ",".join(sorted(used)) or "-", wrapped)
    )
    return 1 if left else 0


if __name__ == "__main__":
    sys.exit(main())
