"""The family's rows (the 43 scalars `src/tui/drawn_wider.rs` lists at the
head) as tmux 3.7b and libghostty-vt 1.3.2 hold them, under each build.

usage: python3 -I measure.py <receipt-dir> <replay_ghostty> [--check <build>]

Reads `<build>/<scenario>-40x120/<prefix>-<scenario>-40x120-<snap>.txt`
(tmux's `capture-pane`) and `.wire` (every byte the binary wrote, capture.sh
`wire`) for each build present among `fx` (v0.0.16), `red` (e6cde019),
`red2` (88c2fec1), `red3` (08026927, whose code is CTRLO-WIDER's head
eeb0f7a7's, before CTRLO-WIDER-F) and `xfx` (the head), and runs
<replay_ghostty>
(replay_ghostty.c) on each wire -- for `resized` and `ctrlo-resized`, with the
terminal resized to 140x40 where the `margin` wire ended (capture.sh `resize`
came next). fx writes the document again at the new width; xfx leaves the rows
it wrote to the terminal (the ledger's `resize-document-repaint`), so on xfx
`resized` asks whether the rows written at 120 columns still read whole, and
`ctrlo-resized` is the screen laid out again. For each form below it prints the rows that hold it, on each
terminal, and whether they read as the text does: every glyph of the text, in
order (`ok`), or the glyphs of the text the longest alignment leaves unmatched
(`LOST ...`). A token `\\u{NNNN}` reads as the glyph it names; a cell a
terminal draws as nothing (U+E0002, U+FE0F, U+2065; and U+FFA0 and U+FFF0 on
libghostty-vt, which draws them as no cell) is not asked for.

A form's rows are the row its pattern finds -- the next one an earlier form
of the same screen did not take -- and the rows after it: `rows` of them, or,
for `block`, every row up to the next blank one.

--check <build> exits 1 if any form whose surface is checked (an answer, in
the document and on Ctrl-O, before and after the resize; Ctrl-O's prompt and
file lines) reads otherwise than `ok` on either terminal under <build>. The
composer and the card of a sent prompt are the user's own text, measured and
not checked (the ledger's `composer-drawn-wider`).
"""
import difflib
import os
import re
import subprocess
import sys

receipt, replay = sys.argv[1:3]
check = sys.argv[4] if len(sys.argv) > 4 and sys.argv[3] == "--check" else None
BUILDS = (("fx", "fx", "fx"), ("red", "xfx", "xfx e6cde01"), ("red2", "xfx", "xfx 88c2fec"), ("red3", "xfx", "xfx 0802692"), ("xfx", "xfx", "xfx head"))
W = lambda n: "w" * n  # noqa: E731
SHORT = "fam \u261d \u2e3a \U0001f590 x\U000e0002y a\ufe0fb \u00ad\ufe0fc \uffa0\ufe0fd \ufff0\ufe0fe \u2065f end"
FOUR = "\u261d" * 4
B = "block"
DIFF = "p5-ctrlo-wider-diff-fam"
INLINE = "p5-ctrlo-wider-inline-fam"
# (scenario, snap, surface, checked, the form's name, the text, the row's pattern, rows)
FORMS = [
    (DIFF, "full", "Ctrl-O file line", True, "read row, short", SHORT, r"^│  2    ", 1),
    (DIFF, "full", "Ctrl-O file line", True, "diff row, short", SHORT, r"^  │ 2   ", 1),
    (DIFF, "full", "Ctrl-O file line", True, "read row, U+261D at the margin", W(111) + "\u261dTAIL5678", r"^│  3    ", 2),
    (DIFF, "full", "Ctrl-O file line", True, "read row, `a` U+FE0F at the margin", W(111) + "a\ufe0fTAIL5678", r"^│  4    ", 2),
    (DIFF, "full", "Ctrl-O file line", True, "read row, four U+261D in front", FOUR + W(100) + "TAIL" + W(6) + "END", r"^│  5    ", 2),
    (DIFF, "full", "Ctrl-O file line", True, "diff row, four U+261D in front", FOUR + W(100) + "TAIL" + W(6) + "END", r"^  │ 5   ", 2),
]
ANSWER = [
    ("short", SHORT, r"^  fam "),
    ("U+261D at the margin", W(117) + "\u261dTAIL5678", r"^  w{110}"),
    ("U+2E3A at the margin", W(117) + "\u2e3aTAIL5678", r"^  w{110}"),
    ("`a` U+FE0F at the margin", W(117) + "a\ufe0fTAIL5678", r"^  w{110}"),
    ("U+E0002 `z` at the margin", W(117) + "\U000e0002zTAIL5678", r"^  w{110}"),
    ("four U+261D in front", FOUR + W(110) + "TAIL" + W(20) + "END", r"^  (?:\u261d|\\u\{261d\})"),
]
for snap, surface in (("answer", "answer"), ("ctrlo", "Ctrl-O Response"), ("resized", "answer, 140 columns"), ("ctrlo-resized", "Ctrl-O Response, 140 columns")):
    for name, text, start in ANSWER:
        FORMS.append((INLINE, snap, surface, True, name, text, start, B))
    if snap.startswith("ctrlo"):
        FORMS.append((INLINE, snap, "Ctrl-O Prompt", True, "short", SHORT, r"^┃ fam ", B))
