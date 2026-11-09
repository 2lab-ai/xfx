#!/usr/bin/env bash
#
# The TUI acceptance gate, on a real terminal.
#
#   scripts/smoke-tui.sh <path-to-xfx> --faulty <path-to-xfx-with-faults> [evidence-dir]
#
# The second runner beside `scripts/smoke.sh`, which keeps running unchanged:
# the line-oriented product it receipts does not stop existing when a TUI
# arrives, and `xfx ask` is still a pipe-friendly command with no terminal. This
# one drives every scenario of `.prd/06-qa-harness.md` -- Phase 1's 1-12,
# Phase 2's 13-21 and Phase 3's 22-25, plus the lettered rows 3b, 3c, 3d, 3e,
# 10b and 23b, and the QA-only rows the parity program's behaviour list adds
# (3f, 3g, 17b; `.prd/07-fx-parity-program.md` § Phase 4 close-out) -- against a
# **release** binary on a real pseudoterminal, with a cell-grid oracle and an
# evidence directory. The count it prints is the length of the list below and
# the check total is summed from what the scenarios wrote down, so neither
# number can be a claim this comment made.
#
# The oracle is judged before the product is. `helpers/oracle_test.py` runs
# first and the whole suite stops if one of the emulator's own claims is false:
# every scenario below reads a screen through it, so an emulator that measured a
# grapheme differently from `unicode-width` would report the product's correct
# row as a defect, and one with a hole in its allowlist would report a sequence
# xfx must never emit as a passing screen.
#
# A TUI's contract is what is on the screen, and no unit test can see a screen.
# So every scenario is judged three ways, cheapest first: bytes on the wire,
# **cells on a grid** an emulator builds from those bytes, and the child's own
# `termios` read off its terminal while it runs. The emulator is written here
# rather than installed, and it **fails the run on any sequence it does not
# know** -- which makes it a contract as well as an oracle: xfx must emit only
# the subset it declares.
#
# Two properties are not negotiable, the same two `scripts/smoke.sh` holds:
#
#   * It never touches a live credential or the network. Every model response
#     comes from a fixture server this script's helpers start on a loopback
#     port, and the only credential in play is a literal that must never appear
#     in output.
#   * It writes nothing into the repository. Every home, workspace, captured
#     stream, grid snapshot and `termios` capture lives under the evidence
#     directory, which defaults to a temporary path and is printed at the end.
#
# And one this suite adds, from `06-qa-harness.md` §"Fixtures and the
# mock-vs-live rule": **every scenario satisfies the three-part positive
# discriminator.** A per-run nonce goes into the prompt, is asserted in the
# client-side capture of the request xfx really sent, and the rendered output
# must be non-empty and carry the fixture's own marker. Absence is never a pass
# condition -- a scenario that passes because nothing appeared is a failed
# scenario.
#
# Requirements: bash, python3 (present on every supported platform and on the CI
# images), and the checkout the script sits in: the slash menu row is judged
# against the registry `src/interactive.rs` declares. The four helpers and a
# copy of that source are written out as real files under the evidence
# directory, and each scenario prints the command that re-runs it by hand.

set -euo pipefail

usage() {
	printf 'usage: %s <path-to-xfx> --faulty <path-to-xfx-with-faults> [evidence-dir]\n' "$0" >&2
	exit 2
}

binary="${1:-}"
[ -n "$binary" ] || usage
shift

faulty=""
evidence=""
while [ $# -gt 0 ]; do
	case "$1" in
	--faulty)
		[ $# -ge 2 ] || usage
		faulty="$2"
		shift 2
		;;
	-*)
		usage
		;;
	*)
		[ -z "$evidence" ] || usage
		evidence="$1"
		shift
		;;
	esac
done

# `--faulty` is required rather than optional, and the rows that need it are
# never skipped. Nine of scenario 3's rows can only be driven by a build that
# fails on purpose; a runner that quietly stepped over them when it was not
# given one would report a green restoration matrix that had proven nothing
# about a panic.
[ -n "$faulty" ] || usage

for path in "$binary" "$faulty"; do
	if [ ! -x "$path" ]; then
		printf 'smoke-tui: %s is not an executable binary\n' "$path" >&2
		exit 2
	fi
done
binary="$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")"
faulty="$(cd "$(dirname "$faulty")" && pwd)/$(basename "$faulty")"

if ! command -v python3 >/dev/null 2>&1; then
	printf 'smoke-tui: python3 is required (it hosts the pty driver, the VT oracle and the fixture server)\n' >&2
	exit 2
fi

evidence="${evidence:-${TMPDIR:-/tmp}/xfx-smoke-tui-$(date +%Y%m%dT%H%M%S)-$$}"
mkdir -p "$evidence"
evidence="$(cd "$evidence" && pwd)"

helpers="$evidence/helpers"
mkdir -p "$helpers"

# The slash registry, as the source this binary was built from declares it.
# Scenario 16 reads its commands, their order and their categories out of
# `src/interactive.rs`'s `SLASH_REGISTRY` rather than out of a list kept here --
# a list kept here goes stale the moment a command lands, and one read out of
# the *binary* would agree with whatever the popup rendered. The source is
# neither: it is what the popup is required to show, read the way
# `scripts/check-no-stubs.sh` reads it, without building anything. Copied beside
# the helpers so the evidence directory holds the registry a run was judged
# against and a scenario re-run by hand needs nothing else.
registry="$(cd "$(dirname "$0")/.." && pwd)/src/interactive.rs"
if [ ! -f "$registry" ]; then
	printf 'smoke-tui: %s is missing; the slash menu row is judged against the registry it declares\n' "$registry" >&2
	exit 2
fi
cp "$registry" "$helpers/interactive.rs"

# ---------------------------------------------------------------------------
# the four helpers
# ---------------------------------------------------------------------------

cat >"$helpers/vt_grid.py" <<'PYTHON'
"""A bounded VT emulator: exactly the sequences xfx says it emits, and nothing else.

`pyte` would be the obvious choice and is deliberately not used. It is not on
the CI images, and adding a `pip install` to a suite whose first rule is "no
network" would be worse than the hundred and fifty lines below -- which buy
something `pyte` could not give anyway: an **unknown sequence is a failure**.
That turns the emulator into a contract as well as an oracle. xfx declares the
subset it emits (`src/tui/term.rs`, `src/tui/frame.rs`, `src/tui/shell.rs`'s
`CLEAR_SCREEN`); a build that started emitting anything else fails the run here
rather than being shrugged at by a permissive emulator and noticed by a user.

**The rules are `src/tui/check.rs`'s**, the product's own refusal of a vector
before it is written, restated here rather than loosened: what that checker
admits this emulator admits, and what it refuses is a finding here too. Two
places where the product writes outside the checker's reach -- the launch's
push and the exit's report, both through `src/tui/term.rs` -- are the reason
`CR` is still accepted although the checker's alphabet has none.

What is modelled, and nothing else:

* `CUP` (`CSI row ; col H`, both numbers, on the screen), `ED`
  (`CSI 0/2/3 J`), `EL` (`CSI 0 K`)
* `CHA` (`CSI col G`, a column on the screen) and `ECH` (`CSI n X`, one cell
  or more, none past the last column, the caret left where it is): what the
  painter writes around a glyph whose suffix holds a variation selector
  (`src/tui/grid.rs`'s `put_unit`, upstream's `engine.zig:2548-2589`) -- the
  clear in front of it and the column after it. Both stay on the caret's row,
  so `check.rs` takes them under an open link where it refuses `CUP`, `ED`
  and `EL`, and so does this
* `SGR` (`CSI ... m`), remembered per cell so a colour can be asserted on --
  the foreground colour in `attrs`, the background in `backgrounds`, and the
  text attributes (bold, dim, italic, underline, reverse, strike) beside them in
  `styles`, because the vocabulary is `src/tui/pacer.rs::is_palette_sgr`'s and
  `check.rs`'s `wire_color`: groups of a reset or the default foreground (`0`,
  `39`), a text attribute or its close (`1 2 3 4 7 9 22 23 24 27 29` -- the
  reverse pair is a draft's caret cell and a composer selection), one
  foreground colour (`38;5;n`, `38;2;r;g;b`), or one background and its close
  (`48;5;n`, `49` -- the file picker's), every number a
  terminal reads (`0-255`). One group outside it and the whole sequence is a
  finding
* `DECSET`/`DECRST` for the six private modes xfx sets: `?2026` (synchronized
  output), `?25` (cursor visibility), `?7` (autowrap -- xfx turns it **off**,
  which is why the wrap below is not the usual one), `?2004` (bracketed paste),
  `?1049` (**the alternate screen buffer**, with its own cells and its own
  cursor), `?2031` (the theme-change subscription -- accepted and nothing
  else, since a subscribe or unsubscribe moves no cell)

The `?1049` pair is modelled rather than ignored, and that is what makes the
Phase-2 approval screen assertable at all. A terminal answers `?1049h` by saving
the cursor, switching to a second, blank buffer, and answering `?1049l` by
switching back and restoring the cursor -- the normal buffer's cells untouched
throughout, and no scrollback on the alternate one. An emulator that treated the
pair as a no-op would report a diff painted on the other plane as a diff painted
over the user's document, and would report a restore that never happened as a
band that came back.
* `CSI 6 n` (the launch's cursor query) and the three `>`/`<` keyboard-protocol
  sequences of `term.rs:32-52`
* `OSC 2` (the window title, remembered rather than drawn; `BEL`-terminated and
  free of control characters, as `check.rs` spells one), `OSC 11` (the
  background query, `11;?` and `ST`-terminated -- the reply is the terminal's
  and never on this wire) and `OSC 8` (a hyperlink, `ST`-terminated, by
  `check.rs`'s `wire_link`: parameters empty or `id=` and a short
  `[A-Za-z0-9._-]` run, a target of at most 2083 bytes with no control
  character in one of `http`, `https`, `file`, `mailto`, and a close that
  carries nothing). A link is remembered per cell in `links`, and one left open
  across a linefeed, a `CUP`, an erase, a plane change or the end of a
  synchronized update is a finding, as each is a refusal there. And `OSC 7501`
  (the program status, `ST`-terminated, by `check.rs`'s
  `program_status_command`: `state=clear` alone, or one of the state words --
  `idle`, `working`, `done`, `error`, `blocked:kind=permission`,
  `blocked:kind=question` -- then `:app=xfx`, and on a blocked state only an
  optional `:msg=` of canonical standard base64, at most 344 characters,
  decoding to at most 256 bytes of UTF-8 with no control character). Each is
  kept in `statuses`; one written inside an open synchronized update is a
  finding, since `check.rs` refuses a frame that carries one. **Every other
  `OSC` is a finding**, `OSC 52` first among them -- a band that started writing
  the user's clipboard would otherwise be an escape sequence nobody looked at
* `XTWINOPS` `CSI 22 ; 2 t` / `CSI 23 ; 2 t`, the title stack xfx pushes at
  entry and pops on every restore path
* `CR`, `LF` with a scroll at the bottom margin, and `BEL` -- counted in
  `bells` and moving nothing, `/sound`'s bell (`check.rs`'s `Intent::Bell`)
* whether a synchronized update is open (`synchronized`): `check.rs` refuses a
  vector that leaves one open, and the wire's form of that is a session that
  gives the terminal back inside one, which scenario 1 asserts against
* a cluster written past the last column while autowrap is off is a finding:
  the terminal would clip it, and the checker refuses it before it is written

Text is put on the screen a **grapheme cluster** at a time, as wide as
`unicode-width` says it is -- which is what `src/tui/wrap.rs` measures the
product's own rows with. An emulator that counted scalars would put a ZWJ family
in six cells and then report the product's correct row as a column-count defect.

Everything else lands in `unknown`, and a scenario that finds `unknown`
non-empty fails. `helpers/oracle_test.py` is where each of those claims is
falsified before any scenario trusts it.
"""

import re
import unicodedata

# ESC [ <private> <params> <intermediates> <final>. The private prefix is kept
# out of the parameter string on purpose: `CSI > 4 ; 2 m` is a keyboard-protocol
# request and `CSI 4 ; 2 m` is a colour, and an emulator that folded the two
# would silently accept a band painted with the wrong one.
CSI = re.compile(rb"\x1b\[(?P<private>[<=>?]?)(?P<params>[0-9;]*)(?P<inter>[ -/]*)(?P<final>[@-~])")

# ESC ] ... BEL, or ESC ] ... ESC \. The body excludes both terminators, so a
# truncated OSC does not swallow the rest of the stream. Which of the two ended
# it is kept: each string this crate writes is spelled with exactly one of them.
OSC = re.compile(rb"\x1b\](?P<body>[^\x07\x1b]*)(?P<end>\x07|\x1b\\)")

# The private modes xfx is allowed to set. Anything else is a finding.
#
# `?2031` is the theme-change subscription (`src/tui/term.rs:56,65`): a session
# turns it on in its mode set and off in every restore, the same enable/disable
# pair as `?2004` and `?7` above it, so a terminal volunteers a notification
# rather than being polled for one.
KNOWN_MODES = {"?2026", "?25", "?7", "?2004", "?1049", "?2031"}

# The `OSC` strings xfx is allowed to write, each with the one terminator it is
# written with (`src/tui/check.rs`'s `operating_system_command`): the window
# title (`src/tui/frame.rs`, `BEL`), the background query it asks the terminal
# (`src/tui/theme.rs:67`, `ST`), and a hyperlink (`src/tui/hyperlink.rs`,
# `ST`). `OSC 52` is deliberately **not** here -- a build that started writing
# the clipboard must fail this run rather than be shrugged at.
KNOWN_OSC = {"2": b"\x07", "11": b"\x1b\\", "8": b"\x1b\\", "7501": b"\x1b\\"}

# The program status (`src/tui/program_status.rs`, `check.rs`'s
# `program_status_command`), restated rather than imported: the state words,
# the name the record is kept under, and the message's two bounds.
STATUS_STATES = (
    "idle",
    "working",
    "done",
    "error",
    "blocked:kind=permission",
    "blocked:kind=question",
)
STATUS_APP = "xfx"
STATUS_MAX_ENCODED = 344
STATUS_MAX_MESSAGE = 256
BASE64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

# What a hyperlink may carry (`check.rs`'s `wire_link`, `hyperlink.rs`'s
# `MAX_URI_BYTES` and `MAX_ID_BYTES`), restated rather than imported.
LINK_ID = re.compile(r"^id=[A-Za-z0-9._-]{1,64}$")
LINK_SCHEMES = ("http", "https", "file", "mailto")
MAX_URI_BYTES = 2083

# The window operations xfx is allowed to write: push and pop of the title
# stack, so a session gives the terminal's title back the way it found it.
KNOWN_WINDOW_OPS = {"22;2", "23;2"}

# The keyboard-protocol sequences of `src/tui/term.rs:32-52`, spelled out:
# `>4;2m`/`>4;0m` are modifyOtherKeys on and off, `>1u` pushes the kitty
# keyboard flags and `<u` pops them. `?996n` (`src/tui/theme.rs:105`) is the
# outright "which way round are you now?" query a launch asks once, paired
# with the `?2031` subscription above but not itself an enable/disable pair --
# it carries no parameter, and a report with one (`?996;1n`) is a sequence
# nobody sends and stays a finding.
KNOWN_PRIVATE = {
    (">", "4;2", "m"),
    (">", "4;0", "m"),
    (">", "1", "u"),
    ("<", "", "u"),
    ("?", "996", "n"),
}

# How many rows of what left the top of the screen are kept.
SCROLLBACK_ROWS = 2000


# The joiner that makes several emoji one, and the range regional indicators
# come from -- two flags of them are one flag.
ZWJ = "\u200d"
REGIONAL = range(0x1F1E6, 0x1F200)


def decoded_base64(encoded):
    """Standard base64 with its padding, canonical only, or `None`.

    Restated from `check.rs`'s `wire_base64`: whole quads of the alphabet,
    `=` only at the end and at most two of it, and the bits the padding leaves
    over zero (`YQ==`, never `YR==`).
    """
    if len(encoded) % 4:
        return None
    pad = len(encoded) - len(encoded.rstrip("="))
    if pad > 2:
        return None
    body = encoded[: len(encoded) - pad]
    if any(c not in BASE64_ALPHABET for c in body):
        return None
    out = bytearray()
    for start in range(0, len(encoded), 4):
        quad = 0
        for c in encoded[start : start + 4]:
            quad = (quad << 6) | (0 if c == "=" else BASE64_ALPHABET.index(c))
        last = start + 4 == len(encoded)
        if last and pad == 2 and quad & 0xFFFF:
            return None
        if last and pad == 1 and quad & 0xFF:
            return None
        three = bytes(((quad >> 16) & 0xFF, (quad >> 8) & 0xFF, quad & 0xFF))
        out += three[: 3 - pad] if last else three
    return bytes(out)


def cell_width(character):
    """How many cells one character occupies.

    The band's geometry is computed with `unicode-width`, so an oracle that
    counted every scalar as one cell would disagree with the product about
    where the caret is on any row holding CJK or an emoji -- and would report
    that disagreement as a product defect.
    """
    if unicodedata.combining(character) or unicodedata.category(character) in ("Mn", "Me", "Cf"):
        return 0
    return 2 if unicodedata.east_asian_width(character) in ("W", "F") else 1


def is_regional(character):
    return ord(character) in REGIONAL


def clusters(text):
    """`text` split into the units a terminal draws in one cell entry.

    Not the whole of UAX #29 -- the standard library carries no segmentation
    table, and this needs to agree with `unicode-width` over what xfx actually
    paints rather than over every string. Three rules cover that:

    * a zero-width scalar (a combining mark, a variation selector, the ZWJ
      itself) belongs to the cluster in front of it;
    * a scalar after a ZWJ belongs to the same cluster, which is what makes a
      ZWJ family one cluster instead of three emoji;
    * two regional indicators are one flag.

    Anything the rules do not join starts a cluster of its own, so a fix that
    over-joined would be caught by the wide-scalar control in
    `oracle_test.py`.
    """
    out = []
    for character in text:
        if out:
            previous = out[-1]
            if cell_width(character) == 0:
                out[-1] = previous + character
                continue
            if previous.endswith(ZWJ):
                out[-1] = previous + character
                continue
            if is_regional(character) and len(previous) == 1 and is_regional(previous):
                out[-1] = previous + character
                continue
        out.append(character)
    return out


def cluster_width(cluster):
    """How many cells one cluster occupies, as `unicode-width` measures it.

    A pair of regional indicators is a flag and takes two; otherwise the
    cluster is as wide as its widest scalar, which is what makes a ZWJ family
    two columns rather than six.
    """
    if len(cluster) == 2 and all(is_regional(scalar) for scalar in cluster):
        return 2
    return max(cell_width(scalar) for scalar in cluster)


class Grid:
    """A bounded VT: exactly the sequences xfx says it emits, and nothing else."""

    def __init__(self, rows, cols, record_frames=False):
        self.rows, self.cols = rows, cols
        self.cells = [[" "] * cols for _ in range(rows)]
        self.attrs = [[""] * cols for _ in range(rows)]
        # The text attributes each cell was written with, as the sorted SGR
        # numbers that are on (`"1"` bold, `"2"` dim, `"1;3"` bold italic) and
        # `""` for none. Beside `attrs` rather than folded into it, so every
        # assertion about a colour stays an assertion about a colour: a bold
        # card and a plain row in the same grey are the same foreground.
        self.styles = [[""] * cols for _ in range(rows)]
        # The background each cell was written with, as the `48;...` group that
        # set it and `""` for the terminal's own -- apart from `attrs` for the
        # reason `styles` is: an assertion about the foreground stays one.
        self.backgrounds = [[""] * cols for _ in range(rows)]
        # The hyperlink target each cell was written under (`OSC 8`), `""` for
        # none: which text a link really covers is a claim about cells.
        self.links = [[""] * cols for _ in range(rows)]
        self.row = self.col = 0
        self.sgr = ""
        self.style = ""
        self.background = ""
        # The link the stream has open, and `""` while none is.
        self.link = ""
        self.autowrap = True
        # Whether a synchronized update (`?2026h`) is open.
        self.synchronized = False
        # How many `BEL`s reached the terminal. A bell moves nothing, so this is
        # the only place it can be read back from.
        self.bells = 0
        self.unknown = []
        # Which of the terminal's two buffers is on the screen: "primary" is the
        # normal one xfx paints its band on, "alternate" is the one an approval
        # screen borrows. A string rather than a bool because every reading of it
        # in a scenario is a sentence about a plane.
        self.plane = "primary"
        # The normal buffer while the alternate one is up: its cells, its
        # attributes and the cursor the terminal saved with them. `None` while
        # the normal buffer is the one on the screen.
        self.saved = None
        # How many times each half of the pair was seen, so "entered once and
        # left once" is a count rather than a search through the raw bytes.
        self.entered_alternate = 0
        self.left_alternate = 0
        # The screen after each complete synchronized frame, oldest first, for a
        # scenario that has to assert about **every** state a terminal could
        # have presented rather than about the one it ended in. Off by default:
        # a paced stream produces thousands of frames and no scenario asserts on
        # more than the recent past of one.
        self.record_frames = record_frames
        self.frames = []
        # What has left the top of the screen, which is what a terminal's own
        # scrollback is fed by. Modelled rather than discarded because the
        # launch's whole job is to push the shell's earlier output *there*: an
        # oracle with no scrollback would report a successful push as erased
        # output, and a band painted over the shell's lines as a successful
        # push. Bounded, because a paced stream can produce a great many rows
        # and nothing here asserts on more than the recent past.
        self.scrollback = []
        # The last window title `OSC 2` asked for, and `None` while none has
        # been asked for. Remembered rather than drawn: a title changes no cell,
        # and a scenario that wants to assert one needs somewhere to read it.
        self.title = None
        # Every `OSC 7501` program status the stream wrote, as its body after
        # `7501;`, in order. A report moves no cell, so it is kept here for
        # the scenarios that assert on one.
        self.statuses = []

    # -- feeding ----------------------------------------------------------

    def feed(self, data):
        index = 0
        while index < len(data):
            byte = data[index]
            if byte == 0x1B:
                index = self._escape(data, index)
                continue
            if byte == 0x0A:
                self._newline()
            elif byte == 0x0D:
                self.col = 0
            elif byte == 0x07:
                self.bells += 1  # a bell moves no cell
            elif byte < 0x20 or byte == 0x7F:
                # A control byte that reached the screen is exactly the class of
                # defect this emulator exists to catch: xfx strips controls from
                # a row before placing it (`src/tui/frame.rs`), so one arriving
                # here means a provider's byte is being obeyed.
                self.unknown.append(bytes(data[index : index + 8]))
            else:
                index = self._text(data, index)
                continue
            index += 1
        return self

    def _text(self, data, index):
        """Decodes the run of printable text at `index` and puts it on the screen.

        A **run** rather than a character, because a grapheme cluster is several
        scalars and the cluster is the unit a terminal draws: decoding one
        character at a time would hand `_put` the halves of a ZWJ family and no
        rule downstream could put them back together.
        """
        end = index
        while end < len(data):
            byte = data[end]
            if byte == 0x1B or byte == 0x0A or byte == 0x0D or byte == 0x07:
                break
            if byte < 0x20 or byte == 0x7F:
                break
            end += 1
        text = bytes(data[index:end]).decode("utf-8", "replace")
        for cluster in clusters(text):
            self._put(cluster)
        return end

    def _put(self, character):
        width = cluster_width(character)
        if width == 0:
            # A combining mark that reached the screen on its own -- across an
            # escape sequence, or as the first thing on a row -- belongs to the
            # cell before it rather than to one of its own.
            if self.col > 0:
                self.cells[self.row][self.col - 1] += character
            return
        if self.col + width > self.cols:
            if self.autowrap:
                self.col = 0
                self._newline()
            else:
                # `?7l` is on: the terminal overwrites the last cell instead of
                # moving to the next row -- so the cluster is a clipped write,
                # which `check.rs` refuses before it is written ("a cluster
                # written past the last column"). A finding, and modelled as the
                # terminal would draw it so the grid stays one a terminal shows.
                self.unknown.append(b"a cluster past the last column: " + character.encode("utf-8"))
                self.col = self.cols - width
        self.cells[self.row][self.col] = character
        self.attrs[self.row][self.col] = self.sgr
        self.styles[self.row][self.col] = self.style
        self.backgrounds[self.row][self.col] = self.background
        self.links[self.row][self.col] = self.link
        for offset in range(1, width):
            self.cells[self.row][self.col + offset] = ""
            self.attrs[self.row][self.col + offset] = self.sgr
            self.styles[self.row][self.col + offset] = self.style
            self.backgrounds[self.row][self.col + offset] = self.background
            self.links[self.row][self.col + offset] = self.link
        self.col += width

    def _newline(self):
        if self.link:
            self.unknown.append(b"a hyperlink left open across a row")
        if self.row + 1 >= self.rows:
            # The bottom margin: a linefeed scrolls, which is how xfx's document
            # appends reach the terminal's own scrollback. **The normal buffer's
            # only**: a terminal keeps no scrollback for its alternate buffer, so
            # a row that leaves the top of that one is gone -- which is exactly
            # why nothing the document owes may be written while it is up.
            leaving = self.cells.pop(0)
            if self.plane == "primary":
                self.scrollback.append("".join(leaving).rstrip())
            del self.scrollback[:-SCROLLBACK_ROWS]
            self.attrs.pop(0)
            self.styles.pop(0)
            self.backgrounds.pop(0)
            self.links.pop(0)
            self.cells.append([" "] * self.cols)
            self.attrs.append([""] * self.cols)
            self.styles.append([""] * self.cols)
            self.backgrounds.append([""] * self.cols)
            self.links.append([""] * self.cols)
        else:
            self.row += 1

    # -- escape sequences -------------------------------------------------

    def _escape(self, data, index):
        match = CSI.match(data, index)
        if not match:
            match = OSC.match(data, index)
            if not match:
                self.unknown.append(bytes(data[index : index + 8]))
                return index + 1
            self._osc(match)
            return match.end()
        private = match.group("private").decode()
        params = match.group("params").decode()
        final = match.group("final").decode()
        if match.group("inter"):
            self.unknown.append(match.group(0))
            return match.end()
        if private:
            self._private(private, params, final, match)
            return match.end()
        if final == "G":
            self._column(params, match)
            return match.end()
        if final == "X":
            self._erase_characters(params, match)
            return match.end()
        if final in "HJK" and self.link:
            # A caret move or an erase under an open link would make what it
            # moves to, or what it blanks, part of the link (`check.rs`).
            self.unknown.append(b"a hyperlink left open across " + match.group(0))
        if final == "H":
            self._position(params, match)
        elif final == "J":
            self._erase_screen(params or "0", match)
        elif final == "K":
            if params in ("", "0"):
                self._erase_right()
            else:
                self.unknown.append(match.group(0))
        elif final == "m":
            self._sgr(params, match)
        elif final == "n":
            if params != "6":
                self.unknown.append(match.group(0))
        elif final == "t":
            # `XTWINOPS`. Only the title stack, and it changes no cell.
            if params not in KNOWN_WINDOW_OPS:
                self.unknown.append(match.group(0))
        else:
            self.unknown.append(match.group(0))
        return match.end()

    def _position(self, params, match):
        """`CUP`: both numbers, and a cell on the screen (`check.rs`'s `position`).

        A bare `CSI H` is the origin. Anything else names a row and a column, and
        an address off the screen -- a `0`, or past the last row or column -- is a
        finding rather than a clamp: a terminal would clamp it, and the caret
        would land somewhere the product did not say.
        """
        if params == "":
            self.row = self.col = 0
            return
        parts = params.split(";")
        if len(parts) != 2 or not all(part.isdigit() for part in parts):
            self.unknown.append(match.group(0))
            return
        row, col = int(parts[0]), int(parts[1])
        if not (1 <= row <= self.rows and 1 <= col <= self.cols):
            self.unknown.append(match.group(0))
        self.row = min(max(row - 1, 0), self.rows - 1)
        self.col = min(max(col - 1, 0), self.cols - 1)

    def _column(self, params, match):
        """`CHA`: a column on the screen (`check.rs`'s `CSI G` arm), the row kept.

        A `0`, an empty parameter or a column past the last is a finding rather
        than a clamp, as an address off the screen is for `CUP`.
        """
        if not params.isdigit() or not 1 <= int(params) <= self.cols:
            self.unknown.append(match.group(0))
            return
        self.col = int(params) - 1

    def _erase_characters(self, params, match):
        """`ECH`: `n` cells from the caret blanked, the caret left where it is.

        The whole of a wide cluster the span covers half of goes with it, as
        `check.rs`'s `erase_span` has it: the lead of a continuation it starts
        on, the continuation of a lead it ends on.
        """
        if not params.isdigit() or int(params) == 0 or self.col + int(params) > self.cols:
            self.unknown.append(match.group(0))
            return
        start, end = self.col, self.col + int(params)
        while start > 0 and self.cells[self.row][start] == "":
            start -= 1
        while end < self.cols and self.cells[self.row][end] == "":
            end += 1
        for col in range(start, end):
            self.cells[self.row][col] = " "
            self.attrs[self.row][col] = ""
            self.styles[self.row][col] = ""
            self.backgrounds[self.row][col] = ""
            self.links[self.row][col] = ""

    def _osc(self, match):
        """An `OSC` string: no cell moves, but the string has to be one of ours."""
        body = match.group("body").decode("utf-8", "replace")
        number, separator, argument = body.partition(";")
        if number not in KNOWN_OSC or not separator or match.group("end") != KNOWN_OSC[number]:
            self.unknown.append(match.group(0))
            return
        if number == "2":
            if any(unicodedata.category(c) == "Cc" for c in argument):
                self.unknown.append(match.group(0))
                return
            self.title = argument
        elif number == "11":
            if argument != "?":
                self.unknown.append(match.group(0))
        elif number == "7501":
            self._program_status(argument, match)
        else:
            self._hyperlink(argument, match)

    def _program_status(self, argument, match):
        """`OSC 7501`, by `check.rs`'s `program_status_command`.

        Kept in `statuses` as its body after `7501;` when it is one of the
        shapes the product writes; anything else, and one written inside an
        open update, is a finding.
        """
        if self.synchronized:
            self.unknown.append(b"a program status inside an update: " + match.group(0))
            return
        if not argument.startswith("state="):
            self.unknown.append(match.group(0))
            return
        rest = argument[len("state=") :]
        if rest == "clear":
            self.statuses.append(argument)
            return
        for word in STATUS_STATES:
            lead = word + ":app=" + STATUS_APP
            if rest == lead or rest.startswith(lead + ":msg="):
                break
        else:
            self.unknown.append(match.group(0))
            return
        tail = rest[len(lead) :]
        if tail:
            encoded = tail[len(":msg=") :]
            message = decoded_base64(encoded)
            if (
                not word.startswith("blocked:")
                or not encoded
                or len(encoded) > STATUS_MAX_ENCODED
                or message is None
                or len(message) > STATUS_MAX_MESSAGE
            ):
                self.unknown.append(match.group(0))
                return
            try:
                text = message.decode("utf-8")
            except UnicodeDecodeError:
                self.unknown.append(match.group(0))
                return
            if any(unicodedata.category(c) == "Cc" for c in text):
                self.unknown.append(match.group(0))
                return
        self.statuses.append(argument)

    def _hyperlink(self, argument, match):
        """`OSC 8`, by `check.rs`'s `wire_link`: a close, or an inert target."""
        params, separator, uri = argument.partition(";")
        if not separator:
            self.unknown.append(match.group(0))
            return
        if uri == "":
            if params:
                self.unknown.append(match.group(0))
                return
            self.link = ""
            return
        scheme, colon, _ = uri.partition(":")
        if (
            (params and not LINK_ID.match(params))
            or len(uri.encode("utf-8")) > MAX_URI_BYTES
            or any(unicodedata.category(c) == "Cc" for c in uri)
            or not colon
            or scheme.lower() not in LINK_SCHEMES
        ):
            self.unknown.append(match.group(0))
            return
        self.link = uri

    def _sgr(self, params, match):
        """One `SGR`, against the vocabulary `src/tui/pacer.rs` actually emits.

        The narrow question, not the SGR grammar: what a *row* may carry is a
        256-colour or direct-colour foreground, the text attributes the
        markdown presenter, the card and the composer use (bold, dim, italic,
        underline, reverse, strike) with their closes, the file picker's
        background and its close, and the resets that end them
        (`src/tui/pacer.rs::is_palette_sgr`, mirrored by `src/tui/check.rs`'s
        `wire_color`). An emulator that took the whole vocabulary would accept
        a band that had started to blink, to conceal a row, or to paint in a
        colour no terminal reads.

        **Atomic**: the groups are checked first and applied only if every one
        of them is in the vocabulary, so a sequence that is half a known
        colour and half a blink is a finding that changed nothing rather than a
        finding that left the pen half moved.
        """
        if params in ("", "0"):
            self.sgr, self.style, self.background = "", "", ""
            return
        parts = params.split(";")
        steps = []
        index = 0
        while index < len(parts):
            part = parts[index]
            if part in ("0", "39", "49", "1", "2", "3", "4", "7", "9", "22", "23", "24", "27", "29"):
                steps.append(part)
                index += 1
                continue
            if part in ("38", "48") and index + 1 < len(parts):
                # A direct background is no shape the product writes: the
                # file picker's bar is `48;5` at every depth.
                shapes = {"5": 1, "2": 3} if part == "38" else {"5": 1}
                wanted = shapes.get(parts[index + 1])
                values = parts[index + 2 : index + 2 + (wanted or 0)]
                if (
                    wanted is not None
                    and len(values) == wanted
                    and all(value.isdigit() and int(value) <= 255 for value in values)
                ):
                    steps.append("%s;%s;%s" % (part, parts[index + 1], ";".join(values)))
                    index += 2 + wanted
                    continue
            self.unknown.append(match.group(0))
            return
        colour, background = self.sgr, self.background
        style = set(self.style.split(";")) - {""}
        closes = {"22": ("1", "2"), "23": ("3",), "24": ("4",), "27": ("7",), "29": ("9",)}
        for step in steps:
            if step == "0":
                colour, background, style = "", "", set()
            elif step == "39":
                colour = ""
            elif step == "49":
                background = ""
            elif step in closes:
                style -= set(closes[step])
            elif step.startswith("38;"):
                colour = "\x1b[%sm" % step
            elif step.startswith("48;"):
                background = "\x1b[%sm" % step
            else:
                style.add(step)
        self.sgr = colour
        self.background = background
        self.style = ";".join(sorted(style, key=int))

    def _private(self, private, params, final, match):
        if private == "?" and final in ("h", "l"):
            if "?" + params not in KNOWN_MODES:
                self.unknown.append(match.group(0))
                return
            if params == "7":
                self.autowrap = final == "h"
            if params == "1049":
                if self.link:
                    self.unknown.append(b"a hyperlink left open across a buffer change")
                self._alternate(final == "h")
            if params == "2026":
                if final == "l":
                    # An update is a screen a terminal presents, and a link
                    # still open at its end would take in whatever the next one
                    # paints first (`check.rs`: "a vector that left a
                    # hyperlink open").
                    if self.link:
                        self.unknown.append(b"a hyperlink left open across an update's end")
                    self._frame_closed()
                self.synchronized = final == "h"
            return
        if (private, params, final) in KNOWN_PRIVATE:
            return
        self.unknown.append(match.group(0))

    def _alternate(self, entering):
        """`?1049h` / `?1049l`: the second buffer, and the cursor saved with it.

        Modelled the way a terminal really behaves, because every assertion the
        approval scenarios make rests on the difference between the two:

        * entering **saves** the normal buffer and the cursor and hands out a
          blank one, so text painted on the alternate plane cannot be mistaken
          for text painted over the user's document;
        * leaving **restores** both, so "the band came back" is a claim the
          emulator can be wrong about -- and a product that painted no band on
          the way back would leave the pre-excursion screen standing, which is
          why the scenario compares the restored screen with the band it is
          required to repaint rather than merely with "not blank".

        Entering twice, or leaving a plane that was never entered, is a finding:
        the pair has to balance or the user is left on a buffer nobody owns.
        """
        if entering:
            self.entered_alternate += 1
            if self.plane == "alternate":
                self.unknown.append(b"\x1b[?1049h twice")
                return
            self.saved = (
                [row[:] for row in self.cells],
                [row[:] for row in self.attrs],
                self.row,
                self.col,
                self.sgr,
                [row[:] for row in self.styles],
                self.style,
                [row[:] for row in self.backgrounds],
                [row[:] for row in self.links],
                self.background,
            )
            self.cells = [[" "] * self.cols for _ in range(self.rows)]
            self.attrs = [[""] * self.cols for _ in range(self.rows)]
            self.styles = [[""] * self.cols for _ in range(self.rows)]
            self.backgrounds = [[""] * self.cols for _ in range(self.rows)]
            self.links = [[""] * self.cols for _ in range(self.rows)]
            self.row = self.col = 0
            self.plane = "alternate"
            return
        self.left_alternate += 1
        if self.plane == "primary":
            # A `1049l` on the normal buffer is what every abnormal restore
            # writes defensively (`src/tui/term.rs`), and a terminal answers it
            # by doing nothing. Counted, so a scenario can still say how many
            # there were, and otherwise ignored.
            return
        cells, attrs, row, col, sgr, styles, style, backgrounds, links, background = self.saved
        self.cells, self.attrs, self.styles = cells, attrs, styles
        self.backgrounds, self.links = backgrounds, links
        self.row, self.col, self.sgr, self.style = row, col, sgr, style
        self.background = background
        self.saved = None
        self.plane = "primary"

    def _frame_closed(self):
        """One synchronized frame has been presented."""
        if self.record_frames:
            self.frames.append(self.text())

    def _blank_row(self, row, start=0):
        for col in range(start, self.cols):
            self.cells[row][col] = " "
            self.attrs[row][col] = ""
            self.styles[row][col] = ""
            self.backgrounds[row][col] = ""
            self.links[row][col] = ""

    def _erase_right(self):
        self._blank_row(self.row, self.col)

    def _erase_screen(self, params, match):
        if params == "0":  # from the cursor to the end of the screen
            self._erase_right()
            for row in range(self.row + 1, self.rows):
                self._blank_row(row)
        elif params in ("2", "3"):
            # `2` is the visible screen and `3` is the terminal's own scrollback,
            # which this emulator does not model -- it has no scrollback to
            # erase, so erasing it is a no-op rather than an unknown sequence.
            if params == "2":
                for row in range(self.rows):
                    self._blank_row(row)
        else:
            self.unknown.append(match.group(0))

    # -- reading ----------------------------------------------------------

    def row_text(self, row):
        return "".join(cell for cell in self.cells[row]).rstrip()

    def text(self):
        """The visible screen, and only that."""
        return "\n".join(self.row_text(row) for row in range(self.rows))

    def scrollback_text(self):
        """What has left the top of the screen, oldest first."""
        return "\n".join(self.scrollback)

    def document_text(self):
        """Everything the terminal holds: its scrollback and then its screen.

        The **normal** buffer's, and only its: xfx paints its band and its
        document there, so a row that scrolled off is still the user's, one
        wheel-turn away. A screen borrowed for an approval has no scrollback and
        is not part of that document, which is why it is not folded in here.
        """
        return self.scrollback_text() + "\n" + self.text()

    def primary_text(self):
        """The normal buffer, whichever plane is on the screen.

        The saved copy while an approval screen is up, and the live cells
        otherwise -- so a scenario can ask "what is the user's own screen still
        holding" at a moment when it is not the one being displayed.
        """
        if self.plane == "primary":
            return self.text()
        cells = self.saved[0]
        return "\n".join("".join(row).rstrip() for row in cells)

    def find(self, needle):
        """Where `needle` first appears, as `(row, column)`, or `None`.

        The column is a cell index, so the attribute of the cell after a marker
        is `attrs[row][column + len(needle)]` -- which is how "no SGR leaked
        past the answer" is asked.
        """
        for row in range(self.rows):
            found = self.row_text(row).find(needle)
            if found >= 0:
                return (row, found)
        return None

    def attrs_of_row(self, row):
        """Every distinct attribute run on `row`, in order.

        The comparison two themes are told apart by: what the palette *is*
        belongs to `src/tui/theme.rs`, and a harness that spelled the greys out
        would pass for whatever numbers that module happened to declare.
        """
        runs = []
        for attr in self.attrs[row]:
            if attr and (not runs or runs[-1] != attr):
                runs.append(attr)
        return runs

    def styles_of_row(self, row):
        """Every distinct text-attribute run on `row`, in order, as `attrs_of_row`."""
        runs = []
        for style in self.styles[row]:
            if style and (not runs or runs[-1] != style):
                runs.append(style)
        return runs

    def is_plain(self, row, col):
        """Whether the cell carries no colour, no background, no text attribute
        and no link at all.

        The question "did a pen leak past here" has as many halves as a pen has
        slots: a cell back on the default foreground but still bold, still on a
        background or still inside a link is a leak exactly as much as a
        coloured one.
        """
        return (
            self.attrs[row][col] == ""
            and self.styles[row][col] == ""
            and self.backgrounds[row][col] == ""
            and self.links[row][col] == ""
        )

    def snapshot(self):
        ruler = "     " + "".join(str(column % 10) for column in range(self.cols))
        lines = [ruler]
        for row in range(self.rows):
            lines.append("%3d  %s" % (row + 1, self.row_text(row)))
        lines.append("")
        lines.append("cursor: row %d col %d" % (self.row + 1, self.col + 1))
        lines.append("autowrap: %s" % ("on" if self.autowrap else "off"))
        lines.append(
            "plane: %s (entered %d, left %d)"
            % (self.plane, self.entered_alternate, self.left_alternate)
        )
        lines.append("title: %r" % (self.title,))
        if self.scrollback:
            lines.append("")
            lines.append("--- what left the top of the screen, oldest first ---")
            lines.extend(self.scrollback[-40:])
        if self.bells:
            lines.append("bells: %d" % self.bells)
        for row in range(self.rows):
            runs = self.attrs_of_row(row)
            if runs:
                lines.append("attrs row %d: %r" % (row + 1, runs))
            styles = self.styles_of_row(row)
            if styles:
                lines.append("styles row %d: %r" % (row + 1, styles))
            backgrounds = [b for i, b in enumerate(self.backgrounds[row]) if b and (i == 0 or self.backgrounds[row][i - 1] != b)]
            if backgrounds:
                lines.append("backgrounds row %d: %r" % (row + 1, backgrounds))
            links = [l for i, l in enumerate(self.links[row]) if l and (i == 0 or self.links[row][i - 1] != l)]
            if links:
                lines.append("links row %d: %r" % (row + 1, links))
        if self.unknown:
            lines.append("UNKNOWN SEQUENCES: %r" % (self.unknown,))
        return "\n".join(lines) + "\n"
PYTHON

cat >"$helpers/oracle_test.py" <<'PYTHON'
"""Falsification tests for the oracle in `vt_grid.py`, run before any scenario.

An oracle nobody has tried to break is not an oracle: it is a second copy of
whatever the product happens to do, and it reports agreement with itself as a
passing screen. Every scenario in this suite judges xfx against `vt_grid.Grid`,
so the emulator's own claims are checked here first -- and the run stops if one
of them is false.

Three claims are checked, and each is checked in both directions, because a
one-directional check is satisfied by an emulator that answers the same thing
to everything:

* **A grapheme cluster is one cell entry, as wide as a terminal draws it.** The
  product measures its rows with `unicode-width` (`src/tui/wrap.rs`), which
  gives a ZWJ emoji sequence **two** columns. An emulator that counted the
  scalars would put the family in six cells and then report the product's
  correct row as a column-count defect.
* **`unknown` really fills.** The emulator's value is that a sequence xfx does
  not declare fails the run; an allowlist with a hole in it is a permissive
  emulator wearing a contract's clothes. `OSC 52` (clipboard) and an
  unsupported SGR parameter are the two the band could plausibly grow by
  accident, so they are the two asserted on -- and beside them every refusal of
  `src/tui/check.rs` the emulator restates: a hyperlink that is not inert or is
  left open, a program status in any shape the product does not write or
  inside an update, a title or a query spelled with the other terminator, a
  caret placed off the screen, a cluster clipped at the last column.
* **The subset xfx really emits is still accepted.** A contract that rejects
  the product is a broken harness rather than a caught defect, so every shape
  `src/tui/term.rs`, `src/tui/theme.rs`, `src/tui/frame.rs` and
  `src/tui/hyperlink.rs` write is fed through and must leave `unknown` empty --
  the reverse video of a caret cell and the picker's background among them.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from vt_grid import Grid  # noqa: E402

# A three-person ZWJ family: five scalars, one cluster, two columns.
FAMILY = "\U0001F468\u200D\U0001F469\u200D\U0001F467"

# The product's two colour spellings -- a palette grey, `38;5` at every depth
# (`src/tui/theme.rs` `Palette`), and the Ultra mark's direct yellow
# (`ULTRA_TRUECOLOR`) -- and the reset that ends every band row.
PALETTE_SGR = ("\x1b[38;5;250m", "\x1b[38;2;255;204;0m", "\x1b[0m", "\x1b[m")

# The background query and the window title (`src/tui/theme.rs:67`,
# `src/tui/frame.rs`), each in the one terminator the product spells it with
# (`src/tui/check.rs`'s `operating_system_command`), and the exit's clear.
PRODUCT_OSC = ("\x1b]11;?\x1b\\", "\x1b]2;\x07", "\x1b]2;xfx \u00b7 glm-5.2\x07")

# A hyperlink as `src/tui/hyperlink.rs` writes one -- open with upstream's `id=`,
# the text, the close -- and the cells it covers.
PRODUCT_LINK = "\x1b]8;id=xfx-1;https://example.com/a\x1b\\docs\x1b]8;;\x1b\\ x"

# The title stack xfx pushes at entry and pops on every restore path
# (`src/tui/term.rs`).
TITLE_STACK = ("\x1b[22;2t", "\x1b[23;2t")

problems = []
checks = 0


def require(condition, what):
    global checks
    checks += 1
    if not condition:
        problems.append(what)


def fed(text, rows=3, cols=20):
    return Grid(rows, cols).feed(text.encode("utf-8"))


def a_zwj_family_is_one_cluster_two_cells_wide():
    grid = fed(FAMILY + "x")
    require(
        grid.cells[0][0] == FAMILY,
        "the family is one cell entry, not %r" % (grid.cells[0][0],),
    )
    require(
        grid.cells[0][1] == "",
        "the family's second column is its continuation, not %r" % (grid.cells[0][1],),
    )
    require(
        grid.cells[0][2] == "x",
        "the character after the family is at column 3, not %r" % (grid.cells[0][2],),
    )
    require(grid.col == 3, "the caret is three columns in, not %d" % grid.col)


def a_combining_mark_stays_with_the_cell_it_marks():
    grid = fed("e\u0301x")
    require(grid.cells[0][0] == "e\u0301", "the mark left its base: %r" % (grid.cells[0][0],))
    require(grid.cells[0][1] == "x", "the mark took a cell of its own")


def a_wide_scalar_is_still_two_cells():
    # The control for the cluster rule above: a fix that made every cluster one
    # column would pass the family assertion and break every CJK row.
    grid = fed("\ud55cx")
    require(grid.cells[0][0] == "\ud55c", "the wide scalar is one cell entry")
    require(grid.cells[0][1] == "", "the wide scalar's continuation")
    require(grid.cells[0][2] == "x", "the character after a wide one is at column 3")


def a_flag_is_one_cluster():
    grid = fed("\U0001F1F0\U0001F1F7x")
    require(
        grid.cells[0][0] == "\U0001F1F0\U0001F1F7",
        "a regional-indicator pair is one cluster, not %r" % (grid.cells[0][0],),
    )
    require(grid.cells[0][2] == "x", "the character after a flag is at column 3")


def a_clipboard_osc_is_a_finding():
    grid = fed("\x1b]52;c;aGVsbG8=\x07")
    require(grid.unknown, "OSC 52 passed the oracle unremarked")


def an_undeclared_osc_number_is_a_finding():
    grid = fed("\x1b]7;file:///tmp\x07")
    require(grid.unknown, "OSC 7 passed the oracle unremarked")


def the_osc_the_product_emits_is_accepted():
    grid = fed("".join(PRODUCT_OSC))
    require(not grid.unknown, "the product's own OSC was rejected: %r" % (grid.unknown,))
    require(
        grid.title == "xfx \u00b7 glm-5.2",
        "the window title was not read off OSC 2: %r" % (grid.title,),
    )


def an_osc_in_the_other_terminator_or_with_another_body_is_a_finding():
    """Each string in the one spelling `check.rs` admits, and only that one.

    The terminal's own *reply* to the background query is here too: it is
    something a terminal writes to xfx, never something xfx writes, so an
    emulator that accepted it on xfx's wire would accept a session echoing its
    input.
    """
    for sequence in (
        "\x1b]2;title\x1b\\",
        "\x1b]11;?\x07",
        "\x1b]11;rgb:0000/0000/0000\x1b\\",
        "\x1b]2;tab\there\x07",
        "\x1b]2\x07",
    ):
        require(fed(sequence).unknown, "%r passed the oracle unremarked" % sequence)


def a_hyperlink_the_product_writes_is_accepted_and_covers_its_cells():
    grid = fed(PRODUCT_LINK)
    require(not grid.unknown, "the product's own link was rejected: %r" % (grid.unknown,))
    require(
        [grid.links[0][col] for col in range(4)] == ["https://example.com/a"] * 4,
        "the link's text is not under its target: %r" % (grid.links[0][:5],),
    )
    require(grid.links[0][5] == "" and grid.is_plain(0, 5), "the close did not end the link")
    require(grid.link == "", "the stream was left inside a link")
    for scheme in ("http://a", "file:///tmp/a.png", "mailto:a@b.c", "HTTPS://A"):
        sequence = "\x1b]8;;%s\x1b\\x\x1b]8;;\x1b\\" % scheme
        require(not fed(sequence).unknown, "%r was rejected" % sequence)


def a_hyperlink_that_is_not_inert_or_is_left_open_is_a_finding():
    """`check.rs`'s `wire_link` and its three places a link may not cross."""
    close = "\x1b]8;;\x1b\\"
    for what, sequence in (
        ("a BEL-terminated open", "\x1b]8;;https://a\x07x" + close),
        ("a scheme nobody links", "\x1b]8;;javascript:alert(1)\x1b\\x" + close),
        ("a target with no scheme", "\x1b]8;;README.md\x1b\\x" + close),
        ("a control in the target", "\x1b]8;;https://a\x7fb\x1b\\x" + close),
        ("parameters nobody writes", "\x1b]8;foo=1;https://a\x1b\\x" + close),
        ("a close with parameters", "\x1b]8;;https://a\x1b\\x\x1b]8;id=a;\x1b\\"),
        ("a target past the cap", "\x1b]8;;https://" + "a" * 2083 + "\x1b\\x" + close),
        ("the recovery cleanup's BEL close", "\x1b]8;;\x07"),
        ("open across a row", "\x1b]8;;https://a\x1b\\x\ny" + close),
        ("open across a caret move", "\x1b]8;;https://a\x1b\\x\x1b[2;1Hy" + close),
        ("open across an erase", "\x1b]8;;https://a\x1b\\x\x1b[K" + close),
        ("open across an update's end", "\x1b[?2026h\x1b]8;;https://a\x1b\\x\x1b[?2026l" + close),
        ("open across a plane change", "\x1b]8;;https://a\x1b\\x\x1b[?1049h" + close),
    ):
        require(fed(sequence).unknown, "%s passed the oracle unremarked: %r" % (what, sequence))


def the_program_status_the_product_writes_is_accepted_and_kept():
    """Every shape `src/tui/program_status.rs` writes, and the restores' clear."""
    reports = (
        "state=idle:app=xfx",
        "state=working:app=xfx",
        "state=done:app=xfx",
        "state=error:app=xfx",
        "state=blocked:kind=permission:app=xfx",
        "state=blocked:kind=permission:app=xfx:msg=c2hlbGwucnVuIGdpdCBwdXNo",
        "state=blocked:kind=question:app=xfx:msg=V2hpY2ggYnJhbmNoPw==",
        # U+202E then `evil`: a bidi mark is text, kept inside the encoding.
        "state=blocked:kind=permission:app=xfx:msg=4oCuZXZpbA==",
        # 256 bytes, the message's bound.
        "state=blocked:kind=question:app=xfx:msg=" + "YWFh" * 85 + "YQ==",
        "state=clear",
    )
    grid = fed("".join("\x1b]7501;%s\x1b\\" % report for report in reports) + "x")
    require(not grid.unknown, "the product's own program status was rejected: %r" % (grid.unknown,))
    require(grid.statuses == list(reports), "the reports were not kept in order: %r" % (grid.statuses,))
    require(grid.cells[0][0] == "x", "a program status moved the caret")


def a_program_status_in_any_other_shape_is_a_finding():
    """`check.rs`'s `program_status_command`, each of its refusals."""
    for what, sequence in (
        ("a BEL terminator", "\x1b]7501;state=idle:app=xfx\x07"),
        ("another program's name", "\x1b]7501;state=idle:app=fx\x1b\\"),
        ("no name", "\x1b]7501;state=idle\x1b\\"),
        ("a state nobody writes", "\x1b]7501;state=busy:app=xfx\x1b\\"),
        ("a kind nobody writes", "\x1b]7501;state=blocked:kind=other:app=xfx\x1b\\"),
        ("no state", "\x1b]7501;app=xfx\x1b\\"),
        ("a clear with more after it", "\x1b]7501;state=clear:app=xfx\x1b\\"),
        ("a message where none is carried", "\x1b]7501;state=idle:app=xfx:msg=YQ==\x1b\\"),
        ("a raw message", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=rm -rf\x1b\\"),
        ("an empty message", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=\x1b\\"),
        ("a partial quad", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=YQ\x1b\\"),
        ("non-canonical padding", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=YR==\x1b\\"),
        ("a decoded ESC", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=YRti\x1b\\"),
        ("a decoded C1", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=wps=\x1b\\"),
        ("a decoded non-UTF-8 byte", "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=/w==\x1b\\"),
        (
            "past 256 decoded bytes",
            "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=%s\x1b\\" % ("QUFB" * 86),
        ),
        (
            "past 344 characters",
            "\x1b]7501;state=blocked:kind=permission:app=xfx:msg=%s\x1b\\" % ("QUFB" * 87),
        ),
        ("inside an update", "\x1b[?2026h\x1b]7501;state=idle:app=xfx\x1b\\x\x1b[?2026l"),
        ("an unterminated report", "\x1b]7501;state=idle:app=xfx"),
    ):
        grid = fed(sequence)
        require(grid.unknown, "%s passed the oracle unremarked: %r" % (what, sequence))
        require(not grid.statuses, "%s was kept as a report: %r" % (what, grid.statuses))


def an_update_is_open_between_its_brackets_and_only_there():
    """`?2026h` opens a synchronized update and `?2026l` closes it.

    `check.rs` refuses a vector that leaves one open; the wire has no vector
    boundaries, so the form this suite can hold the product to is the session's
    own end -- the terminal is given back outside any update. That needs the
    emulator to know whether one is open, in both directions.
    """
    grid = fed("\x1b[?2026hx")
    require(grid.synchronized, "an opened update was not seen as open")
    grid = fed("\x1b[?2026hx\x1b[?2026l")
    require(not grid.synchronized, "a closed update was still seen as open")


def a_bell_moves_nothing_and_is_counted():
    grid = fed("ab\x07\x07c")
    require(not grid.unknown, "a bell was reported as a defect: %r" % (grid.unknown,))
    require(grid.bells == 2, "the bells were not counted: %d" % grid.bells)
    require(grid.row_text(0) == "abc", "a bell moved a cell: %r" % grid.row_text(0))


def a_caret_placed_off_the_screen_or_half_addressed_is_a_finding():
    for sequence in ("\x1b[0;1H", "\x1b[4;1H", "\x1b[1;21H", "\x1b[2H", "\x1b[1;2;3H"):
        require(fed(sequence).unknown, "%r passed the oracle unremarked" % sequence)
    for sequence in ("\x1b[H", "\x1b[3;20H", "\x1b[1;1H"):
        require(not fed(sequence).unknown, "%r was rejected" % sequence)


def the_anchor_around_a_selector_glyph_is_accepted_and_places_what_follows():
    """`src/tui/grid.rs`'s `put_unit`: `ECH 2`, the glyph, `CHA` past it.

    The column is the painter's to say, whatever the glyph's width here: an
    emulator that moved the caret relative to where the glyph left it would
    agree with a terminal that drew it at another width, which is the defect
    the anchor is for.
    """
    bolt = "\u26a1\ufe0e"
    grid = fed("a \x1b[2X" + bolt + "\x1b[5G b")
    require(not grid.unknown, "the painter's anchor was rejected: %r" % (grid.unknown,))
    require(grid.row_text(0) == "a " + bolt + " b", "the anchor misplaced a cell: %r" % grid.row_text(0))
    gap = fed("a" + bolt + "\x1b[7Gb")
    require(gap.cells[0][6] == "b" and gap.cells[0][3] == " ", "the anchor did not set the column")
    linked = fed("\x1b]8;;https://a\x1b\\a\x1b[2X" + bolt + "\x1b[4Gb\x1b]8;;\x1b\\")
    require(not linked.unknown, "an anchor under the cell's link was rejected: %r" % (linked.unknown,))
    cleared = fed("\ud55cb\ud55c\x1b[1;2H\x1b[3X")
    require(cleared.row_text(0) == "", "a clear left half a cluster: %r" % cleared.row_text(0))
    require(cleared.col == 1, "a clear moved the caret to column %d" % (cleared.col + 1))
    margin = fed("\x1b[?7l" + "x" * 18 + "\x1b[?7h\x1b[2X" + bolt + "\x1b[?7l")
    require(not margin.unknown and not margin.autowrap, "the margin bracket: %r" % (margin.unknown,))


def an_anchor_or_a_clear_off_the_screen_is_a_finding():
    for sequence in ("\x1b[0G", "\x1b[21G", "\x1b[G", "\x1b[1;20H\x1b[2X", "\x1b[0X", "\x1b[X"):
        require(fed(sequence).unknown, "%r passed the oracle unremarked" % sequence)
    for sequence in ("\x1b[1G", "\x1b[20G", "\x1b[1;19H\x1b[2X"):
        require(not fed(sequence).unknown, "%r was rejected" % sequence)


def a_cluster_past_the_last_column_is_a_finding_only_with_autowrap_off():
    """`?7l` is what xfx runs under, and there a terminal clips the write."""
    require(fed("\x1b[?7l" + "x" * 21).unknown, "a clipped write passed the oracle unremarked")
    require(not fed("\x1b[?7l" + "x" * 20).unknown, "a row exactly as wide as the screen was rejected")
    wrapped = fed("x" * 21)
    require(not wrapped.unknown and wrapped.row_text(1) == "x", "autowrap on did not wrap")


def an_undeclared_sgr_parameter_is_a_finding():
    for sequence in (
        "\x1b[999m", "\x1b[5m", "\x1b[8m", "\x1b[38;5;m", "\x1b[48;5;256m",
        "\x1b[48;2;1;2m", "\x1b[58;5;1m", "\x1b[38;2;1;2m", "\x1b[48;2;78;78;78m",
    ):
        grid = fed(sequence)
        require(grid.unknown, "%r passed the oracle unremarked" % sequence)


def the_sgr_the_product_emits_is_accepted():
    for sequence in PALETTE_SGR:
        grid = fed(sequence + "x")
        require(not grid.unknown, "the palette's %r was rejected: %r" % (sequence, grid.unknown))
    require(fed("\x1b[38;5;250mx").attrs[0][0] == "\x1b[38;5;250m", "the colour was remembered")
    require(fed("\x1b[38;5;250m\x1b[0mx").attrs[0][0] == "", "the reset closed the colour")


def the_text_attributes_the_product_emits_are_accepted_and_kept_apart_from_colour():
    """Bold, dim, italic, underline, strike and their closes (`src/tui/pacer.rs`).

    The card's bold, the welcome line's bold grey and the markdown presenter's
    emphasis are on every screen now, so an emulator that refused them would
    fail every scenario for a sequence the product is right to write -- and
    one that folded them into the colour would make `attrs` stop meaning the
    foreground every palette assertion reads it as.
    """
    for sequence in ("\x1b[1m", "\x1b[2m", "\x1b[3m", "\x1b[4m", "\x1b[9m",
                     "\x1b[22m", "\x1b[23m", "\x1b[24m", "\x1b[29m", "\x1b[39m",
                     "\x1b[1;38;5;255m", "\x1b[0;2;38;2;1;2;3m"):
        grid = fed(sequence + "x")
        require(not grid.unknown, "the product's %r was rejected: %r" % (sequence, grid.unknown))
    grid = fed("\x1b[1;38;5;255mx\x1b[22my\x1b[39mz")
    require(grid.attrs[0][0] == "\x1b[38;5;255m", "a grouped colour is the colour alone: %r" % grid.attrs[0][0])
    require(grid.styles[0][0] == "1", "and the bold beside it is a style: %r" % grid.styles[0][0])
    require(
        (grid.attrs[0][1], grid.styles[0][1]) == ("\x1b[38;5;255m", ""),
        "22 closed the bold and kept the colour: %r" % ((grid.attrs[0][1], grid.styles[0][1]),),
    )
    require(grid.is_plain(0, 2), "39 put the foreground back to the default")
    grid = fed("\x1b[1;3mx\x1b[0my")
    require(grid.styles[0][0] == "1;3", "two attributes are both on: %r" % grid.styles[0][0])
    require(grid.is_plain(0, 1), "the reset closed every attribute")
    require(not fed("\x1b[1mx").is_plain(0, 0), "a bold cell is not a plain one")


def reverse_video_and_a_background_are_accepted_and_kept_apart():
    """The caret cell's reverse (`7`/`27`) and the picker's background
    (`48;5;n`, `49`), as `check.rs`'s `wire_color` admits them."""
    for sequence in ("\x1b[7m", "\x1b[27m", "\x1b[49m", "\x1b[48;5;236m",
                     "\x1b[0;7m", "\x1b[1;38;5;255;48;5;236m"):
        require(not fed(sequence + "x").unknown, "the product's %r was rejected" % sequence)
    grid = fed("\x1b[0;7m \x1b[0mx\x1b[48;5;236;38;5;250my\x1b[49mz")
    require(grid.styles[0][0] == "7" and grid.attrs[0][0] == "", "reverse is a style: %r" % grid.styles[0][0])
    require(grid.is_plain(0, 1), "the reset closed the reverse")
    require(
        (grid.backgrounds[0][2], grid.attrs[0][2]) == ("\x1b[48;5;236m", "\x1b[38;5;250m"),
        "a background and a foreground are two slots: %r" % ((grid.backgrounds[0][2], grid.attrs[0][2]),),
    )
    require(
        (grid.backgrounds[0][3], grid.attrs[0][3]) == ("", "\x1b[38;5;250m"),
        "49 closed the background and kept the colour",
    )
    require(not fed("\x1b[48;5;1mx").is_plain(0, 0), "a cell on a background is not a plain one")
    require(fed("\x1b[7mx\x1b[27my").is_plain(0, 1), "27 closed the reverse")


def an_sgr_with_one_foreign_group_is_a_finding_that_moves_no_pen():
    """One group outside the vocabulary and the whole sequence is refused.

    Atomic, as the product's own check is: a `1;7` must not leave the cell bold
    and merely report the reverse, or a scenario reading the pen after a finding
    would be reading a state no terminal is in.
    """
    for sequence in ("\x1b[1;5m", "\x1b[38;5;250;5m", "\x1b[38;5;256m", "\x1b[2;48;5;256m", "\x1b[7;8m"):
        grid = fed(sequence + "x")
        require(grid.unknown, "%r passed the oracle unremarked" % sequence)
        require(grid.is_plain(0, 0), "%r moved the pen though it was refused" % sequence)


def the_title_stack_is_accepted_and_other_window_operations_are_not():
    grid = fed("".join(TITLE_STACK))
    require(not grid.unknown, "the title stack was rejected: %r" % (grid.unknown,))
    grid = fed("\x1b[18t")
    require(grid.unknown, "an undeclared window operation passed the oracle unremarked")


def a_control_byte_on_the_screen_is_still_a_finding():
    # The oracle's oldest claim, kept under test so a rewrite of the text path
    # cannot quietly drop it.
    require(fed("a\x00b").unknown, "a NUL reached the screen unremarked")


def the_alternate_plane_is_a_second_buffer_and_not_the_first_one_scrolled():
    """`?1049h` hands out a **blank** screen and keeps the one it replaced.

    The claim every approval scenario rests on. An emulator that treated the
    pair as a no-op would report a diff painted on the other plane as a diff
    painted over the user's document -- and would report the primary as
    "restored" when nothing had been restored at all.
    """
    grid = fed("PRIMARY-ROW\x1b[?1049hALTERNATE-ROW")
    require(grid.plane == "alternate", "the emulator stayed on the primary plane")
    require("ALTERNATE-ROW" in grid.text(), "the alternate plane holds nothing")
    require(
        "PRIMARY-ROW" not in grid.text(),
        "the alternate plane was handed out holding the primary's cells: %r" % grid.text(),
    )
    require(
        "PRIMARY-ROW" in grid.primary_text(),
        "the normal buffer was lost rather than saved: %r" % grid.primary_text(),
    )


def leaving_the_alternate_plane_restores_the_buffer_and_the_cursor():
    grid = fed("\x1b[3;5HPRIMARY\x1b[?1049h\x1b[10;1HALTERNATE\x1b[?1049l")
    require(grid.plane == "primary", "the emulator stayed on the alternate plane")
    require("PRIMARY" in grid.text(), "the normal buffer was not restored: %r" % grid.text())
    require(
        "ALTERNATE" not in grid.text(),
        "the alternate plane's cells survived the leave: %r" % grid.text(),
    )
    require(
        (grid.row, grid.col) == (2, 11),
        "the cursor saved with the plane was not restored: %r" % ((grid.row, grid.col),),
    )
    require(grid.entered_alternate == 1 and grid.left_alternate == 1, "the pair was not counted")


def a_row_that_leaves_the_alternate_plane_reaches_no_scrollback():
    """A terminal keeps no scrollback for its second buffer.

    Which is the whole reason nothing the document owes may be written while an
    approval screen is up: a row scrolled off there is gone, and Phase 1 never
    repaints a document row.
    """
    grid = Grid(3, 20).feed(b"\x1b[?1049h" + b"\x1b[3;1Hrow\n" * 4)
    require(grid.scrollback == [], "the alternate plane fed the scrollback: %r" % grid.scrollback)
    primary = Grid(3, 20).feed(b"\x1b[3;1Hrow\n" * 4)
    require(primary.scrollback, "the primary plane stopped feeding the scrollback, so this proves nothing")


def entering_the_alternate_plane_twice_is_a_finding():
    """The pair has to balance or the user is left on a buffer nobody owns."""
    require(fed("\x1b[?1049h\x1b[?1049h").unknown, "a doubled enter passed unremarked")
    # And the defensive leave every abnormal restore writes is **not** a
    # finding: it is written on the normal buffer on purpose, and a terminal
    # answers it by doing nothing.
    grid = fed("PRIMARY\x1b[?1049l")
    require(not grid.unknown, "a defensive leave was reported as a defect: %r" % (grid.unknown,))
    require("PRIMARY" in grid.text(), "a defensive leave erased the normal buffer")


def a_frame_is_recorded_only_when_it_is_asked_for():
    """Per-frame snapshots, which is how "no intermediate blank grid" is asked.

    A scenario that could only read the screen a session **ended** on could not
    tell a restore that was one write from one that was two: the end state is
    the same either way, and the blank grid lives in the middle.
    """
    stream = "\x1b[?2026hone\x1b[?2026l\x1b[?2026h\x1b[1;1Htwo\x1b[?2026l"
    recorded = Grid(3, 20, record_frames=True).feed(stream.encode())
    require(len(recorded.frames) == 2, "frames were not recorded: %r" % (recorded.frames,))
    require("one" in recorded.frames[0], "the first frame was not captured")
    require("two" in recorded.frames[1], "the second frame was not captured")
    require(not fed(stream).frames, "frames were recorded by an emulator that was not asked to")


def the_theme_subscription_mode_and_query_the_product_emits_are_accepted():
    """`?2031h`/`?2031l` (`src/tui/term.rs:56,65`) and `?996n` (`src/tui/theme.rs:105`).

    The subscription mode xfx's session sets so a terminal volunteers a
    background change, and the outright query it asks at launch, both move no
    cell -- accepting them is a claim about the allowlist, not about the grid,
    so the screen, the caret and the plane are read off before and after and
    have to agree. The snapshot is taken against a grid seeded with real
    content and a caret away from the origin first: a blank default grid is
    satisfied by an emulator that *erases* on `?2031` exactly as well as by one
    that does nothing, so the seed (`keep me` written mid-screen, caret parked
    away from it) is what makes "moves no cell" a claim that can fail. The
    negative controls sit right next to the accepted three: `?2032` (an
    undeclared neighboring mode), `?997;1n` (the terminal's *inbound* answer to
    the subscription, which xfx's own output stream never carries) and
    `?996;1n` (the query with a parameter nobody sends) all stay findings -- an
    oracle that accepted any of them would be whitelisting a report or a
    request the product never makes.
    """
    grid = Grid(3, 20)
    grid.feed(b"\x1b[2;3Hkeep me\x1b[1;2H")
    before = (grid.text(), (grid.row, grid.col), grid.plane)
    grid.feed(b"\x1b[?2031h\x1b[?996n\x1b[?2031l")
    after = (grid.text(), (grid.row, grid.col), grid.plane)
    require(
        not grid.unknown,
        "the product's theme subscription/query was rejected: %r" % (grid.unknown,),
    )
    require(
        before == after,
        "the accepted theme sequences moved the grid, caret, or plane: %r != %r" % (before, after),
    )
    require(fed("\x1b[?2032h").unknown, "an undeclared neighboring mode ?2032 passed the oracle unremarked")
    require(fed("\x1b[?997;1n").unknown, "an inbound-only ?997;1n report passed the oracle unremarked")
    require(fed("\x1b[?996;1n").unknown, "a parametrized ?996;1n passed the oracle unremarked")


def a_wait_and_a_grid_cut_the_capture_at_the_same_frame():
    """`scenarios.through_closed_frames`: the cut a grid is fed through and a
    `Trial.wait_for` waits on.

    It is the oracle's input rather than the oracle, and it is checked here
    for the same reason the oracle is: `3b` failed at random for as long as
    the wait and the grid cut the capture in two different places. Each
    boundary is checked kept and cut, in bytes and in text alike, because the
    two callers hold one each.
    """
    import scenarios  # noqa: E402 - the runner's module; imported here, not at load

    cut = scenarios.through_closed_frames
    begin, end = scenarios.FRAME_BEGIN_BYTES, scenarios.FRAME_END_BYTES
    whole = b"pre" + begin + b"one" + end
    cases = (
        ("a complete frame is kept whole", whole + b"post", whole + b"post"),
        ("a frame with no closer yet is cut at its opener", b"pre" + begin + b"one", b"pre"),
        (
            "a closer still arriving does not close the frame",
            b"pre" + begin + b"one\x1b[?2026",
            b"pre",
        ),
        (
            "a complete frame before an unfinished one is kept, the unfinished one is not",
            whole + begin + b"two",
            whole,
        ),
        ("a capture with no frame at all is kept", b"probe\x1b[6n", b"probe\x1b[6n"),
        (
            # `END_FRAME_HIDDEN` (`src/tui/frame.rs:65`): a frame whose caret
            # stays hidden ends on `?2026l` alone, and it is closed.
            "a frame closed without showing its caret is kept whole",
            b"pre" + begin + b"one" + scenarios.FRAME_CLOSE_BYTES,
            b"pre" + begin + b"one" + scenarios.FRAME_CLOSE_BYTES,
        ),
    )
    for what, given, expected in cases:
        got = cut(given)
        require(got == expected, "%s: %r gave %r" % (what, given, got))
        # The text form, with the markers `Trial.wait_for` passes. Decoded
        # latin-1 here so the two forms line up byte for byte; `Session.text()`
        # decodes utf-8 with replacement, which the ASCII markers survive alike.
        text = cut(given.decode("latin-1"), scenarios.pty.FRAME_BEGIN, scenarios.pty.FRAME_CLOSE)
        require(
            text == expected.decode("latin-1"),
            "%s: the text cut disagrees with the byte cut: %r" % (what, text),
        )
    require(
        b"two" not in cut(whole + begin + b"two"),
        "a needle inside a frame still being written passed as shown",
    )


TESTS = (
    a_zwj_family_is_one_cluster_two_cells_wide,
    a_combining_mark_stays_with_the_cell_it_marks,
    a_wide_scalar_is_still_two_cells,
    a_flag_is_one_cluster,
    a_clipboard_osc_is_a_finding,
    an_undeclared_osc_number_is_a_finding,
    the_osc_the_product_emits_is_accepted,
    an_osc_in_the_other_terminator_or_with_another_body_is_a_finding,
    a_hyperlink_the_product_writes_is_accepted_and_covers_its_cells,
    a_hyperlink_that_is_not_inert_or_is_left_open_is_a_finding,
    the_program_status_the_product_writes_is_accepted_and_kept,
    a_program_status_in_any_other_shape_is_a_finding,
    an_update_is_open_between_its_brackets_and_only_there,
    a_bell_moves_nothing_and_is_counted,
    a_caret_placed_off_the_screen_or_half_addressed_is_a_finding,
    a_cluster_past_the_last_column_is_a_finding_only_with_autowrap_off,
    the_anchor_around_a_selector_glyph_is_accepted_and_places_what_follows,
    an_anchor_or_a_clear_off_the_screen_is_a_finding,
    an_undeclared_sgr_parameter_is_a_finding,
    the_sgr_the_product_emits_is_accepted,
    the_text_attributes_the_product_emits_are_accepted_and_kept_apart_from_colour,
    reverse_video_and_a_background_are_accepted_and_kept_apart,
    an_sgr_with_one_foreign_group_is_a_finding_that_moves_no_pen,
    the_title_stack_is_accepted_and_other_window_operations_are_not,
    a_control_byte_on_the_screen_is_still_a_finding,
    the_alternate_plane_is_a_second_buffer_and_not_the_first_one_scrolled,
    leaving_the_alternate_plane_restores_the_buffer_and_the_cursor,
    a_row_that_leaves_the_alternate_plane_reaches_no_scrollback,
    entering_the_alternate_plane_twice_is_a_finding,
    a_frame_is_recorded_only_when_it_is_asked_for,
    the_theme_subscription_mode_and_query_the_product_emits_are_accepted,
    a_wait_and_a_grid_cut_the_capture_at_the_same_frame,
)


def main():
    evidence = sys.argv[1] if len(sys.argv) > 1 else None
    for test in TESTS:
        try:
            test()
        except Exception as failure:  # noqa: BLE001 - a test that died is a failure
            problems.append("%s could not be run: %r" % (test.__name__, failure))
    if evidence:
        directory = os.path.join(evidence, "0-oracle-falsification")
        os.makedirs(directory, exist_ok=True)
        with open(os.path.join(directory, "checks"), "w", encoding="utf-8") as handle:
            handle.write("%d\n" % checks)
    for problem in problems:
        print("    %s" % problem)
    print("    %d oracle claims checked" % checks)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
PYTHON

cat >"$helpers/fixture_server.py" <<'PYTHON'
"""A scripted stand-in for the Vercel AI Gateway, on a loopback port.

The same shape as `scripts/smoke.sh`'s `fake_gateway.py`, with two differences
this suite needs:

* It records the **method** as well as the path, headers and body, because the
  three-part positive discriminator (`.prd/06-qa-harness.md` §"Fixtures and the
  mock-vs-live rule") is a claim about the request xfx really sent, and a
  capture that could not tell a POST from a GET would be a weaker claim than it
  reads as.
* It can answer with a stream it never terminates (`hang`), which is the shape
  of a real provider that has started answering and has not finished -- the
  only state a user can actually interrupt, and therefore the only one in which
  the activity row, the pacer and Ctrl-C can be asserted at all. It mirrors
  `tests/support/fake_gateway.rs`'s `SseThenHang` byte for byte: chunked
  transfer encoding, no terminating chunk, connection held open until the
  client hangs up.

No credential and no network: it binds `127.0.0.1:0`.
"""

import json
import select
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import TCPServer

# How long a held-open stream is kept alive when the client never hangs up.
#
# Bounded so a scenario that fails before it interrupts anything fails rather
# than hanging the whole run.
HANG_TIMEOUT = 90.0

# What a session-title request's instructions begin with (`src/tui/titles.rs`,
# upstream's `instructions_text`, `vercel-labs/fx@c1db9191
# src/core/session/session_title_generation.zig:23`).
TITLE_INSTRUCTIONS = "Generate a short title for a conversation"


def text_delta(stream_id, delta):
    return {"type": "text-delta", "id": stream_id, "delta": delta}


def finish(reason="stop"):
    return {
        "type": "finish",
        "finishReason": {"unified": reason, "raw": reason},
        "usage": {"inputTokens": {"total": 3}, "outputTokens": {"total": 5}},
    }


def tool_call(call_id, tool, arguments):
    return {"type": "tool-call", "toolCallId": call_id, "toolName": tool, "input": arguments}


def content_only(*texts):
    """A reply that says `texts` and finishes."""
    return {"events": [text_delta("a", text) for text in texts] + [finish()]}


def hang(*events):
    """A reply that says `events` and never finishes the stream."""
    return {"events": list(events), "hang": True}


def edit_then_finish(marker):
    """Read `notes.txt`, edit it, then say `marker`.

    The read is not decoration: an edit may only replace a file the turn has
    already read in full, in every mode, so a script that jumped straight to the
    edit would be exercising that validation rule instead of the permission
    modes. Lifted from `tests/support/sandbox.rs`'s `edit_then_finish` so both
    surfaces are asked the same question.
    """
    return [
        {"events": [tool_call("call-0", "read_file", {"path": "notes.txt"}), finish("tool-calls")]},
        {
            "events": [
                tool_call(
                    "call-1",
                    "edit_file",
                    {"path": "notes.txt", "old_string": "alpha", "new_string": "beta"},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


# How many lines the scripted large edit replaces.
#
# Short lines rather than one long one, and the shape is load-bearing:
# `read_file` clips a line past `ToolLimits::max_read_line_len` and a clipped
# read is not a complete view, so `edit_file` refuses to replace a file this
# session has only partly read (`src/tools/mutate.rs`). Twenty lines is
# comfortably past the 160 bytes the band's own summary quotes
# (`src/permission/authority.rs`), which is what makes the question one the band
# cannot show.
LARGE_EDIT_LINES = 20


def large_edit_sides():
    """What the file holds before the scripted edit, and after it."""
    before = "\n".join("alpha line %d" % line for line in range(LARGE_EDIT_LINES))
    after = "\n".join("beta line %d" % line for line in range(LARGE_EDIT_LINES))
    return before, after


def large_edit_then_finish(marker):
    """Read `notes.txt`, replace the whole of it, then say `marker`.

    `edit_then_finish` with a change the band's own summary cannot show, which
    is the only difference that matters: the surface a question is asked on is
    chosen by the **change** and never by the terminal
    (`src/tui/approval.rs`'s `ApprovalSurface::for_request`).
    """
    before, after = large_edit_sides()
    return [
        {"events": [tool_call("call-0", "read_file", {"path": "notes.txt"}), finish("tool-calls")]},
        {
            "events": [
                tool_call(
                    "call-1",
                    "edit_file",
                    {"path": "notes.txt", "old_string": before, "new_string": after},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


def large_write_content():
    """What the scripted whole-file write puts down."""
    return "\n".join("gamma line %d" % line for line in range(LARGE_EDIT_LINES)) + "\n"


def spelled_out_breaks(text):
    """`text` with every line break replaced by the two characters that spell one.

    The concrete collision the review surface has to survive: this string and
    `text` are different files -- one has a hundred lines, the other has one --
    and an escaping that wrote a line break as a backslash and an `n` without
    escaping the backslash itself rendered them as the same payload, so the
    screen showed a change of every line in the file as a change of nothing.
    """
    return text.replace("\n", "\\n")


def collision_write_then_finish(marker, content):
    """Read `notes.txt` again, replace it with its own text spelled out, say `marker`."""
    return [
        {"events": [tool_call("call-4", "read_file", {"path": "notes.txt"}), finish("tool-calls")]},
        {
            "events": [
                tool_call(
                    "call-5",
                    "write_file",
                    {"path": "notes.txt", "content": spelled_out_breaks(content)},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


# How many escape bytes the scripted control payload carries.
#
# Past the 160 bytes the band's own summary quotes, so the question is one this
# screen has to show rather than one the sentence already showed whole.
CONTROL_PAYLOAD_BYTES = 161


def control_payload():
    """A payload of nothing but `ESC`.

    Delivered by the **fixture**, which is the model's side of the wire, and
    never typed: the harness types keystrokes into a real terminal, and a raw
    `ESC` typed there is an escape sequence to the input decoder rather than
    content. Coming from a tool call it is exactly what it is in life -- bytes a
    model asked to be written into a file -- and the screen has to disarm it.
    """
    return "\u001b" * CONTROL_PAYLOAD_BYTES


def control_write_then_finish(marker):
    """Read `notes.txt` again, replace it with control bytes, then say `marker`."""
    return [
        {"events": [tool_call("call-6", "read_file", {"path": "notes.txt"}), finish("tool-calls")]},
        {
            "events": [
                tool_call(
                    "call-7",
                    "write_file",
                    {"path": "notes.txt", "content": control_payload()},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


def large_write_then_finish(marker):
    """Read `notes.txt` again, replace the whole file, then say `marker`.

    The **second** shape of a change too big for the band's summary, and the one
    that is not an edit: `write_file` replaces everything, so its "before" is the
    file that is there and its "after" is the text that would arrive
    (`src/tools/mutate.rs`'s `execute_write_file`). The read is not decoration
    here either -- the file changed since the turn above read it, so its digest
    no longer matches the proof that turn recorded and the write would be
    refused with "read it again first".
    """
    return [
        {"events": [tool_call("call-2", "read_file", {"path": "notes.txt"}), finish("tool-calls")]},
        {
            "events": [
                tool_call(
                    "call-3",
                    "write_file",
                    {"path": "notes.txt", "content": large_write_content()},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


def ask_then_finish(marker):
    """Ask a two-question batch, then say `marker`.

    Two questions rather than one because the result is an *ordered* document: a
    one-question batch passes whether or not order is preserved. The first
    question's first option carries a description and the second's carries none,
    which is the one part of a choice that is optional.
    """
    return [
        {
            "events": [
                tool_call(
                    "call-0",
                    "ask_user_question",
                    {
                        "questions": [
                            {
                                "question": "Which depth?",
                                "options": [
                                    {"label": "Thorough", "description": "reads every file"},
                                    {"label": "Quick"},
                                ],
                            },
                            {
                                "question": "Anything else?",
                                "options": [{"label": "No"}, {"label": "Yes"}],
                            },
                        ]
                    },
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


# What the call **behind** an interrupted question would write, and where.
#
# A path that does not exist yet rather than `notes.txt`: a write over a file
# this turn has not read is refused for a reason of its own
# (`src/tools/mutate.rs`), and a scenario whose negative check was satisfied by
# that refusal would report "the interrupt stopped the call" about a call the
# interrupt never touched.
# What the scripted amendable write puts down, and where.
#
# A path that does **not** exist before the turn, which is what makes the denied
# trial's assertion an observation rather than a tautology: an absent file after
# a refusal means the call did not run, and it can only mean that if the file
# was never there.
AMENDMENT_PATH = "plan.md"
AMENDMENT_CONTENT = "the arguments the model asked for, unchanged\n"


def amendable_write_then_finish(marker):
    """Write a new file, then say `marker`.

    A **whole-file write to a fresh path**, so the two trials of scenario 25 are
    two different observations of the same call: allowed, the file holds
    `AMENDMENT_CONTENT` byte for byte -- an amendment is context and never an
    argument, so what runs is what the model asked for; refused, the path does
    not exist at all. No read ahead of it, because a file that is not there has
    no preimage for the write to have to match (`src/tools/mutate.rs`).
    """
    return [
        {
            "events": [
                tool_call(
                    "call-0",
                    "write_file",
                    {"path": AMENDMENT_PATH, "content": AMENDMENT_CONTENT},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


QUESTION_FOLLOWER_PATH = "written-by-the-call-behind-the-question.txt"
QUESTION_FOLLOWER_CONTENT = "the call behind the question ran\n"


def ask_then_write(marker):
    """One completion that asks **and then writes**, and a reply for the turn after it.

    The write is in the *same* completion as the question, and that is the whole
    point of the fixture: `execute_tool_calls` runs a completion's calls in
    order, so the file is what says whether the calls queued behind an
    interrupted question ran. A second completion would prove something weaker
    -- that no further request was made -- and would leave the file untouched
    even on a build that ran the rest of the step.

    The reply after it belongs to the **next** prompt: an interrupted turn asks
    for nothing more, so a session that reached it is a session that took a
    fresh turn after the interrupt.
    """
    return [
        {
            "events": [
                tool_call(
                    "call-0",
                    "ask_user_question",
                    {
                        "questions": [
                            {
                                "question": "Proceed?",
                                "options": [{"label": "Go ahead"}, {"label": "Hold on"}],
                            }
                        ]
                    },
                ),
                tool_call(
                    "call-1",
                    "write_file",
                    {"path": QUESTION_FOLLOWER_PATH, "content": QUESTION_FOLLOWER_CONTENT},
                ),
                finish("tool-calls"),
            ]
        },
        content_only(marker),
    ]


class LoopbackHTTPServer(ThreadingHTTPServer):
    """`HTTPServer` without the reverse-DNS lookup it does while binding.

    The same fix `scripts/smoke.sh` carries and for the same measured reason: on
    a GitHub-hosted macOS runner `socket.getfqdn` inside the constructor blocks
    long enough to eat a whole start-up deadline. The host here is loopback, and
    `server_name` is read by the CGI handler and by nothing else.
    """

    daemon_threads = True

    def server_bind(self):
        TCPServer.server_bind(self)
        self.server_name = "127.0.0.1"
        self.server_port = self.server_address[1]

    def handle_error(self, request, client_address):
        """A client that hung up is not a fixture failure, and not printed as one.

        A session that leaves -- `/quit` mid-stream, or an exit with a
        keep-alive connection still open -- resets its socket, and
        `socketserver` printed a `ConnectionResetError` traceback into the
        scenario's log for it. It reached the log of a passing scenario as
        often as a failing one (11 to 16 passing logs a run), and printed above
        a failure it read as that failure's cause. Every other error is still
        printed.
        """
        if isinstance(sys.exc_info()[1], (ConnectionResetError, BrokenPipeError)):
            return
        super().handle_error(request, client_address)


# Where the Gateway publishes its model catalog (`src/provider/model.rs`'s
# `GATEWAY_MODELS_PATH`, upstream's `gateway.zig:48`).
GATEWAY_MODELS_PATH = "/coding-agent/v1/models"


class Fixture:
    """A loopback Gateway that answers `script` in order and records everything.

    **A catalog request is not a turn**, and is answered apart from the script:
    a session asks the Gateway for its model catalog on its own (`/model`, and
    the context window a turn's meter is measured against), so a script keyed
    by arrival order would hand the catalog the reply meant for the next prompt
    and the prompt an `unscripted request` -- which is how scenario 7's second
    turn once failed on a request the product was right to make. The same
    split `tests/support/fake_gateway.rs`'s `with_catalog` makes: answered
    `404` unless a catalog is given, and recorded in `catalog_requests` rather
    than in what `requests` and `bodies` report.
    """

    def __init__(self, script, record_path=None, catalog=None):
        self.script = list(script)
        self.record_path = record_path
        self.catalog = catalog
        self.captured = []
        self.catalog_captured = []
        self.lock = threading.Lock()
        self.served = 0
        self.title_requests = 0
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_POST(self):  # noqa: N802 - the name is the framework's
                fixture._answer(self)

            def do_GET(self):  # noqa: N802 - the name is the framework's
                fixture._answer(self)

            def log_message(self, *_args):
                pass

        self.server = LoopbackHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    # -- the harness's side -----------------------------------------------

    def url(self):
        return "http://127.0.0.1:%d/v3/ai/language-model" % self.server.server_address[1]

    def requests(self):
        with self.lock:
            return list(self.captured)

    def request_count(self):
        with self.lock:
            return len(self.captured)

    def bodies(self):
        return [request["body"] for request in self.requests()]

    def catalog_requests(self):
        with self.lock:
            return list(self.catalog_captured)

    def stop(self):
        self.server.shutdown()
        self.server.server_close()

    # -- the product's side -----------------------------------------------

    def _answer(self, handler):
        length = int(handler.headers.get("content-length", "0"))
        body = handler.rfile.read(length).decode("utf-8", "replace")
        if handler.command == "POST" and TITLE_INSTRUCTIONS in body:
            # A fresh session's first prompt asks for the session's title
            # beside its turn (`src/tui/titles.rs`). Not a turn, so not a
            # step of any scenario's script: answered with a stream that has
            # no text in it -- the session keeps the title its prompt gives
            # it, which is what the scenarios read -- and kept apart, as
            # `tests/support/fake_gateway.rs` keeps it.
            with self.lock:
                self.title_requests += 1
            return self._stream(handler, {"events": [finish("stop")]})
        record = {
            "method": handler.command,
            "path": handler.path,
            "headers": {key.lower(): value for key, value in handler.headers.items()},
            "body": body,
        }
        if handler.command == "GET" and handler.path.split("?")[0] == GATEWAY_MODELS_PATH:
            with self.lock:
                self.catalog_captured.append(record)
            if self.catalog is None:
                status, document = 404, {"error": "fixture: no catalog"}
            else:
                status, document = 200, self.catalog
            payload = json.dumps(document).encode("utf-8")
            handler.send_response(status)
            handler.send_header("content-type", "application/json")
            handler.send_header("content-length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)
            return
        with self.lock:
            index = self.served
            self.served += 1
            record["index"] = index
            self.captured.append(record)
            if self.record_path:
                with open(self.record_path, "a", encoding="utf-8") as record_file:
                    record_file.write(json.dumps(record) + "\n")
        if index >= len(self.script):
            # A round trip nobody scripted fails the scenario instead of hanging
            # it: an extra request is a defect, and a fixture that answered it
            # would hide the defect behind a passing screen.
            payload = b'{"error":"fixture: unscripted request"}'
            handler.send_response(500)
            handler.send_header("content-type", "application/json")
            handler.send_header("content-length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)
            return
        reply = self.script[index]
        # A scripted reply may carry a `release_when` `threading.Event`: the
        # request is already recorded above, and what waits here is only the
        # *response bytes* -- a scenario can capture "the request landed" and
        # then act (paste, resize, deny) while the reply is provably still
        # pending, instead of racing a sleep against the product. Bounded by
        # the same `HANG_TIMEOUT` the held-stream branch below answers to: an
        # event a scenario forgot to `.set()` must fail this request loudly,
        # the same reasoning the unscripted-request branch above already
        # applies, rather than let the wait expire into an ordinary reply that
        # would hide a broken barrier behind a passing screen.
        release_when = reply.get("release_when")
        if release_when is not None and not release_when.wait(HANG_TIMEOUT):
            payload = b'{"error":"fixture: release_when not set within HANG_TIMEOUT"}'
            handler.send_response(500)
            handler.send_header("content-type", "application/json")
            handler.send_header("content-length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)
            return
        if "status" in reply:
            payload = json.dumps(reply.get("body", {})).encode("utf-8")
            handler.send_response(reply["status"])
            handler.send_header("content-type", "application/json")
            handler.send_header("content-length", str(len(payload)))
            handler.end_headers()
            handler.wfile.write(payload)
            return
        self._stream(handler, reply)

    def _stream(self, handler, reply):
        handler.send_response(200)
        handler.send_header("content-type", "text/event-stream")
        handler.send_header("cache-control", "no-cache")
        handler.send_header("transfer-encoding", "chunked")
        handler.send_header("connection", "close")
        handler.end_headers()
        pieces = ["data: %s\n\n" % json.dumps(event) for event in reply["events"]]
        if not reply.get("hang"):
            pieces.append("data: [DONE]\n\n")
        for piece in pieces:
            raw = piece.encode("utf-8")
            try:
                handler.wfile.write(b"%x\r\n" % len(raw) + raw + b"\r\n")
                handler.wfile.flush()
            except OSError:
                return
        if reply.get("hang"):
            self._hold(handler)
            return
        try:
            handler.wfile.write(b"0\r\n\r\n")
            handler.wfile.flush()
        except OSError:
            pass

    @staticmethod
    def _hold(handler):
        """Keeps a started response open until the client closes its end."""
        import time

        deadline = time.time() + HANG_TIMEOUT
        connection = handler.connection
        while time.time() < deadline:
            ready, _, _ = select.select([connection], [], [], 0.1)
            if not ready:
                continue
            try:
                if not connection.recv(1024):
                    return
            except OSError:
                return


class Llmux:
    """A loopback llmux daemon: two probe endpoints and one data plane.

    Answers by **path** rather than by arrival order, because a provider switch
    probes `GET /` and `GET /models` before any turn runs and a script keyed by
    order would let the probe eat the reply meant for a prompt. Only
    `POST /v1/messages` reads the script, and it answers in the Anthropic wire
    shape -- which is half of what "the provider changed" means: the switch
    changes the protocol, not only the port.
    """

    def __init__(self, script, catalog=None, record_path=None):
        self.script = list(script)
        self.catalog = catalog if catalog is not None else default_catalog()
        self.record_path = record_path
        self.captured = []
        self.lock = threading.Lock()
        self.served = 0
        daemon = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_POST(self):  # noqa: N802 - the name is the framework's
                daemon._answer(self)

            def do_GET(self):  # noqa: N802 - the name is the framework's
                daemon._answer(self)

            def log_message(self, *_args):
                pass

        self.server = LoopbackHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def url(self):
        return "http://127.0.0.1:%d" % self.server.server_address[1]

    def requests(self):
        with self.lock:
            return list(self.captured)

    def message_requests(self):
        return [request for request in self.requests() if request["path"] == "/v1/messages"]

    def stop(self):
        self.server.shutdown()
        self.server.server_close()

    def _answer(self, handler):
        length = int(handler.headers.get("content-length", "0"))
        body = handler.rfile.read(length).decode("utf-8", "replace")
        record = {
            "method": handler.command,
            "path": handler.path,
            "headers": {key.lower(): value for key, value in handler.headers.items()},
            "body": body,
        }
        with self.lock:
            self.captured.append(record)
            if self.record_path:
                with open(self.record_path, "a", encoding="utf-8") as record_file:
                    record_file.write(json.dumps(record) + "\n")

        path = handler.path.split("?")[0]
        if path == "/":
            return self._plain(handler, "llmux")
        if path == "/models":
            return self._json(handler, self.catalog)
        if path != "/v1/messages":
            return self._plain(handler, "fake llmux: no such path", status=404)

        with self.lock:
            index = self.served
            self.served += 1
        if index >= len(self.script):
            return self._json(
                handler, {"error": "fixture: unscripted request"}, status=500
            )
        self._stream(handler, self.script[index])

    @staticmethod
    def _plain(handler, text, status=200):
        payload = text.encode("utf-8")
        handler.send_response(status)
        handler.send_header("content-type", "text/plain")
        handler.send_header("content-length", str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)

    @staticmethod
    def _json(handler, document, status=200):
        payload = json.dumps(document).encode("utf-8")
        handler.send_response(status)
        handler.send_header("content-type", "application/json")
        handler.send_header("content-length", str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)

    @staticmethod
    def _stream(handler, events):
        handler.send_response(200)
        handler.send_header("content-type", "text/event-stream")
        handler.send_header("cache-control", "no-cache")
        handler.send_header("transfer-encoding", "chunked")
        handler.send_header("connection", "close")
        handler.end_headers()
        for name, payload in events:
            raw = ("event: %s\ndata: %s\n\n" % (name, json.dumps(payload))).encode("utf-8")
            try:
                handler.wfile.write(b"%x\r\n" % len(raw) + raw + b"\r\n")
                handler.wfile.flush()
            except OSError:
                return
        try:
            handler.wfile.write(b"0\r\n\r\n")
            handler.wfile.flush()
        except OSError:
            pass


def catalog_model(identifier, aliases, efforts, max_context):
    """One `GET /models` row, with the two columns the browser renders.

    `max_context` is **omitted** rather than nulled when there is none, because
    that is what a real daemon sends for a model with no published window --
    and it is what makes the browser's `unknown` column reachable at all.
    """
    row = {"id": identifier, "name": identifier, "aliases": aliases,
           "group": "anthropic", "efforts": efforts}
    if max_context is not None:
        row["max_context"] = max_context
    return row


def default_catalog():
    return {"models": [catalog_model("claude-fable-5[1m]", ["fable"], [], 200000)]}


def anthropic_answer(*texts):
    """The daemon's own wire shape for one complete answer."""
    events = [
        ("message_start", {"type": "message_start",
                           "message": {"id": "msg_fixture", "model": "fable",
                                       "usage": {"input_tokens": 12345,
                                                 "output_tokens": 0}}}),
        ("content_block_start", {"type": "content_block_start", "index": 0,
                                 "content_block": {"type": "text", "text": ""}}),
    ]
    for text in texts:
        events.append(("content_block_delta", {
            "type": "content_block_delta", "index": 0,
            "delta": {"type": "text_delta", "text": text}}))
    events.extend([
        ("content_block_stop", {"type": "content_block_stop", "index": 0}),
        ("message_delta", {"type": "message_delta",
                           "delta": {"stop_reason": "end_turn"},
                           "usage": {"input_tokens": 12345, "output_tokens": 7}}),
        ("message_stop", {"type": "message_stop"}),
    ])
    return events


def _barrier_selfcheck():
    """`python3 fixture_server.py`, never imported by a scenario.

    Proves `release_when` both halves of its contract against a real loopback
    request, rather than trusting that the read of `_answer` above is right:
    an unreleased gate answers **loudly** within its own bounded wait, and a
    released one answers exactly as if it had never carried one at all.
    """
    import urllib.error
    import urllib.request

    def hit(events, event, timeout):
        global HANG_TIMEOUT
        saved, HANG_TIMEOUT = HANG_TIMEOUT, timeout
        fixture = Fixture([{"events": events, "release_when": event}])
        try:
            try:
                urllib.request.urlopen(fixture.url(), data=b"{}", timeout=5)
                return 200
            except urllib.error.HTTPError as error:
                return error.code
        finally:
            fixture.stop()
            HANG_TIMEOUT = saved

    held = hit([], threading.Event(), 0.2)
    assert held == 500, "an unreleased barrier answered %r instead of failing" % held
    released = threading.Event()
    released.set()
    freed = hit([finish()], released, HANG_TIMEOUT)
    assert freed == 200, "a released barrier answered %r instead of the scripted reply" % freed
    print("fixture_server selfcheck: release_when fails unreleased, answers released -- ok")


if __name__ == "__main__":
    _barrier_selfcheck()
PYTHON

cat >"$helpers/pty_tui.py" <<'PYTHON'
"""Drives the real xfx binary on a real pseudoterminal, and reads it back.

A pipe cannot be used: the TUI refuses to open without a terminal on both ends,
which is itself one of the things the suite checks. Three decisions in here are
load-bearing rather than stylistic, and each is the answer to a way an earlier
harness passed while proving nothing.

**The slave descriptor is retained for the pty's whole life.** A pty's line
discipline is reinitialized to the system defaults when its *last* slave closes,
so a harness that opened a slave, read `termios` and closed it again measures
freshly reset defaults both before the child runs and after it exits -- and
reports "the terminal was given back exactly as it was found" for a child that
left it raw with echo off.

**The child does not take the terminal as its controlling one.** On BSD-derived
kernels a session leader's terminal is revoked when it exits, so a post-exit
`tcgetattr` measures a pristine device and the whole restoration matrix becomes
unfalsifiable on macOS. `tests/support/pty.rs` provides
`spawn_without_taking_the_terminal` for exactly this reason and this file does
the same thing: no `setsid`, no `TIOCSCTTY`. A signal case gets a process group
of its own instead -- inside this process's session, because POSIX discards a
`SIGTSTP` sent to an *orphaned* group and both "inherit the runner's group" and
"call `setsid`" produce one.

**Except under job control, where the terminal is the point.** A shell runs a
foreground job in a process group of its own that *owns* the terminal, in the
shell's session, with the shell -- the session leader -- as its parent and
alive. `job_control=True` builds exactly that: a leader process takes a session
and the terminal (`setsid`, `TIOCSCTTY`), starts the binary in a group of its
own made the terminal's foreground (`tcsetpgrp`), and relays every stop,
continue and exit it waits for. The revocation above is the one constraint it is
shaped by: **the leader outlives the binary** and holds the terminal until the
harness has read `termios` off it, and only then is let go -- so the row's
"byte-identical afterwards" is a reading of the terminal the job had, not of a
revoked one. Like `scripts/fxcap/jobctl.pl` (H1), the leader never takes the
terminal back while the job is stopped: a `SIGCONT` finds the job still the
foreground, as `fg` would have made it.

**The child's environment is built from nothing.** A developer with
`XFX_PERMISSION_MODE=yolo` or a live `VERCEL_OIDC_TOKEN` exported would
otherwise be smoke-testing their shell instead of the binary -- and on the
approval scenarios `yolo` would make the panel never appear, which is a pass by
absence.
"""

import errno
import fcntl
import os
import pty
import select
import signal
import struct
import termios
import time

# How long a wait is given before it fails with what the terminal held.
WAIT = 20.0

# How long a poll sleeps when the master has nothing yet.
IDLE_POLL = 0.002

# The bytes that mean the TUI has taken the terminal (`?2004h`, the last of the
# interactive mode set). Response-only: nothing this harness types contains it,
# so waiting for it cannot be satisfied by an echo of its own keystrokes.
READY = "\x1b[?2004h"

# The launch's cursor query (`CSI 6n`), and the background query behind it.
PROBE = "\x1b[6n"
THEME_PROBE = "\x1b]11;?"

# The first and last bytes of a band frame. Waiting on the *end* is what makes
# "a band is on the screen" a fact rather than a race: a needle matching a row's
# `CUP` would be satisfied by half a frame.
#
# **Two ends, because a frame need not show the caret.** Every frame opens with
# `FRAME_BEGIN` (`src/tui/frame.rs`'s `BEGIN_FRAME`), and the one that leaves a
# caret on the screen closes with `FRAME_END`, the cursor shown only once the
# update is presented (`render_engine/terminal_diff.zig:643-644`). A frame whose
# screen has no caret -- a reset's handoff, the full transcript -- closes with
# the synchronized update's own end alone (`END_FRAME_HIDDEN`). So "this frame
# is complete" is a question about `FRAME_CLOSE`, and "the caret came back" is a
# question about `FRAME_END`; a reader that asked the second when it meant the
# first would read a completed hidden-caret frame as one still being painted,
# and cut the screen back to the frame before it.
FRAME_BEGIN = "\x1b[?2026h\x1b[?25l"
FRAME_CLOSE = "\x1b[?2026l"
FRAME_END = FRAME_CLOSE + "\x1b[?25h"

# The whole interactive mode sequence and the two restores, in order
# (`src/tui/term.rs:56-101`). Spelled out rather than imported: a harness that
# read the constant it is checking would pass for whatever the module declared.
# The title push (`22;2t`) is the last thing in the mode set and its pop
# (`23;2t`) leads every restore's own modes -- the only last/first pair here.
# `?2031h`/`?2031l` (`src/tui/term.rs:56,65`), the theme-change subscription,
# sits the other way round it: set *before* the push and unset *after* the pop,
# so the subscription is taken and given back inside the title the session
# borrows, never outside it.
#
# **The planned exit clears the title first** (`ESC ] 2 ; BEL`, upstream's
# `TerminalTitle.clear`, `render.zig:780-792`; `.prd/tui-phase4/receipts/c5`,
# "Both write `ESC]2;BEL` on the way out") and the abnormal restores do not
# (`term.rs`'s `ABNORMAL_RESTORE`). So the two share a tail and differ in what
# leads it, and a search for the normal restore has to start at the clear: one
# that started at the pop would count the clear as the last bytes of whatever
# came before it -- which is how scenario 3c once measured a torn frame five
# bytes longer than the terminal had taken.
#
# **And every restore opens with `?2026l`**, synchronized output off, ahead of
# all of that (T8 round 2, `40e5ffcd`; `src/tui/term.rs:74-109`; fx's both
# restores, `app_lifecycle.zig:41,44`): a session can end with an update open
# -- a torn frame, a signal between a frame's two ends -- and a terminal handed
# back holding one would keep every later byte, the user's shell included,
# unpresented. So a restore is searched for from that byte, and what a torn
# frame put on the wire ends where it starts.
MODE_SET = "\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l\x1b[?2031h\x1b[22;2t"
CLEAR_TITLE = "\x1b]2;\x07"
# **And every restore includes the program status clear right after `?2004l`**
# (`OSC 7501 ; state=clear`, `src/tui/term.rs`; fx v0.0.16's four restores,
# `app_lifecycle.zig:43-48@481104fd`).
PROGRAM_STATUS_CLEAR = "\x1b]7501;state=clear\x1b\\"
RESTORE_MODES = (
    "\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l" + PROGRAM_STATUS_CLEAR + "\x1b[?2031l\x1b[?7h\x1b[?25h"
)
RESTORE = FRAME_CLOSE + CLEAR_TITLE + RESTORE_MODES
ABNORMAL_RESTORE = FRAME_CLOSE + "\x1b[?1049l" + RESTORE_MODES

# The local and input modes raw mode clears (`shell_runtime.zig:108-138`).
RAW_LOCAL_OFF = ("ECHO", "ICANON", "IEXTEN", "ISIG")
RAW_INPUT_OFF = ("IXON", "ICRNL", "BRKINT", "INPCK", "ISTRIP")


class TerminalState:
    """Every terminal fact a session must not silently change.

    Compared field by field. An earlier harness OR-ed the input and output flags
    into one integer first, which is wrong in the direction that matters: the
    two words have overlapping bit values, so a bit gained in one and lost in
    the other cancels out and a changed terminal compares equal.
    """

    def __init__(self, attributes, size):
        self.iflag, self.oflag, self.cflag, self.lflag = attributes[0:4]
        self.ispeed, self.ospeed = attributes[4:6]
        self.cc = tuple(attributes[6])
        self.size = size

    def key(self):
        return (
            self.iflag,
            self.oflag,
            self.cflag,
            self.lflag,
            self.ispeed,
            self.ospeed,
            self.cc,
            self.size,
        )

    def __eq__(self, other):
        return isinstance(other, TerminalState) and self.key() == other.key()

    def local_set(self, name):
        return bool(self.lflag & getattr(termios, name))

    def input_set(self, name):
        return bool(self.iflag & getattr(termios, name))

    def vmin(self):
        return self.cc[termios.VMIN]

    def vtime(self):
        return self.cc[termios.VTIME]

    def is_raw(self):
        return (
            all(not self.local_set(mode) for mode in RAW_LOCAL_OFF)
            and all(not self.input_set(mode) for mode in RAW_INPUT_OFF)
            and bool(self.cflag & termios.CS8)
            and self.vmin() == 1
            and self.vtime() == 0
        )

    def __repr__(self):
        return (
            "TerminalState(iflag=%#x oflag=%#x cflag=%#x lflag=%#x "
            "vmin=%r vtime=%r size=%r)"
            % (self.iflag, self.oflag, self.cflag, self.lflag, self.vmin(), self.vtime(), self.size)
        )

    def describe(self):
        lines = [repr(self)]
        for mode in RAW_LOCAL_OFF:
            lines.append("  %-8s %s" % (mode, "set" if self.local_set(mode) else "clear"))
        for mode in RAW_INPUT_OFF:
            lines.append("  %-8s %s" % (mode, "set" if self.input_set(mode) else "clear"))
        lines.append("  %-8s %s" % ("CS8", "set" if self.cflag & termios.CS8 else "clear"))
        lines.append("  raw: %s" % self.is_raw())
        return "\n".join(lines) + "\n"


class Terminal:
    """A pty pair, sized before anything is spawned on it."""

    def __init__(self, rows=24, cols=80):
        self.master, self.slave = pty.openpty()
        self.path = os.ttyname(self.slave)
        self.rows, self.cols = rows, cols
        self.set_size(rows, cols)
        flags = fcntl.fcntl(self.master, fcntl.F_GETFL)
        fcntl.fcntl(self.master, fcntl.F_SETFL, flags | os.O_NONBLOCK)

    def set_size(self, rows, cols):
        """Changes the terminal's dimensions, as a window manager would.

        The signal is **not** sent from here. `TIOCSWINSZ` raises `SIGWINCH` on
        the terminal's foreground process group, and these children are spawned
        without one on purpose (no `setsid`, no `TIOCSCTTY`), so nothing would
        be signalled at all -- the caller delivers it by name instead
        (`Trial.resize`).
        """
        self.rows, self.cols = rows, cols
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def modes(self):
        """The terminal's state, read through the retained slave.

        Not through the master: BSD-derived kernels answer `tcgetattr` on a pty
        master with `ENOTTY`.
        """
        packed = fcntl.ioctl(self.slave, termios.TIOCGWINSZ, struct.pack("HHHH", 0, 0, 0, 0))
        rows, cols, _, _ = struct.unpack("HHHH", packed)
        return TerminalState(termios.tcgetattr(self.slave), (rows, cols))

    def close(self):
        for descriptor in (self.master, self.slave):
            try:
                os.close(descriptor)
            except OSError:
                pass


class Session:
    """The real binary running on `terminal`, with everything it wrote captured."""

    def __init__(
        self,
        terminal,
        argv,
        env,
        cwd,
        own_process_group=False,
        nonblocking_output=False,
        ignored=(),
        job_control=False,
    ):
        """`ignored` are signals the binary is started ignoring, the way a
        launcher hands them on: `nohup` ignores `SIGHUP`, a non-interactive
        shell's `&` ignores `SIGINT` and `SIGQUIT`, and `exec` keeps an ignore.
        Set in the child itself, right before the `exec`, so nothing between
        the two can change them back."""
        self.terminal = terminal
        self.captured = bytearray()
        self.status = None  # (kind, value) once reaped
        self.reaped = False
        self.leader = None
        if job_control:
            self._start_as_a_job(argv, env, cwd, ignored)
            return
        self.pid = os.fork()
        if self.pid == 0:  # pragma: no cover - the child execs immediately
            try:
                if own_process_group:
                    os.setpgid(0, 0)
                stdin = os.open(terminal.path, os.O_RDWR | os.O_NOCTTY)
                if nonblocking_output:
                    # A separate *file description* for the screen, so the
                    # non-blocking flag applies to what the band is painted on
                    # and not to the descriptor raw mode is entered on.
                    output = os.open(terminal.path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
                else:
                    output = stdin
                os.dup2(stdin, 0)
                os.dup2(output, 1)
                os.dup2(output, 2)
                for descriptor in (stdin, output):
                    if descriptor > 2:
                        try:
                            os.close(descriptor)
                        except OSError:
                            pass
                os.chdir(cwd)
                for number in ignored:
                    signal.signal(number, signal.SIG_IGN)
                os.execve(argv[0], argv, env)
            except BaseException:  # pragma: no cover
                os._exit(127)
        if own_process_group:
            # Waited for rather than assumed: a stop aimed before `setpgid` has
            # landed goes to the runner's group instead.
            deadline = time.time() + WAIT
            while True:
                try:
                    if os.getpgid(self.pid) == self.pid:
                        break
                except OSError:
                    break
                if time.time() > deadline:
                    raise AssertionError("the child never took a process group of its own")
                time.sleep(IDLE_POLL)

    # -- under job control ------------------------------------------------

    def _start_as_a_job(self, argv, env, cwd, ignored):
        """The binary as a job-control shell's foreground job (see the module).

        Three processes: this one, the **leader** (a session of its own, the
        terminal as its controlling one), and the binary, which the leader
        starts in a process group of its own and makes the terminal's
        foreground. The binary is not this process's child, so what it does is
        read off the leader, one line per event over a pipe: its pid, the group
        the terminal's foreground is once it is up -- read by the leader, the
        one process in the job's session that can ask -- and every stop,
        continue and end `waitpid` reports.

        **The leader sends the job its signals**, as a shell's `fg` and `kill %1`
        do, told to over a second pipe (`signal`). Not ceremony: measured on
        macOS 26, a continue is reported to a parent waiting with `WCONTINUED`
        when the parent sent the `SIGCONT` and not when the job's grandparent
        did -- the job continues and the leader's `waitpid` never says so. The
        same pipe's `release` is what the leader waits for once the job has
        ended (`release`).

        Both sides set the binary's group, so neither the `exec` nor a signal
        can come first, as `jobctl.pl` does; `SIGTTOU` is ignored for the one
        `tcsetpgrp` a process outside the foreground makes, and put back.
        """
        if not hasattr(termios, "TIOCSCTTY"):
            raise AssertionError(
                "this platform's termios has no TIOCSCTTY; job control cannot be built"
            )
        relay_read, relay_write = os.pipe()
        orders_read, orders_write = os.pipe()
        leader = os.fork()
        if leader == 0:  # pragma: no cover - the leader only waits, signals and relays
            code = 0
            try:
                os.close(relay_read)
                os.close(orders_write)
                for descriptor in (self.terminal.master, self.terminal.slave):
                    os.close(descriptor)
                os.setsid()
                tty = os.open(self.terminal.path, os.O_RDWR)
                fcntl.ioctl(tty, termios.TIOCSCTTY, 0)
                job = os.fork()
                if job == 0:
                    try:
                        os.close(relay_write)
                        os.close(orders_read)
                        os.setpgid(0, 0)
                        signal.signal(signal.SIGTTOU, signal.SIG_IGN)
                        os.tcsetpgrp(tty, os.getpid())
                        signal.signal(signal.SIGTTOU, signal.SIG_DFL)
                        for descriptor in (0, 1, 2):
                            os.dup2(tty, descriptor)
                        if tty > 2:
                            os.close(tty)
                        os.chdir(cwd)
                        for number in ignored:
                            signal.signal(number, signal.SIG_IGN)
                        os.execve(argv[0], argv, env)
                    except BaseException:
                        os._exit(127)
                try:
                    os.setpgid(job, job)
                except OSError:
                    pass  # it set its own first, or has already exec'd

                def say(line):
                    os.write(relay_write, (line + "\n").encode())

                say("pid %d" % job)
                deadline = time.time() + WAIT
                while True:
                    try:
                        front = os.tcgetpgrp(tty)
                    except OSError:
                        front = -1
                    if front == job or time.time() > deadline:
                        break
                    time.sleep(IDLE_POLL)
                say("foreground %d" % front)
                ended = False
                orders = b""
                while True:
                    ready, _, _ = select.select([orders_read], [], [], 0.005)
                    if ready:
                        chunk = os.read(orders_read, 4096)
                        if not chunk:
                            break  # the harness is gone: nothing left to hold for
                        orders += chunk
                    released = False
                    while b"\n" in orders:
                        order, orders = orders.split(b"\n", 1)
                        word, _, value = order.decode("ascii").partition(" ")
                        if word == "kill" and not ended:
                            os.kill(job, int(value))
                        elif word == "release":
                            released = True
                    if released and ended:
                        break
                    if ended:
                        continue
                    got, raw = os.waitpid(job, os.WNOHANG | os.WUNTRACED | os.WCONTINUED)
                    if got == 0:
                        continue
                    if os.WIFSTOPPED(raw):
                        say("stopped %d" % os.WSTOPSIG(raw))
                    elif os.WIFCONTINUED(raw):
                        say("continued 0")
                    elif os.WIFSIGNALED(raw):
                        say("signalled %d" % os.WTERMSIG(raw))
                        ended = True
                    elif os.WIFEXITED(raw):
                        say("exited %d" % os.WEXITSTATUS(raw))
                        ended = True
                # Held, terminal and all, until the harness had read it: this
                # exit is what revokes it.
            except BaseException:
                code = 1
            os._exit(code)
        os.close(relay_write)
        os.close(orders_read)
        self.leader = leader
        self._relay = relay_read
        self._orders = orders_write
        self._relayed = b""
        self.job = ("running", None)
        self.pid = None
        self.foreground = None
        deadline = time.time() + WAIT
        while self.pid is None or self.foreground is None:
            if time.time() > deadline:
                raise AssertionError("the job never started under its leader (%r)" % self._relayed)
            self._read_relay(0.05)

    def _order(self, line):
        os.write(self._orders, (line + "\n").encode("ascii"))

    def _read_relay(self, timeout):
        """Takes whatever the leader has said, and keeps the last state."""
        ready, _, _ = select.select([self._relay], [], [], timeout)
        if ready:
            chunk = os.read(self._relay, 4096)
            if not chunk and self.job[0] not in ("exited", "signalled"):
                self.job = ("leader-gone", None)
            self._relayed += chunk
        while b"\n" in self._relayed:
            line, self._relayed = self._relayed.split(b"\n", 1)
            word, value = line.decode("ascii").split()
            if word == "pid":
                self.pid = int(value)
            elif word == "foreground":
                self.foreground = int(value)
            elif word == "continued":
                self.job = ("continued", None)
            else:
                self.job = (word, int(value))

    def release(self):
        """Lets a job's leader go, once the terminal has been read for the last time.

        On BSD-derived kernels this revokes the terminal: nothing reads its
        `termios` after this, which is why it is a step of its own rather than
        part of the binary's exit.
        """
        if self.leader is None or self._orders is None:
            return
        try:
            self._order("release")
        except OSError:
            pass
        os.close(self._orders)
        self._orders = None
        deadline = time.time() + WAIT
        while True:
            pid, _ = os.waitpid(self.leader, os.WNOHANG)
            if pid:
                break
            if time.time() > deadline:
                os.kill(self.leader, 9)
                os.waitpid(self.leader, 0)
                break
            time.sleep(IDLE_POLL)
        os.close(self._relay)

    # -- reading and writing ----------------------------------------------

    def pump(self, timeout=0.05):
        """Takes whatever the terminal has, without blocking on an empty one."""
        ready, _, _ = select.select([self.terminal.master], [], [], timeout)
        if not ready:
            return False
        try:
            chunk = os.read(self.terminal.master, 65536)
        except OSError as failure:
            if failure.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                return False
            return False
        if not chunk:
            return False
        self.captured.extend(chunk)
        return True

    def text(self):
        return self.captured.decode("utf-8", "replace")

    def send(self, data):
        if isinstance(data, str):
            data = data.encode("utf-8")
        while data:
            try:
                written = os.write(self.terminal.master, data)
            except OSError as failure:
                if failure.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                    # A full input queue is backpressure, and the reader on the
                    # other side is a real process that will get to it. Pumping
                    # rather than sleeping, because the reason it is not reading
                    # may be that its own output has nowhere to go.
                    self.pump(IDLE_POLL)
                    continue
                raise
            data = data[written:]

    def wait_until(self, what, ready, timeout=WAIT):
        deadline = time.time() + timeout
        while True:
            text = self.text()
            if ready(text):
                return text
            if time.time() > deadline:
                raise Timeout(what, text)
            self.pump(0.01)

    def wait_for(self, needle, timeout=WAIT):
        return self.wait_until(
            "%r on the terminal" % needle, lambda text: needle in text, timeout=timeout
        )

    def wait_for_count(self, needle, count, timeout=WAIT):
        return self.wait_until(
            "%d x %r on the terminal" % (count, needle),
            lambda text: text.count(needle) >= count,
            timeout=timeout,
        )

    def last_frame(self):
        """The last **complete** frame, or `None`.

        Everything the session ever painted is still in the buffer, so a claim
        about one frame has to be made inside one frame. A frame still open
        reads as no frame at all, which is what keeps a predicate over this from
        asserting against half a paint.
        """
        text = self.text()
        begins = text.rfind(FRAME_BEGIN)
        if begins < 0:
            return None
        ends = text.find(FRAME_CLOSE, begins)
        return None if ends < 0 else text[begins:ends]

    # -- what the child is doing ------------------------------------------

    def state(self):
        """What the child is doing, without consuming the answer.

        `waitid` with `WNOWAIT`, not `waitpid`: `waitpid` consumes the event it
        reports, so a second reading of a child that is *still stopped* comes
        back as running -- precisely the lie a test asserting on a stopped
        process exists to catch -- and it reaps a terminated child behind the
        back of the reaper that owns the status.
        """
        if self.status is not None:
            return self.status
        if self.leader is not None:
            # The leader's last word, which a second reading does not consume
            # either: it is kept until the leader says something new.
            self._read_relay(0)
            return self.job
        info = os.waitid(
            os.P_PID,
            self.pid,
            os.WEXITED | os.WSTOPPED | os.WCONTINUED | os.WNOHANG | os.WNOWAIT,
        )
        if info is None:
            return ("running", None)
        if info.si_code == os.CLD_STOPPED:
            return ("stopped", info.si_status)
        if info.si_code == os.CLD_CONTINUED:
            return ("continued", None)
        if info.si_code == os.CLD_EXITED:
            return ("exited", info.si_status)
        return ("signalled", info.si_status)

    def wait_state(self, what, ready, timeout=WAIT):
        deadline = time.time() + timeout
        while True:
            state = self.state()
            if ready(state):
                return state
            if time.time() > deadline:
                raise Timeout("%s (child is %r)" % (what, state), self.text())
            self.pump(0.01)

    def wait_exit(self, timeout=WAIT):
        """Reaps the child and returns `("exited", code)` or `("signalled", n)`."""
        if self.status is not None:
            return self.status
        deadline = time.time() + timeout
        if self.leader is not None:
            # The leader reaped it; the terminal stays held until `release`.
            while self.state()[0] not in ("exited", "signalled", "leader-gone"):
                if time.time() > deadline:
                    raise Timeout("xfx to exit (job is %r)" % (self.job,), self.text())
                self.pump(0.01)
            self.status = self.job
            self.reaped = True
            return self.status
        while True:
            pid, raw = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                if os.WIFSIGNALED(raw):
                    self.status = ("signalled", os.WTERMSIG(raw))
                elif os.WIFEXITED(raw):
                    self.status = ("exited", os.WEXITSTATUS(raw))
                else:  # pragma: no cover - WUNTRACED is not passed here
                    self.status = ("unknown", raw)
                self.reaped = True
                return self.status
            if time.time() > deadline:
                raise Timeout("xfx to exit", self.text())
            self.pump(0.01)

    def settled_text(self):
        """Everything the terminal will **ever** receive.

        `text()` is a snapshot of a stream still being written, and an absence
        asserted against a snapshot is not an absence: a child that wrote the
        forbidden bytes an instant later passes it. With the writer reaped, what
        is still in the pty is all there will ever be -- and `EAGAIN` is the
        only "end" this pty can offer, because the harness holds a slave open on
        purpose so the master never reports EOF.
        """
        if self.status is None:
            raise AssertionError("settled_text before the child was reaped")
        deadline = time.time() + WAIT
        while True:
            if not self.pump(0.01):
                break
            if time.time() > deadline:  # pragma: no cover
                raise Timeout("the pty to run dry", self.text())
        return self.text()

    def signal(self, number):
        if self.leader is not None:
            # Sent by the job's parent, as a shell sends it (`_start_as_a_job`).
            self._order("kill %d" % number)
            return
        os.kill(self.pid, number)

    def close(self):
        if self.leader is not None:
            if self.status is None and self.pid is not None:
                # By pid, as everything here is killed; the leader relays the
                # end and is then let go.
                try:
                    os.kill(self.pid, 9)
                except OSError:
                    pass
                try:
                    self.wait_exit()
                except Timeout:
                    pass
                self.status = ("killed", 9)
            self.release()
            return
        if self.status is None:
            try:
                os.kill(self.pid, 9)
            except OSError:
                pass
            try:
                os.waitpid(self.pid, 0)
            except OSError:
                pass
            self.status = ("killed", 9)


class Timeout(Exception):
    """A wait that never came true, with what the terminal held when it gave up."""

    def __init__(self, what, text):
        super().__init__("timed out waiting for %s; terminal so far:\n%s" % (what, text[-4000:]))
        self.what = what
PYTHON

cat >"$helpers/scenarios.py" <<'PYTHON'
"""The QA scenarios of `.prd/06-qa-harness.md`, on a release binary.

Run one at a time -- `python3 scenarios.py <name> --binary … --faulty … --evidence …`
-- so that a failure is isolated to its own scenario, keeps its own evidence
directory, and can be re-run by hand from the printed command.

Two rules the whole file is written to, both from `06-qa-harness.md`:

* **Every scenario satisfies the three-part positive discriminator.** A per-run
  nonce is embedded in the prompt, asserted in the client-side capture of the
  request xfx really sent, and the rendered output must be non-empty and carry
  the fixture's own marker. Marker *absence* is only ever an additional negative
  check. This is the failure class where a mockup screen is mistaken for real
  data, and it is only closed by requiring something to be **present**: a
  scenario that passes because nothing appeared is a failed scenario.
* **Nothing is skipped.** The rows that need a deliberate failure run against
  the binary built with `--features fault-injection`; against a binary without
  it they fail loudly rather than being quietly stepped over. Scenario 13 uses
  that build for something that is not a failure at all -- the Phase-1
  whole-band painter, kept as the reference the cell diff is compared against on
  a real terminal.
"""

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import fixture_server as fixtures  # noqa: E402
import pty_tui as pty  # noqa: E402
from vt_grid import Grid, cluster_width, clusters  # noqa: E402

# The dark background reply and the cursor report, in the order they were asked
# for -- which is the order one read serves them in. Answering both keeps every
# launch on the same palette and off the 200 ms deadline a silent terminal pays.
PROBE_ANSWERS = "\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[2;1R"

# What a session says while a turn is waiting to be told whether it may act.
PERMISSION_TITLE = "Permission needed"

# A file change's three answers, in upstream's words and with xfx's own grant in
# the second (`src/tui/approval_view.rs`'s `label`; `.prd/tui-phase4/receipts/f2`,
# `edit-ask`: `❯ 1  Apply once`, `3  Don't apply`, and "What differs" #1 for the
# second). An edit's second choice and a write's name different grants, so each
# has its own needle. Spelled out rather than imported, as every needle here is.
APPLY_ONCE = "1  Apply once"
DONT_APPLY = "3  Don't apply"
ALWAYS_WORDING = "2  Apply + allow future edits to this file for this session"
ALWAYS_WRITE_WORDING = "2  Apply + allow future writes to this file for this session"

# What marks the answer Enter would take, in front of it
# (`fx-approvals-40x120-edit-ask`: `  ❯ 1  Apply once`).
CHOICE_MARK = "❯ "

# The tool group's row for a finished read of the notes every approval fixture
# reads first (`.prd/tui-phase4/receipts/f1`: `Read a.txt`; the call's row is
# `Reading …` while it runs). It replaces the `[tool] read_file ok` line the
# transcript used to write, and is the document row an approval is asked under.
READ_NOTES = "Read notes.txt"

# The line a cancelled turn leaves in the document (`src/tui/shell.rs`'s
# `cancelled_line`; `.prd/tui-phase5/receipts/cancel-marker`, fx v0.0.16's
# `▪ Cancelled · What can fx do differently?`, in this product's name). It
# replaces the `stopping the turn` notice an interrupt used to write.
CANCELLED_TURN = "▪ Cancelled · What can xfx do differently?"

# What the hint row says, whole, while a second Ctrl-C would leave
# (`src/tui/shell.rs`'s `EXIT_ARMED`; `receipts/f4`, "exit hint").
EXIT_ARMED = "press ctrl+c again to exit"

# What the shell says to an affirmative given before the question's frame has
# been committed and reconciled (`shell::APPROVAL_NOT_READY`). A literal here,
# beside the other product wordings this script pins, and the head of the Rust
# constant.
APPROVAL_NOT_READY = "the question is not on the screen yet"

# The bytes that take the terminal's second buffer and give it back. Spelled out
# here rather than imported, for the reason every needle in this suite is.
# Response-only: no scenario types them.
ALTERNATE_ENTER = "\x1b[?1049h"
ALTERNATE_LEAVE = "\x1b[?1049l"

# The hint row of a session with a credential, on the compiled-in defaults.
HINT_AUTO = "auto · glm-5.2"

# The half of it that does not name the permission mode, for a scenario driven
# in `ask` rather than on the defaults.
HINT_MODEL = "glm-5.2"

FRAME_BEGIN_BYTES = pty.FRAME_BEGIN.encode()
FRAME_CLOSE_BYTES = pty.FRAME_CLOSE.encode()
FRAME_END_BYTES = pty.FRAME_END.encode()

# The tail of a capture that a read of the pty cut inside something: an escape
# whose final byte has not arrived yet, or a UTF-8 character whose last bytes
# have not.
UNFINISHED = re.compile(
    rb"(?:\x1b(?:\[[<=>?]?[0-9;]*[ -/]*|\][^\x07\x1b]*\x1b?)?|[\xc0-\xff][\x80-\xbf]*)\Z"
)


def received(captured):
    """`captured` without the sequence a terminal would still be receiving.

    A read of the pty ends wherever the kernel's buffer did, which can be
    inside a sequence: `ESC [ ?` with the `2026h` still on its way. A terminal
    holds those bytes in its parser until the rest arrives, and draws nothing
    of them; an emulator fed them as they stand would report the half sequence
    as one xfx does not declare -- a finding about the harness's read, not
    about the product. So the unfinished tail is left for the next reading,
    exactly as a terminal leaves it, and everything before it is fed.
    """
    found = UNFINISHED.search(captured)
    if found is None:
        return captured
    tail = found.group(0)
    if tail[:1] != b"\x1b":
        lead = tail[0]
        wanted = 1 if lead < 0xE0 else 2 if lead < 0xF0 else 3
        if len(tail) - 1 >= wanted:
            return captured
    return captured[: found.start()]


def through_closed_frames(captured, begin=FRAME_BEGIN_BYTES, close=FRAME_CLOSE_BYTES):
    """`captured` cut before a synchronized frame that has opened and not closed.

    The one definition of "what the terminal shows" that `Trial.peek` reads a
    grid from **and** `Trial.wait_for` waits on, so the two cannot disagree.
    They did: the wait searched every byte read, the grid stopped at the last
    complete frame, and a pty read ends wherever the kernel's buffer did -- so
    a wait could return on a needle inside a frame still being written, and the
    grid read straight after it showed the frame before (`3b-shutdown-drain`,
    `chunk-0`, CI runs 41740485579 and 41799433671). Bytes or text alike: pass
    the markers of the same type.
    """
    begins = captured.rfind(begin)
    if begins >= 0 and captured.find(close, begins) < 0:
        return captured[:begins]
    return captured


class Failure(Exception):
    """A scenario that could not be driven far enough to assert anything."""


# ---------------------------------------------------------------------------
# the run
# ---------------------------------------------------------------------------


class Run:
    """One scenario: its evidence directory, its nonce, and its verdict."""

    def __init__(self, name, binary, faulty, evidence):
        self.name = name
        self.binary = binary
        self.faulty = faulty
        self.dir = os.path.join(evidence, name)
        shutil.rmtree(self.dir, ignore_errors=True)
        os.makedirs(self.dir)
        # Unique per run and per scenario, so a stale capture from an earlier
        # run cannot satisfy this one's discriminator.
        self.nonce = "XFXNONCE-%s-%s" % (name.upper().replace("-", ""), os.urandom(4).hex())
        self.problems = []
        self.checks = 0
        self.trials = []
        self.driving = None

    def require(self, condition, what):
        self.checks += 1
        if not condition:
            self.problems.append(what)
        return bool(condition)

    def trial(self, label, **kwargs):
        # Remembered so that a scenario which dies mid-matrix says **which row**
        # it died on. Scenario 3 drives eleven sessions; "it timed out" without
        # a name would send a reader to the wrong one.
        self.driving = label
        trial = Trial(self, label, **kwargs)
        self.trials.append(trial)
        return trial

    def marker(self, what):
        """A fixture marker that exists nowhere in the product or in a prompt.

        Deliberately not derived from the nonce: scenario 8 asserts that the
        marker appears **exactly once** on the final grid, and a marker
        containing the nonce would also match the echo of the prompt that
        carried it.
        """
        return "XFXMARK-%s-%s" % (self.name.upper().replace("-", ""), what.upper())

    def finish(self):
        for trial in self.trials:
            trial.close()
        record = {
            "scenario": self.name,
            "nonce": self.nonce,
            "checks": self.checks,
            "problems": self.problems,
        }
        with open(os.path.join(self.dir, "verdict.json"), "w", encoding="utf-8") as handle:
            json.dump(record, handle, indent=2)
        with open(os.path.join(self.dir, "checks"), "w", encoding="utf-8") as handle:
            handle.write("%d\n" % self.checks)
        for problem in self.problems:
            print("    %s" % problem)
        return 1 if self.problems else 0


class Trial:
    """One session of one scenario: a sized pty, a child, and its evidence."""

    def __init__(
        self,
        run,
        label,
        rows=24,
        cols=80,
        faulty=False,
        fault=None,
        gateway=None,
        mode=None,
        env_extra=None,
        prior=None,
        own_process_group=False,
        nonblocking_output=False,
        answer_probes=True,
        notes=False,
        notes_text="alpha\n",
        home=None,
        tmux=False,
        args=None,
        workspace=None,
        ignored=(),
        job_control=False,
    ):
        self.run = run
        self.label = label
        self.dir = os.path.join(run.dir, label)
        os.makedirs(self.dir, exist_ok=True)
        # A relaunch is the same profile and the same workspace as the session
        # it continues, so both can be handed in.
        self.home = home or os.path.join(self.dir, "home")
        self.workspace = workspace or os.path.join(self.dir, "workspace")
        for directory in (self.home, self.workspace):
            os.makedirs(directory, exist_ok=True)
        self.notes = os.path.join(self.workspace, "notes.txt")
        if notes:
            with open(self.notes, "w", encoding="utf-8") as handle:
                handle.write(notes_text)

        binary = run.faulty if faulty else run.binary
        # Built from nothing, exactly as `tests/support/sandbox.rs` does. The
        # runner exports a hostile model, a hostile token and
        # `XFX_PERMISSION_MODE=yolo` before any scenario runs; if one of them
        # reached the binary, the approval scenarios would pass by never being
        # asked at all.
        env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": self.home,
            "TERM": "dumb",
            "XFX_TUI": "1",
            "AI_GATEWAY_API_KEY": FAKE_KEY,
        }
        if gateway is not None:
            env["XFX_GATEWAY_URL"] = gateway.url()
        if mode is not None:
            env["XFX_PERMISSION_MODE"] = mode
        if fault is not None:
            env["XFX_TUI_FAULT"] = fault
        if tmux:
            env["TMUX"] = "/tmp/tmux-1000/default,1234,0"
        env.update(env_extra or {})
        self.env = env

        self.terminal = pty.Terminal(rows, cols)
        self.before = self.terminal.modes()
        self._write_modes("before", self.before)
        if prior is None:
            argv = [binary] + list(args or [])
        else:
            # A real shell writing real output before the launch, so "prior
            # output survived" is about a terminal's own document rather than
            # about something the harness painted.
            argv = ["/bin/sh", "-c", "printf '%s\\n'; exec %s" % (prior, binary)]
        self.session = pty.Session(
            self.terminal,
            argv,
            env,
            self.workspace,
            own_process_group=own_process_group,
            nonblocking_output=nonblocking_output,
            ignored=ignored,
            job_control=job_control,
        )
        self.rows, self.cols = rows, cols
        self.snapshots = 0
        if answer_probes:
            self.session.wait_for(pty.PROBE)
            self.session.send(PROBE_ANSWERS)

    # -- driving ----------------------------------------------------------

    def send(self, data):
        self.session.send(data)

    def resize(self, rows, cols):
        """Changes the terminal's size and tells the child about it, by name.

        Two halves and neither is optional. The ioctl is what the child's own
        `TIOCGWINSZ` will answer with; the signal is what makes it ask. A
        scenario that changed only the size would be asserting that xfx redraws
        on a measurement it never took, and one that sent only the signal would
        find the screen exactly as it was.

        The grid every later `peek` is read on is sized from here, which is the
        whole reason this is a method rather than two calls: a snapshot taken at
        the old size after a resize is a screen no terminal ever showed.
        """
        self.terminal.set_size(rows, cols)
        self.rows, self.cols = rows, cols
        self.session.signal(signal.SIGWINCH)

    def wait_for(self, needle, timeout=pty.WAIT):
        """`needle` on the terminal **as far as its last complete frame**.

        What `peek` and `grid` read, cut the same way (`through_closed_frames`):
        a needle seen only inside a frame still being written is not on the
        screen yet, and a grid taken right after a wait that said it was would
        show the frame before.
        """
        return self.session.wait_until(
            "%r on the terminal, in a complete frame" % needle,
            lambda text: needle in through_closed_frames(text, pty.FRAME_BEGIN, pty.FRAME_CLOSE),
            timeout=timeout,
        )

    def wait_until(self, what, ready, timeout=pty.WAIT):
        return self.session.wait_until(what, ready, timeout=timeout)

    def settled(self):
        """A first frame is on the screen: the band exists and is complete."""
        self.session.wait_for(pty.READY)
        self.session.wait_for(pty.FRAME_END)
        self._write_modes("during", self.terminal.modes())
        return self

    def text(self):
        return self.session.text()

    def modes(self):
        return self.terminal.modes()

    def wait_until_raw(self, timeout=pty.WAIT):
        """The child's terminal, once the session has finished taking it.

        Asked of the terminal rather than of the wire, because after a stop
        whose timing the harness does not control a mode set already on the wire
        is satisfied by the one from *before* the stop -- and the assertion then
        runs inside the legitimate cooked interval.
        """
        deadline = time.time() + timeout
        while True:
            state = self.terminal.modes()
            if state.is_raw():
                return state
            if time.time() > deadline:
                raise Failure(self._never_raw(state))
            time.sleep(pty.IDLE_POLL)

    def _never_raw(self, state):
        """What a session that never re-entered raw mode was doing, kept.

        Diagnostics, not a verdict: the child's process state (still stopped,
        or running on a cooked terminal), how many bytes it had written that
        nobody had read -- this wait does not drain the master, so a child
        parked in a write would show here -- and the last bytes the terminal
        was handed, all in `never-raw.txt` beside the trial's other evidence.
        """
        seen = len(self.session.captured)
        while self.session.pump(0):
            pass
        unread = len(self.session.captured) - seen
        try:
            child = self.session.state()
        except OSError as failure:
            child = ("unknown", str(failure))
        tail = bytes(self.session.captured[-400:])
        path = os.path.join(self.dir, "never-raw.txt")
        with open(path, "w", encoding="utf-8") as handle:
            handle.write("termios: %r\n" % (state,))
            handle.write("child: %r\n" % (child,))
            handle.write("unread on the master when the wait gave up: %d byte(s)\n" % unread)
            handle.write("last bytes on the wire: %r\n" % (tail,))
        return (
            "the session never took the terminal back; it is still %r; the child is %r; "
            "%d byte(s) were waiting unread on the master (%s)" % (state, child, unread, path)
        )

    # -- evidence ---------------------------------------------------------

    def peek(self, record_frames=False):
        """The grid as it stands, with no snapshot written.

        Fed only as far as the last **complete** frame. Everything the band
        paints arrives inside `?2026h … ?2026l`, and a snapshot taken between
        the two is a screen no terminal ever shows: the divider half drawn, the
        composer not yet placed. Asserting on one would report a torn paint as
        a product defect. Bytes after the last complete frame are kept -- the
        restore and the exit's own erase are written outside any frame, and a
        scenario asserting on what a session left behind needs them.

        A predicate that a bounded wait polls must also not leave a file behind
        per poll: the evidence directory is for the states a scenario
        *asserted on*.
        """
        captured = through_closed_frames(received(bytes(self.session.captured)))
        return Grid(self.rows, self.cols, record_frames=record_frames).feed(captured)

    def grid(self, label, record_frames=False):
        grid = self.peek(record_frames=record_frames)
        self.snapshots += 1
        path = os.path.join(self.dir, "grid-%02d-%s.txt" % (self.snapshots, label))
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(grid.snapshot())
        return grid

    def _write_modes(self, when, state):
        with open(os.path.join(self.dir, "termios-%s.txt" % when), "w", encoding="utf-8") as h:
            h.write(state.describe())

    def close(self):
        # A reaped child's terminal is read **before** the session is closed:
        # under job control, closing lets the leader go and that revokes the
        # terminal. For every other session the order changes nothing.
        reaped = self.session.status is not None
        if not reaped:
            self.session.close()
        try:
            self._write_modes("after", self.terminal.modes())
        except OSError:
            pass
        if reaped:
            self.session.close()
        with open(os.path.join(self.dir, "raw.log"), "wb") as handle:
            handle.write(bytes(self.session.captured))
        self.terminal.close()


# A credential-shaped literal that is not a credential.
FAKE_KEY = "xfx-smoke-tui-key-must-not-appear-in-output"


# ---------------------------------------------------------------------------
# the discriminator every scenario owes
# ---------------------------------------------------------------------------


def discriminate(run, trial, fixture, marker, label="discriminator", prompt=None):
    """Nonce in the prompt, nonce in the captured request, marker on the screen.

    All three, because each alone is satisfied by a failure: a prompt nobody
    sent still contains the nonce, a request nobody rendered still carries it,
    and a screen can be blank.
    """
    trial.send((prompt or ("say " + run.nonce)) + "\r")
    trial.wait_for(marker)
    sent = fixture.bodies()
    run.require(
        any(run.nonce in body for body in sent),
        "the nonce this run minted is in the request xfx sent (%d request(s) captured)" % len(sent),
    )
    grid = trial.grid(label)
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(
        grid.find(marker) is not None,
        "the fixture's own marker %r is rendered on the screen" % marker,
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    return grid


def from_a_known_composer(run, trial, label):
    """Puts `trial` in the one state two sessions can be compared from.

    The launch answers the terminal's `OSC 11` and `CSI 6n` on a deadline
    (`theme::DEADLINE`, `probe::DEADLINE`). This harness writes the replies as
    soon as it sees the query, but on a loaded machine the child's deadline can
    close first -- and then the reply is not an answer, it is **typing**: the
    session hands it to the input decoder, exactly as `12-theme-detection`
    requires it to, and the composer starts the scenario holding
    `11;rgb:0000/0000/0000`.

    Every scenario before this one tolerates that, because each looks for its
    own needle. Scenario 13 compares two whole screens, so any difference
    between the two launches reads as the diff having painted a different
    screen -- which is a false accusation against the thing under test. So the
    composer is emptied and *proved* empty before either session is driven: a
    `C-u` is a no-op on a session that won the race and a kill on one that did
    not, and both then start from the same screen.
    """
    trial.send(b"\x15")
    trial.wait_until(
        "%s: the composer to start empty" % label,
        lambda _t: composer_text(trial.peek()) == "",
    )
    run.require(
        composer_text(trial.peek()) == "",
        "%s starts from an empty composer" % label,
    )


# ---------------------------------------------------------------------------
# reading the band
# ---------------------------------------------------------------------------
#
# **Every reading of the band goes through `find_band`, and nothing else in this
# file may address a band row by a fixed number.** The band floats: on a short
# document the composer sits right under it (a fresh session puts the composer
# on row 3, its blank rule row on row 4 and the hint on row 5, under the welcome
# line and its blank row), it follows the document down until the screen fills
# and then stays on the bottom rows, a resize pins it to the bottom and `/clear`
# floats it back to the top. A reader that said "the hint is the last row" was
# right for one of those and wrong for the rest -- and wrong in the direction
# that hides a defect, since a blank last row reads as an empty composer.
#
# So the band is found from the one row it always has: the **hint**, which is
# the bottom-most painted row of the primary plane (the frame erases everything
# below a floated band). Counting up from it, in upstream's footer order
# (`src/tui/shell.rs::band_rows`):
#
#   activity row      only while a turn runs, `• Thinking (0s)` and friends
#   blank             the gap under it (dropped on a screen too short for it)
#   composer rows     every one behind the rail `┃ ` (or `┃↑` on the first row
#                     shown when rows above it are scrolled out of the window)
#   rule              only while a menu or question is up -- the rule between
#                     the composer and the panel
#   panel rows        the menu or the question, while it is up
#   divider           a `─` rule while a panel is up, a **blank** row otherwise
#   hint              statusline grey, the permission mode in its own accent --
#                     or the panel's own key help while one is up
#
# The composer is found by its rail and only as the run of rail rows directly
# above that structure -- never by scanning the screen for a rail, because a
# submitted prompt is echoed into the document as a card behind the same rail.
#
# **A question has no composer and no activity row.** A permission question
# and the model's own batch replace the composer rather than sitting under it,
# and the turn's row is off the band while one is up (`src/tui/shell.rs`'s
# `fit`, "A question takes them all"; `render_input.zig:705`;
# `.prd/tui-phase4/receipts/f1`, "The activity row is off the band while an
# approval is up"; `receipts/f3`, "the composer gives the question its rows").
# So a panel between its two rules with no rail above it is still the band,
# with an empty `composer`, and its top is the rule that heads the panel. Only a
# panel licenses that: with no panel up, a structure with no rail above its
# divider is not the band's.

# The rail every composer row and every echoed card starts with.
RAIL = "\u2503"

# The rail's variant on the first composer row shown when rows above it are
# scrolled out of the window: the arrow takes the space's cell.
RAIL_HIDDEN_ABOVE = "\u2191"

# One cell of the rules the band draws across the screen.
RULE = "\u2500"

# The activity row (`src/tui/activity.rs::Activity::row`): the marker -- lit or,
# twice a second, a blank cell -- the phase, the clock in whole units, and the
# token suffix once the turn has spent any. Anchored at both ends so a document
# row that merely mentions thinking cannot pass for it.
ACTIVITY_ROW = re.compile(
    r"^[\u2022 ] (?:Thinking|Generating|Running) \((?:\d+h)?(?:\d+m)?\d+s\)"
    r"(?: \(\u2191\d+(?:\.\d)?k? \u2193\d+(?:\.\d)?k?\))?$"
)


# The dim row a cleanly ended turn writes under its answer (`3efe232`,
# `shell.rs`'s `Tally::line`): two columns in, the elapsed time as
# `formatDurationCompact` says it, and the token counts once either is non-zero.
SUMMARY_ROW = re.compile(
    r"^  (?:\d+s|\d+m \d+s|\d+h \d{2}m)(?: \(\u2191\d+(?:\.\d)?k? \u2193\d+(?:\.\d)?k?\))?$"
)


# The line a session with a conversation leaves under its document on the way
# out, naming the command that resumes it (`src/tui/shell.rs`'s `RESUME_LABEL`;
# `.prd/tui-phase4/receipts/c5`: `Continue session with: xfx --resume <id>`).
RESUME_HANDOFF = re.compile(r"^Continue session with: xfx --resume \S+$")


def grid_row_blank(grid, row):
    """Whether `row` holds nothing at all -- no text, no colour, no attribute."""
    return 0 <= row < grid.rows and all(
        grid.cells[row][col] in (" ", "") and grid.is_plain(row, col) for col in range(grid.cols)
    )


def is_rule(grid, row):
    """Whether `row` is a rule across the whole width and nothing else."""
    return 0 <= row < grid.rows and grid.row_text(row) == RULE * grid.cols


class Band:
    """Where the band's rows are on one grid. Zero-based, like every row here.

    `panel` is empty and `picker_rule` is `None` while no menu or question is
    up, and `activity` is `None` while no turn is running. `document_bottom` is
    the last row above the band's top, `-1` when the band starts on the first
    row.
    """

    def __init__(self, hint, divider, panel, picker_rule, composer, activity):
        self.hint = hint
        self.divider = divider
        self.panel = panel
        self.picker_rule = picker_rule
        self.composer = composer
        self.activity = activity

    @property
    def top(self):
        if self.activity is not None:
            return self.activity
        return self.composer[0] if self.composer else self.picker_rule

    @property
    def document_bottom(self):
        return self.top - 1

    def __repr__(self):
        return (
            "Band(hint=%d divider=%d panel=%r picker_rule=%r composer=%r activity=%r)"
            % (self.hint, self.divider, self.panel, self.picker_rule, self.composer, self.activity)
        )


def find_band(grid):
    """The band on `grid`, found from its hint row up, or `None`.

    `None` is a real answer and never an empty band: a screen with no painted
    row, a session on the alternate plane, a torn frame, or a bottom structure
    that is not the band's (no rail directly above the divider). A caller that
    folded `None` into "idle" or "empty" would be satisfied by exactly those.
    """
    if grid.plane != "primary":
        return None
    hint = None
    for row in range(grid.rows - 1, -1, -1):
        if grid.row_text(row).strip():
            hint = row
            break
    if hint is None or hint < 2:
        return None
    divider = hint - 1
    panel = []
    picker_rule = None
    if is_rule(grid, divider):
        row = divider - 1
        while row >= 0 and not is_rule(grid, row):
            panel.append(row)
            row -= 1
        if row < 0 or not panel:
            return None
        picker_rule = row
        above = row - 1
    elif grid.row_text(divider) == "":
        above = divider - 1
    else:
        return None
    # A composer shorter than the rows its geometry gave it pads the rest blank
    # (`band_rows`' own loop) -- those rows are the band's and are skipped over
    # rather than read as the end of it. **Not under a panel**: there the
    # composer sits on the rule that heads the panel, and a blank row above
    # that rule is the document's own -- the gap under a card the question was
    # asked below, which a reader that skipped it would take for the composer.
    if picker_rule is None:
        while above >= 0 and grid.row_text(above) == "":
            above -= 1
    composer = []
    while above >= 0 and grid.cells[above][0] == RAIL:
        composer.append(above)
        above -= 1
    panel.reverse()
    if not composer:
        if picker_rule is None:
            return None
        return Band(hint, divider, panel, picker_rule, [], None)
    composer.reverse()
    activity = None
    first = composer[0]
    if first >= 2 and grid.row_text(first - 1) == "" and ACTIVITY_ROW.match(grid.row_text(first - 2)):
        activity = first - 2
    elif first >= 1 and ACTIVITY_ROW.match(grid.row_text(first - 1)):
        activity = first - 1
    return Band(hint, divider, panel, picker_rule, composer, activity)


def panel_rows_text(grid):
    """The text of the band's panel rows -- a menu or a question -- top first.

    `[]` while no panel is up, and `None` when there is no band at all.
    """
    band = find_band(grid)
    if band is None:
        return None
    return [grid.row_text(row) for row in band.panel]


def wait_for_panel(trial, needle, what=None):
    """Waits until `needle` is on one of the band's panel rows of the committed
    grid, and returns that grid.

    On the grid and in the panel rather than on the wire: the title reaching the
    terminal says nothing about whether the frame carrying it was completed, and
    a snapshot taken behind a wire match can be the frame from before it.
    """
    trial.wait_until(
        what or "%r on the band's panel" % needle,
        lambda _t: any(needle in text for text in (panel_rows_text(trial.peek()) or [])),
    )
    return trial.peek()


def activity_text(grid):
    """The activity row's text, or `None` while the band has none (or no band)."""
    band = find_band(grid)
    if band is None or band.activity is None:
        return None
    return grid.row_text(band.activity)


def activity_seconds(grid):
    """The activity row's clock in whole seconds, or `None` without one."""
    text = activity_text(grid)
    if text is None:
        return None
    clock = re.search(r"\((?:(\d+)h)?(?:(\d+)m)?(\d+)s\)", text)
    if clock is None:
        return None
    hours, minutes, seconds = (int(part or 0) for part in clock.groups())
    return hours * 3600 + minutes * 60 + seconds


def composer_first_row(grid):
    """The composer's first row shown, or `None` when there is no composer.

    Every composer row carries the rail, so this is `None` only when the band
    itself could not be found or a question has replaced the composer -- never
    because the draft is tall. A draft taller than the window shows `┃↑` on
    this row instead (`composer_scrolled`).
    """
    band = find_band(grid)
    return None if band is None or not band.composer else band.composer[0]


def composer_scrolled(grid):
    """Whether rows above the composer's first row shown are scrolled away."""
    band = find_band(grid)
    return (
        band is not None
        and bool(band.composer)
        and grid.cells[band.composer[0]][1] == RAIL_HIDDEN_ABOVE
    )


def composer_rows_text(grid):
    """Each composer row's text after its rail, top row first, or `None`.

    `None` too while a question has replaced the composer: a draft that is not
    on the screen is not an empty one.
    """
    band = find_band(grid)
    if band is None or not band.composer:
        return None
    return [grid.row_text(row)[2:] for row in band.composer]


def composer_text(grid):
    """The draft as the composer shows it: its rows after the rail, joined.

    Joined with nothing between them, because a soft wrap is the screen's and
    not the draft's; a scenario asserting a hard newline asserts it on the rows.
    """
    rows = composer_rows_text(grid)
    return None if rows is None else "".join(rows)


def hint_text(grid):
    """The band's hint row -- where the mode, the model and the context meter are."""
    band = find_band(grid)
    return None if band is None else grid.row_text(band.hint).strip()


def stopped_on_the_grid(trial, what):
    """Waits until a first Ctrl-C has stopped the running turn, and returns that grid.

    Two facts on one committed grid, both of them upstream's answer to the key
    (`.prd/tui-phase4/receipts/f4`, `cc-0.3`): the turn's cancelled line in the
    document and the exit armed on the hint row. They replace the `stopping the
    turn` notice the interrupt used to write; a session that ignored the key
    shows neither, and one that left on it shows no band at all.
    """
    trial.wait_until(
        what,
        lambda _t: (
            lambda grid: grid.find(CANCELLED_TURN) is not None and hint_text(grid) == EXIT_ARMED
        )(trial.peek()),
    )
    return trial.peek()


def rows_above_band(grid, band=None):
    """The visible rows above the band, as text, top first."""
    band = band or find_band(grid)
    if band is None:
        return None
    return [grid.row_text(row) for row in range(0, band.top)]


def answer_rows(grid, needle):
    """Visible rows above the band that carry `needle`, as `(row, text)`.

    How an answer is read: by its content, in the document, and never at a
    fixed row. The answer is markdown-rendered in a two-column indent, so the
    text is returned whole and a caller asserting the indent asserts it here.
    """
    band = find_band(grid)
    limit = band.top if band is not None else grid.rows
    return [(row, grid.row_text(row)) for row in range(limit) if needle in grid.row_text(row)]


# The welcome line every fresh document starts with (`src/tui/shell.rs`'s
# `welcome`), as its two fixed halves -- the version between them is the
# build's.
WELCOME_HEAD = "x\U0001D487x v"
WELCOME_TAIL = " \u00b7 Run /help for commands"


def is_welcome(text):
    return text.startswith(WELCOME_HEAD) and text.endswith(WELCOME_TAIL)


def is_fresh_document(rows):
    """Whether `rows` are a fresh session's whole document: the welcome line and
    the blank row under it -- the welcome on as many rows as the width wraps it
    to, joined back with the space a word wrap takes away."""
    return (
        rows is not None
        and len(rows) >= 2
        and rows[-1] == ""
        and is_welcome(" ".join(rows[:-1]))
    )


def cards(grid):
    """Every submitted prompt the document echoes, as its text, oldest first.

    A submitted line is upstream's card (`3efe232`): each of its rows behind the
    rail, in bold, and a blank row under the card. Read out of the scrollback
    and the visible rows **above the band** -- never the band's own rows, whose
    composer wears the same rail -- and each card's rows joined with nothing
    between them, because a wrap is the screen's and not the prompt's.
    """
    band = find_band(grid)
    limit = band.top if band is not None else grid.rows
    lines = list(grid.scrollback) + [grid.row_text(row) for row in range(limit)]
    found, current = [], None
    for line in lines:
        if line.startswith(RAIL):
            current = (current or []) + [line[2:]]
        elif current is not None:
            found.append("".join(current))
            current = None
    if current is not None:
        found.append("".join(current))
    return found


# ---------------------------------------------------------------------------
# 1. launch and band ownership
# ---------------------------------------------------------------------------


def scenario_1(run):
    """The band floats under the document, prior output is above it, nothing took 1049.

    **Restated for the floating band** (`fcc8e44`): this row used to assert the
    band's three rows were the screen's last three and every row above them
    blank. The band now sits directly under a short document, so the claim is
    the exact fresh-session shape instead -- the welcome line and its blank row
    as the whole document, the composer on row 3, the blank rule row on row 4,
    the hint on row 5 -- and every row below the hint blank, which a band still
    anchored to the bottom (or one painted twice) fails.
    """
    marker = run.marker("launch")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    prior = "PRIOR-OUTPUT-" + run.nonce
    trial = run.trial("launch", gateway=fixture, prior=prior).settled()

    # The band reader, falsified before any scenario trusts it. Every reading of
    # the band in this file goes through `find_band`, so a reader that took a
    # card in the document for the composer, or answered a band where there is
    # none, would pass rows for the wrong reason everywhere at once.
    def synthetic(*rows):
        return Grid(len(rows) + 2, 30).feed(
            "".join("\x1b[%d;1H%s" % (index + 1, row) for index, row in enumerate(rows)).encode()
        )

    rule = RULE * 30
    reads = {
        "a card over its blank row is the document's, not the composer's": (
            find_band(synthetic(RAIL + " echoed", "", RAIL + " draft", "", "auto")),
            ([2], 3, 4, [], None),
        ),
        "no rail above the divider is no band": (
            find_band(synthetic("  an answer", "", "auto")),
            None,
        ),
        "a panel between two rules under the composer": (
            find_band(synthetic(RAIL + " /", rule, "> /help", rule, "auto")),
            ([0], 3, 4, [2], None),
        ),
        "an activity row with its blank row dropped": (
            find_band(synthetic("\u2022 Running (3s)", RAIL + " x", "", "auto")),
            ([1], 2, 3, [], 0),
        ),
        "a question under a card replaces the composer; the card is the document's": (
            find_band(synthetic(RAIL + " steered", "", rule, "  Permission needed", rule, "keys")),
            ([], 4, 5, [3], None),
        ),
    }
    for what, (band, expected) in reads.items():
        got = None if band is None else (band.composer, band.divider, band.hint, band.panel, band.activity)
        run.require(got == expected, "the band reader: %s (%r, wanted %r)" % (what, got, expected))

    grid = trial.grid("after-first-frame")
    band = find_band(grid)
    run.require(band is not None, "the band is on the screen, found from its hint row up")
    run.require(
        hint_text(grid) == HINT_AUTO,
        "the hint row says what a turn would run with: %r" % hint_text(grid),
    )
    # The push is the claim, and it is exact: the terminal reported the cursor
    # on row 2, so the launch moves to the bottom row and writes **one**
    # linefeed -- which carries exactly the shell's own line off the top of the
    # screen and into the terminal's native scrollback, where the user still
    # has it. Anything else is either a band painted over the shell's output or
    # a scroll by a number nobody measured.
    run.require(
        grid.scrollback == [prior],
        "the shell's output was pushed into scrollback, whole and by exactly one row: %r"
        % (grid.scrollback,),
    )
    run.require(prior not in grid.text(), "and nothing repainted a fragment of it on the screen")
    document = rows_above_band(grid, band) if band is not None else None
    run.require(
        document is not None
        and len(document) == 2
        and is_welcome(document[0])
        and document[1] == "",
        "the document above the band is the welcome line and its blank row, and nothing else: %r"
        % (document,),
    )
    run.require(
        band is not None
        and (band.composer, band.divider, band.hint, band.activity, band.panel)
        == ([2], 3, 4, None, []),
        "the band floats right under it: composer on row 3, the blank rule row 4, the hint row 5 (%r)"
        % (band,),
    )
    painted_below = [
        row + 1 for row in range(5, grid.rows) if grid.row_text(row).strip() != ""
    ]
    run.require(
        not painted_below,
        "nothing is painted below the floating band: %r" % painted_below,
    )
    run.require(composer_text(grid) == "", "the composer starts empty")
    raw = trial.text()
    run.require("\x1b[?1049h" not in raw, "the main surface stayed off the alternate screen")
    for mouse in ("\x1b[?1000h", "\x1b[?1002h", "\x1b[?1006h"):
        run.require(mouse not in raw, "mouse reporting %r was never enabled" % mouse)
    run.require(
        raw.count("\x1b[?2026h") == raw.count("\x1b[?2026l") and raw.count("\x1b[?2026h") > 0,
        "every frame was wrapped in synchronized output (%d open, %d closed)"
        % (raw.count("\x1b[?2026h"), raw.count("\x1b[?2026l")),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    discriminate(run, trial, fixture, marker)

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "Ctrl-D left cleanly")
    settled = trial.session.settled_text()
    run.require(pty.RESTORE in settled, "the normal restore is on the terminal, in order")
    run.require(trial.modes() == trial.before, "the terminal was given back byte for byte")
    # `check.rs` refuses a vector that leaves synchronized output open; on the
    # wire that is a terminal given back with an update still open, holding
    # every later byte -- the user's shell included -- unpresented.
    given_back = Grid(trial.rows, trial.cols).feed(bytes(trial.session.captured))
    run.require(
        not given_back.synchronized,
        "the session gave the terminal back inside a synchronized update",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 2. cursor probe and scrollback push
# ---------------------------------------------------------------------------


def scenario_2(run):
    """`6n` is asked, and what the shell printed is still readable above the band."""
    marker = run.marker("push")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    prior = "PRIOR-OUTPUT-" + run.nonce
    # Not `answer_probes`: the query has to be seen on the wire *before* the
    # reply is typed, or the reply is racing it.
    trial = run.trial("push", gateway=fixture, prior=prior, answer_probes=False)
    trial.wait_for(pty.PROBE)
    run.require(pty.PROBE in trial.text(), "the launch asked the terminal where the cursor is")
    run.require(
        pty.THEME_PROBE in trial.text(), "the launch asked the terminal for its background"
    )
    trial.send(PROBE_ANSWERS)
    trial.settled()

    text = trial.text()
    announced = text.index(pty.READY)
    launch = text[announced : text.index(pty.FRAME_BEGIN, announced)]
    run.require(
        "\x1b[%d;1H" % trial.rows in launch,
        "the push moved the cursor to the bottom margin, so a linefeed scrolls",
    )
    to_bottom = launch.find("\x1b[%d;1H" % trial.rows)
    first_newline = launch.find("\n")
    run.require(
        to_bottom >= 0 and first_newline >= 0 and to_bottom < first_newline,
        "the move to the bottom came before the newline that scrolls",
    )
    run.require(
        launch.count("\n") == 1,
        "the push was exactly the rows the terminal reported (%d newlines)" % launch.count("\n"),
    )
    grid = trial.grid("after-the-push")
    run.require(
        grid.scrollback == [prior],
        "the shell's own line is in the terminal's scrollback, whole: %r" % (grid.scrollback,),
    )
    run.require(
        prior in grid.document_text(),
        "so it is still readable above the band rather than painted over",
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    discriminate(run, trial, fixture, marker)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "Ctrl-D left cleanly")
    fixture.stop()


# ---------------------------------------------------------------------------
# 3. the restoration matrix
# ---------------------------------------------------------------------------


def scenario_3(run):
    """Every row of `03-tui-port.md` §"Acceptance", each asserting termios equality."""
    marker = run.marker("restore")
    fixture = start_fixture(run, [fixtures.content_only(marker)])

    # -- the normal exit, and this scenario's discriminator ---------------
    normal = run.trial("normal-exit", gateway=fixture).settled()
    run.require(normal.modes() != normal.before, "the session really took the terminal")
    run.require(normal.modes().is_raw(), "and took it into raw mode")
    discriminate(run, normal, fixture, marker)
    normal.send("/quit\r")
    run.require(normal.session.wait_exit() == ("exited", 0), "normal exit: /quit leaves with 0")
    settled = normal.session.settled_text()
    run.require(pty.RESTORE in settled, "normal exit: the restore sequence, in order")
    run.require(
        "\x1b[?1049l" not in settled,
        "normal exit: nothing restored an alternate screen it never entered",
    )
    run.require(normal.modes() == normal.before, "normal exit: termios byte-identical")
    fixture.stop()

    # -- a panic on the thread that owns the terminal ---------------------
    ui = run.trial("panic-ui-thread", faulty=True, fault="ui-frame", answer_probes=False)
    kind, code = ui.session.wait_exit()
    run.require((kind, code) != ("exited", 0), "ui panic: the process died nonzero")
    text = ui.session.settled_text()
    restored = text.find("\x1b[?2004l")
    reported = text.find("panicked")
    run.require(restored >= 0, "ui panic: the terminal was restored")
    run.require(reported >= 0, "ui panic: the report reached the user")
    run.require(
        restored >= 0 and reported >= 0 and restored < reported,
        "ui panic: the report landed on a cooked terminal, not in a torn band",
    )
    run.require(ui.modes() == ui.before, "ui panic: termios byte-identical")

    # -- a panic on a thread that owns nothing ----------------------------
    other = run.trial("panic-other-thread", faulty=True, fault="non-owner-panic")
    other.wait_for("panicked")
    run.require(
        other.modes().is_raw(),
        "non-owner panic: the terminal was left to its owner, still raw",
    )
    other.settled()
    other.send(b"\x04")
    run.require(other.session.wait_exit() == ("exited", 0), "non-owner panic: the session lived on")
    text = other.session.settled_text()
    run.require(
        "\x1b[?1049l" not in text,
        "non-owner panic: a thread that owned nothing did not run the abnormal restore",
    )
    run.require(other.modes() == other.before, "non-owner panic: termios byte-identical")

    # -- a panic inside a turn, on the runtime thread ---------------------
    worker_fixture = start_fixture(run, [fixtures.content_only("unused")], name="worker")
    worker = run.trial(
        "panic-worker", faulty=True, fault="worker-turn", gateway=worker_fixture
    ).settled()
    worker.send("anything\r")
    kind, code = worker.session.wait_exit()
    run.require((kind, code) != ("exited", 0), "worker panic: the process died nonzero")
    text = worker.session.settled_text()
    run.require(
        text.count("a turn panicked") == 1,
        "worker panic: reported once, by the thread that owns the terminal (%d copies)"
        % text.count("a turn panicked"),
    )
    restored = text.find("\x1b[?2004l")
    reported = text.find("a turn panicked")
    run.require(
        restored >= 0 and reported >= 0 and restored < reported,
        "worker panic: the panic arrived as data, printed after the restore",
    )
    run.require(worker.modes() == worker.before, "worker panic: termios byte-identical")
    worker_fixture.stop()

    # -- the signals an operator or a supervisor sends --------------------
    for name, number in (
        ("sigterm", signal.SIGTERM),
        ("sighup", signal.SIGHUP),
        ("external-sigint", signal.SIGINT),
    ):
        trial = run.trial(name, own_process_group=True).settled()
        trial.session.signal(number)
        state = trial.session.wait_exit()
        run.require(
            state == ("signalled", int(number)),
            "%s: the child died *by* the signal rather than fabricating a status (%r)"
            % (name, state),
        )
        run.require(trial.modes() == trial.before, "%s: termios byte-identical" % name)
        run.require(
            pty.ABNORMAL_RESTORE in trial.session.settled_text(),
            "%s: the handler wrote the abnormal restore before re-raising" % name,
        )

    # -- stop, resume, and stop again -------------------------------------
    stop = run.trial("sigtstp-and-cont", own_process_group=True).settled()
    stop.session.signal(signal.SIGTSTP)
    state = stop.session.wait_state(
        "the child to really stop", lambda seen: seen[0] == "stopped"
    )
    run.require(state[0] == "stopped", "sigtstp: the process is really stopped (%r)" % (state,))
    run.require(stop.modes() == stop.before, "sigtstp: termios byte-identical while stopped")
    stop.session.signal(signal.SIGCONT)
    # The terminal itself, not a marker on the wire: the timing of a stop is
    # not this harness's to control, so a mode set already on the wire proves
    # nothing about the one after the resume.
    run.require(stop.wait_until_raw().is_raw(), "sigcont: raw mode was positively re-entered")
    stop.wait_for(pty.FRAME_END)

    stop.session.signal(signal.SIGTSTP)
    state = stop.session.wait_state(
        "the child to stop a second time", lambda seen: seen[0] == "stopped"
    )
    run.require(
        state[0] == "stopped",
        "second sigtstp: the handler was reinstalled, so the second stop is a stop too (%r)"
        % (state,),
    )
    run.require(stop.modes() == stop.before, "second sigtstp: termios byte-identical again")
    stop.session.signal(signal.SIGCONT)
    stop.wait_until_raw()
    stop.send(b"\x04")
    # `Exited`, never "not running": a `SIGCONT` leaves a continued
    # notification standing, which any "is it still running" predicate
    # satisfies immediately -- and would then read the terminal before the exit
    # restored anything.
    run.require(
        stop.session.wait_exit() == ("exited", 0), "after two stops the session still leaves at 0"
    )
    run.require(stop.modes() == stop.before, "after two stops: termios byte-identical")

    # -- an initialization that fails on either side of raw mode ----------
    after = run.trial("partial-init-after-raw", faulty=True, fault="after-raw", answer_probes=False)
    run.require(
        after.session.wait_exit() == ("exited", 1), "partial init after raw: it exits 1"
    )
    run.require("xfx: " in after.session.settled_text(), "partial init after raw: it says why")
    run.require(
        after.modes() == after.before, "partial init after raw: no raw terminal was left behind"
    )

    before_raw = run.trial(
        "partial-init-before-raw", faulty=True, fault="before-raw", answer_probes=False
    )
    run.require(
        before_raw.session.wait_exit() == ("exited", 1), "partial init before raw: it exits 1"
    )
    text = before_raw.session.settled_text()
    run.require(
        "xfx: " in text,
        "partial init before raw: the refusal reached the terminal, so the absences below "
        "prove something",
    )
    for sequence in ("\x1b[?2004l", "\x1b[?7h", "\x1b[<u"):
        run.require(
            sequence not in text,
            "partial init before raw: nothing restored %r that was never set" % sequence,
        )
    run.require(before_raw.modes() == before_raw.before, "partial init before raw: untouched")


# ---------------------------------------------------------------------------
# 3b. shutdown drain, and a screen that refuses every frame
# ---------------------------------------------------------------------------


def scenario_3b(run):
    """Two ways a session can be held under, and one terminal it still gives back."""
    marker = run.marker("drain")
    # The marker leads the stream, and the two thousand chunks behind it are the
    # backlog. It has to lead: this scenario quits *mid*-stream on purpose, and
    # a marker at the tail would be one the pacer has not reached -- so the
    # scenario would be asserting on generic `chunk-0` and passing on a screen
    # that never showed anything unique to this fixture/scenario, which is the
    # discriminator failure `06-qa-harness.md` §"Fixtures and the mock-vs-live
    # rule" exists to rule out. (Per-*run* uniqueness is the nonce's job inside
    # `run.marker`; what this needle has to prove is that the screen showed
    # *this scenario's own* text rather than something any fixture would emit.)
    #
    # In paragraphs, each ended by a blank line, because the answer is shown a
    # line at a time once the presenter has seen what follows it (`3efe232`: a
    # prose line waits for the next one, which may make it a setext heading).
    # One unbroken run of chunks into a stream that never closes would never
    # show a single row -- the marker included -- so the backlog is ten chunks
    # to a paragraph and the marker is a paragraph of its own.
    deltas = [fixtures.text_delta("d", marker + "\n\n")]
    deltas.extend(
        fixtures.text_delta("d", "chunk-%d" % n + ("\n\n" if n % 10 == 9 else " "))
        for n in range(2000)
    )
    fixture = start_fixture(run, [fixtures.hang(*deltas)])
    trial = run.trial("drain", faulty=True, fault="slow-ui", gateway=fixture).settled()

    trial.send("stream a lot " + run.nonce + "\r")
    trial.wait_for(marker)
    trial.wait_for("chunk-0")
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    # Asked of the **document** -- the screen and the scrollback above it --
    # and not of the screen alone. How much of the backlog one frame carries
    # is the pacer's elapsed time times its rate (`src/tui/pacer.rs`), and this
    # fault sleeps every tick: the first frame with chunks in it has carried
    # forty of them (the marker and `chunk-0` still on the screen) and eighty
    # (twenty-four rows, so the same synchronized update scrolled both into the
    # terminal's own history -- CI runs 41740485579, 41799433671). Either way
    # they were painted, and a row the stream pushed up is still the user's,
    # one wheel-turn away (`Grid.document_text`).
    grid = trial.grid("mid-stream")
    run.require(
        marker in grid.document_text(),
        "the fixture's own marker %r is rendered (on the screen or in the scrollback the "
        "stream pushed it into)" % marker,
    )
    run.require("chunk-0" in grid.document_text(), "and the stream behind it is running")
    document = grid.document_text()
    run.require(
        0 <= document.find(marker) < document.find("chunk-0"),
        "and the marker leads the stream in the document (marker at %d, chunk-0 at %d)"
        % (document.find(marker), document.find("chunk-0")),
    )
    run.require(grid.text().strip() != "", "the screen is not blank")

    # **Restated for fx's Ctrl-D** (`.prd/tui-phase4/receipts/f4`, `cd-streaming`:
    # "Ctrl-D while an answer streams is not an exit on either",
    # `app_input_runtime.zig:1750`; `src/tui/shell.rs`'s `Action::Eof`). This
    # row used to leave mid-stream with Ctrl-D, and a session that did so now
    # would be ending the answer its user is reading. So the key is asserted
    # to be read and refused -- the keystroke behind it reaches the composer of
    # a session that is still up -- and the drain is driven the way fx leaves
    # a session with a stream still inside it: `/quit` (`commandQuit`,
    # `app_commands.zig:662-665`), which ends the session whatever is running.
    trial.send(b"\x04z")
    trial.wait_until(
        "the keystroke behind a mid-stream Ctrl-D to reach the composer",
        lambda _t: composer_text(trial.peek()) == "z",
    )
    run.require(
        trial.session.state()[0] == "running",
        "Ctrl-D while the answer streams did not end the session",
    )
    trial.send(b"\x15")
    trial.wait_until(
        "the composer to be empty again", lambda _t: composer_text(trial.peek()) == ""
    )
    trial.send("/quit\r")
    started = time.time()
    state = trial.session.wait_exit(timeout=30)
    elapsed = time.time() - started
    run.require(state == ("exited", 0), "the drain did not deadlock (%r)" % (state,))
    # Near what the protocol promises rather than an order of magnitude above
    # it: `worker::DRAIN_DEADLINE` (2 s) plus `JOIN_GRACE` (250 ms) plus room
    # for a loaded machine. A ten-second bound would pass a deadline that had
    # quietly stopped being enforced.
    run.require(elapsed < 5.0, "it left inside the deadline (%.2fs)" % elapsed)
    run.require(trial.modes() == trial.before, "the terminal came back byte for byte")
    listed = subprocess.run(
        [run.binary, "session", "last"],
        cwd=trial.workspace,
        env={"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": trial.home},
        capture_output=True,
        text=True,
    )
    run.require(listed.returncode == 0, "the session log is still readable (%r)" % listed.stderr)
    run.require(
        run.nonce in listed.stdout, "and it records the prompt that was interrupted"
    )
    # Recorded honestly rather than tidily. A torn manifest is worse than a slow
    # exit, and so is a tidy one: a turn the user walked out on is `interrupted`
    # or `unfinished`, never `final`. Only the **upper** bound on the exit is
    # asserted above, and deliberately: which of the two the drain ends by
    # depends on where the runtime happened to be parked -- in `send().await` on
    # a channel the UI closed, or in a socket read that never answers -- and
    # both are inside the protocol. A lower bound would be pinning the
    # scheduler.
    run.require(
        "outcome=final" not in listed.stdout,
        "and it does not pretend the turn concluded: %r"
        % [line for line in listed.stdout.splitlines() if "outcome=" in line],
    )
    run.require(
        any(
            word in listed.stdout
            for word in ("outcome=interrupted", "outcome=unfinished")
        ),
        "it says which way the turn ended instead: %r"
        % [line for line in listed.stdout.splitlines() if "outcome=" in line],
    )
    fixture.stop()

    # -- a screen that refuses every frame --------------------------------
    #
    # The composed row `Task 6` could not write: its `BrokenScreen` unit covers
    # the policy and `faults::a_failure_after_raw_mode…` covers the restoration,
    # separately. Here they are one session on a real terminal, and no fault
    # injection is involved: the screen is genuinely, persistently full.
    # Standard output is a *separate file description* opened `O_NONBLOCK`, so a
    # screen that has no room answers `EAGAIN` -- which is backpressure, which
    # is what `FRAME_BUDGET` is generous for, and which past half a second of
    # wall clock must end the session rather than be retried forever.
    #
    # **How the screen is made to refuse, on every kernel this gates.** The
    # first version of this row filled the pty by writing frames at it and not
    # reading it, which made the row's premise a kernel constant. How much a
    # pty takes before it answers `EAGAIN` is **1,024 bytes** on macOS 26 and
    # **17,408** on Linux 6.8 and 7.1 -- measured on each, with a pty and a
    # non-blocking write -- so four hundred small frames overran one target's
    # screen and stopped forty-eight bytes short of the other's. That is
    # exactly what CI reported: green on `x86_64-apple-darwin` and a twenty
    # second timeout on the three targets whose screens are seventeen times
    # larger.
    #
    # So the premise is a **size** now, and the size is the product's own: a
    # frame carries the whole band (`src/tui/frame.rs`, "the whole band is
    # repainted, every frame"), and the draft below makes one frame 39,187
    # bytes -- 2.25x the largest of those screens and 38x the smallest.
    #
    # One frame is one counted emit (`src/tui/deliver.rs`, onto the terminal
    # descriptor passed down from `event_loop.rs`), and an emit retries
    # `Interrupted` and nothing else. `WouldBlock` ends it, on whichever of the
    # two roads its position puts it: a screen that took **nothing** is a
    # refusal the band owes again and the budget counts, and a screen that took
    # a **prefix** ends the session on the spot, because the vector cannot be
    # re-offered without writing that prefix twice. Both are bounded and both
    # are asserted below by the same pair of claims -- exit 1 inside the
    # deadline. Which of the two a given kernel produces is its buffer's
    # business: a screen larger than one frame refuses whole vectors, and one
    # smaller takes a prefix of the first (macOS 26 answers this row with
    # `accepted 1024 bytes`, its whole pty buffer). So for a
    # frame this size to *land*, the reader would have to keep freeing room
    # inside the gaps between consecutive `write(2)` calls of a loop that has
    # no sleep in it -- twenty-two more kilobytes of it on the larger screen,
    # thirty-eight on the smaller -- against a harness that reads once per fill
    # chunk and once per ten milliseconds thereafter. Measured across six full
    # runs, the largest complete frame either screen ever took is 9,040 bytes
    # on Linux and 718 on macOS, both a fraction of that kernel's capacity.
    #
    # And the residual is safe in the one direction that matters: a frame that
    # *did* land calls `FrameFailures::succeeded`, the budget starts over, the
    # session does not give up, and this row **fails** on its wait. There is no
    # arrangement of the scheduler that turns this into a pass.
    #
    # The terminal is therefore read at full speed throughout, which is what a
    # real terminal does.
    #
    # The fill stops the moment the session leaves, which on the small screen
    # is long before the cap: 1,024 bytes is under two composer rows there.
    starved = run.trial(
        "starved-screen", rows=STARVING_ROWS, cols=STARVING_COLS, nonblocking_output=True
    ).settled()
    began = time.time()
    pending = b""
    while time.time() - began < STARVING_CEILING:
        if starved.session.state()[0] not in ("running", "continued"):
            break
        if not pending:
            pending = STARVING_CHUNK
        try:
            pending = pending[os.write(starved.terminal.master, pending) :]
        except OSError:
            # The input queue is full: the session has stopped reading, which
            # is the state this row is waiting for anyway.
            pass
        # A hundredth of the ceiling, so the loop offers the draft steadily for
        # the whole window instead of spending it in a spin.
        time.sleep(0.001)
        # Whatever a short write left over is carried and offered again with
        # the next chunk rather than dropped, because a write cut **inside** a
        # three-byte letter would put a replacement character into the draft in
        # place of the letter -- the same width on the screen, a different
        # number of bytes on the wire, and this row's premise is a byte count.
        #
        # **Nothing reads the master here, and that is the starvation.** An
        # earlier version pumped once per chunk and relied on a whole-band
        # repaint outrunning the reader; a frame is a difference now, so a
        # session between two keystrokes writes a handful of bytes and a reader
        # that drains between them keeps the screen from ever being full. What
        # this row is about is a screen with no room, so the harness gives it
        # none: the master is left unread until the session has decided.
        #
        # **And the draft is offered for the whole window rather than a fixed
        # number of chunks.** The refusal has to last `FRAME_BUDGET`, and a
        # frame is only *attempted* when something asks for one: a fill that
        # stopped early would leave the session with a full screen, nothing to
        # paint, and therefore no second failure for the budget to expire on.
    state = starved.session.wait_state(
        "the session to give up on a screen that takes nothing",
        lambda seen: seen[0] not in ("running", "continued"),
        timeout=20,
    )
    # One, and not merely non-zero: `ExitCode::FAILURE` is what the give-up
    # path returns (`src/main.rs:44-49`, through `tui::run_blocking`'s `fail`),
    # and a panic on the way out would leave with 101 and satisfy "non-zero"
    # while proving the opposite of what this row is about.
    run.require(
        state == ("exited", 1),
        "a screen that refused every frame ended the session with its error (%r)" % (state,),
    )
    elapsed = time.time() - began
    run.require(
        elapsed < STARVED_DEADLINE,
        "it ended on the budget, or on the first prefix, rather than retrying forever "
        "(%.2fs, deadline %.2fs)" % (elapsed, STARVED_DEADLINE),
    )
    starved.session.wait_exit()
    run.require(
        starved.modes() == starved.before,
        "and the terminal it could not write to was still given back byte for byte",
    )
    # **The line discipline is the whole contract here, and the bytes are not.**
    # `docs/parity.md` promises the `termios` back "whether or not the screen
    # could still be written"; it promises nothing about a restore *sequence*
    # reaching a screen that is refusing, and it cannot: the exit writes those
    # bytes at the one instant the screen has no room for them -- the give-up is
    # decided *by* a frame that just filled the screen and failed -- and this
    # row's premise is that the room never comes back. An earlier version of
    # this row asserted them anyway and passed on macOS four runs in five and
    # on Linux never: it was reading a screen that was not really full, which
    # is the same lie the deadline was tightened to catch, one layer down. The
    # wire bytes are asserted where they can be, on screens that take them:
    # scenario 3's `normal-exit` row for a clean exit and its
    # `partial-init-after-raw` row for a failing one -- the latter is where
    # "it says why" lives.


# ---------------------------------------------------------------------------
# 3c. a terminal that takes part of a frame
# ---------------------------------------------------------------------------


def first_frame(captured):
    """The launch's first **complete** frame, as bytes, or `None`.

    Complete on purpose: it is the thing the torn run below is measured
    against, so an incomplete one would make the comparison vacuous.
    """
    begins = captured.find(FRAME_BEGIN_BYTES)
    if begins < 0:
        return None
    ends = captured.find(FRAME_END_BYTES, begins)
    if ends < 0:
        return None
    return captured[begins : ends + len(FRAME_END_BYTES)]


# The window title a write leads with: `OSC 2`, its text, `BEL`.
# The title, and the launch's program status report when it rides the lead
# after it (`event_loop.rs`'s `commit_tick`: fx's launch is `OSC 2`, then
# `idle`, then the first `?2026h`).
TITLE_LEAD = re.compile(rb"\x1b\]2;[^\x07\x1b]*\x07(?:\x1b\]7501;[\x20-\x7e]*\x1b\\)?\Z")


def write_start(captured, begins):
    """Where the write that opened the frame at `begins` started, or `None`.

    **The first update carries the title ahead of it, in the same write** (T8
    round 2, `40e5ffcd`: "the title ahead of it", as fx's; the composer's own
    test, `event_loop.rs`'s `…torn_inside_its_band`: "the first frame owes the
    title"). So what a terminal that tore that write had taken -- the count the
    session reports -- starts at the title, not at the frame. Read off the wire
    rather than assumed: the bytes right before the frame are exactly one title
    -- and the launch's program status report, which rides the lead after it
    -- and nothing else, or there is no lead to count.
    """
    lead = TITLE_LEAD.search(captured[:begins])
    return None if lead is None else lead.start()


def band_part_start(control, launch_frame):
    """Where the band's part of the control's launch frame begins, or `None`.

    A launch frame is the document's rows and then the band, in one update (T8;
    `event_loop.rs`'s
    `a_streamed_answers_rows_land_in_the_update_that_paints_the_band_under_them`).
    Found from the screen, not from the composer's bookkeeping: the band's top
    row as `find_band` reads it on the grid that frame painted, and the first
    byte that addresses that row -- the rows part writes only rows above the
    band, so nothing before the band's part can.
    """
    captured = bytes(control.session.captured)
    begins = captured.find(launch_frame)
    if begins < 0:
        return None
    grid = Grid(control.rows, control.cols).feed(received(captured[: begins + len(launch_frame)]))
    band = find_band(grid)
    if band is None:
        return None
    at = launch_frame.find(b"\x1b[%d;1H" % (band.top + 1))
    return at if at > 0 else None


def require_a_band_tear(run, what, control, launch_frame, prefix):
    """The torn write is cut inside the band's part, at the fault's own point.

    `partial-frame` and `partial-frame-once` mean a tear in the first update's
    **band**, and that update carries the document's rows ahead of it -- so half
    of the write, where they used to cut, is inside the rows whenever the rows
    outweigh the band, which is a different fault. T8 round 2 (`21f194c9`) cuts
    halfway into the band's part instead (`fault::band_tear`: the rows' end plus
    half of what follows, rounded up). Measured from the frame's opener the
    title lead cancels out of that arithmetic, so the point is read off the
    control's own launch frame: past every row, short of the close, and exactly
    there.
    """
    band = band_part_start(control, launch_frame) if launch_frame else None
    run.require(
        band is not None,
        "%s: the control's launch frame has a band's part to tear inside" % what,
    )
    if band is None:
        return
    closes = len(launch_frame) - len(FRAME_END_BYTES)
    run.require(
        band < len(prefix) < closes,
        "%s: the tear is inside the band's part, past every document row (%d, band %d..%d)"
        % (what, len(prefix), band, closes),
    )
    run.require(
        len(prefix) == band + (len(launch_frame) - band + 1) // 2,
        "%s: the tear is halfway into the band's part, the fault's own point (%d, expected %d)"
        % (what, len(prefix), band + (len(launch_frame) - band + 1) // 2),
    )


def scenario_3c(run):
    """A prefix on a real terminal: written once, never re-offered, terminal back.

    The one failure a refusing screen cannot stand in for. Every other row of
    the restoration matrix fails a write that delivered **nothing**, so the
    terminal is exactly where it was and the vector may be offered again; here
    the terminal really has taken part of a synchronized frame. This build's
    one answer to that -- a fixed cleanup vector, then a rebuild
    (`Band::recover_primary`) -- is attempted, and `partial-frame` refuses
    that cleanup vector too, which is what makes the tear fatal here rather
    than recovered. `3d-primary-band-recovery` drives the row where the same
    cleanup vector is answered for real.

    **What this row cannot use, and what it uses instead.** The three-part
    discriminator wants a nonce in a prompt, in the captured request and on the
    screen -- and this session dies on the *first band frame*, before a prompt
    can be typed and before any turn exists to send one. Faking a nonce that was
    never sent would be a mockup of the very kind that discriminator exists to
    catch. So the nonce row is driven in full against a **control** session --
    same fixture, same geometry, same profile, nothing injected -- which proves
    this scenario's fixture and binary really render this scenario's own text;
    and the torn session is discriminated positively against *that* control:
    what the terminal took is asserted to be a genuine, incomplete **prefix of
    the very frame the control's launch paints**. Nothing here passes by
    absence.

    The control runs the release binary and the torn session the
    fault-injection one, which is the pairing every faulty row uses. The two
    builds differ in branches that no launch reaches (`super::fault`), so a
    disagreement between their launch frames is a real finding rather than a
    tolerated one -- and it is the comparison below that would report it.

    **What is deliberately not claimed**: that the terminal's parser recovered.
    The `termios` comparison is a `tcsetattr` fact read off the child's
    terminal, and the restore sequence is a byte fact read off the wire. Neither
    says what a terminal made of an incomplete vector, and this row asserts
    neither.
    """
    marker = run.marker("partial")
    fixture = start_fixture(run, [fixtures.content_only(marker)])

    # -- the control: this scenario's own text, rendered for real ---------
    control = run.trial("normal-control", gateway=fixture).settled()
    run.require(control.modes().is_raw(), "control: the session took the terminal into raw mode")
    launch_frame = first_frame(bytes(control.session.captured))
    run.require(
        launch_frame is not None and len(launch_frame) > len(FRAME_BEGIN_BYTES),
        "control: the launch painted a complete first frame to compare against",
    )
    discriminate(run, control, fixture, marker)
    control.send("/quit\r")
    run.require(control.session.wait_exit() == ("exited", 0), "control: /quit leaves with 0")
    run.require(control.modes() == control.before, "control: termios byte-identical")

    # -- the same launch, onto a terminal that stops mid-frame ------------
    torn = run.trial("partial-frame", faulty=True, fault="partial-frame", gateway=fixture)
    state = torn.session.wait_exit()
    # One rather than merely non-zero: `ExitCode::FAILURE` is what the give-up
    # path returns, and a panic would leave with 101 while proving the opposite.
    run.require(
        state == ("exited", 1),
        "partial frame: the session left with its own error rather than surviving (%r)" % (state,),
    )
    captured = bytes(torn.session.captured)
    text = torn.session.settled_text()

    begins = captured.find(FRAME_BEGIN_BYTES)
    run.require(begins >= 0, "partial frame: a band frame really reached the terminal")
    run.require(
        captured.count(FRAME_BEGIN_BYTES) == 1,
        "partial frame: the torn frame was never offered again (%d frames on the wire)"
        % captured.count(FRAME_BEGIN_BYTES),
    )
    run.require(
        FRAME_END_BYTES not in captured,
        "partial frame: no frame was ever completed on this terminal",
    )
    restore = pty.RESTORE.encode()
    restore_at = captured.find(restore, begins if begins >= 0 else 0)
    run.require(restore_at > begins >= 0, "partial frame: the exit's restore followed the prefix")
    prefix = captured[begins:restore_at] if restore_at > begins >= 0 else b""
    run.require(
        0 < len(prefix) < len(launch_frame or b""),
        "partial frame: what the terminal took is a real and incomplete part of a frame "
        "(%d bytes of %d)" % (len(prefix), len(launch_frame or b"")),
    )
    run.require(
        bool(launch_frame) and launch_frame.startswith(prefix),
        "partial frame: those bytes are a prefix of the frame this launch paints",
    )
    require_a_band_tear(run, "partial frame", control, launch_frame, prefix)

    # The count the session reports is the count on the wire. Two independent
    # measurements of one number: the product's own accounting, and the harness
    # counting what its terminal was handed -- **from the start of the write**,
    # which is the title the first update leads with (`write_start`), not the
    # frame's opener.
    lead = write_start(captured, begins) if begins >= 0 else None
    run.require(
        lead is not None,
        "partial frame: the torn write led with the title the first update owes: %r"
        % captured[max(0, begins - 40) : max(0, begins)],
    )
    taken = restore_at - lead if lead is not None and restore_at > begins >= 0 else -1
    reported = re.search(r"accepted (\d+) bytes", text)
    run.require(
        reported is not None,
        "partial frame: the session says how much the terminal accepted: %r" % text[-200:],
    )
    run.require(
        reported is not None and int(reported.group(1)) == taken,
        "partial frame: the reported count is the number of bytes on the wire (%s vs %d)"
        % (reported.group(1) if reported else "none", taken),
    )

    # The exit's own segments. The restore went out whole -- the terminal was
    # taking bytes again by then -- so the cleanup below it is still written:
    # the skip belongs to an exit whose *first* segment is taken in part, and a
    # frame that ended in a prefix earlier in the session is not that.
    after = captured[restore_at + len(restore) :] if restore_at >= 0 else b""
    run.require(
        b"\x1b[J" in after,
        "partial frame: the exit still wrote the cleanup it owed after a whole restore",
    )
    run.require(
        torn.modes() == torn.before,
        "partial frame: termios byte-identical -- the line discipline, which is all this measures",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 3d. the primary-band recovery this build actually answers
# ---------------------------------------------------------------------------

# The fixed vector `Band::recover_primary` emits and `check::RECOVERY_CLEANUP`
# licenses (`src/tui/check.rs:1084`), spelled out rather than imported for the
# same reason every other needle in this suite is.
RECOVERY_CLEANUP_BYTES = b"\x18\x1b]8;;\x07\x1b[0m\x1b[?2026l\x1b[?7l\x1b[?25h"


def recovered_peek(trial, after, record_frames=False, until=None):
    """`Trial.peek`, sliced to start at `after` (a recovery cleanup's end).

    The oracle's allowlist knows neither a bare `CAN` nor `OSC 8` -- the bytes
    `RECOVERY_CLEANUP` is built from -- and cannot model a parser mid tear
    either way, so nothing in this scenario ever feeds it the raw capture
    from byte zero. `Trial.peek`'s own last-complete-frame guard is kept on
    the slice: everything from `after` on is either the rebuild this row
    asserts on or an ordinary later frame, and both are real, complete,
    synchronized ones.

    `until` stops the slice at a byte offset in the capture, as `excised`
    does for `3e`: the rebuild's own screen is the one **before** the frame
    the next tick is forced to write, which erases and rewrites every band
    row (`Band::force_redraw`) and would paint over a rebuild that got a row
    wrong.
    """
    captured = bytes(trial.session.captured)
    if until is not None:
        captured = captured[:until]
    captured = through_closed_frames(received(captured[after:]))
    return Grid(trial.rows, trial.cols, record_frames=record_frames).feed(captured)


def recovered_grid(trial, after, label, until=None):
    grid = recovered_peek(trial, after, until=until)
    trial.snapshots += 1
    path = os.path.join(trial.dir, "grid-%02d-post-cleanup-%s.txt" % (trial.snapshots, label))
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("# post-cleanup reconstruction; not proof of parser resync\n")
        handle.write(grid.snapshot())
    return grid


def recovered_discriminate(run, trial, fixture, marker, after, label, prompt=None):
    """`discriminate`, minus the read through `trial.grid`.

    That helper decodes this trial's whole capture from byte zero, so on this
    trial it would fail `grid.unknown` on the tear's own undeclared bytes
    rather than on anything this row is actually testing. Every other part of
    the three-part check -- nonce in the request, marker on the screen, the
    screen not blank -- is unchanged.
    """
    trial.send((prompt or ("say " + run.nonce)) + "\r")
    trial.wait_for(marker)
    sent = fixture.bodies()
    run.require(
        any(run.nonce in body for body in sent),
        "recovery: the nonce this run minted is in the request xfx sent (%d request(s) captured)"
        % len(sent),
    )
    grid = recovered_grid(trial, after, label)
    run.require(grid.text().strip() != "", "recovery: the screen is not blank")
    run.require(
        grid.find(marker) is not None,
        "recovery: the fixture's own marker %r is rendered on the screen" % marker,
    )
    run.require(
        not grid.unknown,
        "recovery: the reconstructed post-cleanup screen emits only declared sequences: %r"
        % grid.unknown,
    )
    return grid


def scenario_3d(run):
    """The one row `3c` cannot drive: a torn primary-band frame this build
    actually recovers from, in the same call, on a real terminal.

    `fault="partial-frame-once"` tears the same first frame `3c` does, but
    answers the recovery attempt's own fixed cleanup vector for real instead
    of refusing it too -- so this row watches the rest of
    `Band::recover_primary` actually happen: the cleanup lands, the band is
    rebuilt from an erased shadow, and the session keeps running rather than
    dying.

    The oracle cannot be trusted with the torn prefix or the cleanup vector's
    own bytes (`recovered_peek` above), so every grid this row reads is
    sliced to start after the cleanup vector. That is a real screen a real
    terminal shows, but it is **post-cleanup reconstruction**: it says the
    rebuild reached the terminal correctly, not that a terminal's own parser
    resynchronized on the torn prefix -- that claim needs an independent
    terminal experiment, same as `Band::recover_primary`'s own doc comment
    says, and is not made here.
    """
    marker = run.marker("recovered")
    # Two scripted replies: the control's own turn and the recovered trial's
    # turn, each a real round trip against this scenario's own fixture.
    fixture = start_fixture(run, [fixtures.content_only(marker), fixtures.content_only(marker)])

    # -- the control: this scenario's own text, rendered for real ---------
    control = run.trial("normal-control", gateway=fixture).settled()
    run.require(control.modes().is_raw(), "control: the session took the terminal into raw mode")
    launch_frame = first_frame(bytes(control.session.captured))
    run.require(
        launch_frame is not None and len(launch_frame) > len(FRAME_BEGIN_BYTES),
        "control: the launch painted a complete first frame to compare against",
    )
    discriminate(run, control, fixture, marker)
    control.send("/quit\r")
    run.require(control.session.wait_exit() == ("exited", 0), "control: /quit leaves with 0")
    run.require(control.modes() == control.before, "control: termios byte-identical")

    # -- the same launch, onto a terminal that tears once and is recovered -
    trial = run.trial(
        "torn-then-recovered", faulty=True, fault="partial-frame-once", gateway=fixture
    ).settled()
    run.require(trial.modes().is_raw(), "recovery: raw mode really was entered")

    def recovered_and_repainted(_text):
        captured = bytes(trial.session.captured)
        at = captured.find(RECOVERY_CLEANUP_BYTES)
        return at >= 0 and captured.count(FRAME_END_BYTES, at) >= 2

    # The tear, the cleanup, the rebuild -- and the frame the tick behind a
    # recovery is forced to write (`FrameFailures::pending_recovery`), as `3e`
    # waits for on the other plane. Waited for rather than left to however
    # much `settled()`'s last read happened to take, so what is captured below
    # is the same wire on every run.
    trial.wait_until(
        "the cleanup, the rebuild and the frame the next tick owed", recovered_and_repainted
    )
    captured = bytes(trial.session.captured)

    begins = captured.find(FRAME_BEGIN_BYTES)
    run.require(begins >= 0, "recovery: a band frame really reached the terminal")
    cleanup_at = captured.find(RECOVERY_CLEANUP_BYTES, begins if begins >= 0 else 0)
    run.require(cleanup_at > begins >= 0, "recovery: the fixed cleanup vector followed the tear")
    prefix = captured[begins:cleanup_at] if cleanup_at > begins >= 0 else b""
    run.require(
        0 < len(prefix) < len(launch_frame or b""),
        "recovery: the terminal first took a real, incomplete part of a frame (%d of %d bytes)"
        % (len(prefix), len(launch_frame or b"")),
    )
    run.require(
        bool(launch_frame) and launch_frame.startswith(prefix),
        "recovery: those bytes are a prefix of the frame this launch paints",
    )
    run.require(
        FRAME_END_BYTES not in prefix, "recovery: the torn write never completed its own frame"
    )
    # **And the tear is a band tear.** `Band::recover_primary` rebuilds the
    # band; a tear inside the document's rows ahead of it is `partial-rows`,
    # whose recovery is a different claim. So the premise this row's recovery
    # rests on is asserted, with the fault's own point (T8 round 2).
    require_a_band_tear(run, "recovery", control, launch_frame, prefix)

    cleanup_end = cleanup_at + len(RECOVERY_CLEANUP_BYTES)
    rebuild_region = captured[cleanup_end:]
    rebuild_begins = rebuild_region.find(FRAME_BEGIN_BYTES)
    run.require(rebuild_begins >= 0, "recovery: a fresh frame opened behind the cleanup vector")
    rebuild_ends = rebuild_region.find(
        FRAME_END_BYTES, rebuild_begins if rebuild_begins >= 0 else 0
    )
    run.require(
        rebuild_ends > rebuild_begins >= 0,
        "recovery: the rebuild is a whole frame, opened and closed after the cleanup",
    )
    # Two `FRAME_BEGIN`s (the tear, then the rebuild) and one `FRAME_END`
    # (only the rebuild reaches it) -- pairing the *first* begin with the
    # only end on the wire, as `first_frame` does, would silently splice the
    # tear, the cleanup vector and the rebuild into one "frame" no terminal
    # ever showed. Every check below is bounded by `cleanup_at`/`cleanup_end`
    # instead, on purpose.
    #
    # **Counted through the rebuild's own close, not over `captured`.** The
    # session owes one more frame behind the rebuild, every time: the torn
    # attempt's reasons were handed back (`failed_band`), and the tick after a
    # recovery forces a real write (`FrameFailures::pending_recovery`,
    # `Band::force_redraw`) one `TICK` (8 ms) later. Counted over the whole
    # capture, that frame was in or out by however much the harness's last
    # read took -- on a loaded runner it took both, and the count read 3 and 2
    # (CI run 41832130240) on a wire whose tear and rebuild were exactly right.
    # It is waited for above and asserted below instead; a tear re-offered, or
    # a frame completed, *before* the rebuild still counts here.
    through_rebuild = (
        captured[: cleanup_end + rebuild_ends + len(FRAME_END_BYTES)]
        if rebuild_ends > rebuild_begins >= 0
        else captured
    )
    run.require(
        through_rebuild.count(FRAME_BEGIN_BYTES) == 2,
        "recovery: exactly the frame this build tore and the one it rebuilt (%d)"
        % through_rebuild.count(FRAME_BEGIN_BYTES),
    )
    run.require(
        through_rebuild.count(FRAME_END_BYTES) == 1,
        "recovery: exactly one frame on this wire ever completed before the rebuild closed (%d)"
        % through_rebuild.count(FRAME_END_BYTES),
    )
    # The forced frame: the next whole frame after the rebuild, and nothing
    # between them -- exactly one opener before its close, and no second
    # cleanup vector, so a tear re-offered or recovered again in that gap
    # cannot pass as the tick's own write.
    forced = captured[len(through_rebuild) :]
    forced_ends = forced.find(FRAME_END_BYTES)
    forced_frame = forced[: forced_ends + len(FRAME_END_BYTES)] if forced_ends >= 0 else b""
    run.require(
        forced_ends >= 0
        and forced_frame.startswith(FRAME_BEGIN_BYTES)
        and forced_frame.count(FRAME_BEGIN_BYTES) == 1
        and RECOVERY_CLEANUP_BYTES not in forced_frame,
        "recovery: the tick behind the recovery wrote one whole frame of its own, "
        "and nothing else, right behind the rebuild (%r)" % forced[:80],
    )

    # -- the rebuilt screen: divider, composer, hint row, caret -----------
    # Read **through the rebuild's own close** and no further: the forced
    # frame behind it rewrites every band row, so a grid that included it
    # would be judging that frame and passing a rebuild that dropped the hint
    # or misplaced the caret. The forced frame is checked against it below.
    grid = recovered_grid(trial, cleanup_end, "rebuilt", until=len(through_rebuild))
    run.require(composer_text(grid) == "", "recovery: the rebuilt composer is empty")
    # Restated for the fx band (`a50f8a0`): there is no always-drawn divider
    # any more, so "the rebuilt screen has a divider" is now the whole band
    # structure read up from its hint -- a blank rule row under the composer
    # and no panel -- which a rebuild that dropped or misplaced a row fails.
    rebuilt_band = find_band(grid)
    run.require(
        rebuilt_band is not None
        and grid.row_text(rebuilt_band.divider) == ""
        and rebuilt_band.panel == []
        and rebuilt_band.divider == rebuilt_band.composer[-1] + 1,
        "recovery: the rebuilt band is whole -- composer, its blank rule row, the hint (%r)"
        % (rebuilt_band,),
    )
    run.require(
        hint_text(grid) == HINT_AUTO,
        "recovery: the rebuilt hint row says what a turn would run with: %r" % hint_text(grid),
    )
    composer_row = composer_first_row(grid)
    run.require(
        composer_row is not None and grid.row == composer_row and grid.col == 2,
        "recovery: the caret sits at the rebuilt, empty composer's own start (row %r, col %d)"
        % (grid.row, grid.col),
    )
    run.require(
        not grid.unknown,
        "recovery: the rebuilt screen emits only declared sequences: %r" % grid.unknown,
    )
    # And the frame the next tick was forced to write shows that same screen,
    # caret and all -- the standing form of "it is an identical repaint".
    repainted = recovered_grid(
        trial,
        cleanup_end,
        "repainted-by-the-next-tick",
        until=len(through_rebuild) + len(forced_frame),
    )
    run.require(
        repainted.text() == grid.text() and (repainted.row, repainted.col) == (grid.row, grid.col),
        "recovery: the forced repaint shows the screen the rebuild did (caret %r vs %r)"
        % ((repainted.row, repainted.col), (grid.row, grid.col)),
    )

    # -- typed nonce visible, C-u empties it, the real prompt is submitted -
    trial.send(run.nonce)
    trial.wait_until(
        "the typed nonce in the rebuilt composer",
        lambda _t: composer_text(recovered_peek(trial, cleanup_end)) == run.nonce,
    )
    typed = recovered_grid(trial, cleanup_end, "typed-nonce")
    run.require(
        composer_text(typed) == run.nonce,
        "recovery: the typed nonce is in the composer: %r" % composer_text(typed),
    )
    trial.send(b"\x15")  # C-u
    trial.wait_until(
        "the composer to empty after C-u",
        lambda _t: composer_text(recovered_peek(trial, cleanup_end)) == "",
    )
    recovered_discriminate(run, trial, fixture, marker, cleanup_end, "post-recovery")

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "recovery: Ctrl-D leaves with 0")
    run.require(
        trial.modes() == trial.before, "recovery: termios byte-identical after a recovered tear"
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 3e. a torn repaint of the alternate plane the session already owns
# ---------------------------------------------------------------------------

# The bytes that open any `?1049` sequence, take or give: what a vector that
# moved the terminal between its two buffers would have to carry.
PLANE_MOVE_BYTES = b"\x1b[?1049"


def excised(trial, torn_at, cleanup_end, until=None):
    """The capture with the torn repaint and the cleanup vector cut out of it.

    `recovered_peek`'s slice from the cleanup's end would lose the `1049h` that
    took the plane, and with it the one fact this row is about -- which buffer
    the rebuild landed on. So the stream is kept on both sides of the tear and
    only the two regions the oracle cannot read are removed: the torn prefix,
    which may end inside a sequence, and the cleanup, whose `CAN` and `OSC 8`
    its allowlist refuses. Neither carries a `?1049` -- the scenario asserts
    that on the bytes before it reads a grid built this way -- so cutting them
    out cannot change the plane; what it does assume is that the terminal
    discarded the fragment, which is the parser-resync claim nothing here
    makes. Every grid from it says so in its file.
    """
    captured = bytes(trial.session.captured)
    if until is not None:
        captured = captured[:until]
    spliced = received(captured[:torn_at] + captured[cleanup_end:])
    begins = spliced.rfind(FRAME_BEGIN_BYTES)
    if begins >= 0 and spliced.find(FRAME_CLOSE_BYTES, begins) < 0:
        spliced = spliced[:begins]
    return spliced


def excised_peek(trial, torn_at, cleanup_end, until=None):
    return Grid(trial.rows, trial.cols).feed(excised(trial, torn_at, cleanup_end, until))


def excised_grid(trial, torn_at, cleanup_end, label, until=None):
    grid = excised_peek(trial, torn_at, cleanup_end, until)
    trial.snapshots += 1
    path = os.path.join(trial.dir, "grid-%02d-excised-%s.txt" % (trial.snapshots, label))
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(
            "# torn prefix and cleanup vector excised; post-cleanup reconstruction, "
            "not proof of parser resync\n"
        )
        handle.write(grid.snapshot())
    return grid


def asked_on_the_alternate_plane(run, trial, what):
    """Asks for the large edit with this run's nonce and waits for the question.

    Returns how many bytes the terminal held once the question's frame was
    complete, which is where every later search for the repaint starts.
    """
    trial.send("edit the notes " + run.nonce + "\r")
    trial.wait_for(READ_NOTES)
    trial.wait_for(ALTERNATE_ENTER)
    trial.wait_until(
        "%s: the question to be painted on the plane it took" % what,
        lambda _t: trial.peek().find(PERMISSION_TITLE) is not None,
    )
    grid = trial.grid("question-on-the-alternate-plane")
    run.require(grid.plane == "alternate", "%s: the question is on a plane of its own" % what)
    run.require(grid.entered_alternate == 1, "%s: the plane was taken once" % what)
    run.require(not grid.unknown, "%s: the question emits only declared sequences" % what)
    return len(trial.session.captured)


def scenario_3e(run):
    """A torn repaint of the alternate plane the session already owns: recovered
    in the same call when the cleanup lands, fatal when it does not.

    The one other vector `3d`'s recovery reaches. A question about a change too
    big for the band is up on the plane it took; one step of the walk (`C-p`,
    up from the tail the review opens on -- `.prd/tui-phase4/receipts/f2`,
    "the review's tail against the dotted rule" -- where `C-n` is a step past
    the end that paints nothing) asks for a repaint of that plane, which takes no plane, gives none back and
    scrolls nothing into a scrollback the terminal keeps -- so its whole
    intended content is in hand when it tears. `partial-alternate-once` takes
    half of that repaint and fails, and then answers the fixed cleanup and the
    rebuild for real; `partial-alternate` refuses the cleanup too, so the
    session must end on it. Neither fault can land on the `1049h` that took the
    plane or the `1049l` that gives it back: those transitions stay contained
    and fatal, and are not what this row drives.

    **The discriminator.** The control -- release binary, nothing injected --
    answers the same question end to end and carries this run's nonce in its
    request and the fixture's marker on its screen. The torn sessions cannot be
    compared with the control's bytes, because the review screen shows the
    workspace path and every trial has its own; so each is discriminated
    against **itself**: the rebuild is asserted to open with exactly the bytes
    the terminal took and then finish them -- the same rows, geometry and caret
    the torn attempt was built from -- and the recovered session's nonce, marker
    and answer are asserted as the control's are.

    **What is not claimed**: that a terminal's parser resynchronized on the torn
    prefix. Every grid after the tear is built with the prefix and the cleanup
    cut out (`excised`), which reconstructs what the rebuild painted and says
    nothing about what the fragment did.
    """
    marker = run.marker("altrecovered")
    before, after = fixtures.large_edit_sides()

    # -- the control: the same question, answered, nothing injected -------
    control_fixture = start_fixture(
        run, fixtures.large_edit_then_finish(marker), name="control-gateway"
    )
    control = run.trial(
        "normal-control",
        gateway=control_fixture,
        mode="ask",
        notes=True,
        notes_text=before + "\n",
    ).settled()
    run.require(control.modes().is_raw(), "control: the session took the terminal into raw mode")
    asked = asked_on_the_alternate_plane(run, control, "control")
    control.send(b"\x10")
    control.wait_until(
        "control: the walk to repaint the plane",
        lambda _t: bytes(control.session.captured).find(FRAME_END_BYTES, asked) >= 0,
    )
    control.send(b"1")
    control.wait_for(marker)
    run.require(
        any(run.nonce in body for body in control_fixture.bodies()),
        "control: the nonce this run minted is in the request xfx sent",
    )
    control.wait_until(
        "control: the band to be painted on the primary plane again",
        lambda _t: control.peek().plane == "primary"
        and HINT_MODEL in (hint_text(control.peek()) or ""),
    )
    grid = control.grid("control-back-on-the-primary-plane")
    run.require(grid.text().strip() != "", "control: the screen is not blank")
    run.require(grid.find(marker) is not None, "control: the fixture's own marker is rendered")
    run.require(not grid.unknown, "control: only declared sequences: %r" % grid.unknown)
    run.require(read(control.notes) == after + "\n", "control: `1` let the edit through")
    control.send(b"\x04")
    run.require(control.session.wait_exit() == ("exited", 0), "control: Ctrl-D leaves with 0")
    run.require(control.modes() == control.before, "control: termios byte-identical")
    control_fixture.stop()

    # -- the repaint torn once, and recovered --------------------------------
    once_fixture = start_fixture(
        run, fixtures.large_edit_then_finish(marker), name="recovered-gateway"
    )
    trial = run.trial(
        "torn-then-recovered",
        faulty=True,
        fault="partial-alternate-once",
        gateway=once_fixture,
        mode="ask",
        notes=True,
        notes_text=before + "\n",
    ).settled()
    run.require(trial.modes().is_raw(), "recovery: raw mode really was entered")
    asked = asked_on_the_alternate_plane(run, trial, "recovery")

    trial.send(b"\x10")

    def recovered_and_repainted(_text):
        captured = bytes(trial.session.captured)
        at = captured.find(RECOVERY_CLEANUP_BYTES, asked)
        return at >= 0 and captured.count(FRAME_END_BYTES, at) >= 2

    # The tear, the cleanup, the rebuild -- and the repaint the tick behind a
    # recovery is forced to write, which is the frame readiness comes back
    # from. An answer typed before it lands is refused as not ready.
    trial.wait_until(
        "the cleanup, the rebuild and the repaint the next tick owed", recovered_and_repainted
    )
    captured = bytes(trial.session.captured)

    cleanup_at = captured.find(RECOVERY_CLEANUP_BYTES, asked)
    run.require(cleanup_at >= asked, "recovery: the fixed cleanup vector followed the question")
    torn_at = captured.rfind(FRAME_BEGIN_BYTES, asked, cleanup_at)
    run.require(
        torn_at >= asked, "recovery: the torn vector was a repaint written after the question"
    )
    prefix = captured[torn_at:cleanup_at] if torn_at >= asked else b""
    run.require(
        len(prefix) > len(FRAME_BEGIN_BYTES),
        "recovery: the terminal took a real part of the repaint (%d bytes)" % len(prefix),
    )
    run.require(
        FRAME_END_BYTES not in prefix, "recovery: the torn repaint never completed its frame"
    )
    run.require(
        PLANE_MOVE_BYTES not in prefix,
        "recovery: the torn vector was a repaint, not a plane transition",
    )
    cleanup_end = cleanup_at + len(RECOVERY_CLEANUP_BYTES)
    rebuilt = captured[cleanup_end:]
    rebuilt_end = rebuilt.find(FRAME_END_BYTES)
    rebuilt_end = rebuilt_end + len(FRAME_END_BYTES) if rebuilt_end >= 0 else -1
    run.require(rebuilt_end > 0, "recovery: the rebuild is a whole frame after the cleanup")
    run.require(
        rebuilt.startswith(prefix) and rebuilt_end > len(prefix),
        "recovery: the rebuild opens with exactly the bytes the terminal took and finishes them",
    )
    run.require(
        b"\x1b[1;1H\x1b[2J" in rebuilt[:rebuilt_end],
        "recovery: the rebuild erases the plane before it paints it",
    )
    run.require(
        PLANE_MOVE_BYTES not in captured[torn_at:cleanup_end + max(rebuilt_end, 0)],
        "recovery: neither the cleanup nor the rebuild moved the terminal between its planes",
    )
    forced = rebuilt[max(rebuilt_end, 0) :]
    run.require(
        forced.find(FRAME_BEGIN_BYTES) >= 0 and forced.find(FRAME_END_BYTES) >= 0,
        "recovery: the tick behind the recovery wrote a whole repaint of its own",
    )
    run.require(
        captured.count(ALTERNATE_ENTER.encode()) == 1
        and ALTERNATE_LEAVE.encode() not in captured,
        "recovery: the plane was taken once and not given back before the answer",
    )

    # -- the rebuilt screen, and the one the next tick painted ------------------
    until_rebuilt = cleanup_end + max(rebuilt_end, 0)
    grid = excised_grid(trial, torn_at, cleanup_end, "rebuilt", until=until_rebuilt)
    run.require(grid.plane == "alternate", "recovery: the rebuild landed on the plane it tore")
    run.require(
        grid.entered_alternate == 1 and grid.left_alternate == 0,
        "recovery: the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    run.require(grid.find(PERMISSION_TITLE) is not None, "recovery: the rebuild names the question")
    for choice in (CHOICE_MARK + APPLY_ONCE, DONT_APPLY):
        run.require(grid.find(choice) is not None, "recovery: the rebuild kept %r" % choice)
    run.require(
        not grid.unknown,
        "recovery: the rebuild emits only declared sequences: %r" % grid.unknown,
    )
    primary = grid.primary_text()
    run.require(
        READ_NOTES in primary and HINT_MODEL in primary,
        "recovery: the user's own screen is still saved behind the plane: %r" % primary,
    )
    repainted = excised_grid(trial, torn_at, cleanup_end, "repainted-by-the-next-tick")
    run.require(
        repainted.text() == grid.text() and repainted.plane == "alternate",
        "recovery: the forced repaint shows the same question the rebuild did",
    )

    # -- still answerable, and the session goes on --------------------------
    trial.send(b"1")
    trial.wait_for(marker)
    run.require(ALTERNATE_LEAVE in trial.text(), "recovery: the answer gave the plane back")
    run.require(read(trial.notes) == after + "\n", "recovery: `1` let the edit through")
    trial.wait_until(
        "the band to be painted on the primary plane after the recovered question",
        lambda _t: excised_peek(trial, torn_at, cleanup_end).plane == "primary"
        and HINT_MODEL in (hint_text(excised_peek(trial, torn_at, cleanup_end)) or ""),
    )
    grid = excised_grid(trial, torn_at, cleanup_end, "back-on-the-primary-plane")
    run.require(
        grid.entered_alternate == grid.left_alternate == 1,
        "recovery: the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    run.require(grid.find(marker) is not None, "recovery: the fixture's own marker is rendered")
    run.require(grid.find("beta line") is None, "recovery: the change left the user's screen")
    run.require(not grid.unknown, "recovery: only declared sequences: %r" % grid.unknown)
    run.require(
        any(run.nonce in body for body in once_fixture.bodies()),
        "recovery: the nonce this run minted is in the request xfx sent",
    )
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "recovery: Ctrl-D leaves with 0")
    run.require(trial.modes() == trial.before, "recovery: termios byte-identical")
    run.require(
        not os.path.exists(os.path.join(trial.home, ".xfx", "last-tui-error.json")),
        "recovery: a session that recovered left no give-up report",
    )
    once_fixture.stop()

    # -- the same tear, with the cleanup refused too ------------------------
    fatal_fixture = start_fixture(
        run, fixtures.large_edit_then_finish(marker), name="unrecovered-gateway"
    )
    torn = run.trial(
        "torn-unrecovered",
        faulty=True,
        fault="partial-alternate",
        gateway=fatal_fixture,
        mode="ask",
        notes=True,
        notes_text=before + "\n",
    ).settled()
    run.require(torn.modes().is_raw(), "containment: raw mode really was entered")
    asked = asked_on_the_alternate_plane(run, torn, "containment")
    run.require(
        any(run.nonce in body for body in fatal_fixture.bodies()),
        "containment: the nonce this run minted is in the request xfx sent",
    )
    torn.send(b"\x10")
    state = torn.session.wait_exit()
    run.require(
        state == ("exited", 1),
        "containment: the session left with its own error (%r)" % (state,),
    )
    captured = bytes(torn.session.captured)
    text = torn.session.settled_text()
    run.require(
        RECOVERY_CLEANUP_BYTES not in captured,
        "containment: the refused cleanup is not on the wire",
    )
    torn_at = captured.find(FRAME_BEGIN_BYTES, asked)
    left_at = captured.find(ALTERNATE_LEAVE.encode(), torn_at if torn_at >= 0 else asked)
    run.require(
        left_at > torn_at >= asked, "containment: the exit gave the plane back after the tear"
    )
    prefix = captured[torn_at:left_at] if left_at > torn_at >= asked else b""
    run.require(
        len(prefix) > len(FRAME_BEGIN_BYTES) and FRAME_END_BYTES not in prefix,
        "containment: the terminal took a real, incomplete part of the repaint (%d bytes)"
        % len(prefix),
    )
    run.require(
        prefix.count(FRAME_BEGIN_BYTES) == 1 and PLANE_MOVE_BYTES not in prefix,
        "containment: the torn repaint was never offered again and moved no plane",
    )
    reported = re.search(r"accepted (\d+) bytes", text)
    run.require(
        reported is not None and int(reported.group(1)) == len(prefix),
        "containment: the reported count is the number of bytes on the wire (%s vs %d)"
        % (reported.group(1) if reported else "none", len(prefix)),
    )
    run.require(
        "could not be recovered" in text,
        "containment: the session says a recovery was tried and failed: %r" % text[-300:],
    )
    restore_at = captured.find(pty.RESTORE.encode(), left_at if left_at >= 0 else 0)
    run.require(restore_at > left_at >= 0, "containment: the exit's restore followed the leave")
    run.require(torn.modes() == torn.before, "containment: termios byte-identical")
    run.require(
        read(torn.notes) == before + "\n",
        "containment: the edit did not run on a question that was never answered",
    )
    report_path = os.path.join(torn.home, ".xfx", "last-tui-error.json")
    try:
        with open(report_path, encoding="utf-8") as handle:
            body = handle.read()
        report = json.loads(body)
    except (OSError, ValueError) as failure:
        body, report = "", {}
        run.require(False, "containment: the independent report is readable: %s" % failure)
    run.require(
        report.get("schema") == 1 and report.get("reason") == "partial",
        "containment: the independent report names the tear: %r" % body,
    )
    run.require("accepted" not in body, "containment: the report carries no session text")
    fatal_fixture.stop()


# ---------------------------------------------------------------------------
# 3f. under job control
# ---------------------------------------------------------------------------


def scenario_3f(run):
    """The stop, the continue and the two deaths, with xfx a shell's foreground job.

    Scenario 3's signal rows give the child a process group of its own inside
    the harness's session and no terminal of its own: enough for a stop to be
    a stop, and not what a user's shell does. Here the session is the one a
    job-control shell makes (`pty.Session`'s `job_control`): a leader that owns
    the terminal, and xfx in its own group as the terminal's **foreground**, so
    the `tcsetattr`s a stop and a resume make are a foreground job's, and a
    resume is answered on the terminal the job really holds. The same rows as
    3's -- the stop gives the terminal back, the continue re-takes it and
    paints, a second stop is a stop, `SIGTERM` and `SIGHUP` restore and end the
    process by the signal -- read off that terminal while its leader still holds
    it, then a turn after two stops so the resumed session is shown to work.

    What fx does differently here is not asserted (the K-jobctl gap in
    `.prd/tui-phase4/loop.md`: fx writes nothing on an external `SIGTSTP`, and
    suspends on a typed Ctrl-Z, which xfx reads as a byte); this row holds the
    terminal-safety contract `03-tui-port.md` §"Signals" gives xfx's own stop.
    """
    marker = run.marker("job")
    fixture = start_fixture(run, [fixtures.content_only(marker)])

    job = run.trial("foreground-job", gateway=fixture, job_control=True).settled()
    pid = job.session.pid
    run.require(
        job.session.foreground == pid,
        "the job owns the terminal: its foreground group is xfx's (%r, xfx %r)"
        % (job.session.foreground, pid),
    )
    run.require(
        os.getpgid(pid) == pid and os.getsid(pid) == job.session.leader != os.getsid(0),
        "xfx leads a group of its own in its leader's session, not the harness's",
    )
    run.require(job.modes().is_raw(), "the foreground job took its terminal into raw mode")

    for round_ in ("first", "second"):
        stopped_at = len(job.session.captured)
        job.session.signal(signal.SIGTSTP)
        state = job.session.wait_state(
            "the job to stop (%s)" % round_, lambda seen: seen[0] == "stopped"
        )
        run.require(
            state[0] == "stopped", "%s SIGTSTP: the job is really stopped (%r)" % (round_, state)
        )
        run.require(
            job.modes() == job.before,
            "%s SIGTSTP: the terminal was given back before the stop, byte for byte" % round_,
        )
        job.wait_until(
            "the stop's restore on the wire (%s)" % round_,
            lambda text: pty.ABNORMAL_RESTORE.encode() in bytes(job.session.captured)[stopped_at:],
        )
        resumed_at = len(job.session.captured)
        job.session.signal(signal.SIGCONT)
        state = job.session.wait_state(
            "the job to continue (%s)" % round_, lambda seen: seen[0] != "stopped"
        )
        run.require(
            state[0] == "continued",
            "%s SIGCONT: the leader saw the job continue (%r)" % (round_, state),
        )
        run.require(
            job.wait_until_raw().is_raw(),
            "%s SIGCONT: raw mode was positively re-entered on the job's terminal" % round_,
        )
        job.wait_until(
            "the resumed job's mode set and repaint (%s)" % round_,
            lambda text: pty.MODE_SET.encode() in bytes(job.session.captured)[resumed_at:]
            and FRAME_END_BYTES in bytes(job.session.captured)[resumed_at:],
        )
    discriminate(run, job, fixture, marker, label="after-two-stops")
    job.send(b"\x04")
    run.require(job.session.wait_exit() == ("exited", 0), "after two stops the job leaves at 0")
    job.session.settled_text()
    run.require(job.modes() == job.before, "after two stops: termios byte-identical")

    for name, number in (
        ("foreground-sigterm", signal.SIGTERM),
        ("foreground-sighup", signal.SIGHUP),
    ):
        trial = run.trial(name, gateway=fixture, job_control=True).settled()
        run.require(
            trial.session.foreground == trial.session.pid,
            "%s: the job owns the terminal it is signalled on" % name,
        )
        trial.session.signal(number)
        state = trial.session.wait_exit()
        run.require(
            state == ("signalled", int(number)),
            "%s: the job died *by* the signal (%r)" % (name, state),
        )
        run.require(
            pty.ABNORMAL_RESTORE in trial.session.settled_text(),
            "%s: the handler wrote the abnormal restore before re-raising" % name,
        )
        run.require(trial.modes() == trial.before, "%s: termios byte-identical" % name)
    fixture.stop()


# ---------------------------------------------------------------------------
# 3g. started ignoring SIGINT and SIGHUP
# ---------------------------------------------------------------------------

# The command the inherited-ignores row has the model run, on one line as the
# `terminal` tool takes one. It asks each signal of its own shell whether it is
# still ignored -- a shell that kills itself with a signal it ignores lives on to
# say `kept`; one with the default dies before it can, as `process.rs`'s own
# cases ask -- then ignores the three a stop could send it and parks a sleeper,
# so only a `SIGKILL` ends the group. `ulimit -c 0` first: `SIGQUIT`'s default
# writes a core.
IGNORES_PROBE = (
    "ulimit -c 0; "
    "(sh -c 'kill -INT $$; echo kept') > int.txt 2>&1; "
    "(sh -c 'kill -HUP $$; echo kept') > hup.txt 2>&1; "
    "(sh -c 'kill -QUIT $$; echo kept') > quit.txt 2>&1; "
    "trap '' INT HUP TERM; sleep 30 & echo $! > sleeper.pid; wait"
)

# How long a command that ignores every catchable stop may outlive two Escapes:
# fx's grace for a cooperative stop (`src/tools/terminal.rs`'s
# `TERMINATION_GRACE`, 700 ms, `command_runner.zig:85`) and then the `SIGKILL`,
# with room for a loaded runner. The sleeper would otherwise live thirty seconds.
IGNORED_STOP_DEADLINE = 5.0


def scenario_3g(run):
    """xfx started the way `nohup … &` starts it: `SIGHUP`, `SIGINT` and `SIGQUIT` ignored.

    T9's rule (`src/process.rs:24-43`, merged `104ccaa2`; made exact for what
    the TUI catches in `fix/p2-sigint-inherit`): **a signal xfx was started
    ignoring and does not catch is left ignored** in what it runs, as a shell
    leaves it --
    fx's spawn sets no `POSIX_SPAWN_SETSIGDEF` (`darwin_process_spawn.zig:74-83`)
    -- and **xfx's own stop of a command is `SIGKILL` to the command's group**,
    which no ignore holds back. `process.rs`'s cases hold the mask half on a
    runner started with these ignores; this row holds the rule end to end, on
    the release build, in the TUI:

    * `SIGQUIT`, which neither xfx nor fx catches, is still ignored in the
      command.
    * `SIGINT` is still ignored in the command. fx c1db919 catches no `SIGINT`
      while it runs (`app_lifecycle.zig:82-84` leaves it out; its only `SIGINT`
      action is the graceful-exit ignore around its shutdown,
      `app_entry_runtime.zig:370`), so the launcher's ignore is what fx's
      command inherits -- and xfx's, since its TUI leaves an inherited
      `SIGINT` ignore alone (`src/tui/signals.rs`'s `install_handlers`).
    * `SIGHUP` is **not**: while the TUI owns the terminal a hangup is its to
      answer -- fx installs its restore over the launcher's ignore
      (`installAbnormalExitHandlers`, `app_lifecycle.zig:85-104`) and xfx
      does the same (`src/tui/signals.rs`'s `install_handlers`) -- and `exec`
      gives a caught signal its default, so the command's `SIGHUP` is the
      default under both.
    * Two Escapes stop the command although it ignores `SIGINT`, `SIGHUP` and
      `SIGTERM`, within fx's grace and the kill after it.
    * And an external `SIGHUP` still restores the terminal and ends xfx by the
      signal, the launcher's ignore notwithstanding, as fx's handler does.
    """
    marker = run.marker("ignored")
    script = [
        {
            "events": [
                fixtures.tool_call(
                    "call-0", "terminal", {"action": "exec", "command": IGNORES_PROBE}
                ),
                fixtures.finish("tool-calls"),
            ]
        },
        fixtures.content_only(marker),
    ]
    fixture = start_fixture(run, script)
    trial = run.trial(
        "started-ignoring",
        gateway=fixture,
        mode="yolo",
        own_process_group=True,
        ignored=(signal.SIGHUP, signal.SIGINT, signal.SIGQUIT),
    ).settled()
    trial.send("probe the signals " + run.nonce + "\r")
    sleeper_file = os.path.join(trial.workspace, "sleeper.pid")
    deadline = time.time() + pty.WAIT
    sleeper = None
    while sleeper is None:
        try:
            with open(sleeper_file, encoding="utf-8") as handle:
                sleeper = int(handle.read().strip() or "0") or None
        except (OSError, ValueError):
            sleeper = None
        if sleeper is None:
            if time.time() > deadline:
                raise Failure(
                    "the probe command never parked its sleeper: %r" % trial.text()[-400:]
                )
            trial.session.pump(0.02)
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request that asked for the command",
    )

    def said(name):
        try:
            with open(os.path.join(trial.workspace, name), encoding="utf-8") as handle:
                return handle.read()
        except OSError:
            return None

    run.require(
        said("quit.txt") == "kept\n",
        "SIGQUIT, which xfx does not own, is still ignored in the command it runs: %r"
        % said("quit.txt"),
    )
    run.require(
        said("int.txt") == "kept\n",
        "SIGINT, which xfx was started ignoring and fx c1db919 leaves alone while it runs, "
        "is still ignored in the command it runs (process.rs:24-43; got %r -- the command "
        "died of it)" % said("int.txt"),
    )
    # Died is "never said `kept`": a shell may report its child's signal on
    # the same stream, in words of its own.
    run.require(
        said("hup.txt") is not None and "kept" not in said("hup.txt"),
        "SIGHUP is the TUI's while it owns the terminal, as fx's, so the command has its "
        "default and died of it: %r" % said("hup.txt"),
    )

    trial.send(b"\x1b")
    trial.wait_until(
        "the first Escape to arm the interrupt over the command",
        lambda _t: interrupt_armed(hint_text(trial.peek())),
    )
    escaped = time.time()
    trial.send(b"\x1b")
    while True:
        try:
            os.kill(sleeper, 0)
        except OSError:
            break
        if time.time() - escaped > IGNORED_STOP_DEADLINE:
            break
        trial.session.pump(0.01)
    lived = time.time() - escaped
    with open(os.path.join(trial.dir, "stop-latency.txt"), "w", encoding="utf-8") as handle:
        handle.write("sleeper %d gone %.3fs after the second Escape\n" % (sleeper, lived))
    run.require(
        lived <= IGNORED_STOP_DEADLINE,
        "a command ignoring SIGINT, SIGHUP and SIGTERM was stopped by the kill to its group "
        "(gone %.2fs after the second Escape, deadline %.1fs)" % (lived, IGNORED_STOP_DEADLINE),
    )
    trial.send("next " + run.nonce + "\r")
    grid = on_the_grid(trial, marker, "the answer after the stopped command")
    grid = trial.grid("after-the-stop")
    bodies = fixture.bodies()
    run.require(
        len(bodies) == 2 and tool_result_text(bodies[1], "call-0") == QUESTION_ABORTED,
        "the stopped call is told to the model as fx's `aborted by user`: %r"
        % (tool_result_text(bodies[1], "call-0") if len(bodies) > 1 else None,),
    )
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")

    trial.session.signal(signal.SIGHUP)
    state = trial.session.wait_exit()
    run.require(
        state == ("signalled", int(signal.SIGHUP)),
        "an external SIGHUP still ends xfx by the signal, the launcher's ignore notwithstanding "
        "(%r)" % (state,),
    )
    run.require(
        pty.ABNORMAL_RESTORE in trial.session.settled_text(),
        "and its handler wrote the abnormal restore first",
    )
    run.require(trial.modes() == trial.before, "started ignoring: termios byte-identical")
    fixture.stop()


# ---------------------------------------------------------------------------
# 4. raw mode positively entered
# ---------------------------------------------------------------------------


# How the starved-screen row's deadline is arrived at, because a number nobody
# can derive is a number nobody can defend. The property under test is
# `event_loop::FRAME_BUDGET` -- **half a second** of wall clock, past which a
# screen that has taken nothing ends the session instead of being retried
# forever -- so the bound has to be close enough to half a second that a budget
# which quietly stopped being enforced is detectable. It is an **upper** bound
# on both roads out: a screen that takes a prefix of the first frame ends the
# session before the budget's clock matters at all, which is faster than this
# and inside it. The first version of this
# row accepted fifteen seconds, which bounds nothing: a regression to a
# fourteen-second retry would have passed it.
#
#   the fill             <= STARVING_CEILING = 1.50 s
#                        (a ceiling the harness enforces rather than a cost:
#                         the loop stops the moment the session leaves, and the
#                         budget's clock starts at the first refusal, which is
#                         inside this window)
#   FRAME_BUDGET         0.50 s   `src/tui/event_loop.rs`
#   the give-up path     0.50 s   leave through `hold`, `term::shutdown`, exit
#   CI slack             1.50 s   a loaded hosted runner, four jobs in parallel
#   ------------------------------------------------------------------
#   STARVED_DEADLINE     4.00 s
#
# Measured five consecutive runs on each of the two kernels this row was written
# against: **0.68-0.70 s** on macOS 26 arm64 and **1.48-1.99 s** on Linux 7.1
# x86_64. The difference between them is the fill and nothing else -- the
# larger screen has to be given more of the draft before one frame outgrows it
# -- which is why the ceiling above is the term the slack is measured against.
#
# **Those two numbers were measured against the whole-band painter**, and item
# 11 replaced it: a frame is a difference now, so how quickly a screen fills is
# a property of what changed rather than of the screen's size. The fill is
# therefore driven for the *whole* window rather than for a fixed number of
# chunks, and the elapsed is bounded by `STARVING_CEILING` plus the give-up path
# by construction rather than by measurement. Re-measured on macOS 26 arm64
# after that change only as five consecutive green runs of this scenario; the
# per-kernel timings above are left as the history of where the budget came
# from and are not re-asserted.
# The slack is generous about the *machine* and strict about the *policy*,
# which is the split the drain row's own comment argues for -- bound the
# contract, never the scheduler.
#
# The geometry and the fill are the row's premise rather than a taste, and they
# are the numbers that make the refusal a size rather than a race. All four are
# measured against a screen that takes frames, not estimated:
#
#   the draft         as many `STARVING_CHUNK`s as the window takes, offered
#                     one at a time and never more than one outstanding -- a
#                     fixed count of them was what an earlier version spent, and
#                     it ended the fill before the budget could expire. About
#                     thirty-seven of them fit a full window, which is
#                     12,617 letters of `ẋ`: a screen counts bytes and a
#                     composer counts characters, so a three-byte letter buys
#                     the frame its size at a third of the wrapping the session
#                     has to do to hold it (a draft is re-wrapped on every
#                     keystroke, and a row that made the session do a hundred
#                     thousand characters' worth of it would be timing the wrap
#                     rather than the budget)
#   the frame it makes 39,187 bytes, against pty capacities of 17,408 (Linux
#                     6.8 and 7.1) and 1,024 (macOS 26) -- 2.25x and 38x -- so
#                     `write_all` cannot land one in a screen of either size
#                     while anything like a terminal is reading it
#   composer cap      layout::input_row_limit(300) = 149 rows, and the draft
#                     occupies 64 of them: the cap is headroom here rather than
#                     the mechanism
#
# Two hundred columns rather than more, because the *first* frame -- the one a
# session opens its band with -- has to fit in the **smallest** of those
# screens or the row would be testing a session that could never draw at all:
# at this width it is **704 bytes** against macOS's 1,024, and it is the only
# complete frame that screen ever takes.
STARVING_ROWS = 300
STARVING_COLS = 200
STARVING_LETTER = "ẋ".encode("utf-8")
STARVING_CHUNK = STARVING_LETTER * 341
STARVING_CEILING = 1.50
STARVED_DEADLINE = STARVING_CEILING + 0.50 + 0.50 + 1.50


def scenario_4(run):
    """The one property this product currently sells, read off the child's terminal."""
    marker = run.marker("raw")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    trial = run.trial("raw-mode", gateway=fixture).settled()

    during = trial.modes()
    for mode in pty.RAW_LOCAL_OFF:
        run.require(not during.local_set(mode), "%s is clear while the session runs" % mode)
    for mode in pty.RAW_INPUT_OFF:
        run.require(not during.input_set(mode), "%s is clear while the session runs" % mode)
    import termios as _termios

    run.require(bool(during.cflag & _termios.CS8), "CS8 is set")
    run.require(during.vmin() == 1, "VMIN is 1 (got %r)" % during.vmin())
    run.require(during.vtime() == 0, "VTIME is 0 (got %r)" % during.vtime())
    text = trial.text()
    run.require(pty.MODE_SET in text, "the interactive mode sequence was written, in order")
    for mouse in ("\x1b[?1000h", "\x1b[?1002h", "\x1b[?1006h"):
        run.require(mouse not in text, "mouse tracking %r is absent" % mouse)

    discriminate(run, trial, fixture, marker)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "Ctrl-D left cleanly")
    run.require(trial.modes() == trial.before, "and the terminal came back byte for byte")
    fixture.stop()

    # Under tmux the kitty keyboard push is omitted, because sending it there
    # breaks key input.
    tmuxed = run.trial("under-tmux", tmux=True).settled()
    text = tmuxed.text()
    run.require("\x1b[>1u" not in text, "under tmux the kitty keyboard push is omitted")
    run.require("\x1b[?2004h" in text, "and the rest of the mode set is still written")
    tmuxed.send(b"\x04")
    run.require(tmuxed.session.wait_exit() == ("exited", 0), "the tmux session left cleanly")
    run.require(
        "\x1b[<u" not in tmuxed.session.settled_text(),
        "and popped nothing it never pushed",
    )


# ---------------------------------------------------------------------------
# 5. editor basics
# ---------------------------------------------------------------------------


def scenario_5(run):
    """Typing, motion, deletion -- and a ZWJ family that moves as one unit."""
    marker = run.marker("editor")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    trial = run.trial("editor", gateway=fixture).settled()

    trial.send("hello 한글")
    # Read off the **composer** rather than off the wire. A frame is a
    # difference from what the terminal already holds, so a keystroke writes the
    # cell it changed: the row is never on the wire in one piece, and what the
    # row says is the property this row of the matrix is about either way.
    trial.wait_until(
        "the composer to hold what was typed",
        lambda _t: composer_text(trial.peek()) == "hello 한글",
    )
    grid = trial.grid("typed")
    run.require(
        composer_text(grid) == "hello 한글",
        "what was typed is in the composer: %r" % composer_text(grid),
    )

    trial.send(b"\x7f")  # Backspace takes a whole grapheme, not a byte of one
    trial.wait_until(
        "the backspace to take the whole grapheme",
        lambda _t: composer_text(trial.peek()) == "hello 한",
    )
    grid = trial.grid("backspaced")
    run.require(
        composer_text(grid) == "hello 한",
        "Backspace removed the whole grapheme: %r" % composer_text(grid),
    )

    trial.send(b"\x01")  # C-a: home
    # The composer's own row, found up from the hint: the band floats under
    # the document, so "home" is the composer's first text cell wherever the
    # composer is, not a row counted from the bottom of the screen.
    trial.wait_until(
        "the caret to go home in the frame that painted the row",
        lambda text: composer_first_row(trial.peek()) is not None
        and (trial.session.last_frame() or "").endswith(
            "\x1b[%d;3H" % (composer_first_row(trial.peek()) + 1)
        ),
    )
    trial.send(b"\x04")  # C-d on text is a forward delete, not an exit
    trial.wait_until(
        "the forward delete to take the character under the caret",
        lambda _t: composer_text(trial.peek()) == "ello 한",
    )
    run.require(
        trial.session.state()[0] == "running",
        "Ctrl-D over text deleted forward rather than leaving",
    )
    trial.send(b"\x05\x15")  # C-e to the end, C-u to kill the line
    trial.wait_until("the composer to be empty", lambda _t: composer_text(trial.peek()) == "")
    trial.grid("killed")

    # A ZWJ family is one unit for motion **and** for deletion: the Left steps
    # over `y` and the Backspace then takes the whole family rather than the
    # last emoji of it.
    family = "\U0001f468‍\U0001f469‍\U0001f467"
    trial.send("x" + family + "y")
    trial.wait_until(
        "the family in the composer",
        lambda _t: composer_text(trial.peek()) == "x" + family + "y",
    )
    trial.grid("family")
    trial.send(b"\x1b[D")  # Left
    trial.send(b"\x7f")  # Backspace
    trial.wait_until(
        "the family to go as one unit", lambda _t: composer_text(trial.peek()) == "xy"
    )
    grid = trial.grid("after-family")
    run.require(composer_text(grid) == "xy", "a ZWJ family moved and died as one unit")
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    trial.send(b"\x15")
    discriminate(run, trial, fixture, marker)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "Ctrl-D on an empty composer leaves")
    fixture.stop()


# ---------------------------------------------------------------------------
# 6. soft wrap and the growth cap
# ---------------------------------------------------------------------------

WRAP_WORDS = ["alphaAA", "bravoBB", "charlCC", "deltaDD", "echoEEE", "foxtrFF", "golfGGG"]


def scenario_6(run):
    """Word-aware wrap with a hanging gutter, and a composer that stops growing."""
    marker = run.marker("wrap")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    trial = run.trial("wrap", gateway=fixture).settled()

    paragraph = " ".join(WRAP_WORDS * 4)
    trial.send(paragraph)
    trial.wait_until(
        "the whole paragraph in the composer",
        lambda _t: (composer_text(trial.peek()) or "").startswith(WRAP_WORDS[0]),
    )
    grid = trial.grid("wrapped")
    band = find_band(grid)
    run.require(band is not None, "the composer is on the screen")
    if band is not None:
        rows = composer_rows_text(grid)
        run.require(len(rows) > 1, "the paragraph really wrapped (%d row(s))" % len(rows))
        broken = [
            token
            for row in rows
            for token in row.split()
            if token not in WRAP_WORDS
        ]
        run.require(not broken, "no word was split across a row boundary: %r" % broken)
        # Restated for the fx composer (`a50f8a0`): the gutter a wrapped row
        # hangs in is the rail's -- `┃` and a space on **every** row -- where it
        # used to be `> ` on the first row and two spaces under it. Asserted on
        # the cells, so a continuation row that lost its rail (or started its
        # text in the rail's cell) fails.
        for row in band.composer:
            run.require(
                grid.cells[row][0] == RAIL and grid.cells[row][1] == " " and grid.cells[row][2] != " ",
                "composer row %d hangs its text behind the rail's two-cell gutter: %r"
                % (row + 1, grid.row_text(row)),
            )
    trial.send(b"\x15")
    discriminate(run, trial, fixture, marker)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the wrap session left cleanly")
    fixture.stop()

    # The cap: on a 12-row screen the content area a one-row composer leaves
    # is 9 rows, so a composer may take at most `9 / 2 + 1` = 5 rows
    # (`layout::input_row_limit`). Seventeen rows of draft, five of them shown.
    #
    # **Restated for the floating band** (`fcc8e44`): this used to read the cap
    # off a divider pinned to row 6 above a composer on the bottom rows. The
    # band now floats under the two-row document (the welcome line and its
    # blank row) and there is no always-drawn divider, so the cap is read off
    # the band itself -- exactly five rail rows, the first of them `┃↑`
    # because rows above it are scrolled out of the window -- and "did not
    # grow onto the document" is the welcome line and its blank row still
    # being the whole document above it.
    cap = 5
    capped = run.trial("growth-cap", rows=12, cols=20).settled()
    for _ in range(8):
        capped.send(b"0123456789012345678")
        capped.send(b"\x0a")  # C-j: a newline in the composer

    def at_the_cap(grid):
        band = find_band(grid)
        return band is not None and len(band.composer) == cap and composer_scrolled(grid)

    capped.wait_until("the band to reach its cap", lambda _t: at_the_cap(capped.peek()))
    # The ninth line is empty and the caret is on it: the frame that put it
    # there is the one this snapshot has to be taken behind.
    capped.wait_until(
        "the caret on the draft's last, empty line",
        lambda _t: (lambda g: at_the_cap(g) and g.row == find_band(g).composer[-1] and g.col == 2)(
            capped.peek()
        ),
    )
    grid = capped.grid("at-the-cap")
    band = find_band(grid)
    run.require(
        band is not None and len(band.composer) == cap,
        "the composer stopped growing at its cap of %d rows: %r" % (cap, band),
    )
    run.require(
        composer_scrolled(grid),
        "and its first row shown is `┃↑`, the rows above it scrolled away: %r"
        % (grid.row_text(band.composer[0]) if band else None,),
    )
    document = rows_above_band(grid, band) if band is not None else None
    run.require(
        is_fresh_document(document),
        "the composer did not grow past its cap onto document rows: %r" % (document,),
    )
    # Four rows of draft and the caret's own empty last line: eight `C-j`s
    # leave a ninth, empty logical line, and the window shows the *tail* of the
    # draft. Requiring five painted rows would be requiring that last line not
    # to exist.
    showing = [text for text in (composer_rows_text(grid) or []) if text.strip() != ""]
    run.require(
        len(showing) >= 4,
        "the composer window is showing the tail of the draft (%d row(s) of it)" % len(showing),
    )
    run.require(
        band is not None and band.composer[0] <= grid.row <= band.composer[-1],
        "and the caret is inside the composer's five rows (row %d)" % (grid.row + 1),
    )
    run.require(
        band is not None
        and band.divider == band.composer[-1] + 1
        and band.hint == band.divider + 1
        and hint_text(grid) != "",
        "and the blank rule row and the hint row are still right under it: %r" % (band,),
    )
    capped.send(b"\x0d")
    capped.send(b"\x04")
    run.require(capped.session.wait_exit() == ("exited", 0), "the capped session left cleanly")
    grid = capped.grid("submitted")
    # Read as the card the document echoes it in, by content: on a 20-column
    # screen each line wraps behind the rail, so the text is in the document
    # only once its card's rows are joined back together.
    run.require(
        any("0123456789012345678" * 8 in card for card in cards(grid)),
        "and what was submitted reached the terminal's own document, as one card: %r"
        % (cards(grid),),
    )


# ---------------------------------------------------------------------------
# 7. multiline and paste framing
# ---------------------------------------------------------------------------


def scenario_7(run):
    """One paste is one prompt, whatever is inside it -- and input behind a
    paste's end marker in the same read is no prompt at all."""
    marker = run.marker("paste")
    big = run.marker("bigpaste")
    suffix = run.marker("aftersuffix")
    fixture = start_fixture(
        run,
        [fixtures.content_only(marker), fixtures.content_only(big), fixtures.content_only(suffix)],
    )
    trial = run.trial("paste", gateway=fixture).settled()

    run.require("\x1b[?2004h" in trial.text(), "bracketed paste was enabled")

    trial.send(b"\x1b[200~")
    trial.send(("first " + run.nonce + "\nsecond \x03line\n\x1b[Athird line").encode("utf-8"))
    trial.send(b"\x1b[201~")
    # Both ends of the paste in one complete frame, and the composer's rows --
    # each behind the rail -- holding exactly its three lines: the first with
    # the nonce, the `0x03` filtered out of the second, the `ESC [A` of the
    # third kept as text rather than decoded as a key.
    pasted_rows = ["first " + run.nonce, "second line", "[Athird line"]
    trial.wait_until(
        "the whole paste in one complete frame",
        lambda _t: ("first " + run.nonce) in (trial.session.last_frame() or "")
        and "[Athird line" in (trial.session.last_frame() or "")
        and composer_rows_text(trial.peek()) == pasted_rows,
    )
    run.require(fixture.request_count() == 0, "a paste did not submit itself")
    grid = trial.grid("pasted")
    run.require(
        composer_rows_text(grid) == pasted_rows,
        "the composer holds the paste as three rows: %r" % (composer_rows_text(grid),),
    )
    run.require(
        grid.find("[Athird line") is not None,
        "the escape inside the paste is ordinary text on the screen, not a key",
    )

    trial.send(b"\x0d")
    trial.wait_for(marker)
    run.require(fixture.request_count() == 1, "one paste became exactly one prompt")
    # The **last** user message: a request carries the conversation's history,
    # so a comparison against the whole of it would be a claim about the
    # session rather than about the paste.
    sent = user_messages(fixture.requests()[0])[-1]
    run.require(run.nonce in sent, "the nonce this run minted is in the request xfx sent")
    run.require(
        sent == "first " + run.nonce + "\nsecond line\n[Athird line",
        "the body carries the whole pasted text, filtered and nothing else: %r" % sent,
    )
    run.require("\x1b" not in sent, "no escape byte reached the model as text")
    grid = trial.grid("answered")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(grid.text().strip() != "", "the screen is not blank")

    # Over a thousand codepoints collapses on screen and expands on submit.
    block = "y" * 900 + "\n" + "z" * 900
    trial.send(b"\x1b[200~")
    trial.send(block.encode("utf-8"))
    trial.send(b"\x1b[201~")
    trial.wait_until(
        "the paste to be summarized in the composer",
        lambda _t: composer_text(trial.peek()) == "[Pasted text #1, 2 lines]",
    )
    grid = trial.grid("collapsed")
    run.require(
        composer_text(grid) == "[Pasted text #1, 2 lines]",
        "a large paste is a summary on screen: %r" % composer_text(grid),
    )
    run.require("y" * 900 not in trial.text(), "1800 codepoints were not painted into the band")
    trial.send(b"\x0d")
    trial.wait_for(big)
    sent = user_messages(fixture.requests()[1])[-1]
    run.require(sent == block, "the collapsed block expanded verbatim on submit")
    run.require("Pasted text #1" not in sent, "the summary was not sent instead of the text")

    # -- a paste and an Enter behind its end marker, in one read -----------
    # fx v0.0.16's paste framing (9e0eab41; `paste_framing.zig:58-68,102-107,
    # 159-165`, `input_paste_runtime.zig:168-198@481104fd`): input that
    # arrives behind a paste's end marker before the terminal goes quiet is
    # taken for the clipboard's, not the user's -- clipboard text carrying
    # `ESC [201~` and a newline, written in one go, would otherwise submit the
    # composer as if typed (written with a pause between, it still can: see
    # docs/parity.md's `full-screen TUI` row). The paste is
    # kept, nothing behind it is read as a key, and fx warns once under
    # `input`. A draft is typed first, so "nothing was sent" is not an empty
    # composer that had nothing to send, and a later Enter of its own sends the
    # draft with the paste in it.
    draft = "kept draft " + run.nonce
    trial.send(draft)
    trial.wait_until(
        "the draft the paste lands on",
        lambda _t: composer_text(trial.peek()) == draft,
    )
    asked = fixture.request_count()
    trial.send(b"\x1b[200~ clipboard words\x1b[201~\r")
    on_the_grid(trial, PASTE_IGNORED, "the ignored input's notice")
    grid = trial.grid("paste-suffix")
    run.require(
        composer_text(grid) == draft + " clipboard words",
        "the paste was kept on the draft it fell on: %r" % composer_text(grid),
    )
    run.require(
        fixture.request_count() == asked,
        "the Enter behind the end marker sent nothing (%d requests, %d before)"
        % (fixture.request_count(), asked),
    )
    run.require(
        grid.text().count(PASTE_IGNORED) == 1, "the ignored input is said once, in fx's words"
    )
    trial.send(b"\x0d")
    trial.wait_for(suffix)
    run.require(
        fixture.request_count() == asked + 1
        and user_messages(fixture.requests()[asked])[-1] == draft + " clipboard words",
        "an Enter of its own sent the draft with the paste, and only it: %r"
        % (user_messages(fixture.requests()[-1])[-1:],),
    )

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the paste session left cleanly")
    fixture.stop()


# What input behind a paste's end marker leaves in the document
# (`src/tui/shell.rs`'s `PASTE_SUFFIX` under `input`, as a warning notice;
# `tests/tui.rs`'s `a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing`
# reads the same row), spelled out as every needle here is.
PASTE_IGNORED = "! input: Input that arrived right after the paste was ignored."


# ---------------------------------------------------------------------------
# 8. streaming render
# ---------------------------------------------------------------------------


def scenario_8(run):
    """A stream is released over frames, arrives whole, leaks no attribute, and
    stops on two Escapes as it does on a Ctrl-C."""
    head, tail = run.marker("head"), run.marker("tail")
    # One delta, held open, with the same three hundred characters between the
    # head and the tail -- **in paragraphs**, each ended by a blank line. The
    # answer is shown a line at a time once the presenter has seen what
    # follows it (`3efe232`: a prose line waits for the next one, which may
    # make it a setext heading), so one unbroken paragraph into a stream that
    # never closes would show nothing at all; paragraphs are what let the
    # pacer release the head while the tail is still inside the product.
    body = head + "\n\n" + "".join(("xx " * 10).rstrip() + "\n\n" for _ in range(10)) + tail + "\n\n"
    fixture = start_fixture(run, [fixtures.hang(fixtures.text_delta("a", body))])
    trial = run.trial("streaming", gateway=fixture).settled()

    trial.send("stream " + run.nonce + "\r")
    trial.wait_for(head)
    run.require(
        tail not in trial.text(),
        "the head of one delta was on the screen while its tail was not: the answer "
        "was released rather than dumped",
    )
    trial.wait_for(tail)
    run.require(
        any(run.nonce in captured for captured in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    # Settled rather than snapshotted: the count below is a claim about the
    # screen the user is left looking at.
    trial.send(b"\x03")
    stopped_on_the_grid(trial, "the first Ctrl-C to stop the stream")
    trial.send(b"\x03")
    run.require(trial.session.wait_exit() == ("exited", 130), "a second Ctrl-C left with 130")
    trial.session.settled_text()

    grid = trial.grid("final")
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(
        grid.text().count(tail) == 1,
        "the marker appears exactly once on the final grid (%d)" % grid.text().count(tail),
    )
    found = grid.find(tail)
    run.require(found is not None, "the fixture's own marker is rendered")
    if found is not None:
        row, column = found
        after = column + len(tail)
        # Plain in both halves of the pen: no colour **and** no text attribute,
        # now that a row may be bold or dim as well as coloured.
        run.require(
            after >= grid.cols or grid.is_plain(row, after),
            "the cell after the answer carries plain attributes (%r)"
            % (((grid.attrs[row][after], grid.styles[row][after]) if after < grid.cols else None),),
        )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    fixture.stop()

    # The ordinary case, and the one a user meets every turn: a stream that
    # ends by itself. `06-qa-harness.md` asks for the same property of it --
    # the marker on the final grid **exactly once** -- and it is a different
    # question from the one above, because a turn that concludes is a turn
    # whose band shrinks back over the document it was painting on.
    ordinary_marker = run.marker("ordinary")
    ordinary_fixture = start_fixture(
        run, [fixtures.content_only("answer: " + ordinary_marker)], name="ordinary"
    )
    ordinary = run.trial("ordinary-turn", gateway=ordinary_fixture).settled()
    ordinary_prompt = "say " + run.nonce
    discriminate(
        run,
        ordinary,
        ordinary_fixture,
        ordinary_marker,
        label="ordinary",
        prompt=ordinary_prompt,
    )
    ordinary.send(b"\x04")
    run.require(ordinary.session.wait_exit() == ("exited", 0), "the ordinary turn left at 0")
    ordinary.session.settled_text()
    grid = ordinary.grid("ordinary-final")
    run.require(
        grid.text().count(ordinary_marker) == 1,
        "a turn that ended by itself left the marker on the screen exactly once (%d)"
        % grid.text().count(ordinary_marker),
    )
    # And the whole screen, not only the marker. The exit clears from the
    # band's top row downward, so what is left is the document -- and counting
    # the marker alone is not enough: a stale, **truncated** copy of the
    # answer's row does not contain the marker and would be scored as a clean
    # screen.
    #
    # **Restated for the fx transcript** (`3efe232`, `ca044a1`): this row used
    # to count every non-blank row except the prompt's echo, which a starting
    # turn's activity row painted over on some targets and not others. The echo
    # is now the card, and the activity row no longer takes a row from the
    # document -- it sits under the card's own blank row, on a band that floats
    # down rather than up -- so the card is counted too, exactly once. What a
    # clean turn leaves is the welcome line, the card, the answer in its
    # two-column indent and the dim summary row, and nothing else: a second
    # answer, a truncated one, a lost card or a band row left behind fails.
    #
    # **And the session's own name, last** (`.prd/tui-phase4/receipts/c5`,
    # `quit`: the band's top row erased and `Continue session with: … --resume
    # <id>` on the row after it, upstream's `render.zig:801-802`): a session
    # that had a conversation names itself on the way out, Ctrl-D as much as
    # `/quit`. One row, after everything the turn left.
    remaining = [
        text for text in (grid.row_text(row) for row in range(grid.rows)) if text.strip()
    ]
    run.require(
        len(remaining) == 5
        and is_welcome(remaining[0])
        and remaining[1] == RAIL + " " + ordinary_prompt
        and remaining[2] == "  answer: " + ordinary_marker
        and SUMMARY_ROW.match(remaining[3]) is not None
        and RESUME_HANDOFF.match(remaining[4]) is not None,
        "a completed turn left the welcome line, its card, its answer once, its summary and "
        "the session's name: %r" % (remaining,),
    )
    ordinary_fixture.stop()

    # -- Escape, twice, over a stream still arriving ----------------------
    # fx's own stop key (`app_input_runtime.zig:3139-3143`; `src/tui/gesture.rs`'s
    # `INTERRUPT_WINDOW`, upstream's one second): the first Escape over running
    # work stops nothing and says on the hint row that a second would, and the
    # second inside the window stops the turn -- the cancelled line in the
    # document, nothing more asked of the model, and the session still there
    # for the next prompt. The row above stops a stream with Ctrl-C; this is
    # the key a user reaches for first.
    escaped_head = run.marker("escapedhead")
    after_escape = run.marker("afterescape")
    escape_fixture = start_fixture(
        run,
        [
            fixtures.hang(fixtures.text_delta("e", escaped_head + "\n\n")),
            fixtures.content_only(after_escape),
        ],
        name="escape",
    )
    escaped = run.trial("escape-cancel", gateway=escape_fixture).settled()
    escaped.send("stream " + run.nonce + "\r")
    on_the_grid(escaped, escaped_head, "the head of the stream Escape is about to stop")
    escaped.send(b"\x1b")
    escaped.wait_until(
        "the first Escape to arm the interrupt",
        lambda _t: interrupt_armed(hint_text(escaped.peek())),
    )
    grid = escaped.grid("interrupt-armed")
    run.require(
        grid.find(CANCELLED_TURN) is None and grid.find(escaped_head) is not None,
        "the first Escape stopped nothing: the stream's head is up and no cancelled line",
    )
    escaped.send(b"\x1b")
    on_the_grid(escaped, CANCELLED_TURN, "the second Escape's cancelled line")
    grid = escaped.grid("cancelled")
    run.require(
        not interrupt_armed(hint_text(grid)) and hint_text(grid) != EXIT_ARMED,
        "the second Escape was the stop, not another arm or Ctrl-C's exit gesture: %r"
        % hint_text(grid),
    )
    run.require(
        escape_fixture.request_count() == 1,
        "the cancel asked the model for nothing more (%d requests)"
        % escape_fixture.request_count(),
    )
    run.require(
        any(run.nonce in body for body in escape_fixture.bodies()),
        "the nonce this run minted is in the request the Escapes stopped",
    )
    escaped.send("after " + run.nonce + "\r")
    on_the_grid(escaped, after_escape, "the next prompt's answer after the cancel")
    run.require(
        escape_fixture.request_count() == 2
        and user_messages(escape_fixture.requests()[1])[-1] == "after " + run.nonce,
        "the session went on to the next prompt after the Escapes stopped the turn",
    )
    escaped.send(b"\x04")
    run.require(
        escaped.session.wait_exit() == ("exited", 0),
        "a session whose turn Escape stopped leaves at 0",
    )
    run.require(escaped.modes() == escaped.before, "escape-cancel: termios byte-identical")
    escape_fixture.stop()


# What the hint row's right-hand slot says while a second Escape would stop the
# work in hand, widest first -- the first that fits is the one shown -- and the
# whole row when none does (`src/tui/shell.rs`'s `INTERRUPT_ARMED` and
# `INTERRUPT_ARMED_ALONE`; `input_presentation.zig:510-525`).
INTERRUPT_ARMED = ("esc again to interrupt", "esc esc interrupt", "esc esc")
INTERRUPT_ARMED_ALONE = "esc esc to interrupt"


def interrupt_armed(hint):
    """Whether `hint` (a hint row's text) says a second Escape would interrupt."""
    if hint is None:
        return False
    return hint == INTERRUPT_ARMED_ALONE or any(hint.endswith(cue) for cue in INTERRUPT_ARMED)


# ---------------------------------------------------------------------------
# 9. the activity row
# ---------------------------------------------------------------------------


# The lit face of the activity row's marker (`src/tui/activity.rs:55`). Spelled
# out here rather than imported for the reason the mode sequences are: a
# harness that read the constant it is checking would pass for whatever the
# module declared.
activity_marker = "\u2022"


def scenario_9(run):
    """`Thinking` while the fixture withholds, and a clock that stops for a person."""
    quiet = start_fixture(run, [fixtures.hang()], name="quiet")
    trial = run.trial("thinking", gateway=quiet).settled()
    trial.send("think about " + run.nonce + "\r")
    # Response-only **and** positional, read off the grid: the row the band
    # found **as its activity row** -- above the composer, over the blank row
    # that keeps it from the draft -- saying `Thinking`. A needle matched
    # anywhere on the wire would be satisfied by the word appearing in the
    # document; this one is satisfied only by the band's own row.
    #
    # **Restated for the fx activity row** (`ca044a1`): the row used to be
    # asserted on screen row 21 above an always-drawn divider on row 22. The
    # band now floats and has no such divider, so the place is the band's
    # structure instead: the document's own blank row above it (the card's),
    # one blank row under it, then the composer.
    trial.wait_until(
        "the activity row to say Thinking",
        lambda _t: (activity_text(trial.peek()) or "")[2:].startswith("Thinking"),
    )
    # Off the activity row rather than off the wire: a second that ticked over
    # writes the one digit that changed, so the wire never carries `2s` whole.
    trial.wait_until(
        "the activity row's clock to reach two seconds",
        lambda _t: (activity_seconds(trial.peek()) or 0) >= 2,
    )
    grid = trial.grid("thinking")
    band = find_band(grid)
    # By its place, its words and its clock -- and by whichever of its two faces
    # the marker is wearing, because a snapshot cannot choose. The marker keeps
    # a cell of its own whether it is lit or dark (`src/tui/activity.rs`: `•`
    # for half a second and then a space for half a second, so the text beside
    # it does not step left and right twice a second), so a grid catches
    # `• Thinking (2s) (↑12 ↓0)` half the time and `  Thinking (2s) …` the
    # other half. `ACTIVITY_ROW` matches both faces and nothing else.
    row = activity_text(grid) or ""
    run.require(
        band is not None
        and band.activity is not None
        and row[:1] in (activity_marker, " ")
        and row[2:].startswith("Thinking (")
        and (activity_seconds(grid) or 0) >= 2,
        "the activity row is the band's own, marked, and counting: %r" % row,
    )
    run.require(
        band is not None
        and band.activity is not None
        and band.activity >= 1
        and grid.row_text(band.activity - 1) == ""
        and grid.row_text(band.activity + 1) == ""
        and band.composer[0] == band.activity + 2,
        "and it sits under the document's blank row with one blank row between it and the "
        "composer: %r" % (band,),
    )
    activity_row = band.activity if band is not None and band.activity is not None else 0
    label_end = row.find(" (\u2191")
    label_end = len(row) if label_end < 0 else label_end
    # The row's own literal foreground (read off the cells, not a string
    # search: what the palette *is* belongs to `src/tui/theme.rs`, and a check
    # against that module's own accessor would pass for whatever it happened
    # to declare) -- the marker, the phase and the clock in `252`, and the
    # token suffix receding into the dim grey `245` beside them (`ca044a1`,
    # `shimmer_runtime.zig:262-291`).
    run.require(
        {grid.attrs[activity_row][col] for col in range(2, label_end)} == {"\x1b[38;5;252m"},
        "the activity row is not painted in the dark palette's literal foreground: %r"
        % grid.attrs_of_row(activity_row),
    )
    run.require(
        label_end < len(row)
        and {grid.attrs[activity_row][col] for col in range(label_end + 1, len(row))}
        == {"\x1b[38;5;245m"},
        "and its token suffix is in the dark palette's dim grey: %r" % grid.attrs_of_row(activity_row),
    )
    # And none of it past the row: the blank row under it and the composer's
    # own cells after the rail carry no colour and no attribute (upstream's
    # `input_bar_style` is empty, `render.zig:69,88`), so a leaked foreground
    # would show up there as this row's grey rather than as nothing.
    composer_row = band.composer[0] if band is not None else 0
    leaked = [
        (r + 1, c + 1)
        for r in (activity_row + 1, composer_row)
        for c in range(1 if r == composer_row else 0, grid.cols)
        if not grid.is_plain(r, c)
    ]
    run.require(
        not leaked,
        "the activity row's colour leaked past it onto the gap or the composer: %r" % leaked[:8],
    )
    run.require(
        any(run.nonce in body for body in quiet.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.text().strip() != "", "the screen is not blank")
    trial.send(b"\x03\x03")
    run.require(trial.session.wait_exit() == ("exited", 130), "the quiet turn was interruptible")
    quiet.stop()

    # The dark index above is only half of what upstream pins: the light row
    # is `238` (`render.zig:55,86,105`), and a role that only matched one of
    # the two palettes would leave a light terminal reading
    # `permission_auto_style`'s dark grey. Fixed by environment rather than by
    # answering the background query, the same way scenario 12's
    # `fixed-by-environment` case is -- the palette lever moved and nothing
    # else.
    bright = start_fixture(run, [fixtures.hang()], name="bright")
    lit = run.trial(
        "thinking-light", gateway=bright, env_extra={"XFX_THEME": "light"}, answer_probes=False
    )
    lit.wait_for(pty.PROBE)
    lit.send("\x1b[2;1R")
    lit.settled()
    lit.send("think about " + run.nonce + "\r")
    # A raw `wait_for` only proves the needle reached the wire, not that a
    # whole frame carrying it is complete -- `peek`/`grid` are fed only as
    # far as the last **complete** frame (`Trial.peek`, above), so a grid
    # built right after a raw match can still read the *previous* complete
    # frame: a screen from before the row was painted. Waited for on the
    # grid itself instead, the same way the dark half above already is --
    # its clock-tick `wait_until` forces a complete frame to exist before
    # its own `grid()` call, and this is that same discipline for the case
    # that has no clock tick to wait on.
    def light_label(grid):
        band = find_band(grid)
        if band is None or band.activity is None:
            return None
        text = grid.row_text(band.activity)
        end = text.find(" (\u2191")
        end = len(text) if end < 0 else end
        return {grid.attrs[band.activity][col] for col in range(2, end)}

    lit.wait_until(
        "a completed frame carrying the activity row in the light palette",
        lambda _t: light_label(lit.peek()) == {"\x1b[38;5;238m"}
        and (activity_text(lit.peek()) or "")[2:].startswith("Thinking"),
    )
    grid_light = lit.grid("thinking-light")
    run.require(
        light_label(grid_light) == {"\x1b[38;5;238m"},
        "the activity row is not painted in the light palette's literal foreground: %r"
        % (light_label(grid_light),),
    )
    band_light = find_band(grid_light)
    leaked_light = [
        (r + 1, c + 1)
        for r in ((band_light.activity + 1, band_light.composer[0]) if band_light else ())
        for c in range(1 if r == band_light.composer[0] else 0, grid_light.cols)
        if not grid_light.is_plain(r, c)
    ]
    run.require(
        band_light is not None and not leaked_light,
        "the activity row's colour leaked past it onto the gap or the composer: %r"
        % leaked_light[:8],
    )
    lit.send(b"\x03\x03")
    run.require(
        lit.session.wait_exit() == ("exited", 130), "the light-palette turn was interruptible"
    )
    bright.stop()

    # And the clock stops while xfx is waiting to be told what it may do,
    # because that interval measures the person rather than the model.
    #
    # **Restated for fx's footer** (`render_input.zig:705`;
    # `.prd/tui-phase4/receipts/f1`, "The activity row is off the band while an
    # approval is up"; `src/tui/shell.rs`'s band settle, "the row comes back, at
    # the elapsed time it stopped at, when the question goes"). This row used to
    # read the clock twice beside the question and require the same number.
    # There is no row beside a question now, so the stop is read from the
    # number the row comes back with: the turn is held at both ends of the
    # question by the fixture (`release_when`), the clock is read before the
    # question is let through and again once the answer has given the turn its
    # row back, and the question is held up for `QUESTION_HOLD` in between. A
    # clock that ran through the wait comes back at least that much later; one
    # that stopped comes back within the time the two releases themselves took.
    marker = run.marker("frozen")
    asked, answered = threading.Event(), threading.Event()
    script = fixtures.edit_then_finish(marker)
    script[1]["release_when"] = asked
    script[2]["release_when"] = answered
    fixture = start_fixture(run, script, name="frozen")
    frozen = run.trial("frozen-clock", gateway=fixture, mode="ask", notes=True).settled()
    frozen.send("edit the notes " + run.nonce + "\r")
    frozen.wait_until(
        "the turn to be held before its question, its clock past two seconds",
        lambda _t: fixture.request_count() == 2 and (activity_seconds(frozen.peek()) or 0) >= 2,
    )
    before = activity_seconds(frozen.grid("held-before-the-question"))
    asked.set()
    grid = wait_for_panel(frozen, PERMISSION_TITLE)
    up = time.time()
    frozen.grid("question-up")
    band = find_band(grid)
    run.require(
        band is not None and band.activity is None and band.composer == [],
        "the question has the footer to itself -- no activity row, no composer: %r" % (band,),
    )
    while time.time() - up < QUESTION_HOLD:
        frozen.session.pump(0.1)
    run.require(
        find_band(frozen.peek()) is not None
        and panel_rows_text(frozen.peek()) is not None
        and any(PERMISSION_TITLE in text for text in panel_rows_text(frozen.peek())),
        "the question was still up after %.0fs" % QUESTION_HOLD,
    )
    waited = time.time() - up
    frozen.send(b"3")
    frozen.wait_until(
        "the turn's row to come back once the question is answered",
        lambda _t: activity_seconds(frozen.peek()) is not None,
    )
    after = activity_seconds(frozen.grid("row-back-after-the-answer"))
    run.require(
        before is not None
        and after is not None
        and before <= after < before + QUESTION_HOLD - 2,
        "the clock stopped while xfx was waiting for a decision: %r before a question held "
        "up %.1fs, %r after it" % (before, waited, after),
    )
    answered.set()
    frozen.wait_for(marker)
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce reached the provider in the request xfx sent",
    )
    grid = frozen.grid("answered")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    frozen.send(b"\x04")
    run.require(frozen.session.wait_exit() == ("exited", 0), "the frozen-clock session left at 0")
    fixture.stop()


# How long scenario 9 keeps a question up between its two readings of the
# clock. Long enough that a clock which ran through the wait comes back clear of
# the time the two releases around it take on a loaded runner (a second or two
# between them), and short enough not to dominate the suite.
QUESTION_HOLD = 8.0


# ---------------------------------------------------------------------------
# 10. the approval panel
# ---------------------------------------------------------------------------


def scenario_10(run):
    """Every way of answering a question, and the outcome the fixture then sees.

    **Restated for fx's approvals** (`.prd/tui-phase4/receipts/f2`, F1's tool
    group). The panel's words are upstream's -- `❯ 1  Apply once`, xfx's own
    grant in the second, `3  Don't apply` -- and two of the keys mean what they
    mean there: **Escape cancels the turn** ("Both gateway logs end at the
    request that asked") where it used to refuse the call and let the turn go
    on, and **Ctrl-C refuses only the call, and the turn goes on** where it used
    to stop the turn as well. Each trial below still proves what its key does
    to the call and to the turn, by the file and by what the fixture is asked
    next; the two claims that changed are replaced by fx's, for the same keys.
    A refusal reaches the user as the call's own row in the tool group
    (`Denied file`, `Cancelled file`) where it used to be an `edit_file
    refused` line.
    """
    marker = run.marker("panel")

    def ask(label):
        fixture = start_fixture(run, fixtures.edit_then_finish(marker), name=label)
        trial = run.trial(label, gateway=fixture, mode="ask", notes=True).settled()
        trial.send("edit the notes " + run.nonce + "\r")
        trial.wait_for(PERMISSION_TITLE)
        wait_for_panel(trial, PERMISSION_TITLE)
        return fixture, trial

    # `1`: yes, once. This is also the scenario's discriminator.
    fixture, yes = ask("answer-yes")
    grid = yes.grid("panel")
    run.require(grid.find(PERMISSION_TITLE) is not None, "the panel names itself")
    run.require(grid.find(APPLY_ONCE) is not None, "the first choice is offered")
    run.require(grid.find(ALWAYS_WORDING) is not None, "the second says exactly what it grants")
    run.require(grid.find(DONT_APPLY) is not None, "the third refuses")
    run.require(
        grid.find("for the rest of this session") is not None,
        "and the disclosure the line shell's prompt makes is made here too",
    )
    run.require(caret_on(yes, APPLY_ONCE), "the marker sits on the choice Enter would take")
    run.require(
        grid.find(APPLY_ONCE) is not None and grid.row == grid.find(APPLY_ONCE)[0],
        "and so does the terminal's caret (row %d)" % (grid.row + 1),
    )
    run.require(
        read(yes.notes) == "alpha\n", "the edit did not run before it was approved"
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    yes.send(b"1")
    yes.wait_for(marker)
    run.require(read(yes.notes) == "beta\n", "`1` let the edit through")
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    grid = yes.grid("answered")
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(
        grid.find("Edited notes.txt +1 / -1") is not None,
        "the edit's row in the tool group says what it did: %r" % grid.text(),
    )
    run.require(
        any("beta" in body for body in fixture.bodies()),
        "and the fixture saw the tool result the choice implies",
    )
    yes.send(b"\x04")
    run.require(yes.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()

    # `2`: yes, and stop asking for this file.
    fixture, always = ask("answer-always")
    always.send(b"2")
    always.wait_for(marker)
    run.require(read(always.notes) == "beta\n", "`2` let the edit through as well")
    always.send(b"\x04")
    always.session.wait_exit()
    fixture.stop()

    # `3`: no.
    fixture, no = ask("answer-no")
    no.send(b"3")
    no.wait_for(marker)
    run.require(read(no.notes) == "alpha\n", "`3` left the file alone")
    grid = no.grid("refused")
    run.require(
        grid.find("Denied file") is not None,
        "and the refusal reached the user as the call's own row rather than being silent: %r"
        % grid.text(),
    )
    refusal = tool_result_text(amended_request(fixture, "call-1") or "{}", "call-1")
    run.require(
        refusal is not None and "not permitted" in refusal,
        "and the model was told the call was refused: %r" % refusal,
    )
    no.send(b"\x04")
    no.session.wait_exit()
    fixture.stop()

    # The arrows and Enter: two steps down is the refusal, and taking it with
    # Enter is a different key from typing `3`.
    fixture, arrows = ask("answer-arrows")
    arrows.send(b"\x1b[B\x1b[B")
    arrows.wait_until("the marker to move to the third choice", lambda _t: caret_on(arrows, DONT_APPLY))
    arrows.grid("caret")
    arrows.send(b"\r")
    arrows.wait_for(marker)
    run.require(read(arrows.notes) == "alpha\n", "the arrows chose, and Enter took the choice")
    arrows.send(b"\x04")
    arrows.session.wait_exit()
    fixture.stop()

    # Esc refuses the call **and cancels the turn** behind it, as upstream's
    # does: the call's row says it was cancelled, the turn's cancelled line is
    # under it, and nothing more is asked of the model.
    fixture, escaped = ask("answer-escape")
    escaped.send(b"\x1b")
    grid = on_the_grid(escaped, "▪ Cancelled file", "the escaped call's cancelled line")
    escaped.grid("escaped")
    run.require(read(escaped.notes) == "alpha\n", "Esc refused the call")
    run.require(
        grid.find("Cancelled file") is not None
        and grid.find("▪ Cancelled file · What can xfx do differently?") is not None,
        "the call's row and the turn's cancelled line say the turn was stopped: %r" % grid.text(),
    )
    run.require(
        escaped.session.state()[0] == "running", "and the session it belonged to is still up"
    )
    escaped.send(b"\x04")
    run.require(escaped.session.wait_exit() == ("exited", 0), "the escaped session left at 0")
    run.require(
        fixture.request_count() == 2,
        "the cancelled turn asked the model for nothing more (%d requests)" % fixture.request_count(),
    )
    run.require(
        marker not in escaped.session.settled_text(),
        "the cancelled turn never reached its own conclusion",
    )
    fixture.stop()

    # Ctrl-C at the question refuses **the call**, and the turn goes on to its
    # own conclusion -- upstream's key here, and not the interrupt it is over a
    # running answer.
    fixture, cancelled = ask("answer-ctrl-c")
    cancelled.send(b"\x03")
    cancelled.wait_for(marker)
    run.require(read(cancelled.notes) == "alpha\n", "Ctrl-C refused the call")
    grid = cancelled.grid("refused-by-ctrl-c")
    run.require(
        grid.find("Denied file") is not None and grid.find(marker) is not None,
        "the call was denied and the turn went on to its conclusion: %r" % grid.text(),
    )
    run.require(
        hint_text(grid) != EXIT_ARMED,
        "a Ctrl-C that answered a question armed the exit as well: %r" % hint_text(grid),
    )
    cancelled.send(b"\x04")
    run.require(
        cancelled.session.wait_exit() == ("exited", 0), "the refused session left at 0"
    )
    fixture.stop()


def caret_on(trial, choice):
    """Whether the panel's marker sits on `choice` in the grid as it stands."""
    grid = trial.peek()
    found = grid.find(choice)
    return found is not None and (CHOICE_MARK + choice) in grid.row_text(found[0])


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


# ---------------------------------------------------------------------------
# 10b. an approval mid-turn does not deadlock
# ---------------------------------------------------------------------------


# What wraps a prompt sent into a running turn, on the wire
# (`.prd/tui-phase4/receipts/f4`, `interrupt-steer`:
# `user <user_steering>…what are you doing…</user_steering>`).
STEERING_OPEN = "<user_steering>"
STEERING_CLOSE = "</user_steering>"


def steered_text(message):
    """The prompt a `<user_steering>` user message carries, or `None` for any other message."""
    if message.get("role") != "user":
        return None
    for part in message.get("content", []):
        text = part.get("text", "") if part.get("type") == "text" else ""
        if text.startswith(STEERING_OPEN) and text.rstrip().endswith(STEERING_CLOSE):
            body = text[len(STEERING_OPEN) : text.rstrip().rindex(STEERING_CLOSE)]
            return body.strip().split("\n")[-1]
    return None


def scenario_10b(run):
    """A prompt sent mid-turn steers it, and a question asked after it is still answered.

    **Restated for fx's steering** (`.prd/tui-phase4/receipts/f4`, round 2: "a
    prompt typed mid-turn steers it at fx's boundaries", "no `queued N`";
    `src/tui/shell.rs`'s `send`, `admitInteractivePrompt`,
    `worker_runtime.zig:1080-1104`). This row used to queue a prompt behind a
    running turn, answer a question with it waiting, and then refuse a third on
    the hint row because one was already queued. Neither state exists in fx: a
    prompt typed while a turn runs goes **into** that turn at its next boundary
    -- at once over a pending answer -- so there is nothing queued for an answer
    to be stuck behind, and no queue for a third prompt to overflow.

    What the row proves is kept in fx's terms. The deadlock it guarded against
    was an answer that could not reach the runtime past a prompt the session
    had already sent it; so a prompt is sent into a turn held mid-step, the
    steered turn then asks a question, and the answer still lands -- the
    steering and the answer arrive on their own channels, in the order the
    wire records. And where the third prompt was refused, a prompt typed over a
    streaming answer is shown to steer it at once: the answer is cut where it
    stood and the next request carries the cut answer and the prompt.
    """
    marker = run.marker("midturn")
    steer = "queued while deciding"
    held = threading.Event()
    edit = {"path": "notes.txt", "old_string": "alpha", "new_string": "beta"}
    script = [
        fixtures.edit_then_finish(marker)[0],
        # The step the steer cuts: its request is recorded and its reply held,
        # so the turn is provably waiting on the model when the prompt arrives.
        {
            "events": [fixtures.tool_call("call-1", "edit_file", edit), fixtures.finish("tool-calls")],
            "release_when": held,
        },
        # What the steered turn asks for: the same edit, so the question it
        # raises is the one this row answers.
        {"events": [fixtures.tool_call("call-2", "edit_file", edit), fixtures.finish("tool-calls")]},
        fixtures.content_only(marker),
    ]
    fixture = start_fixture(run, script)
    trial = run.trial("mid-turn", gateway=fixture, mode="ask", notes=True).settled()

    trial.send("edit the notes " + run.nonce + "\r")
    trial.wait_until(
        "the turn to be held on the model, mid-step",
        lambda _t: fixture.request_count() == 2,
    )
    trial.send(steer + "\r")
    grid = wait_for_panel(trial, PERMISSION_TITLE, "the steered turn's question")
    # The handler holding the cut request is let go; its reply has no reader.
    held.set()
    trial.grid("question-after-the-steer")
    run.require(
        steer in cards(grid),
        "the prompt sent mid-turn was taken into the turn, its card in the document: %r"
        % (cards(grid),),
    )
    run.require(read(trial.notes) == "alpha\n", "the edit ran before it was approved")

    trial.send(b"1")
    trial.wait_for(marker)
    run.require(
        read(trial.notes) == "beta\n",
        "the answer reached the runtime past the prompt the session had sent it",
    )
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    bodies = fixture.bodies()
    run.require(
        len(bodies) == 4,
        "the cut step, the steered one and the answered one, and nothing else (%d requests)"
        % len(bodies),
    )
    steered = json.loads(bodies[2]).get("prompt", []) if len(bodies) > 2 else []
    run.require(
        bool(steered)
        and steered_text(steered[-1]) == steer
        and tool_result_text(bodies[2], "call-0") is not None
        and tool_result_text(bodies[2], "call-1") is None,
        "the steer reached the model behind the read it followed, in place of the cut step: %r"
        % (message_parts(bodies[2]) if len(bodies) > 2 else None,),
    )
    # The result's own words are the tool's (`edited notes.txt (5 bytes)`, fx's
    # since D1 round 6); what this row owns is that the answer's result is the
    # one the next request carries, and that it is not a refusal.
    answered_result = tool_result_text(bodies[3], "call-2") if len(bodies) > 3 else None
    run.require(
        answered_result is not None and "not permitted" not in answered_result,
        "and the answered edit's result came after it: %r"
        % (message_parts(bodies[3]) if len(bodies) > 3 else None,),
    )
    grid = trial.grid("answered")
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()

    # A prompt typed over a streaming answer steers it at once: no refusal,
    # nothing queued, and the answer is cut where it stood. A paragraph ended by
    # a blank line, so the presenter writes it while the stream is still held
    # open (`3efe232`: a prose line is shown once the presenter has seen what
    # follows it).
    steered_marker = run.marker("steered")
    running = start_fixture(
        run,
        [
            fixtures.hang(fixtures.text_delta("d", "FIRST-TURN-RUNNING\n\n")),
            fixtures.content_only(steered_marker),
        ],
        name="steer",
    )
    over = run.trial("steered-over-the-answer", gateway=running).settled()
    first = "first " + run.nonce
    over.send(first + "\r")
    over.wait_for("FIRST-TURN-RUNNING")
    over.send("second\r")
    grid = on_the_grid(over, steered_marker, "the steered turn's own answer")
    grid = over.grid("steered")
    run.require(
        cards(grid) == [first, "second"],
        "the prompt typed over the answer was written as the turn took it: %r" % (cards(grid),),
    )
    run.require(
        QUEUE_REJECTED not in (hint_text(grid) or "") and QUEUE_REJECTED not in over.text(),
        "a prompt typed over a running turn was refused rather than steering it",
    )
    bodies = running.bodies()
    run.require(
        len(bodies) == 2,
        "the steer is one request, made at once over the answer (%d requests)" % len(bodies),
    )
    prompt = json.loads(bodies[1]).get("prompt", []) if len(bodies) > 1 else []
    said = [message for message in prompt if message.get("role") != "system"]
    run.require(
        len(said) == 3
        and user_messages(running.requests()[1])[0] == first
        and said[1].get("role") == "assistant"
        and said[1]["content"][0].get("text", "").strip() == "FIRST-TURN-RUNNING"
        and steered_text(said[2]) == "second",
        "the next request carries the cut answer and the steer behind it: %r" % (said,),
    )
    run.require(grid.find(steered_marker) is not None, "the fixture's own marker is rendered")
    over.send(b"\x04")
    run.require(over.session.wait_exit() == ("exited", 0), "the steered session left at 0")
    running.stop()


# What the hint row says when a submission arrives with the queue already full
# (`src/tui/shell.rs`'s `QUEUE_REJECTED`), spelled out to assert its absence.
QUEUE_REJECTED = "one prompt is already queued"


# ---------------------------------------------------------------------------
# 11. Ctrl-C as a byte
# ---------------------------------------------------------------------------


def scenario_11(run):
    """`0x03` is a keystroke here, and no signal is delivered by it."""
    marker = run.marker("interrupt")
    # Ended by a blank line, so the presenter writes it while the stream is
    # still held open (`3efe232`: a prose line is shown once the presenter has
    # seen what follows it) and the turn is still running when Ctrl-C lands.
    fixture = start_fixture(run, [fixtures.hang(fixtures.text_delta("a", marker + "\n\n"))])
    trial = run.trial("ctrl-c", gateway=fixture).settled()

    trial.send("start something long " + run.nonce + "\r")
    trial.wait_for(marker)
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    grid = trial.grid("streaming")
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")

    trial.send(b"\x03")
    # The turn's cancelled line and the exit armed on the hint row: the byte was
    # read as the key it is (`.prd/tui-phase4/receipts/f4`, `cc-0.3`).
    stopped_on_the_grid(trial, "the first Ctrl-C to stop the turn")
    trial.grid("stopped")
    # No signal was delivered, so the child is still alive and the byte did the
    # work. Read without consuming anything, so asking does not change the
    # answer for the exit below.
    run.require(
        trial.session.state()[0] == "running",
        "a typed Ctrl-C delivered no signal: the process is still alive",
    )
    run.require(trial.modes().is_raw(), "and the terminal is still the session's")

    trial.send(b"\x03")
    run.require(trial.session.wait_exit() == ("exited", 130), "a second Ctrl-C leaves with 130")
    run.require(trial.modes() == trial.before, "and the terminal came back byte for byte")
    run.require(
        pty.RESTORE in trial.session.settled_text(), "through the restore sequence, in order"
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 12. theme detection
# ---------------------------------------------------------------------------


def scenario_12(run):
    """The background is asked for, and two answers paint two different bands."""
    marker = run.marker("theme")

    # The two band rows the palette paints, found up from the hint: the hint
    # row itself (the statusline grey with the mode in its accent) and the
    # composer's rail (the hint grey). **Restated for the fx band**
    # (`a50f8a0`): the second used to be the divider, which is blank now until
    # a panel is up, so it would compare two empty runs and prove nothing; the
    # rail is the row that carries the palette in its place.
    def band_attributes(trial):
        grid = trial.grid("band")
        band = find_band(grid)
        if band is None:
            return ([], [])
        return (
            grid.attrs_of_row(band.hint),
            grid.attrs_of_row(band.composer[0]),
        )

    # A light terminal.
    light_fixture = start_fixture(run, [fixtures.content_only(marker)], name="light")
    light = run.trial("light", gateway=light_fixture, answer_probes=False)
    light.wait_for(pty.THEME_PROBE)
    run.require(pty.THEME_PROBE in light.text(), "the launch asked the terminal its background")
    light.send("\x1b]11;rgb:ffff/ffff/ffff\x1b\\\x1b[2;1R")
    light.settled()
    light_attrs = band_attributes(light)
    discriminate(run, light, light_fixture, marker)
    light.send(b"\x04")
    run.require(light.session.wait_exit() == ("exited", 0), "the light session left at 0")
    light_fixture.stop()

    # A dark one.
    dark = run.trial("dark", answer_probes=False)
    dark.wait_for(pty.THEME_PROBE)
    dark.send("\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[2;1R")
    dark.settled()
    dark_attrs = band_attributes(dark)
    run.require(
        light_attrs[0] and dark_attrs[0] and light_attrs[1] and dark_attrs[1],
        "the band painted colour at all, so the comparison below can fail for the "
        "reason it claims (%r / %r)" % (light_attrs, dark_attrs),
    )
    run.require(
        light_attrs != dark_attrs,
        "a light and a dark answer selected different cell attributes (%r vs %r)"
        % (light_attrs, dark_attrs),
    )
    dark.send(b"\x04")
    run.require(dark.session.wait_exit() == ("exited", 0), "the dark session left at 0")

    # A reply whose **body** is malformed, arriving in one read with other
    # bytes. `theme::REPLY_PREFIX` matches the envelope and not the body on
    # purpose: a terminal that answered `11` with something unparseable has
    # still answered, the bytes of that answer are not keystrokes, and the
    # detection falls back to `COLORFGBG` and then to its conservative default.
    # The `k` typed in the same write is what proves the two halves are told
    # apart -- it is deferred and delivered to the composer, in order, while
    # the malformed reply is consumed.
    malformed = run.trial("malformed-reply", answer_probes=False)
    malformed.wait_for(pty.THEME_PROBE)
    malformed.send("\x1b]11;NOT-A-COLOUR\x1b\\\x1b[2;1Rk")
    malformed.settled()
    malformed.wait_until(
        "the deferred keystroke to reach the composer",
        lambda _t: composer_text(malformed.peek()) == "k",
    )
    malformed_attrs = band_attributes(malformed)
    run.require(
        malformed_attrs == dark_attrs,
        "a malformed body falls back rather than being adopted (%r vs the dark %r)"
        % (malformed_attrs, dark_attrs),
    )
    grid = malformed.grid("composer")
    run.require(
        composer_text(grid) == "k",
        "the malformed reply was consumed as an answer rather than typed, and the "
        "keystroke behind it was not: %r" % composer_text(grid),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    malformed.send(b"\x15\x04")
    run.require(
        malformed.session.wait_exit() == ("exited", 0),
        "a session answered with nonsense still leaves cleanly",
    )

    # A conversation that is not this one's: an `OSC 10` reporting the
    # foreground is handed back rather than eaten as the background, and the
    # session neither obeys it nor loses its place. What the handed-back bytes
    # then *become* is the input decoder's business and is asserted no further
    # here -- see the report's open findings.
    foreign = run.trial("foreign-osc", answer_probes=False)
    foreign.wait_for(pty.THEME_PROBE)
    foreign.send("\x1b]10;rgb:1111/2222/3333\x1b\\\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[2;1R")
    foreign.settled()
    run.require(
        band_attributes(foreign) == dark_attrs,
        "the background reply behind the foreign one was still the one adopted",
    )
    grid = foreign.grid("after-a-foreign-osc")
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    run.require(
        find_band(grid) is not None and hint_text(grid) != "",
        "the session did not lose its place: the band is still painted",
    )
    foreign.send(b"\x15\x04")
    run.require(
        foreign.session.wait_exit() == ("exited", 0),
        "and it still leaves cleanly on Ctrl-D",
    )

    # And a session told what the palette is asks nothing.
    fixed = run.trial("fixed-by-environment", env_extra={"XFX_THEME": "dark"}, answer_probes=False)
    fixed.wait_for(pty.PROBE)
    fixed.send("\x1b[2;1R")
    fixed.settled()
    fixed.send(b"\x04")
    run.require(fixed.session.wait_exit() == ("exited", 0), "the fixed-palette session left at 0")
    run.require(
        pty.THEME_PROBE not in fixed.session.settled_text(),
        "XFX_THEME decided the palette, so no query was sent",
    )


# ---------------------------------------------------------------------------
# 13. cell diff correctness
# ---------------------------------------------------------------------------

# The keystrokes both painters are driven with after the answer has landed.
#
# Each one is a case a diff gets wrong by default and a full repaint cannot get
# wrong at all: a wide grapheme, a wide grapheme *overwritten by a narrow one*
# (whose second cell nothing else erases), a row that gets shorter, and a caret
# that moves without changing a single cell.
CELL_DIFF_EDITS = (
    ("a wide grapheme and a tail", "한글 tail", "한글 tail"),
    ("the tail deleted", "\x7f" * 5, "한글"),
    ("a narrow grapheme over a wide one", "\x7f\x7fx", "x"),
    ("the caret home over unchanged cells", "\x01", "x"),
)


def scenario_13(run):
    """The diff leaves the screen the Phase-1 full repaint leaves.

    The optimization's whole contract, and the only place it can honestly be
    asked: two release-quality binaries, the same facts, two painters, one
    screen. The `fault-injection` build keeps the Phase-1 painter behind
    `Fault::FullPaintReference` -- a compile-time seam, so the shipped binary
    has no way to select it and there is no second painter in the field.
    """
    marker = run.marker("celldiff")

    def drive(label, faulty):
        fixture = start_fixture(run, [fixtures.content_only(marker)], name=label)
        trial = run.trial(
            label,
            faulty=faulty,
            fault="full-paint-reference" if faulty else None,
            gateway=fixture,
        ).settled()
        from_a_known_composer(run, trial, label)
        discriminate(run, trial, fixture, marker, label="%s-answered" % label)
        for what, keys, composer in CELL_DIFF_EDITS:
            trial.send(keys)
            trial.wait_until(
                "%s: the composer to read %r" % (label, composer),
                lambda _text, wanted=composer: composer_text(trial.peek()) == wanted,
            )
        # **The two runs are compared in the same turn state, or they are not
        # comparing painters.** See [`settled_band`]: the marker
        # `discriminate` waited for arrives while the turn is still running, and
        # an active band holds the document's last line as its activity row --
        # so a snapshot taken before the release and one taken after it disagree
        # about that row whichever painter wrote them. Nothing is filtered and
        # no equality is loosened; only the moment of the snapshot is pinned.
        settled_band(run, trial, label)
        grid = trial.grid("%s-final" % label)
        run.require(
            not grid.unknown,
            "%s emitted only the sequences it declares: %r" % (label, grid.unknown),
        )
        # C-e to the end, C-u to kill the line, C-d on the empty composer to
        # leave: the last edit above left the caret at home, and a `C-u` there
        # kills nothing.
        trial.send(b"\x05\x15\x04")
        run.require(
            trial.session.wait_exit() == ("exited", 0),
            "the %s session left cleanly" % label,
        )
        fixture.stop()
        return grid

    reference = drive("full-paint", True)
    diffed = drive("cell-diff", False)

    # **The band is compared by row and the document by content**, and the split
    # is the product's rather than a convenience.
    #
    # The band is what a painter paints, and the two painters must leave it
    # identical cell for cell, attribute for attribute, with the caret in the
    # same place. The document is written by `Band::append_document`, which is
    # the same code in both builds -- and *where* its rows land is decided by a
    # race neither painter controls: an append places the transcript's unfinished
    # line at `band_top - 1` (`src/tui/frame.rs`'s `render_append`), and
    # `band_top` moves down by one the moment the turn ends and gives the
    # activity row back. So the last release of an answer lands on the activity
    # row's line if it wins that race and on the line below it if it does not,
    # in **either** build -- measured: the full-paint run above placed every
    # release on row 20 with seven activity-row frames, the diff run placed the
    # first three on row 20 and the rest on row 21 with two. Comparing document
    # rows by index would be asserting on which side of that race a run landed,
    # which is a property of the scheduler and not of the painter.
    band_top = the_band_top(run, reference, diffed)
    for row in range(band_top, reference.rows):
        run.require(
            diffed.cells[row] == reference.cells[row],
            "band row %d differs from the full-repaint reference:\n  diff %r\n  full %r"
            % (row + 1, diffed.row_text(row), reference.row_text(row)),
        )
        run.require(
            diffed.attrs[row] == reference.attrs[row] and diffed.styles[row] == reference.styles[row],
            "band row %d's attributes differ from the reference:\n  diff %r %r\n  full %r %r"
            % (
                row + 1,
                diffed.attrs_of_row(row),
                diffed.styles_of_row(row),
                reference.attrs_of_row(row),
                reference.styles_of_row(row),
            ),
        )
    run.require(
        (diffed.row, diffed.col) == (reference.row, reference.col),
        "the caret is at %r and the reference left it at %r"
        % ((diffed.row + 1, diffed.col + 1), (reference.row + 1, reference.col + 1)),
    )
    # Every row of the document either terminal has held, in order, scrollback
    # included -- so a build that dropped, doubled or garbled a row of the
    # answer fails here even though the line it sits on, and whether it is still
    # on the screen at all, are the scheduler's to choose.
    run.require(
        written_document(diffed, band_top) == written_document(reference, band_top),
        "the document differs from the full-repaint reference:\n  diff %r\n  full %r"
        % (written_document(diffed, band_top), written_document(reference, band_top)),
    )
    # A comparison of two blank screens would pass for anything, so the screen
    # both painters produced has to be a screen with the answer on it.
    run.require(
        reference.find(marker) is not None,
        "the reference screen never showed the answer, so the comparison is vacuous",
    )
    run.require(
        hint_text(reference),
        "the reference screen has no band on it, so the comparison is vacuous",
    )


def band_is_idle(grid):
    """Whether the band on `grid` has given its activity row back.

    Three answers rather than two. `True` is an idle band, `False` is a turn
    still running, and **`None` is a screen with no band on it** -- which is
    not an idle screen, it is a screen this oracle cannot read: a torn frame, a
    session that has not painted yet, or one whose band has gone. A barrier that
    folded `None` into `True` would be satisfied by exactly those.

    Read through `find_band`, which looks for the activity row where the band
    puts it -- above the composer, over the blank row that keeps it from the
    draft -- and in **either** blinking face of its marker (`ACTIVITY_ROW`), so a
    marker dark for half of every second cannot make a running turn read idle.

    **And the caret must be in the composer.** On a floating band an append
    lands on the rows the band has just moved down from (`fcc8e44`). Since T8 a
    frame carries its document growth inside the same synchronized update as
    the band (`fix/tui-sync-appends`, upstream's `composeWireFrame`), so no
    complete frame shows the line without the band that follows it -- but an
    update whose band part was refused ends with the caret wherever the
    document's last row left it (`src/tui/frame.rs`'s `END_FRAME_HIDDEN`). Every
    band frame ends by putting the caret back in the composer, so a caret
    outside it is still a screen the session has not settled on, and is not
    read as idle.
    """
    band = find_band(grid)
    if band is None:
        return None
    # A question replaces the composer and holds the turn: not idle.
    if not band.composer:
        return False
    return band.activity is None and band.composer[0] <= grid.row <= band.composer[-1]


def settled_band(run, trial, what):
    """Waits until the band has given its activity row back, and says so.

    **Why the comparison needs it.** `discriminate` waits for the fixture's
    marker, and the marker arrives while the turn is still running: the answer
    is streamed, and the turn ends some time afterwards. For that interval the
    band keeps its activity row -- and, per [`band_is_idle`], the row it keeps
    is the document's last line. So a grid taken on one side of the release and
    a grid taken on the other disagree about that row: the released one holds
    the tail of the answer there and the running one holds `• Thinking  0s`.
    Scenario 13 was comparing two painters across that difference, which is a
    property of the scheduler and not of either painter.

    **The absence is not filtered out and the equality is not loosened**, which
    would each hide a real defect: a build that stranded its activity row, or
    one that wrote a different document, would still pass. What changes is only
    *when* the snapshot is taken -- both grids are now taken in the same turn
    state, which is what the comparison always meant.

    Bounded, and a real stranded row **fails** here rather than being ignored:
    `Trial.wait_until` raises after its deadline, and the scenario runner
    records that as the failure it is.
    """
    trial.wait_until(
        "%s: the band to give its activity row back" % what,
        lambda _text: band_is_idle(trial.peek()) is True,
    )
    run.require(
        band_is_idle(trial.peek()) is True,
        "%s: the snapshot was taken while a turn still owned a row of the document" % what,
    )


def the_band_top(run, reference, diffed):
    """The row index the band starts at, agreed by both screens.

    Found rather than computed -- up from each screen's hint row, through
    `find_band` -- so a scenario that lost the band entirely fails here instead
    of comparing two empty ranges and passing. **Restated for the fx band**
    (`a50f8a0`): this used to find the always-drawn divider, which is blank
    now until a panel is up; the band's own top (the composer's first row on
    an idle band) is the boundary in its place.
    """
    found, expected = find_band(diffed), find_band(reference)
    run.require(expected is not None, "the full-repaint reference painted a band at all")
    run.require(
        found is not None and expected is not None and found.top == expected.top,
        "the two screens put the band's top on different rows (%r vs %r)" % (found, expected),
    )
    if found is None or expected is None or found.top != expected.top:
        raise Failure("the band could not be located on both screens")
    return found.top


def written_document(grid, band_top):
    """Every row the terminal has ever held above the band, oldest first.

    **Scrollback included, and row numbers excluded**, because both of those are
    the scheduler's rather than the painter's. `Band::append_document` scrolls
    with a real linefeed, so how many rows have left the top of the screen -- and
    therefore which of them are still visible and on what line -- depends on how
    many appends a run took and on whether the turn had ended when the last one
    landed. What does *not* depend on any of that is the text itself: the same
    prompt echo and the same answer, in the same order, whichever painter wrote
    the band beneath them.

    Attributes are not compared here and are compared exhaustively on the band
    instead. A row that has left the top of the screen is text to a terminal --
    this oracle keeps no attributes for it, and neither does a real one's
    scrollback -- so comparing them would mean comparing only the rows that
    happened to still be visible, which is the very thing this function exists
    to stop asserting. Every attribute this phase emits is painted on the band
    (`src/tui/theme.rs`: the divider, the hint row, the activity row), and those
    rows are compared cell for cell and run for run.

    **One row is compared by predicate rather than by text**: the dim summary a
    clean turn writes under its answer (`3efe232`) carries the turn's elapsed
    seconds, which are the clock's and not the painter's, so a summary row is
    replaced by a placeholder here once `SUMMARY_ROW` has matched it -- a row
    that is not a well-formed summary is kept verbatim and still compared.
    """
    written = [line for line in grid.scrollback if line.strip()]
    written.extend(
        grid.row_text(row) for row in range(band_top) if grid.row_text(row).strip()
    )
    return ["  <turn summary>" if SUMMARY_ROW.match(line) else line for line in written]


# ---------------------------------------------------------------------------
# 14. no-op frame skip
# ---------------------------------------------------------------------------

# How long a settled session is watched for bytes it has no reason to write.
#
# Thirty `render_request::ANIMATION_TICK`s and roughly two hundred of the event
# loop's own 8 ms ticks: long enough that a painter repainting per tick, per
# animation phase, or per runtime poll cannot get through it silently.
QUIET_WATCH = 1.5

# How long the wire has to have been still before the watch above begins.
QUIET_SETTLE = 0.4


def scenario_14(run):
    """A settled session writes nothing at all, and is still alive afterwards.

    Absence is never a pass condition by itself, so this scenario is two claims
    and needs both: the wire does not move while nothing changes, **and** the
    very next keystroke moves it. The second is what tells a working skip apart
    from a session that has died, wedged, or stopped reading its terminal.
    """
    marker = run.marker("noop")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    trial = run.trial("idle", gateway=fixture).settled()
    from_a_known_composer(run, trial, "idle")
    discriminate(run, trial, fixture, marker)

    # The turn is over and the band says the same thing it will go on saying.
    # Waited for rather than assumed: the pacer releases the answer against a
    # clock, so a session measured while it is still releasing text is a session
    # that has something to write.
    deadline = time.time() + pty.WAIT
    settled = len(trial.session.captured)
    quiet_since = time.time()
    while time.time() - quiet_since < QUIET_SETTLE:
        trial.session.pump(0.05)
        if len(trial.session.captured) != settled:
            settled = len(trial.session.captured)
            quiet_since = time.time()
        if time.time() > deadline:
            raise Failure("the session never stopped writing, so nothing can be measured")

    before = len(trial.session.captured)
    grid_before = trial.grid("settled")
    watch = time.time() + QUIET_WATCH
    while time.time() < watch:
        trial.session.pump(0.05)
    after = len(trial.session.captured)
    run.require(
        after == before,
        "a settled session wrote %d bytes across %.1fs of ticks with nothing to say: %r"
        % (after - before, QUIET_WATCH, trial.session.text()[before:after]),
    )

    # The positive half. A session that is skipping frames because it is broken
    # would pass everything above.
    trial.send("z")
    trial.wait_until(
        "the next keystroke to reach the composer",
        lambda _text: composer_text(trial.peek()) == "z",
    )
    run.require(
        len(trial.session.captured) > before,
        "the session was silent because it had stopped painting, not because "
        "it had nothing to paint",
    )
    grid_after = trial.grid("after-a-keystroke")
    run.require(
        grid_before.text() != grid_after.text(),
        "the screen did not change when the composer did",
    )
    run.require(not grid_after.unknown, "xfx emitted only the sequences it declares")

    trial.send(b"\x15\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the idle session left at 0")
    fixture.stop()


# ---------------------------------------------------------------------------
# 15. resize reflow
# ---------------------------------------------------------------------------


def rows_a_gesture_wrote_on(written):
    """Every terminal row a captured run of bytes placed the cursor on.

    **Every column, not only the first**, and that is the whole of what makes
    this an instrument rather than a decoration. The band places its own rows at
    column 1, but a frame is a *difference*: `src/tui/grid.rs` emits its `CUP`
    at the **first changed column** of a row, and a frame ends by putting the
    caret wherever the composer's text ends. So a detector matching only `;1H`
    would report "xfx wrote nothing above the band" for a session that had
    rewritten a row of the user's transcript from its second cell onwards --
    which is precisely the claim scenario 15 exists to make, and a detector that
    cannot fail is not evidence.
    """
    return [int(row) for row in re.findall(r"\x1b\[(\d+);\d+H", written)]


def scenario_15(run):
    """A screen that changed size gets the whole band back, where it now goes.

    Item 12, end to end on a real terminal. The claim has two halves and the
    second is what bounds the work: **the band and the composer reflow to the
    screen that now exists, and the terminal's own document is left alone.**
    Everything above the divider was written into the terminal's document once
    and is in its native scrollback now, where the terminal re-wrapped it by
    rules xfx does not model -- so this scenario does not require xfx to
    repaint one, and would be wrong to.
    """
    before_marker = run.marker("before")
    wide_marker = run.marker("wide")
    # The second answer is one unbroken token longer than the old screen was
    # wide: on a 120-column terminal it is one document row, and a transcript
    # still wrapping to the 80 it launched on would put it on two.
    wide_answer = wide_marker + "y" * 80
    fixture = start_fixture(
        run,
        [fixtures.content_only(before_marker), fixtures.content_only(wide_answer)],
    )
    trial = run.trial("reflow", gateway=fixture).settled()
    from_a_known_composer(run, trial, "reflow")

    # The instrument, falsified before it is trusted. A `CUP` above the band at
    # a column other than the first is exactly what a cell diff emits, so a
    # detector that missed one would pass this scenario for a session that had
    # rewritten the user's transcript. Synthetic and non-vacuous: the row at
    # column 2 must be seen, and the row at column 1 must still be.
    run.require(
        rows_a_gesture_wrote_on("\x1b[7;2Hx\x1b[9;1Hy") == [7, 9],
        "the above-band detector sees a CUP at a column other than the first: %r"
        % rows_a_gesture_wrote_on("\x1b[7;2Hx\x1b[9;1Hy"),
    )

    # An answer in the terminal's own document, and the discriminator every
    # scenario owes: the nonce in the request, the fixture's marker on the
    # screen, and only sequences xfx declares.
    discriminate(run, trial, fixture, before_marker, label="before-resize")

    # A draft that wraps at eighty and does not at a hundred and twenty: the
    # composer's height is a function of the width, so a band re-solved without
    # re-measuring the draft would put the divider where the old wrap said.
    draft = "x" * 90
    trial.send(draft)
    trial.wait_until(
        "the whole draft in the composer",
        lambda _t: composer_text(trial.peek()) == draft,
    )
    narrow = trial.grid("before-resize-band")
    # **Restated for the floating band** (`fcc8e44`): this used to pin the
    # narrow band to rows 21-24 (a rule, two composer rows, the hint) and the
    # wide one to rows 28-30 under a rule on row 28. The band now floats under
    # the document until the screen fills, and a resize pins it to the bottom
    # (where a terminal reflowed its rows is unknowable), so the narrow band is
    # read where it floats -- two composer rows, the blank rule row and the
    # hint right under them -- and the wide one must be **at the bottom** of
    # the taller screen: one composer row on row 28, the blank rule row 29, the
    # hint on row 30.
    narrow_band = find_band(narrow)
    run.require(
        narrow_band is not None
        and len(narrow_band.composer) == 2
        and narrow_band.divider == narrow_band.composer[-1] + 1
        and grid_row_blank(narrow, narrow_band.divider)
        and narrow_band.hint == narrow_band.divider + 1,
        "the draft is on two composer rows to begin with, over the blank rule row and the "
        "hint: %r" % (narrow_band,),
    )
    old_top = narrow_band.top if narrow_band is not None else 0

    # Where the wire stands before the resize, so what is scanned below is this
    # gesture's bytes and nothing else.
    wire = len(trial.session.captured)
    trial.resize(30, 120)

    def pinned(grid):
        band = find_band(grid)
        return (
            band is not None
            and band.hint == grid.rows - 1
            and band.divider == grid.rows - 2
            and band.composer == [grid.rows - 3]
        )

    trial.wait_until(
        "the band pinned to the foot of the taller screen",
        lambda _t: pinned(trial.peek()) and composer_text(trial.peek()) == draft,
    )
    wide = trial.grid("after-resize-band")
    wide_band = find_band(wide)
    run.require(
        pinned(wide),
        "the band is pinned to the bottom of the new screen -- composer row 28, the blank "
        "rule row 29, the hint row 30: %r" % (wide_band,),
    )
    run.require(
        composer_text(wide) == draft and composer_first_row(wide) == 27,
        "the draft is back on one composer row and it is still the whole draft: %r"
        % composer_text(wide),
    )
    run.require(
        hint_text(wide) == HINT_AUTO,
        "the hint row on the last row of the new screen says what a turn runs with: %r"
        % hint_text(wide),
    )
    # **No stale rows.** The rows the band owned on the old screen are not the
    # band's now, and nothing in this phase repaints a document row -- so a band
    # that merely drew itself somewhere else would leave a second composer and
    # a second hint row on the screen for the rest of the session.
    stale = [
        row + 1
        for row in range(old_top, (narrow_band.hint + 1) if narrow_band is not None else 0)
        if wide_band is not None and row < wide_band.top and wide.row_text(row).strip() != ""
    ]
    run.require(not stale, "the band the resize left behind is gone from rows %r" % stale)
    # **And nothing above them was touched.** The committed transcript is the
    # terminal's own document and its own scrollback; the terminal re-wrapped it
    # when it changed size, by rules xfx does not model, so a session that
    # repainted one of those rows would be overwriting text it cannot describe.
    # The rows this gesture is allowed to write are the band's own -- from the
    # top row the band had on the old screen downwards.
    written = bytes(trial.session.captured)[wire:].decode("utf-8", "replace")
    above = sorted({row for row in rows_a_gesture_wrote_on(written) if row < old_top + 1})
    run.require(
        old_top > 0 and not above,
        "the resize wrote on the terminal's own document (above row %d), at row(s) %r"
        % (old_top + 1, above),
    )
    run.require(
        not wide.unknown, "xfx emitted only the sequences it declares: %r" % wide.unknown
    )

    # And a decision still reaches the fixture: the session is not merely
    # painting, it is still reading its keyboard and still running turns.
    trial.send(b"\x15")
    trial.send("say " + run.nonce + " again\r")
    # Waited for on the **whole** answer rather than on its first cells: the
    # pacer releases text against a clock, so a grid snapshotted at the marker
    # is a screen with a third of the line on it.
    trial.wait_until(
        "the whole of the second answer on one row of the new screen",
        lambda _t: any(
            trial.peek().row_text(row).strip() == wide_answer
            for row in range(trial.rows)
        ),
    )
    run.require(
        len(fixture.bodies()) == 2,
        "the prompt submitted after the resize reached the provider (%d request(s))"
        % len(fixture.bodies()),
    )
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "carrying the nonce this run minted",
    )
    after = trial.grid("after-resize-answer")
    found = after.find(wide_marker)
    run.require(found is not None, "the second answer is rendered on the new screen")
    if found is not None:
        row, _column = found
        run.require(
            after.row_text(row).strip() == wide_answer,
            "the answer is on one document row of the wider screen rather than "
            "wrapped to the width it launched on: %r" % after.row_text(row),
        )
    run.require(
        not after.unknown, "xfx emitted only the sequences it declares: %r" % after.unknown
    )

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the reflow session left at 0")
    fixture.stop()


# ---------------------------------------------------------------------------
# 16. the slash menu
# ---------------------------------------------------------------------------


# The canonical palette, read out of the source the binary was built from.
#
# **Not a list kept here.** One kept here was seven names long when the
# registry was twenty-nine, and the row failed for a menu that was right. And
# not one read out of the binary, which would agree with whatever the popup
# rendered: `src/interactive.rs`'s `SLASH_REGISTRY` is what the popup is
# *required* to show -- upstream's order, upstream's categories
# (`vercel-labs/fx@c1db9191 src/builtins/commands.zig:463-499`) -- so it is read
# the way `scripts/check-no-stubs.sh` reads it, textually and without building
# anything, from the copy the runner put beside these helpers. A screen that
# disagrees with its own source fails here.
class SlashSpec:
    """One registry row: a name, its aliases, its summary and its category."""

    def __init__(self, name, aliases, summary, category):
        self.name, self.aliases, self.summary, self.category = name, aliases, summary, category

    def __repr__(self):
        return "SlashSpec(%r, %r, %r)" % (self.name, self.aliases, self.category)


def rust_strings(text):
    """Every string literal in `text`, unescaped, in order."""
    return [
        re.sub(r"\\(.)", r"\1", literal)
        for literal in re.findall(r'"((?:[^"\\]|\\.)*)"', text)
    ]


def rust_const_block(source, declaration):
    """The body of the `const` array that opens with `declaration`, comments dropped."""
    start = source.index(declaration) + len(declaration)
    end = source.index("\n];", start)
    lines = source[start:end].split("\n")
    return "\n".join(line for line in lines if not line.strip().startswith("//"))


def ranked_completions(registry, typed):
    """The names the popup offers for `typed`, in its order, read off the registry.

    Upstream's rule (`vercel-labs/fx@481104fd
    src/core/slash_commands/command_specs.zig:520-565`): a command matches by
    the best of its name and its aliases -- 0 for the word exactly, 1 for a
    prefix, 2 for the word after its slash anywhere after the name's (a
    one-character word has no substring tier) -- the name kept on a tie, and
    the list is every rank-0 match, then every rank-1, then every rank-2, each
    in the registry's order, shown under the spelling that matched.
    """

    def rank(command):
        if command == typed:
            return 0
        if command.startswith(typed):
            return 1
        if len(typed) > 1 and len(command) > 1 and typed[1:] in command[1:]:
            return 2
        return None

    matches = []
    for spec in registry:
        best = None
        for spelling in [spec.name] + list(spec.aliases):
            found = rank(spelling)
            if found is not None and (best is None or found < best[0]):
                best = (found, spelling)
        if best is not None:
            matches.append(best)
    return [spelling for tier in (0, 1, 2) for found, spelling in matches if found == tier]


def slash_registry():
    """`SLASH_REGISTRY` in its own order, checked against the declarations pinned to it.

    The crate pins `SLASH_REGISTRY` to the flat `SLASH_COMMANDS` and every row's
    category to `SLASH_CATEGORIES` at compile time, so a reading that disagreed
    with either would be this parser's defect rather than the product's -- and
    is refused here, before any scenario trusts it, rather than turned into a
    menu the popup was "wrong" about.
    """
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "interactive.rs")
    with open(path, encoding="utf-8") as handle:
        source = handle.read()
    rows = rust_const_block(source, "pub const SLASH_REGISTRY: &[SlashSpec] = &[")
    registry = []
    for body in re.findall(r"SlashSpec \{(.*?)\n\s*\}", rows, re.S):
        fields = {}
        for field in ("name", "summary", "category"):
            found = re.search(r'\b%s:\s*("(?:[^"\\]|\\.)*")' % field, body)
            if found is None:
                raise Failure("a registry row with no %s: %r" % (field, body))
            fields[field] = rust_strings(found.group(1))[0]
        aliases = re.search(r"\baliases:\s*&\[(.*?)\]", body, re.S)
        if aliases is None:
            raise Failure("a registry row with no aliases: %r" % body)
        registry.append(
            SlashSpec(fields["name"], rust_strings(aliases.group(1)), fields["summary"], fields["category"])
        )
    commands = rust_strings(rust_const_block(source, "pub const SLASH_COMMANDS: &[&str] = &["))
    categories = rust_strings(rust_const_block(source, "pub const SLASH_CATEGORIES: &[&str] = &["))
    names = [spec.name for spec in registry]
    if not registry or names != commands:
        raise Failure("the registry read %r, and SLASH_COMMANDS says %r" % (names, commands))
    stray = [spec for spec in registry if spec.category not in categories]
    if stray:
        raise Failure("registry rows filed under no category of %r: %r" % (categories, stray))
    return registry


# The popup's key help, on the hint row while it is up (`.prd/tui-phase4/receipts/wu-b`,
# "the key-help row"; `fx-slash-40x120`).
MENU_KEYS = "\u2191\u2193 navigate     enter use     esc close"

# The popup's header: how many commands the word matches, the filter prompt on a
# bare slash, and the window's range at the right (`receipts/wu-b`: `Commands 36
# · type to filter … 1–6`).
MENU_HEADER = re.compile(r"^Commands (\d+)(?: \u00b7 type to filter)?(?:\s+(\d+)\u2013(\d+))?$")


def menu_rows(grid):
    """The rows the menu is on, top first, and `[]` when there is no menu.

    **The band's panel**, read through `find_band`: the rows between the rule
    under the composer and the bottom rule over the key help, and only those of
    them in the menu's own shape -- an indent, then a slash name. Bounded that
    way rather than "every row on the screen that starts with a slash name",
    because the document above the band holds rows in exactly that shape -- an
    answer line is written two columns in, so an answer that begins with
    `/help` is `  /help` on the screen -- and a reader that counted those would
    report a menu that had closed as a menu that was still up.

    **Restated for fx's popup** (`receipts/wu-b`): the marked row has no `> `
    any more -- it is told apart by its bold name ([`marked_entry`]) -- and a
    header and a blank row sit above the entries.
    """
    band = find_band(grid)
    if band is None:
        return []
    return [row for row in band.panel if grid.row_text(row).startswith("  /")]


def offered(grid):
    """Every command name the menu is showing, top row first.

    Read off the **cells** rather than out of the byte stream: a menu is a claim
    about the screen, and a name that reached the terminal inside a frame that
    was then overwritten is not on it.
    """
    return [grid.row_text(row)[2:].strip().split(" ")[0] for row in menu_rows(grid)]


def menu_header(grid):
    """The popup's header as `(count, first, last)`, or `None` with no popup up.

    `first` and `last` are the window's range, `None` when the header shows
    none (a list that fits whole).
    """
    band = find_band(grid)
    if band is None or not band.panel:
        return None
    found = MENU_HEADER.match(grid.row_text(band.panel[0]).strip())
    if found is None:
        return None
    first = int(found.group(2)) if found.group(2) else None
    last = int(found.group(3)) if found.group(3) else None
    return (int(found.group(1)), first, last)


def marked_entry(grid):
    """The entry Enter would take, as `(name, category)`, or `None`.

    Upstream marks it with a bold name and no glyph (`receipts/wu-b`: "marked
    row `1;255`", every other row in the dim grey), so it is read off the name's
    first cell's style. Exactly one row may be marked; two is `None`.
    """
    marked = [row for row in menu_rows(grid) if "1" in grid.styles[row][2].split(";")]
    if len(marked) != 1:
        return None
    text = grid.row_text(marked[0])
    return (text[2:].split(" ")[0], text.split()[-1])


def walk_menu(trial, count, what):
    """Every entry the popup marks as Down walks it, `count` of them, then one more.

    The window shows six entries at a time (`receipts/wu-b`, `1–6`) and scrolls
    to keep the marked one in it, so the whole list is read one marked entry at
    a time -- each step waited for on the committed grid, because a step is a
    repaint and a reading taken before it lands would see the entry before.
    Returns the entries in the order they were marked and the entry the step
    past the last one marked, which is where the list wraps to.
    """
    seen = []
    for step in range(count):
        entry = marked_entry(trial.peek())
        seen.append(entry)
        trial.send(b"\x1b[B")
        trial.wait_until(
            "%s: the walk's step %d to land" % (what, step + 1),
            lambda _t, before=entry: marked_entry(trial.peek()) not in (None, before),
        )
    return seen, marked_entry(trial.peek())


# What the inline model column says once its catalog read has failed
# (`src/tui/model_column.rs`'s `Known::Failed` row), as opposed to the
# `loading models...` it says while the read is out.
COLUMN_UNLOADED = "unable to load models"


def scenario_16(run):
    """Typing a slash opens a ranked menu that completes without taking focus.

    Item 13 on a real terminal. Five claims, and none of them is "a menu
    appeared": **where** it is (under the composer, in the rows a question
    would take), **what order** it is in (an exact name, then a prefix of a
    name or an alias in the registry's order, then a substring), **who owns the
    caret** (the composer, always), **what Escape does** (closes it, and keeps
    it closed while the same word grows), and that a completed command **runs
    once** rather than being sent to a model.

    `/setup` is what supplies the alias: it is a real registry alias for
    `/provider` rather than a fixture, so the ranking asserted here is the
    ranking the product has. (It was `/exit` for `/e` until fx v0.0.16's
    `/effort` took that prefix; see the alias block below.)

    **Restated for fx's popup** (`.prd/tui-phase4/receipts/wu-b`). The menu used
    to list every command at once, marked with `> `, and a completed `/model`
    answered with a `[shell] model=` line. Upstream's popup shows a header with
    the count and the window's range and six entries at a time, marks the entry
    Enter would take by its bold name, files each entry under its category at
    the right, and puts its own key help on the hint row; a completed `/model`
    opens `/model`'s menu. So "every command, in `/help`'s order" is read by
    walking the window from the first entry to the last -- every name the
    registry declares, in its order and under its category, then the wrap --
    and "it ran once" is read off the menu the command opened. The registry is
    the source's (`slash_registry`), not a list kept here.
    """
    registry = slash_registry()
    canonical = [spec.name for spec in registry]
    marker = run.marker("menu")
    # The answer's second paragraph begins with a slash name, so the document
    # holds a row in the menu's own shape -- `  /help …`, two columns in -- for
    # the last reading below to tell the band's menu apart from.
    lookalike = "/help is a word in this answer, not a menu row"
    fixture = start_fixture(run, [fixtures.content_only(marker + "\n\n" + lookalike)])
    trial = run.trial("menu", gateway=fixture).settled()
    from_a_known_composer(run, trial, "menu")
    # The discriminator every scenario owes, before the menu is touched: the
    # nonce in the request xfx really sent, the fixture's marker on the screen.
    discriminate(run, trial, fixture, marker)
    trial.send(b"\x15")
    trial.wait_until(
        "the composer to be empty again",
        lambda _t: composer_text(trial.peek()) == "",
    )

    # --- a bare slash lists the whole registry, in `/help`'s order ----------
    trial.send("/")
    trial.wait_until(
        "the menu to open on a bare slash",
        lambda _t: menu_header(trial.peek()) is not None and offered(trial.peek()),
    )
    opened = trial.grid("menu-open")
    header = menu_header(opened)
    shown = len(offered(opened))
    run.require(
        header is not None
        and header[0] == len(canonical)
        and header[1:] == (1, shown)
        and shown == min(len(canonical), 6),
        "the header counts the whole registry (%d) over the window it shows: %r, %d entr(y/ies)"
        % (len(canonical), header, shown),
    )
    run.require(
        offered(opened) == canonical[:shown],
        "the window opens on the registry's head, in its order: %r" % offered(opened),
    )
    # **Restated for the fx band** (`a50f8a0`): the menu used to be the rows
    # directly above the one rule, with the composer under it. It is now the
    # band's panel -- under the composer, between the rule that heads it and
    # the bottom rule over the key help -- and the composer is directly above it.
    band = find_band(opened)
    run.require(
        band is not None and band.picker_rule is not None,
        "the band has a rule under the composer to place the menu against: %r" % (band,),
    )
    rule = band.picker_rule if band is not None and band.picker_rule is not None else 0
    run.require(
        band is not None
        and band.panel[:2] == [rule + 1, rule + 2]
        and opened.row_text(rule + 2) == ""
        and menu_rows(opened) == list(range(rule + 3, rule + 3 + shown))
        and band.divider == rule + 3 + shown
        and is_rule(opened, band.divider)
        and opened.row_text(band.hint) == MENU_KEYS,
        "the menu is the header, a blank row and its %d entries between the rule under the "
        "composer and the bottom rule, its key help under them: %r against %r"
        % (shown, menu_rows(opened), band),
    )
    run.require(
        marked_entry(opened) == (canonical[0], registry[0].category),
        "exactly the top entry is marked: %r" % (marked_entry(opened),),
    )
    run.require(
        band is not None and band.composer == [rule - 1] and opened.row_text(rule - 1) == RAIL + " /",
        "the composer is directly above the rule that heads the menu, holding the draft: %r"
        % opened.row_text(rule - 1),
    )
    # The caret, which is the one thing that says which of the two the next
    # keystroke goes to: in the composer, past the draft, never on the marked
    # row of the menu.
    run.require(
        (opened.row, opened.col) == (rule - 1, 3),
        "the menu took the caret off the text being typed: row %d col %d, composer at row %d"
        % (opened.row, opened.col, rule - 1),
    )
    run.require(not opened.unknown, "xfx emitted only the sequences it declares: %r" % opened.unknown)
    # Every command, in the registry's order and under its category, read one
    # marked entry at a time -- and the step past the last wraps to the first.
    walked, wrapped = walk_menu(trial, len(canonical), "the bare-slash menu")
    run.require(
        walked == [(spec.name, spec.category) for spec in registry],
        "walking the menu marked every command in /help's order, each under its own "
        "category: %r" % (walked,),
    )
    run.require(
        wrapped == (canonical[0], registry[0].category),
        "the step past the last command wrapped to the first: %r" % (wrapped,),
    )
    walked_grid = trial.grid("menu-wrapped")
    run.require(
        menu_header(walked_grid) == (len(canonical), 1, shown),
        "and the window followed it back to the head: %r" % (menu_header(walked_grid),),
    )

    # --- an exact prefix narrows it to one ---------------------------------
    trial.send(b"\x15")
    trial.send("/he")
    trial.wait_until(
        "the menu to narrow to the command the draft names",
        lambda _t: offered(trial.peek()) == ["/help"],
    )
    exact = trial.grid("menu-exact-prefix")
    run.require(
        offered(exact) == ["/help"] and (menu_header(exact) or (None,))[0] == 1,
        "an exact prefix offers exactly the command it names: %r %r"
        % (offered(exact), menu_header(exact)),
    )

    # --- the alias tier outranks the substring tier ------------------------
    #
    # `/se` names `/settings`, and `/provider` is on this list for one reason:
    # its `/setup` alias -- shown under its own name, as upstream's popup shows
    # an alias that matched (`receipts/wu-b`, `filter-ex`). Upstream ranks an
    # exact name, then a prefix of a name **or** an alias -- one tier, in the
    # registry's order -- then a substring, and gives each command the best of
    # its name and its aliases (`completionCommandMatchRank`,
    # `bestCompletionCommandMatch`, `nthSlashCommandCompletionMatch`,
    # `vercel-labs/fx@481104fd src/core/slash_commands/command_specs.zig:520-565`).
    # So `/setup` is first, `/settings` after it, and `/reset` -- an `se` in
    # the middle -- last: fx v0.0.16 shows exactly that for `/se`
    # (`Commands 3`, `/setup`, `/settings`, `/reset`; the EF1g oracle capture).
    #
    # **Restated for fx v0.0.16** (EF1g). This used to type `/e`, whose only
    # prefix was the `/exit` alias. `/effort` (`commands.zig:459@481104fd`) is a
    # name with that prefix and comes first in the registry, so `/e` now opens
    # on `/effort` -- in fx v0.0.16 too (`/effort`, `/exit`, then the
    # substrings) -- and no longer pins an alias above the substrings.
    trial.send(b"\x15")
    trial.send("/se")
    trial.wait_until(
        "the menu to rank the alias above the substrings",
        lambda _t: offered(trial.peek())[:1] == ["/setup"],
    )
    ranked = trial.grid("menu-alias-ranking")
    expected = ranked_completions(registry, "/se")
    substrings = [at for at, name in enumerate(expected) if not name.startswith("/se")]
    run.require(
        expected[:1] == ["/setup"] and bool(substrings),
        "the registry gives /se an alias first and a substring after it, or this pins nothing: %r"
        % (expected,),
    )
    run.require(
        expected.index("/setup") < min(substrings),
        "the alias is above every substring in the expected list: %r" % (expected,),
    )
    count = (menu_header(ranked) or (0,))[0]
    walked, _wrapped = walk_menu(trial, count, "the /se menu")
    run.require(
        count == len(expected) and [name for name, _category in walked] == expected,
        "the alias prefix outranks every substring: %r" % (walked,),
    )
    run.require(
        bool(walked) and walked[0][1] == next(spec.category for spec in registry if "/setup" in spec.aliases),
        "the alias is filed under its command's category: %r" % (walked[:1],),
    )

    # --- ties in the substring tier keep the registry's order ---------------
    #
    # `/et` is all substring: `/reset`, the `/setup` alias, `/settings` -- each
    # with an `et` after its slash and none starting with it -- and upstream
    # lists them in the registry's order, the alias under its own name
    # (`nthSlashCommandCompletionMatch`,
    # `src/core/slash_commands/command_specs.zig:549-565@481104fd`; fx v0.0.16
    # shows `/reset`, `/setup`, `/settings`, identical to xfx, in
    # `.prd/tui-phase5/receipts/effort-menu`). That order is neither the
    # names' alphabetical one (`/settings` before `/setup`) nor the match's
    # position (`/setup` before `/reset`), so a menu that sorted its ties
    # either way would fail here. The old `/e` probe pinned this over some
    # twenty substrings; `/se`'s tier is one entry and cannot.
    # Waited for by the draft as well as by the list: `/se`'s menu has as many
    # entries as this one, so a list read before `/et` is in the composer
    # would be the last probe's.
    expected = ranked_completions(registry, "/et")
    trial.send(b"\x15")
    trial.send("/et")
    trial.wait_until(
        "the menu to list the substrings of /et",
        lambda _t: composer_text(trial.peek()) == "/et"
        and (menu_header(trial.peek()) or (0,))[0] == len(expected)
        and set(offered(trial.peek())) == set(expected[:6]),
    )
    tied = trial.grid("menu-substring-ties")
    run.require(
        len(expected) >= 2 and not any(name.startswith("/et") for name in expected),
        "the registry gives /et a substring tier of two or more and nothing else, or this pins "
        "no tie order: %r" % (expected,),
    )
    count = (menu_header(tied) or (0,))[0]
    walked, _wrapped = walk_menu(trial, count, "the /et menu")
    run.require(
        count == len(expected) and [name for name, _category in walked] == expected,
        "the substring tier keeps the registry's order, an alias under its own name: %r"
        % (walked,),
    )

    # --- a word that names nothing lists nothing ---------------------------
    trial.send(b"\x15")
    trial.send("/zzz")
    trial.wait_until(
        "the composer to hold a word that names nothing",
        lambda _t: composer_text(trial.peek()) == "/zzz",
    )
    empty = trial.grid("menu-no-match")
    run.require(
        offered(empty) == [] and menu_header(empty) is None,
        "an unknown sequence offers nothing at all: %r" % offered(empty),
    )

    # --- Escape closes it, and keeps it closed while the word grows ---------
    trial.send(b"\x15")
    trial.send("/he")
    trial.wait_until(
        "the menu to be open before it is dismissed",
        lambda _t: offered(trial.peek()) == ["/help"],
    )
    trial.send(b"\x1b")
    trial.wait_until(
        "the menu to close",
        lambda _t: offered(trial.peek()) == [],
    )
    dismissed = trial.grid("menu-dismissed")
    run.require(offered(dismissed) == [], "Escape closed the menu: %r" % offered(dismissed))
    run.require(
        composer_text(dismissed) == "/he",
        "Escape left the draft alone: %r" % composer_text(dismissed),
    )
    # And it did **not** arm the composer's own double-Escape clear, which is
    # the gesture the same key means when there is no menu to close.
    run.require(
        hint_text(dismissed) is not None and "esc again to clear" not in hint_text(dismissed),
        "Escape armed the clear gesture instead of closing the menu: %r" % hint_text(dismissed),
    )
    trial.send("l")
    trial.wait_until(
        "the next letter to reach the composer",
        lambda _t: composer_text(trial.peek()) == "/hel",
    )
    grown = trial.grid("menu-suppressed")
    run.require(
        offered(grown) == [],
        "the dismissal survived the next letter of the same word: %r" % offered(grown),
    )

    # --- and the trigger resets once the word is gone -----------------------
    trial.send(b"\x15")
    trial.send("/mod")
    trial.wait_until(
        "a new word to open a menu of its own",
        lambda _t: offered(trial.peek()) == ["/model"],
    )
    reset = trial.grid("menu-trigger-reset")
    run.require(
        offered(reset) == ["/model"],
        "a new slash word inherited the old one's dismissal: %r" % offered(reset),
    )

    # --- Tab completes, and the completed command runs once -----------------
    trial.send(b"\t")
    trial.wait_until(
        "the completed name in the composer",
        lambda _t: composer_text(trial.peek()) == "/model",
    )
    completed = trial.grid("menu-completed")
    run.require(
        composer_text(completed) == "/model",
        "Tab completed the whole name: %r" % composer_text(completed),
    )
    # **The trailing space is asserted through the caret**, because a space is
    # not a cell anyone can read: the composer's marker is two cells and the
    # name is six, so a caret at column nine is a caret one cell past the last
    # letter -- which is the room the argument goes in.
    run.require(
        completed.col == 9,
        "the completion left room for the argument: caret at column %d" % completed.col,
    )
    run.require(
        offered(completed) == [],
        "the menu outlived the completion it wrote: %r" % offered(completed),
    )
    before = len(fixture.bodies())
    # **The inline column's own read answers first.** A completed `/model `
    # opens the inline model column, and since D2c round 6 (`9aa666c7`) that
    # column asks a catalog that failed again, from its own frames, and says
    # `loading models...` until the read lands -- and an Enter on an empty
    # query while it is loading is spent, as fx's `submitModelPicker` spends
    # it (`src/tui/shell.rs`'s `submit_column`, D2b;
    # `input_completion_runtime.zig:1161-1210`). This row sent its Enter
    # straight after the completion and lost that race on 1 run in 3 at plan
    # `de6618de`. With no catalog to give the read fails, the column says so,
    # and only then is the Enter the command's.
    trial.wait_until(
        "the inline model column's catalog read to answer",
        lambda _t: trial.peek().find(COLUMN_UNLOADED) is not None,
    )
    trial.send("\r")
    # `/model` answers with its own menu (`.prd/tui-phase4/receipts/d2a`),
    # which is the session's answer and nothing a model was asked for: the
    # catalog it lists is fetched apart from any turn (`Fixture`'s
    # `catalog_requests`), and with none to give, the menu says so.
    trial.wait_until(
        "the completed command to be answered by the session",
        lambda _t: any(
            text.startswith("Models ") for text in (panel_rows_text(trial.peek()) or [])
        ),
    )
    ran = trial.grid("menu-completed-ran")
    run.require(
        composer_text(ran) == "",
        "the completed command was taken out of the composer to run: %r" % composer_text(ran),
    )
    run.require(
        len(fixture.bodies()) == before,
        "the completed command was answered without asking the provider (%d new request(s))"
        % (len(fixture.bodies()) - before),
    )
    run.require(not ran.unknown, "xfx emitted only the sequences it declares: %r" % ran.unknown)
    trial.send(b"\x1b")
    trial.wait_until(
        "the command's menu to close",
        lambda _t: panel_rows_text(trial.peek()) == [],
    )

    # --- and it is still the band's menu with a lookalike in the document ---
    #
    # **Restated for the fx band** (`a50f8a0`, `3efe232`): this reading used to
    # be taken with the echo of `/model` -- `> /model`, a row in the menu's own
    # shape -- directly on top of a one-row menu. The echo is a card behind the
    # rail now and the menu is under the composer, so the lookalike is the
    # answer's own `  /help …` row instead: the document holds a row in the
    # menu's shape and the band's menu must still offer `/help` exactly once,
    # read off its panel and not off every slash-shaped row on the screen. And
    # the document is pressed against the band: its last written row is right
    # above the blank row the band's top keeps from it.
    trial.send("/he")
    trial.wait_until(
        "a one-entry menu with the document directly above the band",
        lambda _t: composer_text(trial.peek()) == "/he" and offered(trial.peek()) == ["/help"],
    )
    pressed = trial.grid("menu-against-the-document")
    pressed_band = find_band(pressed)
    run.require(
        any(text.startswith("  " + lookalike) for text in (rows_above_band(pressed) or [])),
        "the document holds a row in the menu's own shape: %r" % (rows_above_band(pressed),),
    )
    run.require(
        offered(pressed) == ["/help"],
        "the menu is the band's block rather than every slash-shaped row on the screen: %r"
        % offered(pressed),
    )
    run.require(
        pressed_band is not None
        and pressed.row_text(pressed_band.top - 1) == ""
        and pressed.row_text(pressed_band.top - 2).strip() != "",
        "the document really is pressed against the band: %r" % (pressed_band,),
    )

    # The draft goes before the Ctrl-D does: with text under the caret that key
    # is a forward delete on this surface, not an exit.
    trial.send(b"\x15\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the menu session left at 0")
    fixture.stop()


# ---------------------------------------------------------------------------
# plumbing
# ---------------------------------------------------------------------------


def user_messages(request):
    """The text of every user message in a captured request body."""
    body = json.loads(request["body"])
    return [
        part["text"]
        for message in body.get("prompt", [])
        if message.get("role") == "user"
        for part in message.get("content", [])
        if part.get("type") == "text"
    ]


def start_fixture(run, script, name="gateway"):
    path = os.path.join(run.dir, "%s-requests.jsonl" % name)
    return fixtures.Fixture(script, record_path=path)


def start_daemon(run, script, catalog=None, name="llmux"):
    path = os.path.join(run.dir, "%s-requests.jsonl" % name)
    return fixtures.Llmux(script, catalog=catalog, record_path=path)


def seeded_home(run, label, document):
    """A home whose `~/.xfx/settings.json` says `document`, before any launch.

    Built and written **before** the trial exists, because the child reads the
    profile at startup: a file written after the process is up is a file the
    session it is meant to configure has already gone past. A provider is a
    property of the machine, and the profile is where a real one is configured,
    so writing it here is also what proves the shell reads it.
    """
    home = os.path.join(run.dir, label, "home")
    directory = os.path.join(home, ".xfx")
    os.makedirs(directory, mode=0o700, exist_ok=True)
    with open(os.path.join(directory, "settings.json"), "w", encoding="utf-8") as handle:
        json.dump(document, handle)
    return home


def profile_of(trial):
    """The settings file as it stands now, or `None` when there is not one.

    Read rather than believed. A band that says `provider=llmux` over a profile
    that still says gateway is exactly the failure the transaction's reload step
    exists to prevent, and a scenario that only read the screen could not tell
    the two apart.
    """
    path = os.path.join(trial.home, ".xfx", "settings.json")
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def document_rows(grid, needle):
    """Every row of the screen carrying `needle`.

    Rows are zero-based here, as everywhere else in this harness
    (`vt_grid.Grid.row_text`).
    """
    return [
        grid.row_text(row) for row in range(grid.rows) if needle in grid.row_text(row)
    ]


def scenario_17(run):
    """What was submitted comes back, and so does the draft that stood aside.

    Item 15 on a real terminal. Five claims, and the first one alone is what a
    history without a captured draft would satisfy: **the arrow walks** from the
    edge of the draft, **it walks on from anywhere on a line**, **the near end
    is the half-typed line** rather than an empty composer, **the far end does
    not wrap**, and **an edit ends the walk** -- so the next step back starts at
    the newest entry again and comes back to the text the edit left behind.

    **Restated for fx's composer keys** (`.prd/tui-phase4/receipts/d4d`, "Up/Down
    caret edge"; `.prd/tui-phase4/receipts/e1`). Two keys changed meaning:

    * **Up is a step to the line's edge first.** From a caret that is not at
      the start of a one-row draft, Up moves it there and changes no text; the
      next Up is the recall. So every step back from the end of a line is two
      presses, each asserted on the screen it leaves -- the text and the caret.
    * **`C-p`/`C-n` are not history keys.** `C-p` is upstream's model catalog
      shortcut (`src/tui/shell.rs`'s `open_model_catalog`), and fx binds no
      history walk to either. The claim they carried -- the walk goes on from
      wherever the caret is on a line -- is made with Up instead, from the end
      of a recalled line.

    The two markers are the fixture's answers and nothing else says them, so a
    prompt echoed into the document can never satisfy a wait for one; the nonce
    is in both prompts, which is what makes the recalled composer rows the
    lines this run really sent rather than words that were on the screen.
    """
    first = run.marker("first")
    second = run.marker("second")
    fixture = start_fixture(
        run, [fixtures.content_only(first), fixtures.content_only(second)]
    )
    trial = run.trial("history", gateway=fixture).settled()
    from_a_known_composer(run, trial, "history")

    # The discriminator every scenario owes, and the first of the two prompts
    # the walk below is made of: `discriminate` submits `say <nonce>`.
    oldest = "say " + run.nonce
    discriminate(run, trial, fixture, first)
    trial.wait_until(
        "the composer to be empty after the first prompt",
        lambda _t: composer_text(trial.peek()) == "",
    )
    newest = "recall " + run.nonce
    trial.send(newest + "\r")
    trial.wait_for(second)
    trial.wait_until(
        "the composer to be empty after the second prompt",
        lambda _t: composer_text(trial.peek()) == "",
    )

    def composer_and_caret(grid):
        """The draft and the caret's column in it, or `None` with no composer."""
        row = composer_first_row(grid)
        if row is None or grid.row != row:
            return None
        return (composer_text(grid), grid.col - 2)

    def step(keys, text, column, label, what):
        """Sends `keys`, waits for the composer to read `text` with the caret at
        `column`, snapshots it and says whether it did."""
        trial.send(keys)
        trial.wait_until(
            what, lambda _t: composer_and_caret(trial.peek()) == (text, column)
        )
        grid = trial.grid(label)
        return run.require(
            composer_and_caret(grid) == (text, column),
            "%s: %r" % (what, composer_and_caret(grid)),
        )

    up, down = b"\x1b[A", b"\x1b[B"

    # --- the line that is typed and never sent ------------------------------
    draft = "unsent " + run.nonce
    trial.send(draft)
    trial.wait_until(
        "the composer to hold the line that is never sent",
        lambda _t: composer_and_caret(trial.peek()) == (draft, len(draft)),
    )
    typed = trial.grid("history-draft")
    run.require(
        composer_text(typed) == draft,
        "the draft the walk will be measured against: %r" % composer_text(typed),
    )

    # --- Up, from the end of the only row a one-row draft has ---------------
    #
    # The first press is the edge and nothing else: the draft is untouched and
    # the caret is at its start. The second is the recall, with the caret at
    # the **end** of the recalled line -- the claim the edit further down rests
    # on: a caret left in front of a recalled line would make the next
    # keystroke an insertion into the middle of it.
    step(up, draft, 0, "history-up-to-the-edge", "the first Up to take the caret to the draft's start")
    step(up, newest, len(newest), "history-up-newest", "the second Up to recall the newest line, caret at its end")
    run.require(
        not trial.peek().unknown,
        "xfx emitted only the sequences it declares: %r" % trial.peek().unknown,
    )

    # --- and on, from the end of a recalled line ----------------------------
    step(up, newest, 0, "history-up-recalled-edge", "Up from a recalled line's end to take the caret to its start")
    step(up, oldest, len(oldest), "history-up-older", "the next Up to reach the line before the newest one")

    # --- and the far end does not wrap --------------------------------------
    #
    # Two more steps back from the oldest line: the edge, and then nothing --
    # the oldest line stays, where a walk that wrapped would have brought the
    # newest line or the draft round again. Then forward, which is the step
    # that tells a refusal from a wrap that happened to land on the same text.
    step(up, oldest, 0, "history-oldest-edge", "Up at the oldest line to take the caret to its start")
    trial.send(up)
    step(down, oldest, len(oldest), "history-oldest-bound", "an Up past the oldest line to change nothing, and Down to take the caret to its end")
    step(down, newest, len(newest), "history-forward-from-the-oldest", "one step forward from the oldest line to reach the newest")

    # --- Down at the last row is the draft coming back ----------------------
    step(down, draft, len(draft), "history-draft-returned", "the draft the walk began from to come back")

    # --- an edit ends the walk ----------------------------------------------
    step(up, draft, 0, "history-second-walk-edge", "a second walk's first Up to take the caret to the draft's start")
    step(up, newest, len(newest), "history-second-walk-newest", "a second walk to reach the newest line")
    step(b"!", newest + "!", len(newest) + 1, "history-edited", "the recalled line to take the keystroke at its end")
    step(up, newest + "!", 0, "history-edited-edge", "Up from the edited line's end to take the caret to its start")
    step(up, newest, len(newest), "history-restarted", "the walk to start from the newest line again rather than carry on from it")
    step(down, newest + "!", len(newest) + 1, "history-edited-draft-returned", "the edited line to be what the walk comes back to")

    # --- and none of it sent anything ---------------------------------------
    run.require(
        len(fixture.bodies()) == 2,
        "a recall submitted the line it recalled: %d request(s) captured"
        % len(fixture.bodies()),
    )

    trial.send(b"\x15")
    trial.send(b"\x04")
    run.require(
        trial.session.wait_exit() == ("exited", 0),
        "Ctrl-D did not leave cleanly after walking the history",
    )


# ---------------------------------------------------------------------------
# 17b. leave, relaunch, resume
# ---------------------------------------------------------------------------

# What a resumed launch says first (`src/tui/shell.rs`'s `resumed`: a neutral
# notice under `session resumed`, upstream's `writeResumeNotice`,
# `app_session_runtime.zig:4657-4672`).
SESSION_RESUMED = "session resumed:"


def sessions_in(home):
    """The session ids the store under `home` holds, sorted."""
    try:
        return sorted(os.listdir(os.path.join(home, ".xfx", "sessions")))
    except OSError:
        return []


def concluded_turns(home):
    """How many turns the session logs under `home` say have concluded.

    The exit names a session only once its log says the turn is over
    (`tests/tui.rs`'s `wait_for_conclusions`), so a row that left straight after
    the answer was painted would be racing the log rather than reading it.
    """
    count = 0
    for session in sessions_in(home):
        try:
            with open(
                os.path.join(home, ".xfx", "sessions", session, "events.jsonl"), encoding="utf-8"
            ) as handle:
                lines = handle.read().splitlines()
        except OSError:
            continue
        for line in lines:
            try:
                frame = json.loads(line)
            except ValueError:
                continue  # a frame being appended as this read went past it
            if (frame.get("event") or {}).get("kind") == "turn_concluded":
                count += 1
    return count


def resume_handoff(trial):
    """The id the exit's `Continue session with: xfx --resume <id>` row names, or `None`."""
    grid = trial.grid("left")
    for row in range(grid.rows):
        text = grid.row_text(row).strip()
        if RESUME_HANDOFF.match(text):
            return text.rsplit(" ", 1)[1]
    return None


def scenario_17b(run):
    """A session left, the binary started again with `--resume`, and the conversation goes on.

    The restart half of the program's behaviour list
    (`.prd/07-fx-parity-program.md` § Phase 4 close-out, Receipt). The exit
    names the session (`receipts/c5`, `quit`: `Continue session with: xfx
    --resume <id>`), and the command it names is the one this row runs: the
    saved turn comes back on the screen -- its card and its answer, under the
    resume notice and with no welcome line, as upstream replays one
    (`replayHistoryToSinkIncremental`, `app_session_runtime.zig:4753-4810`) --
    the next prompt is sent with the saved one before it, no second session is
    made, and leaving again names the same one. `tests/tui.rs`'s
    `quit_after_a_conversation_names_the_session_and_resume_continues_it` is
    the same claim on the debug build; this is the release binary's.
    """
    first_marker = run.marker("first")
    second_marker = run.marker("second")
    fixture = start_fixture(
        run, [fixtures.content_only(first_marker), fixtures.content_only(second_marker)]
    )
    first_prompt = "first " + run.nonce
    before = run.trial("before-the-exit", gateway=fixture).settled()
    discriminate(run, before, fixture, first_marker, label="first-turn", prompt=first_prompt)
    before.wait_until(
        "the first turn's conclusion in its log",
        lambda _t: concluded_turns(before.home) >= 1,
    )
    before.send("/quit\r")
    run.require(before.session.wait_exit() == ("exited", 0), "the first session left at 0")
    before.session.settled_text()
    run.require(before.modes() == before.before, "the first session: termios byte-identical")
    named = resume_handoff(before)
    stored = sessions_in(before.home)
    run.require(
        named is not None and stored == [named],
        "the exit named the one session the store holds (%r, store %r)" % (named, stored),
    )
    if named is None:
        return

    resumed = run.trial(
        "resumed",
        gateway=fixture,
        home=before.home,
        workspace=before.workspace,
        args=["--resume", named],
    ).settled()
    on_the_grid(resumed, first_marker, "the saved answer replayed under the resume")
    grid = resumed.grid("replayed")
    run.require(
        grid.find(SESSION_RESUMED) is not None,
        "the relaunch says it resumed a session: %r" % (rows_above_band(grid),),
    )
    run.require(
        grid.find(RAIL + " " + first_prompt) is not None
        and grid.find("  " + first_marker) is not None,
        "the saved turn is on the screen again: its card and its answer (%r)"
        % (rows_above_band(grid),),
    )
    run.require(
        not any(is_welcome(text) for text in rows_above_band(grid) or []),
        "a resumed launch wrote no welcome line: %r" % (rows_above_band(grid),),
    )
    second_prompt = "second " + run.nonce
    resumed.send(second_prompt + "\r")
    on_the_grid(resumed, second_marker, "the resumed session's own answer")
    run.require(
        fixture.request_count() == 2
        and user_messages(fixture.requests()[1]) == [first_prompt, second_prompt],
        "the resumed turn was sent with the saved one before it: %r"
        % (user_messages(fixture.requests()[-1]),),
    )
    run.require(
        sessions_in(before.home) == [named], "the resume opened no second session"
    )
    resumed.wait_until(
        "the resumed turn's conclusion in its log",
        lambda _t: concluded_turns(before.home) >= 2,
    )
    resumed.send(b"\x04")
    run.require(resumed.session.wait_exit() == ("exited", 0), "the resumed session left at 0")
    resumed.session.settled_text()
    run.require(
        resume_handoff(resumed) == named, "leaving again named the same session"
    )
    run.require(resumed.modes() == resumed.before, "the resumed session: termios byte-identical")
    fixture.stop()


# What a provider switch finishes on, the provider's own name in it
# (`src/tui/shell.rs`'s `Switched to {} with {}. The next prompt starts a fresh
# conversation.`).
SWITCHED_TO_LLMUX = "* provider: Switched to llmux"


def scenario_18(run):
    """A provider switch reaches only the fixture it switched to.

    Two fixtures are up and exactly one may be asked. That is the whole design
    of this scenario: with a single fixture running, "the prompt went to the
    other provider" and "the prompt went nowhere" are the same observation, and
    a scenario that could not tell them apart would pass a build whose switch
    silently broke every turn.

    The other half is the **file**. A band that says `provider=llmux` over a
    profile that still says gateway is exactly the failure the transaction's
    reload step exists to prevent, so the profile is read off disk rather than
    inferred from the screen.
    """
    gateway_marker = run.marker("gateway")
    daemon_marker = run.marker("daemon")
    fixture = start_fixture(run, [fixtures.content_only(gateway_marker)])
    daemon = start_daemon(run, [fixtures.anthropic_answer(daemon_marker)])
    home = seeded_home(
        run,
        "switch",
        {
            "provider": "gateway",
            "llmux_url": daemon.url(),
            "models": {"gateway": "zai/glm-5.2"},
        },
    )
    trial = run.trial("switch", gateway=fixture, home=home).settled()
    from_a_known_composer(run, trial, "switch")

    before = profile_of(trial)
    run.require(
        before is not None and before["provider"] == "gateway",
        "the trial did not start on the provider it was seeded with: %r" % before,
    )

    # --- the switch --------------------------------------------------------
    #
    # **Restated for fx's `/provider`** (`.prd/tui-phase4/receipts/d3`,
    # `provider-switch`: `* provider: Preparing …`, then `* provider: Switched to
    # … with <model>. The next prompt starts a fresh conversation.`). `/setup`
    # is upstream's alias for `/provider` (`src/interactive.rs`'s registry), and
    # the line a switch finishes on is that sentence where it used to be
    # `[shell] provider=llmux`.
    trial.send("/setup llmux\r")
    # The line the switch is **finished** on. It is sent after the document was
    # committed and the configuration re-read from it, so waiting for it is
    # waiting for the reload rather than for the write -- which is the whole
    # difference between what the session will do and what the write intended.
    trial.wait_until(
        "the switch to report the provider it reloaded",
        lambda _t: trial.peek().find(SWITCHED_TO_LLMUX) is not None,
    )
    switched = trial.grid("switched")
    run.require(
        document_rows(switched, SWITCHED_TO_LLMUX + " with fable."),
        "the switch did not say what the session became, and with which model: %r"
        % switched.text(),
    )

    after = profile_of(trial)
    run.require(
        after is not None and after["provider"] == "llmux",
        "the band reported a switch the settings file does not agree with: %r" % after,
    )
    run.require(
        after.get("llmux_url") == daemon.url(),
        "the switch recorded a url other than the daemon it probed: %r" % after,
    )
    run.require(
        after.get("models", {}).get("gateway") == "zai/glm-5.2",
        "switching away from a provider lost the model chosen for it: %r" % after,
    )

    # --- and the next prompt goes to exactly one of them --------------------
    trial.send("say the marker\r")
    trial.wait_until(
        "the new provider's answer on the screen",
        lambda _t: daemon_marker in trial.peek().text(),
    )
    answered = trial.grid("answered")
    run.require(
        daemon_marker in answered.text(),
        "the daemon's answer is not on the screen",
    )
    run.require(
        gateway_marker not in answered.text(),
        "the provider that was switched away from answered: %r" % answered.text(),
    )

    asked = daemon.message_requests()
    run.require(
        len(asked) == 1,
        "the daemon was asked %d times rather than once" % len(asked),
    )
    run.require(
        "say the marker" in asked[0]["body"],
        "the daemon was asked something else: %r" % asked[0]["body"][:400],
    )
    run.require(
        fixture.request_count() == 0,
        "the provider that was switched away from was still asked %d time(s)"
        % fixture.request_count(),
    )
    trial.send(b"\x04")
    run.require(
        trial.session.wait_exit() == ("exited", 0),
        "Ctrl-D did not leave cleanly after a provider switch",
    )
    fixture.stop()
    daemon.stop()


def scenario_19(run):
    """The catalog browser renders context and effort, and the meter needs both.

    Four claims. The rows carry the **two columns a daemon publishes** and both
    shapes are driven -- a model with a window and efforts, and one with
    neither -- because a browser that only ever rendered the happy shape would
    not be shown to render an absence as an absence. The catalog is what
    supplies the hint row's **denominator**, the completed turn supplies its
    **numerator**, and with only one of the two on hand the row reports no
    percentage nobody measured.

    **Restated for fx's `/model`** (`.prd/tui-phase4/receipts/d2a`,
    `receipts/d2b`) **and fx's statusline** (`receipts/d4b`). A bare `/model`
    used to print `catalog=` and one `<id> context=… efforts=…` line per row
    into the document. It opens upstream's menu now: `Models N`, each row's
    name and its window (`1M context`) where one is published and nothing where
    none is; a row's efforts are the column Enter opens under it (`default`
    and the daemon's levels), and a row with none commits at once
    (`* Switched to …`). And the meter is upstream's `context` segment, off
    until it is turned on (`statusLine.context`, seeded here): over a window it
    reads `{used}k/{total}k {pct}%`, and with no window it reads the numerator
    alone (`receipts/d4b`, "The context shows the numerator alone, because the
    Gateway publishes no window") -- so "only one of the two" is now that
    numerator with no percentage, where it was no meter at all.
    """
    marker = run.marker("browse")
    plain_marker = run.marker("plain")
    catalog = {
        "models": [
            fixtures.catalog_model("claude-fable-5[1m]", ["fable"], ["low", "high"], 1000000),
            fixtures.catalog_model("plain-model", [], [], None),
        ]
    }
    daemon = start_daemon(
        run,
        [fixtures.anthropic_answer(marker), fixtures.anthropic_answer(plain_marker)],
        catalog=catalog,
    )
    home = seeded_home(
        run,
        "browse",
        {
            "provider": "llmux",
            "llmux_url": daemon.url(),
            "models": {"llmux": "fable"},
            "statusLine": {"context": True},
        },
    )
    trial = run.trial("browse", home=home).settled()
    from_a_known_composer(run, trial, "browse")

    # **Restated for the fx statusline** (`a50f8a0`): the meter used to read
    # `Context: 12k/1000k 1%`; upstream's hint row says `{used}k/{total}k
    # {pct}%` as its own segment (`render.zig:419-427`), so the meter is
    # recognised by that shape rather than by a label it no longer has --
    # which is what keeps "no meter before a turn" a claim that can fail.
    def meters(hint):
        return re.findall(r"(?:^| )\d+k/\d+k \d+%(?: |$)", hint or "")

    # --- nothing is claimed about context before anything has been measured -
    quiet = trial.grid("before-any-turn")
    run.require(
        hint_text(quiet) and not meters(hint_text(quiet)) and not re.search(r"\d+k$", hint_text(quiet)),
        "the hint row reported a context before a turn had measured one: %r"
        % hint_text(quiet),
    )

    # --- a bare /model browses ---------------------------------------------
    trial.send("/model\r")
    # Waited on the **last** row the browser will paint, not the first. The rows
    # arrive from the runtime thread and are painted in the provider's order, so
    # a wait satisfied by row one can be answered while row two is still owed --
    # which reads as "the browser did not render an absence" when what really
    # happened is that the reader looked too early.
    trial.wait_until(
        "the catalog rows to arrive from the runtime thread",
        lambda _t: "  plain-model" in (panel_rows_text(trial.peek()) or []),
    )
    browsed = trial.grid("browsed")
    rows = panel_rows_text(browsed) or []
    run.require(
        bool(rows) and rows[0].startswith("Models 2"),
        "the browser did not say how many rows it was shown: %r" % rows,
    )
    run.require(
        "  claude-fable-5[1m]  1M context" in rows,
        "a published window is not rendered beside its model: %r" % rows,
    )
    # The other shape a real daemon sends: no window is no column, rather than
    # a zero or a guess -- the row is the model's name and nothing after it.
    run.require(
        "  plain-model" in rows,
        "an absent window is not rendered as an absence: %r" % rows,
    )
    # The catalog alone is a denominator with no numerator, and that is not a
    # meter.
    run.require(
        not meters(hint_text(browsed)),
        "a catalog with no completed turn put a meter on the hint row: %r"
        % hint_text(browsed),
    )
    run.require(not browsed.unknown, "xfx emitted only the sequences it declares: %r" % browsed.unknown)

    # --- a published effort list is the column its row opens ----------------
    trial.send(b"\r")
    trial.wait_until(
        "the marked row's effort column",
        lambda _t: [text.strip() for text in (panel_rows_text(trial.peek()) or [])][:3]
        == ["default", "low", "high"],
    )
    efforts = trial.grid("effort-column")
    run.require(
        composer_text(efforts) == "/model claude-fable-5[1m]",
        "Enter on the row took it into the composer, its efforts under it: %r"
        % composer_text(efforts),
    )
    trial.send(b"\x1b")
    trial.wait_until(
        "the effort column to close", lambda _t: panel_rows_text(trial.peek()) == []
    )
    trial.send(b"\x15")
    trial.wait_until("the composer to be empty", lambda _t: composer_text(trial.peek()) == "")

    # --- one completed turn supplies the numerator -------------------------
    trial.send("say the marker\r")
    trial.wait_until(
        "the answer, and the usage that comes with it",
        lambda _t: marker in trial.peek().text(),
    )
    trial.wait_until(
        "the context meter on the hint row",
        lambda _t: meters(hint_text(trial.peek())),
    )
    measured = trial.grid("measured")
    hint = hint_text(measured) or ""
    # The fixture reports 12345 input tokens against a published window of
    # 1000000: thousands, and the percentage between them.
    run.require(
        hint.endswith(" · 12k/1000k 1%"),
        "the meter is not the fixture's own numbers over the catalog's window: %r" % hint,
    )
    run.require(
        len(meters(hint)) == 1,
        "the row carries more than one meter: %r" % hint,
    )

    # --- an empty effort list is no column, and no window is no percentage --
    trial.send("/model\r")
    trial.wait_until(
        "the menu to be up again",
        lambda _t: "  plain-model" in (panel_rows_text(trial.peek()) or []),
    )
    trial.send(b"\x1b[B\r")
    trial.wait_until(
        "a row with no efforts to commit at once",
        lambda _t: trial.peek().find("* Switched to plain-model") is not None
        and panel_rows_text(trial.peek()) == [],
    )
    trial.grid("plain-model-chosen")
    trial.send("say the other marker\r")
    trial.wait_until(
        "the second answer, and its usage",
        lambda _t: plain_marker in trial.peek().text()
        and (hint_text(trial.peek()) or "").endswith(" · 12k"),
    )
    unmeasured = trial.grid("no-window")
    run.require(
        (hint_text(unmeasured) or "").endswith("plain-model · 12k")
        and not meters(hint_text(unmeasured)),
        "a model with no published window was given a percentage nobody measured: %r"
        % hint_text(unmeasured),
    )
    run.require(
        len(daemon.message_requests()) == 2,
        "the browsing asked the daemon for an answer (%d requests)" % len(daemon.message_requests()),
    )
    trial.send(b"\x04")
    run.require(
        trial.session.wait_exit() == ("exited", 0),
        "Ctrl-D did not leave cleanly after browsing the catalog",
    )
    daemon.stop()


def scenario_21(run):
    """A collapsed paste is one thing to the keys, and comes back renumbered.

    Item 16 on a real terminal, and the three claims of `06-qa-harness.md`'s
    row 21 that only a terminal can make together:

    * **movement** steps over the summary as one unit, so the keystroke after a
      single `Left` lands in front of the whole of it rather than inside its
      name;
    * **delete** takes the whole block: one backspace at the right edge, and the
      screen is empty rather than holding a damaged name;
    * **history** hands the line back with its block under a number this session
      has not used, and what is *sent* is the paste rather than the words.

    The row's fourth clause -- the undo boundary -- is driven as a key in
    scenario 22 now that item 18 has one (`C-_`, `0x1f`; `C-z` is still not
    undo). The cargo receipt that a framed paste is exactly one entry however
    many reads it arrived in stays where it can count entries:
    `tui::shell::tests::one_framed_paste_is_one_transaction_however_many_reads_it_arrived_in`.

    The two markers are the fixture's own answers and nothing else says them, so
    a prompt echoed into the document can never satisfy a wait for one; the
    nonce is in both prompts, which is what makes the block on the wire this
    run's rather than words that were on the screen.

    **Restated for fx's summary** (`src/tui/paste.rs`'s `summary`, W2 round 8):
    a one-line block is `[Pasted text #N, 1 line]`, upstream's singular, where
    it used to read `1 lines`.
    """
    atomic = run.marker("atomic")
    recalled = run.marker("recalled")
    fixture = start_fixture(
        run, [fixtures.content_only(atomic), fixtures.content_only(recalled)]
    )
    trial = run.trial("paste-entities", gateway=fixture).settled()
    from_a_known_composer(run, trial, "paste-entities")

    def paste(block):
        trial.send(b"\x1b[200~")
        trial.send(block.encode("utf-8"))
        trial.send(b"\x1b[201~")

    # --- one backspace takes the whole block --------------------------------
    first = "y" * 1100
    paste(first)
    trial.wait_until(
        "the first paste to be summarized",
        lambda _t: composer_text(trial.peek()) == "[Pasted text #1, 1 line]",
    )
    collapsed = trial.grid("collapsed")
    run.require(
        composer_text(collapsed) == "[Pasted text #1, 1 line]",
        "a large paste is a summary on screen: %r" % composer_text(collapsed),
    )
    run.require(
        not collapsed.unknown,
        "xfx emitted only the sequences it declares: %r" % collapsed.unknown,
    )
    trial.send(b"\x7f")
    trial.wait_until(
        "one backspace to take the whole block",
        lambda _t: composer_text(trial.peek()) == "",
    )
    removed = trial.grid("backspaced")
    run.require(
        composer_text(removed) == "",
        "the backspace edited the summary instead of removing the block: %r"
        % composer_text(removed),
    )

    # --- the caret steps over the next one as one unit ----------------------
    #
    # And it is `#2`: a number a block was given is never given again, so the
    # one the backspace took is spent.
    second = "z" * 1100 + " " + run.nonce
    paste(second)
    trial.wait_until(
        "the second paste to be summarized",
        lambda _t: composer_text(trial.peek()) == "[Pasted text #2, 1 line]",
    )
    trial.send(b"\x1b[D")
    trial.send("see ")
    trial.wait_until(
        "the keystroke to land in front of the whole unit",
        lambda _t: composer_text(trial.peek()) == "see [Pasted text #2, 1 line]",
    )
    stepped = trial.grid("stepped-over")
    run.require(
        composer_text(stepped) == "see [Pasted text #2, 1 line]",
        "a left step landed inside the summary: %r" % composer_text(stepped),
    )
    # The caret is between what was typed and the unit it was typed in front
    # of, which is the cell claim the row cannot make: a `Left` that had stepped
    # *into* the summary would leave the same text on the row -- `see` would sit
    # one character inside the name -- and only the caret says which happened.
    marker_row = composer_first_row(stepped)
    run.require(
        (stepped.row, stepped.col) == (marker_row, 2 + len("see ")),
        "the caret is not beside the unit it was typed in front of: row %d col %d, "
        "composer on row %r" % (stepped.row, stepped.col, marker_row),
    )

    trial.send(b"\x0d")
    trial.wait_for(atomic)
    sent = user_messages(fixture.requests()[0])[-1]
    run.require(run.nonce in sent, "the nonce this run minted is in the request xfx sent")
    run.require(
        sent == "see " + second,
        "the block beside the keystroke is what was sent: %r" % sent[:60],
    )
    run.require(
        first not in sent,
        "the block one backspace removed was sent anyway",
    )
    answered = trial.grid("answered")
    run.require(answered.find(atomic) is not None, "the fixture's own marker is rendered")
    run.require(answered.text().strip() != "", "the screen is not blank")

    # --- and the recall renumbers it ----------------------------------------
    trial.send(b"\x1b[A")
    trial.wait_until(
        "the submitted line to come back under a fresh number",
        lambda _t: composer_text(trial.peek()) == "see [Pasted text #3, 1 line]",
    )
    renumbered = trial.grid("recalled")
    run.require(
        composer_text(renumbered) == "see [Pasted text #3, 1 line]",
        "the recalled line kept a number this session had already used: %r"
        % composer_text(renumbered),
    )
    trial.send(b"\x0d")
    trial.wait_for(recalled)
    resent = user_messages(fixture.requests()[1])[-1]
    run.require(
        resent == "see " + second,
        "the recalled summary was sent as the words it looks like: %r" % resent[:60],
    )
    run.require(
        "Pasted text" not in resent,
        "a summary reached the model in place of its block",
    )

    trial.send(b"\x15")
    trial.send(b"\x04")
    run.require(
        trial.session.wait_exit() == ("exited", 0),
        "Ctrl-D did not leave cleanly after the paste entities were driven",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 20. a change reviewed on a screen of its own
# ---------------------------------------------------------------------------


# One numbered row of the review's diff: the line's number in its file, its
# sign -- ` ` for context, `-` removed, `+` added -- and the line's own text
# (`.prd/tui-phase4/receipts/f2`, `edit-ask`: `      2 - alpha line 2`,
# `      2 + ALPHA LINE TWO`). A line too long for its row goes on in
# continuation rows with no number of their own, so a numbered row is one line
# of the file and a continuation row never is.
REVIEW_ROW = re.compile(r"^\s+(\d+) ([ +-]) (.*)$")


def review_lines(grid):
    """Every numbered row of the review on `grid`, top first, as `(number, sign, text)`."""
    out = []
    for row in range(grid.rows):
        found = REVIEW_ROW.match(grid.row_text(row))
        if found:
            out.append((int(found.group(1)), found.group(2), found.group(3)))
    return out


def has_line(grid, sign, text):
    """Whether one numbered row of the review is exactly `text` under `sign`.

    Row-exact rather than a substring, and the difference is load-bearing on
    this screen: the review's own header and the scope's disclosure quote the
    file's name and the tool inside sentences, so a substring would be
    satisfied by them and would never see the change at all.
    """
    return any((s, x) == (sign, text) for _number, s, x in review_lines(grid))


def scenario_20(run):
    """Large changes are reviewed on the alternate plane, and the band comes back.

    Phase-2 item 14 on a real terminal, judged on **cells of two grids**. Four
    claims, and each needs the widened oracle to be sayable at all:

    * the plane is taken once per question, and the change is on it;
    * the user's own screen -- the document above the band, and the band -- is
      untouched while it is up, which is a claim about the buffer the terminal
      *saved*, not about the one it is showing;
    * the answer gives the plane back and repaints the band, and the screen the
      session comes back to is the one it left;
    * **no frame in between shows a blank grid.** The end state is the same
      whether the restore was one write or two, so the middle is where the
      difference lives, and per-frame snapshots are the only place to look.

    Driven **twice, through the two mutations that have a before and an after**:
    an `edit_file` whose two strings outrun the summary, and a `write_file` that
    replaces the whole file. The second is not a repetition -- a whole-file write
    is the largest change this product makes, its "before" is the file that is
    there rather than a string the model sent, and it reached the alternate plane
    only when `execute_write_file` began carrying the pair. The two halves also
    discriminate each other: the edit's screen shows `alpha`/`beta` and the
    write's `beta`/`gamma`, so a screen showing the wrong change fails on a
    named cell rather than passing for looking right.

    **Restated for fx's review** (`.prd/tui-phase4/receipts/f2`). The review used
    to be a `before` heading over one side and an `after` heading over the
    other, opened at the head of the change. It is upstream's numbered diff now
    -- each line of the file a numbered row behind its sign, `-` for what goes
    and `+` for what comes -- above a dotted rule and the question, and it
    opens on the **tail** of the change against that rule ("the review's tail
    against the dotted rule"), with `C-p`/`C-n` walking it. So each claim that
    read a heading reads a sign instead, the first screenful is the tail and
    the walk goes up to the head; what each claim proves is unchanged.
    """
    marker = run.marker("altscreen")
    written_marker = run.marker("altscreen-write")
    spelled_marker = run.marker("altscreen-spelled")
    control_marker = run.marker("altscreen-control")
    written = fixtures.large_write_content()
    fixture = start_fixture(
        run,
        fixtures.large_edit_then_finish(marker)
        + fixtures.large_write_then_finish(written_marker)
        + fixtures.collision_write_then_finish(spelled_marker, written)
        + fixtures.control_write_then_finish(control_marker),
    )
    before, after = fixtures.large_edit_sides()
    spelled = fixtures.spelled_out_breaks(written)
    trial = run.trial(
        "alternate-screen",
        gateway=fixture,
        mode="ask",
        notes=True,
        notes_text=before + "\n",
    ).settled()

    # The screen the session is required to come back to, read before it goes
    # anywhere: this is the comparison the whole scenario turns on, and one
    # taken afterwards would be comparing the restore with itself.
    trial.send("edit the notes " + run.nonce + "\r")
    # The document row the tool wrote before the question, which is what makes
    # "the user's own screen survived" a claim with something on it.
    trial.wait_for(READ_NOTES)

    trial.wait_for(ALTERNATE_ENTER)
    # Waited for on the **grid** rather than on the wire, because `peek` reads
    # only as far as the last complete frame: the bytes that take the plane are
    # written in front of the frame that paints it, so a wait satisfied by the
    # title arriving can still be a wait that ends inside the paint.
    trial.wait_until(
        "the question to be painted on the plane it took",
        lambda _t: trial.peek().find(PERMISSION_TITLE) is not None,
    )
    grid = trial.grid("on-the-alternate-plane")

    run.require(grid.plane == "alternate", "the question is not on a plane of its own")
    run.require(
        grid.entered_alternate == 1,
        "the plane was taken %d times" % grid.entered_alternate,
    )
    run.require(grid.left_alternate == 0, "the plane was given back before it was answered")
    run.require(grid.find(PERMISSION_TITLE) is not None, "the screen does not name itself")
    run.require(
        grid.find("Edit · +20  -20") is not None,
        "the review does not say how much of the file the edit changes: %r" % grid.text(),
    )
    # **Line for line.** The review keeps the file's own breaks and gives each
    # one a numbered row (`permission::bounded_diff_side`), so twenty lines are
    # twenty rows rather than one wrapped run -- which is why the head of the
    # change is not on the first screenful, and why the walk below exists. The
    # first screenful is the tail, against the dotted rule: the last lines the
    # edit would put down, in order, each numbered as the file numbers it.
    shown = [number for number, sign, text in review_lines(grid) if sign == "+" and text == "beta line %d" % (number - 1)]
    run.require(
        len(shown) >= 4 and shown == list(range(shown[0], shown[0] + len(shown))) and shown[-1] == 20,
        "the first screenful is not the change's tail, line for line: %r" % review_lines(grid),
    )
    run.require(
        grid.find("beta line 18\\nbeta line 19") is None,
        "the file's line breaks were flattened into one run: %r" % grid.text(),
    )
    for choice in (CHOICE_MARK + APPLY_ONCE, DONT_APPLY):
        run.require(grid.find(choice) is not None, "the screen dropped %r" % choice)
    # And the other end is reached by walking, which is what a viewport onto a
    # change longer than a screen is for. Walked past its end on purpose: the
    # viewport clamps there (`ApprovalScreen::scroll_by`), so the head of the
    # change is a position this does not have to count its way to.
    trial.send(b"\x10" * 60)
    trial.wait_until(
        "the walk to reach the first line of the change",
        lambda _t: has_line(trial.peek(), "-", "alpha line 0"),
    )
    walked = trial.grid("walked-to-the-head-of-the-change")
    for number, needle in ((1, "alpha line 0"), (2, "alpha line 1"), (3, "alpha line 2")):
        run.require(
            (number, "-", needle) in review_lines(walked),
            "%r was not reached by walking the change as line %d" % (needle, number),
        )
    run.require(
        walked.plane == "alternate" and walked.left_alternate == 0,
        "walking the change gave the plane back",
    )
    run.require(
        read(trial.notes) == before + "\n", "the edit ran before it was approved"
    )
    # The user's own screen, which is the buffer the terminal saved rather than
    # the one it is showing. Nothing of the change is on it, and the band and
    # the document it had are still there.
    primary = grid.primary_text()
    run.require(
        "beta line" not in primary,
        "the change was painted on the user's own screen: %r" % primary,
    )
    run.require(
        READ_NOTES in primary,
        "the document the session had before the question is gone: %r" % primary,
    )
    run.require(
        HINT_MODEL in primary,
        "the band the restore has to repaint is not on the saved buffer: %r" % primary,
    )

    trial.send(b"1")
    trial.wait_for(marker)
    # A `require` rather than a wait, so a session that answers the question and
    # never gives the plane back fails on a named claim instead of on a timeout
    # whose message is about the harness.
    run.require(ALTERNATE_LEAVE in trial.text(), "the plane was never given back")
    run.require(read(trial.notes) == after + "\n", "`1` did not let the edit through")

    trial.wait_until(
        "the band to be painted on the primary plane again",
        lambda _t: trial.peek().plane == "primary"
        and HINT_MODEL in (hint_text(trial.peek()) or ""),
    )
    grid = trial.grid("back-on-the-primary-plane", record_frames=True)
    run.require(grid.plane == "primary", "the session is still on the borrowed plane")
    run.require(
        grid.entered_alternate == grid.left_alternate == 1,
        "the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    # Found up from the bottom-most painted row, because the band floats
    # under the document rather than owning the screen's last row: the hint
    # must be back **and** be the band's -- with the composer above it -- and
    # say what a turn runs with.
    run.require(
        find_band(grid) is not None and HINT_MODEL in (hint_text(grid) or ""),
        "the band's hint row was not repainted by the restore: %r" % hint_text(grid),
    )
    run.require(
        grid.find("beta line") is None,
        "the change stayed on the user's screen after the question was answered",
    )
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    # **Every** frame, not the last one: a restore written as two writes leaves
    # a frame between them holding the terminal's own restored buffer with no
    # band on it, and a scenario that only read the end state would pass.
    blank = [index for index, frame in enumerate(grid.frames) if not frame.strip()]
    run.require(
        not blank,
        "the session presented %d blank frame(s) at %r" % (len(blank), blank[:5]),
    )
    run.require(len(grid.frames) > 2, "no frames were recorded, so the check above proves nothing")

    # --- and the same for a whole-file write --------------------------------
    #
    # The other mutation with a before and an after, and the one whose "before"
    # is the file rather than a string the model sent. Same session, so the
    # plane is taken a **second** time and given back a second time: the pair
    # has to balance per question, not once per session.
    trial.send("rewrite the notes " + run.nonce + "\r")
    trial.wait_until(
        "the write's question to be painted on the plane it took",
        lambda _t: trial.peek().plane == "alternate"
        and trial.peek().find(PERMISSION_TITLE) is not None
        and has_line(trial.peek(), "+", "gamma line 19"),
    )
    grid = trial.grid("write-on-the-alternate-plane")

    run.require(grid.plane == "alternate", "the write's question is not on a plane of its own")
    run.require(
        grid.entered_alternate == 2 and grid.left_alternate == 1,
        "the plane was taken %d times and given back %d before the write was answered"
        % (grid.entered_alternate, grid.left_alternate),
    )
    run.require(
        grid.find(PERMISSION_TITLE) is not None and grid.find("Write · +20  -20") is not None,
        "the write's screen does not name itself as a write of twenty lines over twenty: %r"
        % grid.text(),
    )
    # **Both halves, and both are the write's own.** The "after" is the text
    # the write would put down -- the tail, on the first screenful -- and the
    # "before" is the file this session is replacing, which is the *edit's*
    # result, so a screen showing the edit again fails here.
    written_shown = [number for number, sign, text in review_lines(grid) if sign == "+" and text == "gamma line %d" % (number - 1)]
    run.require(
        len(written_shown) >= 4 and written_shown[-1] == 20,
        "the write's first screenful is not its tail, line for line: %r" % review_lines(grid),
    )
    for choice in (CHOICE_MARK + APPLY_ONCE, DONT_APPLY):
        run.require(grid.find(choice) is not None, "the write's screen dropped %r" % choice)
    trial.send(b"\x10" * 60)
    trial.wait_until(
        "the walk to reach the first line the write would replace",
        lambda _t: has_line(trial.peek(), "-", "beta line 0"),
    )
    walked = trial.grid("walked-to-the-file-the-write-replaces")
    for number, needle in ((1, "beta line 0"), (2, "beta line 1"), (3, "beta line 2")):
        run.require(
            (number, "-", needle) in review_lines(walked),
            "%r was not reached by walking the write as line %d" % (needle, number),
        )
    run.require(
        read(trial.notes) == after + "\n",
        "the write ran before it was approved",
    )
    primary = grid.primary_text()
    run.require(
        "gamma line" not in primary,
        "the write was painted on the user's own screen: %r" % primary,
    )

    trial.send(b"1")
    trial.wait_for(written_marker)
    run.require(
        read(trial.notes) == written,
        "`1` did not let the write through: %r" % read(trial.notes)[:60],
    )
    trial.wait_until(
        "the band to be painted on the primary plane after the write",
        lambda _t: trial.peek().plane == "primary"
        and HINT_MODEL in (hint_text(trial.peek()) or ""),
    )
    grid = trial.grid("back-on-the-primary-plane-after-the-write", record_frames=True)
    run.require(grid.plane == "primary", "the session is still on the borrowed plane")
    run.require(
        grid.entered_alternate == grid.left_alternate == 2,
        "the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    # Found up from the bottom-most painted row, because the band floats
    # under the document rather than owning the screen's last row: the hint
    # must be back **and** be the band's -- with the composer above it -- and
    # say what a turn runs with.
    run.require(
        find_band(grid) is not None and HINT_MODEL in (hint_text(grid) or ""),
        "the band's hint row was not repainted by the write's restore: %r" % hint_text(grid),
    )
    run.require(
        grid.find("gamma line") is None,
        "the write stayed on the user's screen after the question was answered",
    )
    run.require(
        grid.find(written_marker) is not None,
        "the write turn's own marker is rendered",
    )
    run.require(
        not grid.unknown,
        "xfx emitted only the sequences it declares: %r" % grid.unknown,
    )
    blank = [index for index, frame in enumerate(grid.frames) if not frame.strip()]
    run.require(
        not blank,
        "the write's restore presented %d blank frame(s) at %r" % (len(blank), blank[:5]),
    )

    # --- and a change only its line structure tells apart --------------------
    #
    # The **collision**: the file that is there and the same characters with
    # every line break spelled out as a backslash and an `n`. They are two
    # different files -- twenty lines against one -- and an escaping that spent
    # a backslash on a break without escaping the backslash itself rendered them
    # as one string, so this screen showed the whole change as a no-op. What is
    # asserted here is not that the two sides are different text: it is that
    # they are different **shapes**, which is the only thing a reader can see.
    trial.send("spell the breaks out " + run.nonce + "\r")
    trial.wait_until(
        "the collision's question to be painted on the plane it took",
        lambda _t: trial.peek().plane == "alternate"
        and trial.peek().find(PERMISSION_TITLE) is not None
        and trial.peek().find("Write · +1  -20") is not None,
    )
    grid = trial.grid("a-change-only-its-line-structure-tells-apart")

    run.require(grid.plane == "alternate", "the collision is not on a plane of its own")
    run.require(
        grid.entered_alternate == 3 and grid.left_alternate == 2,
        "the plane was taken %d times and given back %d before the collision was answered"
        % (grid.entered_alternate, grid.left_alternate),
    )
    # The side that only *spells* the breaks is **one** line: a single numbered
    # `+` row, its literal escapes shown as escapes, and the rest of it on
    # continuation rows that carry no number -- where the side whose breaks
    # are real is a numbered row per line (below).
    added = [line for line in review_lines(grid) if line[1] == "+"]
    run.require(
        [number for number, _sign, _text in added] == [1],
        "the spelled-out side is not one line of the file: %r" % review_lines(grid),
    )
    run.require(
        bool(added) and added[0][2].startswith("gamma line 0\\\\ngamma line 1\\\\n"),
        "the spelled-out break was not shown as a literal escape: %r" % (added,),
    )
    run.require(
        not has_line(grid, "+", "gamma line 1"),
        "the spelled-out breaks were given rows, so the two sides read alike: %r" % grid.text(),
    )
    run.require(
        read(trial.notes) == written,
        "the collision write ran before it was approved",
    )

    # And the side whose breaks are real is **rows**: walked to the head, each
    # of its lines is a numbered row of its own with nothing else on it.
    trial.send(b"\x10" * 60)
    trial.wait_until(
        "the walk to reach the first line the collision would replace",
        lambda _t: has_line(trial.peek(), "-", "gamma line 0"),
    )
    walked = trial.grid("the-side-whose-breaks-are-real-is-rows")
    for number, needle in ((1, "gamma line 0"), (2, "gamma line 1"), (3, "gamma line 2")):
        run.require(
            (number, "-", needle) in review_lines(walked),
            "%r is not a numbered row of its own on the side whose breaks are real: %r"
            % (needle, walked.text()),
        )

    trial.send(b"1")
    trial.wait_for(spelled_marker)
    run.require(
        read(trial.notes) == spelled,
        "`1` did not let the collision write through: %r" % read(trial.notes)[:60],
    )
    trial.wait_until(
        "the band to be painted on the primary plane after the collision",
        lambda _t: trial.peek().plane == "primary"
        and HINT_MODEL in (hint_text(trial.peek()) or ""),
    )
    grid = trial.grid("back-on-the-primary-plane-after-the-collision", record_frames=True)
    run.require(
        grid.entered_alternate == grid.left_alternate == 3,
        "the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    run.require(
        not grid.unknown,
        "xfx emitted only the sequences it declares: %r" % grid.unknown,
    )
    blank = [index for index, frame in enumerate(grid.frames) if not frame.strip()]
    run.require(
        not blank,
        "the collision's restore presented %d blank frame(s) at %r" % (len(blank), blank[:5]),
    )

    # --- and a change made of bytes the terminal would obey -------------------
    #
    # The payload arrives from the **fixture**, which is the model's side of the
    # wire, and is never typed: a raw `ESC` typed at a real terminal is an
    # escape sequence to the input decoder rather than content, so driving this
    # from the keyboard would be measuring the harness. Coming from a tool call
    # it is what it is in life -- bytes a model asked to be written into a file.
    #
    # Two claims the grid can make and a unit test cannot: the control is
    # **named by its code point** on the screen, so a reader is told which byte
    # it was rather than that one was there; and the emulator finds **no
    # sequence it does not know**, which is how "a hundred and sixty-one escape
    # bytes reached the review and none of them reached the terminal" is a
    # measurement rather than a hope. The pairwise claim -- that two *different*
    # controls stay two different screens -- is proven over the whole 65-scalar
    # domain in `cargo test`, at the boundary and at the renderer, because
    # driving it here would cost two more approval turns to say less.
    trial.send("write the control bytes " + run.nonce + "\r")
    trial.wait_until(
        "the control payload's question to be painted on the plane it took",
        lambda _t: trial.peek().plane == "alternate"
        and trial.peek().find(PERMISSION_TITLE) is not None
        and trial.peek().find("Write · +1  -1") is not None,
    )
    # The named bytes are the `+` line, which a hundred and sixty-one names
    # make taller than the viewport: its numbered head is reached by the walk,
    # the same walk the changes above use.
    trial.send(b"\x10" * 60)
    trial.wait_until(
        "the named control bytes' own line to be walked into the viewport",
        lambda _t: any(sign == "+" for _n, sign, _t2 in review_lines(trial.peek())),
    )
    grid = trial.grid("a-change-made-of-bytes-the-terminal-would-obey")

    run.require(grid.plane == "alternate", "the control payload is not on a plane of its own")
    run.require(
        grid.entered_alternate == 4 and grid.left_alternate == 3,
        "the plane was taken %d times and given back %d before the controls were answered"
        % (grid.entered_alternate, grid.left_alternate),
    )
    added = [text for _number, sign, text in review_lines(grid) if sign == "+"]
    run.require(
        bool(added) and added[0].startswith("\\u{001B}\\u{001B}"),
        "the escape byte was not named by its code point on the line it arrives in: %r"
        % (added,),
    )
    run.require(
        grid.find("�") is None,
        "a control was blanked to one symbol instead of being named: %r" % grid.text(),
    )
    # **Nothing executable reached the terminal.** The emulator fails the run on
    # any sequence it does not know, so an escape byte that had been passed
    # through rather than named would land here rather than on a reader.
    run.require(
        not grid.unknown,
        "a control byte reached the terminal: %r" % grid.unknown,
    )
    run.require(
        read(trial.notes) == spelled,
        "the control write ran before it was approved",
    )

    trial.send(b"1")
    trial.wait_for(control_marker)
    run.require(
        read(trial.notes) == "\x1b" * 161,
        "`1` did not let the control write through",
    )
    trial.wait_until(
        "the band to be painted on the primary plane after the controls",
        lambda _t: trial.peek().plane == "primary"
        and HINT_MODEL in (hint_text(trial.peek()) or ""),
    )
    grid = trial.grid("back-on-the-primary-plane-after-the-controls", record_frames=True)
    run.require(
        grid.entered_alternate == grid.left_alternate == 4,
        "the plane was taken %d times and given back %d"
        % (grid.entered_alternate, grid.left_alternate),
    )
    run.require(
        not grid.unknown,
        "xfx emitted only the sequences it declares: %r" % grid.unknown,
    )

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    settled = trial.session.settled_text()
    run.require(
        settled.count(ALTERNATE_ENTER) == settled.count(ALTERNATE_LEAVE) == 4,
        "the plane was entered %d times and left %d over the whole session"
        % (settled.count(ALTERNATE_ENTER), settled.count(ALTERNATE_LEAVE)),
    )
    run.require(
        settled.rfind(ALTERNATE_LEAVE) < settled.rfind(pty.RESTORE),
        "the exit restored the terminal before giving the plane back",
    )
    fixture.stop()


def scenario_22(run):
    """Undo, redo in both pinned spellings, the kill ring, and the boundary.

    Ladder item 18 on a real terminal. Six claims that only a terminal can make
    together, each read off the grid emulator rather than off the wire, because
    a frame is a difference and the row it changed is never on the wire in one
    piece:

    * a kill fills the **one** slot and `C-y` puts it back at the caret;
    * `C-_` undoes the **yank** rather than the kill, and the caret goes back
      with the text;
    * **redo** puts it back, once per spelling, each proved on its own round
      trip -- redo, assert the text *and* the caret, undo again -- so neither
      encoding rides on the other's work;
    * a near miss under the same final byte is not a redo, driven while redo
      still holds something so that accepting one would show;
    * `C-z` is **not** undo, and a recorded edit **clears** redo: after either,
      the keystroke behind the bytes proves the draft they did not touch;
    * a submit is a **boundary**: the draft it took is not undoable, and the
      undo after it changes nothing and brings nothing down.

    Redo has no control byte -- `shortcuts.zig`'s table has no arm for one -- so
    what a session receives for Super+Shift+Z is a CSI sequence, and a sequence
    is bytes: writing `ESC[122;10u` and `ESC[27;10;122~` into this pty is
    exactly what a terminal speaking either protocol would write
    (`runtime.zig:3018,3025`). What that does **not** prove is that a given
    terminal emits them for that chord -- a claim about terminals rather than
    about xfx, and one this scenario does not make.
    """
    marker = run.marker("edit-history")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    trial = run.trial("edit-history", gateway=fixture).settled()

    trial.send("one two")
    trial.wait_until(
        "the composer to hold what was typed",
        lambda _t: composer_text(trial.peek()) == "one two",
    )
    trial.grid("typed")

    # `row_text` strips the blanks at the end of a row, so the draft's own
    # trailing space is not part of what the grid reports.
    trial.send(b"\x17")  # C-w: the word goes into the kill slot
    trial.wait_until(
        "the word delete to take the last word",
        lambda _t: composer_text(trial.peek()) == "one",
    )
    grid = trial.grid("killed")
    run.require(
        composer_text(grid) == "one",
        "C-w took the last word: %r" % composer_text(grid),
    )

    trial.send(b"\x19")  # C-y: and it comes back
    trial.wait_until(
        "the yank to put the killed word back",
        lambda _t: composer_text(trial.peek()) == "one two",
    )
    grid = trial.grid("yanked")
    run.require(
        composer_text(grid) == "one two",
        "C-y yanked what C-w killed: %r" % composer_text(grid),
    )

    # The composer row is the band's own, found up from the hint (one-based
    # here, as the caret is reported) -- it floats under the document rather
    # than sitting on a fixed row -- and the caret's column is three for the
    # two-cell rail plus the bytes in front of it: 7 for `one ` and 10 for
    # `one two`. Read off the emulator's own cursor, which is the same oracle
    # the rows come from.
    caret_row = composer_first_row(grid) + 1

    def caret(grid):
        return (grid.row + 1, grid.col + 1)

    trial.send(b"\x1f")  # C-_: the undo takes back the yank, not the kill
    trial.wait_until(
        "the undo to take back the yank",
        lambda _t: composer_text(trial.peek()) == "one",
    )
    grid = trial.grid("undone-yank")
    run.require(
        composer_text(grid) == "one",
        "C-_ took back the yank rather than the kill: %r" % composer_text(grid),
    )
    run.require(
        caret(grid) == (caret_row, 7),
        "the caret went back with the undone text: %r" % (caret(grid),),
    )

    # Each spelling on its own round trip.
    for label, spelling in (
        ("csi-u", b"\x1b[122;10u"),  # runtime.zig:3018
        ("csi-tilde", b"\x1b[27;10;122~"),  # runtime.zig:3025
    ):
        trial.send(spelling)
        trial.wait_until(
            "the %s redo to put the yank back" % label,
            lambda _t: composer_text(trial.peek()) == "one two",
        )
        grid = trial.grid("redone-" + label)
        run.require(
            composer_text(grid) == "one two",
            "%s redid the yank: %r" % (label, composer_text(grid)),
        )
        run.require(
            caret(grid) == (caret_row, 10),
            "%s put the caret back where the yank left it: %r" % (label, caret(grid)),
        )
        run.require(
            (trial.session.last_frame() or "").endswith("\x1b[%d;10H" % caret_row),
            "the frame that painted the %s redo placed the caret itself" % label,
        )
        trial.send(b"\x1f")
        trial.wait_until(
            "the undo after the %s redo to take it back again" % label,
            lambda _t: composer_text(trial.peek()) == "one",
        )
        grid = trial.grid("undone-after-" + label)
        run.require(
            composer_text(grid) == "one" and caret(grid) == (caret_row, 7),
            "the undo after %s took back exactly that redo: %r %r"
            % (label, composer_text(grid), caret(grid)),
        )

    # A near miss under the same final byte is not a redo -- and redo still
    # holds the yank here, so a decoder that accepted one would show it.
    trial.send(b"\x1b[122;1u")  # no super bit
    trial.send(b"\x1b[122;010u")  # a spelling no terminal emits

    # C-z is not undo either. Upstream binds nothing to it and neither does this
    # decoder. Both claims are proved by the keystroke behind them: `!` reaches
    # a composer none of those four sequences touched.
    trial.send(b"\x1a")
    trial.send("!")
    trial.wait_until(
        "the keystroke after the near misses and C-z to reach the composer",
        lambda _t: composer_text(trial.peek()) == "one !",
    )
    grid = trial.grid("ctrl-z-and-near-misses-are-not-undo")
    run.require(
        composer_text(grid) == "one !",
        "a near miss or C-z changed the draft: %r" % composer_text(grid),
    )

    # And that recorded edit cleared redo, so the two real spellings mean
    # nothing now -- proved the same way.
    trial.send(b"\x1b[122;10u")
    trial.send(b"\x1b[27;10;122~")
    trial.send("?")
    trial.wait_until(
        "the keystroke after the two redo spellings to reach the composer",
        lambda _t: composer_text(trial.peek()) == "one !?",
    )
    grid = trial.grid("redo-cleared-by-an-edit")
    run.require(
        composer_text(grid) == "one !?",
        "a redo survived the edit that cleared it: %r" % composer_text(grid),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    # The undo stack below the cleared redo is still a stack: back over the two
    # typed characters, then over the kill.
    trial.send(b"\x1f")
    trial.send(b"\x1f")
    trial.send(b"\x1f")
    trial.wait_until(
        "the undo walk to reach the kill",
        lambda _t: composer_text(trial.peek()) == "one two",
    )
    grid = trial.grid("undone-kill")
    run.require(
        composer_text(grid) == "one two",
        "the undo walk did not reach the kill: %r" % composer_text(grid),
    )

    # A submit is a boundary: what it took is not undoable.
    discriminate(run, trial, fixture, marker, label="submitted")
    trial.wait_until(
        "the submit to empty the composer",
        lambda _t: composer_text(trial.peek()) == "",
    )
    trial.send(b"\x1f")
    trial.send(b"\x1b[122;10u")
    trial.send(b"\x1b[27;10;122~")
    trial.send("z")
    trial.wait_until(
        "the composer to hold only what was typed after the boundary",
        lambda _t: composer_text(trial.peek()) == "z",
    )
    grid = trial.grid("after-the-boundary")
    run.require(
        composer_text(grid) == "z",
        "an undo after a submit put a discarded draft back: %r" % composer_text(grid),
    )
    run.require(
        trial.session.state()[0] == "running",
        "an undo with nothing to undo brought the session down",
    )

    trial.send(b"\x15")
    trial.send(b"\x04")
    run.require(
        trial.session.wait_exit() == ("exited", 0),
        "Ctrl-D did not leave cleanly after the edit history was driven",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 23. the question the model asks
# ---------------------------------------------------------------------------


# The cancellation the tool hands the model when a batch is refused, byte for
# byte (`src/tools/question.rs`'s `CANCEL_SENTINEL`). Spelled out rather than
# imported, for the reason every needle in this suite is spelled out: a harness
# that read the constant it is checking would pass for whatever the module
# happened to declare.
QUESTION_CANCELLED = "(user cancelled the question)"

# The synthetic freeform choice every question grows, appended after the
# model's own options and never one of them.
FREEFORM_SLOT = "Other"

# What a question's record says once Escape or Ctrl-C has taken it down: the
# glyph and the word, and nothing after them (`.prd/tui-phase4/receipts/f4`,
# "question cancelled by Ctrl-C"; `receipts/f3`, `q2-esc`; `src/tui/question.rs`'s
# `CANCELLED`). It replaces the `stopping the turn` notice the interrupt wrote.
QUESTION_STOPPED = "\u25aa Cancelled"

# What the model is told about a question whose turn was stopped, in the next
# request (`receipts/f4`, "a question cancelled with Ctrl-C": the call's
# `error-text` result `aborted by user`; `receipts/f3` #4 for Escape).
QUESTION_ABORTED = "aborted by user"

# The column a choice's text starts in, behind its number (`receipts/f3`: `    1)
# Postgres`), which is where an open freeform draft is typed.
CHOICE_TEXT_COLUMN = 7

# What the sentence a user attached to a decision is read back as, in the
# transcript: the user's own card, the rail and a space before their words, as
# upstream writes it (`write_user_prompt`, `app_worker_runtime.zig:1020-1026`).
#
# Spelled out rather than imported, for the reason `ALWAYS_WORDING` is: a needle
# that read the constant it is checking would pass for whatever `src/tui/shell.rs`
# happened to declare.
AMENDMENT_CARD = RAIL + " "


def on_the_grid(trial, needle, what=None):
    """Waits until `needle` is on the **committed** grid, and returns that grid.

    Not `trial.wait_for`, and the difference is the whole reason this exists:
    the band paints a cell diff inside a synchronized frame, so a row that is
    really on the screen reaches the wire as the cells that changed with cursor
    moves between them, and a title that is on the screen may never appear in
    the stream as one run of bytes. `peek` feeds the emulator as far as the last
    **complete** frame, which is the only place a screen is a screen.

    **And the wire must end on that frame.** Bytes after a frame's close are a
    screen still being written: the exit's restore, a write outside the band's
    updates, or the head of the next update before its close has arrived. Since
    T8 a document append is inside the update that carries it
    (`fix/tui-sync-appends`), so this rarely waits -- but it was written when
    an append was its own write ahead of the band, which put the new line on
    the screen with the band as it was before it (an interrupt's notice above
    a question panel about to come down), and a needle read there is real
    while the screen around it is not one the session settled on. Kept, so
    that a build that split them again is waited out rather than read.
    """
    trial.wait_until(
        what or "%r to be on the screen" % needle,
        lambda _text: trial.peek().find(needle) is not None and wire_ends_on_a_frame(trial),
    )
    return trial.peek()


def wire_ends_on_a_frame(trial):
    """Whether the last bytes on the wire close a frame -- nothing appended after it.

    Either close (`pty.FRAME_CLOSE`): a frame that leaves the caret hidden is as
    complete as one that shows it. **And a bell after it is not an append**:
    `/sound`'s turn-end bell is written once the frame that ends the turn has
    been (`src/tui/check.rs`'s `Intent::Bell` -- no cell, no caret), so a wire
    that ends on a frame and then a `BEL` is a screen the session settled on.
    **Nor is a whole program status report** (`OSC 7501`, `src/tui/check.rs`'s
    `Intent::ProgramStatus` -- no cell, no caret): a report a keystroke caused
    is written after the frame that shows the keystroke, as upstream's next
    pass writes it (`event_loop.rs`'s `commit_tick`), and the session may
    settle there. Anything else after the close is still an append and still
    waited out.

    **A report is not proof that nothing follows it**, though: the session can
    write a report after one tick's frame and the next tick's frame a few
    milliseconds later. So a wire that ends on a frame only once a trailing
    report is set aside must also be quiet -- one more read that brings no byte
    in `REPORT_QUIET` -- before the screen counts as settled; bytes that did
    arrive are read and the wait goes on.
    """
    captured = bytes(trial.session.captured)
    reported = False
    while True:
        unrung = captured.rstrip(b"\x07")
        trimmed = TRAILING_REPORT.sub(b"", unrung)
        if trimmed == captured:
            break
        reported = reported or trimmed != unrung
        captured = trimmed
    framed = captured.endswith(FRAME_END_BYTES) or captured.endswith(FRAME_CLOSE_BYTES)
    if framed and reported and trial.session.pump(REPORT_QUIET):
        return False
    return framed


# A whole `OSC 7501` report at the very end of the wire.
TRAILING_REPORT = re.compile(rb"\x1b\]7501;[\x20-\x7e]*\x1b\\\Z")

# How long a wire that ends on a report has to stay still before the frame in
# front of the report counts as the screen the session settled on: six of the
# event loop's 8 ms ticks (`event_loop::TICK`).
REPORT_QUIET = 0.05


def cells_wide(text):
    """How many cells `text` takes, measured the way the band measures it.

    The oracle's own measurement, which is `unicode-width`'s: the caret the
    panel reports is a column count, so a harness that counted scalars would put
    the caret of a Korean draft several cells left of where the product
    correctly placed it, and would report that as a defect.
    """
    return sum(cluster_width(cluster) for cluster in clusters(text))


def choice_marked(grid, number, label):
    """Whether the question's choice `number) label` is the marked one.

    Upstream marks it by its pen alone -- the whole choice in bold, every other
    one dim, no marker glyph (`.prd/tui-phase4/receipts/f3`: "the marked choice
    `ESC[1mESC[38;5;255m`, every other `ESC[38;5;245m`, no marker glyph") -- so
    it is read off the style of the choice's number cell.
    """
    head = "    %d) " % number
    for row in range(grid.rows):
        text = grid.row_text(row)
        if text.startswith(head) and (label is None or text[len(head):].startswith(label)):
            return "1" in grid.styles[row][4].split(";")
    return False


def draft_caret(trial, slot):
    """The freeform draft on choice `slot` and the column its caret is in, or `None`.

    **The caret, not the marker.** The slot is marked by its pen whether or not
    a draft is open, so the mark says nothing about where a keystroke goes.
    What says the draft has the keys is the caret upstream draws in it: the
    draft is typed on the slot's own row, past its number, with a reversed blank
    cell where the next character goes (`receipts/f3`: `    4) ` and a reversed
    blank, `    3) MariaDB please` and the reversed blank after it). `None`
    unless exactly one such cell is on that row.
    """
    grid = trial.peek()
    head = "    %d) " % slot
    for row in range(grid.rows):
        text = grid.row_text(row)
        if not (text == head.rstrip() or text.startswith(head)):
            continue
        carets = [col for col in range(grid.cols) if "7" in grid.styles[row][col].split(";")]
        if len(carets) != 1:
            return None
        return (text[CHOICE_TEXT_COLUMN:], carets[0])
    return None


def tool_result_text(body, call_id):
    """The text of `call_id`'s tool result in a captured request, or `None`.

    Correlated by **id and role** rather than by searching the body for the
    tool's name: `ask_user_question` is advertised in the `tools` array of every
    request this suite captures, so picking a request by that substring picks
    the first one -- the request that *asked* the question, which carries no
    answer at all. A tool result is a `tool-result` part on a `tool` message
    (`src/gateway/protocol.rs`), and nothing else in a request is.

    **A refusal's text is its `reason`**: a call the user declined is told to
    the model as upstream's `execution-denied` output, which carries the
    sentence under `reason` rather than `value`
    (`.prd/tui-phase4/receipts/f2`, "Ctrl-C refuses only the call").
    """
    for message in json.loads(body).get("prompt", []):
        if message.get("role") != "tool":
            continue
        for part in message.get("content", []):
            if part.get("type") == "tool-result" and part.get("toolCallId") == call_id:
                output = part.get("output", {})
                if output.get("type") == "execution-denied":
                    return output.get("reason")
                return output.get("value")
    return None


def message_parts(body):
    """Every part of a captured request, in order, as `(role, kind, value)`.

    Order is the claim scenario 25 is about: upstream delivers an amendment as a
    user message **after** the tool result it is about
    (`orchestrator.zig:6598-6618`), because the wire merges consecutive user
    messages and hoists `tool_result` blocks to the front of the merged one -- so
    a sentence interleaved between two results would arrive above the result it
    answered.
    """
    out = []
    for message in json.loads(body).get("prompt", []):
        role = message.get("role")
        content = message.get("content", [])
        # A system message's content is one string, not a list of parts
        # (`src/gateway/protocol.rs`), and since M1 every request carries one:
        # the MCP advertisement, `<mcp_servers>` with `<none />` when nothing is
        # configured (`84805786`). It is one text part here, so the order this
        # reader reports is still the whole request's.
        if isinstance(content, str):
            content = [{"type": "text", "text": content}]
        for part in content:
            if part.get("type") == "tool-result":
                out.append((role, "tool-result", part.get("toolCallId")))
            elif part.get("type") == "text":
                out.append((role, "text", part.get("text")))
    return out


def amendment_after_result(body, call_id, phrase):
    """Whether `phrase` is a user message of `body`, exactly once, after `call_id`'s result.

    Both halves matter and neither is enough: a phrase that never arrived is a
    sentence the model was not told, and a phrase ahead of the result is a
    sentence about a call the model has not been shown yet.
    """
    parts = message_parts(body)
    results = [
        index
        for index, (_role, kind, value) in enumerate(parts)
        if kind == "tool-result" and value == call_id
    ]
    said = [
        index
        for index, (role, kind, value) in enumerate(parts)
        if role == "user" and kind == "text" and value == phrase
    ]
    return bool(results) and len(said) == 1 and max(results) < said[0]


def amended_request(fixture, call_id):
    """The first captured request that carries a result for `call_id`, or `None`.

    The **next** request after the answered call, which is the one an amendment
    travels in. Picked by correlation rather than by position, because a session
    that asked for one more turn than expected would otherwise be read as one
    that lost the sentence.
    """
    for body in fixture.bodies():
        if tool_result_text(body, call_id) is not None:
            return body
    return None


def approval_draft_caret(trial, choice):
    """The amendment draft on `choice`'s row, and the column its caret is in.

    **The caret, not the marker.** A `❯` in front of `1  Apply once` says the
    choice is marked and says nothing about where a keystroke goes. What says
    the draft has the keys is the caret upstream draws in it: the draft is typed
    on the marked answer's own row after its words and a comma, with a reversed
    cell where the next character goes -- on the placeholder's first letter
    while it is empty (`.prd/tui-phase4/receipts/f2`, `edit-amend-typed`: `❯ 1
    Apply once, use capitals`; "a reverse-video first cell"). `None` unless the
    row is there with exactly one such cell.
    """
    grid = trial.peek()
    head = "  " + CHOICE_MARK + choice + ", "
    for row in range(grid.rows):
        text = grid.row_text(row)
        if not text.startswith(head):
            continue
        carets = [col for col in range(grid.cols) if "7" in grid.styles[row][col].split(";")]
        if len(carets) != 1:
            return None
        return (text[len(head):], carets[0])
    return None


# What an open amendment draft says before anything is typed into it, after
# its answer's words (`src/tui/approval_view.rs`'s `amended_head`; upstream's
# `approval_ui.zig:1227-1260`, in this product's name).
AMEND_PLACEHOLDER_ALLOW = "and tell xfx what to do next"
AMEND_PLACEHOLDER_DENY = "and tell xfx what to do differently"


def carried_results(fixture, call_id):
    """Every captured request's result for `call_id`, in the order they were sent.

    A list rather than the last one, because "the model was told exactly this,
    once" is two claims and a scenario that read only the newest request could
    not make the second.
    """
    results = [tool_result_text(body, call_id) for body in fixture.bodies()]
    return [result for result in results if result is not None]


def scenario_23(run):
    """A real question tool call, answered by ordinal and by `Other`.

    Ladder item 19 on a real terminal, and the three claims no unit test can
    make together: the panel is a screen the user can read, the keys do what the
    screen says they do, and the ordered answer document reaches the **wire** as
    the next request's tool result.

    **Restated for fx's question** (`.prd/tui-phase4/receipts/f3`). The panel's
    choices are `1) Thorough` rather than `1. Thorough`, the marked one is told
    apart by its bold pen rather than a `> `, and which question of the batch
    this is is said at the right of the key help -- `Question 1 of 2`, shown
    only with room beside the keys, so this row is driven at 120 columns, where
    upstream shows it -- rather than after the title. `Other` opens its draft on
    its own row, the caret a reversed cell, where it used to open one under it.
    Once answered the question leaves its record in the document (`  1) Which
    depth?` / `     Thorough`), so "the panel came down" is the band without a
    panel and the record in its place.
    """
    marker = run.marker("question")
    fixture = start_fixture(run, fixtures.ask_then_finish(marker), name="question")
    trial = run.trial("question", gateway=fixture, mode="ask", cols=120).settled()
    trial.send("decide for me " + run.nonce + "\r")

    grid = on_the_grid(trial, "Which depth?", "the first question of the batch")
    trial.grid("panel")
    for expected, why in [
        ("    1) Thorough", "the first model option is offered by number"),
        ("    2) Quick", "so is the second"),
        ("    3) " + FREEFORM_SLOT, "the synthetic freeform slot is last"),
        ("reads every file", "the option's description is shown"),
    ]:
        run.require(grid.find(expected.strip()) is not None, why)
    title = grid.find("Which depth?")
    run.require(
        grid.row_text(title[0]) == "  Which depth?" and "1" in grid.styles[title[0]][2].split(";"),
        "the question is the panel's bold title: %r" % grid.row_text(title[0]),
    )
    run.require(
        (hint_text(grid) or "").endswith("Question 1 of 2"),
        "the batch says which question this is: %r" % hint_text(grid),
    )
    run.require(grid.find("Anything else?") is None, "one question is visible at a time")
    run.require(
        choice_marked(grid, 1, "Thorough") and not choice_marked(grid, 2, "Quick"),
        "the mark sits on the choice Enter would take, and only there",
    )
    band = find_band(grid)
    run.require(
        band is not None and band.composer == [] and band.activity is None,
        "the question replaced the composer and the turn's row: %r" % (band,),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    trial.send(b"1")  # a model ordinal submits at once
    grid = on_the_grid(trial, "Anything else?", "the second question of the batch")
    trial.grid("second")
    run.require(
        grid.find("Which depth?") is None,
        "the answered question left the screen when the next one arrived",
    )
    run.require(
        (hint_text(grid) or "").endswith("Question 2 of 2"),
        "and the batch moved on to its second question: %r" % hint_text(grid),
    )

    trial.send(b"3")  # `Other` only opens editing
    trial.wait_until(
        "the freeform slot to open a draft with the caret in it",
        lambda _text: draft_caret(trial, 3) == ("", CHOICE_TEXT_COLUMN),
    )
    freeform = "다시 묻지 마 — später"
    trial.send(freeform.encode("utf-8"))
    trial.wait_until(
        "the draft to hold what was typed",
        lambda _text: draft_caret(trial, 3) == (freeform, CHOICE_TEXT_COLUMN + cells_wide(freeform)),
    )
    grid = trial.grid("freeform")
    run.require(
        draft_caret(trial, 3) == (freeform, CHOICE_TEXT_COLUMN + cells_wide(freeform)),
        "the draft holds the text and the caret is past it: %r" % (draft_caret(trial, 3),),
    )
    run.require(
        choice_marked(grid, 3, None),
        "and the slot the draft belongs to is still the marked choice",
    )
    trial.send(b"\r")

    grid = on_the_grid(trial, marker, "the turn to carry on past the answered batch")
    grid = trial.grid("answered")
    carried = carried_results(fixture, "call-0")
    run.require(
        len(carried) == 1,
        "exactly one request carried a result for the question (%d did)" % len(carried),
    )
    run.require(
        carried
        and json.loads(carried[0])
        == [
            {"question": "Which depth?", "answer": "Thorough"},
            {"question": "Anything else?", "answer": freeform},
        ],
        "the next model request carries both answers, in question order, unclipped: %r" % carried,
    )
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    run.require(
        grid.text().count(marker) == 1,
        "response-only: the marker the fixture minted appears once on the screen",
    )
    run.require(
        not any(marker in body for body in fixture.bodies()),
        "and it was never typed at this session: no request xfx sent carries it",
    )
    record = rows_above_band(grid) or []
    run.require(
        "  1) Which depth?" in record
        and "     Thorough" in record
        and "  2) Anything else?" in record
        and "     " + freeform in record,
        "the panel came down and the document holds the answered record: %r" % (record,),
    )
    band = find_band(grid)
    run.require(
        band is not None and band.panel == [] and band.composer != [],
        "the panel is not left standing behind the answer: %r" % (band,),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()


def scenario_23b(run):
    """Escape and Ctrl-C take a question down and stop the turn it belonged to.

    The two keys that take a question down without answering it, and the calls
    queued behind the question in that same completion are work nobody asked
    for any more (`src/agent/machine.rs`'s `execute_tool_calls`).

    **Restated for fx's question** (`.prd/tui-phase4/receipts/f3`: "Escape and
    Ctrl-C cancel the turn on both ... The model is never sent the sentinel";
    `receipts/f4`, "question cancelled by Ctrl-C"). Escape used to be an answer
    about the call -- the byte-exact `(user cancelled the question)` sentinel,
    and the turn carrying on to its own conclusion -- and it is upstream's
    cancel now, the same as Ctrl-C: the question's record says `▪ Cancelled`,
    the turn asks the model for nothing more, and the model hears about it in
    the next prompt's request, as the call's `aborted by user`. That claim
    replaces the sentinel's; the sentinel is asserted never to reach the wire.

    The interrupt half is driven twice on purpose. Once answered, so the write
    behind the question is **observed to run**, and once interrupted, so the
    same write is observed not to: a file that never appears proves nothing
    about an interrupt unless the run that was not interrupted wrote it.
    """
    marker = run.marker("cancelled")
    fixture = start_fixture(
        run,
        [fixtures.ask_then_finish(marker)[0], fixtures.content_only(marker)],
        name="cancelled",
    )
    trial = run.trial("cancelled", gateway=fixture, mode="ask").settled()
    trial.send("decide for me " + run.nonce + "\r")
    on_the_grid(trial, "Which depth?", "the question Escape is about to take down")
    trial.grid("panel")

    trial.send(b"\x1b")
    grid = on_the_grid(trial, QUESTION_STOPPED, "the question's cancelled record")
    grid = trial.grid("after")
    run.require(grid.find("Which depth?") is None, "the panel came down on Escape")
    run.require(
        QUESTION_STOPPED in (rows_above_band(grid) or []),
        "the record is the glyph and the word, nothing after them: %r" % (rows_above_band(grid),),
    )
    run.require(
        fixture.request_count() == 1,
        "Escape stopped the turn: it asked the model for nothing more (%d requests)"
        % fixture.request_count(),
    )
    run.require(
        hint_text(grid) != EXIT_ARMED,
        "a key that took a question down armed the exit as well: %r" % hint_text(grid),
    )
    after = run.nonce + "-AFTER"
    trial.send("now say it " + after + "\r")
    grid = on_the_grid(trial, marker, "the fresh turn after the cancelled question")
    grid = trial.grid("next-turn")
    run.require(
        carried_results(fixture, "call-0") == [QUESTION_ABORTED],
        "the next request told the model the question was aborted, once: %r"
        % carried_results(fixture, "call-0"),
    )
    run.require(
        not any(QUESTION_CANCELLED in body for body in fixture.bodies()),
        "the old cancellation sentinel reached the wire",
    )
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(
        grid.text().count(marker) == 1,
        "response-only: the marker the fixture minted appears once on the screen",
    )
    run.require(
        not any(marker in body for body in fixture.bodies()),
        "and it was never typed at this session: no request xfx sent carries it",
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()

    # The same completion, answered: the call behind the question runs, and the
    # turn asks for its own conclusion. This is what makes the absence below an
    # observation about the interrupt rather than about a call that was never
    # going to happen.
    answered_marker = run.marker("answered")
    served = start_fixture(run, fixtures.ask_then_write(answered_marker), name="answered")
    answered = run.trial("answered", gateway=served).settled()
    answered.send("go on then " + run.nonce + "\r")
    on_the_grid(answered, "Proceed?", "the question the answered run takes")
    answered.send(b"1")
    on_the_grid(answered, answered_marker, "the answered turn to reach its conclusion")
    answered.grid("answered")
    written = os.path.join(answered.workspace, fixtures.QUESTION_FOLLOWER_PATH)
    run.require(
        os.path.exists(written) and read(written) == fixtures.QUESTION_FOLLOWER_CONTENT,
        "the call behind an **answered** question ran and wrote its file",
    )
    run.require(
        carried_results(served, "call-0") == ['[{"question":"Proceed?","answer":"Go ahead"}]'],
        "and the answer document reached the wire: %r" % carried_results(served, "call-0"),
    )
    run.require(
        served.request_count() == 2,
        "an answered turn asked for its own conclusion (%d requests)" % served.request_count(),
    )
    answered.send(b"\x04")
    run.require(answered.session.wait_exit() == ("exited", 0), "the answered session left at 0")
    served.stop()

    # And interrupted at the same question. One key, one message: the requester
    # answers the tool and hands the interrupt back, and the step is abandoned
    # **before** the next call of that completion is admitted.
    stopped_marker = run.marker("after-interrupt")
    scripted = start_fixture(run, fixtures.ask_then_write(stopped_marker), name="interrupted")
    stopped = run.trial("interrupted", gateway=scripted).settled()
    stopped.send("go on then " + run.nonce + "\r")
    on_the_grid(stopped, "Proceed?", "the question Ctrl-C is about to stop the turn at")
    stopped.grid("question")

    stopped.send(b"\x03")
    on_the_grid(stopped, QUESTION_STOPPED, "the interrupt to be answered on the screen")
    grid = stopped.grid("interrupted")
    run.require(grid.find("Proceed?") is None, "the panel came down on the interrupt")
    unwritten = os.path.join(stopped.workspace, fixtures.QUESTION_FOLLOWER_PATH)
    run.require(
        not os.path.exists(unwritten),
        "the call queued behind the interrupted question did not run",
    )
    run.require(
        scripted.request_count() == 1,
        "and the interrupted turn asked for nothing more (%d requests)"
        % scripted.request_count(),
    )
    run.require(
        stopped.session.state()[0] == "running",
        "a Ctrl-C at a question is a keystroke: the session is still alive",
    )

    # The session is still a session: a fresh prompt is answered, which is the
    # positive half of "the turn stopped" -- a hung requester would fail here
    # rather than at any absence above.
    after = run.nonce + "-AFTER"
    stopped.send("now say it " + after + "\r")
    grid = on_the_grid(stopped, stopped_marker, "the fresh turn after the interrupt")
    grid = stopped.grid("after-interrupt")
    run.require(
        scripted.request_count() == 2,
        "exactly one request followed the interrupt, and it was this prompt's (%d)"
        % scripted.request_count(),
    )
    run.require(
        any(after in body for body in scripted.bodies()),
        "the prompt typed after the interrupt is in the request xfx sent",
    )
    run.require(
        carried_results(scripted, "call-0") == [QUESTION_ABORTED],
        "and it told the model the question was aborted: %r" % carried_results(scripted, "call-0"),
    )
    run.require(
        grid.find(stopped_marker) is not None, "and the fixture's own marker is rendered"
    )
    run.require(
        not os.path.exists(unwritten),
        "the write behind the interrupted question did not run later either",
    )
    run.require(
        not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown
    )
    stopped.send(b"\x04")
    run.require(stopped.session.wait_exit() == ("exited", 0), "the interrupted session left at 0")
    scripted.stop()


def scenario_24(run):
    """A committed frame is what makes an affirmative answerable, and a resize
    takes that back until the next one lands.

    Synchronized on the **committed** grid throughout (`on_the_grid`), which is
    what makes this deterministic without a sleep: it feeds the emulator only as
    far as the last complete frame, so an assertion after it is an assertion
    about a screen the terminal really showed.

    **What this does not prove.** The pre-commit case -- a key pressed strictly
    between the request arriving and its first frame being written -- has no
    seam here: `on_the_grid` can only wait for something already on the screen,
    and the tool call is produced asynchronously, so a byte sent with the prompt
    can reach the composer before the request does and pass this for the wrong
    reason. That boundary is proven by the deterministic `commit_band` cases in
    `cargo test --lib tui::event_loop`, where a refused write is the cause and
    the receipt is read directly. Recorded as a limitation rather than papered
    over with timing.
    """
    marker = run.marker("readiness")
    fixture = start_fixture(run, fixtures.edit_then_finish(marker), name="readiness")
    trial = run.trial("readiness", gateway=fixture, mode="ask", notes=True).settled()
    trial.send("edit the notes " + run.nonce + "\r")

    # Positive control: the question is on the screen, whole, and `1` is taken.
    grid = on_the_grid(trial, PERMISSION_TITLE, "the approval frame to be committed")
    run.require(grid.find(ALWAYS_WORDING) is not None, "the always-scope is disclosed whole")
    run.require(grid.find(DONT_APPLY) is not None, "the refusal is on the screen")
    # The **tail** of the scope, which is the half a fixed two-row allotment cut
    # and the reason no ordinary screen could disclose an ordinary request.
    run.require(
        grid.find("of this saved session") is not None
        or grid.find("the approval ends with this command") is not None,
        "what `always` would buy was cut off, so nothing on this screen may be granted",
    )
    trial.grid("disclosed")
    trial.send(b"1")
    on_the_grid(trial, marker, "the turn to carry on past the granted call")
    run.require(
        carried_results(fixture, "call-0") != [],
        "the answered call produced no tool result, so the positive control proves nothing",
    )
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the readiness session left at 0")
    fixture.stop()

    # A resize revokes the receipt: the same key is refused until a frame for
    # the new screen has landed.
    second = start_fixture(run, fixtures.edit_then_finish(marker), name="resized")
    resized = run.trial("resized", gateway=second, mode="ask", notes=True).settled()
    resized.send("edit the notes " + run.nonce + "\r")
    on_the_grid(resized, PERMISSION_TITLE, "the approval frame before the resize")
    resized.resize(30, 100)
    resized.send(b"1")
    on_the_grid(resized, APPROVAL_NOT_READY, "the refusal of an answer to a moved screen")
    run.require(
        read(resized.notes) == "alpha\n",
        "the refused answer let the edit through anyway",
    )
    # **The question is still standing**, and this is a wait rather than a check
    # on the grid the notice arrived with: the notice is a document write, a
    # document write is a scroll, and the band is repainted by the frame after
    # it -- so there is one committed frame, between the two, that carries the
    # sentence and not yet the panel. That is the band's ordinary behaviour for
    # any line written while a panel is up rather than anything this gate does,
    # and asserting on the earlier of the two frames would be asserting that it
    # never happens.
    grid = on_the_grid(
        resized, PERMISSION_TITLE, "the question to be standing again after the refusal"
    )
    run.require(
        grid.find(DONT_APPLY) is not None,
        "the refused answer left a question with no visible way to refuse it",
    )
    resized.grid("revoked")
    on_the_grid(resized, ALWAYS_WORDING, "the repaint on the new screen")
    resized.send(b"1")
    on_the_grid(resized, marker, "the turn to carry on once the frame had landed")
    run.require(
        read(resized.notes) == "beta\n",
        "the answer after the repaint did not let the edit through",
    )
    resized.send(b"\x04")
    run.require(resized.session.wait_exit() == ("exited", 0), "the resized session left at 0")
    second.stop()


# ---------------------------------------------------------------------------
# 25. an amended approval
# ---------------------------------------------------------------------------


def scenario_25(run):
    """A decision the user amended, and where the sentence goes.

    The three claims no unit test can make together: the draft is a surface the
    user can read and type into on a real terminal, the decision it is attached
    to is unchanged by it, and the sentence reaches the **wire** as a user
    message behind the tool result it is about.

    Two trials, because an amendment means a different thing on each side of the
    decision and both have to be observed: allowed, the original arguments run;
    refused, nothing runs. The two phrases are distinct and appear nowhere else
    in the harness, so a grep across the evidence directory tells "the sentence
    reached the wire" from "the sentence was echoed on the screen".

    **Restated for fx's approvals** (`.prd/tui-phase4/receipts/f2`). The draft
    used to open on a row of its own under the answer; it is typed on the
    answer's own row now, after its words and a comma, over upstream's
    placeholder (`❯ 1  Apply once, and tell xfx what to do next`) with a
    reversed cell for its caret. And the third trial's key changed meaning:
    Ctrl-C at a question **refuses the call and the turn goes on**, where it
    used to stop the turn (`receipts/f2`, "Ctrl-C refuses only the call") -- so
    "the interrupted draft says nothing on the next turn" is now "the draft a
    Ctrl-C refused out of says nothing on the turn that goes on", observed on
    the very next request rather than on a fresh prompt's.
    """
    marker = run.marker("amendment")
    allow_phrase = "keep the header comment when you rewrite it"
    deny_phrase = "that path belongs to the deploy job"

    def asked(label, fixture_name):
        fixture = start_fixture(
            run, fixtures.amendable_write_then_finish(marker), name=fixture_name
        )
        trial = run.trial(label, gateway=fixture, mode="ask").settled()
        trial.send("draft the plan " + run.nonce + "\r")
        on_the_grid(trial, PERMISSION_TITLE, "the approval frame to be committed")
        return fixture, trial

    def written(trial):
        return os.path.join(trial.workspace, fixtures.AMENDMENT_PATH)

    def head(choice):
        return len("  " + CHOICE_MARK + choice + ", ")

    # -- trial A: allowed, with an amendment ------------------------------
    fixture, allowed = asked("allowed-with-amendment", "amend-allow")
    run.require(
        not os.path.exists(written(allowed)),
        "the write ran before it was approved, so nothing below is about an approval",
    )
    allowed.send(b"\t")
    allowed.wait_until(
        "the allow draft to open with the caret in it",
        lambda _text: approval_draft_caret(allowed, APPLY_ONCE)
        == (AMEND_PLACEHOLDER_ALLOW, head(APPLY_ONCE)),
    )
    allowed.send(allow_phrase.encode("utf-8"))
    allowed.wait_until(
        "the draft to hold what was typed",
        lambda _text: approval_draft_caret(allowed, APPLY_ONCE)
        == (allow_phrase, head(APPLY_ONCE) + cells_wide(allow_phrase)),
    )
    grid = allowed.grid("draft")
    run.require(
        approval_draft_caret(allowed, APPLY_ONCE)
        == (allow_phrase, head(APPLY_ONCE) + cells_wide(allow_phrase)),
        "the draft holds the text and the caret is past it: %r"
        % (approval_draft_caret(allowed, APPLY_ONCE),),
    )
    run.require(
        caret_on(allowed, APPLY_ONCE),
        "the answer the draft belongs to is still the marked choice",
    )
    run.require(
        grid.find(DONT_APPLY) is not None and grid.find(ALWAYS_WRITE_WORDING) is not None,
        "the draft cut an answer off the panel",
    )
    run.require(
        grid.find("for the rest of this session") is not None,
        "the draft cut what `always` would buy",
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    during = allowed.modes()
    run.require(during.is_raw(), "the terminal is raw while a draft has the keys")

    allowed.send(b"\r")
    on_the_grid(allowed, marker, "the turn to carry on past the amended approval")
    # **The transcript, not the draft.** The draft row came down with the panel;
    # what has to survive is the sentence the model was really told, written when
    # the runtime says it was delivered (`src/agent/machine.rs`'s
    # `flush_amendments`) rather than when the key was pressed.
    on_the_grid(
        allowed,
        allow_phrase,
        "the accepted sentence to be read back in the transcript",
    )
    grid = allowed.grid("answered")
    run.require(
        grid.find(AMENDMENT_CARD + allow_phrase) is not None,
        "the user's own sentence is not in the transcript, labelled as the user's own, after "
        "the panel came down: %r" % grid.text(),
    )
    run.require(
        grid.find(PERMISSION_TITLE) is None,
        "the sentence is still only on the draft, behind a panel that is still up",
    )

    run.require(
        os.path.exists(written(allowed))
        and read(written(allowed)) == fixtures.AMENDMENT_CONTENT,
        "the amended allow did not run the model's own arguments, byte for byte: %r"
        % (read(written(allowed)) if os.path.exists(written(allowed)) else None,),
    )
    body = amended_request(fixture, "call-0")
    run.require(body is not None, "no request carried a result for the amended call")
    run.require(
        body is not None and amendment_after_result(body, "call-0", allow_phrase),
        "the sentence did not reach the wire once, behind the result it was about: %r"
        % (message_parts(body) if body else None,),
    )
    run.require(
        body is not None and allow_phrase not in (tool_result_text(body, "call-0") or ""),
        "the user's sentence was merged into the tool's own report of what it did",
    )
    run.require(
        any(run.nonce in captured for captured in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    allowed.send(b"\x04")
    run.require(allowed.session.wait_exit() == ("exited", 0), "the amended session left at 0")
    run.require(
        allowed.modes() == allowed.before,
        "and the terminal came back byte for byte after a session that drafted",
    )
    fixture.stop()

    # -- trial B: refused, with an amendment ------------------------------
    refused_fixture, refused = asked("denied-with-amendment", "amend-deny")
    refused.send(b"\x1b[B\x1b[B")
    refused.wait_until(
        "the marker to reach the refusal", lambda _t: caret_on(refused, DONT_APPLY)
    )
    refused.send(b"\t")
    refused.wait_until(
        "the deny draft to open with the caret in it",
        lambda _text: approval_draft_caret(refused, DONT_APPLY)
        == (AMEND_PLACEHOLDER_DENY, head(DONT_APPLY)),
    )
    refused.send(deny_phrase.encode("utf-8"))
    refused.wait_until(
        "the deny draft to hold what was typed",
        lambda _text: approval_draft_caret(refused, DONT_APPLY)
        == (deny_phrase, head(DONT_APPLY) + cells_wide(deny_phrase)),
    )
    grid = refused.grid("deny-draft")
    run.require(
        approval_draft_caret(refused, DONT_APPLY)
        == (deny_phrase, head(DONT_APPLY) + cells_wide(deny_phrase)),
        "the deny draft holds the text and the caret is past it: %r"
        % (approval_draft_caret(refused, DONT_APPLY),),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    run.require(refused.modes().is_raw(), "the terminal is raw while the deny draft has the keys")

    refused.send(b"\r")
    on_the_grid(refused, marker, "the turn to carry on past the amended refusal")
    on_the_grid(
        refused,
        deny_phrase,
        "the refusal's sentence to be read back in the transcript",
    )
    grid = refused.grid("refused")
    run.require(
        grid.find(AMENDMENT_CARD + deny_phrase) is not None,
        "a refusal's own sentence is not in the transcript after the panel came down",
    )
    run.require(
        not os.path.exists(written(refused)),
        "a refusal with a sentence beside it wrote the file anyway",
    )
    body = amended_request(refused_fixture, "call-0")
    run.require(body is not None, "no request carried a result for the refused call")
    run.require(
        body is not None and amendment_after_result(body, "call-0", deny_phrase),
        "the refusal's sentence did not reach the wire once, behind its result: %r"
        % (message_parts(body) if body else None,),
    )
    run.require(
        body is not None and "not permitted" in (tool_result_text(body, "call-0") or ""),
        "the model was not told the call was refused: %r"
        % (tool_result_text(body, "call-0") if body else None),
    )
    run.require(
        body is not None and deny_phrase not in (tool_result_text(body, "call-0") or ""),
        "the user's sentence was merged into the refusal the tool reported",
    )
    run.require(
        allow_phrase not in json.dumps(refused_fixture.bodies()),
        "the allowed trial's sentence reached the refused trial's wire",
    )
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")
    refused.send(b"\x04")
    run.require(refused.session.wait_exit() == ("exited", 0), "the refusing session left at 0")
    run.require(
        refused.modes() == refused.before,
        "and the terminal came back byte for byte after a refusal that drafted",
    )
    refused_fixture.stop()

    # -- Ctrl-C at a filled draft says nothing on the turn that goes on ------
    cancelled_fixture, cancelled = asked("cancelled-draft", "amend-cancel")
    cancelled.send(b"\t")
    cancelled.wait_until(
        "the draft to open",
        lambda _text: approval_draft_caret(cancelled, APPLY_ONCE)
        == (AMEND_PLACEHOLDER_ALLOW, head(APPLY_ONCE)),
    )
    cancelled.send("never sent".encode("utf-8"))
    cancelled.wait_until(
        "the draft to hold what was typed",
        lambda _text: approval_draft_caret(cancelled, APPLY_ONCE) is not None
        and approval_draft_caret(cancelled, APPLY_ONCE)[0] == "never sent",
    )
    cancelled.grid("filled")
    cancelled.send(b"\x03")
    grid = on_the_grid(cancelled, marker, "the turn to go on past the call Ctrl-C refused")
    grid = cancelled.grid("after-ctrl-c")
    run.require(
        not os.path.exists(written(cancelled)),
        "the refused call wrote its file anyway",
    )
    run.require(
        grid.find("Denied file") is not None,
        "the call Ctrl-C refused says so in its row: %r" % grid.text(),
    )
    body = amended_request(cancelled_fixture, "call-0")
    run.require(
        body is not None and "not permitted" in (tool_result_text(body, "call-0") or ""),
        "the model was told the call was refused: %r"
        % (tool_result_text(body, "call-0") if body else None),
    )
    run.require(
        not any("never sent" in captured for captured in cancelled_fixture.bodies()),
        "a draft the user refused out of reached the wire on the turn that went on",
    )
    # **The display half of the same drop.** Showing it would tell the user
    # something was said that never was. The draft *was* on the screen before
    # the key (`grid-01-filled`), which is what makes this absence an
    # observation rather than a phrase that was never rendered at all.
    run.require(
        grid.find("never sent") is None,
        "the transcript claims a sentence was delivered that the key dropped",
    )
    run.require(
        grid.find(AMENDMENT_CARD + "never sent") is None,
        "a dropped amendment was labelled as delivered: %r" % grid.text(),
    )
    run.require(
        any(run.nonce in captured for captured in cancelled_fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)
    cancelled.send(b"\x04")
    run.require(
        cancelled.session.wait_exit() == ("exited", 0), "the refused session left at 0"
    )
    cancelled_fixture.stop()


# ---------------------------------------------------------------------------
# 26. layout stabilization
# ---------------------------------------------------------------------------


def draft_rows(grid):
    """Every row of an open composer draft, top first, gutter stripped.

    Not `composer_text` -- that joins rows with nothing between them, which is
    right for a draft that never had a line break in it and wrong for one that
    did: a paste with two embedded newlines is three *rows*, and joining them
    bare would compare `"FIRSTSECONDTHIRD"` against a draft that was never
    submitted as one word.
    """
    return composer_rows_text(grid)


def scenario_26(run):
    """Document content and order survive the composer's growth and shrink,
    and the band's steadiness survives an inline panel opening and closing.

    Three claims chained through one continuous session, because a fresh trial
    per stage would compare a screen against itself rather than against what an
    earlier stage really left behind:

    * a real answer writes three ordered lines into the terminal's own
      document, and a paste that grows the draft to three rows -- and an undo
      that shrinks it back without ever submitting it -- neither loses them nor
      reorders them. Read off `Grid.document_text()`, scrollback and screen
      together, because a scroll the growth or the shrink caused is exactly
      what this scenario is testing and a check that read only the screen would
      be blind to a line the growth had already carried into scrollback.
    * a real approval panel -- not a mockup, `edit_then_finish` answered by
      denying it -- opens and closes within the band's own owned region
      (`Geometry::panel_first`, `src/tui/layout.rs`: between the rule that
      heads it and the bottom rule over its key help), and the three lines from
      the stage before are still in the combined document, once each, in order,
      behind it. What is asserted about the band is its structure around the
      panel -- the question in place of the composer and the turn's row, the
      band's top directly under the document's blank row -- and that closing it
      gives the rows back with nothing left below.
    * the finish is ordinary: the draft is cleared before `C-D`, the idle
      screen is watched rather than assumed quiet, and the terminal comes back
      byte for byte.

    **Restated for the floating band** (`fcc8e44`, `a50f8a0`). This scenario
    used to assert literal coordinates on a band anchored to the bottom: a
    one-row baseline of divider 21 / composer 22, a three-row paste moving them
    up to 19 / 20, and a panel above the divider that left 21 / 22 alone. The
    band now floats under the document and the panel sits **under** the
    composer, so the coordinates are relative to where the band is found:
    the one-row baseline is the composer directly under the document's blank
    row with the blank rule row and the hint right below it; a three-row paste
    grows the band **downward** from that same first row (rule and hint pushed
    down by two); the undo puts the rule and the hint back and leaves nothing
    painted below them; and the panel opens between the rule that heads it and
    the bottom rule -- **in place of the composer and the turn's row**, as
    upstream's question replaces them (`src/tui/shell.rs`'s `fit`;
    `render_input.zig:705`), where it used to open under the composer with the
    activity row above it -- the band's top directly under the document's blank
    row, and, since this panel does not fit under a floating band, its key help
    on the screen's last row with the document carried into scrollback rather
    than painted over. Closing it gives the panel's rows back: one composer
    row, its blank rule row and the hint, with nothing left below.
    `20-alternate-screen-approval` is the case a panel this size never reaches:
    a change too large for the band to hold at all leaves the primary plane
    instead of growing further into it, a different mechanism, out of scope
    for the `edit_then_finish` fixture used here.

    **What this does not prove**, named rather than left implicit:

    * Whether a keystroke sent in the narrow window between the panel's
      request going out and its first frame landing reaches the composer or
      the panel is the same seam `24-approval-readiness` names and does not
      close: `on_the_grid` can only wait for a frame already committed, so no
      wait built from it can stand on the far side of that race. That boundary
      is proven instead, and only, by the deterministic `commit_band` cases in
      `cargo test --lib tui::event_loop`. Left there rather than claimed here.
    * Composer-yield-to-panel is not exercised here, and for a different
      reason than the readiness race above: by the time stage 2's question is
      sent, stage 1's draft has already been undone back to empty, and nothing
      is typed again until the question line itself is sent and submitted
      whole before any wait for the panel begins -- there is no draft standing
      in the composer for this scenario to observe yielding rows at all. That
      absence is `26b-composer-yield-to-panel`'s own scope: a tall draft is
      held pending with `fixture_server.Fixture`'s `release_when` gate, pasted
      and read off the grid while the request that would open the panel is
      provably still unanswered, and only then released -- so the question is
      read against a composer the harness knows was already tall, not one
      raced into being tall at the same moment.
    """
    doc1 = run.marker("doc-one")
    doc2 = run.marker("doc-two")
    doc3 = run.marker("doc-three")
    edit_marker = run.marker("edit")
    fixture = start_fixture(
        run,
        [fixtures.content_only(doc1 + "\n", doc2 + "\n", doc3 + "\n")]
        + fixtures.edit_then_finish(edit_marker),
    )
    trial = run.trial("layout", gateway=fixture, mode="ask", notes=True).settled()
    from_a_known_composer(run, trial, "layout")

    def survives(text, markers):
        """Whether every one of `markers` is in `text` exactly once, in order."""
        positions = [text.find(marker) for marker in markers]
        return (
            all(position >= 0 for position in positions)
            and positions == sorted(positions)
            and all(text.count(marker) == 1 for marker in markers)
        )

    # -- stage 1: three ordered lines, a paste-grown draft, a safe shrink ----
    trial.send("recite the three lines " + run.nonce + "\r")
    # Not `on_the_grid` alone: the answer is paced onto the screen, so a frame
    # that already carries the third line can still be one the band is
    # mid-repaint on (the activity row not yet folded back in). The marker's
    # presence is waited for first, as the positive half of the pair; what
    # actually says the turn has ended is `settled_band`'s `band_is_idle`
    # check, below -- divider position is not that evidence, and never was:
    # the divider sits at the one-row baseline whether a turn is running or
    # not, so it says nothing about settlement on its own.
    trial.wait_until(
        "the third document line to be on the committed grid",
        lambda _t: trial.peek().find(doc3) is not None,
    )
    settled_band(run, trial, "initial layout")
    grid = trial.peek()
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(
        survives(grid.document_text(), (doc1, doc2, doc3)),
        "the three lines reached the document once each, in order: %r" % (grid.document_text(),),
    )
    trial.grid("three-lines")
    baseline = find_band(grid)
    base = baseline.composer[0] if baseline is not None else -1

    def one_row_band(grid, what):
        band = find_band(grid)
        run.require(
            band is not None
            and band.composer == [base]
            and band.divider == base + 1
            and grid_row_blank(grid, base + 1)
            and band.hint == base + 2
            and grid.row_text(base - 1) == ""
            and SUMMARY_ROW.match(grid.row_text(base - 2)) is not None
            and all(grid_row_blank(grid, row) for row in range(base + 3, grid.rows)),
            "%s: one composer row directly under the document's blank row, the blank rule "
            "row and the hint right below it, nothing painted under them: %r" % (what, band),
        )

    one_row_band(grid, "the one-row composer's own rows, before anything grew it")

    draft_lines = [
        "FIRST-ROW-" + run.nonce,
        "SECOND-ROW-" + run.nonce,
        "THIRD-ROW-" + run.nonce,
    ]
    trial.send(b"\x1b[200~")
    trial.send("\n".join(draft_lines).encode("utf-8"))
    trial.send(b"\x1b[201~")
    trial.wait_until(
        "the pasted draft's three rows in the composer",
        lambda _t: draft_rows(trial.peek()) == draft_lines,
    )
    run.require(fixture.request_count() == 1, "the paste did not submit itself")
    grown = trial.grid("grown-three-rows")
    grown_band = find_band(grown)
    run.require(
        grown_band is not None
        and grown_band.composer == [base, base + 1, base + 2]
        and grown_band.divider == base + 3
        and grown_band.hint == base + 4,
        "the band grew downward for a three-row draft, its first row unmoved: %r" % (grown_band,),
    )
    run.require(
        grown.row == base + 2 and grown.col == 2 + cells_wide(draft_lines[-1]),
        "the caret sits past the pasted text on the draft's own last row: (%d, %d)"
        % (grown.row, grown.col),
    )
    run.require(
        survives(grown.document_text(), (doc1, doc2, doc3)),
        "the growth did not disturb the earlier document: %r" % (grown.document_text(),),
    )

    # Not `C-u`: `Action::KillToStart` (`editor.rs:351`) kills to the start of
    # the *line* the caret sits on -- one of the paste's three, delimited by
    # the real newlines it carried -- not the whole draft, so it would leave
    # the first two rows standing. The paste that grew this draft went in
    # through one `editor.insert()` call (`shell.rs:2597`), which is one
    # history entry, so `C-_` (`Action::Undo`, `input.rs:708`) reverses the
    # whole paste in the one step a shrink is supposed to be -- and is what a
    # person who pasted the wrong thing would actually reach for.
    trial.send(b"\x1f")  # C-_: undo the paste as the one edit it was
    trial.wait_until(
        "the composer to empty without submitting",
        lambda _t: composer_text(trial.peek()) == "",
    )
    run.require(fixture.request_count() == 1, "clearing the draft did not submit it")
    shrunk = trial.grid("shrunk-back")
    one_row_band(shrunk, "the band returned to the one-row baseline")
    run.require(
        not any(line in shrunk.text() for line in draft_lines),
        "the cleared draft left a row of itself painted",
    )
    run.require(
        survives(shrunk.document_text(), (doc1, doc2, doc3)),
        "the shrink did not disturb the earlier document: %r" % (shrunk.document_text(),),
    )

    # -- stage 2: a real approval panel opens within the band's own owned
    # region, then closes ----
    #
    # The panel's rows belong to the band, not the document: they sit between
    # the rule under the composer and the bottom rule over the hint
    # (`src/tui/layout.rs`'s footer order), and a panel too tall for a band
    # floating under this document pins the band to the bottom and carries the
    # document's top rows into the terminal's scrollback -- by a real scroll,
    # never by painting over them. The growth this stage proves is the panel's
    # own -- title, both choices and the tail-cut disclosure all committed at
    # once -- read against the band's structure around it, and the three lines
    # of stage 1 surviving in the combined document behind it.
    run.require(read(trial.notes) == "alpha\n", "the notes are unedited before any panel asked")
    trial.send("edit the notes " + run.nonce + "\r")
    trial.wait_for(READ_NOTES)
    panel = on_the_grid(trial, PERMISSION_TITLE, "the approval panel to be committed")
    trial.grid("panel-grown")
    run.require(panel.find(DONT_APPLY) is not None, "the refusal is on the screen")
    run.require(
        panel.find(ALWAYS_WORDING) is not None,
        "the always-scope is disclosed whole",
    )
    # **Restated for fx's question** (`src/tui/shell.rs`'s `fit`, "A question
    # takes them all"; `render_input.zig:705`; `.prd/tui-phase4/receipts/f1`).
    # The panel used to open under the composer with the turn's row above it;
    # upstream's question replaces both, so the band is the panel between its
    # two rules and its key help, and its top is the rule that heads it. A
    # file change has no row of its own until it is decided (`receipts/f1`:
    # "A file change has no row until it is decided"), so the document's last
    # written row is the read's.
    panel_band = find_band(panel)
    titled = [text for text in (panel_rows_text(panel) or []) if PERMISSION_TITLE in text]
    run.require(
        panel_band is not None
        and len(titled) == 1
        and panel_band.composer == []
        and panel_band.activity is None
        and is_rule(panel, panel_band.picker_rule)
        and is_rule(panel, panel_band.divider)
        and panel_band.hint == panel_band.divider + 1
        and panel_band.hint == panel.rows - 1,
        "the panel opens as the band -- between its two rules, the question in place of the "
        "composer and the turn's row, its key help on the last row: %r" % (panel_band,),
    )
    run.require(
        panel_band is not None
        and panel.row_text(panel_band.top - 1) == ""
        and panel.row_text(panel_band.top - 2) == "\u2514 " + READ_NOTES,
        "the band's top is directly under the document's blank row, its last written row the "
        "read's, which it did not paint over: %r"
        % ([panel.row_text(row) for row in range(max(panel_band.top - 2, 0), panel_band.top)] if panel_band else None,),
    )
    run.require(
        survives(panel.document_text(), (doc1, doc2, doc3)),
        "the panel's own growth did not disturb the earlier document: %r" % (panel.document_text(),),
    )

    trial.send(b"3")  # deny
    grid = on_the_grid(trial, edit_marker, "the turn's own marker past the refused call")
    # The marker lands while the turn still owns the band, and on a floating
    # band its line can sit where the activity row was until the next frame
    # (`band_is_idle`): what is read off `closed` is final geometry, so it is
    # taken only once the turn has given the band back.
    settled_band(run, trial, "panel closed")
    closed = trial.grid("panel-closed")
    run.require(
        closed.find(PERMISSION_TITLE) is None,
        "the panel is still standing behind the answer",
    )
    closed_band = find_band(closed)
    run.require(
        closed_band is not None
        and closed_band.panel == []
        and len(closed_band.composer) == 1
        and closed_band.divider == closed_band.composer[0] + 1
        and closed_band.hint == closed_band.divider + 1
        and closed.row_text(closed_band.top - 1) == ""
        and SUMMARY_ROW.match(closed.row_text(closed_band.top - 2)) is not None
        and all(grid_row_blank(closed, row) for row in range(closed_band.hint + 1, closed.rows)),
        "the band gave the panel's rows back: one composer row under the document, its "
        "blank rule row and the hint, nothing left below: %r" % (closed_band,),
    )
    run.require(read(trial.notes) == "alpha\n", "the denied edit ran anyway")
    body = amended_request(fixture, "call-1")
    refusal = tool_result_text(body, "call-1") if body else None
    run.require(
        body is not None and refusal is not None and "not permitted" in refusal,
        "the model was not told the edit was refused: %r" % refusal,
    )
    run.require(
        any(run.nonce in captured for captured in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.find(edit_marker) is not None, "the fixture's own marker is rendered")
    run.require(
        grid.text().count(edit_marker) == 1,
        "response-only: the marker the fixture minted appears once on the screen",
    )
    run.require(
        not any(edit_marker in captured for captured in fixture.bodies()),
        "and it was never typed at this session: no request xfx sent carries it",
    )
    run.require(
        survives(grid.document_text(), (doc1, doc2, doc3, edit_marker)),
        "all four markers survived the whole scenario once each, in order: %r"
        % (grid.document_text(),),
    )
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    # -- finish: draft cleared, idle watched, terminal restored -------------
    settled_band(run, trial, "final")
    trial.send(b"\x15")
    trial.wait_until(
        "the composer to be empty before the session is given up",
        lambda _t: composer_text(trial.peek()) == "",
    )

    # A positive marker before a negative watch, per `14-no-op-frame-skip`'s own
    # discipline: absence is only evidence once presence has been shown first.
    deadline = time.time() + pty.WAIT
    settled_len = len(trial.session.captured)
    quiet_since = time.time()
    while time.time() - quiet_since < QUIET_SETTLE:
        trial.session.pump(0.05)
        if len(trial.session.captured) != settled_len:
            settled_len = len(trial.session.captured)
            quiet_since = time.time()
        if time.time() > deadline:
            raise Failure("the session never stopped writing, so the idle screen cannot be measured")
    before_idle = len(trial.session.captured)
    idle = trial.grid("final-idle")
    run.require(idle.find(edit_marker) is not None, "the final marker is still on the idle screen")
    watch = time.time() + QUIET_WATCH
    while time.time() < watch:
        trial.session.pump(0.05)
    after_idle = len(trial.session.captured)
    run.require(
        after_idle == before_idle,
        "the idle screen wrote %d bytes across %.1fs with nothing to say: %r"
        % (after_idle - before_idle, QUIET_WATCH, trial.session.text()[before_idle:after_idle]),
    )

    run.require(trial.modes().is_raw(), "the terminal is raw before the session gives it back")
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the layout session left at 0")
    run.require(
        trial.modes() == trial.before,
        "the terminal came back byte for byte after a session that grew and shrank the band",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 26b. composer yield to an inline panel
# ---------------------------------------------------------------------------


def scenario_26b(run):
    """A tall draft really yields to an inline question, and gets its rows back.

    The gap `26`'s own docstring names: its panel opens against an empty
    composer, so nothing standing in the composer has a reason to give way.
    Reaching it needs a draft standing in the composer at the moment the panel
    would open -- and a wait built only from frames already on the wire
    (`on_the_grid`) cannot itself prove a paste landed *before* a race with the
    panel's own arrival, only that both eventually did. So the edit call's
    reply is held at the source instead: `fixtures.edit_then_finish`'s shape,
    scripted with a `release_when` gate (`fixture_server.Fixture._answer`)
    on the edit-call reply, lets the request that would open the panel be
    captured and confirmed still unanswered, *then* the draft pasted and read
    off the committed grid, and only then the gate opened -- a real barrier
    ordering the two rather than a sleep racing them.

    A 150-byte replacement sits below `permission::authority`'s
    `MAX_EXCERPT_BYTES` (160) -- the byte count `ApprovalSurface::for_request`
    admits before it routes a change to the alternate screen instead
    (`20-alternate-screen-approval`'s surface, out of scope here) -- so the
    question is asked in the band, on the plane the draft is on.

    **Restated for fx's question** (`src/tui/shell.rs`'s `fit`, "A question
    takes them all -- a permission question and the model's own batch alike:
    upstream's question replaces the composer rather than sitting under it";
    `render_input.zig:705`; `.prd/tui-phase4/receipts/f3`, "the composer gives
    the question its rows"). This row used to watch the draft give way one row
    at a time -- ten rows down to the five a fourteen-row panel left it, the
    tail shown. Upstream's question takes the composer's rows **whole**: while
    it is up there is no composer row and no row of the draft on the screen,
    and the turn's own row is off the band too. The claims are the same three
    in that form: the tall draft is really standing before the question, it
    really gives the question the footer -- not one row of it left painted --
    and it really gets all ten rows back, unedited, once the question is gone.
    """
    finish_marker = run.marker("finish")
    before, after = "a" * 150, "b" * 150
    release = threading.Event()
    script = [
        {"events": [fixtures.tool_call("call-0", "read_file", {"path": "notes.txt"}), fixtures.finish("tool-calls")]},
        {
            "events": [
                fixtures.tool_call(
                    "call-1", "edit_file", {"path": "notes.txt", "old_string": before, "new_string": after}
                ),
                fixtures.finish("tool-calls"),
            ],
            "release_when": release,
        },
        fixtures.content_only(finish_marker),
    ]
    fixture = start_fixture(run, script)
    trial = run.trial("yield", gateway=fixture, mode="ask", notes=True, notes_text=before + "\n").settled()
    from_a_known_composer(run, trial, "yield")

    trial.send("edit the notes " + run.nonce + "\r")
    trial.wait_for(READ_NOTES)
    trial.wait_until(
        "the edit call's own request to be captured and held",
        lambda _t: fixture.request_count() == 2,
    )

    draft_lines = ["ROW-%02d-%s" % (line, run.nonce) for line in range(10)]
    trial.send(b"\x1b[200~")
    trial.send("\n".join(draft_lines).encode("utf-8"))
    trial.send(b"\x1b[201~")
    trial.wait_until(
        "the ten-row draft to be on the committed grid while the edit reply is held",
        lambda _t: draft_rows(trial.peek()) == draft_lines,
    )
    held = trial.grid("draft-captured-while-held")
    run.require(fixture.request_count() == 2, "the paste neither submitted nor started a new request")
    run.require(held.find(PERMISSION_TITLE) is None, "no barrier: the panel painted before its reply was released")
    held_band = find_band(held)
    run.require(
        held_band is not None
        and len(held_band.composer) == 10
        and not composer_scrolled(held)
        and held_band.activity is not None
        and held_band.composer[0] == held_band.activity + 2
        and held_band.divider == held_band.composer[-1] + 1
        and grid_row_blank(held, held_band.divider),
        "the ten-row draft's own rows, before any panel -- all ten shown under the running "
        "turn's activity row, over the blank rule row and the hint: %r" % (held_band,),
    )
    run.require(
        held_band is not None
        and held.row == held_band.composer[-1]
        and held.col == 2 + cells_wide(draft_lines[-1]),
        "the caret sits past the pasted text's own last row: (%d, %d)" % (held.row, held.col),
    )

    release.set()
    panel = on_the_grid(trial, PERMISSION_TITLE, "the panel to be committed once the held reply is released")
    trial.grid("panel-open-over-tall-draft")
    run.require(panel.find(APPLY_ONCE) is not None, "the first choice is offered")
    run.require(panel.find(DONT_APPLY) is not None, "the refusal is offered")
    run.require(panel.find(ALWAYS_WORDING) is not None, "the second choice is offered")
    # The **tail** of the scope, separately from the second choice's own fixed
    # prefix (`ALWAYS_WORDING`): a screen could disclose the second choice and
    # still have cut the clause that says what "always" buys, the same
    # distinction `24-approval-readiness` draws for the identical reason.
    run.require(
        panel.find("of this saved session") is not None
        or panel.find("the approval ends with this command") is not None,
        "the scope's own tail is on the screen, not just the choice's fixed prefix",
    )
    panel_band = find_band(panel)
    run.require(
        panel_band is not None
        and panel_band.composer == []
        and panel_band.activity is None
        and is_rule(panel, panel_band.picker_rule)
        and any(PERMISSION_TITLE in text for text in (panel_rows_text(panel) or []))
        and is_rule(panel, panel_band.divider)
        and panel_band.hint == panel_band.divider + 1,
        "the question has the footer to itself -- its rules, its rows and its key help, no "
        "composer row and no activity row: %r" % (panel_band,),
    )
    left = [line for line in draft_lines if panel.find(line) is not None]
    run.require(
        not left and panel.find(RAIL + "↑") is None,
        "the draft gave the question every row it had -- none of it is left painted: %r" % left,
    )

    trial.send(b"3")  # deny
    grid = on_the_grid(trial, finish_marker, "the turn's own marker past the refused call")
    # The marker alone is not proof the band has finished settling back down --
    # it arrives while the turn can still own the document's last row
    # (`settled_band`'s own reasoning) -- and what is read off `closed` below
    # is final geometry (divider, composer rows, the whole restored draft), so
    # the snapshot is taken only once the band has given that row back.
    settled_band(run, trial, "yield")
    closed = trial.grid("panel-closed-draft-restored")
    run.require(closed.find(PERMISSION_TITLE) is None, "the panel is gone")
    closed_band = find_band(closed)
    run.require(
        closed_band is not None
        and closed_band.panel == []
        and len(closed_band.composer) == 10
        and not composer_scrolled(closed)
        and closed_band.divider == closed_band.composer[-1] + 1
        and grid_row_blank(closed, closed_band.divider)
        and closed_band.hint == closed_band.divider + 1
        and all(grid_row_blank(closed, row) for row in range(closed_band.hint + 1, closed.rows)),
        "the draft's own ten rows are back now that the panel gave them back, the blank rule "
        "row and the hint under them and nothing left below: %r" % (closed_band,),
    )
    run.require(
        draft_rows(closed) == draft_lines,
        "the whole draft survived the yield and the restore, unedited and in order: %r" % (draft_rows(closed),),
    )
    run.require(read(trial.notes) == before + "\n", "the denied edit ran anyway")
    body = amended_request(fixture, "call-1")
    refusal = tool_result_text(body, "call-1") if body else None
    run.require(
        body is not None and refusal is not None and "not permitted" in refusal,
        "the model was not told the edit was refused: %r" % refusal,
    )
    run.require(
        any(run.nonce in captured for captured in fixture.bodies()),
        "the nonce this run minted is in a request xfx sent",
    )
    run.require(grid.find(finish_marker) is not None, "the fixture's own marker is rendered")
    run.require(not grid.unknown, "xfx emitted only the sequences it declares: %r" % grid.unknown)

    # -- finish: the draft is undone (never submitted), then C-D ------------
    trial.send(b"\x1f")  # C-_: undo the paste as the one edit it was
    trial.wait_until(
        "the composer to empty without submitting the draft",
        lambda _t: composer_text(trial.peek()) == "",
    )
    run.require(fixture.request_count() == 3, "clearing the draft did not submit it")
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the yield session left at 0")
    run.require(
        trial.modes() == trial.before,
        "the terminal came back byte for byte after a panel that shrank and restored the composer",
    )
    fixture.stop()


# ---------------------------------------------------------------------------
# 27. live theme retint
# ---------------------------------------------------------------------------

# The six greys the document's own three roles are painted in, spelled out here
# rather than read off anything the product shares with this file. They are
# `src/tui/theme.rs`'s `DARK`/`LIGHT` tables at the roles `BODY` (index 4, the
# grey a card's rail and the welcome line's brand are in), `NOTICE` (index 2)
# and `DIM` (index 6, a turn's summary and the welcome line's version) -- the
# three `Palette::document_retint` moves -- spelt `38;5;N`, the one spelling
# the palette has whatever the terminal claims (`src/tui/theme.rs` `Palette`,
# as upstream's builtins are).
#
# A literal, because the question this scenario asks is *which* grey a cell
# ended up in. A harness that built the expectation from a palette helper would
# agree with whatever the module happened to declare, and one that searched the
# captured bytes for an SGR would be satisfied by the same sequence opened for
# some other row: the reading below is the attribute of one **named cell**.
RETINT_DARK_BODY = "\x1b[38;5;255m"
RETINT_DARK_NOTICE = "\x1b[38;5;250m"
RETINT_LIGHT_BODY = "\x1b[38;5;235m"
RETINT_LIGHT_NOTICE = "\x1b[38;5;241m"
RETINT_DARK_DIM = "\x1b[38;5;245m"
RETINT_LIGHT_DIM = "\x1b[38;5;247m"
RETINT_DARK = (RETINT_DARK_BODY, RETINT_DARK_NOTICE, RETINT_DARK_DIM)
RETINT_LIGHT = (RETINT_LIGHT_BODY, RETINT_LIGHT_NOTICE, RETINT_LIGHT_DIM)

# What a terminal that took the `?2031` subscription volunteers when its
# background changes (`src/tui/theme.rs:109-127`). Nobody typed it, so every
# assertion about a draft below is also an assertion that it stayed out of the
# composer.
WENT_LIGHT = b"\x1b[?997;2n"
WENT_DARK = b"\x1b[?997;1n"

# The command whose answer is used as this scenario's `Role::Notice` row
# (`src/tui/shell.rs`'s `version`, `Shell::say`).
#
# Chosen for what it does **not** do. `/model` answers with a second, later line
# once the catalog comes back and `/new` submits work whose end the band shows,
# so both leave the document still moving after the line under test has landed
# -- and a row that is about to be scrolled is not a row a colour can be
# compared on. `/version` is one local sentence, said and finished: no request,
# no worker, no activity row painting over the bottom of the document.
NOTICE_COMMAND = "/version"

# What its answer starts with: upstream's `* version: 0.0.13`, the bare version
# under the notice's topic (`src/tui/shell.rs`'s `version`, `app_commands.zig:2153-
# 2160`; `.prd/tui-phase4/receipts/c3`). A prefix, because the version is the
# build's and not this file's.
NOTICE_PREFIX = "* version: "

# The warning grey a cancelled line's `▪` is painted in, in each palette
# (`src/tui/theme.rs`'s `WARNING`, `DARK`/`LIGHT` at index 9).
RETINT_DARK_WARNING = "\x1b[38;5;252m"
RETINT_LIGHT_WARNING = "\x1b[38;5;238m"


def cell_attribute(grid, needle):
    """The attribute of the **first cell** of `needle`, or `None` if it is gone.

    The whole colour question of this scenario, asked of one cell. `find`
    answers a cell index, so this is the attribute the terminal is really
    holding for the first grapheme of a marker -- not a substring of the row and
    not a sequence seen somewhere in the stream.
    """
    found = grid.find(needle)
    if found is None:
        return None
    row, column = found
    return grid.attrs[row][column]


def own_line_row(grid, text):
    """The row whose **whole** text is `text`, or `None`.

    An equality rather than a search, because the row this is used for is the
    card a submitted prompt is echoed in, and the screen also carries other
    rows with the same words in them. A reader that matched a substring could
    read the colour of xfx's line and report it as the user's.
    """
    for row in range(grid.rows):
        if grid.row_text(row) == text:
            return row
    return None


def notice_row(grid):
    """The row xfx answered [`NOTICE_COMMAND`] on, or `None`.

    Read off the document above the band, as the one row its notice's own
    topic opens (`* version: `). **Restated for fx's dispatch**
    (`.prd/tui-phase4/receipts/c0`, `version`: "There is no echo of `/version`
    on either"): this used to be the first written row under the command's
    card, read that way because the answer opened with the word the band's
    hint row also carries. A slash command leaves no card now, and its answer
    opens with a topic nothing else on the screen does -- the hint row
    included -- so the row is found by it, and only above the band.
    """
    band = find_band(grid)
    limit = band.top if band is not None else grid.rows
    for row in range(limit):
        if grid.row_text(row).startswith(NOTICE_PREFIX):
            return row
    return None


def rows_above_the_rule(grid):
    """The rows above the band: the terminal's own document, as text.

    Named for where it reads rather than for what is there, because
    `document_rows` above is a different question (`17-history`'s "which rows
    carry this needle") and two helpers with one name is a scenario quietly
    calling the other one. **Restated for the floating band** (`a50f8a0`): the
    boundary is the band's top as `find_band` reads it, where it used to be the
    always-drawn rule.
    """
    band = find_band(grid)
    last = grid.rows if band is None else band.top
    return [grid.row_text(row) for row in range(last)]


def attrs_above_the_rule(grid):
    """The same rows' attribute runs, in order."""
    band = find_band(grid)
    last = grid.rows if band is None else band.top
    return [grid.attrs_of_row(row) for row in range(last)]


def card_rows(grid):
    """The visible document rows that are a card's -- behind the rail, above the band."""
    band = find_band(grid)
    last = grid.rows if band is None else band.top
    return [row for row in range(last) if grid.cells[row][0] == RAIL]


def rail_attributes(grid):
    """The colour of every visible card's rail cell, top first."""
    return [grid.attrs[row][0] for row in card_rows(grid)]


def card_text_attributes(grid):
    """Every colour a visible card's own text cells carry -- the user's words."""
    return {
        grid.attrs[row][col]
        for row in card_rows(grid)
        for col in range(2, len(grid.row_text(row)))
    }


def summary_attribute(grid):
    """The colour of the first visible turn summary's first written cell, or `None`."""
    band = find_band(grid)
    last = grid.rows if band is None else band.top
    for row in range(last):
        if SUMMARY_ROW.match(grid.row_text(row)):
            return grid.attrs[row][2]
    return None


def plain_marker(grid, needle):
    """Whether every cell of `needle` on the screen carries no colour and no attribute."""
    found = grid.find(needle)
    if found is None:
        return False
    row, column = found
    return all(grid.is_plain(row, col) for col in range(column, column + len(needle)))


def left_behind(grid, palette):
    """Every document row still carrying one of `palette`'s greys, with the row.

    The "and nothing was missed" half of a retint: the marker readings below say
    the named cells moved, and this says no row of the document was left in the
    palette the session has left.

    No row is exempt. The welcome line was once (it was written before the
    band had a record of the screen, so no report reached it); that was fixed
    by sizing the band before the first document write.
    """
    return [
        (row, attrs)
        for row, attrs in enumerate(attrs_above_the_rule(grid))
        if any(attr in palette for attr in attrs)
    ]


def a_document_with_both_owned_roles(run, trial, fixture, body_text, label):
    """Drives `trial` to a screen holding an answer, a notice and the user's echo.

    The things a report has different duties towards, put on the screen in an
    order chosen so the document has stopped moving when the caller reads it:
    the turn first -- its card, its plain answer and its dim summary -- and
    [`NOTICE_COMMAND`] after it, because that one starts no turn at all.

    The wait is on the answer's **own row** rather than on the echo of the
    command, so what the caller is handed back is a document that has stopped
    moving.
    """
    trial.send("say " + run.nonce + "\r")
    trial.wait_for(body_text)
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "%s: the nonce this run minted is in the request xfx sent" % label,
    )
    asked = fixture.request_count()
    trial.send(NOTICE_COMMAND + "\r")
    trial.wait_until(
        "%s: xfx's own line about the session" % label,
        lambda _t: said_row(trial.peek()) is not None,
    )
    run.require(
        fixture.request_count() == asked,
        "%s: the notice was answered locally (%d new request(s))"
        % (label, fixture.request_count() - asked),
    )
    return trial


def said_row(grid):
    """The notice row once it is really there, or `None` while it is not."""
    row = notice_row(grid)
    if row is None or not grid.row_text(row).startswith(NOTICE_PREFIX):
        return None
    return row


def notice_attribute(grid):
    """The attribute of the notice row's first cell, or `None` without one."""
    row = said_row(grid)
    return None if row is None else grid.attrs[row][0]


def scenario_27(run):
    """A background report recolours what is on the screen, and changes nothing else.

    **Restated for the fx transcript** (`3efe232`, `e153ff3`). The rows this row
    re-tints used to be the answer, in the body grey, and xfx's notice. The
    answer is plain now -- markdown in the two-column gutter, with no palette
    colour of its own in either palette -- and what xfx paints into the document
    is its notices (`NOTICE`), a card's rail and the welcome line's brand
    (`BODY`, the hint grey) and the quiet rows, a turn's summary and the
    welcome line's version (`DIM`): exactly the three roles
    `Palette::document_retint` moves. So the report must move all three, leave
    the answer plain (a re-tint that painted text it never coloured fails), and
    leave the user's own words in the card uncoloured.

    **Restated for fx's dispatch and interrupt** (`.prd/tui-phase4/receipts/c0`,
    `receipts/c3`, `receipts/f4`): `/version` answers `* version: <v>` and
    leaves no card of its own, so the screen holds one card -- the prompt's --
    where it held two; and the line an interrupt writes after the report is
    upstream's cancelled line, read in its own two greys.
    """
    first, second = run.marker("first"), run.marker("second")

    # --- a session that was told the terminal is dark -----------------------
    #
    # Through the launch's own `OSC 11` conversation rather than `XFX_THEME`,
    # because a fixed palette is a session that takes no reports at all -- which
    # is the control at the bottom of this scenario and not its subject. Dark is
    # also what the detection falls back to, so this start is **not** read here
    # as proof that the reply was adopted: `12-theme-detection` owns that claim
    # by painting a light answer and a dark one differently.
    fixture = start_fixture(run, [fixtures.content_only(first + "\n\n" + second)])
    trial = run.trial("visible", gateway=fixture, answer_probes=False)
    trial.wait_for(pty.THEME_PROBE)
    run.require(pty.THEME_PROBE in trial.text(), "the launch asked the terminal its background")
    trial.send(PROBE_ANSWERS)
    trial.settled()
    # The subscription, positively: the report typed below is a sequence a
    # terminal sends **because it was asked to**, and a session that never asked
    # would be being fed something no real terminal would volunteer.
    run.require(
        "\x1b[?2031h" in trial.text(),
        "the launch subscribed to background changes before any report was sent",
    )
    from_a_known_composer(run, trial, "visible")
    a_document_with_both_owned_roles(run, trial, fixture, second, "visible")

    # A draft, so the claim below is about a composer with something in it. A
    # report that reached the input decoder as text would land here.
    draft = run.marker("draft")
    trial.send(draft)
    trial.wait_until(
        "the draft in the composer",
        lambda _t: composer_text(trial.peek()) == draft and band_is_idle(trial.peek()),
    )

    before = trial.grid("dark-before-the-report")
    echo = own_line_row(before, RAIL + " say " + run.nonce)
    run.require(echo is not None, "the user's own submitted line is on the screen, as its card")
    run.require(
        plain_marker(before, first) and plain_marker(before, second),
        "the answer's rows carry no palette colour of their own (%r / %r)"
        % (cell_attribute(before, first), cell_attribute(before, second)),
    )
    run.require(
        notice_attribute(before) == RETINT_DARK_NOTICE,
        "xfx's own line is in the dark notice grey (%r)" % notice_attribute(before),
    )
    run.require(
        len(rail_attributes(before)) == 1 and set(rail_attributes(before)) == {RETINT_DARK_BODY},
        "the card's rail is in the dark body grey (%r)" % rail_attributes(before),
    )
    run.require(
        summary_attribute(before) == RETINT_DARK_DIM,
        "the turn's summary is in the dark dim grey (%r)" % summary_attribute(before),
    )
    run.require(
        card_text_attributes(before) == {""},
        "the user's own words in the cards carry none of xfx's colours (%r)"
        % card_text_attributes(before),
    )
    asked = fixture.request_count()

    # --- the terminal says it went light ------------------------------------
    trial.send(WENT_LIGHT)
    trial.wait_until(
        "the rows already on the screen to be recoloured",
        lambda _t: notice_attribute(trial.peek()) == RETINT_LIGHT_NOTICE
        and set(rail_attributes(trial.peek())) == {RETINT_LIGHT_BODY}
        and summary_attribute(trial.peek()) == RETINT_LIGHT_DIM,
    )
    after = trial.grid("light-after-the-report")
    run.require(
        notice_attribute(after) == RETINT_LIGHT_NOTICE,
        "xfx's own line moved to the light notice grey (%r)" % notice_attribute(after),
    )
    run.require(
        len(rail_attributes(after)) == 1 and set(rail_attributes(after)) == {RETINT_LIGHT_BODY},
        "the rail moved to the light body grey (%r)" % rail_attributes(after),
    )
    run.require(
        summary_attribute(after) == RETINT_LIGHT_DIM,
        "the summary moved to the light dim grey (%r)" % summary_attribute(after),
    )
    run.require(
        plain_marker(after, first) and plain_marker(after, second),
        "the answer was not painted by a re-tint it has no colour in (%r / %r)"
        % (cell_attribute(after, first), cell_attribute(after, second)),
    )
    run.require(
        card_text_attributes(after) == {""},
        "the user's own words were not repainted (%r)" % card_text_attributes(after),
    )
    run.require(
        left_behind(after, RETINT_DARK) == [],
        "no document row was left in the palette the session has left: %r"
        % left_behind(after, RETINT_DARK),
    )
    # A recolour and **only** a recolour. Text, row positions, the caret, what
    # has left the top of the screen, the plane, the draft and the provider are
    # each a way for this vector to have been more than it claims.
    run.require(
        rows_above_the_rule(after) == rows_above_the_rule(before),
        "not one character of the document moved",
    )
    run.require(after.text() == before.text(), "and no row of the screen changed its text")
    run.require(
        after.find(first) == before.find(first) and after.find(second) == before.find(second),
        "the answer is on the rows and columns it was on (%r/%r vs %r/%r)"
        % (after.find(first), after.find(second), before.find(first), before.find(second)),
    )
    run.require(
        (after.row, after.col) == (before.row, before.col),
        "the caret came back where the last frame left it (%r vs %r)"
        % ((after.row, after.col), (before.row, before.col)),
    )
    run.require(
        after.scrollback == before.scrollback,
        "nothing was pushed into the terminal's own scrollback (%d vs %d rows)"
        % (len(after.scrollback), len(before.scrollback)),
    )
    run.require(
        (after.plane, after.entered_alternate) == (before.plane, before.entered_alternate),
        "the recolour stayed on the normal buffer (%r)" % (after.plane,),
    )
    run.require(composer_text(after) == draft, "the draft survived: %r" % composer_text(after))
    run.require(
        fixture.request_count() == asked,
        "a report asked the provider for nothing (%d new request(s))"
        % (fixture.request_count() - asked),
    )
    run.require(not after.unknown, "xfx emitted only the sequences it declares: %r" % after.unknown)

    # --- and back ------------------------------------------------------------
    trial.send(WENT_DARK)
    trial.wait_until(
        "the rows to be recoloured back",
        lambda _t: notice_attribute(trial.peek()) == RETINT_DARK_NOTICE
        and set(rail_attributes(trial.peek())) == {RETINT_DARK_BODY}
        and summary_attribute(trial.peek()) == RETINT_DARK_DIM,
    )
    back = trial.grid("dark-again")
    run.require(
        plain_marker(back, first) and plain_marker(back, second),
        "the answer is still plain on the way back (%r / %r)"
        % (cell_attribute(back, first), cell_attribute(back, second)),
    )
    run.require(
        left_behind(back, RETINT_LIGHT) == [],
        "and nothing was left in the light palette: %r" % left_behind(back, RETINT_LIGHT),
    )
    run.require(back.text() == before.text(), "the screen is the one it started as, in text")
    run.require(
        (back.row, back.col) == (before.row, before.col),
        "with the caret where it started (%r vs %r)"
        % ((back.row, back.col), (before.row, before.col)),
    )
    run.require(back.scrollback == before.scrollback, "and nothing in scrollback either")
    run.require(not back.unknown, "xfx emitted only the sequences it declares: %r" % back.unknown)

    trial.send(b"\x15\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the retinted session left at 0")
    run.require(
        pty.RESTORE in trial.session.settled_text(),
        "and gave the subscription back with the rest of the terminal",
    )
    fixture.stop()

    # --- the control: a session told what the palette is ---------------------
    #
    # **A control and not a mutation proof.** What it establishes is that the
    # same bytes, on a session whose palette was decided by `XFX_THEME`, move
    # nothing -- so the trial above measured a report rather than any keystroke
    # that happens to arrive. It cannot stand in for a build without the
    # recolour: this binary has it, and a palette that is locked never reaches
    # the code the trial above exercises.
    locked_marker = run.marker("locked")
    locked_fixture = start_fixture(run, [fixtures.content_only(locked_marker)], name="locked")
    locked = run.trial(
        "locked",
        gateway=locked_fixture,
        env_extra={"XFX_THEME": "dark"},
        answer_probes=False,
    )
    locked.wait_for(pty.PROBE)
    locked.send("\x1b[2;1R")
    locked.settled()
    run.require(
        pty.THEME_PROBE not in locked.text(),
        "XFX_THEME decided the palette, so no background query was sent",
    )
    from_a_known_composer(run, locked, "locked")
    a_document_with_both_owned_roles(run, locked, locked_fixture, locked_marker, "locked")
    locked.wait_until(
        "the locked session's band to be idle",
        lambda _t: band_is_idle(locked.peek()) is True and notice_attribute(locked.peek()),
    )
    held = locked.grid("locked-before-the-report")
    run.require(
        plain_marker(held, locked_marker),
        "the locked session's answer carries no palette colour (%r)"
        % cell_attribute(held, locked_marker),
    )
    run.require(
        notice_attribute(held) == RETINT_DARK_NOTICE
        and set(rail_attributes(held)) == {RETINT_DARK_BODY},
        "and its notice and rails are in the dark greys (%r / %r)"
        % (notice_attribute(held), rail_attributes(held)),
    )
    held_band = find_band(held)

    def band_colours(grid, band):
        return (grid.attrs_of_row(band.hint), grid.attrs_of_row(band.composer[0])) if band else None

    band_before = band_colours(held, held_band)

    # The report and a keystroke in **one** write, which is what makes "nothing
    # changed" a bounded claim rather than a sleep: the decoder consumes one
    # read in order, so a composer holding the `z` is a session that has already
    # been past the report.
    locked.send(WENT_LIGHT + b"z")
    locked.wait_until(
        "the keystroke behind the report to reach the composer",
        lambda _t: composer_text(locked.peek()) == "z",
    )
    unmoved = locked.grid("locked-after-the-report")
    run.require(
        plain_marker(unmoved, locked_marker)
        and notice_attribute(unmoved) == RETINT_DARK_NOTICE
        and set(rail_attributes(unmoved)) == {RETINT_DARK_BODY},
        "a locked palette took no report (%r / %r / %r)"
        % (cell_attribute(unmoved, locked_marker), notice_attribute(unmoved), rail_attributes(unmoved)),
    )
    run.require(
        rows_above_the_rule(unmoved) == rows_above_the_rule(held)
        and attrs_above_the_rule(unmoved) == attrs_above_the_rule(held),
        "and the document is the same rows in the same colours",
    )
    # The band's own two palette-painted rows, found up from the hint: the hint
    # row and the composer's rail. **Restated** (`a50f8a0`): the second used to
    # be the always-drawn divider, which is blank now.
    run.require(
        band_before is not None and band_colours(unmoved, find_band(unmoved)) == band_before,
        "the band kept its own colours too (%r vs %r)"
        % (band_colours(unmoved, find_band(unmoved)), band_before),
    )
    run.require(
        not unmoved.unknown, "xfx emitted only the sequences it declares: %r" % unmoved.unknown
    )
    locked.send(b"\x15\x04")
    run.require(locked.session.wait_exit() == ("exited", 0), "the locked session left at 0")
    locked_fixture.stop()

    # --- a report while the rest of the answer is still inside xfx -----------
    #
    # Head and tail are in **one** `text-delta`, which is what makes the pending
    # text a fact rather than a name: the fixture writes that event in a single
    # chunk and only then holds the stream open, so a screen showing the head
    # is a session that has already parsed the whole event -- and the tail it is
    # not showing is held by the product, not by the producer. Which of the
    # product's own buffers holds it is not readable from a screen; the pacer's
    # timing has its own proof in `src/tui/pacer.rs`'s unit tests, and
    # `8-streaming-render` makes the same release claim about the same shape --
    # in paragraphs, each ended by a blank line, because a prose line is shown
    # only once the presenter has seen what follows it (`3efe232`).
    #
    # **Restated for the plain answer** (`3efe232`): the claim used to be that
    # the head was re-tinted in place and the tail painted in the light body
    # grey. Prose has no palette colour now, so the head and the tail are each
    # asserted **plain** -- which no palette can tell apart -- and the two
    # palette halves of the claim move to the rows that carry one: what is
    # already on the screen (the card's rail and the welcome line) is re-tinted
    # while the tail is still inside the product, and xfx's own line written
    # **after** the report -- the cancelled line the interrupt leaves
    # (`receipts/f4`), its `▪` in the warning grey and its word in the body
    # grey (`src/tui/shell.rs`'s `cancelled_line`) -- is in the light palette's,
    # the palette now in force. It used to be a `stopping the turn` notice in
    # the notice grey; the claim is the same, read on fx's line.
    head, tail = run.marker("head"), run.marker("tail")
    paced = (
        head
        + "\n\n"
        + "".join(("xx " * 33).rstrip() + "\n\n" for _ in range(3))
        + tail
        + "\n\n"
    )
    pending_fixture = start_fixture(
        run, [fixtures.hang(fixtures.text_delta("a", paced))], name="pending"
    )
    pending = run.trial("pending-stream", gateway=pending_fixture, answer_probes=False)
    pending.wait_for(pty.THEME_PROBE)
    pending.send(PROBE_ANSWERS)
    pending.settled()
    from_a_known_composer(run, pending, "pending-stream")
    pending.send("stream " + run.nonce + "\r")
    pending.wait_for(head)
    run.require(
        tail not in pending.text(),
        "the head of the delta was on the screen and its tail had not been written at all",
    )
    holding = pending.grid("stream-dark-head")
    run.require(
        plain_marker(holding, head),
        "what is already released carries no palette colour (%r)" % cell_attribute(holding, head),
    )
    run.require(
        set(rail_attributes(holding)) == {RETINT_DARK_BODY},
        "and the card above it is in the dark body grey (%r)" % rail_attributes(holding),
    )

    pending.send(WENT_LIGHT)
    pending.wait_until(
        "what is on the screen to be recoloured mid-stream",
        lambda _t: set(rail_attributes(pending.peek())) == {RETINT_LIGHT_BODY},
    )
    run.require(
        tail not in pending.text(),
        "the tail was still inside the product when the screen was re-tinted",
    )
    pending.wait_for(tail)
    final = pending.grid("stream-final")
    run.require(
        plain_marker(final, head) and plain_marker(final, tail),
        "the head and the tail released after the report are both plain (%r / %r)"
        % (cell_attribute(final, head), cell_attribute(final, tail)),
    )
    run.require(
        left_behind(final, RETINT_DARK) == [],
        "no visible document row is left in the old palette: %r" % left_behind(final, RETINT_DARK),
    )
    run.require(
        final.text().count(tail) == 1,
        "the fixture's own end marker is on the screen exactly once (%d)"
        % final.text().count(tail),
    )
    run.require(
        any(run.nonce in body for body in pending_fixture.bodies()),
        "the nonce this run minted is in the request xfx sent (%d request(s))"
        % pending_fixture.request_count(),
    )
    run.require(not final.unknown, "xfx emitted only the sequences it declares: %r" % final.unknown)
    pending.send(b"\x03")
    pending.wait_until(
        "xfx's own interrupt line on the committed grid",
        lambda _t: pending.peek().find(CANCELLED_TURN) is not None,
    )
    stopped = pending.grid("stream-interrupted")
    found = stopped.find(CANCELLED_TURN)
    painted = (
        (stopped.attrs[found[0]][found[1]], stopped.attrs[found[0]][found[1] + 2])
        if found
        else None
    )
    run.require(
        painted == (RETINT_LIGHT_WARNING, RETINT_LIGHT_BODY),
        "xfx's own line written after the report is in the light palette's greys, the "
        "palette now in force (%r)" % (painted,),
    )
    pending.send(b"\x03")
    run.require(
        pending.session.wait_exit() == ("exited", 130), "the streaming session left with 130"
    )
    pending_fixture.stop()


SCENARIOS = {
    "1-launch-and-band-ownership": scenario_1,
    "2-cursor-probe-and-scrollback-push": scenario_2,
    "3-restore-matrix": scenario_3,
    "3b-shutdown-drain": scenario_3b,
    "3c-partial-frame-containment": scenario_3c,
    "3d-primary-band-recovery": scenario_3d,
    "3e-alternate-repaint-recovery": scenario_3e,
    "3f-job-control": scenario_3f,
    "3g-inherited-ignores": scenario_3g,
    "4-raw-mode-positively-entered": scenario_4,
    "5-editor-basics": scenario_5,
    "6-soft-wrap-and-growth-cap": scenario_6,
    "7-multiline-and-paste-framing": scenario_7,
    "8-streaming-render": scenario_8,
    "9-activity-row": scenario_9,
    "10-approval-panel": scenario_10,
    "10b-approval-mid-turn": scenario_10b,
    "11-ctrl-c-as-a-byte": scenario_11,
    "12-theme-detection": scenario_12,
    "13-cell-diff-correctness": scenario_13,
    "14-no-op-frame-skip": scenario_14,
    "15-resize-reflow": scenario_15,
    "16-slash-menu": scenario_16,
    "17-history": scenario_17,
    "17b-quit-and-resume": scenario_17b,
    "18-provider-switch": scenario_18,
    "19-model-catalog-and-context-meter": scenario_19,
    "20-alternate-screen-approval": scenario_20,
    "21-paste-entities": scenario_21,
    "22-edit-history": scenario_22,
    "23-question-panel": scenario_23,
    "23b-question-cancelled": scenario_23b,
    "24-approval-readiness": scenario_24,
    "25-amended-approval": scenario_25,
    "26-layout-stabilization": scenario_26,
    "26b-composer-yield-to-panel": scenario_26b,
    "27-live-theme-retint": scenario_27,
}


def main():
    # `--list` is an option rather than a scenario name spelled like one, which
    # is what it was: `argparse` reads a leading `--list` as an unknown option
    # and refuses before the branch that answered it could ever run. It is the
    # runner's half of the registration gate -- the shell script compares this
    # list against its own -- so it has to be reachable.
    parser = argparse.ArgumentParser()
    parser.add_argument("scenario", nargs="?")
    parser.add_argument("--list", action="store_true", dest="listing")
    parser.add_argument("--binary")
    parser.add_argument("--faulty")
    parser.add_argument("--evidence")
    arguments = parser.parse_args()
    if arguments.listing:
        for name in SCENARIOS:
            print(name)
        return 0
    # Required for a run, and checked here rather than by `required=True`,
    # because a listing needs none of them.
    for name in ("scenario", "binary", "faulty", "evidence"):
        if not getattr(arguments, name):
            parser.error("%s is required to drive a scenario" % name)
    if arguments.scenario not in SCENARIOS:
        parser.error("no scenario named %r is registered" % arguments.scenario)
    run = Run(arguments.scenario, arguments.binary, arguments.faulty, arguments.evidence)
    try:
        SCENARIOS[arguments.scenario](run)
    except Exception as failure:  # noqa: BLE001 - a scenario that died is a failure
        run.checks += 1
        run.problems.append(
            "the scenario could not be driven (at %r): %s" % (run.driving, failure)
        )
        import traceback

        with open(os.path.join(run.dir, "traceback.txt"), "w", encoding="utf-8") as handle:
            traceback.print_exc(file=handle)
    return run.finish()


if __name__ == "__main__":
    sys.exit(main())
PYTHON

# ---------------------------------------------------------------------------
# the run
# ---------------------------------------------------------------------------

# What a developer's shell looks like, and not one of these may reach the
# binary: the helpers build every child's environment from nothing, and this is
# where that claim is tested rather than asserted. `XFX_PERMISSION_MODE=yolo` is
# the sharpest of them -- if it leaked, the approval scenarios would pass by
# never being asked a question at all.
export VERCEL_OIDC_TOKEN="hdr-xfx-smoke-tui-hostile.payload-must-never-be-used.sig-not-real"
export XFX_MODEL="hostile/model-must-not-be-used"
export XFX_PERMISSION_MODE="yolo"
export XFX_MAX_AGENT_STEPS="1"
export XFX_THEME="light"
export TMUX="/tmp/tmux-hostile/default,1,0"

# Every scenario of `.prd/06-qa-harness.md`, in its order: Phase 1's 1-12 with
# the lettered rows the drain, the counted-delivery containment, the recovery
# that containment's own cleanup vector answers, the same recovery for a torn
# repaint of the alternate plane, and the mid-turn approval added, then
# Phase 2's 13-21 and Phase 3's 22, 23, the lettered row 23b, 24 and 25 --
# plus 26, 26b and 27, QA-only additions not in that PRD's own numbered list,
# for the band's growth-and-shrink layout contract, the composer's own yield to
# a panel within it, and the recolour a terminal's own background report owes
# the rows already on the screen -- and three more for the parity program's
# behaviour list: 3f, the stop and the deaths under real job control; 3g, a
# launch that inherited ignored signals; 17b, a session left and resumed. This
# list and `SCENARIOS` in the python helper are
# the two registrations, and they are one order -- a name in either that the
# other does not have is a scenario nothing runs or a runner nothing names.
scenarios=(
	1-launch-and-band-ownership
	2-cursor-probe-and-scrollback-push
	3-restore-matrix
	3b-shutdown-drain
	3c-partial-frame-containment
	3d-primary-band-recovery
	3e-alternate-repaint-recovery
	3f-job-control
	3g-inherited-ignores
	4-raw-mode-positively-entered
	5-editor-basics
	6-soft-wrap-and-growth-cap
	7-multiline-and-paste-framing
	8-streaming-render
	9-activity-row
	10-approval-panel
	10b-approval-mid-turn
	11-ctrl-c-as-a-byte
	12-theme-detection
	13-cell-diff-correctness
	14-no-op-frame-skip
	15-resize-reflow
	16-slash-menu
	17-history
	17b-quit-and-resume
	18-provider-switch
	19-model-catalog-and-context-meter
	20-alternate-screen-approval
	21-paste-entities
	22-edit-history
	23-question-panel
	23b-question-cancelled
	24-approval-readiness
	25-amended-approval
	26-layout-stabilization
	26b-composer-yield-to-panel
	27-live-theme-retint
)

printf 'xfx smoke-tui\n  binary:   %s\n  faulty:   %s\n  evidence: %s\n\n' \
	"$binary" "$faulty" "$evidence"

# The two registrations are reconciled rather than trusted. A scenario written
# and never listed here runs in no gate; a name listed here with no runner is a
# `KeyError` in the middle of a suite, which reads like a scenario that failed
# rather than like a list that is wrong. Both are silent in the only way that
# matters -- the printed count still looks right -- so the comparison happens
# before anything is driven.
if ! registered="$(python3 "$helpers/scenarios.py" --list 2>&1)"; then
	printf 'smoke-tui: the scenario list and the runner disagree\n' >&2
	printf '  the runner could not list what it has: %s\n' "$registered" >&2
	exit 1
fi
if [ "$registered" != "$(printf '%s\n' "${scenarios[@]}")" ]; then
	printf 'smoke-tui: the scenario list and the runner disagree\n' >&2
	printf '  listed here : %s\n' "${scenarios[*]}" >&2
	printf '  registered  : %s\n' "$(printf '%s ' $registered)" >&2
	exit 1
fi

failures=0
passed=0

# The oracle first, and the run stops on a dishonest one. Every scenario below
# judges xfx against `vt_grid.Grid`; an emulator whose own claims are false
# reports its disagreement with the product as a product defect, and reports a
# sequence xfx should never emit as a passing screen. It costs milliseconds and
# it is the only check here that needs no binary.
printf '  %-38s' "0-oracle-falsification"
set +e
python3 "$helpers/oracle_test.py" "$evidence" >"$evidence/0-oracle-falsification.log" 2>&1
oracle_status=$?
set -e
if [ "$oracle_status" -eq 0 ]; then
	printf 'ok\n'
else
	printf 'FAIL\n'
	sed 's/^/      /' "$evidence/0-oracle-falsification.log" >&2
	printf '      re-run: python3 %s %s\n' "$helpers/oracle_test.py" "$evidence" >&2
	printf '\nsmoke-tui: the VT oracle failed its own falsification tests; no scenario was run\nevidence: %s\n' \
		"$evidence"
	exit 1
fi

for scenario in "${scenarios[@]}"; do
	printf '  %-38s' "$scenario"
	# File-redirected, and the status taken from the command itself. A pipeline
	# reports its *last* command's status, so `python3 … | tail` would score a
	# scenario that failed as one that passed -- which is how a gate stops being
	# one.
	set +e
	python3 "$helpers/scenarios.py" "$scenario" \
		--binary "$binary" --faulty "$faulty" --evidence "$evidence" \
		>"$evidence/$scenario.log" 2>&1
	status=$?
	set -e
	if [ "$status" -eq 0 ]; then
		passed=$((passed + 1))
		printf 'ok\n'
	else
		failures=$((failures + 1))
		printf 'FAIL\n'
		sed 's/^/      /' "$evidence/$scenario.log" >&2
		printf '      re-run: python3 %s %s --binary %s --faulty %s --evidence %s\n' \
			"$helpers/scenarios.py" "$scenario" "$binary" "$faulty" "$evidence" >&2
	fi
done

# Summed by the machine from what each scenario wrote down. Hand-summing a count
# that appears in a report is how a report starts disagreeing with its run.
checks="$(find "$evidence" -name checks -type f -exec cat {} + 2>/dev/null |
	awk '{total += $1} END {print total + 0}')"

printf '\nsmoke-tui: %d scenarios + the oracle, %d checks, %d failures\nevidence: %s\n' \
	"${#scenarios[@]}" "$checks" "$failures" "$evidence"
if [ "$failures" -ne 0 ]; then
	exit 1
fi
