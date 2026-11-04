"""Tests for the parity sweep in CI (ci_sweep.py): the selection, the base
comparison, the re-runs of a known-varying pair, and the report, end to end
over a fake capture script and a scratch git repository. Runs no binary and
no tmux.

usage: scripts/fxcap/test-sweep.sh   (runs this file after test_sweep.py)
"""

import hashlib
import json
import os
import subprocess
import sys
import textwrap
import unittest

HERE = os.path.dirname(os.path.realpath(__file__))
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)

import ci_sweep  # noqa: E402
import test_sweep  # noqa: E402

CI_SWEEP = os.path.join(HERE, "ci-sweep.sh")

# A capture.sh that answers by scenario name and by which xfx build it is
# handed (the fake binaries are named `head` and `base`), and counts its runs
# per scenario and side, so a pair can vary from one run to the next.
FAKE_CAPTURE = r"""#!/usr/bin/env bash
set -euo pipefail
which=$1 scenario=$2 size=$3 out=$4
mkdir -p "$out"
if [[ $scenario == inventory ]]; then
  cp "$FAKE_INV/help.txt" "$out/fx-inventory-$size-help-00.txt"
  cp "$FAKE_INV/slash.txt" "$out/fx-inventory-$size-slash-00.txt"
  exit 0
fi
side=$which
[[ $which == fx ]] || side=$(basename "$XFX_BIN")
count=$FAKE_STATE/$scenario.$side
n=$(( $(cat "$count" 2>/dev/null || echo 0) + 1 ))
echo "$n" > "$count"
snap() { printf '%s\nfooter\n' "$2" > "$out/$which-$scenario-$size-$1.ansi"; }
case $scenario:$side in
  regress:head) snap a "B" ;;
  regress:*) snap a "A" ;;
  flaky:head) snap a "tick $n" ;;
  flaky:*) snap a "tick 2" ;;
  always:head) snap a "tick 9" ;;
  always:*) snap a "tick 0" ;;
  broken:fx|newscen:fx) snap a "X" ;;
  broken:*|newscen:*) snap a "Y" ;;
  *) snap a "row"; snap b "row" ;;
esac
"""

LEDGER = textwrap.dedent(
    """
    [[pin]]
    name = "fake"
    version = "9"
    sha256 = "%s"

    [[cause]]
    id = "tick"
    kind = "nondeterminism"
    owner = "scenario"
    scenarios = ["*"]
    snaps = ["*"]
    fx_row = '^tick \\d+$'
    xfx_row = '^tick \\d+$'
    fix = "wait on content"
    why = "the tick is the run's clock"
    """
)


