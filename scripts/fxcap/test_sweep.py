"""Tests for the parity sweep: cells.py, the ledger's classification, the
redaction, and sweep.py end to end over a fake capture script.

usage: scripts/fxcap/test-sweep.sh   (python3 -I scripts/fxcap/test_sweep.py)
"""

import atexit
import datetime
import glob
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest

HERE = os.path.dirname(os.path.realpath(__file__))
# No __pycache__ beside the scripts: the checkout holds only what is committed.
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)

import capfile  # noqa: E402
import cells  # noqa: E402
import inventory  # noqa: E402
import ledger as ledgermod  # noqa: E402
import redact  # noqa: E402

REPO = os.path.realpath(os.path.join(HERE, "..", ".."))
M1 = os.path.join(REPO, ".prd", "tui-phase4", "receipts", "m1")
LEDGER = os.path.join(HERE, "parity-ledger.toml")
SWEEP = os.path.join(HERE, "sweep.sh")


_SCRATCH = []
atexit.register(lambda: [shutil.rmtree(d, True) for d in _SCRATCH])


def scratch():
    d = tempfile.mkdtemp(prefix="fxsw-test.", dir=os.environ.get("TMPDIR") or "/tmp")
    _SCRATCH.append(d)
    return d


def slurp(path, mode="r"):
    with open(path, mode) as f:
        return f.read()


def compare(fx_text, xfx_text, led=None, cols=None):
    a, b = cells.parse(fx_text), cells.parse(xfx_text)
    if led is not None:
        a = cells.normalize(a, ledgermod.rules_for(led, "fx"), cols=cols)
        b = cells.normalize(b, ledgermod.rules_for(led, "xfx"), cols=cols)
    return cells.differing(a, b)


class Cells(unittest.TestCase):
    def test_attributes_carry_from_row_to_row(self):
        rows = cells.parse("\x1b[1mab\ncd\x1b[0m")
        self.assertEqual(rows[1][0], (0, "c", (("bold", True),)))

    def test_blank_cells_past_the_last_character_are_not_compared(self):
        self.assertEqual(compare("ab   \x1b[0m", "ab"), [])

    def test_a_space_under_a_background_is_compared(self):
        self.assertEqual(len(compare("\x1b[41m \x1b[0mx", " x")), 1)

    def test_a_spaces_foreground_is_not(self):
        self.assertEqual(compare("\x1b[31m \x1b[0ma", " a"), [])

    def test_a_wide_character_takes_two_columns(self):
        self.assertEqual(cells.parse("漢a")[0][1], (2, "a", ()))

    def test_osc_sequences_are_not_cells(self):
        self.assertEqual(compare("\x1b]8;;file:///x\x07ab\x1b]8;;\x07", "ab"), [])

    def test_every_row_is_compared_row_1_too(self):
        self.assertEqual([d[0] for d in compare("a\nb", "c\nb")], [0])

    def test_the_m1_receipts_copy_still_runs_and_says_the_same(self):
        # The receipt's command line, through its own copy of the script.
        wrapper = os.path.join(M1, "cells.py")
        same = subprocess.run(
            [sys.executable, "-I", wrapper, "fx-mcp-empty-40x120-list.ansi", "xfx-mcp-empty-40x120-list.ansi"],
            cwd=M1,
            capture_output=True,
            text=True,
        )
        self.assertEqual((same.returncode, same.stdout), (0, "0 rows differ in their cells (row 1 not compared)\n"))
        clock = subprocess.run(
            [sys.executable, "-I", wrapper, "fx-mcp-wire-40x120-turn.ansi", "xfx-mcp-wire-40x120-turn.ansi"],
            cwd=M1,
            capture_output=True,
            text=True,
        )
        self.assertEqual(clock.returncode, 1)
        self.assertTrue(clock.stdout.startswith("row 18: fx "), clock.stdout)
        self.assertTrue(clock.stdout.endswith("1 rows differ in their cells (row 1 not compared)\n"), clock.stdout)
        # And for every pair the receipt holds, the copy and the promoted
        # script print the same bytes.
        for fx in sorted(glob.glob(os.path.join(M1, "fx-*.ansi"))):
            xfx = os.path.join(M1, "x" + os.path.basename(fx))
            if not os.path.exists(xfx):
                continue
            outs = [
                subprocess.run([sys.executable, "-I", script, fx, xfx], capture_output=True, text=True)
                for script in (wrapper, os.path.join(HERE, "cells.py"))
            ]
            self.assertEqual(
                (outs[0].returncode, outs[0].stdout), (outs[1].returncode, outs[1].stdout), os.path.basename(fx)
            )


