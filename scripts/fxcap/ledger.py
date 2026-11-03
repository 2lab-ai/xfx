"""`parity-ledger.toml`: what the sweep may set aside, and how a snap pair is
classified against it.

The ledger has three bins:

  normalization  a fixed, mechanical rewrite of a row's text before the two
                 sides are compared (each product's banner, the product name);
                 a pair equal after them is normalized-identical. `absorb =
                 true` lets a rewrite that widens the text (`fx` -> `xfx`)
                 take the column it adds from the padding after it or before
                 it, so a padded or right-aligned column stays where it
                 stood; a row equal in any of the three readings is the same
  divergence     an approved difference, allowed only for a reason outside
                 xfx (`.prd/07-fx-parity-program.md`, Measurement: Ledger),
                 one per `class`:
                   user-decision         a decision that is the user's, made
                                         or waiting to be asked
                   unsafe-to-copy        a reproduced fx defect, or fx
                                         behaviour that cannot be copied
                                         safely
                   replaced-in-next-pin  an explicit interim decision for a
                                         surface the next pin replaces
                   platform-limit        a real platform limit
                 and each with an `owner` (a unit ID, or `user decision
                 <topic>`), a `resolution` (what ends it) and an `expires`
                 (`pin:<a [[pin]] name>` -- it lapses when the sweep's pin is
                 that one, so it may not also be one of its `pins` -- or
                 `date:YYYY-MM-DD`, after which it applies to nothing).
                 Review rounds or the work it would take are never a reason.
  anything else  an unresolved defect

A `[[cause]]` approves nothing: it says why rows that stay unresolved are
unresolved -- `kind` decision (awaiting the user), unit (owned by a unit in
flight), nondeterminism (the scenario waits on time; `fix` says how to make
it wait on content), scenario (the .cap itself is wrong; `fix`), defect
(`xfx` and `fx` name the two source lines) -- and who owns the fix. Same selectors; the first cause in the file that covers a
row is that row's.

Every divergence is scoped to the pins (`[[pin]]`, by sha256) it was recorded
against: a sweep against another fx applies none of them until they are
approved again for it.

A divergence's selector is `scenarios` and `snaps` (fnmatch globs) and at least
one row selector -- `rows` (row numbers, 1 is the top row, `"a-b"` a range),
`fx_row` / `xfx_row` (regexes searched in that side's normalized row text; a
row matches when either matches) -- unless it says `whole_snap = true`, which
the scoreboard lists apart.

A pair whose rows differ only under approved divergences by position is
approved-divergence (or replaced-in-next-pin). One whose rows differ by
position beyond them -- a row an approved one pushed down -- may still be
`approved-divergence-cascade` (`cascade`): with the two sides aligned as
sequences (trailing blank rows aside), every row left without a match is
covered by an approved divergence for its own side's text -- a blank row by
one that covers a row with text in the same unmatched run (the separator
beside an extra notice). A row pushed off the screen, a run of blank rows
alone, or any other row without a match keeps the pair an unresolved defect.
"""

import datetime
import difflib
import fnmatch
import os
import re
import tomllib

CLASSES = ("user-decision", "unsafe-to-copy", "replaced-in-next-pin", "platform-limit")
VERDICTS = (
    "identical",
    "normalized-identical",
    "approved-divergence",
    "approved-divergence-cascade",
    "replaced-in-next-pin",
    "unresolved-defect",
)
OWNER = re.compile(r"^(?:user decision \S.*|[A-Z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*(?: .*)?)$")
CAUSE_KINDS = ("decision", "unit", "nondeterminism", "scenario", "defect")
FX_REF = re.compile(r"^[\w./-]+\.zig:\d+(?:-\d+)?@c1db9191$")
EXPIRES = re.compile(r"^(?:pin:(?P<pin>\S+)|date:(?P<date>\d{4}-\d{2}-\d{2}))$")
SOURCE = re.compile(r"^(?P<path>[^:]+):(?P<from>\d+)(?:-(?P<to>\d+))?$")


class LedgerError(Exception):
    pass


def _rows(spec, where):
    out = set()
    for item in spec:
        if isinstance(item, int):
            out.add(item)
        elif isinstance(item, str) and re.fullmatch(r"\d+-\d+", item):
            a, b = map(int, item.split("-"))
            out.update(range(a, b + 1))
        else:
            raise LedgerError("%s: a row is a number or \"a-b\", not %r" % (where, item))
    return out


