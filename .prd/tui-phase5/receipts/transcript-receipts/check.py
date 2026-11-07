"""T-U6's receipt checker: fx v0.0.16's T7, T8 and T10 (transcript.md §2),
each claim checked against the captures, cell by cell with their styles.

usage: python3 -I -B check.py <repo> <receipt-dir> [xfx|red|red-u6b]

Reads `fx/` and, for xfx's side, `xfx/` (the build under the receipt),
`red/` (the base's) or `red-u6b/` (T-U6's, before T-U6b): `<side>/<scenario>-40x120/<prefix>-<scenario>-40x120-
<snap>.ansi`, and the `frame<k>`/`wireend` screens `frames.py` replays from
the wires. Each row is read as `scripts/fxcap/cells.py` reads it; where a claim
compares two sides, the parity ledger's normalizations for each side are
applied first (`banner`, `turn-wall-time`, `activity-pulse`, ...), and nothing
else is set aside unless the claim names it.

The claims (SPEC below):

  build    xfx's wire holds the banner of the build the side names: the
           frames are that binary's.
  replay   the whole after-wire, replayed (`wireend`), is the `second` snap
           taken just before the wire was copied: every row, as captured. The
           replay is the capture's terminal.
  frame    T7: on each side, the screen frame k after Enter leaves: the
           composer row (38) is `┃` and nothing else, and the rows that start
           `┃ ` above it are exactly the claim's card lines, in order, one
           blank row under them and one over them, the previous turn's summary
           above that.
  same     T7: fx's screen after the frame that completes its card against
           xfx's (and the snap 0.15 s after Enter), aligned at the card: the
           rows above it and its rows the same, and the composer's rows
           (38-40). The rows between the card and the composer are set aside
           and printed: the band's activity row, which fx writes with the card
           (`• Thinking`, its clock a frame later) and xfx once the runtime
           says the turn started -- in the same frame or the next, as the
           thread hands it over; not the card's.
  inline   a snap: every row the same on both sides.
  whole    T8: the tall card's 60 rows (`TALL_PROMPT_HEAD`, `tall prompt body
           01`..`58`, `TALL_PROMPT_TAIL`) in the whole history (`.full`),
           each once, in order, one after another; under the banner and one
           blank row for the session's first prompt, else under the previous
           turn's summary and one blank row.
  spelled  T10: every diff row of a snap (Ctrl-O's `  │ N ± text`, or the
           review's `    N ± text` between its rules) is the fixture's line N
           -- the file before the edit for ` ` and `-`, after it for `+` --
           spelled as the claim's spelling says: fx's on fx's side; on xfx's,
           fx's in Ctrl-O (T-U6b; and U+061C named, `ctrl-o-arabic-letter-
           mark`) and xfx's own in the review (`review-diff-spelling`)
           (`fx_spelled`, `xfx_spelled` below); the rows are exactly the
           window's lines; no cell of one holds a control or a code point fx
           names (`hidden`). A row whose text is the same on both sides is the
           same cell by cell, styles included; one spelled differently has the
           same styles as fx's.
  bytes    T10 (T-U6b): Ctrl-O's diff rows -- or the ones the claim lists --
           each the same bytes on both sides as the capture holds them
           (`capture-pane -e`, colours and all), and printed. The width
           scenario (T-U6c) is captured on fx and xfx only, so its claim is
           not run against an earlier build.
  refused  T10: xfx draws no diff for an edit of a file that is not UTF-8: no
           diff row, and the call's row says it failed.
  nodiff   T10: the inline transcript draws no diff of a change, on either
           side: no diff row, and the call's row `└ Edited <file> +1 / -1`
           (fx v0.0.16's own inline transcript draws none either).

What a claim does not cover and the screens show anyway is printed after the
claims as findings: no claim's result depends on them.

Exit 0 when every claim holds.
"""

import os
import re
import sys
import unicodedata

repo = os.path.realpath(sys.argv[1])
receipt = os.path.realpath(sys.argv[2])
xside = sys.argv[3] if len(sys.argv) > 3 else "xfx"
assert xside in ("xfx", "red", "red-u6b"), xside
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

