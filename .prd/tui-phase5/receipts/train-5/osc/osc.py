"""L-U1+U2's receipt checker: the OSC 7501 program status on the wire, fx
v0.0.16 against xfx, report by report.

Train 5's copy (`.prd/tui-phase5/receipts/osc-program-status/osc.py` as
L-U1+U2 left it), with one change: `m04-exit`, the `/quit` of train 5's
`p5-lifecycle-osc-mcp` twin, is one more label of a wire that ends a session
(`ENDS`), so `last` asks it too. Train 5 runs it on <receipt-dir>s of its own
(`osc/default`, `osc/tc`, `osc/mcp`), each holding `fx/` and `xfx/`; a `-tc`
twin's wires are named with its own scenario name, so they are compared with
fx's `-tc` twin's.

usage: python3 -I -B osc.py <receipt-dir> <side> <sha7> [<scenario>]

<side> is `xfx` (the build under the receipt), `red` (the base's), `prev`
(the unit's first build, `743c5381`: `picker` only), `prev2` (round 1's
build, `eb90e677`: `autocompactfail` only), `prev3` (round 2's build,
`7d8b2c10`), `prev4` (round 3's build, `afe7bda4`) or `prev5` (round 4's
first build, `b65eeedc`); a kept build is compared on every scenario it was
captured on, which is every one but `interruptnext` (round 5's) for the last
three. A <scenario> limits the run to it. Reads
`fx/<scenario>-40x120/fx-<scenario>-40x120-<label>.wire` and the same under
<side>/ with the `xfx-` prefix. Every report is `ESC ] 7501 ; <body> ESC \\`
with a body of printable ASCII; this file's own parser reads them -- it shares
nothing with the code under the receipt -- and it reads them **from every
introducer** on the wire (`ESC ] 7501` and the 8-bit `0x9d 7501`), so a
report cut short, spelled with another terminator or carrying a raw byte is
a failure of its own (`intact`) rather than bytes the comparison skips.

Three normalizations, each named where it is applied and nothing else set
aside:

  app-name           `app=fx` and `app=xfx` are both the program's name: the
                     record names the program that owns the terminal
                     (docs/parity.md, `terminal program status`).
  command-tool-name  a permission's `msg` names the asking tool: fx's
                     command tool is `shell` with `action: run`, labelled
                     `shell.run <command>`; xfx's is `terminal`, labelled
                     `terminal <command>`. The command after it is compared.
  question-breaks    a question's `msg` is the question as each product's
                     `ask_user_question` holds it. fx v0.0.16 keeps a line
                     feed and a tab in it as they are, and its fold makes the
                     run one space; xfx's tool spells every control
                     (`terminal_safe`, `src/tools/question.rs:57-73`), so the
                     same run reaches the reporter as the text `\x0a\x09`. A
                     run of those two spellings, in a question, compares as
                     one space. The question tool's rule, not the reporter's:
                     every other spelled control is compared as it is.
                     It expires with the `docs/parity.md` row that records
                     the difference (owner unit QUESTION-BREAKS).

The claims, per wire label:

  build    each xfx wire carries the banner of the build the side names, and
           no other.
  intact   every introducer on the wire, on either side, begins a whole
           report: `ESC ] 7501 ;`, printable ASCII, `ESC \\`.
  same     the reports, in order, decoded and normalized, are fx's.
  between  no report is inside a synchronized update a frame opened, on
           either side.
  order    where each report sits is where fx's does: the first report is
           preceded, since the mode set (`?2004h`), by a window title
           (`OSC 2`) and followed by the first frame's `?2026h` -- the
           launch's `idle` after its title and right ahead of the first
           frame -- and every report has a frame between it and the last
           mode set exactly when fx's does: a resume's report after the
           frame the resume repaints, never against the mode set. (Which
           sequence is nearest each report is not compared: fx's timer
           paints frames on its own cadence -- `(0s)` between a `working`
           and an answer that comes at once -- and xfx's on its, so two
           reports with no frame between them on one side and a clock's
           frame between them on the other are the same order.)
           And **which frame** a report follows, for three kinds of
           report, each its own claim: for every `working` after a settled
           report (`idle`, `done`, `error`) -- work a keystroke began --
           whether a frame since the report before it showed `Thinking`; for
           every `working` after a `blocked` report -- a question or a
           permission answered -- whether there was a frame between the two
           at all; for every `error`, whether a frame since the report before
           it showed `Compaction failed`. Each is what it is on fx. A frame's
           ordinal cannot be compared: fx writes frames xfx does not (its
           cursor probe's, a timer's own) and xfx writes a band in fewer, so
           the frame is told by what it showed. A `working` written ahead of
           the frame that shows the prompt's `Thinking`, an answer's
           `working` ahead of any frame after its question, and an automatic
           compaction's `error` behind the frame that shows its failure,
           where fx's is ahead of it, fail here. (What the frame after an
           answer shows is not compared: the activity row there is each
           product's own -- xfx's names the running tool, fx repaints the
           whole screen -- and a compaction's progress row is on a frame only
           as long as its request takes.)
  clear    every `state=clear` follows `?2004l` immediately, on either side.
  last     on a wire that ends the session (an exit, a signal death), no
           introducer -- whole report or not -- follows the last clear, on
           either side.
  inert    every `msg` is the base64 alphabet only, and decodes to UTF-8
           with no control character; the report holds no ESC but its two.

Exit 0 when every claim holds; otherwise each failing claim is printed and
the exit is 1.
"""

