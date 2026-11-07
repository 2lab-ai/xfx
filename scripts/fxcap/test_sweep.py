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


def compare(fx_text, xfx_text, led=None):
    a, b = cells.parse(fx_text), cells.parse(xfx_text)
    if led is not None:
        a = cells.normalize(a, ledgermod.rules_for(led, "fx"))
        b = cells.normalize(b, ledgermod.rules_for(led, "xfx"))
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
        xfx = "\x1b[38;5;240mx\x1b[1;38;5;255m𝒇x\x1b[0m\x1b[38;5;245m v0.1.0-1a94cdf [debug] · Run /help for commands\x1b[0m\nrest"
        self.assertEqual(compare(fx, xfx, self.led), [])
        self.assertEqual(len(compare(fx, xfx)), 1, "and they differ before")

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
        self.red = redact.Redactor({"/Users/someone/repo": "<repo>", "/private/tmp/claude-501/out": "<out>"})

    def test_known_roots_and_every_other_local_path(self):
        text = (
            "repo /Users/someone/repo/scripts/fxcap; out /private/tmp/claude-501/out/runs/x; "
            "home /Users/someone/.xfx/settings.json; scratch /private/tmp/claude-501/fxcap.Kun3pm/home/.fx; "
            "temp /var/folders/ab/T/x; tmp /tmp/fxcap.a/s; volume /Volumes/Disk/fx"
        )
        got = self.red.public(text)
        for leak in ("/Users/", "/private/", "/var/", "/tmp/", "/Volumes/", "someone"):
            self.assertNotIn(leak, got)
        self.assertIn("<repo>/scripts/fxcap", got)
        self.assertIn("<out>/runs/x", got)
        self.assertEqual(got.count("<local-path>"), 5, got)

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

    def test_the_out_directory_is_outside_the_repository(self):
        r, out = self.sweep("--fx", self.fx, "--pin-sha", self.sha, out=os.path.join(REPO, "target", "sweep-out"))
        self.assertEqual(r.returncode, 2)
        self.assertIn("outside the repository", r.stderr)
        self.assertFalse(os.path.exists(out))


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
