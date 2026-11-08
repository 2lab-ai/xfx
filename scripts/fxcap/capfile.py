"""Reads `scripts/fxcap/scenarios/*.cap` the way `capture.sh` runs them.

A scenario line prefixed `fx: ` or `xfx: ` is for that binary only
(capture.sh `for_this`), so each binary has its own reading of a file: its
lines, its snaps and its waits.

A sweep unit is one scenario, run once on each binary. A pair of files
`<base>-fx.cap` and `<base>-xfx.cap` is one unit, `<base>`: fx runs the first
and xfx the second (each sized for its own product, `compact-inturn-fx` /
`compact-inturn-xfx`, `.prd/tui-phase4/receipts/w4/README.md:19-20`), and
their snaps are paired by label.
"""

import os
import re

# The largest .cap read: every one in the repository is a few KB.
MAX_BYTES = 1 << 20

SNAP = re.compile(r"^(snap|snapfull) (.+)$")
COMMAND_TOKEN = re.compile(r"^(/[A-Za-z][A-Za-z0-9_-]*)(?:\s|$)")


def for_this(which, line):
    """capture.sh `for_this`: the line for this binary, or None."""
    if line.startswith("fx: "):
        return line[4:] if which == "fx" else None
    if line.startswith("xfx: "):
        return line[5:] if which == "xfx" else None
    return line


class Reading:
    """One binary's reading of one scenario file."""

    def __init__(self, path, which):
        self.path = path
        self.which = which
        self.name = os.path.basename(path)[: -len(".cap")]
        self.lines = []
        with open(path, encoding="utf-8") as f:
            for raw in f.read().split("\n"):
                line = for_this(which, raw)
                if line is not None:
                    self.lines.append(line)
        self.snaps, self.duplicate_snaps = [], []
        self.waits = 0.0
        for line in self.lines:
            m = SNAP.match(line)
            if m:
                label = m.group(2) + (".full" if m.group(1) == "snapfull" else "")
                if label in self.snaps:
                    # A second snap with the same label overwrites the first:
                    # one pair, not two.
                    self.duplicate_snaps.append(label)
                else:
                    self.snaps.append(label)
            if line.startswith("wait "):
                try:
                    self.waits += float(line[5:].strip())
                except ValueError:
                    pass

    def verbs(self, verb):
        """The rest of every line with this verb, in order."""
        out = []
        for line in self.lines:
            if line == verb or line.startswith(verb + " "):
                out.append(line[len(verb) + 1 :])
        return out

    def typed_commands(self):
        """Every `/word` a `type` line starts with, in order."""
        out = []
        for text in self.verbs("type"):
            m = COMMAND_TOKEN.match(text)
            if m:
                out.append(m.group(1))
        return out


class Unit:
    def __init__(self, name, fx_path, xfx_path):
        self.name = name
        self.fx = Reading(fx_path, "fx")
        self.xfx = Reading(xfx_path, "xfx")

    def labels(self):
        """Every label either binary snaps, fx's order first."""
        out = list(self.fx.snaps)
        out += [label for label in self.xfx.snaps if label not in out]
        return out

    def reading(self, which):
        return self.fx if which == "fx" else self.xfx

    def group(self):
        """The scenario-name prefix the scoreboard groups by: the leading
        letters of the name, so `model-anchor`, `model2` and `model` are one
        group, `fulltranscript-notice` is `fulltranscript`."""
        return re.match(r"[a-z]*", self.name).group(0) or self.name


class Timer:
    """A row a binary puts on the screen and takes away on its own clock.

    `seconds` after it is armed the screen changes with no keystroke and no
    byte from the gateway, so a snap a second count takes near then shows
    either screen: the race FLAKE-HINT4 removed
    (`.prd/tui-phase5/receipts/flake-hint4/`). `text` is on the screen while
    the row is; an `until` or `absent` line speaks of the row only when its
    text is exactly this. `xfx` and `fx` are where each binary keeps the
    number.

    What arms it is what a .cap says for certain. `launch` matches a line of
    the binary's environment (`env`) or a file a `seed`, `homefile`, `reseed`
    or `rehomefile` line puts in its HOME: armed at launch and again at each
    `relaunch`. `keys` are tmux key names a `key` line arms it with; `disarms`
    says any other keystroke takes it down -- a `key` line or a `type` line,
    but not a paste (`paste`, `pastex`) and not a `key -H` terminal report:
    both binaries let those through (xfx shell.rs `consume`, its `disarms`;
    fx keeps the hint over a paste frame). A key that only sometimes arms one
    is not in `keys`: Shift-Tab enters full access from auto and is a menu's
    own key elsewhere, so a scenario that means it takes its snaps by
    `until`, and the check counts the warning from that `until shown`.

    `pauses` matches a line that can take the row from it while it is up --
    a Ctrl-C or an Escape arms a cue that owns the row, a typed `/` opens the
    command menu, Ctrl-O opens the full transcript -- and its clock stops
    while it is off the row, so the moment it goes moves by however long that
    lasts. Such a line inside the window is refused as a snap is; one before
    it stops the check's clock too (`races`).

    `settle` is how long after arming the row is surely on the screen if it
    was armed: an `absent` sooner than that says nothing of it, since the
    binary may not have drawn it yet."""

    def __init__(self, name, seconds, text, xfx, fx, launch=None, keys=(), disarms=False, pauses=None, settle=1.0):
        self.name, self.seconds, self.text, self.xfx, self.fx = name, seconds, text, xfx, fx
        self.launch, self.keys, self.disarms, self.pauses = launch, keys, disarms, pauses
        self.settle = settle


