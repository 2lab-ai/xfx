"""Writes a capture run's own roots as placeholders in a file the run wrote,
and refuses a file in which any of them survives.

usage: python3 -I roots.py [--root <path> <name>]... [--suffix <fxcap.XXXXXX>]
                           [--check] -- <file or directory>...

capture.sh runs it on every file before the file reaches the out directory
(`publish`), and once more, `--check` only, over everything the run left
there. A root is a path the run knows -- its working directory, its run
directory, the scenario directory, bun, the harness's TMPDIR and HOME -- and
its name one of NAMES. Each root is written

  <name>        where it stands as given (`$PWD`, an environment value)
  <name:real>   where it stands resolved (`getcwd()`, `realpath`), when that
                differs: a binary that prints one form and its pair the
                other differ in the capture as they did on the screen

and its JSON-escaped (`\\/`) and percent-encoded spellings as its own.

**Where a root is found.** In the text a reader of the file sees, not only in
its bytes:

- across a terminal control sequence: a coloured path, or a path whose
  matched letters a picker bolds, is one path (`/pri\\e[1mvate/...`);
- across a row's end: a root a terminal wrapped at its margin, or a binary
  broke onto its next row behind a gutter of spaces or box-drawing
  characters, is one root;
- inside an OSC sequence's payload, by its bytes: an OSC 8 link's target.

It ends where no path-name character follows it, a row's end being no such
character (`<run>/workx` is not `<run>/work`). What precedes it is not read:
the longest root is found first, so `/tmp/x` inside `/private/tmp/x` is the
longer root's.

**How it is written.** The placeholder stands where the root's first visible
character stood. Its other characters go; every control sequence, gutter and
row end between them stays, in order: the row structure and the colours after
the root are the file's own. A root broken across rows leaves the placeholder
on the first row, which can then be longer than the screen was wide. Colours
that changed inside a root are replayed after its placeholder: the state after
it is exact, which of its characters carried them is not kept.

**A literal placeholder.** Text that already reads `<work>` -- a binary's
help, a fixture -- would read as a rewritten root. Before anything is
rewritten, every `<name>` and `<\\name>`, ... already in the file gains one
backslash (`<work>` -> `<\\work>`), so a placeholder with no backslash is a
root and nothing else, and the rewrite can be undone.

**Fail closed.** After the rewrite the file is read again: if a root in any of
its forms, or the run directory's own name (`--suffix`, mktemp's random tail),
is still in its bytes or in its visible text, the file is named on standard
error with what survived -- by placeholder, never the path -- and the exit
status is 3. A root a binary cut short (`.../fxcap.Ab12Cd/wo...`, or its head
scrolled off the top of a frame) is found by its run name.

`--check` rewrites nothing and only refuses.
"""

import os
import re
import sys
from urllib.parse import quote

NAMES = ("work", "scratch", "scenarios", "bun", "tmp", "home")
PATH_CHAR = re.compile(rb"[A-Za-z0-9._~+-]")
# A control sequence: CSI (cursor moves among them), OSC (to BEL or ST), or
# ESC and one byte.
CONTROL = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[\x20-\x7e]")
# Ends a row: a newline, a carriage return, or a CSI that moves the cursor.
CURSOR_MOVE = re.compile(rb"\x1b\[[0-?]*[HfABCDEGd]\Z")
LITERAL = re.compile(rb"<(\\*)(%s)(:real)?>" % b"|".join(n.encode() for n in NAMES))
GUTTER_BYTES = (b" ", b"\t")


class Refused(Exception):
    pass


def placeholder(name, real=False):
    return ("<%s%s>" % (name, ":real" if real else "")).encode()


def spellings(path):
    """A root's bytes as a file may spell them: as they are, JSON-escaped,
    percent-encoded (a file URI)."""
    raw = os.fsencode(path)
    out = [raw, raw.replace(b"/", b"\\/")]
    pct = quote(raw, safe="/~").encode()
    if pct != raw:
        out.append(pct)
    return out


