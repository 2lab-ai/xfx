"""Tests for the parity sweep in CI (ci_sweep.py): the selection, the base
comparison, the re-runs of a known-varying pair, and the report, end to end
over a fake capture script and a scratch git repository. Runs no binary and
no tmux.

usage: scripts/fxcap/test-sweep.sh   (runs this file after test_sweep.py)
"""

import hashlib
import json
import os
import re
import signal
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
  basevary:fx) snap a "tick 2" ;;
  basevary:head) snap a "B" ;;
  basevary:*) snap a "tick $n" ;;
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
        self.assertIn("| regress | a | identical | unresolved-defect | 1 / 1 | row 1: `A` / `B` |", report)
        for leak in (self.d, "/Users/", "/private/"):
            self.assertNotIn(leak, report)
            self.assertNotIn(leak, r.stdout)

    def test_a_varying_base_gets_the_heads_retries_and_a_real_regression_still_fails(self):
        # The base varies (a `tick` row, which the ledger's nondeterminism
        # cause covers) and passes on its second run; the head prints `B`
        # every time. Run once, the base reads as failing too, and the pair
        # would be "neither passes": reported, not failed.
        self.write(self.core, "basevary\n")
        self.write(os.path.join(self.scen, "basevary.cap"), "snap a\n")
        self.commit("basevary")
        base = self.git("rev-parse", "HEAD").strip()
        r, out = self.run_ci("--base", base, "--base-xfx", self.bins["base"])
        res = self.result(out)
        pair = res["pairs"][0]
        self.assertEqual((pair["base"], pair.get("base_runs")), ("identical", 2), pair)
        self.assertTrue(pair["regression"], pair)
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertFalse(os.path.exists(os.path.join(out, "base-retry-2")), "no run past the first that passes")

    def test_a_nondeterminism_cause_past_its_date_buys_no_retry(self):
        # The case above with the `tick` cause expired: it covers nothing, so
        # the varying base is run once, fails, and the pair is one neither
        # build passes -- not a regression it would take a retry to see.
        self.write(self.ledger, LEDGER.replace('fix = "wait on content"', 'fix = "wait on content"\nexpires = "date:2000-01-01"') % self.sha)
        self.write(self.core, "basevary\n")
        self.write(os.path.join(self.scen, "basevary.cap"), "snap a\n")
        self.commit("basevary")
        base = self.git("rev-parse", "HEAD").strip()
        r, out = self.run_ci("--base", base, "--base-xfx", self.bins["base"])
        pair = self.result(out)["pairs"][0]
        self.assertEqual(pair.get("base_runs"), 1, pair)
        self.assertNotEqual(pair["base"], "identical", pair)
        self.assertFalse(pair["regression"], pair)
        self.assertFalse(os.path.exists(os.path.join(out, "base-retry-1")), "an expired cause bought a retry")

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

    def test_a_file_named_through_the_scenario_directory_selects_its_users(self):
        """capture.sh's `@SCENARIOS@/<path>`: in a typed line, in a seeded
        profile, in a workspace directory's file -- one level, and never a
        path outside scenarios/."""
        server = '{"mcpServers":{"s":{"command":"@BUN@","args":["@SCENARIOS@/server.mjs"]}}}\n'
        self.write(os.path.join(self.scen, "server.mjs"), "// one\n")
        self.write(os.path.join(self.scen, "prof.json"), server)
        os.makedirs(os.path.join(self.scen, "ws"))
        self.write(os.path.join(self.scen, "ws", ".mcp.json"), server)
        self.write(os.path.join(self.scen, "deeper.json"), '["@SCENARIOS@/prof.json"]\n')
        caps = {
            "viaprofile": "fx: homefile .fx/mcp.json prof.json\nsnap a\n",
            "viawork": "work ws\nsnap a\n",
            "viatype": "type /mcp add two -- @BUN@ @SCENARIOS@/server.mjs\nsnap a\n",
            "viadeeper": "homefile x.json deeper.json\nsnap a\n",
            "outside": "type /mcp ../scen/server.mjs /etc/hosts\nsnap a\n",
        }
        for name, text in caps.items():
            self.write(os.path.join(self.scen, name + ".cap"), text)
        self.commit("servers")
        r = self.select("HEAD", "server.mjs", "// two\n")
        for name in ("viaprofile", "viawork", "viatype"):
            self.assertIn("%s\tuses server.mjs\n" % name, r)
        # deeper.json names prof.json, not server.mjs: one level only.
        self.assertNotIn("viadeeper", r)
        self.assertNotIn("outside", r)
        # The profile itself, edited, selects what names it either way.
        r = self.select("HEAD", "prof.json", server + "\n")
        self.assertIn("viaprofile\tuses prof.json\n", r)
        self.assertIn("viadeeper\tuses prof.json\n", r)
        self.assertNotIn("viatype", r)

    def test_a_link_out_of_the_scenarios_is_never_read(self):
        """A link under a named directory that leaves scenarios/ is not read:
        here to a FIFO no one writes, whose open never returns (a link to
        /dev/zero is the same path in the code, but a read of it that does not
        end grows without bound -- 46 GB in three minutes, measured on the
        unfixed code), and to a file that names a scenario file."""
        outside = os.path.join(self.d, "outside.json")
        self.write(outside, '["@SCENARIOS@/server.mjs"]\n')
        fifo = os.path.join(self.d, "never")
        os.mkfifo(fifo)
        self.write(os.path.join(self.scen, "server.mjs"), "// one\n")
        os.makedirs(os.path.join(self.scen, "ws"))
        os.symlink(fifo, os.path.join(self.scen, "ws", "never"))
        os.symlink(outside, os.path.join(self.scen, "ws", "out.json"))
        self.write(os.path.join(self.scen, "ws", "in.json"), "[]\n")
        self.write(os.path.join(self.scen, "linked.cap"), "work ws\nsnap a\n")
        self.commit("links")
        r = self.select("HEAD", "server.mjs", "// two\n")
        self.assertNotIn("linked", r)
        r = self.select("HEAD", "ws/in.json", "[1]\n")
        self.assertIn("linked\tuses ws/in.json\n", r)

    def test_a_cap_that_is_no_regular_file_in_the_scenarios_stops_the_selection_at_once(self):
        """A `.cap` that is a link out of scenarios/ (here to a FIFO no one
        writes, whose open never returns) is not read: the selection refuses
        it, exit 2, within the bound, rather than waiting on it."""
        fifo = os.path.join(self.d, "never.cap")
        os.mkfifo(fifo)
        os.symlink(fifo, os.path.join(self.scen, "hang.cap"))
        self.commit("a linked cap")
        code, out, err = self.select_run(self.base)
        self.assertEqual(code, 2, out + err)
        self.assertIn("hang.cap", err)
        self.assertIn("not a regular file", err)

    def select_run(self, base):
        """--select against `base`, in its own process group, killed whole
        after 60 s (a hung read is a failure, never a hang)."""
        p = subprocess.Popen(
            ["bash", CI_SWEEP, "--select", "--base", base, "--scenarios", self.scen, "--core", self.core],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        try:
            out, err = p.communicate(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.communicate()
            self.fail("the selection did not finish in 60 s")
        return p.returncode, out, err

    def select(self, base, name, text):
        """The --select output once `name` under scenarios/ is `text`,
        committed, against `base`."""
        base = self.git("rev-parse", base).strip()
        self.write(os.path.join(self.scen, name), text)
        self.commit("edit " + name)
        code, out, err = self.select_run(base)
        self.assertEqual(code, 0, err)
        return out


EXPRESSION = re.compile(r"\$\{\{(.*?)\}\}", re.S)
WORKFLOWS = [os.path.join(test_sweep.REPO, ".github", "workflows", n) for n in ("sweep.yml", "sweep-full.yml")]


def load_workflow(path):
    """A workflow as GitHub reads it (YAML, through ruby's Psych: Python has
    no YAML parser of its own, and check-preview-contract.sh already needs
    ruby)."""
    r = subprocess.run(
        ["ruby", "-ryaml", "-rjson", "-e", "puts JSON.dump(YAML.load_file(ARGV[0]))", path],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(r.stdout)


def step(workflow, job, name):
    for st in workflow["jobs"][job]["steps"]:
        if st.get("name") == name:
            return st
    raise AssertionError("%s has no step %r" % (job, name))


class Workflows(unittest.TestCase):
    """What the sweep's workflows may and may not do: no secret, no write
    token, no trigger that runs a fork's change with this repository's
    authority, no expression spliced into a shell."""

    def test_the_invariants(self):
        for path in WORKFLOWS:
            name = os.path.basename(path)
            w = load_workflow(path)
            text = test_sweep.slurp(path)
            self.assertEqual(w["permissions"], {"contents": "read"}, name)
            for job, spec in w["jobs"].items():
                self.assertIn(spec.get("permissions", {"contents": "read"}), ({"contents": "read"},), (name, job))
                for st in spec["steps"]:
                    if st.get("uses", "").startswith("actions/checkout@"):
                        self.assertIs(st.get("with", {}).get("persist-credentials"), False, (name, job, st.get("name")))
                    self.assertNotIn("${{", st.get("run", ""), (name, job, st.get("name")))
            for trigger in ("pull_request_target", "workflow_run"):
                self.assertNotIn(trigger, w["on"], name)
            self.assertNotIn("secrets.", text, name)
            self.assertNotIn("github.token", text, name)
            # An expression reaches a secret or the token by index or as a
            # whole too (`secrets['X']`, `toJSON(secrets)`, `github['token']`):
            # neither word may appear in one.
            for expr in EXPRESSION.findall(text):
                self.assertIsNone(re.search(r"\b(?:secrets|token)\b", expr, re.I), (name, expr))

    def test_the_capture_tests_run_once_tmux_bun_and_curl_are_there(self):
        # test-capture.sh runs tmux and bun: after the step that fetches bun
        # onto the PATH and the one that installs tmux, before the sweep.
        names = [st.get("name") for st in load_workflow(WORKFLOWS[0])["jobs"]["sweep"]["steps"]]
        here = names.index("The capture harness's own tests")
        self.assertLess(names.index("Fetch fx v0.0.16 and bun, each checked by sha256"), here)
        self.assertLess(names.index("Install tmux"), here)
        self.assertLess(here, names.index("Sweep the changed scenarios and the core set"))
        self.assertEqual(step(load_workflow(WORKFLOWS[0]), "sweep", "The capture harness's own tests")["run"].strip(), "./scripts/fxcap/test-capture.sh")

    def test_only_a_pull_requests_run_is_cancelled_by_the_next(self):
        # A push to main is the backstop for a pull request built on an old
        # base: two quick merges must not cancel the first one's run, and a
        # third must not replace the second's while it waits (a concurrency
        # group holds one pending run), so each push has a group of its own.
        w = load_workflow(WORKFLOWS[0])
        self.assertEqual(w["concurrency"]["cancel-in-progress"], "${{ github.event_name == 'pull_request' }}")
        self.assertEqual(
            w["concurrency"]["group"], "sweep-${{ github.event_name == 'pull_request' && github.ref || github.sha }}"
        )
        self.assertIs(load_workflow(WORKFLOWS[1])["concurrency"]["cancel-in-progress"], False)

    def test_a_pull_requests_base_and_a_pushs_before_are_two_variables(self):
        # One `pull_request && base.sha || before` expression gives a pull
        # request with no base.sha its synchronize event's `before`: the
        # previous head, which gates only the last push.
        env = load_workflow(WORKFLOWS[0])["jobs"]["sweep"]["env"]
        self.assertEqual(env.get("BASE_SHA"), "${{ github.event.pull_request.base.sha }}")
        self.assertEqual(env.get("PUSH_BEFORE"), "${{ github.event.before }}")
        self.assertNotIn("BASE_TIP", env)

    def repo(self):
        d = test_sweep.scratch()
        repo = os.path.join(d, "repo")
        os.makedirs(repo)

        def git(*args):
            return subprocess.run(
                ["git", "-C", repo, "-c", "user.name=t", "-c", "user.email=t@example.invalid"] + list(args),
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()

        git("init", "-q", "-b", "main")
        for n in ("one", "two"):
            with open(os.path.join(repo, n), "w") as f:
                f.write(n)
            git("add", "-A")
            git("commit", "-q", "-m", n)
        return d, repo, git

    def run_step(self, name, cwd, env):
        script = step(load_workflow(WORKFLOWS[0]), "sweep", name)["run"]
        return subprocess.run(["bash", "-e", "-c", script], cwd=cwd, env=env, capture_output=True, text=True, timeout=120)

    def test_a_pull_request_whose_base_is_not_in_the_clone_fails(self):
        d, repo, git = self.repo()
        head = git("rev-parse", "HEAD")
        out = os.path.join(d, "out")
        env = dict(os.environ, COMMIT_UNDER_TEST=head, GITHUB_OUTPUT=out)
        missing = "1" * 40
        parent = git("rev-parse", "HEAD^1")
        # (event, BASE_SHA, PUSH_BEFORE, the base it must write or None)
        for event, base_sha, before, expect in (
            ("pull_request", missing, "", None),
            ("pull_request", "", "", None),
            ("pull_request", "", parent, None),  # never the synchronize event's `before`
            ("push", "", missing, parent),
            ("push", "", "0" * 40, parent),
            ("push", head, "", parent),  # a push reads `before` only
            ("pull_request", parent, head, parent),  # a pull request, `base.sha` only
        ):
            tip = (base_sha, before)
            open(out, "w").close()
            step_env = dict(env, GITHUB_EVENT_NAME=event, BASE_SHA=base_sha, PUSH_BEFORE=before)
            r = self.run_step("Confirm the checkout and find the base", repo, step_env)
            written = test_sweep.slurp(out)
            if expect is None:
                self.assertNotEqual(r.returncode, 0, (event, tip, written))
                self.assertEqual(written, "", (event, tip))
            else:
                self.assertEqual(r.returncode, 0, (event, tip, r.stderr))
                self.assertEqual(written, "sha=%s\n" % expect, (event, tip))

    def test_the_base_is_built_from_inside_its_own_checkout(self):
        # cargo and rustup read `.cargo/config.toml` and `rust-toolchain.toml`
        # from the directory they run in: run from the head's checkout, the
        # head's would drive the base's build.
        d, repo, git = self.repo()
        temp = os.path.join(d, "runner-temp")
        fake = os.path.join(d, "fakebin")
        os.makedirs(temp)
        os.makedirs(fake)
        log = os.path.join(d, "cargo.log")
        with open(os.path.join(fake, "cargo"), "w") as f:
            f.write(
                textwrap.dedent(
                    """\
                    #!/bin/bash
                    set -eu
                    printf '%s|%s\\n' "$(pwd -P)" "$*" >> "$FAKE_LOG"
                    target=target
                    while [ $# -gt 0 ]; do
                      case $1 in --target-dir) target=$2; shift ;; --manifest-path) echo "manifest-path $2" >> "$FAKE_LOG"; shift ;; esac
                      shift
                    done
                    mkdir -p "$target/debug"
                    printf '#!/bin/sh\\necho xfx built in %s\\n' "$(pwd -P)" > "$target/debug/xfx"
                    chmod +x "$target/debug/xfx"
                    """
                )
            )
        os.chmod(os.path.join(fake, "cargo"), 0o755)
        env = dict(
            os.environ,
            PATH=fake + os.pathsep + os.environ["PATH"],
            RUNNER_TEMP=temp,
            GITHUB_WORKSPACE=repo,
            BASE=git("rev-parse", "HEAD^1"),
            FAKE_LOG=log,
        )
        r = self.run_step("Build xfx at the base and at the head", repo, env)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        calls = test_sweep.slurp(log).splitlines()
        self.assertNotIn("manifest-path", " ".join(calls))
        self.assertEqual([c.split("|")[0] for c in calls], [os.path.realpath(os.path.join(temp, "base")), os.path.realpath(repo)])
        self.assertIn("built in %s" % os.path.realpath(os.path.join(temp, "base")), r.stdout)


CI_FETCH = os.path.join(HERE, "ci-fetch.sh")

# A curl that answers from a directory: the file named like the URL's last
# path segment, written where `--output` says. Anything else it is asked fails.
STUB_CURL = r"""#!/bin/bash
set -eu
out='' url=''
while [ $# -gt 0 ]; do
  case $1 in --output) out=$2; shift ;; --retry|--retry-delay|--proto) shift ;; -*) ;; *) url=$1 ;; esac
  shift
done
echo "$url" >> "$FAKE_FILES/asked"
cp "$FAKE_FILES/${url##*/}" "$out"
"""


def sha_of(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


class CiFetch(unittest.TestCase):
    """ci-fetch.sh's refusals, through its own `fetch_tools`, with pins of the
    test's and a stub curl: no network."""

    def setUp(self):
        self.d = test_sweep.scratch()
        self.files = os.path.join(self.d, "files")
        self.stub = os.path.join(self.d, "stub")
        for d in (self.files, self.stub):
            os.makedirs(d)
        with open(os.path.join(self.stub, "curl"), "w") as f:
            f.write(STUB_CURL)
        os.chmod(os.path.join(self.stub, "curl"), 0o755)
        self.fx_tar = self.tarball("fx", "#!/bin/sh\necho fake fx\n")
        self.bun_zip = self.zipfile("#!/bin/sh\necho 1.3.14\n")
        self.fx_sha = sha_of(os.path.join(self.d, "fx-member"))

    def tarball(self, name, body, symlink=False, out="fx-macos-aarch64.tar.gz"):
        src = os.path.join(self.d, "tar-src-" + out)
        os.makedirs(src)
        member = os.path.join(src, "fx")
        if symlink:
            os.symlink("/bin/sh", member)
        else:
            with open(member, "w") as f:
                f.write(body)
            os.chmod(member, 0o755)
            with open(os.path.join(self.d, "fx-member"), "w") as f:
                f.write(body)
        path = os.path.join(self.files, out)
        subprocess.run(["tar", "-czf", path, "-C", src, "fx"], check=True)
        return path

    def zipfile(self, body, symlink=False, out="bun-darwin-aarch64.zip"):
        src = os.path.join(self.d, "zip-src-" + out)
        os.makedirs(os.path.join(src, "bun-darwin-aarch64"))
        member = os.path.join(src, "bun-darwin-aarch64", "bun")
        if symlink:
            os.symlink("/bin/sh", member)
        else:
            with open(member, "w") as f:
                f.write(body)
            os.chmod(member, 0o755)
        path = os.path.join(self.files, out)
        subprocess.run(["zip", "-q", "-y", "-r", path, "bun-darwin-aarch64"], cwd=src, check=True)
        return path

    def fetch(self, fx_tar=None, fx_tar_sha=None, fx_sha=None, bun_zip=None, bun_sha=None, dest=None):
        dest = dest or os.path.join(self.d, "tools")
        fx_tar = fx_tar or self.fx_tar
        bun_zip = bun_zip or self.bun_zip
        args = [
            dest,
            "https://example.invalid/r/" + os.path.basename(fx_tar),
            fx_tar_sha or sha_of(fx_tar),
            fx_sha or self.fx_sha,
            "https://example.invalid/r/" + os.path.basename(bun_zip),
            bun_sha or sha_of(bun_zip),
        ]
        script = 'source "$1"; shift; fetch_tools "$@"'
        env = dict(os.environ, PATH=self.stub + os.pathsep + os.environ["PATH"], FAKE_FILES=self.files)
        r = subprocess.run(["bash", "-c", script, "ci-fetch", CI_FETCH] + args, env=env, capture_output=True, text=True, timeout=60)
        return r, dest

    def refused(self, r, dest, message, code=1):
        self.assertEqual(r.returncode, code, r.stdout + r.stderr)
        self.assertIn(message, r.stderr)
        self.assertFalse(os.path.exists(os.path.join(dest, "bin", "fx")), "nothing installed")
        self.assertFalse(os.path.exists(os.path.join(dest, "bin", "bun")), "nothing installed")

    def test_the_pinned_files_are_installed_read_only(self):
        r, dest = self.fetch()
        self.assertEqual(r.returncode, 0, r.stderr)
        for name in ("fx", "bun"):
            path = os.path.join(dest, "bin", name)
            self.assertEqual(os.stat(path).st_mode & 0o777, 0o555, name)
        self.assertEqual(sha_of(os.path.join(dest, "bin", "fx")), self.fx_sha)
        self.assertFalse(os.path.exists(os.path.join(dest, "dl")))

    def test_a_tarball_off_its_pin_is_refused_before_it_is_read(self):
        r, dest = self.fetch(fx_tar_sha="0" * 64)
        self.refused(r, dest, "the fx v0.0.16 tarball has sha256")
        self.assertEqual(os.listdir(os.path.join(dest, "fx")), [], "tar never ran")

    def test_an_executable_off_its_pin_is_refused(self):
        r, dest = self.fetch(fx_sha="0" * 64)
        self.refused(r, dest, "the fx v0.0.16 executable has sha256")

    def test_a_symlinked_member_is_refused(self):
        fx = self.tarball("fx", "", symlink=True, out="fx-link.tar.gz")
        r, dest = self.fetch(fx_tar=fx)
        self.refused(r, dest, "the tarball holds no regular file fx")
        bun = self.zipfile("", symlink=True, out="bun-link.zip")
        r, dest = self.fetch(bun_zip=bun, dest=os.path.join(self.d, "tools2"))
        self.refused(r, dest, "the archive holds no regular file bun")

    def test_an_existing_destination_is_refused(self):
        dest = os.path.join(self.d, "tools")
        os.makedirs(dest)
        r, _ = self.fetch(dest=dest)
        self.refused(r, dest, "exists; it must be made here, empty", code=2)
        self.assertFalse(os.path.exists(os.path.join(self.files, "asked")), "nothing was fetched")

    def test_a_bun_zip_off_its_pin_is_refused(self):
        r, dest = self.fetch(bun_sha="0" * 64)
        self.refused(r, dest, "the bun 1.3.14 archive has sha256")

    def test_the_scripts_fx_pin_is_the_ledgers(self):
        text = test_sweep.slurp(CI_FETCH)
        led = test_sweep.ledgermod.load(test_sweep.LEDGER, test_sweep.REPO)
        pin = next(p for p in led["pins"] if p["name"] == "v0.0.16")
        self.assertIn("\nFX_SHA256=%s\n" % pin["sha256"], text)
        self.assertIn(pin["note"].split("whose sha256 is ")[1][:64], text)


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