import base64
import glob
import os
import re
import sys

# Where a report could begin: the 7-bit introducer, and the 8-bit one.
INTRODUCER = re.compile(rb"\x1b\]7501|\x9d7501")
# A whole report, anchored at an introducer: printable ASCII, then `ESC \`.
WHOLE = re.compile(rb"\x1b\]7501;([\x20-\x7e]*)\x1b\\")
B64 = re.compile(r"^[A-Za-z0-9+/]*={0,2}$")
ENDS = ("w09-exit", "h02-exit", "i02-exit", "n04-exit", "t01-terminated", "u01-hung-up", "a00-quit", "a01-ask", "m04-exit")


def reports(data):
    """Each whole report: its offset and its body, as bytes."""
    found = []
    for intro in INTRODUCER.finditer(data):
        whole = WHOLE.match(data, intro.start())
        if whole:
            found.append((whole.start(), whole.group(1)))
    return found


def broken(data):
    """The offsets of every introducer that does not begin a whole report."""
    return [m.start() for m in INTRODUCER.finditer(data) if not WHOLE.match(data, m.start())]


def normalized(body):
    """A report body as a comparable string, with the three normalizations."""
    text = body.decode("utf-8", "replace")
    fields = text.split(":")
    kind = "question" if "kind=question" in fields else None
    out = []
    i = 0
    while i < len(fields):
        field = fields[i]
        if field.startswith("app="):
            out.append("app=<program>")  # app-name
        elif field.startswith("msg="):
            try:
                msg = base64.b64decode(field[4:], validate=True).decode("utf-8")
            except ValueError:
                # Not base64, or not text: `inert` names it; here it is
                # compared as what it is, which nothing on fx's side is.
                out.append("msg=<not base64 text: %r>" % field[4:])
                i += 1
                continue
            if kind == "question":  # question-breaks
                msg = re.sub(r"(?:\\x0[9a])+", " ", msg)
                msg = re.sub(r" {2,}", " ", msg)
            if msg.startswith("shell.run "):  # command-tool-name
                msg = "<command-tool> " + msg[len("shell.run "):]
            elif msg.startswith("terminal "):
                msg = "<command-tool> " + msg[len("terminal "):]
            out.append("msg=%r" % msg)
        else:
            out.append(field)
        i += 1
    return ":".join(out)


def inert(body):
    """Why a report's message is not inert, or None."""
    if b"\x1b" in body or b"\x07" in body:
        return "a control inside the report"
    text = body.decode("ascii", "replace")
    m = re.search(r":msg=(.*)$", text)
    if not m:
        return None
    encoded = m.group(1)
    if not B64.match(encoded) or len(encoded) % 4:
        return "msg is not base64: %r" % encoded
    try:
        msg = base64.b64decode(encoded, validate=True).decode("utf-8")
    except Exception as err:  # noqa: BLE001
        return "msg does not decode: %s" % err
    if any(ord(c) < 0x20 or 0x7F <= ord(c) <= 0x9F for c in msg):
        return "msg decodes to a control: %r" % msg
    if len(msg.encode()) > 256:
        return "msg past 256 bytes"
    return None


