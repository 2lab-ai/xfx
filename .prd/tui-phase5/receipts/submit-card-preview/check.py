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
           but three kinds set aside and printed: a banner row (each product's
           own, or each build's), the band's activity row -- fx writes `•
           Thinking` with the prompt and xfx once the runtime says the turn
           started, its clock, its counts and its pulse its own (T7,
           `.prd/tui-phase5/receipts/transcript-receipts/README.md`) -- and a
           row that differs only in a turn's wall clock (`<n>s (`, the
           ledger's `turn-wall-time`).
  frame2   the same for the second frame, the one that writes the card.
  card     (with <base-side>) the frame that writes the card leaves the
           screen the base's first frame left: the head's second frame where
           its first is a preview, its first where it is not (a card that
           fits) -- every row, the same three kinds set aside.
  later    (with <base-side>) the frame after the card's leaves the screen
           the base's frame after its card left, the same three kinds set
           aside; whether its bytes are the base's is printed, not claimed
           (they carry the activity row's clock and counts).
  snaps    (with <base-side>) every snap of the scenario, `.full` histories
           included, the head's against the base's: every row the same, the
           same three kinds set aside.
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


def aside(tx, ty):
    """Why two rows that differ are set aside, or None."""
    if BANNER.match(tx) and BANNER.match(ty):
        return "banner"
    if ACTIVITY.match(tx) and ACTIVITY.match(ty):
        return "activity"
    if WALL.sub("<s> (", tx) == WALL.sub("<s> (", ty):
        return "wall clock"
    return None


def same_screen(a_path, b_path):
    a, b = screen(a_path, False), screen(b_path, False)
    return all(x == y or aside(cells.text_of(x), cells.text_of(y))
               for x, y in zip(a + [[]] * (len(b) - len(a)), b + [[]] * (len(a) - len(b))))


def compare(what, a_path, b_path, a_fx, b_fx):
    a, b = screen(a_path, a_fx), screen(b_path, b_fx)
    kept, differ = [], []
    for i in range(max(len(a), len(b))):
        x = a[i] if i < len(a) else []
        y = b[i] if i < len(b) else []
        if x == y:
            continue
        tx, ty = cells.text_of(x), cells.text_of(y)
        why = aside(tx, ty)
        (kept if why else differ).append((i + 1, tx, ty, why))
    verdict = "ok" if not differ else "FAIL"
    print("%-6s %s: %d of %d rows differ, %d set aside" % (verdict, what, len(differ), max(len(a), len(b)), len(kept)))
    for row, tx, ty, why in kept:
        print("         set aside (%s) row %d: %r | %r" % (why, row, tx, ty))
    for row, tx, ty, _ in differ:
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


fx_dir = os.path.join(receipt, "fx")
for before in sorted(glob.glob(os.path.join(fx_dir, "*-before*.wire"))):
    name = os.path.basename(before)
    m = re.match(r"fx-(.+)-(\d+x\d+)-(?:(.+)-)?before(-submit)?\.wire$", name)
    scenario, size, pair = m.group(1), m.group(2), (m.group(3) or "submit")
    stem = name[: name.index("before")].rstrip("-")
    unit = "%s %s" % (scenario, pair) if size == "40x120" else "%s %s %s" % (scenario, size, pair)

    def path(who, k):
        return os.path.join(receipt, who, "%s-%s-frame%d.ansi" % ("xfx", stem[3:], k)) \
            if who != "fx" else os.path.join(fx_dir, "%s-frame%d.ansi" % (stem, k))

    for k in (1, 2):
        compare("%s frame%d" % (unit, k), path("fx", k), path(side, k), True, False)
    if base:
        # A pair whose card fits has no preview: its card is the first frame
        # on both builds, and the frames line up one for one.
        shift = 0 if same_screen(path(base, 1), path(side, 1)) else 1
        compare("%s card" % unit, path(base, 1), path(side, 1 + shift), False, False)
        # The frame after the card's, against the base's frame after its own:
        # the screen it leaves, and whether its bytes are the base's.
        later = path(side, 2 + shift), path(base, 2)
        if all(os.path.exists(p) for p in later):
            compare("%s later" % unit, later[1], later[0], False, False)
            a, b = (open(p.replace(".ansi", ".bytes"), "rb").read() for p in later)
            print("         its bytes: %s" % ("the base's" if a == b else "not the base's (%d against %d)" % (len(a), len(b))))

for scenario, size in sorted({re.match(r"fx-(.+)-(\d+x\d+)-", os.path.basename(p)).groups()
                              for p in glob.glob(os.path.join(fx_dir, "fx-*-*x*-*.wire"))}):
    named = scenario if size == "40x120" else "%s %s" % (scenario, size)
    prefix = "xfx-%s-%s-" % (scenario, size)
    for full in sorted(glob.glob(os.path.join(receipt, side, prefix + "*.full.ansi"))):
        label = os.path.basename(full)[len(prefix): -len(".full.ansi")]
        if label in ("block", "tall"):
            whole("%s %s.full whole" % (named, label), full)
    if base:
        for snap in sorted(glob.glob(os.path.join(receipt, base, prefix + "*.ansi"))):
            label = os.path.basename(snap)[len(prefix): -len(".ansi")]
            if re.search(r"frame\d|wireend", label):
                continue
            compare("%s %s snap" % (named, label), snap, os.path.join(receipt, side, os.path.basename(snap)), False, False)

print("\n%d claims failed: %s" % (len(failed), ", ".join(failed) if failed else "none"))
sys.exit(1 if failed else 0)
