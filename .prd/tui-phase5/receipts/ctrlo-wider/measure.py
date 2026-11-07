"""Part 2's measurement: the rows that hold a scalar of the family (the 43
`src/tui/drawn_wider.rs` lists at the head), as tmux 3.7b and libghostty-vt
1.3.2 hold them, under fx, the base's xfx and the head's.

usage: python3 -I measure.py <receipt-dir> <replay_ghostty>

Reads `fx/`, `red/` (e6cde019) and `xfx/` (the head's code) under
<receipt-dir>, each `<scenario>-40x120/<prefix>-<scenario>-40x120-<snap>.txt`
(tmux's `capture-pane`) and `.wire` (every byte the binary wrote, capture.sh
`wire`), and runs <replay_ghostty> (replay_ghostty.c, built against
libghostty-vt) on each wire. For each form below it prints the screen row
that holds it, on each terminal, under each build, and whether the row reads
as the text does: every glyph of the text, in order, and nothing in place of
one (`ok`), or what is missing (`LOST ...`). A token `\\u{NNNN}` is the glyph it
names, and a cell a terminal draws as nothing (U+E0002, U+FE0F, U+2065, the
Hangul filler U+FFA0, U+FFF0, which libghostty-vt draws as no cell) is not
asked for. A form laid out on two rows by the count is read on both.
"""
import difflib
import os
import re
import subprocess
import sys

receipt, replay = sys.argv[1:3]
BUILDS = (("fx", "fx", "fx"), ("red", "xfx", "xfx e6cde01"), ("xfx", "xfx", "xfx head"))
W = lambda n: "w" * n  # noqa: E731
SHORT = "fam \u261d \u2e3a \U0001f590 x\U000e0002y a\ufe0fb \u00ad\ufe0fc \uffa0\ufe0fd \ufff0\ufe0fe \u2065f end"
# (scenario, snap, the form's name, the text, where its row starts -- a regex
# on the row -- and how many rows the count lays it out on)
FORMS = [
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O read row, short", SHORT, r"^│  2    ", 1),
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O diff row, short", SHORT, r"^  │ 2   ", 1),
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O read row, U+261D at the margin", W(111) + "\u261dTAIL5678", r"^│  3    ", 2),
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O read row, `a` U+FE0F at the margin", W(111) + "a\ufe0fTAIL5678", r"^│  4    ", 2),
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O read row, four U+261D in front", "\u261d" * 4 + W(100) + "TAIL" + W(6) + "END", r"^│  5    ", 2),
    ("p5-ctrlo-wider-diff-fam", "full", "Ctrl-O diff row, four U+261D in front", "\u261d" * 4 + W(100) + "TAIL" + W(6) + "END", r"^  │ 5   ", 2),
    ("p5-ctrlo-wider-inline-fam", "answer", "answer, short", SHORT, r"^  fam ", 1),
    ("p5-ctrlo-wider-inline-fam", "answer", "answer, U+261D at the margin", W(117) + "\u261dTAIL5678", r"^  w{117}", 2),
    ("p5-ctrlo-wider-inline-fam", "answer", "answer, U+2E3A at the margin", W(117) + "\u2e3aTAIL5678", r"^  w{117}", 2),
    ("p5-ctrlo-wider-inline-fam", "answer", "answer, `a` U+FE0F at the margin", W(117) + "a\ufe0fTAIL5678", r"^  w{117}", 2),
    ("p5-ctrlo-wider-inline-fam", "answer", "answer, four U+261D in front", "\u261d" * 4 + W(110) + "TAIL", r"^  \u261d", 1),
    ("p5-ctrlo-wider-inline-fam", "composer", "composer, short", SHORT, r"^┃ fam ", 1),
    ("p5-ctrlo-wider-inline-fam", "margin", "composer, four U+261D in front", "\u261d" * 4 + W(110) + "TAIL", r"^┃ \u261d", 1),
    ("p5-ctrlo-wider-inline-fam", "margin", "composer, U+261D at the margin", W(117) + "\u261dTAIL", r"^┃ w{117}", 2),
]
UNDRAWN = set("\U000e0002\ufe0f\u2065")
GHOSTTY_UNDRAWN = UNDRAWN | set("\uffa0\ufff0")
TOKEN = re.compile(r"\\u\{([0-9a-f]{4,6})\}")


def screens(build, prefix, scenario, snap):
    base = os.path.join(receipt, build, f"{scenario}-40x120", f"{prefix}-{scenario}-40x120-{snap}")
    with open(base + ".txt", encoding="utf-8") as f:
        tmux = f.read().split("\n")
    out = subprocess.run([replay, "120", "40", base + ".wire"], capture_output=True, text=True, check=True).stdout
    ghostty = out.split("--- cells")[0].split("\n")
    return {"tmux": tmux, "libghostty-vt": ghostty}


def glyphs(text, undrawn):
    text = TOKEN.sub(lambda m: chr(int(m.group(1), 16)), text)
    return [c for c in text if c not in undrawn]


def read_row(rows, start, count, used):
    """The row the form starts on -- the next one the pattern finds that an
    earlier form of the same screen did not take -- and the `count - 1` rows
    after it, joined, their gutters taken off."""
    pattern = re.compile(start)
    gutters = re.compile(r"^(?:│  (?:\d+ {4})?|  │ (?:\d+ [ +-] |    )|┃ |  )")
    for i, row in enumerate(rows):
        if i in used or not pattern.search(row):
            continue
        used.add(i)
        text = "".join(gutters.sub("", r, count=1) for r in rows[i:i + count])
        return row, text
    return None, ""


def verdict(want, got):
    """`ok` when `got` holds every glyph of `want` in order; else the glyphs
    of `want` that the longest alignment of the two leaves unmatched."""
    matcher = difflib.SequenceMatcher(None, want, got, autojunk=False)
    matched = set()
    for block in matcher.get_matching_blocks():
        matched.update(range(block.a, block.a + block.size))
    lost = [c for i, c in enumerate(want) if i not in matched]
    if not lost:
        return "ok"
    return "LOST " + " ".join("U+%04X" % ord(c) if ord(c) > 0x7e else "space" if c == " " else c for c in lost)


current = None
for scenario, snap, name, text, start, count in FORMS:
    if (scenario, snap) != current:
        current = (scenario, snap)
        print(f"\n## {scenario} `{snap}`")
        shots = {label: screens(build, prefix, scenario, snap) for build, prefix, label in BUILDS}
        used = {(label, term): set() for label in shots for term in ("tmux", "libghostty-vt")}
    print(f"\n{name}: {ascii(text)}")
    for label, by in shots.items():
        for term, rows in by.items():
            row, got = read_row(rows, start, count, used[(label, term)])
            undrawn = GHOSTTY_UNDRAWN if term == "libghostty-vt" else UNDRAWN
            want = "".join(glyphs(text, undrawn))
            have = "".join(glyphs(got, undrawn))
            v = verdict(want, have) if row is not None else "NO ROW"
            print(f"  {label:12s} {term:14s} {v:30s} {got[:60]!r}{'...' if len(got) > 60 else ''}")