# What a report is placed against: the mode set, a window title, a frame's
# opener, a report.
PLACE = re.compile(rb"\x1b\[\?2004h|\x1b\]2;[^\x07\x1b]*\x07|\x1b\[\?2026h|\x1b\]7501;[\x20-\x7e]*\x1b\\")


def placed(data):
    """For each report but a clear, whether a frame lies between it and the
    last mode set; and for the first, whether a title lies between them and
    whether a frame's opener is the next thing placed after it."""
    tokens = [m.group(0) for m in PLACE.finditer(data)]
    framed, titled, out, first = False, False, [], None
    for i, token in enumerate(tokens):
        if token == b"\x1b[?2004h":
            framed, titled = False, False
        elif token.startswith(b"\x1b]2;"):
            titled = True
        elif token == b"\x1b[?2026h":
            framed = True
        elif token != b"\x1b]7501;state=clear\x1b\\":
            out.append(framed)
            if first is None:
                following = tokens[i + 1] if i + 1 < len(tokens) else b""
                first = (titled, following == b"\x1b[?2026h")
    return out, first


FRAME = re.compile(rb"\x1b\[\?2026h(.*?)\x1b\[\?2026l", re.S)
# A frame's escape sequences, set aside so a word is read as the cells show it
# (an SGR run's final `m` is not part of the word after it).
SEQUENCE = re.compile(rb"\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
THINKING = re.compile(rb"\bThinking\b")
COMPACTION_FAILED = re.compile(rb"\bCompaction failed\b")
SETTLED = (b"state=idle", b"state=done", b"state=error")


def which_frames(data):
    """For each report but a clear, what the frames since the report before
    it showed, where the report is one of three: a `working` after a settled
    report (or none) -- whether one showed `Thinking`; a `working` after a
    `blocked` one -- whether there was a frame at all; an `error` -- whether
    one showed `Compaction failed`. For any other report, `None`."""
    events = []
    for m in FRAME.finditer(data):
        cells = SEQUENCE.sub(b" ", m.group(1))
        events.append((m.start(), b"F", (bool(THINKING.search(cells)), bool(COMPACTION_FAILED.search(cells)))))
    events += [(at, b"S", body) for at, body in reports(data) if body != b"state=clear"]
    events.sort(key=lambda event: event[0])
    out, thinking, failed, framed, previous = [], False, False, False, None
    for _, kind, value in events:
        if kind == b"F":
            thinking, failed, framed = thinking or value[0], failed or value[1], True
            continue
        kind_before = None if previous is None else previous.split(b":")[0]
        if value.startswith(b"state=working") and (kind_before is None or kind_before in SETTLED):
            out.append(("Thinking", thinking))
        elif value.startswith(b"state=working") and kind_before == b"state=blocked":
            out.append(("a frame", framed))
        elif value.startswith(b"state=error"):
            out.append(("Compaction failed", failed))
        else:
            out.append(None)
        thinking, failed, framed, previous = False, False, False, value
    return out


def between(data, at):
    opened = data.rfind(b"\x1b[?2026h", 0, at)
    closed = data.rfind(b"\x1b[?2026l", 0, at)
    return opened <= closed


def side_wires(receipt, side):
    prefix = "fx" if side == "fx" else "xfx"
    found = {}
    for path in sorted(glob.glob(os.path.join(receipt, side, "*-40x120", "*.wire"))):
        unit = os.path.basename(os.path.dirname(path))
        lead = "%s-%s-" % (prefix, unit)
        label = os.path.basename(path)[len(lead):-len(".wire")]
        found[(unit, label)] = open(path, "rb").read()
    return found


def main():
    receipt, side, sha = sys.argv[1], sys.argv[2], sys.argv[3]
    only = sys.argv[4] + "-40x120" if len(sys.argv) > 4 else None
    fx = {k: v for k, v in side_wires(receipt, "fx").items() if only in (None, k[0])}
    xfx = side_wires(receipt, side)
    failures = []
    # A kept build is compared on the scenarios it was captured on: a scenario
    # added after it has no wire there. The build under the receipt and the
    # base's must have every one.
    kept = side.startswith("prev")
    absent = sorted({k[0] for k in fx if k not in xfx and kept})
    compared = 0
    for key in sorted(fx):
        unit, label = key
        if unit in absent:
            continue
        compared += 1
        if key not in xfx:
            failures.append("%s %s: no %s wire" % (unit, label, side))
            continue
        ours, theirs = xfx[key], fx[key]
        banners = {f.decode() for f in re.findall(rb"v0\.1\.0-([0-9a-f]{7})", ours)}
        if banners != {sha}:
            failures.append("build %s %s: the wire carries %s, not %s" % (unit, label, sorted(banners), sha))
        for name, data in (("fx", theirs), (side, ours)):
            for at in broken(data):
                failures.append(
                    "intact %s %s %s @%d: an introducer with no whole report: %r"
                    % (name, unit, label, at, data[at : at + 48])
                )
            for at, body in reports(data):
                why = inert(body)
                if why:
                    failures.append("inert %s %s %s @%d: %s" % (name, unit, label, at, why))
                if not between(data, at):
                    failures.append("between %s %s %s @%d: inside a frame" % (name, unit, label, at))
                if body == b"state=clear" and not data[:at].endswith(b"\x1b[?2004l"):
                    failures.append("clear %s %s %s @%d: not right after ?2004l" % (name, unit, label, at))
            if label in ENDS:
                last_clear = data.rfind(b"\x1b]7501;state=clear\x1b\\")
                if last_clear < 0:
                    failures.append("last %s %s %s: the session ended with no clear" % (name, unit, label))
                elif INTRODUCER.search(data, last_clear + 1):
                    failures.append("last %s %s %s: an introducer after the last clear" % (name, unit, label))
        a = [normalized(b) for _, b in reports(theirs)]
        b = [normalized(b) for _, b in reports(ours)]
        (fx_framed, fx_first), (our_framed, our_first) = placed(theirs), placed(ours)
        if len(fx_framed) == len(our_framed) and (fx_framed != our_framed or fx_first != our_first):
            at = [i for i, (x, y) in enumerate(zip(fx_framed, our_framed)) if x != y]
            failures.append(
                "order %s %s: a frame since the mode set before report %s: %s on fx, %s on %s; "
                "the first report (titled, then a frame): %s on fx, %s on %s"
                % (unit, label, at, [fx_framed[i] for i in at], [our_framed[i] for i in at], side,
                   fx_first, our_first, side)
            )
        fx_frames, our_frames = which_frames(theirs), which_frames(ours)
        if len(fx_frames) == len(our_frames):
            # One claim per kind of frame asked about, so each is counted.
            for what, report in (("Thinking", "working"), ("a frame", "working after blocked"),
                                 ("Compaction failed", "error")):
                at = [i for i, (x, y) in enumerate(zip(fx_frames, our_frames))
                      if x != y and what in (x and x[0], y and y[0])]
                if at:
                    failures.append(
                        "order %s %s: %s %s after %s: %s on fx, %s on %s"
                        % (unit, label, report, at, what, [fx_frames[i] and fx_frames[i][1] for i in at],
                           [our_frames[i] and our_frames[i][1] for i in at], side)
                    )
        mark = "same" if a == b else "DIFF"
        print("%-4s %-34s %-22s fx %2d  %s %2d  %s" % (mark, unit, label, len(a), side, len(b), " > ".join(b) or "(none)"))
        if a != b:
            failures.append("same %s %s: fx %s / %s %s" % (unit, label, a, side, b))
    if not any(f.startswith("build ") for f in failures):
        print("build: every %s wire carries v0.1.0-%s and no other banner" % (side, sha))
    for failure in failures:
        print("FAIL", failure)
    for unit in absent:
        print("not captured on %s: %s" % (side, unit))
    print("%d wire pairs, %d failing claims" % (compared, len(failures)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
