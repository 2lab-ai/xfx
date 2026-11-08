"""STOP-NEXT-2's receipt checker: STOP-NEXT's `osc.py` (`../stop-next/osc.py`)
unchanged but for this paragraph -- the OSC 7501 program status on the wire,
fx v0.0.16 against xfx, report by report. No claim is added: none of this
unit's items reaches a captured window (its README, "Wires").

usage: python3 -I -B osc.py <receipt-dir> <side> <sha7> [<scenario>]

<side> is `xfx` (the build under the receipt), `base` (the base's,
`d2459a37`), or a further run of one of them (`base-run2`, `xfx-run3`:
`stopnext` only, so name it). A <scenario> limits the run to it. The fx
wires are the ones captured at the same pin by the units before this one:
`stopnext`'s three runs by STOP-LATE-EVENT (`../stop-late-event/fx/`,
`fx-run2/`, `fx-run3/`) and every other scenario's by L-U1+U2
(`../osc-program-status/fx/`) -- the binary and the scenario files are the
same, so they are not captured again -- read as
`fx/<scenario>-40x120/fx-<scenario>-40x120-<label>.wire`, and the same under
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

  activity (STOP-LATE-EVENT's) on a scenario STOP-LATE-EVENT captured on fx, for
           every `working`: whether a frame since the report before it showed
           the turn's activity -- `Thinking` or `Generating`, as fx's first
           frame of a queued prompt shows either, by its clock -- is what it
           is on fx. A prompt sent in the stop window is reported ahead of
           the frame that first shows its turn on fx; written behind that
           frame, it fails here as well as under `order`'s `Thinking`.
  title    (STOP-LATE-EVENT's, round 3) on a scenario STOP-LATE-EVENT captured
           on fx, for every `working`: whether a window title (`OSC 2`) was
           written since the report before it is what it is on fx. A stopped
           turn's title is on fx's wire ahead of the next prompt's `working`
           (`stopnext`: `idle`, the `Cancelled` frame, `OSC 2`, `working`);
           written behind that `working`, it fails here.
  prompt   (this receipt's) on `stopnext`, for every report: whether a frame
           since the report before it showed the prompt typed in the stop's
           read (`second question`) is what it is on fx. fx's `Cancelled`
           frame has nothing of the prompt, which it queues at once and shows
           only when its worker takes it, after `working`; a card written at
           the keystroke, on the stop's frame, fails here.
  stable   (STOP-LATE-EVENT's) every further fx run of `stopnext`
           (`fx-run2/`, `fx-run3/`) has the same reports, the same
           which-frame answers, the same activity, title and prompt answers
           as `fx/`: the oracle is one order, not a race the comparison
           happened to catch.

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
ENDS = ("s03-exit", "w09-exit", "h02-exit", "i02-exit", "n04-exit", "t01-terminated", "u01-hung-up", "a00-quit", "a01-ask")


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


ACTIVITY = re.compile(rb"\b(?:Thinking|Generating)\b")


def activity_before_working(data):
    """For each `working`, whether a frame since the report before it showed
    the turn's activity (`activity`)."""
    events = [(m.start(), b"F", bool(ACTIVITY.search(SEQUENCE.sub(b" ", m.group(1)))))
              for m in FRAME.finditer(data)]
    events += [(at, b"S", body) for at, body in reports(data) if body != b"state=clear"]
    events.sort(key=lambda event: event[0])
    out, seen = [], False
    for _, kind, value in events:
        if kind == b"F":
            seen = seen or value
            continue
        if value.startswith(b"state=working"):
            out.append(seen)
        seen = False
    return out


TITLE = re.compile(rb"\x1b\]2;")


def titled_before_working(data):
    """For each `working`, whether a window title was written since the
    report before it (`title`)."""
    events = [(m.start(), b"T", None) for m in TITLE.finditer(data)]
    events += [(at, b"S", body) for at, body in reports(data) if body != b"state=clear"]
    events.sort(key=lambda event: event[0])
    out, seen = [], False
    for _, kind, value in events:
        if kind == b"T":
            seen = True
            continue
        if value.startswith(b"state=working"):
            out.append(seen)
        seen = False
    return out