# CTRLO-WIDER-F: a code panel's label (the fence's word), and the nine emoji
# bases of the family with a skin-tone modifier.
FENCE = "p5-ctrlo-wider-fence"
LABEL = "fam\u261d\u2e3a\U0001f590x\U000e0002ya\ufe0fb\u00ad\ufe0fc"
for snap, surface in (("answer", "code panel's label"), ("ctrlo", "Ctrl-O code panel's label")):
    FORMS.append((FENCE, snap, surface, True, "short", LABEL, r"^  ─ fam", 1))
    FORMS.append((FENCE, snap, surface, True, "four U+261D", FOUR + "TAIL", r"^  ─ (?:\u261d|\\u\{261d\})", 1))
MOD = "p5-ctrlo-wider-mod"
BASES = "\u261d\u26f9\u270c\u270d\U0001f3cb\U0001f3cc\U0001f574\U0001f575\U0001f590"
TONES = "\U0001f3fb\U0001f3fc\U0001f3fd\U0001f3fe\U0001f3ff"
PAIRS = [b + TONES[i % 5] for i, b in enumerate(BASES)]
TONED = " ".join(PAIRS) + " end"
FORMS.append((MOD, "answer", "answer", True, "with modifiers, short", "mod " + TONED, r"^  mod ", B))
for pair in PAIRS:
    named = 8 if ord(pair[0]) <= 0xFFFF else 9
    FORMS.append((MOD, "answer", "answer", True, "U+%04X U+%04X at the margin" % (ord(pair[0]), ord(pair[1])),
                  W(118 - named - 2) + pair + "TAIL5678", r"^  w{107,108}(?!w)", B))
for other in ("\U0001f46f\U0001f3fb", "\U0001f93c\U0001f3fb"):
    FORMS.append((MOD, "more", "answer, not the family", False, "U+%04X U+%04X at the margin" % (ord(other[0]), ord(other[1])),
                  W(110) + other + "TAIL5678", r"^  w{110}(?!w)", B))
FORMS.append((MOD, "more", "code panel", True, "with modifiers, short", "code " + TONED, r"^  code ", 1))
FORMS.append((MOD, "more", "table cell", True, "with modifiers, short", "cell " + TONED, r"^  │ cell (?:\u261d|\\u)", 1))
FORMS += [
    (INLINE, "composer", "composer", False, "short", SHORT, r"^┃ fam ", 1),
    (INLINE, "prompt", "card of the sent prompt", False, "short", SHORT, r"^┃ fam ", 1),
    (INLINE, "margin", "composer", False, "four U+261D in front", FOUR + W(110) + "TAIL", r"^┃ (?:\u261d|\\u\{261d\})", 1),
    (INLINE, "margin", "composer", False, "U+261D at the margin", W(117) + "\u261dTAIL", r"^┃ w{110}", 2),
]
ORDER = {s: i for i, s in enumerate(["full", "answer", "composer", "prompt", "ctrlo", "margin", "resized", "ctrlo-resized", "more"])}
FORMS.sort(key=lambda f: (f[0], ORDER[f[1]]))
UNDRAWN = set("\U000e0002\ufe0f\u2065")
GHOSTTY_UNDRAWN = UNDRAWN | set("\uffa0\ufff0")
TOKEN = re.compile(r"\\u\{([0-9a-f]{4,6})\}")
GUTTER = re.compile(r"^(?:│  (?:\d+ {4})?|  │ (?:\d+ [ +-] |    )|┃ |  )")
RESIZED = ("resized", "ctrlo-resized")