LEDGER = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
RULES = {"fx": ledgermod.rules_for(LEDGER, "fx"), "xfx": ledgermod.rules_for(LEDGER, "xfx")}
BUILD = {"xfx": "f84734d", "red": "039fb5c", "red-u6b": "4aca257"}
SCEN = os.path.join(repo, "scripts", "fxcap", "scenarios")
text = cells.text_of


class Malformed(Exception):
    """A capture that is not the shape a claim reads: the claim fails."""


class Claim:
    def __init__(self, name):
        self.name, self.lines, self.ok = name, [], True

    def say(self, line):
        self.lines.append(line)

    def fail(self, line):
        self.ok = False
        self.lines.append("FAIL " + line)


def where(side, scenario):
    return os.path.join(receipt, side if side == "fx" else xside, scenario + "-40x120")


def path(side, scenario, snap, ext="ansi"):
    prefix = "fx" if side == "fx" else "xfx"
    found = os.path.join(where(side, scenario), "%s-%s-40x120-%s.%s" % (prefix, scenario, snap, ext))
    if not os.path.exists(found):
        raise Malformed("missing capture: %s" % os.path.relpath(found, receipt))
    return found


def raw_rows(side, scenario, snap):
    return cells.parse(open(path(side, scenario, snap), encoding="utf-8").read())


def rows_of(side, scenario, snap):
    return [cells.normalize_row(r, RULES["fx" if side == "fx" else "xfx"]) for r in raw_rows(side, scenario, snap)]


def side_name(side):
    return "fx" if side == "fx" else xside


# --- the two products' spellings of a file's line ----------------------------

# fx's `isNonPrintingCodepoint` (`src/core/shared/text_utils.zig:703-708@481104fd`).
def fx_nonprinting(cp):
    return 0x80 <= cp <= 0x9F or 0x200B <= cp <= 0x200F or 0x2028 <= cp <= 0x202E or 0x2060 <= cp <= 0x206F or cp == 0xFEFF


def hidden(ch):
    """A character a terminal would act on, or could draw as nothing: a
    control, or one fx names (`isNonPrintingCodepoint`)."""
    return unicodedata.category(ch) == "Cc" or fx_nonprinting(ord(ch))


def fx_spelled(raw, col, alm=False):
    """`appendTerminalSafeLine` (`text_utils.zig:419-442@481104fd`): a
    trailing CR dropped; a tab kept (drawn here as the terminal draws it, to
    the next multiple of 8 from column `col`, where the text starts); a C0
    control or DEL `\\xNN`; a byte outside a valid UTF-8 sequence `\\xNN`; a
    code point `isNonPrintingCodepoint` names `\\u{NNNN}` (four lowercase hex
    digits at least, `writeCodepointEscape`); all else as itself
    (`terminalSafeToken`, `:643-673`)."""
    if raw.endswith(b"\r"):
        raw = raw[:-1]
    out, i = "", 0
    while i < len(raw):
        b = raw[i]
        if b == 0x09:
            if any(ord(c) > 0x7E for c in out):
                raise Malformed("a tab after a wide or unknown character: no rule for its column")
            out += " " * (8 - (col + len(out)) % 8)
            i += 1
        elif b < 0x20 or b == 0x7F:
            out += "\\x%02x" % b
            i += 1
        elif b < 0x80:
            out += chr(b)
            i += 1
        else:
            n = 2 if b >> 5 == 0b110 else 3 if b >> 4 == 0b1110 else 4 if b >> 3 == 0b11110 else 0
            try:
                ch = raw[i : i + n].decode("utf-8") if n and i + n <= len(raw) else None
            except UnicodeDecodeError:
                ch = None
            if ch is None:
                out += "\\x%02x" % b
                i += 1
            else:
                named = fx_nonprinting(ord(ch)) or (alm and ch == "\u061c")
                out += "\\u{%04x}" % ord(ch) if named else ch
                i += n
    return out


