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