def load(path, repo_root=None):
    with open(path, "rb") as f:
        data = tomllib.load(f)
    pins = data.get("pin", [])
    names = set()
    for pin in pins:
        for key in ("name", "sha256", "version"):
            if not pin.get(key):
                raise LedgerError("a pin has no %s" % key)
        if not re.fullmatch(r"[0-9a-f]{64}", pin["sha256"]):
            raise LedgerError("pin %s: sha256 is not 64 lowercase hex" % pin["name"])
        names.add(pin["name"])

    norms = []
    for n in data.get("normalization", []):
        where = "normalization %s" % n.get("id")
        for key in ("id", "why", "source"):
            if not n.get(key):
                raise LedgerError("%s: no %s" % (where, key))
        sides = {}
        for side in ("fx", "xfx"):
            pattern = n.get(side)
            if pattern is not None:
                try:
                    sides[side] = re.compile(pattern)
                except re.error as e:
                    raise LedgerError("%s: %s pattern: %s" % (where, side, e))
        if not sides:
            raise LedgerError("%s: no fx or xfx pattern" % where)
        if ("replace" in n) == ("mask" in n):
            raise LedgerError("%s: exactly one of replace and mask" % where)
        if n.get("attrs", "keep") not in ("keep", "none"):
            raise LedgerError("%s: attrs is keep or none" % where)
        norms.append(
            {
                "id": n["id"],
                "patterns": sides,
                "replace": n.get("replace"),
                "mask": n.get("mask"),
                "attrs": n.get("attrs", "keep"),
                "absorb": bool(n.get("absorb", False)),
                "why": n["why"],
                "source": _sources(n["source"], where, repo_root),
            }
        )

    divs, ids = [], set()
    for d in data.get("divergence", []):
        where = "divergence %s" % d.get("id")
        for key in ("id", "class", "pins", "scenarios", "snaps", "reason", "owner", "resolution", "expires", "source"):
            if not d.get(key):
                raise LedgerError("%s: no %s" % (where, key))
        if not OWNER.match(d["owner"]):
            raise LedgerError("%s: the owner is a unit ID or `user decision <topic>`, not %r" % (where, d["owner"]))
        m = EXPIRES.match(d["expires"])
        if not m:
            raise LedgerError("%s: expires is pin:<name> or date:YYYY-MM-DD, not %r" % (where, d["expires"]))
        if m.group("pin"):
            if m.group("pin") not in names:
                raise LedgerError("%s: expires at no [[pin]] named %s" % (where, m.group("pin")))
            if m.group("pin") in d["pins"]:
                raise LedgerError("%s: approved for the pin it expires at, %s" % (where, m.group("pin")))
        else:
            try:
                expiry_date = datetime.date.fromisoformat(m.group("date"))
            except ValueError:
                raise LedgerError("%s: %s is no date" % (where, m.group("date")))
        if d["id"] in ids:
            raise LedgerError("%s: the id is used twice" % where)
        ids.add(d["id"])
        if d["class"] not in CLASSES:
            raise LedgerError("%s: class is one of %s" % (where, ", ".join(CLASSES)))
        unknown = set(d["pins"]) - names
        if unknown:
            raise LedgerError("%s: no [[pin]] named %s" % (where, ", ".join(sorted(unknown))))
        rx = {}
        for side in ("fx_row", "xfx_row"):
            if side in d:
                try:
                    rx[side] = re.compile(d[side])
                except re.error as e:
                    raise LedgerError("%s: %s: %s" % (where, side, e))
        rows = _rows(d.get("rows", []), where)
        whole = bool(d.get("whole_snap", False))
        if not rows and not rx and not whole:
            raise LedgerError("%s: no row selector (rows, fx_row, xfx_row) and not whole_snap" % where)
        if whole and (rows or rx):
            raise LedgerError("%s: whole_snap with a row selector" % where)
        divs.append(
            dict(
                d,
                rows=rows,
                regexes=rx,
                whole_snap=whole,
                expiry_date=None if m.group("pin") else expiry_date,
                source=_sources(d["source"], where, repo_root),
            )
        )
    causes, cids = [], set()
    for c in data.get("cause", []):
        where = "cause %s" % c.get("id")
        for key in ("id", "kind", "owner", "scenarios", "snaps", "why"):
            if not c.get(key):
                raise LedgerError("%s: no %s" % (where, key))
        if c["id"] in cids or c["id"] in ids:
            raise LedgerError("%s: the id is used twice" % where)
        cids.add(c["id"])
        kinds = c["kind"] if isinstance(c["kind"], list) else [c["kind"]]
        if not kinds or set(kinds) - set(CAUSE_KINDS):
            raise LedgerError("%s: kind is %s" % (where, " | ".join(CAUSE_KINDS)))
        if "defect" in kinds:
            for key in ("xfx", "fx"):
                if not c.get(key):
                    raise LedgerError("%s: a defect names its %s source line" % (where, key))
            for ref in [c["fx"]] if isinstance(c["fx"], str) else c["fx"]:
                if not FX_REF.match(ref):
                    raise LedgerError("%s: fx's line is path:line@c1db9191, not %r" % (where, ref))
            _sources(c["xfx"], where, repo_root)
        if ("nondeterminism" in kinds or "scenario" in kinds) and not c.get("fix"):
            raise LedgerError("%s: a capture-side cause names its fix" % where)
        rx = {}
        for side in ("fx_row", "xfx_row"):
            if side in c:
                try:
                    rx[side] = re.compile(c[side])
                except re.error as e:
                    raise LedgerError("%s: %s: %s" % (where, side, e))
        rows = _rows(c.get("rows", []), where)
        whole = bool(c.get("whole_snap", False))
        if not rows and not rx and not whole:
            raise LedgerError("%s: no row selector and not whole_snap" % where)
        causes.append(dict(c, kind=kinds, rows=rows, regexes=rx, whole_snap=whole))
    return {"pins": pins, "normalizations": norms, "divergences": divs, "causes": causes, "path": path}