def xfx_spelled(raw):
    """The permission boundary's `bounded_diff_side` (`src/permission/
    authority.rs`, `Escaped::of`): a backslash doubled, a tab `\\t`, a CR
    `\\r`, every other control (`char::is_control`, category Cc) by its code
    point, `\\u{NNNN}` in upper case (`scalar_token`); then the review's
    `approval_view::tamed`, which names by code point what would reorder or
    hide the text -- here the code points fx's set holds. Any other
    non-ASCII character is refused rather than guessed: the fixtures hold
    none."""
    out = ""
    for ch in raw.decode("utf-8"):
        cp = ord(ch)
        if ch == "\\":
            out += "\\\\"
        elif ch == "\t":
            out += "\\t"
        elif ch == "\r":
            out += "\\r"
        elif unicodedata.category(ch) == "Cc" or fx_nonprinting(cp):
            out += "\\u{%04X}" % cp
        elif cp > 0x7E:
            raise Malformed("%r: this checker has no xfx spelling for it" % ch)
        else:
            out += ch
    return out


# --- claims --------------------------------------------------------------------

def check_build(scenario):
    claim = Claim("build %s (%s)" % (scenario, side_name("xfx")))
    # The banner as xfx writes it: its colours between the name, the version
    # and the build's mark.
    sgr = r"(?:\x1b\[[0-9;]*m)*"
    want = re.compile(("x𝒇x%s v0\\.1\\.0-%s %s\\[debug\\]" % (sgr, BUILD[xside], sgr)).encode("utf-8"))
    for wire in ("before-submit", "after-submit"):
        data = open(path("xfx", scenario, wire, "wire"), "rb").read()
        if len(want.findall(data)) != 1:
            claim.fail("%s.wire holds the banner of %s %d times" % (wire, BUILD[xside], len(want.findall(data))))
        else:
            claim.say("%s.wire writes the banner of %s once" % (wire, BUILD[xside]))
    return claim


def check_replay(scenario):
    claim = Claim("replay %s" % scenario)
    for side in ("fx", "xfx"):
        a = raw_rows(side, scenario, "wireend")
        b = raw_rows(side, scenario, "second")
        if a != b:
            diff = [i + 1 for i in range(max(len(a), len(b))) if (a[i] if i < len(a) else None) != (b[i] if i < len(b) else None)]
            claim.fail("%s: the replayed wire is not the `second` snap: rows %s" % (side_name(side), diff))
        else:
            claim.say("%-3s the whole wire replayed is the `second` snap, all %d rows" % (side_name(side), len(a)))
    return claim


SUMMARY = re.compile(r"^  <wall> \(↑\d+ ↓\d+\)$")


def card_rows(rows):
    """(first row index, [card line texts]) of the rows above the composer
    that start `┃ `, which must be one run."""
    at = [i for i in range(37) if text(rows[i]).startswith("┃ ")]
    if not at:
        return None, []
    if at != list(range(at[0], at[0] + len(at))):
        raise Malformed("the card's rows are not one run: rows %s" % [i + 1 for i in at])
    return at[0], [text(rows[i])[2:] for i in at]


def check_frame(scenario, expect):
    claim = Claim("frame %s" % scenario)
    for side in ("fx", "xfx"):
        for k, lines in expect[side]:
            rows = rows_of(side, scenario, "frame%d" % k)
            label = "%s frame %d" % (side_name(side), k)
            if text(rows[37]) != "┃" or text(rows[39]) != "auto · model-a":
                claim.fail("%s: the composer row is %r (the hint %r), not an empty composer" % (label, text(rows[37]), text(rows[39])))
                continue
            first, shown = card_rows(rows)
            if shown != lines:
                claim.fail("%s: the card shows %r, the claim is %r" % (label, shown, lines))
                continue
            under, over = first + len(shown), first - 1
            if text(rows[under]) or text(rows[over]) or not SUMMARY.match(text(rows[over - 1])):
                claim.fail("%s: around the card %r / %r / %r" % (label, text(rows[over - 1]), text(rows[over]), text(rows[under])))
                continue
            claim.say("%s: composer empty; the card %s at rows %d-%d, under %r" % (label, shown, first + 1, under, text(rows[over - 1])))
    return claim


