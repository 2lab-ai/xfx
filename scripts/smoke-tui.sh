#!/usr/bin/env bash
#
# The Phase-1 TUI acceptance gate, on a real terminal.
#
#   scripts/smoke-tui.sh <path-to-xfx> --faulty <path-to-xfx-with-faults> [evidence-dir]
#
# The second runner beside `scripts/smoke.sh`, which keeps running unchanged:
# the line-oriented product it receipts does not stop existing when a TUI
# arrives, and `xfx ask` is still a pipe-friendly command with no terminal. This
# one drives the Phase-1 scenarios of `.prd/06-qa-harness.md`, and the Phase-2
# ones as they land, against a **release** binary on a real pseudoterminal, with
# a cell-grid oracle and an evidence directory.
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
# images). The four helpers are written out as real files under the evidence
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

What is modelled, and nothing else:

* `CUP` (`CSI row ; col H`), `ED` (`CSI 0/2/3 J`), `EL` (`CSI 0 K`)
* `SGR` (`CSI ... m`), remembered per cell so a colour can be asserted on
* `DECSET`/`DECRST` for the five private modes xfx sets: `?2026` (synchronized
  output), `?25` (cursor visibility), `?7` (autowrap -- xfx turns it **off**,
  which is why the wrap below is not the usual one), `?2004` (bracketed paste),
  `?1049` (the alternate screen it only ever *resets*, defensively)