def rules_for(roots):
    """[(bytes, placeholder)], the longest first: each root as given and, when
    that differs, as resolved, in every spelling. Empty roots and `/` are
    none."""
    seen, rules = set(), []
    for path, name in roots:
        if not path:
            continue
        forms = [(path.rstrip("/") or "/", False)]
        real = os.path.realpath(path).rstrip("/") or "/"
        if real != forms[0][0]:
            forms.append((real, True))
        for form, is_real in forms:
            if form == "/":
                continue
            for b in spellings(form):
                if b not in seen:
                    seen.add(b)
                    rules.append((b, placeholder(name, is_real)))
    rules.sort(key=lambda r: -len(r[0]))
    return rules


def _gutter_at(data, i):
    """The length of the gutter character at `i` (a space, a tab, or a
    box-drawing character, U+2500-U+259F), or 0."""
    if data[i : i + 1] in GUTTER_BYTES:
        return 1
    if data[i : i + 1] == b"\xe2" and data[i + 1 : i + 2] in (b"\x94", b"\x95", b"\x96"):
        return 3
    return 0


class View:
    """The visible text of `data` with each row's edge gutters dropped and its
    rows joined: `text`, and for each of its runs where it came from."""

    def __init__(self, data):
        self.data = data
        self.controls = []  # (start, end) of every control sequence
        rows, row, pos = [], [], 0
        for m in CONTROL.finditer(data):
            self._visible(data, pos, m.start(), rows, row)
            self.controls.append(m.span())
            if CURSOR_MOVE.search(m.group()):
                rows.append(row)
                row = []
            pos = m.end()
        self._visible(data, pos, len(data), rows, row)
        rows.append(row)
        text, runs, ends = bytearray(), [], set()
        for row in rows:
            row = self._trim(data, row)
            for start, end in row:
                runs.append((len(text), start, end))
                text += data[start:end]
            ends.add(len(text))
        self.text = bytes(text)
        self.runs = runs
        self.row_ends = ends

    @staticmethod
    def _visible(data, start, end, rows, row):
        """Splits data[start:end] at its row ends into runs of `row`."""
        pos = start
        for m in re.finditer(rb"\r?\n|\r", data[start:end]):
            if m.start() + start > pos:
                row.append((pos, m.start() + start))
            rows.append(list(row))
            row.clear()
            pos = m.end() + start
        if end > pos:
            row.append((pos, end))

    @staticmethod
    def _trim(data, row):
        """The row's runs without the gutter at either end."""
        row = [list(r) for r in row]
        while row:
            n = _gutter_at(data, row[0][0])
            if not n:
                break
            row[0][0] += n
            if row[0][0] >= row[0][1]:
                row.pop(0)
        while row:
            start, end = row[-1]
            if data[end - 1 : end] in GUTTER_BYTES:
                n = 1
            elif end - 3 >= start and _gutter_at(data, end - 3) == 3:
                n = 3
            else:
                n = 0
            if not n:
                break
            row[-1][1] -= n
            if row[-1][1] <= row[-1][0]:
                row.pop()
        return [tuple(r) for r in row]

    def spans(self, a, b):
        """The bytes of `data` that text[a:b] came from, as (start, end)."""
        out = []
        for t, start, end in self.runs:
            length = end - start
            lo, hi = max(a, t), min(b, t + length)
            if lo < hi:
                out.append((start + lo - t, start + hi - t))
            if t >= b:
                break
        return out

    def ends_whole(self, e):
        return e >= len(self.text) or e in self.row_ends or not PATH_CHAR.match(self.text[e : e + 1])


def _find(view, needle, taken):
    """Every occurrence of `needle` in the view that ends whole and overlaps
    nothing in `taken`, as (a, b)."""
    out, start = [], 0
    while True:
        a = view.text.find(needle, start)
        if a < 0:
            return out
        b = a + len(needle)
        start = a + 1
        if view.ends_whole(b) and not any(a < y and x < b for x, y in taken):
            out.append((a, b))
            taken.append((a, b))