def check_same(scenario, fx_snap, xfx_snap):
    """The two screens aligned at the card: the rows above it as far as both
    go, and the card's rows, the same; the composer and the two rows under it
    (38-40) the same. The rows between the card and the composer -- the
    band's blank row and its activity row, which xfx writes once the runtime
    says the turn has started and fx with the card -- are set aside, and
    printed."""
    claim = Claim("same %s fx %s / %s %s" % (scenario, fx_snap, side_name("xfx"), xfx_snap))
    a = rows_of("fx", scenario, fx_snap)
    b = rows_of("xfx", scenario, xfx_snap)
    fa, card_a = card_rows(a)
    fb, card_b = card_rows(b)
    if not card_a or card_a != card_b:
        claim.fail("the cards are fx %r / %s %r" % (card_a, side_name("xfx"), card_b))
        return claim
    n, above = len(card_a), min(fa, fb)
    diff = [("above", i) for i in range(above) if a[fa - above + i] != b[fb - above + i]]
    diff += [("card", i) for i in range(n) if a[fa + i] != b[fb + i]]
    diff += [("band", i + 1) for i in (37, 38, 39) if a[i] != b[i]]
    if diff:
        claim.fail("rows differ: %s" % diff)
        return claim
    claim.say("aligned at the card (fx row %d, %s row %d): the %d rows above it and its %d the same; rows 38-40 the same"
              % (fa + 1, side_name("xfx"), fb + 1, above, n))
    claim.say("set aside, between the card and the composer: fx %r / %s %r"
              % ([text(r) for r in a[fa + n : 37]], side_name("xfx"), [text(r) for r in b[fb + n : 37]]))
    return claim


def check_inline(scenario, snap):
    claim = Claim("inline %s %s" % (scenario, snap))
    a = rows_of("fx", scenario, snap)
    b = rows_of("xfx", scenario, snap)
    diff = [i + 1 for i in range(max(len(a), len(b))) if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]
    if diff:
        claim.fail("rows %s differ" % diff)
    else:
        claim.say("all %d rows the same" % len(a))
    return claim


TALL = ["TALL_PROMPT_HEAD"] + ["tall prompt body %02d" % n for n in range(1, 59)] + ["TALL_PROMPT_TAIL"]


def check_whole(scenario, snap, first_prompt):
    claim = Claim("whole %s %s" % (scenario, snap))
    for side in ("fx", "xfx"):
        rows = rows_of(side, scenario, snap)
        texts = [text(r) for r in rows]
        card = ["┃ " + line for line in TALL]
        counts = [texts.count(t) for t in card]
        if counts != [1] * len(card):
            claim.fail("%s: %d of %d card rows not exactly once (%s)" % (side_name(side), sum(c != 1 for c in counts), len(card),
                                                                        [(t, c) for t, c in zip(card, counts) if c != 1][:3]))
            continue
        at = texts.index(card[0])
        if texts[at : at + len(card)] != card:
            claim.fail("%s: the card's rows from row %d are not in order, one after another" % (side_name(side), at + 1))
            continue
        if first_prompt:
            ok = at == 2 and texts[0] == "<banner>" and texts[1] == ""
            above = "the banner and one blank row"
        else:
            ok = at >= 2 and texts[at - 1] == "" and SUMMARY.match(texts[at - 2])
            above = "the previous turn's summary %r and one blank row" % texts[at - 2]
        if not ok:
            claim.fail("%s: above the card (row %d): %r / %r" % (side_name(side), at + 1, texts[at - 2] if at > 1 else None, texts[at - 1] if at else None))
            continue
        claim.say("%-3s all %d card rows once, in order, rows %d-%d of %d, under %s" % (side_name(side), len(card), at + 1, at + len(card), len(texts), above))
    return claim


CTRL_O = re.compile(r"^  │ (\d+) ([ +-]) (.*)$")
REVIEW = re.compile(r"^ +(\d+) ([ +-]) (.*)$")


def diff_rows(rows, kind):
    """[(row index, number, sign, text)] of a snap's diff rows."""
    if kind == "ctrl-o":
        return [(i, int(m.group(1)), m.group(2), m.group(3)) for i, r in enumerate(rows) for m in [CTRL_O.match(text(r))] if m]
    top = next((i for i, r in enumerate(rows) if text(r).startswith("─") and set(text(r)) == {"─"}), None)
    end = next((i for i, r in enumerate(rows) if text(r).startswith("┄")), None)
    if top is None or end is None:
        raise Malformed("no review between a rule and a dotted rule")
    return [(i, int(m.group(1)), m.group(2), m.group(3)) for i in range(top + 1, end) for m in [REVIEW.match(text(rows[i]))] if m]