* `CSI 6 n` (the launch's cursor query) and the three `>`/`<` keyboard-protocol
  sequences of `term.rs:32-52`
* `OSC 2` (the window title, remembered rather than drawn) and `OSC 11` (the
  background query and its reply); **every other `OSC` number is a finding**,
  `OSC 52` first among them -- a band that started writing the user's clipboard
  would otherwise be an escape sequence nobody looked at
* `XTWINOPS` `CSI 22 ; 2 t` / `CSI 23 ; 2 t`, the title stack xfx pushes at
  entry and pops on every restore path
* `CR`, `LF` with a scroll at the bottom margin

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
# truncated OSC does not swallow the rest of the stream.
OSC = re.compile(rb"\x1b\](?P<body>[^\x07\x1b]*)(?:\x07|\x1b\\)")

# The private modes xfx is allowed to set. Anything else is a finding.
KNOWN_MODES = {"?2026", "?25", "?7", "?2004", "?1049"}

# The `OSC` numbers xfx is allowed to write: the window title
# (`src/tui/frame.rs`) and the background query it asks the terminal
# (`src/tui/theme.rs:66`). `OSC 52` is deliberately **not** here -- a build that
# started writing the clipboard must fail this run rather than be shrugged at.
KNOWN_OSC = {"2", "11"}

# The window operations xfx is allowed to write: push and pop of the title
# stack, so a session gives the terminal's title back the way it found it.
KNOWN_WINDOW_OPS = {"22;2", "23;2"}

# The keyboard-protocol sequences of `src/tui/term.rs:32-52`, spelled out:
# `>4;2m`/`>4;0m` are modifyOtherKeys on and off, `>1u` pushes the kitty
# keyboard flags and `<u` pops them.
KNOWN_PRIVATE = {(">", "4;2", "m"), (">", "4;0", "m"), (">", "1", "u"), ("<", "", "u")}

# How many rows of what left the top of the screen are kept.
SCROLLBACK_ROWS = 2000


# The joiner that makes several emoji one, and the range regional indicators
# come from -- two flags of them are one flag.
ZWJ = "\u200d"
REGIONAL = range(0x1F1E6, 0x1F200)


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

    def __init__(self, rows, cols):
        self.rows, self.cols = rows, cols
        self.cells = [[" "] * cols for _ in range(rows)]
        self.attrs = [[""] * cols for _ in range(rows)]
        self.row = self.col = 0
        self.sgr = ""
        self.autowrap = True
        self.unknown = []
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
                pass  # a bell moves no cell
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
                # moving to the next row. xfx measures its own rows so this is
                # unreachable in practice, and modelling it wrongly would turn a
                # product bug into a *different* product bug on the grid.
                self.col = self.cols - width
        self.cells[self.row][self.col] = character
        self.attrs[self.row][self.col] = self.sgr
        for offset in range(1, width):
            self.cells[self.row][self.col + offset] = ""
            self.attrs[self.row][self.col + offset] = self.sgr
        self.col += width

    def _newline(self):
        if self.row + 1 >= self.rows:
            # The bottom margin: a linefeed scrolls, which is how xfx's document
            # appends reach the terminal's own scrollback.
            leaving = self.cells.pop(0)
            self.scrollback.append("".join(leaving).rstrip())
            del self.scrollback[:-SCROLLBACK_ROWS]
            self.attrs.pop(0)
            self.cells.append([" "] * self.cols)
            self.attrs.append([""] * self.cols)
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
        if final == "H":
            row, _, col = params.partition(";")
            self.row = min(max(int(row or 1) - 1, 0), self.rows - 1)
            self.col = min(max(int(col or 1) - 1, 0), self.cols - 1)
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

    def _osc(self, match):
        """An `OSC` string: no cell moves, but the number has to be one of ours."""
        body = match.group("body").decode("utf-8", "replace")
        number, separator, argument = body.partition(";")
        if number not in KNOWN_OSC:
            self.unknown.append(match.group(0))
            return
        if number == "2" and separator:
            self.title = argument

    def _sgr(self, params, match):
        """One `SGR`, against the palette `src/tui/theme.rs` actually emits.

        The narrow question, not the SGR grammar: what a *row* may carry is a
        256-colour foreground, a direct-colour foreground, and the reset that
        ends them (`src/tui/pacer.rs::is_palette_sgr`). An emulator that took
        the whole vocabulary would accept a band that had started to blink, to
        conceal a row, or to paint a background across the screen.
        """
        if params in ("", "0"):
            self.sgr = ""
            return
        parts = params.split(";")
        if parts[0] == "38" and len(parts) >= 2:
            wanted = {"5": 3, "2": 5}.get(parts[1])
            if wanted == len(parts) and all(part.isdigit() for part in parts):
                self.sgr = match.group(0).decode()
                return
        self.unknown.append(match.group(0))

    def _private(self, private, params, final, match):
        if private == "?" and final in ("h", "l"):
            if "?" + params not in KNOWN_MODES:
                self.unknown.append(match.group(0))
                return
            if params == "7":
                self.autowrap = final == "h"
            return
        if (private, params, final) in KNOWN_PRIVATE:
            return
        self.unknown.append(match.group(0))

    def _erase_right(self):
        for col in range(self.col, self.cols):
            self.cells[self.row][col] = " "
            self.attrs[self.row][col] = ""

    def _erase_screen(self, params, match):
        if params == "0":  # from the cursor to the end of the screen
            self._erase_right()
            for row in range(self.row + 1, self.rows):
                self.cells[row] = [" "] * self.cols
                self.attrs[row] = [""] * self.cols
        elif params in ("2", "3"):
            # `2` is the visible screen and `3` is the terminal's own scrollback,
            # which this emulator does not model -- it has no scrollback to
            # erase, so erasing it is a no-op rather than an unknown sequence.
            if params == "2":
                self.cells = [[" "] * self.cols for _ in range(self.rows)]
                self.attrs = [[""] * self.cols for _ in range(self.rows)]
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

        The `?1049h` this product never writes is what makes the two one
        document -- xfx paints on the normal buffer, so a row that scrolled off
        is still the user's, one wheel-turn away.
        """
        return self.scrollback_text() + "\n" + self.text()

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

    def snapshot(self):
        ruler = "     " + "".join(str(column % 10) for column in range(self.cols))
        lines = [ruler]
        for row in range(self.rows):
            lines.append("%3d  %s" % (row + 1, self.row_text(row)))
        lines.append("")
        lines.append("cursor: row %d col %d" % (self.row + 1, self.col + 1))
        lines.append("autowrap: %s" % ("on" if self.autowrap else "off"))
        lines.append("title: %r" % (self.title,))
        if self.scrollback:
            lines.append("")
            lines.append("--- what left the top of the screen, oldest first ---")
            lines.extend(self.scrollback[-40:])
        for row in range(self.rows):
            runs = self.attrs_of_row(row)
            if runs:
                lines.append("attrs row %d: %r" % (row + 1, runs))
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
  accident, so they are the two asserted on.
* **The subset xfx really emits is still accepted.** A contract that rejects
  the product is a broken harness rather than a caught defect, so every shape
  `src/tui/term.rs`, `src/tui/theme.rs` and `src/tui/frame.rs` write is fed
  through and must leave `unknown` empty.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from vt_grid import Grid  # noqa: E402

# A three-person ZWJ family: five scalars, one cluster, two columns.
FAMILY = "\U0001F468\u200D\U0001F469\u200D\U0001F467"

# The palette's two spellings (`src/tui/theme.rs:195-227`) and the reset that
# ends every band row.
PALETTE_SGR = ("\x1b[38;5;250m", "\x1b[38;2;188;188;188m", "\x1b[0m", "\x1b[m")

# The background query and the window title (`src/tui/theme.rs:66`,
# `src/tui/frame.rs`), in both terminator spellings a terminal may use.
PRODUCT_OSC = ("\x1b]11;?\x1b\\", "\x1b]11;rgb:0000/0000/0000\x1b\\", "\x1b]2;xfx \u00b7 glm-5.2\x07")

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


def an_undeclared_sgr_parameter_is_a_finding():
    for sequence in ("\x1b[999m", "\x1b[7m", "\x1b[5m", "\x1b[48;5;1m", "\x1b[38;5;m"):
        grid = fed(sequence)
        require(grid.unknown, "%r passed the oracle unremarked" % sequence)


def the_sgr_the_product_emits_is_accepted():
    for sequence in PALETTE_SGR:
        grid = fed(sequence + "x")
        require(not grid.unknown, "the palette's %r was rejected: %r" % (sequence, grid.unknown))
    require(fed("\x1b[38;5;250mx").attrs[0][0] == "\x1b[38;5;250m", "the colour was remembered")
    require(fed("\x1b[38;5;250m\x1b[0mx").attrs[0][0] == "", "the reset closed the colour")


def the_title_stack_is_accepted_and_other_window_operations_are_not():
    grid = fed("".join(TITLE_STACK))
    require(not grid.unknown, "the title stack was rejected: %r" % (grid.unknown,))
    grid = fed("\x1b[18t")
    require(grid.unknown, "an undeclared window operation passed the oracle unremarked")


def a_control_byte_on_the_screen_is_still_a_finding():
    # The oracle's oldest claim, kept under test so a rewrite of the text path
    # cannot quietly drop it.
    require(fed("a\x00b").unknown, "a NUL reached the screen unremarked")


TESTS = (
    a_zwj_family_is_one_cluster_two_cells_wide,
    a_combining_mark_stays_with_the_cell_it_marks,
    a_wide_scalar_is_still_two_cells,
    a_flag_is_one_cluster,
    a_clipboard_osc_is_a_finding,
    an_undeclared_osc_number_is_a_finding,
    the_osc_the_product_emits_is_accepted,
    an_undeclared_sgr_parameter_is_a_finding,
    the_sgr_the_product_emits_is_accepted,
    the_title_stack_is_accepted_and_other_window_operations_are_not,
    a_control_byte_on_the_screen_is_still_a_finding,
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
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from socketserver import TCPServer

# How long a held-open stream is kept alive when the client never hangs up.
#
# Bounded so a scenario that fails before it interrupts anything fails rather
# than hanging the whole run.
HANG_TIMEOUT = 90.0


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


class Fixture:
    """A loopback Gateway that answers `script` in order and records everything."""

    def __init__(self, script, record_path=None):
        self.script = list(script)
        self.record_path = record_path
        self.captured = []
        self.lock = threading.Lock()
        self.served = 0
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

    def stop(self):
        self.server.shutdown()
        self.server.server_close()

    # -- the product's side -----------------------------------------------

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
FRAME_BEGIN = "\x1b[?2026h\x1b[?25l"
FRAME_END = "\x1b[?2026l\x1b[?25h"

# The whole interactive mode sequence and the two restores, in order
# (`src/tui/term.rs:32-52`). Spelled out rather than imported: a harness that
# read the constant it is checking would pass for whatever the module declared.
MODE_SET = "\x1b[>4;2m\x1b[>1u\x1b[?2004h\x1b[?7l\x1b[22;2t"
RESTORE = "\x1b[23;2t\x1b[>4;0m\x1b[<u\x1b[?2004l\x1b[?7h\x1b[?25h"
ABNORMAL_RESTORE = "\x1b[?1049l" + RESTORE

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
    ):
        self.terminal = terminal
        self.captured = bytearray()
        self.status = None  # (kind, value) once reaped
        self.reaped = False
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
        ends = text.find(FRAME_END, begins)
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
        os.kill(self.pid, number)

    def close(self):
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
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import fixture_server as fixtures  # noqa: E402
import pty_tui as pty  # noqa: E402
from vt_grid import Grid  # noqa: E402

# The dark background reply and the cursor report, in the order they were asked
# for -- which is the order one read serves them in. Answering both keeps every
# launch on the same palette and off the 200 ms deadline a silent terminal pays.
PROBE_ANSWERS = "\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[2;1R"

# What a session says while a turn is waiting to be told whether it may act.
PERMISSION_TITLE = "Permission needed"

# The second choice's wording for anything that is not a shell command.
ALWAYS_WORDING = "don't ask again for this request"

# The hint row of a session with a credential, on the compiled-in defaults.
HINT_AUTO = "auto · glm-5.2"

FRAME_BEGIN_BYTES = pty.FRAME_BEGIN.encode()
FRAME_END_BYTES = pty.FRAME_END.encode()


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
        home=None,
        tmux=False,
    ):
        self.run = run
        self.label = label
        self.dir = os.path.join(run.dir, label)
        os.makedirs(self.dir, exist_ok=True)
        self.home = home or os.path.join(self.dir, "home")
        self.workspace = os.path.join(self.dir, "workspace")
        for directory in (self.home, self.workspace):
            os.makedirs(directory, exist_ok=True)
        self.notes = os.path.join(self.workspace, "notes.txt")
        if notes:
            with open(self.notes, "w", encoding="utf-8") as handle:
                handle.write("alpha\n")

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
            argv = [binary]
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
        return self.session.wait_for(needle, timeout=timeout)

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
                raise Failure("the session never took the terminal back; it is still %r" % state)
            time.sleep(pty.IDLE_POLL)

    # -- evidence ---------------------------------------------------------

    def peek(self):
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
        captured = bytes(self.session.captured)
        begins = captured.rfind(FRAME_BEGIN_BYTES)
        if begins >= 0 and captured.find(FRAME_END_BYTES, begins) < 0:
            captured = captured[:begins]
        return Grid(self.rows, self.cols).feed(captured)

    def grid(self, label):
        grid = self.peek()
        self.snapshots += 1
        path = os.path.join(self.dir, "grid-%02d-%s.txt" % (self.snapshots, label))
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(grid.snapshot())
        return grid

    def _write_modes(self, when, state):
        with open(os.path.join(self.dir, "termios-%s.txt" % when), "w", encoding="utf-8") as h:
            h.write(state.describe())

    def close(self):
        self.session.close()
        try:
            self._write_modes("after", self.terminal.modes())
        except OSError:
            pass
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


def composer_first_row(grid):
    """The row the composer's prompt marker is on, or `None`.

    Read off the **cells** rather than off the row's text, because an empty
    composer's row is the marker and two spaces: a text comparison would have
    to see `"> "` where `rstrip` leaves `">"`, and the difference between
    "the composer is empty" and "there is no composer" is exactly what a
    scenario asserting an empty draft is asking about.

    `None` is a real answer and not a failure: a draft taller than the cap
    scrolls *inside* its window, and the marker sits on the composer's first
    row, which such a draft does not show.
    """
    for row in range(grid.rows - 2, -1, -1):
        if grid.cells[row][0] == ">" and grid.cells[row][1] == " ":
            return row
    return None


def composer_text(grid):
    start = composer_first_row(grid)
    if start is None:
        return None
    return "".join(grid.row_text(row)[2:] for row in range(start, grid.rows - 1))


# ---------------------------------------------------------------------------
# 1. launch and band ownership
# ---------------------------------------------------------------------------


def scenario_1(run):
    """The band is at the bottom, prior output is above it, nothing took 1049."""
    marker = run.marker("launch")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
    prior = "PRIOR-OUTPUT-" + run.nonce
    trial = run.trial("launch", gateway=fixture, prior=prior).settled()

    grid = trial.grid("after-first-frame")
    run.require(grid.row_text(grid.rows - 1).strip() != "", "the band's hint row is painted")
    run.require(
        grid.row_text(grid.rows - 1).strip() == HINT_AUTO,
        "the hint row says what a turn would run with: %r" % grid.row_text(grid.rows - 1).strip(),
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
    painted_above = [
        row + 1 for row in range(grid.rows - 3) if grid.row_text(row).strip() != ""
    ]
    run.require(
        not painted_above,
        "the band painted on no document row: %r" % painted_above,
    )
    run.require(
        all(grid.row_text(row).strip() != "" for row in range(grid.rows - 3, grid.rows)),
        "the band's three rows are the last three rows of the screen",
    )
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
    deltas = [fixtures.text_delta("d", marker + " ")]
    deltas.extend(fixtures.text_delta("d", "chunk-%d " % n) for n in range(2000))
    fixture = start_fixture(run, [fixtures.hang(*deltas)])
    trial = run.trial("drain", faulty=True, fault="slow-ui", gateway=fixture).settled()

    trial.send("stream a lot " + run.nonce + "\r")
    trial.wait_for(marker)
    trial.wait_for("chunk-0")
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    grid = trial.grid("mid-stream")
    run.require(
        grid.find(marker) is not None,
        "the fixture's own marker %r is rendered on the screen" % marker,
    )
    run.require(grid.find("chunk-0") is not None, "and the stream behind it is running")
    run.require(grid.text().strip() != "", "the screen is not blank")

    trial.send(b"\x04")
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
    # One frame is one `write_all` (`src/tui/frame.rs:145-157`, onto the locked
    # standard output passed down from `event_loop.rs:286`), and `write_all`
    # retries `Interrupted` and nothing else: the first `WouldBlock` ends the
    # frame, and the band owes it again rather than continuing it. So for a
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
        "it ended on the budget rather than retrying forever (%.2fs, deadline %.2fs)"
        % (elapsed, STARVED_DEADLINE),
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
# 4. raw mode positively entered
# ---------------------------------------------------------------------------


# How the starved-screen row's deadline is arrived at, because a number nobody
# can derive is a number nobody can defend. The property under test is
# `event_loop::FRAME_BUDGET` -- **half a second** of wall clock, past which a
# screen that has taken nothing ends the session instead of being retried
# forever -- so the bound has to be close enough to half a second that a budget
# which quietly stopped being enforced is detectable. The first version of this
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
    trial.wait_until(
        "the caret to go home in the frame that painted the row",
        lambda text: (trial.session.last_frame() or "").endswith("\x1b[%d;3H" % (trial.rows - 1)),
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
    start = composer_first_row(grid)
    run.require(start is not None, "the composer is on the screen")
    if start is not None:
        rows = [grid.row_text(row)[2:] for row in range(start, grid.rows - 1)]
        run.require(len(rows) > 1, "the paragraph really wrapped (%d row(s))" % len(rows))
        broken = [
            token
            for row in rows
            for token in row.split()
            if token not in WRAP_WORDS
        ]
        run.require(not broken, "no word was split across a row boundary: %r" % broken)
        for row in range(start, grid.rows - 1):
            run.require(
                grid.row_text(row).startswith("> ") or grid.row_text(row).startswith("  "),
                "composer row %d is written into the two-cell gutter" % (row + 1),
            )
    trial.send(b"\x15")
    discriminate(run, trial, fixture, marker)
    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the wrap session left cleanly")
    fixture.stop()

    # The cap: on a 12-row screen the content area's bottom is row 9, so a
    # composer may take at most `9 / 2 + 1` = 5 rows and its divider stops at
    # row 6. Sixteen rows of draft, five of them shown.
    capped = run.trial("growth-cap", rows=12, cols=20).settled()
    for _ in range(8):
        capped.send(b"0123456789012345678")
        capped.send(b"\x0a")  # C-j: a newline in the composer
    # The divider at the cap, read off the row rather than off the erase that
    # used to precede it: the diff erases rows rather than screens.
    capped.wait_until(
        "the band to reach its cap with the divider on row 6",
        lambda _t: capped.peek().row_text(5) == "─" * capped.cols,
    )
    capped.wait_for("\x1b[12;1H")
    grid = capped.grid("at-the-cap")
    # `content_bottom / 2 + 1` = five rows, so the divider stops at row 6 and
    # the five rows under it are the composer's window onto a sixteen-row draft.
    # The marker is **not** asserted: it sits on the draft's first row, which a
    # draft scrolled inside its window does not show, and requiring it here
    # would be requiring the cap not to work.
    run.require(
        set(grid.row_text(5)) == {"─"} and len(grid.row_text(5)) == capped.cols,
        "the divider spans row 6, where the cap puts it: %r" % grid.row_text(5),
    )
    grew_past = [row + 1 for row in range(5) if grid.row_text(row).strip() != ""]
    run.require(
        not grew_past, "the composer did not grow past its cap onto document rows %r" % grew_past
    )
    # Four rows of draft and the caret's own empty last line: eight `C-j`s
    # leave a ninth, empty logical line, and the window shows the *tail* of the
    # draft. Requiring five painted rows would be requiring that last line not
    # to exist.
    showing = [row for row in range(6, grid.rows - 1) if grid.row_text(row).strip() != ""]
    run.require(
        len(showing) >= 4,
        "the composer window is showing the tail of the draft (%d row(s) of it)" % len(showing),
    )
    run.require(
        6 <= grid.row <= grid.rows - 2,
        "and the caret is inside the composer's five rows (row %d)" % (grid.row + 1),
    )
    run.require(
        grid.row_text(grid.rows - 1).strip() != "", "and the hint row is still the last row"
    )
    capped.send(b"\x0d")
    capped.send(b"\x04")
    run.require(capped.session.wait_exit() == ("exited", 0), "the capped session left cleanly")
    grid = capped.grid("submitted")
    run.require(
        "0123456789012345678" in grid.document_text(),
        "and what was submitted reached the terminal's own document",
    )


# ---------------------------------------------------------------------------
# 7. multiline and paste framing
# ---------------------------------------------------------------------------


def scenario_7(run):
    """One paste is one prompt, whatever is inside it."""
    marker = run.marker("paste")
    big = run.marker("bigpaste")
    fixture = start_fixture(run, [fixtures.content_only(marker), fixtures.content_only(big)])
    trial = run.trial("paste", gateway=fixture).settled()

    run.require("\x1b[?2004h" in trial.text(), "bracketed paste was enabled")

    trial.send(b"\x1b[200~")
    trial.send(("first " + run.nonce + "\nsecond \x03line\n\x1b[Athird line").encode("utf-8"))
    trial.send(b"\x1b[201~")
    trial.wait_until(
        "the whole paste in one complete frame",
        lambda _t: (trial.session.last_frame() or "").find("> first ") >= 0
        and "  [Athird line" in (trial.session.last_frame() or ""),
    )
    run.require(fixture.request_count() == 0, "a paste did not submit itself")
    grid = trial.grid("pasted")
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

    trial.send(b"\x04")
    run.require(trial.session.wait_exit() == ("exited", 0), "the paste session left cleanly")
    fixture.stop()


# ---------------------------------------------------------------------------
# 8. streaming render
# ---------------------------------------------------------------------------


def scenario_8(run):
    """A stream is released over frames, arrives whole, and leaks no attribute."""
    head, tail = run.marker("head"), run.marker("tail")
    body = "%s %s %s" % (head, "xx " * 100, tail)
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
    trial.wait_for("stopping the turn")
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
        run.require(
            after >= grid.cols or grid.attrs[row][after] == "",
            "the cell after the answer carries plain attributes (%r)"
            % (grid.attrs[row][after] if after < grid.cols else None),
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
    # band's top row downward, so what is left is the document -- which for one
    # prompt and one one-line answer is the answer. Counting the marker alone
    # is not enough: a stale, **truncated** copy of the answer's row does not
    # contain the marker and would be scored as a clean screen.
    #
    # The **echo of the prompt** is the one row this does not count, in either
    # direction, because the product does not promise it in either direction.
    # `docs/parity.md`'s `full-screen TUI` row says the echo is "not durable":
    # the activity row a starting turn adds takes its row from the bottom of
    # the document and paints over it, and Phase 1 repaints no transcript row.
    # Whether that repaint happened before the answer landed is a question
    # about how long the turn was visibly *under way* -- a fixture that answers
    # inside a tick never gets an activity row painted at all -- and the four
    # targets this suite gates disagree about it for exactly that reason: the
    # echo survived on `aarch64-apple-darwin` and both Linuxes and was
    # overpainted on `x86_64-apple-darwin`, from one commit. Asserting on a row
    # whose survival is a scheduling outcome is pinning the scheduler, which is
    # what the drain row above refuses to do; every *other* row is still
    # counted, so a second answer, a truncated one or a band row left behind
    # fails here exactly as before.
    echoed = ordinary_prompt
    remaining = [
        text
        for text in (grid.row_text(row) for row in range(grid.rows))
        if text.strip() and text != echoed
    ]
    run.require(
        remaining == ["answer: " + ordinary_marker],
        "a completed turn left exactly its answer on the screen, once (the prompt echo is "
        "not counted either way): %r" % (remaining,),
    )
    ordinary_fixture.stop()


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
    # Response-only **and** positional: on a 24-row screen the divider is row
    # 22, so this is the row directly above it. A needle matched anywhere would
    # be satisfied by the word appearing in the document.
    trial.wait_for("\x1b[21;1H• Thinking")
    # Off the activity row rather than off the wire: a second that ticked over
    # writes the one digit that changed, so the wire never carries `2s` whole.
    trial.wait_until(
        "the activity row's clock to reach two seconds",
        lambda _t: "2s" in trial.peek().row_text(trial.rows - 4),
    )
    grid = trial.grid("thinking")
    # By its place, its words and its clock -- and by whichever of its two faces
    # the marker is wearing, because a snapshot cannot choose. The marker keeps
    # a cell of its own whether it is lit or dark (`src/tui/activity.rs:54-59`:
    # `•` for half a second and then a space for half a second, so the text
    # beside it does not step left and right twice a second), so the row a grid
    # catches is `• Thinking  2s` half the time and `  Thinking  2s` the other
    # half.
    #
    # The first version of this assertion stripped the row and asked it to
    # start with `Thinking`, which is true **only in the dark half** -- a coin
    # toss that its own comment claimed to have avoided, and one CI called on
    # `aarch64-apple-darwin` with `'• Thinking  2s'` in its hand. What is
    # asserted here is what does not blink: the marker's cell holds one of its
    # two faces, and the word and the elapsed clock follow it, on the row
    # directly above the divider.
    row = grid.row_text(20)
    marked, said = row[:1], row[1:].strip()
    clock = said.rsplit(" ", 1)[-1]
    run.require(
        marked in (activity_marker, " ")
        and said.startswith("Thinking")
        and clock.endswith("s")
        and clock[:-1].isdigit(),
        "the activity row is directly above the divider, marked, and counting: %r" % row,
    )
    run.require(
        set(grid.row_text(21)) == {"─"},
        "and the divider is the row under it: %r" % grid.row_text(21),
    )
    run.require(
        any(run.nonce in body for body in quiet.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    run.require(grid.text().strip() != "", "the screen is not blank")
    trial.send(b"\x03\x03")
    run.require(trial.session.wait_exit() == ("exited", 130), "the quiet turn was interruptible")
    quiet.stop()

    # And the clock stops while xfx is waiting to be told what it may do,
    # because that interval measures the person rather than the model.
    marker = run.marker("frozen")
    fixture = start_fixture(run, fixtures.edit_then_finish(marker), name="frozen")
    frozen = run.trial("frozen-clock", gateway=fixture, mode="ask", notes=True).settled()
    frozen.send("edit the notes " + run.nonce + "\r")
    frozen.wait_for(PERMISSION_TITLE)
    before = elapsed_on_activity_row(frozen.text())
    frames_before = frozen.text().count(pty.FRAME_END)
    run.require(before is not None, "the activity row carries an elapsed time")
    deadline = time.time() + 3.0
    while time.time() < deadline:
        frozen.session.pump(0.1)
    text = frozen.text()
    after = elapsed_on_activity_row(text)
    # The band kept painting, which is what keeps the assertion below from
    # being vacuous: a session that had simply stopped drawing would trivially
    # report the same number.
    run.require(
        text.count(pty.FRAME_END) > frames_before,
        "the band kept painting while the question was up",
    )
    run.require(
        before is not None and before == after,
        "the clock stopped while xfx was waiting for a decision (%r -> %r)" % (before, after),
    )
    frozen.send(b"3")
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


def elapsed_on_activity_row(text):
    """The last elapsed time the activity row has shown, in seconds."""
    latest = None
    index = 0
    while True:
        found = text.find("• Thinking", index)
        if found < 0:
            found = text.find("edit_file ", index)
            if found < 0:
                return latest
        end = text.find("\x1b", found)
        segment = text[found : end if end > found else len(text)]
        digits = ""
        for character in segment:
            if character.isdigit():
                digits += character
            elif character == "s" and digits:
                latest = int(digits)
                digits = ""
            else:
                digits = ""
        index = found + 1


# ---------------------------------------------------------------------------
# 10. the approval panel
# ---------------------------------------------------------------------------


def scenario_10(run):
    """Every way of answering a question, and the outcome the fixture then sees."""
    marker = run.marker("panel")

    def ask(label):
        fixture = start_fixture(run, fixtures.edit_then_finish(marker), name=label)
        trial = run.trial(label, gateway=fixture, mode="ask", notes=True).settled()
        trial.send("edit the notes " + run.nonce + "\r")
        trial.wait_for(PERMISSION_TITLE)
        return fixture, trial

    # `1`: yes, once. This is also the scenario's discriminator.
    fixture, yes = ask("answer-yes")
    grid = yes.grid("panel")
    run.require(grid.find(PERMISSION_TITLE) is not None, "the panel names itself")
    run.require(grid.find("1. Yes") is not None, "the first choice is offered")
    run.require(grid.find(ALWAYS_WORDING) is not None, "the second says exactly what it grants")
    run.require(grid.find("3. No") is not None, "the third refuses")
    run.require(
        grid.find("for the rest of this session") is not None,
        "and the disclosure the line shell's prompt makes is made here too",
    )
    run.require(
        grid.row_text(grid.find("1. Yes")[0]).startswith("> "),
        "the caret sits on the choice Enter would take",
    )
    run.require(
        read(yes.notes) == "alpha\n", "the edit did not run before it was approved"
    )
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
        any("beta" in body for body in fixture.bodies()),
        "and the fixture saw the tool result the choice implies",
    )
    yes.send(b"\x04")
    run.require(yes.session.wait_exit() == ("exited", 0), "the session left at 0")
    fixture.stop()

    # `2`: yes, and stop asking for this request.
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
    run.require(
        "edit_file refused" in no.text(),
        "and the refusal reached the user rather than being silent",
    )
    no.send(b"\x04")
    no.session.wait_exit()
    fixture.stop()

    # The arrows and Enter: two steps down is the refusal, and taking it with
    # Enter is a different key from typing `3`.
    fixture, arrows = ask("answer-arrows")
    arrows.send(b"\x1b[B\x1b[B")
    arrows.wait_until("the caret to move to the third choice", lambda _t: caret_on(arrows, "3. No"))
    arrows.grid("caret")
    arrows.send(b"\r")
    arrows.wait_for(marker)
    run.require(read(arrows.notes) == "alpha\n", "the arrows chose, and Enter took the choice")
    arrows.send(b"\x04")
    arrows.session.wait_exit()
    fixture.stop()

    # Esc answers *this call* -- no -- and the turn goes on.
    fixture, escaped = ask("answer-escape")
    escaped.send(b"\x1b")
    escaped.wait_for(marker)
    run.require(read(escaped.notes) == "alpha\n", "Esc refused the call")
    run.require(
        escaped.session.state()[0] == "running", "and the turn it belonged to went on"
    )
    escaped.send(b"\x04")
    run.require(escaped.session.wait_exit() == ("exited", 0), "the escaped session left at 0")
    fixture.stop()

    # Ctrl-C is the interrupt it is everywhere else: it refuses **and** stops
    # the turn behind the question.
    fixture, cancelled = ask("answer-ctrl-c")
    cancelled.send(b"\x03")
    cancelled.wait_for("stopping the turn")
    run.require(read(cancelled.notes) == "alpha\n", "Ctrl-C refused the call")
    cancelled.send(b"\x04")
    run.require(
        cancelled.session.wait_exit() == ("exited", 0), "the interrupted session left at 0"
    )
    run.require(
        marker not in cancelled.session.settled_text(),
        "the interrupted turn never reached its own conclusion",
    )
    fixture.stop()


def caret_on(trial, choice):
    """Whether the panel's caret sits on `choice` in the grid as it stands."""
    grid = trial.peek()
    found = grid.find(choice)
    return found is not None and grid.row_text(found[0]).startswith("> ")


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


# ---------------------------------------------------------------------------
# 10b. an approval mid-turn does not deadlock
# ---------------------------------------------------------------------------


def scenario_10b(run):
    """An answer lands with a prompt already queued, and a third is refused."""
    marker = run.marker("midturn")
    fixture = start_fixture(run, fixtures.edit_then_finish(marker))
    trial = run.trial("mid-turn", gateway=fixture, mode="ask", notes=True).settled()

    trial.send("edit the notes " + run.nonce + "\r")
    # Queued **before** the question appears, because the panel takes the focus
    # when it does -- which is the other half of the same design.
    trial.send("queued while deciding\r")
    trial.wait_for("queued 1")
    trial.wait_for(PERMISSION_TITLE)

    trial.send(b"1")
    trial.wait_for(marker)
    run.require(
        read(trial.notes) == "beta\n",
        "the answer travelled on the control channel rather than behind the queued prompt",
    )
    run.require(
        any(run.nonce in body for body in fixture.bodies()),
        "the nonce this run minted is in the request xfx sent",
    )
    grid = trial.grid("answered")
    run.require(grid.text().strip() != "", "the screen is not blank")
    run.require(grid.find(marker) is not None, "the fixture's own marker is rendered")

    trial.send(b"\x03\x03")
    run.require(trial.session.wait_exit() == ("exited", 130), "the session left on a second Ctrl-C")
    fixture.stop()

    # A third submission is refused on the hint row, with its text kept.
    running = start_fixture(
        run, [fixtures.hang(fixtures.text_delta("d", "FIRST-TURN-RUNNING"))], name="queue"
    )
    queued = run.trial("third-refused", gateway=running).settled()
    queued.send("first " + run.nonce + "\r")
    queued.wait_for("FIRST-TURN-RUNNING")
    queued.send("second\r")
    queued.wait_for("queued 1")
    queued.send("third\r")
    queued.wait_for("one prompt is already queued")
    grid = queued.grid("refused")
    run.require(
        grid.find("one prompt is already queued") is not None,
        "the refusal is visible on the hint row",
    )
    run.require(
        composer_text(grid) == "third",
        "and the composer kept its text: %r" % composer_text(grid),
    )
    queued.send(b"\x03\x03")
    run.require(queued.session.wait_exit() == ("exited", 130), "the queue session left at 130")
    run.require(
        running.request_count() == 1,
        "neither the waiting prompt nor the refused one reached the wire (%d requests)"
        % running.request_count(),
    )
    running.stop()


# ---------------------------------------------------------------------------
# 11. Ctrl-C as a byte
# ---------------------------------------------------------------------------


def scenario_11(run):
    """`0x03` is a keystroke here, and no signal is delivered by it."""
    marker = run.marker("interrupt")
    fixture = start_fixture(run, [fixtures.hang(fixtures.text_delta("a", marker))])
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
    trial.wait_for("stopping the turn")
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

    def band_attributes(trial):
        grid = trial.grid("band")
        return (
            grid.attrs_of_row(grid.rows - 1),  # the hint row
            grid.attrs_of_row(grid.rows - 3),  # the divider
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
        light_attrs[0] and dark_attrs[0],
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
        grid.row_text(grid.rows - 1).strip() != "",
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
    divider = the_divider(run, reference, diffed)
    for row in range(divider, reference.rows):
        run.require(
            diffed.cells[row] == reference.cells[row],
            "band row %d differs from the full-repaint reference:\n  diff %r\n  full %r"
            % (row + 1, diffed.row_text(row), reference.row_text(row)),
        )
        run.require(
            diffed.attrs[row] == reference.attrs[row],
            "band row %d's attributes differ from the reference:\n  diff %r\n  full %r"
            % (row + 1, diffed.attrs_of_row(row), reference.attrs_of_row(row)),
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
        written_document(diffed, divider) == written_document(reference, divider),
        "the document differs from the full-repaint reference:\n  diff %r\n  full %r"
        % (written_document(diffed, divider), written_document(reference, divider)),
    )
    # A comparison of two blank screens would pass for anything, so the screen
    # both painters produced has to be a screen with the answer on it.
    run.require(
        reference.find(marker) is not None,
        "the reference screen never showed the answer, so the comparison is vacuous",
    )
    run.require(
        reference.row_text(reference.rows - 1).strip() != "",
        "the reference screen has no band on it, so the comparison is vacuous",
    )


def the_divider(run, reference, diffed):
    """The row index the band starts at, agreed by both screens.

    Found rather than computed: it is the one row that is the rule and nothing
    else, so a scenario that lost the band entirely fails here instead of
    comparing two empty ranges and passing.
    """
    def rule(grid):
        wanted = "\u2500" * grid.cols
        for row in range(grid.rows - 1, -1, -1):
            if grid.row_text(row) == wanted:
                return row
        return None

    found, expected = rule(diffed), rule(reference)
    run.require(expected is not None, "the full-repaint reference painted a divider at all")
    run.require(
        found == expected,
        "the two screens put the divider on different rows (%r vs %r)" % (found, expected),
    )
    if found is None or expected is None or found != expected:
        raise Failure("the band could not be located on both screens")
    return found


def written_document(grid, divider):
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
    """
    written = [line for line in grid.scrollback if line.strip()]
    written.extend(
        grid.row_text(row) for row in range(divider) if grid.row_text(row).strip()
    )
    return written


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
    # 24 rows, two composer rows: the rule on row 21, the composer on 22 and
    # 23, the hint row on 24.
    run.require(
        narrow.row_text(20) == "\u2500" * 80,
        "the rule spans the narrow screen before the resize: %r" % narrow.row_text(20),
    )
    run.require(
        composer_first_row(narrow) == 21,
        "the draft is on two composer rows to begin with (first row %r)"
        % composer_first_row(narrow),
    )

    # Where the wire stands before the resize, so what is scanned below is this
    # gesture's bytes and nothing else.
    wire = len(trial.session.captured)
    trial.resize(30, 120)

    # 30 rows, one composer row: the rule on row 28, the composer on 29, the
    # hint row on 30. Waited for on the **rule**, which is the one row whose
    # text says both coordinates at once -- where it is, and how wide.
    trial.wait_until(
        "the rule across the foot of the taller screen",
        lambda _t: trial.peek().row_text(27) == "\u2500" * 120,
    )
    wide = trial.grid("after-resize-band")
    run.require(
        wide.row_text(27) == "\u2500" * 120,
        "the rule spans the whole of the new screen: %r" % wide.row_text(27),
    )
    run.require(
        composer_first_row(wide) == 28,
        "the draft is back on one composer row, directly under the rule "
        "(first row %r)" % composer_first_row(wide),
    )
    run.require(
        composer_text(wide) == draft,
        "and it is still the whole draft: %r" % composer_text(wide),
    )
    run.require(
        wide.row_text(29).strip() != "",
        "the hint row is on the last row of the new screen",
    )
    # **No stale rows.** The rows the band owned on the old screen are the
    # document's now, and nothing in this phase repaints a document row -- so a
    # band that merely drew itself lower would leave a second rule and a second
    # hint row on the screen for the rest of the session.
    stale = [row + 1 for row in range(20, 24) if wide.row_text(row).strip() != ""]
    run.require(not stale, "the band the resize left behind is gone from rows %r" % stale)
    # **And nothing above them was touched.** The committed transcript is the
    # terminal's own document and its own scrollback; the terminal re-wrapped it
    # when it changed size, by rules xfx does not model, so a session that
    # repainted one of those rows would be overwriting text it cannot describe.
    # The rows this gesture is allowed to write are the band's own -- from the
    # top row the band had on the old screen (21, its rule) downwards.
    written = bytes(trial.session.captured)[wire:].decode("utf-8", "replace")
    above = sorted({row for row in rows_a_gesture_wrote_on(written) if row < 21})
    run.require(
        not above,
        "the resize wrote on the terminal's own document, at row(s) %r" % above,
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


def divider_row(grid):
    """The row the band's rule is on, or `None` before a band.

    Zero-based, like every other row index the oracle deals in. The boundary
    every claim below is stated against: the menu is *above* it and the composer
    is *below* it, and a menu painted on the wrong side of the rule would be a
    menu drawn over the draft it is about.
    """
    for row in range(grid.rows):
        text = grid.row_text(row)
        if text and text.strip("─") == "":
            return row
    return None


def menu_rows(grid):
    """The rows the menu is on, top first, and `[]` when there is no menu.

    The **contiguous run directly above the rule** whose rows have the menu's
    own shape -- a marker or an indent, then a slash name. Bounded that way
    rather than "every row above the rule that starts with a slash", because
    what is above the menu is the terminal's own document and a submitted
    `/model` is echoed into it: a reader that counted those would report a menu
    that had closed as a menu that was still up.
    """
    rule = divider_row(grid)
    if rule is None:
        return []
    rows = []
    for row in range(rule - 1, -1, -1):
        text = grid.row_text(row)
        if not (text.startswith("> /") or text.startswith("  /")):
            break
        rows.append(row)
    return list(reversed(rows))


def offered(grid):
    """Every command name the menu is offering, top row first.

    Read off the **cells** rather than out of the byte stream: a menu is a claim
    about the screen, and a name that reached the terminal inside a frame that
    was then overwritten is not on it.
    """
    # Past the two cells the marker owns, so the name is the name whether or
    # not this is the row a completion would take.
    return [grid.row_text(row)[2:].strip().split(" ")[0] for row in menu_rows(grid)]


def scenario_16(run):
    """Typing a slash opens a ranked menu that completes without taking focus.

    Item 13 on a real terminal. Five claims, and none of them is "a menu
    appeared": **where** it is (above the rule, in the rows a question would
    take), **what order** it is in (name-prefix, then alias-prefix, then
    substring), **who owns the caret** (the composer, always), **what Escape
    does** (closes it, and keeps it closed while the same word grows), and that
    a completed command **runs once** rather than being sent to a model.

    `/exit` is what supplies the alias tier: it is a real registry alias for
    `/quit` rather than a fixture, so the ranking asserted here is the ranking
    the product has.
    """
    marker = run.marker("menu")
    fixture = start_fixture(run, [fixtures.content_only(marker)])
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
        lambda _t: len(offered(trial.peek())) >= 6,
    )
    opened = trial.grid("menu-open")
    run.require(
        offered(opened)
        == ["/help", "/new", "/clear", "/model", "/version", "/quit"],
        "a bare slash lists every command in the order /help lists them: %r"
        % offered(opened),
    )
    rule = divider_row(opened)
    run.require(rule is not None, "the band has a rule to place the menu against")
    run.require(
        menu_rows(opened) == list(range(rule - 6, rule)),
        "the menu's six rows are the six directly above the rule: %r against a rule at %r"
        % (menu_rows(opened), rule),
    )
    run.require(
        opened.row_text(rule - 6).startswith("> ")
        and [opened.row_text(row).startswith("  ") for row in menu_rows(opened)[1:]]
        == [True] * 5,
        "exactly the top row is marked: %r"
        % [opened.row_text(row)[:2] for row in menu_rows(opened)],
    )
    run.require(
        opened.row_text(rule + 1) == "> /",
        "the composer is directly under the rule, holding the draft: %r"
        % opened.row_text(rule + 1),
    )
    # The caret, which is the one thing that says which of the two the next
    # keystroke goes to: in the composer, past the draft, never on the marked
    # row of the menu.
    run.require(
        (opened.row, opened.col) == (rule + 1, 3),
        "the menu took the caret off the text being typed: row %d col %d, rule at row %d"
        % (opened.row, opened.col, rule),
    )
    run.require(not opened.unknown, "xfx emitted only the sequences it declares: %r" % opened.unknown)

    # --- an exact prefix narrows it to one ---------------------------------
    trial.send("he")
    trial.wait_until(
        "the menu to narrow to the command the draft names",
        lambda _t: offered(trial.peek()) == ["/help"],
    )
    exact = trial.grid("menu-exact-prefix")
    run.require(
        offered(exact) == ["/help"],
        "an exact prefix offers exactly the command it names: %r" % offered(exact),
    )

    # --- the alias tier outranks the substring tier ------------------------
    #
    # `/e` names no command, so the only reason `/quit` can be on this list is
    # the `/exit` alias -- and it must be above the five commands that merely
    # have an `e` in them.
    trial.send(b"\x15")
    trial.send("/e")
    trial.wait_until(
        "the menu to rank the alias above the substrings",
        lambda _t: offered(trial.peek())[:1] == ["/quit"],
    )
    ranked = trial.grid("menu-alias-ranking")
    run.require(
        offered(ranked)
        == ["/quit", "/help", "/new", "/clear", "/model", "/version"],
        "the alias prefix outranks every substring, ties in registry order: %r"
        % offered(ranked),
    )
    top = menu_rows(ranked)[0]
    run.require(
        "/exit" in ranked.row_text(top),
        "the top row says which name put it there: %r" % ranked.row_text(top),
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
        offered(empty) == [],
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
        "esc again to clear" not in dismissed.row_text(dismissed.rows - 1),
        "Escape armed the clear gesture instead of closing the menu: %r"
        % dismissed.row_text(dismissed.rows - 1),
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
    # name is seven, so a caret at column nine is a caret one cell past the last
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
    trial.send("\r")
    trial.wait_until(
        "the completed command to be answered by the session",
        lambda _t: trial.peek().find("[shell] model=") is not None,
    )
    ran = trial.grid("menu-completed-ran")
    run.require(
        ran.find("[shell] model=") is not None,
        "the completed command was answered by the session",
    )
    run.require(
        len(fixture.bodies()) == before,
        "the completed command was answered without asking the provider (%d new request(s))"
        % (len(fixture.bodies()) - before),
    )
    run.require(not ran.unknown, "xfx emitted only the sequences it declares: %r" % ran.unknown)

    # --- and it is still a menu with the document pressed against it --------
    #
    # Every reading above was taken with blank rows above the menu, because the
    # session had barely written anything. This one is taken with the answer to
    # the command that just ran sitting **directly** on top of it: a one-match
    # menu is one row, so the row above it is the last row of the terminal's own
    # document. What a menu is is the block the band owns, not "everything above
    # the rule", and this is the reading that can tell the two apart.
    trial.send("/he")
    trial.wait_until(
        "a one-row menu with the document directly above it",
        lambda _t: composer_text(trial.peek()) == "/he",
    )
    pressed = trial.grid("menu-against-the-document")
    rule = divider_row(pressed)
    run.require(
        offered(pressed) == ["/help"],
        "the menu is the band's block rather than everything above the rule: %r"
        % offered(pressed),
    )
    run.require(
        pressed.row_text(rule - 2).strip() != "",
        "the document really is pressed against the menu (row %r is blank)"
        % (rule - 2),
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


SCENARIOS = {
    "1-launch-and-band-ownership": scenario_1,
    "2-cursor-probe-and-scrollback-push": scenario_2,
    "3-restore-matrix": scenario_3,
    "3b-shutdown-drain": scenario_3b,
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
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("scenario")
    parser.add_argument("--binary", required=True)
    parser.add_argument("--faulty", required=True)
    parser.add_argument("--evidence", required=True)
    arguments = parser.parse_args()
    if arguments.scenario == "--list":
        for name in SCENARIOS:
            print(name)
        return 0
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

# The fourteen Phase-1 scenarios of `.prd/06-qa-harness.md`, in its order, and
# then the Phase-2 ones this drive adds.
scenarios=(
	1-launch-and-band-ownership
	2-cursor-probe-and-scrollback-push
	3-restore-matrix
	3b-shutdown-drain
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
)

printf 'xfx smoke-tui\n  binary:   %s\n  faulty:   %s\n  evidence: %s\n\n' \
	"$binary" "$faulty" "$evidence"

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