# Every timer a snap must not race, in one place.
TIMERS = (
    Timer(
        "the full-access warning",
        4.0,
        # Both forms begin so, the sentence and the one a narrow row gets
        # (src/tui/permissions.rs:30,34; fx's src/core/permissions/
        # permissions.zig:90 and app_permission_runtime.zig:20@481104fd), and
        # nothing else on either screen does: the mode on the status line is
        # `full access`, lower case.
        "Full access",
        "src/tui/permissions.rs:38",
        "src/core/app/app_permission_runtime.zig:21@481104fd",
        launch=re.compile(r'(?i)PERMISSION_MODE=(?:yolo|full-access)\b|"permission_mode"\s*:\s*"(?:yolo|full[- ]access)"'),
        # The cues, the menu and the full transcript keep the warning off the
        # row (xfx `warning_shown`, src/tui/shell.rs:14083-14108, the full
        # transcript at :14101-14103; fx `dangerStatusText`,
        # src/ui/footer/input_presentation.zig:581-600, and
        # `yolo_warning_visible`, app_render_runtime.zig:1849-1850@481104fd),
        # and its clock runs only while it is on it (permissions.rs:409-437;
        # app_permission_runtime.zig:41-67@481104fd).
        pauses=re.compile(r"^key .*\b(?:C-c|C-o|Escape)\b|^type /"),
        # A launch's first frame came 2.13 s after it on a binary's first run
        # (receipts/flake-hint4/r2/launch-probe.txt).
        settle=3.0,
    ),
    # Every Ctrl-C arms it but one over a running turn with a draft, one a
    # question answers (src/ui/input/runtime.zig:63@481104fd), and fx's in its
    # model and MCP menus (`handleSemanticCtrlC`,
    # app_input_runtime.zig:1511-1545@481104fd; xfx `Gestures::interrupt`,
    # src/tui/gesture.rs): armed for each here. A scenario whose Ctrl-C must
    # arm nothing says so with `absent` (capture.sh), which fails the run if
    # the hint is up; an `until gone` where none was armed returns at once.
    Timer(
        "the Ctrl-C exit hint",
        3.0,
        "press ctrl+c again to exit",
        "src/tui/gesture.rs:67",
        "src/core/input/gesture_state.zig:3@481104fd",
        keys=("C-c",),
        disarms=True,
        # An `until shown` right after a Ctrl-C saw the hint within 0.03 s in
        # every round-2 head sweep (receipts/flake-hint4/r2/until-waits.txt);
        # a second leaves room for a loaded runner.
        settle=1.0,
    ),
)

# A snap is inside a timer's window when it may be taken later than BEFORE
# seconds before the timer runs out and earlier than AFTER seconds after it.
# Measured (receipts/flake-hint4/calib.txt): the warning goes 4.04 to 4.12 s
# after launch at load 20 to 33, a launch's first frame 0.02 to 0.14 s.
BEFORE = 1.0
AFTER = 3.0

# What capture.sh spends on a verb besides `wait`: `bells` and `wire` sleep
# 0.3 s; `held`, `signal` and an `until` on another row wait as long as they
# take (None).
SPENDS = {"bells": 0.3, "wire": 0.3, "held": None, "signal": None, "until": None}
UNBOUNDED = float("inf")

# The lines that hand the binary input a timer can be armed by.
INPUTS = ("key", "type", "paste", "pastex", "relaunch")


def _spend(clock, verb, rest, frozen=False):
    """`clock` after one line that only passes time, or `clock` itself."""
    if verb == "wait":
        try:
            return (clock[0] + (0.0 if frozen else float(rest)), clock[1] + float(rest))
        except ValueError:
            return clock
    if verb in SPENDS:
        spent = SPENDS[verb]
        return (clock[0] + (0.0 if frozen else spent or 0.0), UNBOUNDED if spent is None else clock[1] + spent)
    return clock


