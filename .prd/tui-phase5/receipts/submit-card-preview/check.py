"""SUBMIT-CARD-PREVIEW's receipt checker: the first frames after Enter on fx
v0.0.16 (70611a91) against xfx's, cell by cell with their styles.

usage: python3 -I -B check.py <repo> <receipt-dir> <side> [<base-side>]

Reads `fx/` and `<side>/` (`red/`, the base's build, or `xfx/`, the head's):
`<side>/<prefix>-<scenario>-40x120-<pair>-frame<k>.ansi`, the screens
`frames.py` replays from the wires kept on both sides of an Enter, and the
snaps the captures took. Each row is read as `scripts/fxcap/cells.py` reads it.
<base-side> (`red`) adds the claims that compare the head with the base.

The claims, for every wire pair fx's side holds:

  frame1   the screen once the first frame after Enter has landed, fx's
           against xfx's: every row the same, cell by cell with its styles,
           but two kinds set aside and printed: a banner row (each product's
           own) and the band's activity row -- fx writes `• Thinking` with the
           prompt and xfx once the runtime says the turn started, its clock and
           its counts its own (T7, `.prd/tui-phase5/receipts/transcript-
           receipts/README.md`).
  frame2   the same for the second frame, the one that writes the card.
  card     (with <base-side>) the head's second frame leaves the screen the
           base's first frame left: the frame that writes the card is the
           frame it was, a frame later -- every row, the activity row set
           aside as above.
  snaps    (with <base-side>) every snap of the scenario, `.full` histories
           included, the head's against the base's: every row the same, a
           turn's wall clock (`<n>s (`) read as one.
  whole    (a 60-line block) the `.full` history after it holds the card's 60
           rows -- `TALL_PROMPT_HEAD`, `tall prompt body 01`..`58`,
           `TALL_PROMPT_TAIL` -- once each, in order, one after another: no
           preview row reached the terminal's scrollback.
"""
import glob
import os
import re
import sys

repo, receipt, side = sys.argv[1:4]
base = sys.argv[4] if len(sys.argv) > 4 else None
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import cells  # noqa: E402

BANNER = re.compile(r"^x?𝒇x v")
ACTIVITY = re.compile(r"^\s*[•~]?\s*(Thinking|Generating)\b")
WALL = re.compile(r"\b\d+s \(")
TALL = ["TALL_PROMPT_HEAD"] + ["tall prompt body %02d" % n for n in range(1, 59)] + ["TALL_PROMPT_TAIL"]
failed = []


def screen(path, fx):
    return cells.rows(path, fx)


def compare(what, a_path, b_path, a_fx, b_fx):
    a, b = screen(a_path, a_fx), screen(b_path, b_fx)
    aside, differ = [], []
    for i in range(max(len(a), len(b))):
        x = a[i] if i < len(a) else []
        y = b[i] if i < len(b) else []
        if x == y:
            continue
        tx, ty = cells.text_of(x), cells.text_of(y)
        if (BANNER.match(tx) and BANNER.match(ty)) or (ACTIVITY.match(tx) and ACTIVITY.match(ty)):
            aside.append((i + 1, tx, ty))
            continue
        differ.append((i + 1, tx, ty))
    verdict = "ok" if not differ else "FAIL"
    print("%-6s %s: %d rows differ, %d set aside" % (verdict, what, len(differ), len(aside)))
    for row, tx, ty in aside:
        print("         set aside row %d: %r | %r" % (row, tx, ty))
    for row, tx, ty in differ:
        print("         row %d: %r | %r" % (row, tx, ty))
    if differ:
        failed.append(what)


BADGE = re.compile(r"^\[Image \d+\]")


def whole(what, path):
    # A card row's text behind its rail, an image's badge in front of the
    # first line read off (`p5-submit-preview-notice` attaches one).
    texts = [BADGE.sub("", cells.text_of(r)[2:]) if cells.text_of(r).startswith("┃ ") else None
             for r in screen(path, False)]
    card = [t for t in texts if t in TALL]
    at = [n for n, t in enumerate(texts) if t in TALL]
    ok = card == TALL and at == list(range(at[0], at[0] + len(TALL))) if at else False
    print("%-6s %s: %d card rows, %s" % ("ok" if ok else "FAIL", what, len(card),
          "once each, in order, rows %d-%d" % (at[0] + 1, at[-1] + 1) if ok else "not the card once in order"))
    if not ok:
        failed.append(what)


def snaps(what, a_path, b_path):
    a, b = screen(a_path, False), screen(b_path, False)
    differ = []
    for i in range(max(len(a), len(b))):
        x = a[i] if i < len(a) else []
        y = b[i] if i < len(b) else []
        if x != y and WALL.sub("<s> (", cells.text_of(x)) != WALL.sub("<s> (", cells.text_of(y)):
            differ.append((i + 1, cells.text_of(x), cells.text_of(y)))
    print("%-6s %s: %d of %d rows differ" % ("ok" if not differ else "FAIL", what, len(differ), max(len(a), len(b))))
    for row, tx, ty in differ:
        print("         row %d: %r | %r" % (row, tx, ty))
    if differ:
        failed.append(what)


fx_dir = os.path.join(receipt, "fx")
for before in sorted(glob.glob(os.path.join(fx_dir, "*-before*.wire"))):
    name = os.path.basename(before)
    m = re.match(r"fx-(.+)-40x120-(?:(.+)-)?before(-submit)?\.wire$", name)
    scenario, pair = m.group(1), (m.group(2) or "submit")
    stem = name[: name.index("before")].rstrip("-")
    unit = "%s %s" % (scenario, pair)

    def path(who, k):
        return os.path.join(receipt, who, "%s-%s-frame%d.ansi" % (who if who != "red" else "xfx", stem[3:], k)) \
            if who != "fx" else os.path.join(fx_dir, "%s-frame%d.ansi" % (stem, k))

    for k in (1, 2):
        compare("%s frame%d" % (unit, k), path("fx", k), path(side, k), True, False)
    if base:
        compare("%s card" % unit, path(base, 1), path(side, 2), False, False)

for scenario in sorted({re.match(r"fx-(.+)-40x120-", os.path.basename(p)).group(1)
                        for p in glob.glob(os.path.join(fx_dir, "fx-*-40x120-*.wire"))}):
    for full in sorted(glob.glob(os.path.join(receipt, side, "xfx-%s-40x120-*.full.ansi" % scenario))):
        label = os.path.basename(full)[len("xfx-%s-40x120-" % scenario): -len(".full.ansi")]
        if label in ("block", "tall"):
            whole("%s %s.full whole" % (scenario, label), full)
    if base:
        for snap in sorted(glob.glob(os.path.join(receipt, base, "xfx-%s-40x120-*.ansi" % scenario))):
            label = os.path.basename(snap)[len("xfx-%s-40x120-" % scenario): -len(".ansi")]
            if re.search(r"frame\d|wireend", label):
                continue
            snaps("%s %s snap" % (scenario, label), snap, os.path.join(receipt, side, os.path.basename(snap)))

print("\n%d claims failed: %s" % (len(failed), ", ".join(failed) if failed else "none"))
sys.exit(1 if failed else 0)
