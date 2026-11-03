"""`parity-ledger.toml`: what the sweep may set aside, and how a snap pair is
classified against it.

The ledger has three bins and one class:

  normalization  a fixed, mechanical rewrite of a row's text before the two
                 sides are compared (each product's banner, the product name);
                 a pair equal after them is normalized-identical. `absorb =
                 true` lets a rewrite that widens the text (`fx` -> `xfx`)
                 take the column it adds from the padding after it or before
                 it, so a padded or right-aligned column stays where it
                 stood; a row equal in any of the three readings is the same
  divergence     an approved difference: scenario / snap / row selector, the
                 reason, the owning epic or decision, the expiry, the source
                 line that records it, and its class -- `deliberate` (xfx
                 means to differ) or `replaced-in-next-pin` (the surface is
                 replaced in the next fx pin, so it is not polished against
                 this one)
  anything else  an unresolved defect

Every divergence is scoped to the pins (`[[pin]]`, by sha256) it was recorded
against: a sweep against another fx applies none of them until they are
approved again for it.

A divergence's selector is `scenarios` and `snaps` (fnmatch globs) and at least
one row selector -- `rows` (row numbers, 1 is the top row, `"a-b"` a range),
`fx_row` / `xfx_row` (regexes searched in that side's normalized row text; a
row matches when either matches) -- unless it says `whole_snap = true`, which
the scoreboard lists apart.
"""

import fnmatch
import os
import re
import tomllib

CLASSES = ("deliberate", "replaced-in-next-pin")
VERDICTS = (
    "identical",
    "normalized-identical",
    "approved-divergence",
    "replaced-in-next-pin",
    "unresolved-defect",
)
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
        for key in ("id", "class", "pins", "scenarios", "snaps", "reason", "owner", "expires", "source"):
            if not d.get(key):
                raise LedgerError("%s: no %s" % (where, key))
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
                source=_sources(d["source"], where, repo_root),
            )
        )
    return {"pins": pins, "normalizations": norms, "divergences": divs, "path": path}


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


def active(ledger, pin_name):
    return [d for d in ledger["divergences"] if pin_name is not None and pin_name in d["pins"]]


def _matches(d, unit, label, row, fx_text, xfx_text):
    if not any(fnmatch.fnmatchcase(unit, g) for g in d["scenarios"]):
        return False
    if not any(fnmatch.fnmatchcase(label, g) for g in d["snaps"]):
        return False
    if d["whole_snap"]:
        return True
    if d["rows"] and row not in d["rows"]:
        return False
    if d["regexes"]:
        fx_rx, xfx_rx = d["regexes"].get("fx_row"), d["regexes"].get("xfx_row")
        return bool((fx_rx and fx_rx.search(fx_text)) or (xfx_rx and xfx_rx.search(xfx_text)))
    return True


def classify(raw_identical, diff_rows, unit, label, divergences):
    """The verdict of one snap pair.

    raw_identical  whether the two captures are equal cell for cell with
                   nothing normalized
    diff_rows      the rows that still differ after normalization, as
                   (index, fx text, xfx text, ...) from `cells.differing`

    Returns (verdict, [(row number, [entry ids]) for each differing row]).
    """
    if raw_identical:
        return "identical", []
    if not diff_rows:
        return "normalized-identical", []
    rows, classes = [], []
    for item in diff_rows:
        i, fx_text, xfx_text = item[0], item[1], item[2]
        hit = [d for d in divergences if _matches(d, unit, label, i + 1, fx_text, xfx_text)]
        rows.append((i + 1, [d["id"] for d in hit]))
        if not hit:
            classes.append(None)
        elif any(d["class"] == "deliberate" for d in hit):
            classes.append("deliberate")
        else:
            classes.append("replaced-in-next-pin")
    if None in classes:
        return "unresolved-defect", rows
    if "replaced-in-next-pin" in classes:
        return "replaced-in-next-pin", rows
    return "approved-divergence", rows