# The prompt each scenario types in a stop's read, where it has one.
PROMPTS = {"p5-lifecycle-osc-stopnext-40x120": rb"\bsecond question\b"}


def prompt_before_reports(data, unit):
    """For each report but a clear, whether a frame since the report before
    it showed the prompt the scenario types in its stop's read (`prompt`)."""
    pattern = re.compile(PROMPTS[unit])
    events = [(m.start(), b"F", bool(pattern.search(SEQUENCE.sub(b" ", m.group(1)))))
              for m in FRAME.finditer(data)]
    events += [(at, b"S", body) for at, body in reports(data) if body != b"state=clear"]
    events.sort(key=lambda event: event[0])
    out, seen = [], False
    for _, kind, value in events:
        if kind == b"F":
            seen = seen or value
            continue
        out.append(seen)
        seen = False
    return out


def between(data, at):
    opened = data.rfind(b"\x1b[?2026h", 0, at)
    closed = data.rfind(b"\x1b[?2026l", 0, at)
    return opened <= closed


def side_wires(receipt, side, root=None):
    prefix = "fx" if side.startswith("fx") else "xfx"
    found = {}
    for path in sorted(glob.glob(os.path.join(root or receipt, side, "*-40x120", "*.wire"))):
        unit = os.path.basename(os.path.dirname(path))
        lead = "%s-%s-" % (prefix, unit)
        label = os.path.basename(path)[len(lead):-len(".wire")]
        found[(unit, label)] = open(path, "rb").read()
    return found


def main():
    receipt, side, sha = sys.argv[1], sys.argv[2], sys.argv[3]
    only = sys.argv[4] + "-40x120" if len(sys.argv) > 4 else None
    # L-U1+U2's fx captures at the same pin, and STOP-LATE-EVENT's of
    # `stopnext`, which win where both have one.
    earlier = os.path.join(receipt, "..", "stop-late-event")
    fx = side_wires(receipt, "fx", os.path.join(receipt, "..", "osc-program-status"))
    own = side_wires(receipt, "fx", earlier)
    fx.update(own)
    fx = {k: v for k, v in fx.items() if only in (None, k[0])}
    xfx = side_wires(receipt, side)
    failures = []
    for run in ("fx-run2", "fx-run3"):
        for key, data in side_wires(receipt, run, earlier).items():
            if key not in fx:
                continue
            ours = ([normalized(b) for _, b in reports(data)], which_frames(data),
                    activity_before_working(data), titled_before_working(data),
                    prompt_before_reports(data, key[0]))
            theirs = ([normalized(b) for _, b in reports(fx[key])], which_frames(fx[key]),
                      activity_before_working(fx[key]), titled_before_working(fx[key]),
                      prompt_before_reports(fx[key], key[0]))
            if ours != theirs:
                failures.append("stable %s %s %s: %s, not fx/'s %s" % (run, key[0], key[1], ours, theirs))
    # A kept build is compared on the scenarios it was captured on: a scenario
    # added after it has no wire there. The build under the receipt and the
    # base's must have every one.
    kept = False
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
        if key in own:
            fx_active, our_active = activity_before_working(theirs), activity_before_working(ours)
            if fx_active != our_active:
                failures.append(
                    "activity %s %s: a frame showing the turn's activity before each working: "
                    "%s on fx, %s on %s" % (unit, label, fx_active, our_active, side)
                )
            fx_titled, our_titled = titled_before_working(theirs), titled_before_working(ours)
            if fx_titled != our_titled:
                failures.append(
                    "title %s %s: a window title since the report before each working: "
                    "%s on fx, %s on %s" % (unit, label, fx_titled, our_titled, side)
                )
        if unit in PROMPTS:
            fx_prompt, our_prompt = prompt_before_reports(theirs, unit), prompt_before_reports(ours, unit)
            if len(fx_prompt) == len(our_prompt) and fx_prompt != our_prompt:
                failures.append(
                    "prompt %s %s: a frame showing the stop's prompt before each report: "
                    "%s on fx, %s on %s" % (unit, label, fx_prompt, our_prompt, side)
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