class Normalize(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.led = ledgermod.load(LEDGER, REPO)

    def test_the_two_banners_are_one(self):
        fx = "\x1b[1;38;5;255m𝒇x\x1b[0m\x1b[38;5;245m v0.0.13 · Run /help for commands\x1b[0m\nrest"
        for build in (
            # The user's llmux format of 2026-11-10, with an id and without.
            "v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50)",
            "v0.2.0 (preview 2026-11-10-0019-ec40c7ad2e50)",
            "v0.2.0 (debug)",
            # A base build from before it.
            "v0.1.0-1a94cdf [debug]",
            "v0.1.0",
        ):
            xfx = "\x1b[38;5;240mx\x1b[1;38;5;255m𝒇x\x1b[0m\x1b[38;5;245m %s · Run /help for commands\x1b[0m\nrest" % build
            self.assertEqual(compare(fx, xfx, self.led), [], build)
            self.assertEqual(len(compare(fx, xfx)), 1, "and they differ before")

    # A wrapped banner as the screen leaves it, at the width it was taken
    # at: at 50 and 60 columns xfx's banner takes two rows (72 cells for a
    # debug build, 74 for release and preview; 52 before 0.2.0). The 50- and
    # 60-column debug ones are the narrow sweep's captures
    # (`.prd/tui-phase5/receipts/preview-0.2.0-dated/captures/`).
    WRAPPED = (
        (50, ["x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·", "Run /help for commands"]),
        (50, ["x𝒇x v0.2.0 (release 2026-11-10-0019-ec40c7ad2e50)", "· Run /help for commands"]),
        (50, ["x𝒇x v0.1.0-1a94cdf [debug] · Run /help for", "commands"]),
        (60, ["x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) · Run /help", "for commands"]),
        (60, ["x𝒇x v0.2.0 (release 2026-11-10-0019-ec40c7ad2e50) · Run", "/help for commands"]),
        # Narrower still, three rows.
        (32, ["x𝒇x v0.2.0 (preview", "2026-11-10-0019-ec40c7ad2e50) ·", "Run /help for commands"]),
    )
    FX_BANNER = "\x1b[1;38;5;255m𝒇x\x1b[0m\x1b[38;5;245m v0.0.16 · Run /help for commands\x1b[0m\n\n  rest\nlast"

    @staticmethod
    def xfx_rows(rows):
        return "\x1b[38;5;245m" + "\n".join(rows) + "\x1b[0m\n\n  rest\nlast"

    def test_a_banner_the_screen_wrapped_is_read_as_the_banner(self):
        # Read back as one, and the rows under it stand where fx's do.
        for cols, rows in self.WRAPPED:
            xfx = self.xfx_rows(rows)
            self.assertEqual(compare(self.FX_BANNER, xfx, self.led, cols), [], (cols, rows))
            self.assertGreater(len(compare(self.FX_BANNER, xfx)), 1, "and they differ before")

    def test_a_banner_broken_where_the_screen_would_not_wrap_it_stays_broken(self):
        # A line that fits is never wrapped: the same rows on a 120-column
        # screen are a hard line break, a layout change the comparison must
        # see -- as on a screen of no known width.
        for cols, rows in self.WRAPPED:
            xfx = self.xfx_rows(rows)
            for wide in (120, 74, None):
                self.assertGreater(len(compare(self.FX_BANNER, xfx, self.led, wide)), 1, (wide, rows))
        # Narrow enough to wrap, broken where the wrap would not break it:
        # the first row could still have taken the next word.
        for cols, rows in (
            (50, ["x𝒇x v0.2.0 (debug", "2026-11-10-0019-ec40c7ad2e50) · Run /help for", "commands"]),
            (60, ["x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·", "Run /help for commands"]),
            # A row wider than the screen is no row of it.
            (40, ["x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·", "Run /help for commands"]),
            # A word broken inside it fills its row.
            (50, ["x𝒇x v0.2.0 (debug 2026-11-10-00", "19-ec40c7ad2e50) · Run /help for commands"]),
        ):
            self.assertGreater(len(compare(self.FX_BANNER, self.xfx_rows(rows), self.led, cols)), 1, (cols, rows))

    def test_only_a_whole_banner_is_joined(self):
        fx = "𝒇x v0.0.16 · Run /help for commands\nrest"
        for xfx in (
            # The next row is not the rest of the banner.
            "x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·\nrest",
            # Rows that make more than a banner.
            "x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·\nRun /help for commands now\nrest",
            # A blank row between is not a wrap.
            "x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·\n\nRun /help for commands\nrest",
        ):
            self.assertNotEqual(compare(fx, xfx, self.led, 50), [], xfx)
        rule = next(r for r in ledgermod.rules_for(self.led, "xfx") if r["id"] == "banner")
        self.assertTrue(rule["wrapped"])
        rows = cells.join_wrapped(cells.parse("a\nb\nc"), rule["pattern"], 50)
        self.assertEqual([cells.text_of(r) for r in rows], ["a", "b", "c"], "nothing to join")
        # The whole match is the helper's own (`fullmatch`): an unanchored
        # pattern still joins only rows that are all of it.
        loose = re.compile(r"x𝒇x v\S+ \(debug \S+\) · Run /help for commands")
        rows = cells.join_wrapped(
            cells.parse("x𝒇x v0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50) ·\nRun /help for commands now"), loose, 50
        )
        self.assertEqual(len(rows), 2, "a longer line is not the banner")

    def test_each_snap_is_read_at_its_own_width(self):
        # capfile's `cols_at`: the run's width, then each `resize` before the
        # snap (`h1-resize`: `small` at 20x60).
        u = next(u for u in capfile.units(os.path.join(HERE, "scenarios")) if u.name == "h1-resize")
        self.assertEqual(
            [u.xfx.cols_at(label, "40x120") for label in ("start", "small", "large", "nosuch")], [120, 60, 120, None]
        )

    def test_a_banner_is_its_whole_row(self):
        fx = "𝒇x v0.0.13 · Run /help for commands"
        self.assertEqual(len(compare(fx, "x𝒇x v0.1.0 · Run /help for commands now", self.led)), 1)

    def test_fx_is_read_as_xfx_on_fxs_side_only(self):
        self.assertEqual(compare("  /version   show the fx version", "  /version   show the xfx version", self.led), [])
        self.assertEqual(len(compare("  /version   show the fx version", "  /version   show the fx version", self.led)), 1)

    def test_the_profile_directory_moves_what_follows_it(self):
        self.assertEqual(compare("see ~/.fx/mcp.json and .fx.json  x", "see ~/.xfx/mcp.json and .xfx.json  x", self.led), [])

    def test_the_name_keeps_its_attributes(self):
        self.assertEqual(compare("\x1b[1mfx\x1b[0m!", "\x1b[1mxfx\x1b[0m!", self.led), [])
        self.assertEqual(len(compare("\x1b[1mfx\x1b[0m!", "xfx!", self.led)), 1)

    def test_names_that_are_not_the_product_are_left(self):
        rules = ledgermod.rules_for(self.led, "fx")
        row = cells.normalize(cells.parse("FX_AUTO_UPGRADE=0 𝒇x prefix fxcap fx_trace"), rules)[0]
        self.assertEqual(cells.text_of(row), "FX_AUTO_UPGRADE=0 𝒇x prefix fxcap fx_trace")

    def test_a_widened_name_may_take_its_column_from_the_padding(self):
        fx_row, rules = "  /usage   show local fx tokens" + " " * 20 + "Account", ledgermod.rules_for(self.led, "fx")
        xfx_row = "  /usage   show local xfx tokens" + " " * 19 + "Account"
        a, b = cells.parse(fx_row), cells.parse(xfx_row)
        readings = [cells.normalize(a, rules, absorb=w) for w in ("after", "before")]
        self.assertEqual(len(cells.differing(cells.normalize(a, rules), b)), 1, "by position alone it moved")
        self.assertEqual(cells.differing(cells.normalize(a, rules), b, readings), [])
        warn = "full access · model-a" + " " * 52 + "Full access enabled: fx permission checks disabled"
        right = "full access · model-a" + " " * 51 + "Full access enabled: xfx permission checks disabled"
        a = cells.parse(warn)
        readings = [cells.normalize(a, rules, absorb=w) for w in ("after", "before")]
        self.assertEqual(cells.differing(cells.normalize(a, rules), cells.parse(right), readings), [])
        # The padding gives one column, not the text: a row that differs in
        # anything else still differs.
        self.assertEqual(len(cells.differing(cells.normalize(a, rules), cells.parse(right + "!"), readings)), 1)

    def test_each_runs_own_clock_and_ids(self):
        pairs = [
            ("  2s (↑6 ↓40)", "  1s (↑6 ↓40)"),
            ("• Generating (3s) (↑4 ↓6)", "  Generating (4s) (↑4 ↓6)"),
            ("  2026-11-03 12:45:01.234 UTC · Tool", "  2026-11-03 12:46:11.004 UTC · Tool"),
            ("  2026-11-03 21:45:01.234 KST · Prompt", "  2026-11-03 21:46:11.004 KST · Prompt"),
            ("  2026-11-03 18:15:01.234 +0530 · Tool", "  2026-11-03 18:16:11.004 +0530 · Tool"),
            ("Pane is dead (status 0, Tue Nov  3 21:51:45 2026)", "Pane is dead (status 0, Tue Nov  3 21:52:01 2026)"),
            ("Continue session with: fx --resume 9yclcdoOjcsQ", "Continue session with: xfx --resume 1793709927101-83b1c1b7b10564ac"),
            ("* network: provider: gateway · model: fake/model-a · 444ms", "* network: provider: gateway · model: fake/model-a · 101ms"),
            ("  xfx-trace-2026-11-03-125130-d316652f96c0.md.", "  xfx-trace-2026-11-03-125146-dad28a490c1e.md."),
            ("saved at /t/fxcap.Lq26sv/fxtmp/", "saved at /t/fxcap.uozlti/xfxtmp/"),
            ("* version: 0.0.13", "* version: 0.1.0"),
            ("* version: 0.0.16", "* version: 0.2.0 (debug 2026-11-10-0019-ec40c7ad2e50)"),
            ("* version: 0.0.16", "* version: 0.2.0 (release)"),
            ("  build_revision=c1db919112c5", "  build_revision=1a94cdff75a8"),
        ]
        for fx, xfx in pairs:
            self.assertEqual(compare(fx, xfx, self.led), [], fx)
        # What is the product's, not the run's, is still compared.
        for fx, xfx in (
            ("  2s (↑6 ↓40)", "  1s (↑6 ↓41)"),
            ("Pane is dead (status 0, Tue Nov  3 21:51:45 2026)", "Pane is dead (status 1, Tue Nov  3 21:52:01 2026)"),
            ("Continue session with: fx --resume 9yclcdoOjcsQ", "Continue session with: xfx --resume 9yclcdoOjcsQ"),
            ("~ Generating (3s) (↑4 ↓6)", "~ Thinking (3s) (↑4 ↓6)"),
            # The out count of a turn still streaming: a held stream makes it the same on both.
            ("• Generating (3s) (↑4 ↓61)", "  Generating (4s) (↑4 ↓63)"),
            # The clock is the run's; the zone after it is the product's.
            ("  2026-11-03 21:45:01.234 KST · Tool", "  2026-11-03 12:46:11.004 UTC · Tool"),
            ("  2026-11-03 18:15:01.234 IST · Tool", "  2026-11-03 18:16:11.004 +0530 · Tool"),
        ):
            self.assertEqual(len(compare(fx, xfx, self.led)), 1, (fx, xfx))

    def test_the_aligned_count_reads_a_shift_as_one_row(self):
        a = cells.parse("a\nb\nc\nd")
        b = cells.parse("a\nX\nb\nc\nd")
        self.assertEqual(len(cells.differing(a, b)), 4)
        self.assertEqual(cells.unaligned(a, b), (0, 1))

    def test_a_binarys_tmpdir_is_read_after_the_run_directory_only(self):
        # fx's `fxtmp` is xfx's `xfxtmp` where it is the binary's own TMPDIR:
        # under the run directory, as capture.sh writes it at any width,
        # resolved or not, zero-filled as units wrote it by hand before
        # LOCAL-PATHS, or masked by run-directory.
        for run in ("<scratch______________>", "<scratch:real_________>", "<scratch>", "/t/fxcap.Lq26sv"):
            self.assertEqual(compare("saved at %s/fxtmp/a.md" % run, "saved at %s/xfxtmp/a.md" % run, self.led), [], run)
        # Anywhere else an `fxtmp` is a name the binary printed and is compared:
        # under another root, an escaped literal, plain text ending `_>`, and
        # the short form `<s__>`, which no run directory takes (it is never
        # narrower than `<scratch>`: `$TMPDIR/fxcap.XXXXXX`, 13 columns or more).
        for prefix in ("<work___>", "<work:real__>", "<tmp______>", "<!scratch__>", "a_>", "<s__>", "<S__>", "x/"):
            self.assertEqual(len(compare("saved at %s/fxtmp/a.md" % prefix, "saved at %s/xfxtmp/a.md" % prefix, self.led)), 1, prefix)
        # And only the name itself, with a directory under it: `/` is the one
        # follower the tracked captures hold (w2's and w3's reports and
        # images). Any other character can be part of a name, so a sibling
        # that extends `fxtmp` by one -- or the name before anything but `/`
        # -- is compared.
        run = "<scratch______________>"
        self.assertEqual(compare("a %s/fxtmp/b.md" % run, "a %s/xfxtmp/b.md" % run, self.led), [])
        for name in ("fxtmp-old/a", "fxtmp.bak", "fxtmp_x", "fxtmpx", "fxtmp~", "fxtmp+1", "fxtmp\u00e9/a",
                     "fxtmp%C3%A9/a", "fxtmp\u4e2d/a", "fxtmp@x", "fxtmp=x", "fxtmp#1", "fxtmp(1)", "fxtmp!", "fxtmp<x",
                     "fxtmp,backup/a", "fxtmp backup/a", "fxtmp:x", "fxtmp)", "fxtmp", "fxtmp\tx", 'fxtmp"', "fxtmp'",
                     "fxtmp]", "fxtmp}", "fxtmp>", "fxtmp;", "fxtmp|"):
            with self.subTest(name=name):
                self.assertEqual(len(compare("a %s/%s" % (run, name), "a %s/x%s" % (run, name), self.led)), 1)

    def test_capture_run_directories_are_masked_to_the_same_length(self):
        self.assertEqual(compare("ws=/t/fxcap.Kun3pm/work x", "ws=/t/fxcap.uqw49D/work x", self.led), [])
        self.assertEqual(compare("ws=/t/fxcap.Ku…", "ws=/t/fxcap.uq…", self.led), [])
        self.assertEqual(len(compare("ws=/t/fxcap.Ku/work", "ws=/t/fxcap.uqw/work", self.led)), 1)


LEDGER_TOML = textwrap.dedent(
    """
    [[pin]]
    name = "a"
    version = "1"
    sha256 = "%s"

    [[pin]]
    name = "b"
    version = "2"
    sha256 = "%s"

    [[divergence]]
    id = "words"
    class = "user-decision"
    pins = ["a"]
    scenarios = ["menu*"]
    snaps = ["open"]
    fx_row = '^fx says'
    xfx_row = '^xfx says'
    reason = "r"
    owner = "user decision words"
    resolution = "the user picks"
    expires = "pin:b"
    source = "scripts/fxcap/cells.py:1"

    [[divergence]]
    id = "row-three"
    class = "replaced-in-next-pin"
    pins = ["a"]
    scenarios = ["menu"]
    snaps = ["*"]
    rows = ["3-4"]
    reason = "r"
    owner = "P-status"
    resolution = "the next pin's panel"
    expires = "pin:b"
    source = "scripts/fxcap/cells.py:1-2"

    [[divergence]]
    id = "extra"
    class = "user-decision"
    pins = ["a"]
    scenarios = ["push"]
    snaps = ["*"]
    xfx_row = '^  2 = '
    reason = "r"
    owner = "user decision scope"
    resolution = "the user picks"
    expires = "date:2999-01-01"
    source = "scripts/fxcap/cells.py:1"
    """
) % ("1" * 64, "2" * 64)


def ledger_file(text, extra=""):
    d = scratch()
    p = os.path.join(d, "ledger.toml")
    with open(p, "w") as f:
        f.write(text + extra)
    return p


class Ledger(unittest.TestCase):
    def setUp(self):
        self.led = ledgermod.load(ledger_file(LEDGER_TOML), REPO)
        self.divs = ledgermod.active(self.led, "a")

    def classify(self, fx, xfx, unit="menu", label="open", divs=None):
        a, b = cells.parse(fx), cells.parse(xfx)
        return ledgermod.classify(a == b, cells.differing(a, b), unit, label, self.divs if divs is None else divs)

    def test_identical(self):
        self.assertEqual(self.classify("a\nb", "a\nb")[0], "identical")

    def test_normalized_identical(self):
        a = cells.normalize(cells.parse("fx"), ledgermod.rules_for(ledgermod.load(LEDGER, REPO), "fx"))
        b = cells.parse("xfx")
        self.assertEqual(ledgermod.classify(False, cells.differing(a, b), "s", "l", [])[0], "normalized-identical")

    def test_a_row_the_ledger_approves(self):
        verdict, rows = self.classify("fx says\nb", "xfx says\nb")
        self.assertEqual((verdict, rows), ("approved-divergence", [(1, ["words"])]))

    def test_a_row_replaced_in_the_next_pin(self):
        self.assertEqual(self.classify("a\nb\nc", "a\nb\nC")[0], "replaced-in-next-pin")

    def test_one_row_not_covered_makes_the_pair_a_defect(self):
        verdict, rows = self.classify("fx says\nb", "xfx says\nB")
        self.assertEqual((verdict, rows), ("unresolved-defect", [(1, ["words"]), (2, [])]))

    def test_the_selector_is_scenario_and_snap(self):
        self.assertEqual(self.classify("fx says", "xfx says", label="closed")[0], "unresolved-defect")
        self.assertEqual(self.classify("fx says", "xfx says", unit="other")[0], "unresolved-defect")

    def test_an_approval_is_for_its_pins_only(self):
        self.assertEqual(ledgermod.active(self.led, "b"), [])
        self.assertEqual(ledgermod.active(self.led, None), [])
        self.assertEqual(self.classify("fx says", "xfx says", divs=ledgermod.active(self.led, "b"))[0], "unresolved-defect")

    def test_an_xfx_row_never_vouches_for_the_fx_row_beside_it(self):
        # `extra` names an xfx row only: the fx row at the same position is
        # its own text, which nothing approves.
        a, b = cells.parse("a\nimportant\nz"), cells.parse("a\n  2 = allow\nz")
        self.assertEqual(ledgermod.classify(False, cells.differing(a, b), "push", "s", self.divs)[0], "unresolved-defect")
        # Even fx text the xfx regex would match is fx's, and not approved.
        a = cells.parse("a\n  2 = workspace\nz")
        self.assertEqual(ledgermod.classify(False, cells.differing(a, b), "push", "s", self.divs)[0], "unresolved-defect")
        self.assertIsNone(self.cascade("a\n  2 = workspace\nz", "a\n  2 = allow\nz")[0])
        a, b = cells.parse("a\n\nz"), cells.parse("a\n  2 = allow\nz")
        self.assertEqual(ledgermod.classify(False, cells.differing(a, b), "push", "s", self.divs)[0], "approved-divergence")

    def cascade(self, fx, xfx, unit="push"):
        return ledgermod.cascade(cells.parse(fx), cells.parse(xfx), unit, "s", self.divs, cells.text_of)

    def test_a_row_the_approved_row_pushed_down_is_a_cascade(self):
        fx = "head\nchoice\n\nfooter\n\n\n"
        xfx = "head\nchoice\n  2 = allow this\n\nfooter\n\n"
        a, b = cells.parse(fx), cells.parse(xfx)
        self.assertEqual(ledgermod.classify(False, cells.differing(a, b), "push", "s", self.divs)[0], "unresolved-defect")
        verdict, left = self.cascade(fx, xfx)
        self.assertEqual(verdict, "approved-divergence-cascade")
        self.assertEqual([(side, row) for _, side, row, _, _ in left], [("xfx", 3)])

    def test_a_footer_pushed_off_the_screen_still_fails(self):
        fx = "head\nchoice\nfooter"
        xfx = "head\nchoice\n  2 = allow this"
        verdict, left = self.cascade(fx, xfx)
        self.assertIsNone(verdict)
        self.assertIn(("fx", 3, "footer", []), [(s, r, t, ids) for _, s, r, t, ids in left])

    def test_an_unapproved_row_beside_the_approved_one_fails(self):
        self.assertIsNone(self.cascade("a\nb\nc", "a\n  2 = allow\nnew row\nb\nc")[0])

    def test_a_blank_separator_goes_with_its_row_but_blanks_alone_do_not(self):
        self.assertEqual(self.cascade("a\nb\nc", "a\n  2 = allow\n\nb\nc")[0], "approved-divergence-cascade")
        self.assertIsNone(self.cascade("a\nb\nc", "a\n\n\nb\nc")[0])

    def test_a_cascade_needs_a_divergence_for_this_pin(self):
        self.assertIsNone(
            ledgermod.cascade(
                cells.parse("a\nb"), cells.parse("a\n  2 = allow\nb"), "push", "s", ledgermod.active(self.led, "b"), cells.text_of
            )[0]
        )

    def test_a_date_expiry_lapses(self):
        import datetime

        ids = lambda day: [d["id"] for d in ledgermod.active(self.led, "a", today=day)]  # noqa: E731
        self.assertIn("extra", ids(datetime.date(2998, 12, 31)))
        self.assertNotIn("extra", ids(datetime.date(2999, 1, 2)))
        self.assertEqual([d["id"] for d in ledgermod.expired(self.led, "a", today=datetime.date(2999, 1, 2))], ["extra"])

    def test_malformed_entries_are_refused(self):
        entry = textwrap.dedent(
            """
            [[divergence]]
            id = "x"
            class = "%s"
            pins = ["%s"]
            scenarios = ["*"]
            snaps = ["*"]
            %s
            reason = "r"
            owner = "%s"
            resolution = "%s"
            expires = "%s"
            source = "%s"
            """
        )
        good = ["user-decision", "a", "rows = [1]", "user decision t", "decided", "pin:b", "scripts/fxcap/cells.py:1"]
        ledgermod.load(ledger_file(LEDGER_TOML, entry % tuple(good)), REPO)
        for at, value in (
            (0, "deliberate"),  # no longer a reason
            (0, "maybe"),
            (1, "c"),  # no such pin
            (2, ""),  # no row selector
            (2, "whole_snap = true\nrows = [1]"),
            (3, ""),  # no owner
            (3, "someone"),  # not a unit ID or a user decision
            (4, ""),  # no resolution
            (5, ""),  # no expiry
            (5, "never"),
            (5, "pin:a"),  # expires at the pin it is approved for
            (5, "pin:zz"),
            (5, "date:2026-13-40"),
            (6, "scripts/fxcap/cells.py:100000"),
            (6, "no/such/file.md:1"),
            (6, "scripts/fxcap/cells.py"),
        ):
            bad = list(good)
            bad[at] = value
            with self.assertRaises(ledgermod.LedgerError, msg=bad):
                ledgermod.load(ledger_file(LEDGER_TOML, entry % tuple(bad)), REPO)

    def test_causes_approve_nothing_and_a_defect_names_both_lines(self):
        cause = textwrap.dedent(
            """
            [[cause]]
            id = "c"
            kind = "%s"
            owner = "unowned"
            scenarios = ["*"]
            snaps = ["*"]
            fx_row = '^fx says'
            why = "w"
            %s
            """
        )
        ok = 'xfx = "scripts/fxcap/cells.py:1"\nfx = "src/ui/x.zig:3@c1db9191"'
        led = ledgermod.load(ledger_file(LEDGER_TOML, cause % ("defect", ok)), REPO)
        self.assertEqual([c["id"] for c in led["causes"]], ["c"])
        self.assertNotIn("c", [d["id"] for d in ledgermod.active(led, "a")])
        for kind, extra in (
            ("defect", ""),
            ("defect", 'xfx = "scripts/fxcap/cells.py:1"\nfx = "src/ui/x.zig:3"'),
            ("defect", 'xfx = "no/such.rs:1"\nfx = "src/ui/x.zig:3@c1db9191"'),
            ("nondeterminism", ""),
            ("guess", ""),
        ):
            with self.assertRaises(ledgermod.LedgerError, msg=(kind, extra)):
                ledgermod.load(ledger_file(LEDGER_TOML, cause % (kind, extra)), REPO)

    def test_a_cause_past_its_date_covers_nothing(self):
        cause = textwrap.dedent(
            """
            [[cause]]
            id = "c"
            kind = "unit"
            owner = "U-1"
            scenarios = ["*"]
            snaps = ["*"]
            fx_row = '^fx says'
            why = "w"
            %s
            """
        )
        led = ledgermod.load(ledger_file(LEDGER_TOML, cause % 'expires = "date:2026-01-02"'), REPO)
        live = lambda day: [c["id"] for c in ledgermod.live_causes(led, datetime.date(2026, 1, day))]
        self.assertEqual(live(2), ["c"])
        self.assertEqual(live(3), [])
        led = ledgermod.load(ledger_file(LEDGER_TOML, cause % ""), REPO)
        self.assertEqual([c["id"] for c in ledgermod.live_causes(led)], ["c"])
        for bad in ('expires = "pin:b"', 'expires = "2026-01-02"', 'expires = "date:2026-02-30"'):
            with self.assertRaises(ledgermod.LedgerError, msg=bad):
                ledgermod.load(ledger_file(LEDGER_TOML, cause % bad), REPO)

    def test_the_repos_ledger_loads_and_every_source_it_cites_exists(self):
        led = ledgermod.load(LEDGER, REPO)
        names = [p["name"] for p in led["pins"]]
        self.assertIn("c1db919", names)
        for d in led["divergences"]:
            self.assertTrue(d["source"], d["id"])
            self.assertTrue(set(d["pins"]) <= set(names), d["id"])
            self.assertIn(d["class"], ledgermod.CLASSES, d["id"])
            self.assertTrue(d["owner"] and d["resolution"] and d["expires"], d["id"])


class Redaction(unittest.TestCase):
    def setUp(self):
        self.red = redact.Redactor({"/Users/someone/repo": "<repo>", "/private/tmp/claude-501/out": "<out>"})  # local-path-ok: the redactor's input

    def test_known_roots_and_every_other_local_path(self):
        text = (
            "repo /Users/someone/repo/scripts/fxcap; out /private/tmp/claude-501/out/runs/x; "  # local-path-ok: the redactor's input
            "home /Users/someone/.xfx/settings.json; scratch /private/tmp/claude-501/fxcap.Kun3pm/home/.fx; "  # local-path-ok
            "temp /var/folders/ab/T/x; tmp /tmp/fxcap.a/s; volume /Volumes/Disk/fx"  # local-path-ok
        )
        got = self.red.public(text)
        for leak in ("/Users/", "/private/", "/var/", "/tmp/", "/Volumes/", "someone"):
            self.assertNotIn(leak, got)
        self.assertIn("<repo>/scripts/fxcap", got)
        self.assertIn("<out>/runs/x", got)
        self.assertEqual(got.count("<local-path>"), 5, got)

    def test_a_path_under_a_capture_placeholder_is_left_and_a_redirect_is_not(self):
        # capture.sh writes a run's HOME as `<scratch>/home`: a summary row
        # that names it keeps it whole. A `>` that is no placeholder is not
        # one: a redirect into a home directory is still a local path.
        text = "mcp <scratch>/home/.fx/mcp.json; ws <work>/home/a; log >" + "/ho" + "me/bob/x"
        self.assertEqual(self.red.public(text), "mcp <scratch>/home/.fx/mcp.json; ws <work>/home/a; log ><local-path>")

    def test_screen_text_that_is_no_local_path_is_left(self):
        text = "  /help   show available slash commands   ~/.xfx/mcp.json"
        self.assertEqual(self.red.public(text), text)

    def test_the_writer_refuses_what_survives(self):
        retired = "f" + "x" + "r"
        d = scratch()
        path = os.path.join(d, "summary.md")
        # A local path is rewritten, never refused; what no rewrite reaches
        # -- the retired name outside a path -- stops the write.
        self.assertEqual(self.red.public("a" + os.path.expanduser("~") + "/x"), "a<local-path>")
        for text in ("worktrees/." + retired + "ef/bin/fx", "the " + retired.upper() + " build"):
            with self.assertRaises(redact.RedactionError):
                self.red.write(path, text)
            self.assertFalse(os.path.exists(path))
        with self.assertRaises(redact.RedactionError):
            self.red.check("/Users/someone/x")


class Capfile(unittest.TestCase):
    def test_pairs_prefixes_waits_and_labels(self):
        d = scratch()
        files = {
            "w-fx.cap": "wait 1\nsnap a\n",
            "w-xfx.cap": "wait 2\nsnap a\n",
            "solo.cap": "fx: wait 4\nxfx: wait 125\nwait 1\nsnap a\nsnapfull a\nsnap a\ntype /model gro\ntype /mo\n",
        }
        for name, text in files.items():
            with open(os.path.join(d, name), "w") as f:
                f.write(text)
        units = {u.name: u for u in capfile.units(d)}
        self.assertEqual(sorted(units), ["solo", "w"])
        self.assertEqual((units["w"].fx.name, units["w"].xfx.name), ("w-fx", "w-xfx"))
        solo = units["solo"]
        self.assertEqual((solo.fx.waits, solo.xfx.waits), (5.0, 126.0))
        self.assertEqual(solo.labels(), ["a", "a.full"])
        self.assertEqual(solo.fx.duplicate_snaps, ["a"])
        self.assertEqual(solo.fx.typed_commands(), ["/model", "/mo"])

    def test_a_cap_that_is_no_regular_file_inside_is_refused_without_reading_it(self):
        # Each in a process of its own group, killed whole after 30 s: the
        # FIFO's open would never return, and a hang must fail, not wait.
        for target in ("fifo", "outside", "big"):
            with self.subTest(target=target):
                d = scratch()
                scen = os.path.join(d, "scen")
                os.makedirs(scen)
                with open(os.path.join(scen, "ok.cap"), "w") as f:
                    f.write("snap a\n")
                if target == "fifo":
                    os.mkfifo(os.path.join(d, "never"))
                    os.symlink(os.path.join(d, "never"), os.path.join(scen, "bad.cap"))
                elif target == "outside":
                    with open(os.path.join(d, "out.cap"), "w") as f:
                        f.write("snap a\n")
                    os.symlink(os.path.join(d, "out.cap"), os.path.join(scen, "bad.cap"))
                else:
                    with open(os.path.join(scen, "bad.cap"), "w") as f:
                        f.write("# x\n" * ((1 << 20) // 4 + 1))  # past capfile.MAX_BYTES, 1 MiB
                p = subprocess.Popen(
                    [sys.executable, "-I", "-c", "import sys; sys.dont_write_bytecode = True; sys.path.insert(0, %r)\n"
                     "import capfile\ntry:\n    capfile.units(%r)\nexcept ValueError as e:\n    print(e)\n" % (HERE, scen)],
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True,
                )
                try:
                    out, err = p.communicate(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.communicate()
                    self.fail("capfile.units did not finish in 30 s")
                self.assertEqual(p.returncode, 0, err)
                self.assertIn("bad.cap is not a regular file", out)

    def test_the_repos_scenarios(self):
        units = capfile.units(os.path.join(HERE, "scenarios"))
        names = [u.name for u in units]
        self.assertIn("compact-inturn", names)
        self.assertNotIn("compact-inturn-fx", names)

    def test_no_snap_of_the_repos_scenarios_races_a_timer(self):
        raced = [line for u in capfile.units(os.path.join(HERE, "scenarios")) for side in ("fx", "xfx") for line in capfile.races(u.reading(side))]
        self.assertEqual(raced, [], "\n".join(raced))

    YOLO = "fx: env FX_PERMISSION_MODE=yolo\nxfx: env XFX_PERMISSION_MODE=yolo\n"

    def raced(self, cap, files=None):
        """Each race of a scenario `t` made of `cap`, as (binary, snap line)."""
        d = scratch()
        for name, text in dict(files or {}, **{"t.cap": cap}).items():
            with open(os.path.join(d, name), "w") as f:
                f.write(text)
        (unit,) = capfile.units(d)
        out = []
        for side in ("fx", "xfx"):
            for line in capfile.races(unit.reading(side)):
                self.assertTrue(line.startswith("t (%s): `" % side), line)
                out.append((side, line.split("`")[1]))
        return out

    def test_a_snap_inside_the_warnings_window_is_refused(self):
        YOLO = self.YOLO
        both = [("fx", "snap start"), ("xfx", "snap start")]
        for cap, want in (
            # p5-transcript-records before FLAKE-HINT4: `wait 4`, `snap start`.
            (YOLO + "env TZ=UTC\nwait 4\nsnap start\n", both),
            # LAUNCH-SETTLE r2 (gpt6 NTH): an `until gone` over the warning a
            # launch armed counts only once an `until shown` has seen it --
            # gone on a look that may precede the first frame's warning says
            # nothing, as `absent` there says nothing.
            (YOLO + "wait 4\nuntil gone Full access\nsnap start\n", both),
            (YOLO + "until shown Full access\nwait 4\nuntil gone Full access\nsnap start\n", []),
            (YOLO + "wait 2\nwait 1.5\nsnapfull start\n", [("fx", "snapfull start"), ("xfx", "snapfull start")]),
            # Before the window and after it.
            (YOLO + "wait 2.9\nsnap start\n", []),
            (YOLO + "wait 7\nsnap start\n", []),
            # Another row's `until` waits as long as it takes; so does `held`.
            (YOLO + "wait 4\nuntil gone Something else\nsnap start\n", both),
            (YOLO + "wait 2\nheld\nwait 0.5\nsnap start\n", both),
            (YOLO + "wait 7\nheld\nsnap start\n", []),
            # `until shown`: up now, and up no longer than the waits before it.
            (YOLO + "until shown Full access\nsnap start\n", []),
            (YOLO + "wait 1\nuntil shown Full access\nwait 4\nsnap start\n", both),
            (YOLO + "wait 3.5\nuntil shown Full access\nsnap start\n", both),
            # `wire` and `bells` sleep 0.3 s each.
            (YOLO + "wait 2.5\nwire w\nbells b\nsnap start\n", both),
            # A relaunch arms it again: an `until gone` that counted it gone
            # before does not carry over (LAUNCH-SETTLE-F, opus r2 SHOULD 2:
            # an unseen `until gone` counts nothing since round 2, so the
            # warning is seen first, and the relaunch is what makes this race).
            (YOLO + "until shown Full access\nuntil gone Full access\nrelaunch\nwait 4\nsnap start\n", both),
            (YOLO + "until shown Full access\nuntil gone Full access\nwait 4\nsnap start\n", []),
            # Each binary's own environment, and fx's other spelling.
            ("fx: env FX_PERMISSION_MODE=full-access\nwait 4\nsnap start\n", [("fx", "snap start")]),
            ("xfx: env XFX_PERMISSION_MODE=yolo\nfx: until gone Full access\nwait 4\nsnap start\n", [("xfx", "snap start")]),
            ("wait 4\nsnap start\n", []),
        ):
            with self.subTest(cap=cap):
                self.assertEqual(self.raced(cap), want)

    def test_a_line_that_takes_the_warnings_row_inside_its_window_is_refused(self):
        # interrupt-next-cmd before FLAKE-HINT4: a Ctrl-C 4.5 s after launch.
        cc = self.YOLO + "until shown Full access\nwait 2\ntype run cmd\nkey Enter\nwait 2.5\n%skey C-c\nwait 7\ntype next\n"
        self.assertEqual(self.raced(cc % ""), [("fx", "key C-c"), ("xfx", "key C-c")])
        self.assertEqual(self.raced(cc % "until gone Full access\n"), [])
        for line, want in (("type /model", True), ("key Escape", True), ("key -N 2 Escape", True),
                           ("type hello", False), ("key Enter", False), ("key C-o", True), ("key -H 1b 5b 49", False)):
            with self.subTest(line=line):
                got = self.raced(self.YOLO + "wait 4\n%s\nwait 9\nsnap a\n" % line)
                self.assertEqual(got, [("fx", line), ("xfx", line)] if want else [])
        self.assertEqual(self.raced(self.YOLO + "wait 7\ntype /model\n"), [])

    def test_a_line_that_takes_the_warnings_row_before_its_window_stops_its_clock(self):
        # A menu opened while the warning is up, and closed: how long it held
        # the row is not in the .cap, so neither is when the warning goes.
        menu = self.YOLO + "wait 1\ntype /model\nkey Enter\nwait 0.5\nkey Escape\nwait %s\n%ssnap a\n"
        both = [("fx", "snap a"), ("xfx", "snap a")]
        for wait, until, want in (("12", "", both), ("30", "", both),
                                  # Gone after a pause is hidden, not spent: only an
                                  # `until shown` first makes the going count.
                                  ("12", "until gone Full access\n", both),
                                  ("12", "until shown Full access\nuntil gone Full access\n", []),
                                  ("0.1", "until shown Full access\n", [])):
            with self.subTest(wait=wait, until=until):
                self.assertEqual(self.raced(menu % (wait, until)), want)
        # The same keys once the warning has gone stop nothing.
        self.assertEqual(self.raced(self.YOLO + "wait 8\ntype /model\nkey Enter\nwait 0.5\nkey Escape\nwait 1\nsnap a\n"), [])

    def test_a_ctrl_c_arms_the_exit_hint_and_any_other_keystroke_takes_it_down(self):
        both = [("fx", "snap a"), ("xfx", "snap a")]
        gone = "until gone press ctrl+c again to exit\n"
        for cap, want in (
            # interrupt's cc-3.5 before FLAKE-HINT4.
            ("wait 2\nkey C-c\nwait 0.3\nsnap cc\nwait 3.2\nsnap a\n", both),
            ("wait 2\nkey C-c\nwait 0.3\nsnap cc\nwait 3.2\n" + gone + "snap a\n", []),
            ("key C-c\nuntil shown press ctrl+c again to exit\nwait 0.3\nsnap a\n", []),
            ("key C-c\nwait 1\ntype x\nwait 2\nsnap a\n", []),
            ("key C-c\nwait 1\nkey Escape\nwait 2\nsnap a\n", []),
            # A paste and a terminal report are not keystrokes: both binaries
            # keep the hint over them (xfx shell.rs `consume`; fx over a paste).
            ("key C-c\nwait 1\npastex 3\nwait 2\nsnap a\n", both),
            ("key C-c\nwait 1\npaste x\nwait 2\nsnap a\n", both),
            ("key C-c\nwait 1\nkey -H 1b 5b 3f 39 39 37 3b 32 6e\nwait 2\nsnap a\n", both),
            ("key C-c\nwait 2\nkey C-c\nwait 1\nsnap a\n", []),
            ("keep\nkey C-c\nrelaunch\nwait 3\nsnap a\n", []),
            # A Shift-Tab is not counted; the `until shown` that means it is.
            ("key BTab\nwait 4.5\nsnap a\n", []),
            ("key BTab\nuntil shown Full access\nwait 4.5\nsnap a\n", both),
            ("key BTab\nuntil shown Full access\nwait 0.5\nsnap a\nwait 4.5\nuntil gone Full access\nsnap b\n", []),
        ):
            with self.subTest(cap=cap):
                self.assertEqual(self.raced(cap), want)

    def test_a_profile_in_full_access_arms_it_too(self):
        yolo = {"s.json": '{"permission_mode": "yolo"}\n'}
        for line in ("seed s.json", "homefile .xfx/settings.json s.json"):
            with self.subTest(line=line):
                self.assertEqual(self.raced(line + "\nwait 4\nsnap start\n", yolo),
                                 [("fx", "snap start"), ("xfx", "snap start")])
        self.assertEqual(self.raced("seed s.json\nwait 4\nsnap start\n", {"s.json": '{"permission_mode":"ask"}\n'}), [])
        # A profile written between an exit and a relaunch arms it at the relaunch.
        for line in ("reseed s.json", "rehomefile .xfx/settings.json s.json"):
            with self.subTest(line=line):
                self.assertEqual(self.raced("keep\nwait 9\n" + line + "\nrelaunch\nwait 4\nsnap again\n", yolo),
                                 [("fx", "snap again"), ("xfx", "snap again")])

    def test_an_until_or_absent_counts_only_with_the_timers_exact_text(self):
        # FLAKE-HINT4 r2 (gpt6 MUST 1): `until gone Full access never-rendered`
        # is true while the warning is up.
        both = [("fx", "snap start"), ("xfx", "snap start")]
        # Over the warning an `until shown` saw (neither `until gone` nor
        # `absent` speaks for the one a launch armed unseen:
        # test_absent_says_a_ctrl_c_armed_nothing).
        for line, want in (
            ("until gone Full access", []),
            ("until gone Full access never-rendered", both),
            ("until gone Full access enabled", both),
            ("until gone Full", both),
            ("absent Full access", []),
            ("absent Full access never-rendered", both),
        ):
            with self.subTest(line=line):
                self.assertEqual(self.raced(self.YOLO + "until shown Full access\nwait 3.5\n%s\nsnap start\n" % line), want)
        hint = "key C-c\nwait 1.5\n%s\nwait 1\nsnap a\n"
        for line, want in (("absent press ctrl+c again to exit", []), ("absent press ctrl+c", True),
                           ("until gone press ctrl+c again to exit, please", True)):
            with self.subTest(line=line):
                self.assertEqual(self.raced(hint % line), [("fx", "snap a"), ("xfx", "snap a")] if want else [])
        # `until shown` with a longer text sees the row, but says no more of it
        # than any other row's `until`: it waits as long as it takes.
        self.assertEqual(self.raced(self.YOLO + "until shown Full access enabled\nsnap start\n"), both)

    def test_absent_says_a_ctrl_c_armed_nothing(self):
        # approvals' after-cc-cmd-ask (gpt6 MUST 2): a Ctrl-C a question
        # answers arms no hint. The snap stays at 2.5 s; `absent` halfway
        # fails the run if the hint is up, and the check takes its word.
        both = [("fx", "snap after"), ("xfx", "snap after")]
        self.assertEqual(self.raced("key C-c\nwait 2.5\nsnap after\n"), both)
        self.assertEqual(self.raced("key C-c\nwait 1.25\nabsent press ctrl+c again to exit\nwait 1.25\nsnap after\n"), [])
        # Too soon to say anything: the hint may not be drawn yet.
        self.assertEqual(self.raced("key C-c\nabsent press ctrl+c again to exit\nwait 2.5\nsnap after\n"), both)
        self.assertEqual(self.raced("key C-c\nwait 0.5\nabsent press ctrl+c again to exit\nwait 2\nsnap after\n"), both)
        snap_a = [("fx", "snap a"), ("xfx", "snap a")]
        # The warning a launch armed (opus r2 SHOULD 1): the check knows it is
        # armed, so an `absent` that passes there looked where it was not yet
        # drawn, however long after the launch; only the screen showing it
        # (`until shown`) and then not (`until gone`, `absent`) counts it gone.
        self.assertEqual(self.raced(self.YOLO + "wait 2\nabsent Full access\nwait 2\nsnap a\n"), snap_a)
        self.assertEqual(self.raced(self.YOLO + "wait 3\nabsent Full access\nwait 1\nsnap a\n"), snap_a)
        self.assertEqual(self.raced(self.YOLO + "wait 3.5\nabsent Full access\nsnap a\n"), snap_a)
        self.assertEqual(self.raced(self.YOLO + "until shown Full access\nwait 0.5\nabsent Full access\nwait 3.5\nsnap a\n"), [])
        # A relaunch arms it again, unseen.
        self.assertEqual(self.raced("keep\n" + self.YOLO + "until shown Full access\nwait 1\nrelaunch\nwait 3\nabsent Full access\nwait 1\nsnap a\n"),
                         snap_a)
        # `until gone` is held to the same rule (gpt6 NTH): over a launch's
        # warning unseen it counts nothing, nor right after the key that arms
        # the hint; from the settle on, or once seen, it does.
        self.assertEqual(self.raced(self.YOLO + "wait 3.5\nuntil gone Full access\nsnap a\n"), snap_a)
        self.assertEqual(self.raced("key C-c\nuntil gone press ctrl+c again to exit\nwait 1\nsnap after\n"), both)
        self.assertEqual(self.raced("key C-c\nwait 1.5\nuntil gone press ctrl+c again to exit\nwait 1\nsnap after\n"), [])
        self.assertEqual(self.raced("key C-c\nuntil shown press ctrl+c again to exit\nuntil gone press ctrl+c again to exit\nwait 1\nsnap after\n"), [])
        # A row an `until shown` saw is drawn: `absent` needs no settle after it.
        self.assertEqual(self.raced("key C-c\nuntil shown press ctrl+c again to exit\nabsent press ctrl+c again to exit\nwait 2.5\nsnap after\n"), [])
        # Not while the row is paused: a hidden warning is not a spent one.
        menu = self.YOLO + "wait 1\ntype /model\nkey Escape\nabsent Full access\nwait 9\nsnap a\n"
        self.assertEqual(self.raced(menu), snap_a)
        menu = self.YOLO + "until shown Full access\nwait 1\ntype /model\nkey Escape\nabsent Full access\nwait 9\nsnap a\n"
        self.assertEqual(self.raced(menu), snap_a)

    def test_settle_is_an_allowance_for_a_key_only(self):
        # A row a key arms may not be drawn yet just after the key: an
        # `absent` that soon says nothing. The allowance is measured, and well
        # inside the row's own time. A row only a launch arms has none: no
        # `absent` speaks for it (above).
        for timer in capfile.TIMERS:
            with self.subTest(timer=timer.name):
                if timer.keys:
                    self.assertIsInstance(timer.settle, float)
                    self.assertLess(timer.settle, timer.seconds)
                else:
                    self.assertIsNone(timer.settle)

    def test_an_until_shown_nothing_counted_armed_reaches_back_to_the_last_input(self):
        # opus SHOULD: a Shift-Tab is not counted, but it is input, and the
        # warning it armed has been up no longer than the time since it.
        both = [("fx", "snap a"), ("xfx", "snap a")]
        self.assertEqual(self.raced("key BTab\nwait 3.5\nuntil shown Full access\nsnap a\n"), both)
        self.assertEqual(self.raced("key BTab\nuntil shown Full access\nwait 0.5\nsnap a\n"), [])
        self.assertEqual(self.raced("key BTab\nwait 1\nheld\nuntil shown Full access\nsnap a\n"), both)

    def test_an_until_on_another_row_and_a_signal_take_as_long_as_they_take(self):
        # Each is the only reason these race: two seconds of waits alone do not.
        both = [("fx", "snap a"), ("xfx", "snap a")]
        self.assertEqual(self.raced(self.YOLO + "wait 1\nwait 1\nsnap a\n"), [])
        for line in ("until shown Something else", "until gone Something else", "signal SIGCONT", "held"):
            with self.subTest(line=line):
                self.assertEqual(self.raced(self.YOLO + "wait 1\n%s\nwait 1\nsnap a\n" % line), both)

    def test_nodraw_is_the_harness_tests_own(self):
        # LAUNCH-SETTLE r2 (opus SHOULD): `nodraw` skips the launch wait, so a
        # scenario's first key could race the binary's start again. Only
        # test_capture.py's stub binaries may use it; the check names any
        # scenario that does, on the side it is for.
        d = scratch()
        with open(os.path.join(d, "t.cap"), "w") as f:
            f.write("xfx: nodraw\nwait 1\nsnap a\n")
        (unit,) = capfile.units(d)
        self.assertEqual(capfile.harness_only(unit.reading("fx")), [])
        (line,) = capfile.harness_only(unit.reading("xfx"))
        self.assertIn("t (xfx): `nodraw`", line)
        here = [line for u in capfile.units(os.path.join(HERE, "scenarios")) for side in ("fx", "xfx") for line in capfile.harness_only(u.reading(side))]
        self.assertEqual(here, [], "\n".join(here))

    def test_each_timer_cites_both_binaries_sources(self):
        for timer in capfile.TIMERS:
            path, line = timer.xfx.rsplit(":", 1)
            with open(os.path.join(REPO, path), encoding="utf-8") as f:
                self.assertIn("%g" % (timer.seconds * 1000), f.read().split("\n")[int(line) - 1], timer.xfx)
            self.assertRegex(timer.fx, r"^src/\S+\.zig:\d+@481104fd$")


HELP = """<banner>

┃
────
Commands 3  [All]  General  Model

  /help      show available slash commands
  /quit      exit the interactive shell
  /model     choose a model
────
↑↓ navigate     enter open     esc close
"""
SLASH = """<banner>

┃ /
────
Commands 3 · type to filter                1–3

  /help    show available slash commands     General
  /quit    exit the interactive shell        General
  /model   choose a model                    Model
────
↑↓ navigate     enter use     esc close
"""

FAKE_CAPTURE = r"""#!/usr/bin/env bash
# A capture.sh that answers by scenario name, for test_sweep.py.
set -euo pipefail
which=$1 scenario=$2 size=$3 out=$4
mkdir -p "$out"
case $scenario in
  inventory)
    cp "$FAKE_INV/help.txt" "$out/fx-inventory-$size-help-00.txt"
    cp "$FAKE_INV/slash.txt" "$out/fx-inventory-$size-slash-00.txt" ;;
  bad) echo "a run that fails" >&2; exit 1 ;;
  slow)
    sleep 60 &
    echo $! >> "$FAKE_INV/slow.pids"
    wait ;;
  ok|pair-*)
    printf 'same %s\nrow\n' "$TMPDIR" > "$out/$which-$scenario-$size-a.ansi"
    printf '%s says\n' "$which" > "$out/$which-$scenario-$size-b.ansi" ;;
esac
"""


class McpPort(unittest.TestCase):
    """sweep.run_rules: the port capture.sh wrote for a run's own HTTP MCP
    server, and only that port, is the same on both sides."""

    def setUp(self):
        import sweep

        self.run_rules = sweep.run_rules
        self.out = scratch()
        cap = os.path.join(self.out, "p5-mcp-x.cap")
        with open(cap, "w") as f:
            f.write("snap show\n")
        self.unit = capfile.Unit("p5-mcp-x", cap, cap)
        self.led = ledgermod.load(LEDGER)

    def port(self, side, port):
        runs = os.path.join(self.out, "runs", "p5-mcp-x", side)
        os.makedirs(runs, exist_ok=True)
        with open(os.path.join(runs, "%s-p5-mcp-x-40x120-mcphttp.port" % side), "w") as f:
            f.write("%s\n" % port)

    def diff(self, fx_text, xfx_text):
        rules = {side: ledgermod.rules_for(self.led, side) for side in ("fx", "xfx")}
        a = cells.normalize(cells.parse(fx_text), self.run_rules(self.out, self.unit, "fx", "40x120", rules["fx"]))
        b = cells.normalize(cells.parse(xfx_text), self.run_rules(self.out, self.unit, "xfx", "40x120", rules["xfx"]))
        return cells.differing(a, b)

    def test_each_runs_own_port_is_the_same(self):
        self.port("fx", 51234)
        self.port("xfx", 60001)
        self.assertEqual(self.diff("  url http://127.0.0.1:51234/mcp\n", "  url http://127.0.0.1:60001/mcp\n"), [])

    def test_a_fixed_port_another_runs_port_and_the_digits_elsewhere_are_compared(self):
        self.port("fx", 51234)
        self.port("xfx", 60001)
        for fx_text, xfx_text in (
            # A fixed port in a profile, different on the two sides.
            ("url http://127.0.0.1:45678/mcp\n", "url http://127.0.0.1:45679/mcp\n"),
            # The other side's port is not this side's.
            ("url http://127.0.0.1:60001/mcp\n", "url http://127.0.0.1:51234/mcp\n"),
            # The run's digits, but longer, on another host, or not a port.
            ("url http://127.0.0.1:512345/mcp\n", "url http://127.0.0.1:600011/mcp\n"),
            ("url http://127.0.0.2:51234/mcp\n", "url http://127.0.0.2:60001/mcp\n"),
            ("pid 51234\n", "pid 60001\n"),
        ):
            self.assertEqual(len(self.diff(fx_text, xfx_text)), 1, (fx_text, xfx_text))

    def test_with_no_port_written_nothing_is_rewritten(self):
        self.assertEqual(len(self.diff("url http://127.0.0.1:51234/mcp\n", "url http://127.0.0.1:60001/mcp\n")), 1)
        rules = ledgermod.rules_for(self.led, "fx")
        self.assertIs(self.run_rules(self.out, self.unit, "fx", "40x120", rules), rules)

    def test_the_ledger_has_no_port_rule_of_its_own(self):
        self.assertNotIn("mcphttp-port", [n["id"] for n in self.led["normalizations"]])


class Sweep(unittest.TestCase):
    """sweep.sh end to end, over a fake capture script and fake binaries."""

    def setUp(self):
        self.d = scratch()
        self.addCleanup(shutil.rmtree, self.d, True)
        self.inv = os.path.join(self.d, "inv")
        os.makedirs(self.inv)
        for name, text in (("help.txt", HELP), ("slash.txt", SLASH)):
            with open(os.path.join(self.inv, name), "w") as f:
                f.write(text)
        self.capture = os.path.join(self.d, "capture.sh")
        with open(self.capture, "w") as f:
            f.write(FAKE_CAPTURE)
        self.fx = os.path.join(self.d, "fx")
        self.xfx = os.path.join(self.d, "xfx")
        for path, body in ((self.fx, "#!/bin/sh\nexit 0\n"), (self.xfx, "#!/bin/sh\nexit 1\n")):
            with open(path, "w") as f:
                f.write(body)
            os.chmod(path, 0o755)
        self.sha = hashlib.sha256(slurp(self.fx, "rb")).hexdigest()
        self.scen = os.path.join(self.d, "scenarios")
        os.makedirs(self.scen)
        for name, text in {
            "ok.cap": "type /model\nkey Escape\nsnap a\nsnap b\n",
            "bad.cap": "snap a\nsnap b\nsnapfull c\n",
            "slow.cap": "wait 1\nsnap a\n",
        }.items():
            with open(os.path.join(self.scen, name), "w") as f:
                f.write(text)
        # The fake pin and nothing else: `b` (each binary names itself) stays
        # a defect.
        self.ledger = ledger_file('[[pin]]\nname = "fake"\nversion = "9"\nsha256 = "%s"\n' % self.sha)

    def sweep(self, *args, out=None):
        env = dict(os.environ, FXCAP_SWEEP_CAPTURE=self.capture, FAKE_INV=self.inv, TMPDIR=self.d)
        out = out or os.path.join(self.d, "out")
        cmd = ["bash", SWEEP, "--xfx", self.xfx, "--scenarios", self.scen, "--ledger", self.ledger, "--out", out]
        cmd += ["--timeout-slack", "1", "--timeout-scale", "0"] + list(args)
        return subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=120), out

    def board(self, out):
        with open(os.path.join(out, "scoreboard.json")) as f:
            return json.load(f)

    def test_a_sha_mismatch_runs_nothing(self):
        for extra in ((), ("--inventory-only",)):
            r, out = self.sweep("--fx", self.fx, "--pin-sha", "0" * 64, *extra)
            self.assertEqual(r.returncode, 2, r.stderr)
            self.assertIn("refusing to run", r.stderr)
            self.assertFalse(os.path.exists(out), "nothing was captured")

    def test_failed_and_timed_out_scenarios_count_in_the_denominator(self):
        started = time.time()
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, "--jobs", "3")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        b = self.board(out)
        t = b["totals"]
        # ok: a (the same) + b (each binary's own words); bad: a, b, c.full;
        # slow: a, past its one-second deadline.
        self.assertEqual(t["total"], 6)
        self.assertEqual((t["ran"], t["failed"], t["timeout"]), (2, 3, 1))
        self.assertEqual((t["identical"], t["unresolved-defect"]), (1, 1))
        self.assertAlmostEqual(b["pass"], 1 / 6)
        self.assertEqual(b["scenario_status"], {"ran": 1, "failed": 1, "timeout": 1})
        status = {s["scenario"]: s["status"] for s in b["scenarios"]}
        self.assertEqual(status, {"bad": "failed", "ok": "ran", "slow": "timeout"})
        self.assertLess(time.time() - started, 60, "the slow run was stopped, not waited for")
        # Stopped by PID, children too.
        for pid in slurp(os.path.join(self.inv, "slow.pids")).split():
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid), 0)
        summary = slurp(os.path.join(out, "summary.md"))
        self.assertIn("**pass = 1 / 6 = 16.7%**", summary)
        self.assertIn("`ok` 1/2", summary)
        # Coverage is its own line: three commands and six key hints; /model
        # and the popup's Escape are covered.
        cov = b["coverage"]
        self.assertEqual((cov["surfaces"], cov["covered"]), (9, 2))
        self.assertIn("/help", cov["uncovered"])
        # The capture wrote its TMPDIR into a snap; no committed file says it.
        for name in ("summary.md", "scoreboard.md", "scoreboard.json"):
            text = slurp(os.path.join(out, name))
            for leak in ("/Users/", "/private/", self.d):
                self.assertNotIn(leak, text, name)
        self.assertNotIn(self.d, r.stdout)
        # A second scoring of the same captures, with the ledger as it is and
        # the scenarios as they were: a .cap edited since changes nothing.
        with open(os.path.join(self.scen, "ok.cap"), "a") as f:
            f.write("snap later\n")
        r2, _ = self.sweep("--rescore", out=out)
        self.assertEqual(r2.returncode, 0, r2.stderr)
        self.assertEqual(self.board(out)["totals"], t)

    def test_a_cause_past_its_date_attributes_no_row_of_a_sweep(self):
        # `ok`'s snap `b` differs (each binary names itself). A cause covering
        # both rows, past its date, attributes neither: they come back as rows
        # no cause covers (sweep.py's `attribute` is handed `live_causes`).
        # The same cause with no date takes them, on a rescore of the same
        # captures.
        cause = textwrap.dedent(
            """
            [[cause]]
            id = "says"
            kind = "unit"
            owner = "U-1"
            scenarios = ["ok"]
            snaps = ["b"]
            fx_row = '^fx says$'
            xfx_row = '^xfx says$'
            why = "w"
            %s
            """
        )
        pin = '[[pin]]\nname = "fake"\nversion = "9"\nsha256 = "%s"\n' % self.sha

        def pair_b(out):
            with open(os.path.join(out, "pairs.json")) as f:
                return next(p for p in json.load(f) if p["scenario"] == "ok" and p["label"] == "b")

        self.ledger = ledger_file(pin, cause % 'expires = "date:2000-01-01"')
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, "--only", "ok")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        b = pair_b(out)
        self.assertEqual(b["verdict"], "unresolved-defect", b)
        self.assertEqual(b["causes"], [], b)
        self.assertEqual(sorted(text for _, _, text in b["unattributed"]), ["fx says", "xfx says"], b)
        self.ledger = ledger_file(pin, cause % "")
        r, _ = self.sweep("--rescore", out=out)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        b = pair_b(out)
        self.assertEqual((b["causes"], b["unattributed"]), (["says"], []), b)

    def test_the_inventory_alone(self):
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, "--inventory-only")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(os.path.exists(os.path.join(out, "runs")), "no scenario ran")
        cov = json.loads(slurp(os.path.join(out, "coverage.json")))
        self.assertEqual((cov["surfaces"], cov["covered"], cov["uncovered"][:2]), (9, 2, ["/help", "/quit"]))
        self.assertIn("**Uncovered: 7**", slurp(os.path.join(out, "summary.md")))

    def test_a_partial_sweep_says_so_and_counts_what_it_selected(self):
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, "--only", "o*")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.board(out)["totals"]["total"], 2)
        self.assertIn("**PARTIAL**: `--only o*`", slurp(os.path.join(out, "summary.md")))

    def test_a_snap_that_races_a_timer_runs_nothing(self):
        with open(os.path.join(self.scen, "racy.cap"), "w") as f:
            f.write("xfx: env XFX_PERMISSION_MODE=yolo\nwait 4\nsnap start\n")
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, "--only", "o*")
        self.assertEqual(r.returncode, 0, "a scenario --only leaves out is not refused: " + r.stderr)
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, out=os.path.join(self.d, "out2"))
        self.assertEqual(r.returncode, 2, r.stderr)
        self.assertIn("a snap or keystroke inside a timer's window, timed by a second count", r.stderr)
        self.assertIn("racy (xfx): `snap start` is 4.0 to 4.0 s after the full-access warning is armed", r.stderr)
        self.assertNotIn("racy (fx)", r.stderr)
        self.assertFalse(os.path.exists(out), "nothing was captured")

    def test_a_scenario_with_nodraw_runs_nothing(self):
        with open(os.path.join(self.scen, "stub.cap"), "w") as f:
            f.write("nodraw\nwait 1\nsnap a\n")
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha)
        self.assertEqual(r.returncode, 2, r.stderr)
        self.assertIn("a verb only the harness's own tests use", r.stderr)
        self.assertIn("stub (fx): `nodraw`", r.stderr)
        self.assertFalse(os.path.exists(out), "nothing was captured")

    def test_the_out_directory_is_outside_the_repository(self):
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, out=os.path.join(REPO, "target", "sweep-out"))
        self.assertEqual(r.returncode, 2)
        self.assertIn("outside the repository", r.stderr)
        self.assertFalse(os.path.exists(out))