def _in_controls(data, controls, rules):
    """Each OSC payload with its roots written and its literal placeholders
    escaped, as {(start, end): new bytes}."""
    out = {}
    for start, end in controls:
        seq = data[start:end]
        if not seq.startswith(b"\x1b]"):
            continue
        new = LITERAL.sub(lambda m: b"<\\" + m.group(1) + m.group(2) + (m.group(3) or b"") + b">", seq)
        for needle, name in rules:
            new = re.sub(re.escape(needle) + rb"(?![A-Za-z0-9._~+-])", lambda m, n=name: n, new)
        if new != seq:
            out[(start, end)] = new
    return out


def rewrite(data, rules):
    """`data` with its literal placeholders escaped and its roots written as
    their placeholders; and how many of each were written."""
    view = View(data)
    edits = []  # (start, end, replacement)
    for m in LITERAL.finditer(view.text):
        # One more backslash, right after the `<`.
        (start, end), = view.spans(m.start(), m.start() + 1)
        edits.append((end, end, b"\\"))
    counts = {}
    taken = []
    for needle, name in rules:
        for a, b in _find(view, needle, taken):
            spans = view.spans(a, b)
            edits.append((spans[0][0], spans[0][1], name))
            edits.extend((s, e, b"") for s, e in spans[1:])
            counts[name] = counts.get(name, 0) + 1
    for (start, end), new in _in_controls(data, view.controls, rules).items():
        edits.append((start, end, new))
        counts[b"(in a control sequence)"] = counts.get(b"(in a control sequence)", 0) + 1
    if not edits:
        return data, counts
    edits.sort(key=lambda e: (e[0], e[1]))
    out, pos = bytearray(), 0
    for start, end, new in edits:
        if start < pos:
            raise Refused("overlapping edits at byte %d" % start)
        out += data[pos:start]
        out += new
        pos = end
    out += data[pos:]
    return bytes(out), counts


def survivors(data, rules, suffix):
    """What of the run is still in `data`, by placeholder: a root in its bytes
    or in its visible text, or the run directory's own name."""
    view = View(data)
    found = []
    for needle, name in rules:
        if needle in data or needle in view.text:
            found.append(name.decode())
    if suffix and (suffix.encode() in data or suffix.encode() in view.text):
        found.append("the run directory's name")
    return found


def files_under(paths):
    for p in paths:
        if os.path.isdir(p) and not os.path.islink(p):
            for top, _, names in os.walk(p):
                for n in sorted(names):
                    yield os.path.join(top, n)
        else:
            yield p


def main(argv):
    roots, suffix, check, files, i = [], None, False, [], 0
    while i < len(argv):
        a = argv[i]
        if a == "--root":
            roots.append((argv[i + 1], argv[i + 2]))
            if argv[i + 2] not in NAMES:
                sys.exit("roots.py: no placeholder named %s" % argv[i + 2])
            i += 3
        elif a == "--suffix":
            suffix = argv[i + 1]
            i += 2
        elif a == "--check":
            check = True
            i += 1
        elif a == "--":
            files = argv[i + 1 :]
            break
        else:
            sys.exit(__doc__)
    rules = rules_for(roots)
    bad = 0
    for path in files_under(files):
        if os.path.islink(path) or not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            data = f.read()
        if not check:
            try:
                new, _ = rewrite(data, rules)
            except Refused as e:
                print("LOCAL-PATH GUARD: %s: %s" % (path, e), file=sys.stderr)
                bad += 1
                continue
            if new != data:
                with open(path, "wb") as f:
                    f.write(new)
                data = new
        left = survivors(data, rules, suffix)
        if left:
            print("LOCAL-PATH GUARD: %s still holds %s after the rewrite" % (path, ", ".join(sorted(set(left)))), file=sys.stderr)
            bad += 1
    return 3 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