def _sources(spec, where, repo_root):
    """Each `path:line` or `path:a-b` a ledger entry cites; with a repo root,
    the file must exist and hold those lines."""
    spec = [spec] if isinstance(spec, str) else list(spec)
    for s in spec:
        m = SOURCE.match(s)
        if not m:
            raise LedgerError("%s: a source is path:line or path:a-b, not %r" % (where, s))
        if repo_root is not None:
            p = os.path.join(repo_root, m.group("path"))
            if not os.path.isfile(p):
                raise LedgerError("%s: cites %s, which is not a file" % (where, m.group("path")))
            with open(p, encoding="utf-8") as f:
                n = sum(1 for _ in f)
            last = int(m.group("to") or m.group("from"))
            if not 1 <= int(m.group("from")) <= last <= n:
                raise LedgerError("%s: cites %s, past the file's %d lines" % (where, s, n))
    return spec


def pin_named(ledger, sha256):
    for pin in ledger["pins"]:
        if pin["sha256"] == sha256:
            return pin
    return None


def rules_for(ledger, side):
    """The normalization rules for one side, in the ledger's order, in the
    shape `cells.normalize` takes."""
    out = []
    for n in ledger["normalizations"]:
        pattern = n["patterns"].get(side)
        if pattern is not None:
            out.append(
                {
                    "id": n["id"],
                    "pattern": pattern,
                    "replace": n["replace"],
                    "mask": n["mask"],
                    "attrs": n["attrs"],
                    "absorb": n["absorb"],
                }
            )
    return out


def active(ledger, pin_name, today=None):
    """The divergences approved for this pin and not past their date."""
    today = today or datetime.date.today()
    return [
        d
        for d in ledger["divergences"]
        if pin_name is not None
        and pin_name in d["pins"]
        and (d["expiry_date"] is None or today <= d["expiry_date"])
    ]


def expired(ledger, pin_name, today=None):
    today = today or datetime.date.today()
    return [
        d
        for d in ledger["divergences"]
        if pin_name in d["pins"] and d["expiry_date"] is not None and today > d["expiry_date"]
    ]


def verdict_of(cls):
    return "replaced-in-next-pin" if cls == "replaced-in-next-pin" else "approved-divergence"


def _covers(d, unit, label, row, side, text):
    """Whether an entry covers one side's row. A regex speaks for its own
    side only (`fx_row` for fx's text, `xfx_row` for xfx's): an entry with a
    regex for the other side alone covers nothing here, so an approved xfx
    row never vouches for the fx row it happens to stand beside. `rows` alone
    or `whole_snap` covers both sides."""
    if not any(fnmatch.fnmatchcase(unit, g) for g in d["scenarios"]):
        return False
    if not any(fnmatch.fnmatchcase(label, g) for g in d["snaps"]):
        return False
    if d["whole_snap"]:
        return True
    if d["rows"] and row not in d["rows"]:
        return False
    if d["regexes"]:
        rx = d["regexes"].get(side + "_row")
        return bool(rx and rx.search(text))
    return True