def fixture(ws, name, old, new):
    before = open(os.path.join(SCEN, ws, name), "rb").read()
    assert before.count(old) == 1, (name, old)
    after = before.replace(old, new)
    lines = lambda b: b[:-1].split(b"\n") if b.endswith(b"\n") else b.split(b"\n")  # noqa: E731
    return lines(before), lines(after)


def check_spelled(scenario, snap, kind, ws, name, old, new, window):
    claim = Claim("spelled %s %s (%s)" % (scenario, snap, kind))
    before, after = fixture(ws, name, old, new)
    found = {}
    for side in ("fx", "xfx"):
        label = side_name(side)
        rows = rows_of(side, scenario, snap)
        listed = diff_rows(rows, kind)
        found[side] = (rows, listed)
        if [(n, s) for _, n, s, _ in listed] != window:
            claim.fail("%s: the diff rows are %s, the window is %s" % (label, [(n, s) for _, n, s, _ in listed], window))
            continue
        bad = 0
        spelling = "fx" if side == "fx" or kind == "ctrl-o" else "xfx"
        for i, n, sign, shown in listed:
            line = (after if sign == "+" else before)[n - 1]
            col = len(text(rows[i])) - len(shown)
            if side == "fx":
                want = fx_spelled(line, col)
            elif kind == "ctrl-o":
                want = fx_spelled(line, col, alm=True)
            else:
                want = xfx_spelled(line)
            if shown != want.rstrip(" "):
                claim.fail("%s row %d: %r, %s's spelling is %r" % (label, i + 1, shown, spelling, want))
                bad += 1
            raw = [ch for _, ch, _ in rows[i] if hidden(ch)]
            if raw:
                claim.fail("%s row %d: the terminal was handed %s as itself" % (label, i + 1, ["U+%04X" % ord(c) for c in raw]))
                bad += 1
        if not bad:
            claim.say("%-3s %d diff rows, each the file's line as %s spells it; none holds a hidden character" % (label, len(listed), spelling))
    if claim.ok:
        (a, la), (b, lb) = found["fx"], found["xfx"]
        same = spelled_apart = 0
        for (i, n, s, ta), (j, _, _, tb) in zip(la, lb):
            if ta == tb:
                if a[i] != b[j]:
                    claim.fail("row %d (%d %s): the same text, other cells or styles" % (i + 1, n, s))
                same += 1
            else:
                styles_a = sorted({sh for _, ch, sh in a[i] if ch != " "})
                styles_b = sorted({sh for _, ch, sh in b[j] if ch != " "})
                if styles_a != styles_b:
                    claim.fail("row %d (%d %s): styles fx %s / xfx %s" % (i + 1, n, s, styles_a, styles_b))
                claim.say("%d %s spelled apart: fx %r / %s %r" % (n, s, ta, side_name("xfx"), tb))
                spelled_apart += 1
        claim.say("%d rows the same cell by cell, %d spelled apart with fx's styles" % (same, spelled_apart))
    return claim


def check_bytes(scenario, snap, only=None):
    claim = Claim("bytes %s %s%s" % (scenario, snap, "" if only is None else " %s" % only))
    found = {}
    for side in ("fx", "xfx"):
        lines = open(path(side, scenario, snap), encoding="utf-8").read().split("\n")
        listed = diff_rows(rows_of(side, scenario, snap), "ctrl-o")
        found[side] = [((n, s), lines[i]) for i, n, s, _ in listed if only is None or (n, s) in only]
    if [k for k, _ in found["fx"]] != [k for k, _ in found["xfx"]] or not found["fx"]:
        claim.fail("the diff rows are fx %s / %s %s" % ([k for k, _ in found["fx"]], side_name("xfx"), [k for k, _ in found["xfx"]]))
        return claim
    for ((n, s), a), (_, b) in zip(found["fx"], found["xfx"]):
        if a != b:
            claim.fail("%d %s: fx %r / %s %r" % (n, s, a, side_name("xfx"), b))
        else:
            claim.say("%d %s  %r" % (n, s, a))
    if claim.ok:
        claim.say("all %d diff rows the same bytes on both sides" % len(found["fx"]))
    return claim