class GitEnvironment(unittest.TestCase):
    def test_the_out_guard_finds_this_checkout_whatever_git_variables_it_inherits(self):
        """`--out` is refused inside this checkout and inside its main worktree,
        which git is asked for. GIT_DIR or GIT_COMMON_DIR inherited from the
        caller would have git answer for another repository, and the guard
        would let the sweep write into this one's main worktree."""
        import sweep

        want = sweep.main_worktree()
        if want is None and not os.path.exists(os.path.join(REPO, ".git")):
            self.skipTest("not run from a git checkout: there is no main worktree to find")
        self.assertIsNotNone(want)
        decoy = os.path.join(scratch(), "decoy")
        subprocess.run(
            ["git", "init", "-q", decoy],
            check=True,
            capture_output=True,
            env=dict(sweep.git_env(), GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1"),
        )
        for extra in ({"GIT_DIR": os.path.join(decoy, ".git")}, {"GIT_COMMON_DIR": os.path.join(decoy, ".git")}):
            saved = {k: os.environ.get(k) for k in extra}
            os.environ.update(extra)
            try:
                got = sweep.main_worktree()
            finally:
                for k, v in saved.items():
                    if v is None:
                        os.environ.pop(k, None)
                    else:
                        os.environ[k] = v
            self.assertEqual(got, want, sorted(extra))


class Inventory(unittest.TestCase):
    def write(self, **snaps):
        d = scratch()
        for name, text in snaps.items():
            with open(os.path.join(d, "fx-inventory-40x120-%s.txt" % name.replace("_", "-")), "w") as f:
                f.write(text)
        return d

    def test_commands_categories_and_key_hints(self):
        inv = inventory.read(self.write(help_00=HELP, slash_00=SLASH), "40x120")
        self.assertEqual([c["id"] for c in inv["commands"]], ["/help", "/quit", "/model"])
        self.assertEqual(inv["commands"][2], {"id": "/model", "kind": "command", "description": "choose a model", "category": "Model"})
        self.assertEqual([k["id"] for k in inv["keys"]][:3], ["help: ↑↓ navigate", "help: enter open", "help: esc close"])

    def test_a_short_read_fails(self):
        short = SLASH.replace("  /model   choose a model                    Model\n", "")
        with self.assertRaises(inventory.InventoryError):
            inventory.read(self.write(help_00=HELP, slash_00=short), "40x120")
        with self.assertRaises(inventory.InventoryError):
            inventory.read(self.write(help_00=HELP.replace("Commands 3", "Commands 4"), slash_00=SLASH), "40x120")


if __name__ == "__main__":
    unittest.main(verbosity=1)
