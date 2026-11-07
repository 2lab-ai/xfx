"""Tests for the capture harness's own verbs: capture.sh run end to end over a
stub binary, and its three servers -- fakegw.ts, mcphttp.ts and the p5-mcp
stdio fixture -- asked directly.

usage: scripts/fxcap/test-capture.sh [unittest arguments]

Runs no fx and no xfx. The binary capture.sh launches is a link to /bin/bash
(a shell, so a `type`d line is a command and the screen is its answer) or to
/bin/cat (a process that does nothing, for `signal`: interactive bash ignores
SIGTERM and SIGTSTP). Every tmux server is the run's own socket; every server
listens on 127.0.0.1 and every request goes there. Needs tmux, bun and curl on
PATH, and fails -- never skips -- without them.
"""

import atexit
import glob
import http.client
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.realpath(__file__))
# No __pycache__ beside the scripts: the checkout holds only what is committed.
sys.dont_write_bytecode = True

CAPTURE = os.path.join(HERE, "capture.sh")
FAKEGW = os.path.join(HERE, "fakegw.ts")
MCPHTTP = os.path.join(HERE, "mcphttp.ts")
STDIO = os.path.join(HERE, "scenarios", "p5-mcp-stdio.ts")

for _tool in ("tmux", "bun", "curl"):
    if not shutil.which(_tool):
        sys.exit("test-capture: %s is not on PATH; these tests need tmux, bun and curl and never skip" % _tool)
BUN = shutil.which("bun")

_SCRATCH = []
atexit.register(lambda: [shutil.rmtree(d, True) for d in _SCRATCH])


def scratch():
    # A tmux socket path has a length limit (104 bytes on macOS): the run's
    # socket is <here>/<case>/fxcap.XXXXXX/s, so a long TMPDIR gives way to /tmp.
    base = os.environ.get("TMPDIR") or "/tmp"
    if len(base) > 50:
        base = "/tmp"
    d = tempfile.mkdtemp(prefix="fxcapt.", dir=base)
    _SCRATCH.append(d)
    return d


def write(path, text, mode="w"):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, mode) as f:
        f.write(text)