class CiSweep(unittest.TestCase):
    def setUp(self):
        self.d = test_sweep.scratch()
        self.repo = os.path.join(self.d, "repo")
        self.scen = os.path.join(self.repo, "scen")
        self.state = os.path.join(self.d, "state")
        self.inv = os.path.join(self.d, "inv")
        for d in (self.scen, self.state, self.inv):
            os.makedirs(d)
        for name, text in (("help.txt", test_sweep.HELP), ("slash.txt", test_sweep.SLASH)):
            self.write(os.path.join(self.inv, name), text)
        self.capture = os.path.join(self.d, "capture.sh")
        self.write(self.capture, FAKE_CAPTURE)
        self.bins = {}
        for name, body in (("fx", "#!/bin/sh\nexit 0\n"), ("head", "#!/bin/sh\n"), ("base", "#!/bin/sh\n")):
            path = os.path.join(self.d, "bin", name)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            self.write(path, body)
            os.chmod(path, 0o755)
            self.bins[name] = path
        with open(self.bins["fx"], "rb") as f:
            self.sha = hashlib.sha256(f.read()).hexdigest()
        self.ledger = os.path.join(self.d, "ledger.toml")
        self.write(self.ledger, LEDGER % self.sha)
        self.core = os.path.join(self.d, "core.txt")
        self.write(self.core, "# the core set\nsame   # one\nregress\nflaky\nalways\nbroken\n")
        self.git("init", "-q", "-b", "main")
        caps = {
            "same": "snap a\nsnap b\n",
            "regress": "snap a\n",
            "flaky": "snap a\n",
            "always": "snap a\n",
            "broken": "snap a\n",
            "uses": "gateway data.json\nsnap a\n",
            "recscen": "snap a\n",
            "unchanged": "snap a\n",
            "gone": "snap a\n",
        }
        for name, text in caps.items():
            self.write(os.path.join(self.scen, name + ".cap"), text)
        self.write(os.path.join(self.scen, "data.json"), "[]\n")
        self.commit("base")
        self.base = self.git("rev-parse", "HEAD").strip()

    def write(self, path, text):
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)

    def git(self, *args):
        return subprocess.run(
            ["git", "-C", self.repo, "-c", "user.name=t", "-c", "user.email=t@example.invalid"] + list(args),
            check=True,
            capture_output=True,
            text=True,
        ).stdout

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", message)

    def change(self):
        """The head: a new scenario, an edited turns file, a receipt that
        names one scenario by a capture's file name and another only in prose,
        a removed scenario and a removed snap."""
        self.write(os.path.join(self.scen, "newscen.cap"), "snap a\n")
        self.write(os.path.join(self.scen, "data.json"), "[{}]\n")
        receipts = os.path.join(self.repo, ".prd", "x", "receipts", "u")
        os.makedirs(receipts)
        self.write(os.path.join(receipts, "README.md"), "GREEN: `fx-recscen-40x120-a.ansi`; the unchanged rows.\n")
        os.remove(os.path.join(self.scen, "gone.cap"))
        self.write(os.path.join(self.scen, "same.cap"), "snap a\n")
        self.commit("head")

    def run_ci(self, *extra, out=None):
        out = out or os.path.join(self.d, "out")
        env = dict(
            os.environ,
            FXCAP_SWEEP_CAPTURE=self.capture,
            FAKE_INV=self.inv,
            FAKE_STATE=self.state,
            TMPDIR=self.d,
        )
        env.pop("GITHUB_STEP_SUMMARY", None)
        cmd = ["bash", CI_SWEEP, "--scenarios", self.scen, "--ledger", self.ledger, "--core", self.core]
        cmd += ["--fx", self.bins["fx"], "--pin", "fake", "--xfx", self.bins["head"], "--out", out, "--tmp", self.d]
        cmd += list(extra)
        r = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
        return r, out

    def result(self, out):
        with open(os.path.join(out, "result.json")) as f:
            return json.load(f)

    def test_a_regression_fails_and_everything_else_is_reported(self):
        self.change()
        r, out = self.run_ci("--base", self.base, "--base-xfx", self.bins["base"])
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        res = self.result(out)
        self.assertEqual(
            res["selection"],
            {
                "same": ["core", "changed"],
                "regress": ["core"],
                "flaky": ["core"],
                "always": ["core"],
                "broken": ["core"],
                "newscen": ["changed"],
                "uses": ["uses data.json"],
                "recscen": ["receipt"],
            },
        )
        self.assertEqual(res["removed"], ["gone"])
        self.assertEqual(res["removed_pairs"], ["same/b"])
        pairs = {(p["scenario"], p["label"]): p for p in res["pairs"]}
        # The base passes it and the head does not: a regression, run once (no
        # cause says it varies).
        self.assertTrue(pairs[("regress", "a")]["regression"])
        self.assertEqual(pairs[("regress", "a")]["runs"], 1)
        # Known to vary: passes on its second run.
        self.assertEqual((pairs[("flaky", "a")]["head"], pairs[("flaky", "a")]["runs"]), ("identical", 2))
        self.assertFalse(pairs[("flaky", "a")]["regression"])
        # Known to vary and never passing: four runs, then it counts.
        self.assertEqual(pairs[("always", "a")]["runs"], 4)
        self.assertTrue(pairs[("always", "a")]["regression"])
        # Neither build passes it: reported, not failed.
        self.assertEqual(pairs[("broken", "a")]["base"], "unresolved-defect")
        self.assertFalse(pairs[("broken", "a")]["regression"])
        # New: no base run, never failed.
        self.assertTrue(pairs[("newscen", "a")]["new"])
        self.assertIsNone(pairs[("newscen", "a")]["base"])
        self.assertFalse(pairs[("newscen", "a")]["regression"])
        self.assertEqual(res["regressions"], 2)
        # The base ran only what still failed at the head and the base snaps.
        with open(os.path.join(out, "base", "run.json")) as f:
            self.assertEqual(sorted(json.load(f)["units"]), ["always", "broken", "regress"])
        with open(os.path.join(out, "head-retry-1", "run.json")) as f:
            self.assertEqual(sorted(json.load(f)["units"]), ["always", "flaky"])
        self.assertFalse(os.path.exists(os.path.join(out, "head-retry-4")))
        report = test_sweep.slurp(os.path.join(out, "report.md"))
        self.assertIn("**2 regression(s)**", report)
        self.assertIn("| regress | a | identical | unresolved-defect | 1 | row 1: `A` / `B` |", report)
        for leak in (self.d, "/Users/", "/private/"):
            self.assertNotIn(leak, report)
            self.assertNotIn(leak, r.stdout)

    def test_no_failing_pair_runs_no_base(self):
        self.write(self.core, "same\nuses\n")
        r, out = self.run_ci("--base", self.base, "--base-xfx", self.bins["base"])
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse(os.path.exists(os.path.join(out, "base")))
        self.assertEqual(self.result(out)["regressions"], 0)
        self.assertEqual([f for f in os.listdir(self.state) if f.endswith(".base")], [], "no base build ran")

    def test_a_core_name_the_scenarios_do_not_hold_stops_the_run(self):
        self.write(self.core, "same\nmissing\n")
        r, out = self.run_ci("--base", self.base, "--base-xfx", self.bins["base"])
        self.assertEqual(r.returncode, 2)
        self.assertIn("names missing", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(out, "head")))

    def test_a_pin_the_ledger_does_not_name_stops_the_run(self):
        self.write(self.core, "same\n")
        out = os.path.join(self.d, "out")
        r, _ = self.run_ci("--base", self.base, "--base-xfx", self.bins["base"], out=out)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.write(self.ledger, LEDGER % ("0" * 64))
        r, _ = self.run_ci("--base", self.base, "--base-xfx", self.bins["base"], out=os.path.join(self.d, "out2"))
        self.assertEqual(r.returncode, 2)
        self.assertIn("refusing to run", r.stderr)

    def test_the_full_sweep_reports_and_does_not_fail(self):
        r, out = self.run_ci("--full")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        board = json.loads(test_sweep.slurp(os.path.join(out, "full", "scoreboard.json")))
        self.assertGreater(board["totals"]["unresolved-defect"], 0)
        self.assertEqual(board["meta"]["only"], [])

    def test_the_select_mode_runs_nothing(self):
        self.change()
        r = subprocess.run(
            ["bash", CI_SWEEP, "--select", "--base", self.base, "--scenarios", self.scen, "--core", self.core],
            capture_output=True,
            text=True,
        )
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("uses\tuses data.json\n", r.stdout)
        self.assertIn("gone\tremoved\n", r.stdout)
        self.assertEqual(os.listdir(self.state), [])


class ReceiptNames(unittest.TestCase):
    def test_only_a_receipts_own_spellings_name_a_scenario(self):
        text = (
            "ran scripts/fxcap/scenarios/foo-bar.cap and `baz.cap`; the pair\n"
            "xfx-qux-40x120-a.ansi / fx-p5-x-y-30x100-snap.txt is identical.\n"
            "copy alias help: words, not names. fx-v0.0.16 is a version.\n"
        )
        d = test_sweep.scratch()
        path = os.path.join(d, "README.md")
        with open(path, "w") as f:
            f.write(text)
        self.assertEqual(
            ci_sweep.names_in(".prd/a/receipts/b/fx-zed-40x120-a.ansi", path), {"foo-bar", "baz", "qux", "p5-x-y", "zed"}
        )


if __name__ == "__main__":
    unittest.main()