def covering(entries, unit, label, row, side, text):
    return [d for d in entries if _covers(d, unit, label, row, side, text)]


def classify(raw_identical, diff_rows, unit, label, divergences):
    """The verdict of one snap pair, by position.

    raw_identical  whether the two captures are equal cell for cell with
                   nothing normalized
    diff_rows      the rows that still differ after normalization, as
                   (index, fx text, xfx text, ...) from `cells.differing`

    A differing row is covered when each side's text is blank or covered by
    an entry for that side. Returns (verdict, [(row number, [entry ids])])."""
    if raw_identical:
        return "identical", []
    if not diff_rows:
        return "normalized-identical", []
    rows, classes = [], []
    for item in diff_rows:
        i, fx_text, xfx_text = item[0], item[1], item[2]
        hit, ok = [], True
        for side, text in (("fx", fx_text), ("xfx", xfx_text)):
            if not text.strip():
                continue
            h = covering(divergences, unit, label, i + 1, side, text)
            ok = ok and bool(h)
            hit += [d for d in h if d not in hit]
        if not fx_text.strip() and not xfx_text.strip():
            ok = False  # a difference in blank cells' attributes: no text vouches for it
        rows.append((i + 1, [d["id"] for d in hit]))
        classes.append(_class_of(hit) if ok else None)
    if None in classes:
        return "unresolved-defect", rows
    if "approved-divergence" in classes:
        return "approved-divergence", rows
    return "replaced-in-next-pin", rows


def _class_of(hit):
    """The verdict a row's covering entries give it: approved-divergence if
    any is approved outright, replaced-in-next-pin if all are interim."""
    if not hit:
        return None
    if any(verdict_of(d["class"]) == "approved-divergence" for d in hit):
        return "approved-divergence"
    return "replaced-in-next-pin"


def aligned(fx_rows, xfx_rows, text_of):
    """The two sides aligned as sequences (difflib), trailing blank rows
    aside: every row left without a match, as (block, side, row number,
    text), a block being one run of rows the alignment could not match."""
    a, b = list(fx_rows), list(xfx_rows)
    while a and not a[-1]:
        a.pop()
    while b and not b[-1]:
        b.pop()
    key = lambda rows: [tuple(r) for r in rows]  # noqa: E731
    m = difflib.SequenceMatcher(None, key(a), key(b), autojunk=False)
    out = []
    for n, (tag, i1, i2, j1, j2) in enumerate(m.get_opcodes()):
        if tag == "equal":
            continue
        out += [(n, "fx", i + 1, text_of(a[i])) for i in range(i1, i2)]
        out += [(n, "xfx", j + 1, text_of(b[j])) for j in range(j1, j2)]
    return out


def account(left, unit, label, entries):
    """Which entries account for each row left without a match: a row with
    text is accounted for by the entries that cover its side's text; a blank
    row by the entries that account for a row with text in its block (the
    separator beside an extra notice). Returns [(block, side, row, text,
    [entries])]."""
    out, by_block = [], {}
    for block, side, row, text in left:
        hit = covering(entries, unit, label, row, side, text) if text.strip() else None
        out.append([block, side, row, text, hit])
        if hit:
            seen = by_block.setdefault(block, [])
            seen += [d for d in hit if d not in seen]
    for item in out:
        if item[4] is None:
            item[4] = list(by_block.get(item[0], []))
    return [tuple(x) for x in out]


def cascade(fx_rows, xfx_rows, unit, label, divergences, text_of):
    """The aligned reading of a pair (see the module's docstring).

    Returns (verdict or None, [(block, side, row, text, [entry ids])] for
    every row left without a match). The verdict is
    approved-divergence-cascade, or replaced-in-next-pin when only interim
    entries account for the rows; None when a row is unaccounted for."""
    rows = account(aligned(fx_rows, xfx_rows, text_of), unit, label, divergences)
    classes = [_class_of(hit) for *_, hit in rows]
    left = [(block, side, row, text, [d["id"] for d in hit]) for block, side, row, text, hit in rows]
    if not rows or None in classes:
        return None, left
    if "approved-divergence" in classes:
        return "approved-divergence-cascade", left
    return "replaced-in-next-pin", left