def slurp(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


class Run:
    def __init__(self, r, out, scen, which, size):
        self.code, self.stdout, self.stderr = r.returncode, r.stdout, r.stderr
        self.out, self.scen, self.which, self.size = out, scen, which, size

    def file(self, label, ext=".txt"):
        return slurp(os.path.join(self.out, "%s-t-%s-%s%s" % (self.which, self.size, label, ext)))

    def screen(self, label):
        """A snap's text, its rows joined: bash wraps a long answer, and a
        test reads the answer, not where the terminal broke it."""
        return self.file(label).replace("\n", "")


def capture(cap, files=None, which="fx", binary="/bin/bash", size="30x200", scen_name="scenarios", timeout=120):
    """capture.sh on a scenario `t` made of `cap` and `files` (relative path ->
    text, bytes, or `("link", target)` for a symbolic link), the binary a
    link to `binary` (or, for a binary that is not a link, that file
    itself)."""
    d = scratch()
    scen = os.path.join(d, scen_name)
    os.makedirs(scen)
    write(os.path.join(scen, "t.cap"), cap)
    for rel, text in (files or {}).items():
        path = os.path.join(scen, rel)
        if isinstance(text, tuple):
            os.makedirs(os.path.dirname(path), exist_ok=True)
            os.symlink(text[1], path)
        else:
            write(path, text, "wb" if isinstance(text, bytes) else "w")
    if os.path.islink(binary) or binary.startswith("/bin/"):
        link = os.path.join(d, "bin", "it")
        os.makedirs(os.path.dirname(link))
        os.symlink(binary, link)
    else:
        link = binary
    out = os.path.join(d, "out")
    env = dict(os.environ, FXCAP_SCENARIOS=scen, TMPDIR=d)
    for k in ("TMUX", "TMUX_PANE", "FX_BIN", "XFX_BIN"):
        env.pop(k, None)
    env["FX_BIN" if which == "fx" else "XFX_BIN"] = link
    # Bounded, whatever the harness under test does: its own process group,
    # killed whole at the deadline (bash, fakegw, mcphttp), and then each tmux
    # server it started, through the run's own socket (tmux leaves the group).
    p = subprocess.Popen(["bash", CAPTURE, which, "t", size, out], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         text=True, start_new_session=True)
    try:
        stdout, stderr = p.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        p.communicate()
        for sock in glob.glob(os.path.join(d, "fxcap.*", "s")):
            subprocess.run(["tmux", "-f", "/dev/null", "-S", sock, "kill-server"], capture_output=True, timeout=10)
        raise AssertionError("capture.sh did not finish in %d s" % timeout)
    return Run(subprocess.CompletedProcess(p.args, p.returncode, stdout, stderr), out, scen, which, size)


class Refusals(unittest.TestCase):
    """Every verb's arguments, and a placeholder the run cannot write in, are
    refused with exit 2 before the binary is launched."""

    def refused(self, cap, needle, files=None, scen_name="scenarios"):
        r = capture(cap, files, scen_name=scen_name)
        self.assertEqual(r.code, 2, r.stdout + r.stderr)
        self.assertIn(needle, r.stderr)
        self.assertNotIn("guards:", r.stdout, "the binary was never launched")
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        return r

    def test_each_verbs_arguments(self):
        f = {"c.json": "{}\n"}
        for cap, needle in (
            ("resize 0x80\nsnap a\n", "resize takes <rows>x<cols>, not 0x80"),
            ("resize 24\nsnap a\n", "resize takes <rows>x<cols>, not 24"),
            ("resize\nsnap a\n", "resize takes an argument"),
            ("signal SIGKILL\nsnap a\n", "signal takes SIGTSTP, SIGCONT, SIGTERM or SIGHUP, not SIGKILL"),
            ("signal\nsnap a\n", "signal takes an argument"),
            ("rehomefile ../x c.json\nsnap a\n", "rehomefile takes a relative path inside HOME, not ../x"),
            ("rehomefile .fx/a/../../../x c.json\nsnap a\n", "rehomefile takes a relative path inside HOME"),
            ("rehomefile /etc/x c.json\nsnap a\n", "rehomefile takes a relative path inside HOME, not /etc/x"),
            ("rehomefile .fx/s.json missing.json\nsnap a\n", "rehomefile takes <path> <file in"),
            ("models2 c.json\nsnap a\n", "models2 takes <catalog.json> <ms>, not c.json"),
            ("models2 missing.json 10\nsnap a\n", "models2 takes <catalog.json> <ms>"),
            ("models2 c.json soon\nsnap a\n", "models2 takes <catalog.json> <ms>, not c.json soon"),
            ("modelsdelay soon\nsnap a\n", "modelsdelay takes milliseconds, not soon"),
            ("clock yesterday\nsnap a\n", "clock takes milliseconds since the epoch, not yesterday"),
            ("usage missing.json\nsnap a\n", "usage takes a file in"),
            ("mcphttp sometimes\nsnap a\n", "mcphttp takes ready, methodnotfound, hang, fail or rpcerror, not sometimes"),
            ("jobctl now\nsnap a\n", "jobctl takes no argument, not now"),
            ("homefile ../x c.json\nsnap a\n", "homefile takes a relative path inside HOME, not ../x"),
        ):
            with self.subTest(cap=cap.split("\n")[0]):
                self.refused(cap, needle, f)

    def test_mcpport_without_an_mcp_server(self):
        self.refused("homefile .fx/mcp.json m.json\nsnap a\n", "home/.fx/mcp.json names @MCPPORT@, and the scenario starts no mcphttp server",
                     {"m.json": '{"u":"http://127.0.0.1:@MCPPORT@/mcp"}\n'})
        self.refused("env U=@MCPPORT@\nsnap a\n", "an env value names @MCPPORT@")
        self.refused("type curl 127.0.0.1:@MCPPORT@\nsnap a\n", "a type, paste or relaunch line names @MCPPORT@")
        self.refused("work ws\nsnap a\n", "work/.mcp.json names @MCPPORT@", {"ws/.mcp.json": "@MCPPORT@\n"})

    def test_a_scenario_directory_no_placeholder_can_name(self):
        # A path with a space breaks a shell word, a `&` sed's replacement, a
        # `"` the JSON: refused wherever the placeholder would go.
        for name in ("my scenarios", "a&b", 'q"uote'):
            for cap, files in (
                ("type ls @SCENARIOS@\nsnap a\n", None),
                ("env S=@SCENARIOS@/x\nsnap a\n", None),
                ("paste @SCENARIOS@\nsnap a\n", None),
                ("homefile .fx/mcp.json m.json\nsnap a\n", {"m.json": '{"a":"@SCENARIOS@/s.ts"}\n'}),
                ("work ws\nsnap a\n", {"ws/.mcp.json": '{"a":"@SCENARIOS@/s.ts"}\n'}),
            ):
                with self.subTest(name=name, cap=cap.split("\n")[0]):
                    self.refused(cap, "@SCENARIOS@ would be written as '", files, scen_name=name)

    def test_such_a_directory_still_runs_a_scenario_that_names_no_placeholder(self):
        r = capture("wait 1\nsnap a\n", scen_name="my scenarios")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("guards:", r.stdout)


class Placeholders(unittest.TestCase):
    """`@SCENARIOS@` and `@BUN@` beside `@PORT@`, `@HOME@` and `@MCPPORT@`:
    in a seeded file, a `work` and a `dir` copy, an `env` value, and the text
    of a `type`, `paste` and `relaunch` line."""

    def test_every_place_a_placeholder_is_written(self):
        all5 = "s=@SCENARIOS@ b=@BUN@ h=@HOME@ p=@PORT@ m=@MCPPORT@"
        r = capture(
            "mcphttp ready\n"
            "homefile .fx/p.json p.json\n"
            "work ws\n"
            "dir side sidefrom\n"
            "env V=@SCENARIOS@/x W=@BUN@\n"
            "keep\n"
            "wait 1\n"
            "type printf 'env:%s:%s\\n' \"$V\" \"$W\"\n"
            "key Enter\n"
            "type printf 'typed:%s:%s:%s\\n' @SCENARIOS@ @BUN@ @MCPPORT@\n"
            "key Enter\n"
            "paste echo pasted-$((1+1)):@SCENARIOS@\n"
            "key Enter\n"
            "type cat a.txt ../side/x; echo\n"
            "key Enter\n"
            "wait 1\n"
            "snap a\n"
            "home h\n"
            "workls w\n"
            "type exit\n"
            "key Enter\n"
            "wait 0.5\n"
            "relaunch -c 'printf \"relaunched:%s\\n\" @SCENARIOS@; sleep 5'\n"
            "wait 1\n"
            "snap b\n",
            {
                "p.json": '{"%s"}\n' % all5,
                "ws/a.txt": all5 + "\n",
                "ws/raw.bin": b"\x00@BUN@\n",
                "sidefrom/x": "@HOME@\n",
            },
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        scen = os.path.realpath(r.scen)
        # The run's own directory, as mktemp named it under TMPDIR (not
        # resolved: /tmp and /var are links on macOS); `@SCENARIOS@` is resolved.
        run = re.escape(os.path.dirname(r.scen)) + r"/fxcap\.\w+"
        a = r.screen("a")
        self.assertIn("env:%s/x:%s" % (scen, BUN), a)
        self.assertRegex(a, r"typed:%s:%s:\d+" % (re.escape(scen), re.escape(BUN)))
        self.assertIn("pasted-2:%s" % scen, a)
        self.assertRegex(a, r"s=%s b=%s h=%s/home p=\d+ m=\d+" % (re.escape(scen), re.escape(BUN), run))
        self.assertRegex(a, r"\d+ m=\d+%s/home" % run, "the `dir` copy's file, after the work file's")
        # The `home` dump names the checkout and bun by placeholder, not by
        # this machine's paths.
        home = r.file("h", ".files")
        self.assertRegex(home, r'\{"s=<scenarios> b=<bun> h=<scratch>/home p=\d+ m=\d+"\}')
        self.assertNotIn(scen, home)
        self.assertNotIn(BUN, home)
        work = r.file("work-w")
        self.assertIn("== ./raw.bin\n\x00@BUN@", work, "a file with a NUL is left as it is")
        self.assertIn("relaunched:%s" % scen, r.screen("b"))

    def test_a_file_seeded_owner_only_stays_so(self):
        r = capture(
            "homefile .fx/deep/p.json p.json\n"
            "wait 1\n"
            "type stat -f 'mode:%Lp' ~/.fx ~/.fx/deep ~/.fx/deep/p.json | tr '\\n' ' '; echo\n"
            "key Enter\nwait 1\nsnap a\n",
            {"p.json": "@SCENARIOS@\n"},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("mode:700 mode:700 mode:600", r.screen("a"))


class Confinement(unittest.TestCase):
    """The placeholder rewrite of the files a run seeds and copies writes only
    regular files inside the run: never a name a newline split out of a
    longer one, never through a link, and a path the scenarios live under
    never reaches sed's program unless the scenario names it."""

    def sentinel(self, text="@PORT@\n"):
        path = os.path.join(scratch(), "outside", "sentinel")
        write(path, text)
        return path

    def test_a_name_with_a_newline_never_reaches_outside_the_run(self):
        # `ws/p` and `ws/p<newline>/<the sentinel's path>`: listed one name per
        # line, the second splits into `<run>/work/p` and the sentinel.
        outside = self.sentinel()
        inner = "p\n" + outside
        r = capture(
            "work ws\nwait 0.5\ntype cat $'%s'\nkey Enter\nwait 0.5\nsnap a\n" % inner.replace("\n", "\\n"),
            {"ws/p": "x\n", "ws/" + inner: "@PORT@\n"},
        )
        self.assertEqual(slurp(outside), "@PORT@\n", "the file outside the run is untouched")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertTrue(any(re.fullmatch(r"\d+", row) for row in r.file("a").split("\n")), "the copy inside the run is written")

    # A trailing newline is a name's own last character: `$(realpath x<NL>)`
    # loses it and names the sibling `x`, which the rewrite must never reach.
    TYPE_BOTH = "type for f in x $'x\\n'; do printf '[%s]' \"$(cat \"$f\")\"; done; echo\nkey Enter\nwait 0.5\nsnap a\n"

    def test_a_trailing_newline_never_turns_a_name_into_its_linked_sibling(self):
        outside = self.sentinel()
        r = capture("work ws\nwait 0.5\n" + self.TYPE_BOTH, {"ws/x": ("link", outside), "ws/x\n": "@PORT@\n"})
        self.assertEqual(slurp(outside), "@PORT@\n", "the file outside the run is untouched")
        self.assertTrue(os.path.isfile(outside) and not os.path.islink(outside))
        # The name with the newline is the one rewritten: the link still reads
        # the sentinel's placeholder, the file the port.
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.screen("a"), r"\[@PORT@\]\[\d+\]")

    def test_a_trailing_newline_never_turns_a_name_into_its_sibling(self):
        r = capture("work ws\nwait 0.5\n" + self.TYPE_BOTH, {"ws/x": "plain\n", "ws/x\n": "@PORT@\n"})
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.screen("a"), r"\[plain\]\[\d+\]")

    def test_a_link_to_outside_is_never_written_through(self):
        outside = self.sentinel()
        tree = os.path.dirname(outside)
        r = capture(
            "work ws\ndir side sidefrom\ndir home homefrom\nwait 0.5\n"
            "type for f in l ld ../side/l ~/l; do [ -L \"$f\" ] && printf 'link:%s ' \"$f\"; done; echo\nkey Enter\nwait 0.5\nsnap a\n",
            {
                "ws/l": ("link", outside),
                "ws/ld": ("link", tree),
                "ws/a.txt": "@PORT@\n",
                "sidefrom/l": ("link", outside),
                "homefrom/l": ("link", outside),
            },
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertEqual(slurp(outside), "@PORT@\n", "the file outside the run is untouched")
        self.assertIn("link:l link:ld link:../side/l link:", r.screen("a"), "each link is still a link")

    def test_a_hash_in_the_scenario_path_breaks_nothing_that_does_not_name_it(self):
        r = capture("homefile .fx/p.json p.json\nwait 0.5\nhome h\n", {"p.json": '{"p":@PORT@}\n'}, scen_name="a#b")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.file("h", ".files"), r'\{"p":\d+\}')

    def test_a_sed_flag_in_the_scenario_path_writes_no_file(self):
        # Scenarios under `<d>/x#w <outside>/victim` make the expression
        # `s#@SCENARIOS@#<d>/x#w <outside>/victim#g`: a `w` flag, which opens
        # `<outside>/victim#g`, and empties it, when sed reads the program.
        outside = os.path.dirname(self.sentinel())
        victim = os.path.join(outside, "victim#g")
        write(victim, "keep\n")
        r = capture("homefile .fx/p.json p.json\nwait 0.5\nhome h\n", {"p.json": '{"p":@PORT@}\n'},
                    scen_name="x#w " + os.path.join(outside, "victim"))
        self.assertEqual(slurp(victim), "keep\n")
        # The scenario names only @PORT@: it runs, and @PORT@ is written in.
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.file("h", ".files"), r'\{"p":\d+\}')


