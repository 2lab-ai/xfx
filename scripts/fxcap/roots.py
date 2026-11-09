"""Writes a capture run's own roots as placeholders in a file the run wrote,
and refuses a file in which any of them survives.

usage: python3 -I roots.py [--root <path> <name>]... [--suffix <fxcap.XXXXXX>]
                           [--check] -- <file or directory>...

capture.sh runs it on every file before the file reaches the out directory
(`publish`), and once more, `--check` only, over everything the run left
there. A root is a path the run knows -- its working directory, its run
directory, the scenario directory, bun, the harness's TMPDIR and HOME -- and
its name one of NAMES. Each root is written as its own placeholder:

  <name___>        where it stands as given (`$PWD`, an environment value)
  <name:real___>   where it stands resolved (`getcwd()`, `realpath`), when
                   that differs: a binary that prints one form and its pair
                   the other differ in the capture as they did on the screen

**Exactly the root's width.** A placeholder is as wide as the root it stands
for: the name between `<` and `>`, filled with `_` (FILL) to the root's width
in columns -- for a path of ASCII characters, every root on the CI runners
and on this machine, its length in bytes. A root narrower than its name gets
the short form, `<` and the name's letter (CODES; upper case when resolved)
filled to its width: `/root` is `<h__>`. A root narrower than four columns
has no placeholder, and the file is refused (exit 3). The placeholder is
written over the root's own characters, one column for each of the root's,
through the bytes they stood in: a root a terminal wrapped at its margin
wraps its placeholder at the same column, a colour change inside a root
stays where it was, and every column after the root keeps its column. Two
captures that differed in where a root broke or changed colour differ there
still; two sides whose roots are as long read the same placeholder.

Its spellings are the same root: JSON-escaped (`\\/`), the percent-encoding
`urllib.parse.quote(root, safe="/~")` produces (a space, a `%`, a byte past
ASCII; the separators and `~` as they are), and the root with `//` and `/./`
collapsed (`os.path.normpath`: a TMPDIR with a trailing slash puts `//` in a
path mktemp names). Each placeholder is as wide as the spelling it stands in.

**Where a root is found.** In the text a reader of the file sees, not only in
its bytes:

- across a terminal control sequence (CSI, OSC, nF): a coloured path, or a
  path whose matched letters a picker bolds, is one path
  (`/pri\\e[1mvate/...`);
- across a row's end: a root a terminal wrapped at its margin, or a binary
  broke onto its next row behind a gutter of spaces, tabs or box-drawing and
  block characters (U+2500-U+259F), is one root;
- inside an OSC sequence's payload, by its bytes: an OSC 8 link's target.

It ends where no path-name character follows it within its row
(`<run>/workx` is not `<run>/work`); a row's end ends it. What precedes it is
not read: the longest root is found first, so `/tmp/x` inside
`/private/tmp/x` is the longer root's.

**A literal placeholder.** Text that already reads like one -- `<work>`,
`<work___>`, `<h__>`, a binary's help, a fixture -- would read as a rewritten
root. Before anything is rewritten, each gains one `!` after its `<`
(`<work>` -> `<!work>`, `<!work>` -> `<!!work>`), so a placeholder with no
`!` is a root and nothing else, and the rewrite can be undone. A `!` is
valid in every text these files are, a JSON string among them. A literal so
escaped is one column wider.

**Fail closed.** After the rewrite the file is read again. A root, in any of
its spellings and as given or resolved, still in its bytes or its visible
text where it ends whole -- by the rewrite's own rule, so a sibling whose
name extends the HOME's (`<HOME>2/notes`) is no survivor of it -- refuses the
file; so does the run
directory's own name (`--suffix`, mktemp's random tail) anywhere at all: it
names no other path, and a root a binary cut short (`.../fxcap.Ab12Cd/wo...`)
or whose head scrolled off a frame keeps it. The file is named on standard
error with what survived, by placeholder, never the path, and the exit
status is 3. A fragment of a root above the run directory (the harness's
TMPDIR or HOME) holds no run name; scripts/check-no-local-paths.sh's shapes
are what refuse such a fragment in a tracked file.

`--check` rewrites nothing and only refuses.
"""

import os
import re
import sys
import unicodedata
from urllib.parse import quote

NAMES = ("work", "scratch", "scenarios", "bun", "tmp", "home")
CODES = {"work": "w", "scratch": "s", "scenarios": "c", "bun": "b", "tmp": "t", "home": "h"}
FILL = "_"
EDGE = rb"A-Za-z0-9._~+-"
PATH_CHAR = re.compile(rb"[" + EDGE + rb"]")
# A control sequence: CSI (cursor moves among them), OSC (to BEL or ST), or
# an nF or Fp/Fe/Fs escape (ESC, intermediates 0x20-0x2F, a final byte).
CONTROL = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[\x20-\x2f]*[\x30-\x7e]")
# Ends a row: a newline, a carriage return, or a CSI that moves the cursor.
CURSOR_MOVE = re.compile(rb"\x1b\[[0-?]*[HfABCDEGd]\Z")
GUTTER_BYTES = (b" ", b"\t")
# Every placeholder, and every literal that reads like one, with the `!`s an
# escape gave it.
_NAMES_RE = "|".join(NAMES)
_CODES_RE = "".join(CODES.values()) + "".join(CODES.values()).upper()
PLACEHOLDER_TEXT = r"<(!*)((?:%s)(?::real)?%s*|[%s]%s+)>" % (_NAMES_RE, FILL, _CODES_RE, FILL)
LITERAL = re.compile(PLACEHOLDER_TEXT.encode())
# A placeholder capture.sh wrote (no `!`), for a reader of its files:
# redact.py, scripts/check-no-local-paths.sh's ERE spells the same grammar.
PLACEHOLDER = re.compile(r"<(?:(?:%s)(?::real)?%s*|[%s]%s+)>" % (_NAMES_RE, FILL, _CODES_RE, FILL))


