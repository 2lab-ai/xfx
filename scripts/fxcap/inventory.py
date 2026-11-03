"""fx's surface inventory, read off the pinned binary's own screens, and the
scenarios that exercise each surface.

`inventory/inventory.cap` walks fx's `/help` menu and its slash popup from end
to start (see the file for why that direction); `read` parses the snaps it
leaves:

  commands   every row of the command registry, as `/help` lists it: the name
             and its help entry (the description `/help` shows), with the
             category the slash popup shows beside it
  keys       the key hints `/help` and the slash popup show under their lists
             -- fx's help has no key list of its own (`fx --help` prints
             commands and flags only), so these footers are the key list its
             help shows

The read fails rather than shrinking the inventory: every snap's `Commands N`
must agree, and `/help` and the popup must each have listed exactly N names,
the same N names.

Coverage is mechanical, from the `.cap` lines alone:

  a command  is covered by a unit when a `type` line of it starts with the
             command's name (`/model`, `/model gro`): an alias (`/setup`) or a
             partial name (`/mo`) is not the command, and is listed as an
             unmapped token instead
  a key hint is covered when, after the line that opens its menu (`type /help`
             for `/help`, any `type /...` for the popup), a `key` line sends
             one of the hint's keys (`↑↓` = Up or Down, `tab` = Tab or BTab,
             `enter` = Enter, `esc` = Escape) -- whether the menu was still
             open at that key is not checked
"""

import os
import re

HEADER = re.compile(r"^Commands (\d+)\b")
DIVIDER = re.compile(r"^─+$")
HELP_ROW = re.compile(r"^  (/\S+)\s{2,}(\S.*?)\s*$")
SLASH_ROW = re.compile(r"^  (/\S+)\s{2,}(\S.*?)\s{2,}(\S+)\s*$")

KEYS = {
    "↑↓": ("Up", "Down"),
    "↑": ("Up",),
    "↓": ("Down",),
    "tab": ("Tab", "BTab"),
    "shift+tab": ("BTab",),
    "enter": ("Enter",),
    "esc": ("Escape",),
    "space": ("Space",),
}


class InventoryError(Exception):
    pass


def _lists(text):
    """(count, rows, footer) of one menu snap, or None when no menu is up."""
    lines = text.split("\n")
    for i, line in enumerate(lines):
        m = HEADER.match(line)
        if not m:
            continue
        rows, j = [], i + 1
        while j < len(lines) and not DIVIDER.match(lines[j]):
            if lines[j].strip():
                rows.append(lines[j])
            j += 1
        footer = ""
        for k in range(j + 1, len(lines)):
            if lines[k].strip():
                footer = lines[k].strip()
                break
        return int(m.group(1)), rows, footer
    return None


def _hints(footer):
    out = []
    for hint in re.split(r"\s{2,}", footer.strip()):
        if not hint:
            continue
        word = hint.split(" ")[0].lower()
        keys = KEYS.get(word)
        if keys is None and re.fullmatch(r"ctrl\+[a-z]", word):
            keys = ("C-" + word[-1],)
        out.append({"hint": hint, "keys": list(keys or ())})
    return out


def read(snapdir, size):
    """The inventory from one capture of `inventory.cap` on fx."""
    def snaps(kind):
        prefix = "fx-inventory-%s-%s-" % (size, kind)
        names = sorted(f for f in os.listdir(snapdir) if f.startswith(prefix) and f.endswith(".txt"))
        if not names:
            raise InventoryError("no %s snaps in the inventory capture" % kind)
        texts = []
        for f in names:
            with open(os.path.join(snapdir, f), encoding="utf-8") as fh:
                texts.append(fh.read())
        return texts

    counts, help_rows, slash_rows, footers = set(), {}, {}, {"help": "", "slash": ""}
    for kind, pattern, into in (("help", HELP_ROW, help_rows), ("slash", SLASH_ROW, slash_rows)):
        for text in snaps(kind):
            got = _lists(text)
            if got is None:
                raise InventoryError("a %s snap shows no menu" % kind)
            count, rows, footer = got
            counts.add(count)
            footers[kind] = footers[kind] or footer
            for row in rows:
                m = pattern.match(row)
                if not m:
                    raise InventoryError("a %s row is not a command row: %r" % (kind, row))
                into.setdefault(m.group(1), m.groups()[1:])
    if len(counts) != 1:
        raise InventoryError("the menus disagree on their count: %s" % sorted(counts))
    count = counts.pop()
    if len(help_rows) != count or len(slash_rows) != count or set(help_rows) != set(slash_rows):
        raise InventoryError(
            "read %d /help rows and %d popup rows of %d (missing from /help: %s; from the popup: %s)"
            % (
                len(help_rows),
                len(slash_rows),
                count,
                sorted(set(slash_rows) - set(help_rows)),
                sorted(set(help_rows) - set(slash_rows)),
            )
        )
    # `/help`'s order is the registry's; insertion order is the walk's.
    order = _help_order(snaps("help"))
    commands = [
        {"id": name, "kind": "command", "description": help_rows[name][0], "category": slash_rows[name][1]}
        for name in order
    ]
    keys = []
    for menu in ("help", "slash"):
        for h in _hints(footers[menu]):
            keys.append({"id": "%s: %s" % (menu, h["hint"]), "kind": "key", "menu": menu, "keys": h["keys"]})
    return {"count": count, "commands": commands, "keys": keys}


def _help_order(texts):
    """The registry's order, from `/help`'s windows: each window is a run of
    consecutive rows, so the rows are ordered by merging the runs."""
    after = {}
    names = []
    for text in texts:
        _, rows, _ = _lists(text)
        run = [HELP_ROW.match(r).group(1) for r in rows]
        for a, b in zip(run, run[1:]):
            after.setdefault(a, b)
        for n in run:
            if n not in names:
                names.append(n)
    # The first row is the one nothing comes after; walk the chain from it.
    successors = set(after.values())
    first = [n for n in names if n not in successors]
    if len(first) != 1:
        return names
    order, n = [], first[0]
    while n is not None and n not in order:
        order.append(n)
        n = after.get(n)
    return order if len(order) == len(names) else names


def coverage(inv, units):
    """{surface id: [unit names]} and {unit name: [unmapped tokens]}."""
    names = {c["id"] for c in inv["commands"]}
    covered = {s["id"]: [] for s in inv["commands"] + inv["keys"]}
    unmapped = {}
    for unit in units:
        hits, extra = set(), set()
        for reading in (unit.fx, unit.xfx):
            for token in reading.typed_commands():
                (hits if token in names else extra).add(token)
            for key in inv["keys"]:
                if _key_covered(reading, key):
                    hits.add(key["id"])
        for sid in sorted(hits):
            covered[sid].append(unit.name)
        if extra:
            unmapped[unit.name] = sorted(extra)
    return covered, unmapped


def _key_covered(reading, key):
    opened = False
    for line in reading.lines:
        if line.startswith("type "):
            text = line[5:]
            if key["menu"] == "help" and text.strip() == "/help":
                opened = True
            elif key["menu"] == "slash" and text.startswith("/"):
                opened = True
        elif opened and line.startswith("key ") and key["keys"]:
            if set(line[4:].split()) & set(key["keys"]):
                return True
    return False