class Verbs(unittest.TestCase):
    def test_resize(self):
        r = capture("wait 1\ntype stty size\nkey Enter\nwait 0.5\nresize 20x90\nwait 0.5\ntype stty size\nkey Enter\nwait 0.5\nsnap a\n", size="24x80")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("resize: the pane is now 20x90", r.stdout)
        # The answers, not the typed lines: one per size.
        lines = r.file("a").split("\n")
        self.assertIn("24 80", lines)
        self.assertIn("20 90", lines)

    # A dead pane's signal is named by number or, by tmux 3.7, by name.
    def test_signal_stop_continue_and_end_under_jobctl(self):
        r = capture("jobctl\nkeep\nwait 1\nsignal SIGTSTP\nsignal SIGCONT\ntype hello\nkey Enter\nwait 0.5\nsnap a\nsignal HUP\nwait 0.3\nsnap b\n",
                    binary="/bin/cat")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"signal SIGTSTP -> pid \d+: T")
        self.assertRegex(r.stdout, r"signal SIGCONT -> pid \d+: [^T]")
        self.assertRegex(r.stdout, r"signal SIGHUP -> pid \d+: \S*; pane dead, signal (?:1|hup)\n")
        # cat came back and echoed the line: the terminal's echo, then cat's.
        self.assertEqual(r.file("a").split("\n").count("hello"), 2)

    def test_keep_holds_a_pane_whose_binary_is_gone_at_once(self):
        # A binary that ends before the scenario reaches its `keep` -- xfx
        # refusing a HOME it cannot use, under a scenario's opening comment
        # lines, each a subshell -- still leaves its pane, and the server
        # with it.
        r = capture("# a comment\n" * 12 + "keep\nwait 0.5\nsnap a\n", binary="/bin/echo")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.file("a"), r"Pane is dead \(status 0, ")

    def test_a_stop_is_discarded_without_jobctl_and_a_term_ends_it(self):
        r = capture("keep\nwait 1\nsignal TSTP\nsignal SIGTERM\nwait 0.3\nsnap a\n", binary="/bin/cat")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        # The pane's session leader's group is orphaned: the stop never lands.
        self.assertRegex(r.stdout, r"signal SIGTSTP -> pid \d+: [^T]")
        self.assertRegex(r.stdout, r"signal SIGTERM -> pid \d+: \S*; pane dead, signal (?:15|term)\n")

    def test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run(self):
        d = scratch()
        script = os.path.join(d, "wrapper")
        write(script, "#!/bin/sh\nexec /bin/cat\n")
        os.chmod(script, 0o755)
        r = capture("wait 1\nsignal SIGTERM\nsnap a\n", binary=script)
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertIn("no pid runs %s: pane" % os.path.realpath(script), r.stderr)
        self.assertIn("executable /bin/cat", r.stderr)
        # A run that launched fx checks its guards on the way out, failed or not.
        self.assertRegex(r.stdout, r"guards: %s sha256 [0-9a-f]{12} unchanged" % re.escape(script))

    def test_rehomefile(self):
        r = capture(
            "homefile .fx/settings.json ok.json\nwait 0.5\nhome before\n"
            "rehomefile .fx/settings.json bad.json\nrehomefile .new/dir/f.json bad.json\n"
            "type stat -f 'mode:%Lp' ~/.new ~/.new/dir ~/.new/dir/f.json | tr '\\n' ' '; echo\nkey Enter\nwait 0.5\nsnap a\nhome after\n",
            {"ok.json": '{"ok":true}\n', "bad.json": '{"port":@PORT@,\n'},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn('{"ok":true}', r.file("before", ".files"))
        self.assertRegex(r.stdout, r"rehomefile ~/\.fx/settings\.json: mode 600, \d+ bytes")
        self.assertRegex(r.file("after", ".files"), r'=== ~/\.fx/settings\.json \(\d+ bytes\)\n\{"port":\d+,\n')
        self.assertIn("mode:700 mode:700 mode:600", r.screen("a"))

    def test_clock(self):
        # Each product's variable names the one file, its own name only: fx's
        # FX_E2E_USAGE_CLOCK_FILE, xfx's XFX_E2E_USAGE_CLOCK_FILE.
        ask = (
            "type printf 'clock:%s:%s:%s\\n' \"$FX_E2E_USAGE_CLOCK_FILE\" \"$XFX_E2E_USAGE_CLOCK_FILE\""
            " \"$(cat \"${FX_E2E_USAGE_CLOCK_FILE:-$XFX_E2E_USAGE_CLOCK_FILE}\" 2>/dev/null)\"\nkey Enter\nwait 0.5\n"
        )
        cap = "clock 1791819000000\nwait 1\n" + ask + "clock 1791905400000\n" + ask + "type stat -f 'mode:%Lp' ~/.fxcap-usage-clock\nkey Enter\nwait 0.5\nsnap a\n"
        for which, shape in (("fx", "clock:{f}::{t}"), ("xfx", "clock::{f}:{t}")):
            r = capture(cap, which=which)
            self.assertEqual(r.code, 0, r.stdout + r.stderr)
            a = r.screen("a")
            for t in ("1791819000000", "1791905400000"):
                self.assertRegex(a, re.escape(shape.format(f="@F@", t=t)).replace("@F@", r"\S+/home/\.fxcap-usage-clock"), which)
            self.assertIn("mode:600", a)
        # No `clock` line: neither product is given a clock file.
        for which in ("fx", "xfx"):
            r = capture("wait 1\n" + ask + "snap a\n", which=which)
            self.assertEqual(r.code, 0, r.stdout + r.stderr)
            self.assertIn("clock:::", r.screen("a"))

    def test_usage_is_in_place_at_launch_and_swapped_on_cue(self):
        get = "type curl -s -w ' status:%{http_code}\\n' \"$FX_GATEWAY_BASE_URL/v1/report?start_date=2026-10-12&end_date=2026-10-12\"\nkey Enter\nwait 0.7\n"
        r = capture(
            "usage u1.json\nwait 1\n" + get + "usage u2.json\n" + get + "usage off\n" + get + "snap a\ngwlog\n",
            {"u1.json": '{"/v1/report":{"rows":[{"n":1}]}}\n', "u2.json": '{"/v1/report":{"status":429,"body":"later"}}\n'},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        a = r.screen("a")
        self.assertIn('{"results":[{"n":1}]} status:200', a)
        self.assertIn("later status:429", a)
        self.assertIn("not found status:404", a)
        log = r.file("gateway", ".log")
        self.assertIn("usage /v1/report -> 200 rows=1", log)
        self.assertIn("usage /v1/report -> 429", log)
        self.assertRegex(log, r"request GET /v1/report\?start_date=2026-10-12&end_date=2026-10-12 at=\d+ms -> 404 at=\d+ms headers=-")

    def test_models2_and_modelsdelay(self):
        get = "type curl -s \"$FX_E2E_GATEWAY_MODELS_URL\"; echo\nkey Enter\nwait 0.8\n"
        r = capture(
            "models one.json\nmodels2 two.json 5000\nmodelsdelay 200\nwait 0.5\n" + get + "wait 4.5\n" + get + "snap a\ngwlog\n",
            {"one.json": '{"data":[{"id":"fake/one"}]}\n', "two.json": '{"data":[{"id":"fake/two"}]}\n'},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        a = r.screen("a")
        self.assertLess(a.index('{"data":[{"id":"fake/one"}]}'), a.index('{"data":[{"id":"fake/two"}]}'))
        log = r.file("gateway", ".log")
        self.assertEqual(log.count("-> second catalog"), 1, log)
        asked = [int(m) for m in re.findall(r"GET /coding-agent/v1/models bearer=no at=(\d+)ms", log)]
        answered = [int(m) for m in re.findall(r"GET /coding-agent/v1/models -> 200 at=(\d+)ms", log)]
        self.assertEqual(len(asked), 2, log)
        self.assertTrue(all(b - a >= 200 for a, b in zip(asked, answered)), log)

    def test_mcphttp_on_cue(self):
        rpc = ("type curl -s -m 1 -w ' status:%%{http_code}\\n' -H 'content-type: application/json' "
               "-d '{\"jsonrpc\":\"2.0\",\"id\":%d,\"method\":\"%s\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}' \"$U\"; echo ' rc:'$?\n"
               "key Enter\nwait 1.3\n")
        r = capture(
            "mcphttp hang\nenv U=http://127.0.0.1:@MCPPORT@/mcp\nwait 1\n" + rpc % (1, "initialize")
            + "mcphttp ready\n" + rpc % (2, "initialize") + rpc % (3, "server/discover")
            + "mcphttp methodnotfound\n" + rpc % (4, "server/discover") + "snap a\ngwlog\n",
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        a = r.screen("a")
        self.assertIn("status:000 rc:28", a, "a held request: curl's timeout")
        self.assertIn('"id":2,"result":{"protocolVersion":"2025-06-18"', a)
        self.assertIn(" status:404 rc:0", a)
        self.assertIn('{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"Method not found"}} status:200', a)
        log = r.file("mcphttp", ".log")
        self.assertRegex(log, r"POST /mcp rpc=initialize id=2 session=no version=- mode=ready -> 200")
        self.assertRegex(log, r"POST /mcp rpc=server/discover id=4 session=no version=- mode=methodnotfound -> 200")

    def test_sockets(self):
        r = capture("env P=@PORT@\nwait 1\ntype exec 3<>/dev/tcp/127.0.0.1/$P\nkey Enter\nwait 0.5\nsockets s\n")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("1 socket(s)", r.stdout)
        self.assertRegex(r.file("s", ".sockets"), r"TCP 127\.0\.0\.1:\d+->127\.0\.0\.1:\d+ \(ESTABLISHED\)")


class Server:
    """A server script under bun on a port the OS picks, its log captured."""

    def __init__(self, test, script, *args, env=None):
        self.log_path = os.path.join(scratch(), "log")
        self.log = open(self.log_path, "w")
        self.p = subprocess.Popen([BUN, script, "0", *args], stdout=self.log, stderr=subprocess.STDOUT, env=dict(os.environ, **(env or {})))
        test.addCleanup(self.stop)
        for _ in range(100):
            m = re.search(r"listening (\d+)", slurp(self.log_path))
            if m:
                self.port = int(m.group(1))
                return
            if self.p.poll() is not None:
                break
            time.sleep(0.1)
        raise AssertionError("never listened: " + slurp(self.log_path))

    def stop(self):
        if self.p.poll() is None:
            self.p.kill()
            self.p.wait()
        self.log.close()

    def lines(self):
        return slurp(self.log_path)

    def ask(self, method, path, body=None, headers=None, timeout=5):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        try:
            c.request(method, path, body=body, headers=headers or {})
            r = c.getresponse()
            return r.status, dict(r.getheaders()), r.read()
        finally:
            c.close()


class FakeGateway(unittest.TestCase):
    def gw(self, turns=(), **env):
        d = scratch()
        write(os.path.join(d, "turns.json"), json.dumps(list(turns)))
        os.makedirs(os.path.join(d, "bodies"))
        clean = {k: "" for k in ("FAKEGW_MODELS", "FAKEGW_MODELS_DOWN_MS", "FAKEGW_MODELS_DELAY_MS", "FAKEGW_MODELS2", "FAKEGW_MODELS2_MS", "FAKEGW_CREDITS", "FAKEGW_TITLE", "FAKEGW_USAGE")}
        clean.update(env)
        s = Server(self, FAKEGW, os.path.join(d, "turns.json"), os.path.join(d, "bodies"), env=clean)
        s.bodies = os.path.join(d, "bodies")
        return s

    def test_every_request_is_a_line_unknown_gets_too_and_never_a_header_value(self):
        g = self.gw()
        status, _, _ = g.ask("GET", "/latest.txt?ref=x", headers={"authorization": "Bearer secret-value", "x-userid": "u-value"})
        self.assertEqual(status, 404)
        status, _, _ = g.ask("GET", "/.well-known/openid-configuration")
        self.assertEqual(status, 404)
        time.sleep(0.2)
        log = g.lines()
        self.assertRegex(log, r"request GET /latest\.txt\?ref=x at=\d+ms -> 404 at=\d+ms headers=authorization,x-userid\n")
        self.assertRegex(log, r"request GET /\.well-known/openid-configuration at=\d+ms -> 404 at=\d+ms headers=-\n")
        self.assertNotIn("secret-value", log)
        self.assertNotIn("u-value", log)

    def completion(self, g):
        return g.ask("POST", "/v4/ai/language-model", body=json.dumps({"prompt": []}), headers={"content-type": "application/json"})

    def test_a_reset_turn_cuts_the_stream_and_the_port_listens_again(self):
        g = self.gw([{"reset": True, "chunks": ["par"]}, {"chunks": ["after"]}])
        with self.assertRaises((http.client.IncompleteRead, ConnectionError)) as cm:
            self.completion(g)
        if isinstance(cm.exception, http.client.IncompleteRead):
            self.assertIn(b'"delta":"par"', cm.exception.partial)
            self.assertNotIn(b'"type":"finish"', cm.exception.partial)
        time.sleep(0.3)
        status, _, body = self.completion(g)
        self.assertEqual(status, 200)
        self.assertIn(b'"delta":"after"', body)
        log = g.lines()
        self.assertIn("completion /v4/ai/language-model -> reset", log)
        self.assertIn("reset after 1 event(s)", log)
        self.assertRegex(log, r"accepting again on %d" % g.port)

    def test_a_refused_turn_cuts_then_refuses_then_listens(self):
        g = self.gw([{"refuse": 1200}, {"chunks": ["back"]}])
        with self.assertRaises((http.client.RemoteDisconnected, ConnectionError)):
            self.completion(g)
        time.sleep(0.2)
        with self.assertRaises(ConnectionRefusedError):
            socket.create_connection(("127.0.0.1", g.port), timeout=1).close()
        time.sleep(1.4)
        status, _, body = self.completion(g)
        self.assertEqual(status, 200)
        self.assertIn(b'"delta":"back"', body)
        log = g.lines()
        self.assertIn("completion /v4/ai/language-model -> refuse 1200ms", log)
        self.assertRegex(log, r"request POST /v4/ai/language-model at=\d+ms -> cut, refusing 1200ms")
        self.assertIn("refusing connections for 1200ms", log)

    def test_models2_swaps_the_catalog_with_the_times_logged(self):
        d = scratch()
        write(os.path.join(d, "one.json"), '{"data":[{"id":"fake/one"}]}')
        write(os.path.join(d, "two.json"), '{"data":[{"id":"fake/two"}]}')
        g = self.gw(FAKEGW_MODELS=os.path.join(d, "one.json"), FAKEGW_MODELS2=os.path.join(d, "two.json"), FAKEGW_MODELS2_MS="1500", FAKEGW_MODELS_DELAY_MS="0")
        self.assertIn(b"fake/one", g.ask("GET", "/coding-agent/v1/models")[2])
        time.sleep(1.6)
        self.assertIn(b"fake/two", g.ask("GET", "/coding-agent/v1/models")[2])
        log = g.lines()
        self.assertEqual(log.count("-> second catalog"), 1)
        self.assertEqual(len(re.findall(r"GET /coding-agent/v1/models bearer=no at=\d+ms", log)), 2)

    def test_the_usage_routes_their_shapes_and_knobs(self):
        d = scratch()
        usage = os.path.join(d, "usage.json")
        write(usage, json.dumps({
            "/v1/report": {"rows": [{"r": "default"}], "byStart": {"2026-10-01": [{"r": "month"}]}, "byRange": {"2026-10-12..2026-10-12": [{"r": "today"}]}},
            "/backend-api/wham/usage": {"status": 429},
            "/backend-api/wham/profiles/me": {"json": {"id": "p"}},
            "/backend-api/wham/usage/daily-token-usage-breakdown": {"json": {"days": []}, "delayMs": 400},
            "/billing": {"status": 503, "body": "down"},
        }))
        g = self.gw(FAKEGW_USAGE=usage)

        def get(path):
            status, _, body = g.ask("GET", path)
            return status, body.decode()

        self.assertEqual(get("/v1/report?start_date=2026-10-12&end_date=2026-10-12"), (200, '{"results":[{"r":"today"}]}'))
        self.assertEqual(get("/v1/report?start_date=2026-10-01&end_date=2026-10-12"), (200, '{"results":[{"r":"month"}]}'))
        self.assertEqual(get("/v1/report?start_date=2026-10-06&end_date=2026-10-12&api_key_id=self"), (200, '{"results":[{"r":"default"}]}'))
        self.assertEqual(get("/ai-gateway/usage-report"), (200, '{"users":[]}'))
        self.assertEqual(get("/backend-api/wham/usage"), (429, "fake status 429"))
        self.assertEqual(get("/backend-api/wham/profiles/me"), (200, '{"id":"p"}'))
        started = time.time()
        self.assertEqual(get("/backend-api/wham/usage/daily-token-usage-breakdown"), (200, '{"days":[]}'))
        self.assertGreaterEqual(time.time() - started, 0.4)
        self.assertEqual(get("/billing"), (503, "down"))
        self.assertEqual(get("/settings"), (404, "not found"))
        log = g.lines()
        self.assertIn("usage /v1/report -> 200 rows=1", log)
        self.assertIn("usage /backend-api/wham/usage/daily-token-usage-breakdown -> 200 held 400ms", log)
        self.assertRegex(log, r"request GET /v1/report\?start_date=2026-10-06&end_date=2026-10-12&api_key_id=self at=\d+ms -> 200")
        # Without the file every one of them is unknown.
        os.remove(usage)
        self.assertEqual(get("/v1/report?start_date=2026-10-12&end_date=2026-10-12"), (404, "not found"))
        self.assertEqual(get("/ai-gateway/usage-report"), (404, "not found"))

    def test_a_fake_key_and_team_reach_no_line_of_the_log_and_no_body(self):
        # A key in the harness's own style (capture.sh's AI_GATEWAY_API_KEY is
        # `fake-key-not-real`) and a team, on every path that reads or names a
        # bearer: the report answered, held and refused, the budget, the
        # catalog, the credits, a generation, the Codex catalog, an unknown
        # path, and a completion.
        key, team = "fake-key-h3-not-real", "team-h3-not-real"
        d = scratch()
        usage = os.path.join(d, "usage.json")
        write(usage, json.dumps({
            "/v1/report": {"byRange": {"2026-10-12..2026-10-12": [{"model": "m", "request_count": 1, "total_cost": 0.5}]}, "delayMs": 200},
            "/ai-gateway/usage-report": {"status": 401},
        }))
        g = self.gw([{"chunks": ["ok"]}], FAKEGW_USAGE=usage)
        sent = {"authorization": "Bearer " + key, "x-vercel-ai-gateway-team": team}
        today = "/v1/report?start_date=2026-10-12&end_date=2026-10-12&group_by=model&api_key_id=self"
        self.assertEqual(g.ask("GET", today, headers=sent)[0], 200)
        self.assertEqual(g.ask("GET", "/ai-gateway/usage-report?teamId=t&scope=user", headers=sent)[0], 401)
        for path in ("/coding-agent/v1/models", "/coding-agent/v1/credits", "/v1/generation?id=g", "/codex/models", "/unknown?k=v"):
            g.ask("GET", path, headers=sent)
        # A bare status is that status and no rows, on the report too.
        write(usage, json.dumps({"/v1/report": {"status": 429, "delayMs": 100}}))
        status, _, body = g.ask("GET", today, headers=sent)
        self.assertEqual((status, body), (429, b"fake status 429"))
        status, _, _ = g.ask("POST", "/v4/ai/language-model", body=json.dumps({"prompt": []}), headers=dict(sent, **{"content-type": "application/json"}))
        self.assertEqual(status, 200)
        time.sleep(0.3)
        log = g.lines()
        # Each of the eight GETs is a line naming both headers, never a value.
        self.assertEqual(len(re.findall(r"request GET \S+ at=\d+ms -> \d+ at=\d+ms headers=authorization,x-vercel-ai-gateway-team\n", log)), 8, log)
        self.assertRegex(log, r"request GET /v1/report\?start_date=2026-10-12&end_date=2026-10-12&group_by=model&api_key_id=self at=\d+ms -> 200 at=\d+ms headers=authorization,x-vercel-ai-gateway-team\n")
        self.assertIn("usage /v1/report -> 200 rows=1 held 200ms", log)
        self.assertIn("usage /v1/report -> 429 held 100ms", log)
        for value in (key, team, "Bearer"):
            self.assertNotIn(value, log)
        kept = glob.glob(os.path.join(g.bodies, "*"))
        self.assertEqual(len(kept), 1, kept)
        for path in kept:
            self.assertNotIn(key, slurp(path))
            self.assertNotIn(team, slurp(path))

    def test_it_listens_on_loopback_only(self):
        g = self.gw()
        lsof = "/usr/sbin/lsof" if os.path.exists("/usr/sbin/lsof") else "lsof"
        out = subprocess.run([lsof, "-nP", "-a", "-p", str(g.p.pid), "-iTCP", "-sTCP:LISTEN"], capture_output=True, text=True).stdout
        listening = [line.split()[8] for line in out.splitlines()[1:]]
        self.assertEqual(listening, ["127.0.0.1:%d" % g.port], out)


class McpHttp(unittest.TestCase):
    def server(self, mode):
        d = scratch()
        self.mode = os.path.join(d, "mode")
        write(self.mode, mode + "\n")
        return Server(self, MCPHTTP, self.mode)

    def rpc(self, s, method, id_=None, params=None, headers=None):
        msg = {"jsonrpc": "2.0", "method": method}
        if id_ is not None:
            msg["id"] = id_
        if params is not None:
            msg["params"] = params
        status, h, body = s.ask("POST", "/mcp", body=json.dumps(msg), headers=dict({"content-type": "application/json"}, **(headers or {})))
        return status, h, json.loads(body) if body else None

    def test_ready(self):
        s = self.server("ready")
        self.assertEqual(self.rpc(s, "server/discover", 1)[0::2], (404, None))
        status, h, body = self.rpc(s, "initialize", 2, {"protocolVersion": "2025-06-18"})
        self.assertEqual((status, body["result"]["protocolVersion"], h.get("MCP-Session-Id") or h.get("mcp-session-id")), (200, "2025-06-18", "fxcap-session-1"))
        self.assertEqual(self.rpc(s, "initialize", 3, {"protocolVersion": "2026-07-28"})[2]["result"]["protocolVersion"], "2025-11-25")
        self.assertEqual(self.rpc(s, "notifications/initialized")[0::2], (202, None))
        tools = self.rpc(s, "tools/list", 4)[2]["result"]["tools"]
        self.assertEqual([t["name"] for t in tools], ["echo"])
        call = self.rpc(s, "tools/call", 5, {"name": "echo", "arguments": {"text": "hi"}}, {"mcp-session-id": "fxcap-session-1", "mcp-protocol-version": "2025-06-18"})[2]
        self.assertEqual(call["result"]["content"][0]["text"], "MCPHTTP_ECHO: hi")
        self.assertEqual(self.rpc(s, "bogus", 6)[2]["error"]["code"], -32601)
        self.assertEqual(s.ask("GET", "/mcp")[0], 405)
        self.assertEqual(s.ask("DELETE", "/mcp")[0], 204)
        log = s.lines()
        self.assertRegex(log, r"\+\d+ms POST /mcp rpc=tools/call id=5 session=yes version=2025-06-18 mode=ready -> 200 \+\d+ms")
        self.assertNotIn("fxcap-session-1", log)

    def test_methodnotfound_answers_discover_with_the_rpc_error_and_then_serves(self):
        s = self.server("methodnotfound")
        status, _, body = self.rpc(s, "server/discover", 1)
        self.assertEqual((status, body), (200, {"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "Method not found"}}))
        self.assertEqual(self.rpc(s, "initialize", 2, {"protocolVersion": "2025-03-26"})[2]["result"]["protocolVersion"], "2025-03-26")
        self.assertEqual(self.rpc(s, "tools/list", 3)[2]["result"]["tools"][0]["name"], "echo")

    def test_hang_holds_until_the_mode_changes(self):
        s = self.server("hang")
        with self.assertRaises(socket.timeout):
            s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"ping"}', timeout=0.8)
        write(self.mode, "ready\n")
        self.assertEqual(self.rpc(s, "ping", 2)[2], {"jsonrpc": "2.0", "id": 2, "result": {}})
        time.sleep(0.2)
        self.assertRegex(s.lines(), r"rpc=ping id=1 session=no version=- mode=ready -> 200")

    def test_fail_and_rpcerror(self):
        s = self.server("fail")
        self.assertEqual(s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"initialize"}')[0::2], (500, b"fixture failing"))
        write(self.mode, "rpcerror\n")
        self.assertEqual(self.rpc(s, "initialize", 2)[2]["error"], {"code": -32603, "message": "fixture refuses"})
        self.assertEqual(self.rpc(s, "notifications/initialized")[0], 202)


class McpStdio(unittest.TestCase):
    """scenarios/p5-mcp-stdio.ts, the p5-mcp scenarios' stdio server."""

    def start(self):
        self.log = os.path.join(scratch(), "wire.ndjson")
        self.p = subprocess.Popen([BUN, STDIO], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=dict(os.environ, MCPSTDIO_WIRE_LOG=self.log))
        self.addCleanup(self.stop)

    def stop(self):
        if self.p.poll() is None:
            self.p.kill()
            self.p.wait()
        self.p.stdin.close()
        self.p.stdout.close()

    def send(self, msg):
        self.p.stdin.write((json.dumps(msg) + "\n").encode())
        self.p.stdin.flush()

    def ask(self, msg):
        self.send(msg)
        return json.loads(self.p.stdout.readline())

    def test_the_legacy_handshake_a_call_and_the_wire_log(self):
        self.start()
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 1, "method": "server/discover"})["error"], {"code": -32601, "message": "Method not found"})
        init = self.ask({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "2025-11-25"}})["result"]
        self.assertEqual((init["protocolVersion"], init["serverInfo"]), ("2025-03-26", {"name": "legacy-fixture", "version": "1.0.0"}))
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})
        tools = self.ask({"jsonrpc": "2.0", "id": 3, "method": "tools/list"})["result"]["tools"]
        self.assertEqual([(t["name"], t["description"]) for t in tools], [("echo", "Echo text through the legacy MCP fixture")])
        call = self.ask({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"_meta": {"progressToken": 4}, "name": "echo", "arguments": {"text": "hi"}}})
        self.assertEqual(call["result"]["content"][0]["text"], "LEGACY_MCP_TOOL_RESULT:hi")
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 5, "method": "resources/list"})["result"]["resources"][0]["name"], "legacy-alpha")
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 6, "method": "prompts/list"})["result"]["prompts"][0]["name"], "review")
        with open(self.log) as f:
            logged = [json.loads(line) for line in f]
        self.assertEqual([e["sequence"] for e in logged], [1, 2, 3, 4, 5, 6, 7])
        self.assertEqual(logged[2], {"sequence": 3, "message": {"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}})
        self.assertEqual(sorted(logged[0]), ["message", "sequence"], "no pid, no time")

    def test_a_call_without_a_progress_token_ends_the_server(self):
        self.start()
        self.ask({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
        self.send({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "echo", "arguments": {"text": "hi"}}})
        self.assertEqual(self.p.wait(timeout=5), 4)

    def test_an_initialize_after_other_messages_ends_the_server(self):
        self.start()
        self.ask({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
        self.send({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})
        self.assertEqual(self.p.wait(timeout=5), 2)


if __name__ == "__main__":
    unittest.main()