def check_refused(scenario, snap, ws, name, old, new, window):
    claim = Claim("refused %s %s" % (scenario, snap))
    before, after = fixture(ws, name, old, new)
    rows = rows_of("fx", scenario, snap)
    listed = diff_rows(rows, "ctrl-o")
    want = [(n, s) for n, s in window]
    if [(n, s) for _, n, s, _ in listed] != want:
        claim.fail("fx: the diff rows are %s, the window is %s" % ([(n, s) for _, n, s, _ in listed], want))
    else:
        for i, n, sign, shown in listed:
            line = (after if sign == "+" else before)[n - 1]
            if shown != fx_spelled(line, len(text(rows[i])) - len(shown)).rstrip(" "):
                claim.fail("fx row %d: %r" % (i + 1, shown))
        claim.say("fx  edits it: %d diff rows, its stray bytes spelled %s" % (len(listed), [t for _, _, _, t in listed if "\\x" in t]))
    rows = rows_of("xfx", scenario, snap)
    listed = diff_rows(rows, "ctrl-o")
    failed = [text(r) for r in rows if text(r).startswith("└ Failed %s: " % name)]
    if listed or len(failed) != 1:
        claim.fail("%s: %d diff rows, %d failed rows" % (side_name("xfx"), len(listed), len(failed)))
    else:
        claim.say("%-3s no diff row; %r" % (side_name("xfx"), failed[0]))
    return claim


def check_nodiff(scenario, snap, name):
    claim = Claim("nodiff %s %s" % (scenario, snap))
    for side in ("fx", "xfx"):
        rows = rows_of(side, scenario, snap)
        listed = [i + 1 for i, r in enumerate(rows) if CTRL_O.match(text(r)) or REVIEW.match(text(r))]
        edited = [text(r) for r in rows if text(r) == "└ Edited %s +1 / -1" % name]
        if listed or len(edited) != 1:
            claim.fail("%s: diff rows %s; %d `└ Edited` rows" % (side_name(side), listed, len(edited)))
        else:
            claim.say("%-3s no diff row; %r" % (side_name(side), edited[0]))
    return claim


def findings(scenario, snap):
    """Rows outside every claim that hand a hidden character to the terminal
    on xfx's side; and, for the width scenario, its line at the margin on both
    sides."""
    out = []
    if scenario == "p5-transcript-diffwide":
        if xside != "xfx":
            return out
        for side in ("fx", "xfx"):
            for _, n, s, t in diff_rows(rows_of(side, scenario, snap), "ctrl-o"):
                if n == 3:
                    out.append("%s %s %s line 3 (ctrl-o-width-disputed) ends %r" % (scenario, snap, side_name(side), t[-8:]))
        return out
    for i, r in enumerate(rows_of("xfx", scenario, snap)):
        if CTRL_O.match(text(r)):
            continue
        raw = sorted({"U+%04X" % ord(ch) for _, ch, _ in r if hidden(ch)})
        if raw:
            out.append("%s %s row %d %r: %s as itself" % (scenario, snap, i + 1, text(r), ", ".join(raw)))
    return out