class Refused(Exception):
    pass


def _width(text):
    """Columns, as scripts/fxcap/cells.py counts them."""
    n = 0
    for ch in text:
        if unicodedata.combining(ch):
            continue
        n += 2 if unicodedata.east_asian_width(ch) in "WF" else 1
    return n


def make_placeholder(name, width, real=False):
    """The placeholder for a root of `width` columns: the name (`:real` when
    resolved) filled to that width, or its short form, or Refused."""
    full = "<%s%s>" % (name, ":real" if real else "")
    if width >= len(full):
        return full[:-1] + FILL * (width - len(full)) + ">"
    if width >= 4:
        code = CODES[name].upper() if real else CODES[name]
        return "<" + code + FILL * (width - 3) + ">"
    raise Refused("a root of %d columns has no placeholder of its width" % width)


def spellings(path):
    """A root's bytes as a file may spell them: as they are, JSON-escaped,
    percent-encoded (a file URI) as `quote(path, safe="/~")` does."""
    raw = os.fsencode(path)
    out = [raw, raw.replace(b"/", b"\\/")]
    pct = quote(raw, safe="/~").encode()
    if pct != raw:
        out.append(pct)
    return out


def rules_for(roots):
    """[(bytes, name, real)], the longest first: each root as given, with
    `//` and `/./` collapsed, and, when either differs, as resolved, in every
    spelling. Empty roots and `/` are none."""
    seen, rules = set(), []
    for path, name in roots:
        if not path:
            continue
        given = path.rstrip("/") or "/"
        forms = [(given, False), (os.path.normpath(given), False)]
        real = os.path.realpath(path).rstrip("/") or "/"
        if real not in (given, os.path.normpath(given)):
            forms.append((real, True))
        for form, is_real in forms:
            if form == "/":
                continue
            for b in spellings(form):
                if b not in seen:
                    seen.add(b)
                    rules.append((b, name, is_real))
    rules.sort(key=lambda r: -len(r[0]))
    return rules


def _gutter_at(data, i, limit=None):
    """The length of the gutter character at `i` (a space, a tab, or a
    box-drawing or block character, U+2500-U+259F: e2 94 80 to e2 96 9f),
    or 0. `limit`, if given, is where the character must end by."""
    if data[i : i + 1] in GUTTER_BYTES:
        return 1
    lead, mid, last = data[i : i + 1], data[i + 1 : i + 2], data[i + 2 : i + 3]
    if lead == b"\xe2" and (mid in (b"\x94", b"\x95") or (mid == b"\x96" and last and last <= b"\x9f")):
        if limit is None or i + 3 <= limit:
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
            n = _gutter_at(data, row[0][0], row[0][1])
            if not n:
                break
            row[0][0] += n
            if row[0][0] >= row[0][1]:
                row.pop(0)
        while row:
            start, end = row[-1]
            if data[end - 1 : end] in GUTTER_BYTES:
                n = 1
            elif end - 3 >= start and _gutter_at(data, end - 3, end) == 3:
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


def _escape_literals(data):
    return LITERAL.sub(lambda m: b"<!" + m.group(1) + m.group(2) + b">", data)


def _over(data, spans, name, real):
    """Edits that write the placeholder for the root at `spans` over its own
    characters, each span its share of the placeholder's columns."""
    pieces = [data[s:e].decode("utf-8", "replace") for s, e in spans]
    ph = make_placeholder(name, sum(_width(p) for p in pieces), real).encode()
    edits, at = [], 0
    for (s, e), piece in zip(spans, pieces):
        w = _width(piece)
        edits.append((s, e, ph[at : at + w]))
        at += w
    return edits


def _in_controls(data, controls, rules):
    """Each OSC payload with its roots written and its literal placeholders
    escaped, as {(start, end): new bytes}."""
    out = {}
    for start, end in controls:
        seq = data[start:end]
        if not seq.startswith(b"\x1b]"):
            continue
        new = _escape_literals(seq)
        for needle, name, real in rules:
            pattern = re.escape(needle) + rb"(?![" + EDGE + rb"])"
            new = re.sub(pattern, lambda m, n=name, r=real: make_placeholder(n, _width(m.group().decode("utf-8", "replace")), r).encode(), new)
        if new != seq:
            out[(start, end)] = new
    return out


def rewrite(data, rules):
    """`data` with its literal placeholders escaped and its roots written as
    their placeholders, each over its own characters; and how many of each
    were written."""
    view = View(data)
    edits = []  # (start, end, replacement)
    for m in LITERAL.finditer(view.text):
        # One `!`, right after the `<`.
        (start, end), = view.spans(m.start(), m.start() + 1)
        edits.append((end, end, b"!"))
    counts = {}
    taken = []
    for needle, name, real in rules:
        for a, b in _find(view, needle, taken):
            edits.extend(_over(data, view.spans(a, b), name, real))
            key = name + (":real" if real else "")
            counts[key] = counts.get(key, 0) + 1
    for (start, end), new in _in_controls(data, view.controls, rules).items():
        edits.append((start, end, new))
        counts["(in a control sequence)"] = counts.get("(in a control sequence)", 0) + 1
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
    """What of the run is still in `data`, by placeholder: a root where it
    ends whole, in its bytes or in its visible text, by the rewrite's own
    rule; or the run directory's own name, anywhere."""
    view = View(data)
    found = []
    for needle, name, real in rules:
        whole_bytes = re.search(re.escape(needle) + rb"(?![" + EDGE + rb"])", data)
        if whole_bytes or _find(view, needle, []):
            found.append(name + (":real" if real else ""))
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
