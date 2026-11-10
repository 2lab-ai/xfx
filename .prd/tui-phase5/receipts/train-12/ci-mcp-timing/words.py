"""The `remote` server's status word in every snap of every repeat, per binary.

usage: python3 -I words.py <repeat out> <scenario> [<label>...]

For each snap that shows a row naming `remote` with a status word, the word
fx's capture and xfx's capture hold, run by run, and how often they agree.
A row is `remote` followed by a status word (connecting, retrying, failed,
ready, ...) somewhere after it on the same row.

The word is read from the `.ansi` capture, escapes taken out: the one the
sweep scores (sweep.py:475-476). capture.sh takes a snap's `.txt` and
`.ansi` with two `capture-pane` calls one after the other
(capture.sh:1096-1097), so a snap that falls on a turn can hold one word in
each; those are counted apart. A capture is found by its whole file name,
`<side>-<scenario>-<rows>x<cols>-<label>`, so `list` is not `back-list`.
"""
import collections
import glob
import json
import os
import re
import sys

out, scen = sys.argv[1], sys.argv[2]
only = set(sys.argv[3:])
WORD = re.compile(r"\bremote\b.*?\b(connecting|retrying|failed|ready|disabled|unavailable|needs sign-in)\b")
ESC = re.compile(r"\x1b\[[0-9;:?]*[A-Za-z]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")


def capture(d, side, label, ext):
    name = re.compile(r"^%s-%s-\d+x\d+-%s\.%s$" % (side, re.escape(scen), re.escape(label), ext))
    hits = [p for p in glob.glob(os.path.join(d, "runs", scen, side, "*." + ext)) if name.match(os.path.basename(p))]
    return hits[0] if hits else None


def word(path):
    if not path:
        return "-"
    with open(path, encoding="utf-8", errors="replace") as f:
        for row in f.read().split("\n"):
            m = WORD.search(ESC.sub("", row))
            if m:
                return m.group(1)
    return "-"


def key(d):
    side, i = os.path.basename(d).rsplit("-", 1)
    return (side, int(i))


runs = sorted((p for p in glob.glob(os.path.join(out, "*-*")) if os.path.isfile(os.path.join(p, "pairs.json"))), key=key)
table = collections.OrderedDict()
split = []
for d in runs:
    pairs = {p["label"]: p["verdict"] for p in json.load(open(os.path.join(d, "pairs.json"))) if p["scenario"] == scen}
    for label in pairs:
        if only and label not in only:
            continue
        fx = word(capture(d, "fx", label, "ansi"))
        xf = word(capture(d, "xfx", label, "ansi"))
        for side, w in (("fx", fx), ("xfx", xf)):
            t = word(capture(d, side, label, "txt"))
            if t != w:
                split.append((os.path.basename(d), label, side, t, w))
        if fx == "-" and xf == "-":
            continue
        table.setdefault(label, []).append((os.path.basename(d), fx, xf, pairs[label]))
for label, rows in table.items():
    fxw = collections.Counter(r[1] for r in rows)
    xw = collections.Counter(r[2] for r in rows)
    agree = sum(1 for r in rows if r[1] == r[2])
    passed = sum(1 for r in rows if r[3] in ("identical", "normalized-identical"))
    print("%-22s runs %2d  fx %-34s xfx %-34s same word %2d  pass %2d"
          % (label, len(rows), dict(fxw), dict(xw), agree, passed))
print()
for label, rows in table.items():
    print("%-22s %s" % (label, "  ".join("%s:%s/%s" % (r[0], r[1][:4], r[2][:4]) for r in rows)))
print()
print("snaps whose .txt and .ansi hold different words: %d" % len(split))
for run, label, side, t, a in split:
    print("  %s %s %s: .txt %s, .ansi %s" % (run, label, side, t, a))