CARD1 = ["second prompt"]
CARD3 = ["multi line one", "multi line two", "multi line three"]
CTL = ("p5-transcript-ws", "ctl.txt", b"third line", b"THIRD LINE", [(1, " "), (2, " "), (3, "-"), (3, "+")])
ODD = ("p5-transcript-diffws", "odd.txt", b"first line", b"FIRST LINE", [(1, "-"), (1, "+"), (2, " "), (3, " "), (4, " "), (5, " "), (6, " ")])
BAD = ("p5-transcript-diffws", "bad.txt", b"first line", b"FIRST LINE", [(1, "-"), (1, "+"), (2, " "), (3, " ")])
SPEC = [
    # T7
    ("build", "p5-transcript-submit"),
    ("replay", "p5-transcript-submit"),
    ("frame", "p5-transcript-submit", {"fx": [(1, CARD1)], "xfx": [(1, CARD1)]}),
    ("same", "p5-transcript-submit", "frame1", "frame1"),
    ("inline", "p5-transcript-submit", "filled"),
    ("same", "p5-transcript-submit", "submit-0.15", "submit-0.15"),
    ("inline", "p5-transcript-submit", "second"),
    ("build", "p5-transcript-submit-multi"),
    ("replay", "p5-transcript-submit-multi"),
    # fx's first frame previews the last line; its second writes the card.
    ("frame", "p5-transcript-submit-multi", {"fx": [(1, CARD3[-1:]), (2, CARD3)], "xfx": [(1, CARD3)]}),
    ("same", "p5-transcript-submit-multi", "frame2", "frame1"),
    ("inline", "p5-transcript-submit-multi", "filled"),
    ("inline", "p5-transcript-submit-multi", "second"),
    ("inline", "p5-transcript-submit-multi", "second.full"),
    # T8
    ("whole", "p5-transcript-tallfirst", "tall.full", True),
    ("inline", "p5-transcript-tallfirst", "tall.full"),
    ("inline", "p5-transcript-tallfirst", "tall"),
    ("whole", "p5-transcript-submit", "tall.full", False),
    ("inline", "p5-transcript-submit", "tall.full"),
    ("inline", "p5-transcript-submit", "tall"),
    # T10
    ("spelled", "p5-transcript-diffctl", "full", "ctrl-o") + CTL,
    ("spelled", "p5-transcript-diffodd", "full", "ctrl-o") + ODD,
    ("spelled", "p5-transcript-diffask", "ask", "review") + CTL,
    ("spelled", "p5-transcript-diffask", "full", "ctrl-o") + CTL,
    ("bytes", "p5-transcript-diffctl", "full"),
    ("bytes", "p5-transcript-diffodd", "full"),
    ("bytes", "p5-transcript-diffask", "full"),
    # T-U6c: the width-disputed code points, written as fx writes them; the
    # line at the margin is the ledger's cause `ctrl-o-width-disputed`
    # (printed under the findings).
    ("bytes", "p5-transcript-diffwide", "full", [(1, "-"), (1, "+"), (2, " ")]),
    ("refused", "p5-transcript-diffbad", "full") + BAD,
    ("nodiff", "p5-transcript-diffctl", "edited", "ctl.txt"),
    ("nodiff", "p5-transcript-diffodd", "edited", "odd.txt"),
    ("inline", "p5-transcript-diffodd", "edited"),
    ("inline", "p5-transcript-diffodd", "closed"),
    ("inline", "p5-transcript-diffask", "applied"),
    ("inline", "p5-transcript-diffask", "closed"),
]
FINDINGS = [("p5-transcript-diffodd", "full"), ("p5-transcript-diffctl", "full"), ("p5-transcript-diffask", "full"),
            ("p5-transcript-diffwide", "full")]


def run(item):
    kind, scenario = item[0], item[1]
    if kind == "build":
        return check_build(scenario)
    if kind == "replay":
        return check_replay(scenario)
    if kind == "frame":
        return check_frame(scenario, item[2])
    if kind == "same":
        return check_same(scenario, item[2], item[3])
    if kind == "inline":
        return check_inline(scenario, item[2])
    if kind == "whole":
        return check_whole(scenario, item[2], item[3])
    if kind == "spelled":
        return check_spelled(scenario, item[2], item[3], *item[4:])
    if kind == "bytes":
        return check_bytes(scenario, item[2], item[3] if len(item) > 3 else None)
    if kind == "refused":
        return check_refused(scenario, item[2], *item[3:])
    if kind == "nodiff":
        return check_nodiff(scenario, item[2], item[3])
    raise ValueError(kind)


def main():
    claims = []
    for item in SPEC:
        if item[1] == "p5-transcript-diffwide" and xside != "xfx":
            continue
        try:
            claim = run(item)
        except Malformed as error:
            claim = Claim("%s %s %s" % (item[0], item[1], item[2] if len(item) > 2 and isinstance(item[2], str) else ""))
            claim.fail("malformed capture: %s" % error)
        claims.append(claim)
    bad = 0
    for claim in claims:
        print(("ok   " if claim.ok else "FAIL ") + claim.name)
        for line in claim.lines:
            print("       " + line)
        bad += not claim.ok
    print("%d claims, %d hold, %d fail" % (len(claims), len(claims) - bad, bad))
    found = [f for scenario, snap in FINDINGS for f in findings(scenario, snap)]
    print("findings (no claim covers these rows): %d" % len(found))
    for f in found:
        print("       " + f)
    sys.exit(1 if bad else 0)


main()