def races(reading):
    """Every snap of this reading that a timer's window may hold, one line
    each: the scenario, the binary, the snap, its seconds since the timer was
    armed, and the timer.

    The seconds since arming are a range [lo, hi]. A `wait` adds its seconds
    to both ends, a verb in SPENDS its fixed time, and one that waits for
    something else makes hi unbounded. A snap whose range meets
    (seconds - BEFORE, seconds + AFTER) is a race.

    An `until` or `absent` line counts only when its text is exactly the
    timer's: `until gone Full access never-rendered` is true while the
    warning is up, so it says nothing of it.

    - `until shown <text>`: the row is up. It may have come up only now, so
      lo is 0; it has not been up longer than it was armed, so hi stays --
      and when nothing this check sees armed it, hi is the time since the
      last line that handed the binary input, since only input (or a launch)
      arms one.
    - `until gone <text>` and `absent <text>`: the row is not up, so nothing
      is armed until it is armed again -- `absent` fails the run when it is.
      Not while the row is paused (below): gone then is hidden, not spent.
      And an `absent` only from the timer's `settle` on: right after the key
      that arms it, the row may not be drawn yet.

    A line the timer `pauses` on, before its window, takes the row while it
    is surely up, for a time the .cap does not say: from there lo stops
    growing, until an `until shown` names the row, so every snap after it is
    a race until one does."""
    here = os.path.dirname(reading.path)
    profile = [line for line in reading.lines if line.startswith("env ")]
    for line in reading.lines:
        named = line.startswith(("seed ", "homefile ", "reseed ", "rehomefile "))
        path = os.path.join(here, line.split()[-1]) if named else None
        if path and regular_inside(path, here):
            with open(path, encoding="utf-8", errors="replace") as f:
                profile.append(f.read())
    out = []
    for timer in TIMERS:
        at_launch = timer.launch is not None and any(timer.launch.search(text) for text in profile)
        clock = (0.0, 0.0) if at_launch else None
        # Whether a line before the window took the row: lo stops growing.
        frozen = False
        # The seconds since the binary was last handed input, or launched.
        since_input = (0.0, 0.0)
        for line in reading.lines:
            verb, _, rest = line.partition(" ")
            cond, _, said = rest.partition(" ")
            names = (verb == "until" and said == timer.text) or (verb == "absent" and rest == timer.text)
            pauses = bool(timer.pauses and timer.pauses.search(line))
            if verb == "relaunch":
                clock, frozen = (0.0, 0.0) if at_launch else None, False
            elif verb == "key" and any(k in timer.keys for k in rest.split()):
                clock, frozen = (0.0, 0.0), False
            elif timer.disarms and (verb == "type" or (verb == "key" and not rest.startswith("-H "))):
                clock = None
            elif names and verb == "until" and cond == "shown":
                clock, frozen = (0.0, clock[1] if clock else since_input[1]), False
            elif clock is None:
                pass
            elif names and not frozen and (verb != "absent" or clock[0] >= timer.settle):
                clock = None
            elif verb in ("wait",) or verb in SPENDS:
                clock = _spend(clock, verb, rest, frozen)
            elif pauses and clock[1] <= timer.seconds - BEFORE:
                frozen = True
            elif (verb in ("snap", "snapfull") or pauses) and \
                    clock[0] < timer.seconds + AFTER and clock[1] > timer.seconds - BEFORE:
                out.append(
                    "%s (%s): `%s` is %.1f to %s s after %s is armed, inside its window (%g to %g s), and no `until gone` names %r before it%s"
                    % (reading.name, reading.which, line, clock[0], "%.1f" % clock[1] if clock[1] < UNBOUNDED else "unbounded",
                       timer.name, timer.seconds - BEFORE, timer.seconds + AFTER, timer.text,
                       "" if verb in ("snap", "snapfull") else ": it can take the row, and the clock stops while it is off it")
                )
            since_input = (0.0, 0.0) if verb in INPUTS else _spend(since_input, verb, rest)
    return out


def regular_inside(path, directory):
    """Whether `path` is a regular file of at most MAX_BYTES that resolves
    inside `directory`: what is safe to open and read whole. A link out of the
    directory, a FIFO (its open never returns without a writer) or a device
    (/dev/zero never ends) is not."""
    root = os.path.realpath(directory)
    real = os.path.realpath(path)
    return real.startswith(root + os.sep) and os.path.isfile(real) and os.path.getsize(real) <= MAX_BYTES


def units(directory):
    """Every unit of `directory`. A .cap that is not a regular file inside it
    (regular_inside) is refused, never opened: the sweep's denominator is
    every .cap, so one is not dropped from it either."""
    names = sorted(f[: -len(".cap")] for f in os.listdir(directory) if f.endswith(".cap"))
    for name in names:
        if not regular_inside(os.path.join(directory, name + ".cap"), directory):
            raise ValueError("%s.cap is not a regular file of at most %d bytes inside %s" % (name, MAX_BYTES, directory))
    present = set(names)
    out, taken = [], set()
    for name in names:
        if name in taken:
            continue
        if name.endswith("-fx") and name[:-3] + "-xfx" in present:
            base = name[:-3]
            if base in present:
                raise ValueError("%s.cap and the pair %s-fx/-xfx name one unit twice" % (base, base))
            out.append(Unit(base, os.path.join(directory, name + ".cap"), os.path.join(directory, base + "-xfx.cap")))
            taken.update({name, base + "-xfx"})
        else:
            path = os.path.join(directory, name + ".cap")
            out.append(Unit(name, path, path))
            taken.add(name)
    return sorted(out, key=lambda u: u.name)