def screens(build, prefix, scenario, snap):
    base = os.path.join(receipt, build, f"{scenario}-40x120", f"{prefix}-{scenario}-40x120-{snap}")
    if not os.path.exists(base + ".txt"):
        return None
    with open(base + ".txt", encoding="utf-8") as f:
        tmux = f.read().split("\n")
    args = [replay, "120", "40", base + ".wire"]
    if snap in RESIZED:
        margin = os.path.join(receipt, build, f"{scenario}-40x120", f"{prefix}-{scenario}-40x120-margin.wire")
        args += [str(os.path.getsize(margin)), "140", "40"]
    out = subprocess.run(args, capture_output=True, text=True, check=True).stdout
    return {"tmux": tmux, "libghostty-vt": out.split("--- cells")[0].split("\n")}


def glyphs(text, undrawn):
    text = TOKEN.sub(lambda m: chr(int(m.group(1), 16)), text)
    return "".join(c for c in text if c not in undrawn)


def read_rows(rows, start, count, used):
    pattern = re.compile(start)
    for i, row in enumerate(rows):
        if i in used or not pattern.search(row):
            continue
        used.add(i)
        if count == B:
            j = i + 1
            while j < len(rows) and rows[j].strip() and not rows[j].startswith("  20"):
                j += 1
            taken = rows[i:j]
        else:
            taken = rows[i:i + count]
        return row, "".join(GUTTER.sub("", r, count=1) for r in taken)
    return None, ""


def verdict(want, got):
    matcher = difflib.SequenceMatcher(None, want, got, autojunk=False)
    matched = set()
    for block in matcher.get_matching_blocks():
        matched.update(range(block.a, block.a + block.size))
    lost = [c for i, c in enumerate(want) if i not in matched]
    if not lost:
        return "ok"
    return "LOST " + " ".join("U+%04X" % ord(c) if ord(c) > 0x7e else "space" if c == " " else c for c in lost)


failures = []
current = None
for scenario, snap, surface, checked, name, text, start, count in FORMS:
    if (scenario, snap) != current:
        current = (scenario, snap)
        print(f"\n## {scenario} `{snap}`")
        shots = {}
        for build, prefix, label in BUILDS:
            got = screens(build, prefix, scenario, snap)
            if got is not None:
                shots[(build, label)] = got
        used = {(key, term): set() for key in shots for term in ("tmux", "libghostty-vt")}
    print(f"\n{surface}, {name}{'' if checked else ' (not checked)'}: {ascii(text)}")
    for key, by in shots.items():
        for term, rows in by.items():
            row, got = read_rows(rows, start, count, used[(key, term)])
            undrawn = GHOSTTY_UNDRAWN if term == "libghostty-vt" else UNDRAWN
            v = verdict(glyphs(text, undrawn), glyphs(got, undrawn)) if row is not None else "NO ROW"
            print(f"  {key[1]:12s} {term:14s} {v:30s} {got[:60]!r}{'...' if len(got) > 60 else ''}")
            if checked and key[0] == check and v != "ok":
                failures.append(f"{surface}, {name}, `{snap}`, {term}: {v}")
if check:
    print(f"\n# --check {check}: {len(failures)} form(s) not whole")
    for f in failures:
        print("  " + f)
    sys.exit(1 if failures else 0)
