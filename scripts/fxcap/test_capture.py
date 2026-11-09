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
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest

HERE = os.path.dirname(os.path.realpath(__file__))
# No __pycache__ beside the scripts: the checkout holds only what is committed.
sys.dont_write_bytecode = True

CAPTURE = os.path.join(HERE, "capture.sh")
FAKEGW = os.path.join(HERE, "fakegw.ts")
MCPHTTP = os.path.join(HERE, "mcphttp.ts")
STDIO = os.path.join(HERE, "scenarios", "p5-mcp-stdio.ts")
DISCOVER_STDIO = os.path.join(HERE, "scenarios", "p5-mcp-discover-stdio.ts")

for _tool in ("tmux", "bun", "curl"):
    if not shutil.which(_tool):
        sys.exit("test-capture: %s is not on PATH; these tests need tmux, bun and curl and never skip" % _tool)
BUN = shutil.which("bun")
# The user's home as the password database has it: what a binary given no HOME
# of the user's finds (`~user`), and what capture.sh is given as HOME here.
HOMEDIR = __import__("pwd").getpwuid(os.getuid()).pw_dir
sys.path.insert(0, HERE)
import roots  # noqa: E402

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


def swap(path, text):
    """Replaces `path` with `text` whole, as capture.sh's verbs do: a file
    beside it, renamed into place. For a file a running server reads (its
    mode, a release, the usage answers): `write` truncates and then writes,
    and a reader in between finds the file empty (FLAKE-MCPMODE, registry
    #19: a held mcphttp request read its mode there and answered 500)."""
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".swap.")
    with os.fdopen(fd, "w") as f:
        f.write(text)
    os.replace(tmp, path)


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


def capture(cap, files=None, which="fx", binary="/bin/bash", size="30x200", scen_name="scenarios", timeout=120, stale=None,
            d=None, env=None):
    """capture.sh on a scenario `t` made of `cap` and `files` (relative path ->
    text, bytes, or `("link", target)` for a symbolic link), the binary a
    link to `binary` (or, for a binary that is not a link, that file
    itself). `stale` (file name -> text) is written into the out directory
    before the run, as an earlier run would have left it. `d`, a directory
    an earlier call made, runs this one in it as well, as the sweep runs a
    unit's two sides in one TMPDIR (the out directory is the side's own);
    `env` is more environment for capture.sh."""
    if d is None:
        d = scratch()
        sub = d
    else:
        sub = tempfile.mkdtemp(prefix="side.", dir=d)
    scen = os.path.join(d, scen_name)
    os.makedirs(scen, exist_ok=True)
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
        if not os.path.islink(link):
            os.makedirs(os.path.dirname(link))
            os.symlink(binary, link)
    else:
        link = binary
    out = os.path.join(sub, "out")
    for name, text in (stale or {}).items():
        write(os.path.join(out, name), text)
    extra = env
    env = dict(os.environ, FXCAP_SCENARIOS=scen, TMPDIR=d)
    for k in ("TMUX", "TMUX_PANE", "FX_BIN", "XFX_BIN"):
        env.pop(k, None)
    env["FX_BIN" if which == "fx" else "XFX_BIN"] = link
    env.update(extra or {})
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
            ("models2 missing.json\nsnap a\n", "models2 takes <catalog.json>, served from its line on, not missing.json"),
            # The old form, a moment on the gateway's clock: refused, not read as a file name.
            ("models2 c.json 5000\nsnap a\n", "models2 takes <catalog.json>, served from its line on, not c.json 5000"),
            ("modelsdelay soon\nsnap a\n", "modelsdelay takes milliseconds, not soon"),
            ("clock yesterday\nsnap a\n", "clock takes milliseconds since the epoch, not yesterday"),
            ("usage missing.json\nsnap a\n", "usage takes a file in"),
            ("mcphttp sometimes\nsnap a\n", "mcphttp takes ready, methodnotfound, hang, fail, rpcerror, modern, notimpl, old2024, malformed, hostile, ssehold, sseidless, ssestrid or ssefail, not sometimes"),
            ("jobctl now\nsnap a\n", "jobctl takes no argument, not now"),
            ("nodraw please\nsnap a\n", "nodraw takes no argument, not please"),
            ("held now\nsnap a\n", "held takes no argument, not now"),
            ("release 1\nsnap a\n", "release takes no argument, not 1"),
            ("until\nsnap a\n", "until takes an argument"),
            ("until soon Full access\nsnap a\n", "until takes shown <text> or gone <text>, not soon Full access"),
            ("until shown\nsnap a\n", "until takes shown <text> or gone <text>, not shown"),
            ("until gone   \nsnap a\n", "until takes shown <text> or gone <text>, not gone   "),
            ("absent\nsnap a\n", "absent takes an argument"),
            ("absent   \nsnap a\n", "absent takes <text>, not   "),
            ("expect\nsnap a\n", "expect takes an argument"),
            ("expect a\nsnap a\n", "expect takes <label> <text>, not a"),
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
        # Each placeholder reached the binary as the path it stands for, and
        # the snap that stored what it printed names each path by its own
        # placeholder (RunRoots): `@SCENARIOS@` is the resolved scenario
        # directory, `@HOME@` the run's HOME.
        a = r.screen("a")
        # Each placeholder as wide as its path (roots.py).
        C, B = P("scenarios", scen), P("bun", BUN)
        run = glob.glob(os.path.join(os.path.dirname(r.scen), "fxcap.*"))[0]
        H = re.escape(P("scratch", run))
        self.assertIn("env:%s/x:%s" % (C, B), a)
        self.assertRegex(a, r"typed:%s:%s:\d+" % (re.escape(C), re.escape(B)))
        self.assertIn("pasted-2:" + C, a)
        self.assertRegex(a, r"s=%s b=%s h=%s/home p=\d+ m=\d+" % (re.escape(C), re.escape(B), H))
        self.assertRegex(a, r"\d+ m=\d+%s/home" % H, "the `dir` copy's file, after the work file's")
        self.assertNotIn(scen, r.file("a"))
        self.assertNotIn(BUN, r.file("a"))
        # So does the `home` dump.
        home = r.file("h", ".files")
        self.assertRegex(home, r'\{"s=%s b=%s h=%s/home p=\d+ m=\d+"\}' % (re.escape(C), re.escape(B), H))
        self.assertNotIn(scen, home)
        self.assertNotIn(BUN, home)
        work = r.file("work-w")
        self.assertIn("== ./raw.bin\n\x00@BUN@", work, "a file with a NUL is left as it is")
        self.assertIn("relaunched:" + C, r.screen("b"))

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


def width_of(data):
    """Columns of a row's visible text, as cells.py counts them."""
    return roots._width(re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", "", data))


def P(name, path, real=False):
    """The placeholder for `path`, as wide as it."""
    return roots.make_placeholder(name, roots._width(path), real)


class RootsRewrite(unittest.TestCase):
    """roots.py on bytes, no tmux: each way a frame can carry a root, each
    placeholder written over its root's own columns, the literal placeholders,
    and the refusal."""

    RUN = "/private/tmp/claude-501/fxcapt.njl2bbl3/fxcap.JTbdal"  # local-path-ok: a fixture root, never read

    def rules(self):
        return roots.rules_for([(self.RUN + "/work", "work"), (self.RUN, "scratch"), (os.path.dirname(self.RUN), "tmp")])

    def rw(self, data):
        return roots.rewrite(data, self.rules())[0]

    def test_a_root_wrapped_at_the_margin_wraps_its_placeholder_at_every_byte(self):
        run, S = self.RUN.encode(), P("scratch", self.RUN).encode()
        for k in range(1, len(run)):
            with self.subTest(k=k):
                data = b"tmp=" + run[:k] + b"\n" + run[k:] + b"/fxtmp\n"
                got = self.rw(data)
                self.assertEqual(got, b"tmp=" + S[:k] + b"\n" + S[k:] + b"/fxtmp\n")
                self.assertEqual(len(got), len(data))
                self.assertEqual(roots.survivors(got, self.rules(), "fxcap.JTbdal"), [])

    def test_a_root_wrapped_at_two_offsets_reads_different_on_each_side(self):
        # The same root on both sides, broken by each at another column: the
        # pair differs after the rewrite as it did before it.
        run = self.RUN.encode()
        fx = self.rw(b"x=" + run[:9] + b"\n  " + run[9:] + b"\n")
        xfx = self.rw(b"x=" + run[:12] + b"\n  " + run[12:] + b"\n")
        S = P("scratch", self.RUN).encode()
        self.assertEqual(fx, b"x=" + S[:9] + b"\n  " + S[9:] + b"\n")
        self.assertEqual(xfx, b"x=" + S[:12] + b"\n  " + S[12:] + b"\n")
        self.assertNotEqual(fx, xfx)

    def test_a_root_a_binary_broke_behind_a_gutter_keeps_the_gutter_and_the_colours(self):
        # fx's notice (receipts/s1 `bogus`, rows 19-20): the row ends with the
        # root's head, the next opens with two spaces and a colour.
        run, W, S = self.RUN.encode(), P("work", self.RUN + "/work").encode(), P("scratch", self.RUN).encode()
        got = self.rw(b"Remove it from " + run[:13] + b"\n  \x1b[38;5;250m" + run[13:] + b"/work/.fx\x1b[39m\n")
        self.assertEqual(got, b"Remove it from " + W[:13] + b"\n  \x1b[38;5;250m" + W[13:] + b"/.fx\x1b[39m\n")
        boxed = self.rw(b"\xe2\x94\x82 " + run[:20] + b" \xe2\x94\x82\n\xe2\x94\x82 " + run[20:] + b"/x \xe2\x94\x82\n")
        self.assertEqual(boxed, b"\xe2\x94\x82 " + S[:20] + b" \xe2\x94\x82\n\xe2\x94\x82 " + S[20:] + b"/x \xe2\x94\x82\n")

    def test_a_colour_change_inside_a_root_stays_where_it_was(self):
        # The @plan picker bolds the letters a query matched, some inside the
        # run directory (workspace-index `at-plan`).
        run, S = self.RUN.encode(), P("scratch", self.RUN).encode()
        sgr = (b"\x1b[38;5;245m", b"\x1b[1m", b"\x1b[0m\x1b[38;5;245m", b"\x1b[39m")
        ansi = b"  " + sgr[0] + run[:1] + sgr[1] + run[1:2] + sgr[2] + run[2:] + b"/extra/" + sgr[3] + b"\n"
        got = self.rw(ansi)
        self.assertEqual(got, b"  " + sgr[0] + S[:1] + sgr[1] + S[1:2] + sgr[2] + S[2:] + b"/extra/" + sgr[3] + b"\n")
        # The .txt of the same frame reads the same text, and every cell of
        # both is where it was.
        txt = b"  " + run + b"/extra/\n"
        self.assertEqual(cells_text(got), self.rw(txt).decode())
        self.assertEqual(width_of(got.decode()), width_of(ansi.decode()))
        # Two frames that bolded other letters of the root differ still.
        other = b"  " + sgr[0] + run[:4] + sgr[1] + run[4:5] + sgr[2] + run[5:] + b"/extra/" + sgr[3] + b"\n"
        self.assertNotEqual(self.rw(other), got)

    def test_every_spelling_and_an_osc_8_target(self):
        esc = self.RUN.replace("/", "\\/") + "\\/work"
        got = self.rw(b'{"cwd":"' + esc.encode() + b'"}')
        self.assertEqual(got, b'{"cwd":"' + P("work", esc).encode() + b'"}')
        self.assertEqual(json.loads(got)["cwd"], P("work", esc))
        link = b"\x1b]8;;file://" + self.RUN.encode() + b"/work/a.png\x1b\\[i]\x1b]8;;\x1b\\\n"
        self.assertEqual(self.rw(link), b"\x1b]8;;file://" + P("work", self.RUN + "/work").encode() + b"/a.png\x1b\\[i]\x1b]8;;\x1b\\\n")
        spaced = roots.rules_for([("/tmp/a b/fxcap.Q1w2E3", "scratch")])
        pct = "/tmp/a%20b/fxcap.Q1w2E3"
        self.assertEqual(roots.rewrite(("file://%s/x" % pct).encode(), spaced)[0], ("file://%s/x" % P("scratch", pct)).encode())
        # A TMPDIR with a trailing slash names `<TMPDIR>//fxcap.X`; a binary
        # that normalizes the path prints it with one slash.
        doubled = roots.rules_for([("/tmp/a//b/fxcap.Q1w2E3", "scratch")])
        got = roots.rewrite(b"/tmp/a/b/fxcap.Q1w2E3/x /tmp/a//b/fxcap.Q1w2E3/y", doubled)[0]
        self.assertEqual(got, ("%s/x %s/y" % (P("scratch", "/tmp/a/b/fxcap.Q1w2E3"), P("scratch", "/tmp/a//b/fxcap.Q1w2E3"))).encode())

    def test_an_nf_escape_beside_a_root_is_a_control_sequence(self):
        # `ESC ( B` (designate G0 as ASCII) is three bytes; read as two, its
        # `B` was text, and the root before it did not end whole.
        run, W = self.RUN.encode(), P("work", self.RUN + "/work").encode()
        self.assertEqual(self.rw(b"x \x1b(B" + run + b"/work\x1b(B y\n"), b"x \x1b(B" + W + b"\x1b(B y\n")

    def test_a_root_ends_whole_within_its_row(self):
        run = self.RUN.encode()
        S, W = P("scratch", self.RUN).encode(), P("work", self.RUN + "/work").encode()
        self.assertEqual(self.rw(run + b"/workx\n"), S + b"/workx\n")
        self.assertEqual(self.rw(run + b"/work\nabc\n"), W + b"\nabc\n")

    def test_a_literal_placeholder_is_never_a_root(self):
        # One side shows the real root, the other the text `<work>`: they
        # must not read the same.
        real = self.rw(b"x=" + self.RUN.encode() + b"/work\n")
        literal = self.rw(b"x=<work>\n")
        self.assertEqual(real, b"x=" + P("work", self.RUN + "/work").encode() + b"\n")
        self.assertEqual(literal, b"x=<!work>\n")
        self.assertNotEqual(real, literal)
        self.assertEqual(
            self.rw(b"<!work> <wo\x1b[1mrk> <tmp:real> <work___> <h__> <b> <a__>"),
            b"<!!work> <!wo\x1b[1mrk> <!tmp:real> <!work___> <!h__> <b> <a__>",
        )
        # The escape keeps a JSON body a JSON body.
        self.assertEqual(json.loads(self.rw(b'{"text":"<work> and <!scratch__>"}')), {"text": "<!work> and <!!scratch__>"})

    def test_a_resolved_root_is_its_own_placeholder(self):
        d = scratch()
        os.makedirs(os.path.join(d, "real"))
        os.symlink(os.path.join(d, "real"), os.path.join(d, "link"))
        given = os.path.join(d, "link", "fxcap.Ab12Cd")
        os.makedirs(given)
        rules = roots.rules_for([(given, "scratch")])
        resolved = os.path.realpath(given)
        self.assertNotEqual(given, resolved)
        got = roots.rewrite(("%s and %s\n" % (given, resolved)).encode(), rules)[0]
        self.assertEqual(got, ("%s and %s\n" % (P("scratch", given), P("scratch", resolved, True))).encode())

    def test_a_root_narrower_than_its_name_and_one_too_narrow(self):
        home = "/" + "root"
        got = roots.rewrite(("cd %s/x\n" % home).encode(), roots.rules_for([(home, "home")]))[0]
        self.assertEqual(got, b"cd <h__>/x\n")
        with self.assertRaises(roots.Refused):
            roots.rewrite(b"cd /ab/x\n", roots.rules_for([("/ab", "home")]))
        d = scratch()
        path = os.path.join(d, "f.txt")
        write(path, "cd /ab/x\n")
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "roots.py"), "--root", "/ab", "home", "--", path],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 3, r.stderr)
        self.assertIn("has no placeholder of its width", r.stderr)

    def test_a_root_of_other_characters_keeps_its_columns(self):
        root = "/tmp/j\u00f6s\u00e9/fxcap.Q1w2E3"
        row = ("[%s/x]\n" % root).encode()
        got = roots.rewrite(row, roots.rules_for([(root, "scratch")]))[0].decode()
        self.assertEqual(width_of(got), width_of(row.decode()))
        self.assertEqual(got, "[%s/x]\n" % P("scratch", root))

    def test_a_sibling_of_an_outer_root_is_neither_rewritten_nor_refused(self):
        # A path whose name extends a root's is another path: left as it is,
        # and no survivor (the rewrite's rule, the refusal's rule).
        home, tmp, scen = "/ho" + "me/alice", "/tmp/fxt", "/src/scen"
        rules = roots.rules_for([(home, "home"), (tmp, "tmp"), (scen, "scenarios")])
        siblings = ("%s2/notes %s2/x %s-old/x\n" % (home, tmp, scen)).encode()
        self.assertEqual(roots.rewrite(siblings, rules)[0], siblings)
        self.assertEqual(roots.survivors(siblings, rules, "fxcap.JTbdal"), [])
        # The roots themselves, left unwritten, are survivors.
        whole = ("%s/notes %s %s/x\n" % (home, tmp, scen)).encode()
        self.assertEqual(sorted(roots.survivors(whole, rules, None)), ["home", "scenarios", "tmp"])

    def test_a_styled_sibling_of_an_outer_root_is_neither_rewritten_nor_refused(self):
        # A colour change mid-name, right where the root's bytes end (a picker
        # bolding the letters a query matched): the reader sees `<HOME>2/notes`,
        # another path. The escape is no end of the root, to either rule.
        home, tmp, scen = "/ho" + "me/alice", "/tmp/fxt", "/src/scen"
        rules = roots.rules_for([(home, "home"), (tmp, "tmp"), (scen, "scenarios")])
        d = scratch()
        for root, name, rest in ((home, "home", "2/notes"), (tmp, "tmp", "2/x"), (scen, "scenarios", "-old/x")):
            for style in (b"\x1b[1m", b"\x1b[38;5;245m", b"\x1b(B"):
                data = b"x " + root.encode() + style + rest.encode() + b"\x1b[0m y\n"
                with self.subTest(root=name, style=style):
                    self.assertEqual(roots.rewrite(data, rules)[0], data)
                    self.assertEqual(roots.survivors(data, rules, None), [])
                    path = os.path.join(d, "%s.ansi" % name)
                    with open(path, "wb") as f:
                        f.write(data)
                    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "roots.py"), "--root", root, name,
                                        "--", path], capture_output=True, text=True)
                    self.assertEqual(r.returncode, 0, r.stderr)
                    with open(path, "rb") as f:
                        self.assertEqual(f.read(), data)
        # Styled where the root ends whole, it is still the root.
        data = b"x " + home.encode() + b"\x1b[1m/notes\x1b[0m\n"
        self.assertEqual(roots.survivors(data, rules, None), ["home"])

    def test_a_root_with_a_gutter_character_the_view_drops_still_refuses(self):
        # The view drops a row's edge gutters (spaces, tabs, U+2500-U+259F). A
        # root that ends in one, at a row's end, loses that character to it;
        # a root with one inside it, broken beside it at a row's end, reads
        # joined without it. Neither is found to be written, so each refuses
        # the file: fail closed.
        for root in ("/tmp/fxt ", "/tmp/fxt\t", "/tmp/fxt\u2502"):
            rules = roots.rules_for([(root, "tmp")])
            for data in (("x %s\nnext\n" % root).encode(), ("x %s" % root).encode()):
                with self.subTest(root=root, data=data):
                    self.assertEqual(roots.survivors(roots.rewrite(data, rules)[0], rules, None), ["tmp"])
        spaced = "/tmp/a b"
        rules = roots.rules_for([(spaced, "tmp")])
        for data in (b"x /tmp/a \nb/notes\n", b"x /tmp/a\n b/notes\n", b"x /tmp/a\t\n  \xe2\x94\x82 b/notes\n"):
            with self.subTest(data=data):
                self.assertEqual(roots.survivors(roots.rewrite(data, rules)[0], rules, None), ["tmp"])
        # Away from a row's edge such a root is found and written, and
        # nothing survives.
        got = roots.rewrite(b"x /tmp/a b/notes\n", rules)[0]
        self.assertEqual(got, b"x " + P("tmp", spaced).encode() + b"/notes\n")
        self.assertEqual(roots.survivors(got, rules, None), [])
        # Two runs, broken at either one: the view drops that run whole and
        # keeps the other, so each run is read present and absent.
        rules = roots.rules_for([("/tmp/a b c", "tmp")])
        for data in (b"x /tmp/a \nb c/notes\n", b"x /tmp/a b \nc/notes\n", b"x /tmp/a  \n  \xe2\x94\x82 b c\n"):
            with self.subTest(data=data):
                self.assertEqual(roots.survivors(roots.rewrite(data, rules)[0], rules, None), ["tmp"])
        got = roots.rewrite(b"x /tmp/a b c\n", rules)[0]
        self.assertEqual(roots.survivors(got, rules, None), [])
        # A root with more runs than that refuses its run's every file.
        many = "/tmp/" + " x" * (roots.MAX_GUTTER_RUNS + 1)
        with self.assertRaises(roots.Refused):
            roots.rules_for([(many, "tmp")])
        d = scratch()
        path = os.path.join(d, "f.txt")
        write(path, "nothing\n")
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "roots.py"), "--root", many, "tmp", "--", path],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 3, r.stderr)
        self.assertIn("LOCAL-PATH GUARD", r.stderr)

    def test_a_root_whose_head_an_escape_took_is_refused_beside_a_styled_sibling(self):
        # Conservative, on purpose: `ESC / h` takes the root's head, and a
        # colour change where the root's bytes end makes them end whole. The
        # reader sees `ome/alice2/notes`; nothing it holds tells the sibling
        # from the root, so the file is refused.
        home = "/ho" + "me/alice"
        rules = roots.rules_for([(home, "home")])
        self.assertEqual(roots.survivors(b"\x1b" + home.encode() + b"\x1b[1m2/notes\n", rules, None), ["home"])

    def test_a_root_in_a_control_sequences_bytes_still_refuses(self):
        # No reader sees an OSC payload, so its bytes are read: an OSC 8
        # link's target left unwritten refuses the file, a sibling there does not.
        home = "/ho" + "me/alice"
        rules = roots.rules_for([(home, "home")])
        link = b"\x1b]8;;file://%s/shot.png\x1b\\shot\x1b]8;;\x1b\\\n"
        self.assertEqual(roots.survivors(link % home.encode(), rules, None), ["home"])
        self.assertEqual(roots.survivors(link % (home + "2").encode(), rules, None), [])
        self.assertEqual(roots.survivors(roots.rewrite(link % home.encode(), rules)[0], rules, None), [])
        # A root whose head an escape's own bytes took (`ESC / h` is an nF
        # escape): a reader sees `ome/alice/x`, the bytes hold the root.
        self.assertEqual(roots.survivors(b"x \x1b" + home.encode() + b"/x\n", rules, None), ["home"])

    def test_what_survives_refuses_the_file(self):
        # A binary that cut the root's head off (an ellipsis), or a head that
        # scrolled off the frame: the run directory's name is still there.
        cut = self.rw(b"\xe2\x80\xa6/fxcap.JTbdal/work\n")
        self.assertEqual(roots.survivors(cut, self.rules(), "fxcap.JTbdal"), ["the run directory's name"])
        d = scratch()
        path = os.path.join(d, "f.txt")
        write(path, "\u2026/fxcap.JTbdal/work\n")
        r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "roots.py"), "--suffix", "fxcap.JTbdal",
                            "--root", self.RUN, "scratch", "--", path], capture_output=True, text=True)
        self.assertEqual(r.returncode, 3, r.stderr)
        self.assertIn("still holds the run directory's name", r.stderr)
        self.assertNotIn("JTbdal", r.stderr.replace(path, ""))

    def test_a_file_without_a_root_is_left_byte_for_byte(self):
        data = b"\x1b[1mnothing\x1b[0m here \xe2\x94\x82 /tmp/elsewhere\n<html>\n"
        self.assertEqual(self.rw(data), data)


def cells_text(data):
    """A frame's rows as cells.py reads them, colours set aside."""
    sys.path.insert(0, HERE)
    import cells

    return "".join(cells.text_of(r) + "\n" for r in cells.parse(data.decode())[:-1])


class RunRoots(unittest.TestCase):
    """Every file a run writes for the out directory names the run's own
    roots by a placeholder as wide as each -- `<work___>`, `<scratch___>`,
    `<scenarios___>`, `<bun___>`, `<tmp___>`, `<home___>` -- however the binary
    printed them: a snap's rows and its colours, read apart, an OSC 8 link's
    target, a title, the wire, a request body, mcphttp's log, a clipboard
    copy, a copied file's path, a report, the workspace and HOME dumps. At any
    length of TMPDIR, fx's side and xfx's alike, every column where it was."""

    # The stub binary prints, one item a row: its working directory (as
    # `$PWD` says it and as `pwd -P` resolves it), its HOME and TMPDIR, the
    # harness's TMPDIR two levels up, the user's home (from the password
    # database: the binary is given no HOME of the user's), the scenario
    # directory; a root a name character follows; two padded roots; a root
    # the terminal wraps at its margin (it starts seven columns before it,
    # whatever its length); a root broken behind a box-drawing gutter, as a
    # binary breaks one; a root a colour change splits; a coloured root; a
    # link, a title, a bell; and it copies, copies a file, writes a report,
    # and asks the fake gateway and the MCP server with its directory in the
    # body and the path. One line and one Enter, so the shell echoes nothing
    # in between; `clear` takes the line's own echo away.
    CAP = (
        "mcphttp ready\n"
        "keep\n"
        "wait 1\n"
        "type clear; p=\"$PWD\"; echo \"$p\" > made.txt; echo \"$p\" > ~/note.txt; "
        "printf 'cwd=%s\\nreal=%s\\nhome=%s\\ntmp=%s\\n' \"$p\" \"$(pwd -P)\" \"$HOME\" \"$TMPDIR\"; "
        "printf 'up=%s\\nuser=%s\\nscen=%s\\n' \"$(dirname \"$(dirname \"$p\")\")\" \"$(eval echo ~\"$(id -un)\")\" @SCENARIOS@; "
        "printf 'edge=%sx\\n' \"$p\"; "
        "printf 'pad1=[%120s]\\npad2=[%120s]\\n' \"$p\" \"$TMPDIR\"; "
        "printf 'wrap=%188s%s\\n' '' \"$p\"; "
        "printf 'broke=%s\\n  \\342\\224\\202 %s/end\\n' \"${p:0:9}\" \"${p:9}\"; "
        "printf 'bold=%s\\033[1m%s\\033[22m%s\\n' \"${p:0:3}\" \"${p:3:4}\" \"${p:7}\"; "
        "printf 'tint=\\033[38;5;245m%s\\033[39m\\n' \"$p\"; "
        "printf 'link=\\033]8;;file://%s/shot.png\\033\\\\[img]\\033]8;;\\033\\\\\\n' \"$p\"; "
        "printf '\\033]2;at %s\\007\\a' \"$p\"; "
        "printf %s \"$p\" | pbcopy; osascript -e copy \"$p/shot.png\"; "
        "printf 'report %s\\n' \"$p\" > \"$TMPDIR/stub-trace-1.md\"; "
        "curl -s -m 5 -o /dev/null -X POST --data \"{\\\"cwd\\\":\\\"$p\\\"}\" http://127.0.0.1:@PORT@/v4/ai/language-model; "
        "curl -s -m 5 -o /dev/null \"http://127.0.0.1:@MCPPORT@$p/mcp\"; "
        "echo DO\"\"NE\n"
        "key Enter\n"
        "until shown DONE\n"
        "snap a\n"
        "snapfull a\n"
        "title t\n"
        "cursor c\n"
        "bells b\n"
        "wire w\n"
        "clip k\n"
        "clipfile f\n"
        "reports r\n"
        "sockets s\n"
        "gwlog\n"
        "workls w\n"
        "home h\n"
    )

    def capture_at(self, extra, which="fx", d=None, cap=None, tmpdir=None):
        """A capture at a TMPDIR `extra` bytes longer than the scratch base
        (`tmpdir`, if given, is what capture.sh is handed as TMPDIR)."""
        if d is None:
            d = scratch()
            if extra:
                d = os.path.join(d, "t" * (extra - 1))
                os.makedirs(d)
        env = {"HOME": HOMEDIR}
        if tmpdir is not None:
            env["TMPDIR"] = tmpdir
        return capture(cap or self.CAP, which=which, d=d, env=env), d

    def newest_run(self, d):
        return sorted(glob.glob(os.path.join(d, "fxcap.*")), key=os.path.getmtime)[-1]

    def roots_of(self, d):
        """The paths no file of a run in `d` may hold, as given and resolved,
        and each run directory's name."""
        runs = glob.glob(os.path.join(d, "fxcap.*"))
        paths = set(runs) | {d, os.path.join(d, "scenarios"), BUN, HOMEDIR}
        names = {os.path.basename(r) for r in runs}
        return {q for p in paths for q in (p, os.path.realpath(p))} | names

    def files(self, r):
        for top, _, names in os.walk(r.out):
            for name in names:
                path = os.path.join(top, name)
                with open(path, "rb") as f:
                    yield path, f.read()

    def rows(self, r, label, ext=".txt"):
        return r.file(label, ext).split("\n")

    def row(self, rows, prefix):
        hits = [i for i, x in enumerate(rows) if x.startswith(prefix)]
        self.assertEqual(len(hits), 1, (prefix, rows))
        return hits[0]

    def expected(self, d):
        """Each placeholder the newest run in `d` writes, as wide as its root.
        What the binary derives from `getcwd()` is the run resolved; its HOME
        and TMPDIR are the run as given. Where TMPDIR goes through a link
        (macOS's /tmp, /var/folders), each is its own."""
        run = self.newest_run(d)
        real = os.path.realpath(run) != run
        rrun, rtmp = os.path.realpath(run), os.path.realpath(d)
        return {
            "work": P("work", rrun + "/work", real),
            "scratch": P("scratch", run),
            "scratch_p": P("scratch", rrun, real),
            "tmp": P("tmp", rtmp, real),
            "home": P("home", HOMEDIR),
            "scen": P("scenarios", os.path.realpath(os.path.join(d, "scenarios"))),
            "run": run,
            "real": real,
        }

    def assert_written_by_placeholder(self, r, d):
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        side = r.which
        x = self.expected(d)
        W, S = x["work"], x["scratch"]
        files = list(self.files(r))
        kinds = {os.path.basename(p).split(".", 1)[-1] for p, _ in files}
        for kind in ("txt", "ansi", "full.txt", "full.ansi", "title", "cursor", "bells", "wire", "clip",
                     "clipfile", "md", "sockets", "files", "log", "json", "port"):
            self.assertIn(kind, kinds, "the run wrote no .%s file" % kind)
        for path, data in files:
            for root in self.roots_of(d):
                self.assertNotIn(root.encode(), data, "%s holds %s" % (path, root))
        # The .txt, row by row: what the binary printed, each root in place,
        # as wide as it was.
        rows = self.rows(r, "a")
        for prefix, want in (
            ("cwd=", "cwd=" + W),
            ("real=", "real=" + W),
            ("home=", "home=%s/home" % S),
            ("tmp=", "tmp=%s/%stmp" % (S, side)),
            ("up=", "up=" + x["tmp"]),
            ("user=", "user=" + x["home"]),
            ("scen=", "scen=" + x["scen"]),
            ("edge=", "edge=%s/workx" % x["scratch_p"]),
            ("bold=", "bold=" + W),
            ("tint=", "tint=" + W),
        ):
            self.assertEqual(rows[self.row(rows, prefix)], want)
        # Wrapped at the margin: the placeholder wraps where the root did,
        # seven columns in.
        i = self.row(rows, "wrap=")
        self.assertEqual(rows[i : i + 2], ["wrap=" + " " * 188 + W[:7], W[7:]])
        # Broken behind a gutter: the gutter and the break stay.
        i = self.row(rows, "broke=")
        self.assertEqual(rows[i : i + 2], ["broke=" + W[:9], "  \u2502 %s/end" % W[9:]])
        # The .ansi, read apart from the .txt: the coloured root, the root a
        # colour splits (each change where it was), the wrap, the link.
        ansi = self.rows(r, "a", ".ansi")
        tint = ansi[self.row(ansi, "tint=")]
        self.assertIn("\x1b[38;5;245m%s\x1b[39m" % W, tint)
        bold = ansi[self.row(ansi, "bold=")]
        changes = [width_of(bold[: m.start()]) for m in re.finditer(r"\x1b\[[0-9;]*m", bold)]
        self.assertEqual(re.sub(r"\x1b\[[0-9;]*m", "", bold), "bold=" + W)
        self.assertIn(len("bold=") + 3, changes, repr(bold))
        self.assertIn(len("bold=") + 7, changes, repr(bold))
        self.assertEqual([a.rstrip() for a in ansi[self.row(ansi, "wrap=") :][:2]], ["wrap=" + " " * 188 + W[:7], W[7:]])
        link = ansi[self.row(ansi, "link=")]
        # tmux's `capture-pane -e` writes the link where it keeps hyperlinks
        # (3.7b here); where it writes none, no file holds a root all the
        # same (above), and the wire holds the link (below).
        if "\x1b]8;;" in link:
            self.assertIn("\x1b]8;;file://%s/shot.png" % W, link)
        # Every other kind.
        self.assertEqual(r.file("t", ".title").strip(), "at " + W)
        self.assertRegex(r.file("c", ".cursor"), r"^\d+,\d+\n$")
        self.assertEqual(r.file("b", ".bells"), "bells=1\nclients=0\n")
        wire = r.file("w", ".wire")
        for want in ("cwd=" + W, "file://%s/shot.png" % W, "broke=" + W[:9], "wrap=" + " " * 188 + W):
            self.assertIn(want, wire)
        self.assertEqual(r.file("k", ".clip"), W)
        self.assertEqual(r.file("f", ".clipfile"), W + "/shot.png\n")
        self.assertEqual(r.file("r-1", ".md"), "report %s\n" % W)
        self.assertRegex(r.file("s", ".sockets"), r"^pid \d+\n$")
        self.assertRegex(r.file("mcphttp", ".port"), r"^\d+\n$")
        self.assertIn(" GET %s/mcp " % W, r.file("mcphttp", ".log"))
        bodies = glob.glob(os.path.join(r.out, "%s-t-%s-bodies" % (side, r.size), "*"))
        self.assertTrue(any('{"cwd":"%s"}' % W in slurp(b) for b in bodies), bodies)
        self.assertIn("== ./made.txt\n%s\n" % W, r.file("work-w"))
        self.assertIn(W, r.file("h", ".files"))
        # The `roots:` line: each root's length on this run in bytes, the
        # resolved form's too where it is another path.
        run = x["run"]
        want = "roots: <work>=%d " % (len(run) + 5)
        if x["real"]:
            want += "<work:real>=%d " % (len(os.path.realpath(run)) + 5)
        self.assertIn(want + "<scratch>=%d " % len(run), r.stdout)

    def test_every_file_a_run_writes_names_its_roots_by_placeholder_at_any_tmpdir(self):
        # Three lengths of TMPDIR: the wrap and the pads land at a different
        # byte of each root on each.
        for extra in (0, 9, 26):
            with self.subTest(extra=extra):
                r, d = self.capture_at(extra)
                self.assert_written_by_placeholder(r, d)

    def test_a_tmpdir_with_a_trailing_slash(self):
        # macOS's own TMPDIR ends in `/`. Through a link too, the binary's
        # HOME as it was given (capture.sh strips the slash before mktemp, so
        # no `//` widens it) and normalized (`//` to `/`, as a binary that
        # joins paths prints it) is the run's root, as wide as the run.
        d = scratch()
        os.makedirs(os.path.join(d, "r"))
        os.symlink(os.path.join(d, "r"), os.path.join(d, "l"))
        link = os.path.join(d, "l")
        cap = ("keep\nwait 1\n"
               "type clear; printf 'raw=%s norm=%s\\n' \"$HOME\" \"$(printf %s \"$HOME\" | tr -s /)\"; echo DO\"\"NE\n"
               "key Enter\nuntil shown DONE\nsnap a\n")
        r, _ = self.capture_at(0, d=link, cap=cap, tmpdir=link + "/")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        run = self.newest_run(link)
        self.assertIn("raw={0}/home norm={0}/home".format(P("scratch", run)), self.rows(r, "a"))
        r, d = self.capture_at(9, tmpdir=None)
        r2, _ = self.capture_at(0, d=d, tmpdir=d + "/")
        self.assert_written_by_placeholder(r2, d)

    def test_a_sibling_of_the_harness_tmpdir_is_left_and_refuses_nothing(self):
        cap = ("keep\nwait 1\n"
               "type clear; printf 'sib=%s2/notes\\n' \"$(dirname \"$(dirname \"$PWD\")\")\"; echo DO\"\"NE\n"
               "key Enter\nuntil shown DONE\nsnap a\n")
        r, d = self.capture_at(0, cap=cap)
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("sib=%s2/notes" % os.path.realpath(d), self.rows(r, "a"))

    def test_a_resolved_root_reads_as_its_own_placeholder(self):
        # TMPDIR through a link. The binary's HOME and TMPDIR are the run as
        # given; its working directory is the run resolved (`env -i` drops
        # PWD, so the shell's `$PWD` is `getcwd()` too). One root, two
        # placeholders, each as wide as its own path: a binary that printed
        # the other form would differ.
        d = scratch()
        os.makedirs(os.path.join(d, "r"))
        os.symlink(os.path.join(d, "r"), os.path.join(d, "l"))
        link = os.path.join(d, "l")
        r, _ = self.capture_at(0, d=link)
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        run = self.newest_run(link)
        S, W = P("scratch", run), P("work", os.path.realpath(run) + "/work", True)
        rows = self.rows(r, "a")
        for prefix, want in (("home=", "home=%s/home" % S), ("tmp=", "tmp=%s/fxtmp" % S),
                             ("cwd=", "cwd=" + W), ("real=", "real=" + W)):
            self.assertEqual(rows[self.row(rows, prefix)], want)
        self.assertTrue(W.startswith("<work:real"))

    def test_both_sides_are_written_alike_and_keep_their_widths(self):
        # The sweep's way: both sides in one TMPDIR, one after the other, so
        # each run directory is as long as the other's.
        fx, d = self.capture_at(0)
        xfx, _ = self.capture_at(0, which="xfx", d=d)
        self.assert_written_by_placeholder(xfx, d)
        S = self.expected(d)["scratch"]
        for ext in (".txt", ".ansi"):
            a, b = fx.file("a", ext).split("\n"), xfx.file("a", ext).split("\n")
            self.assertEqual(len(a), len(b))
            for x, y in zip(a, b):
                if x.startswith("pad2="):
                    # Padded to the real TMPDIRs' widths: fx's `/fxtmp` one
                    # column further right than xfx's `/xfxtmp`, as `fxtmp`
                    # is one byte shorter, and the bracket where it was.
                    self.assertEqual(x.rindex("]"), y.rindex("]"))
                    self.assertEqual(x.index(S + "/fxtmp") - 1, y.index(S + "/xfxtmp"))
                else:
                    # Each binary's own TMPDIR keeps its own name, as the
                    # ledger's `capture-tmpdir` reads it; nothing else differs.
                    self.assertEqual(x.replace(S + "/fxtmp", S + "/xfxtmp"), y)

    def test_a_root_each_side_broke_at_another_column_reads_different(self):
        # The same root, broken by fx after its ninth byte and by xfx after
        # its twelfth: the pair differs after the rewrite as it did before.
        cap = ("keep\nwait 1\n"
               "fx: type clear; p=\"$PWD\"; printf 'x=%s\\n  %s\\n' \"${p:0:9}\" \"${p:9}\"; echo DO\"\"NE\n"
               "xfx: type clear; p=\"$PWD\"; printf 'x=%s\\n  %s\\n' \"${p:0:12}\" \"${p:12}\"; echo DO\"\"NE\n"
               "key Enter\nuntil shown DONE\nsnap a\n")
        fx, d = self.capture_at(0, cap=cap)
        Wfx = self.expected(d)["work"]
        xfx, _ = self.capture_at(0, which="xfx", d=d, cap=cap)
        Wxfx = self.expected(d)["work"]
        self.assertEqual(Wfx, Wxfx)
        a, b = self.rows(fx, "a"), self.rows(xfx, "a")
        i, j = self.row(a, "x="), self.row(b, "x=")
        self.assertEqual(a[i : i + 2], ["x=" + Wfx[:9], "  " + Wfx[9:]])
        self.assertEqual(b[j : j + 2], ["x=" + Wfx[:12], "  " + Wfx[12:]])
        self.assertNotEqual(a, b)

    def test_a_real_root_and_a_literal_placeholder_never_read_the_same(self):
        # One side prints its working directory, the other the text `<work>`.
        cap = ("keep\nwait 1\n"
               "fx: type clear; printf 'x=%s\\n' \"$PWD\"; echo DO\"\"NE\n"
               "xfx: type clear; printf 'x=<work>\\n'; echo DO\"\"NE\n"
               "key Enter\nuntil shown DONE\nsnap a\n")
        fx, d = self.capture_at(0, cap=cap)
        W = self.expected(d)["work"]
        xfx, _ = self.capture_at(0, which="xfx", d=d, cap=cap)
        a, b = self.rows(fx, "a"), self.rows(xfx, "a")
        self.assertIn("x=" + W, a)
        self.assertIn("x=<!work>", b)
        self.assertNotEqual(a, b)


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

    def test_a_home_file_a_verb_replaces_is_never_a_link_or_a_directory(self):
        # `rehomefile` renames its file into place (FLAKE-MCPMODE): a HOME path
        # that is a link to a file outside is refused, never written through,
        # and one that is a link to a directory outside is refused, never
        # moved into.
        victim = self.sentinel("keep\n")
        outside = os.path.dirname(victim)
        for target in (victim, outside):
            with self.subTest(target=os.path.basename(target)):
                r = capture("wait 0.5\ntype ln -s '%s' ~/.linked; printf 'made-%%s\\n' it\nkey Enter\nuntil shown made-it\n"
                            "rehomefile .linked new.json\nsnap never\n" % target, {"new.json": "{}\n"})
                self.assertEqual(r.code, 2, r.stdout + r.stderr)
                self.assertIn("refusing to replace", r.stderr)
                self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-never.txt")), "the run stopped there")
                self.assertEqual(slurp(victim), "keep\n")
                self.assertEqual(sorted(os.listdir(outside)), ["sentinel"])


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
        r = capture("nodraw\njobctl\nkeep\nwait 1\nsignal SIGTSTP\nsignal SIGCONT\ntype hello\nkey Enter\nwait 0.5\nsnap a\nsignal HUP\nwait 0.3\nsnap b\n",
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
        r = capture("nodraw\nkeep\nwait 1\nsignal TSTP\nsignal SIGTERM\nwait 0.3\nsnap a\n", binary="/bin/cat")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        # The pane's session leader's group is orphaned: the stop never lands.
        self.assertRegex(r.stdout, r"signal SIGTSTP -> pid \d+: [^T]")
        self.assertRegex(r.stdout, r"signal SIGTERM -> pid \d+: \S*; pane dead, signal (?:15|term)\n")

    def test_a_pid_whose_executable_is_not_the_binary_is_refused_and_the_guards_still_run(self):
        # A wrapper that execs /bin/cat: the pane's pid runs /bin/sh until the
        # exec lands, and /bin/cat after. Once at once, and once with the exec
        # 4 s late, as a loaded machine made it (registry #19, 16th sighting:
        # still /bin/sh after `wait 1` and binary_pid's 2 s of looks). So the
        # scenario waits for the exec, not for a second count: the wrapper
        # turns the terminal's echo off and says `armed`, and a line typed
        # after that is on the screen only once cat has read it and written
        # it back -- once the pid runs /bin/cat.
        for late in ("", "sleep 4\n"):
            with self.subTest(late=late):
                d = scratch()
                script = os.path.join(d, "wrapper")
                write(script, "#!/bin/sh\nstty -echo\nprintf 'armed\\n'\n%sexec /bin/cat\n" % late)
                os.chmod(script, 0o755)
                r = capture("until shown armed\ntype ready\nkey Enter\nuntil shown ready\nsignal SIGTERM\nsnap a\n", binary=script)
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
        # The second catalog from the `models2` line on, by order alone: each
        # ask's answer is waited for on the screen, so no step races another
        # however late the shell or the gateway runs (FLAKE-HINT4 r2: row 19,
        # where the first ask came 9 s after the gateway started, past the
        # old fixed 5 s, and got the second catalog).
        get = "type curl -s \"$FX_E2E_GATEWAY_MODELS_URL\"; echo\nkey Enter\nuntil shown %s\n"
        one, two = '{"data":[{"id":"fake/one"}]}', '{"data":[{"id":"fake/two"}]}'
        r = capture(
            "models one.json\nmodelsdelay 200\nwait 0.5\n" + get % one + "models2 two.json\n" + get % two + "snap a\ngwlog\n",
            {"one.json": one + "\n", "two.json": two + "\n"},
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
        self.assertRegex(log, r"POST /mcp rpc=initialize id=2 session=no version=- mcp-method=- mcp-name=- mode=ready -> 200")
        self.assertRegex(log, r"POST /mcp rpc=server/discover id=4 session=no version=- mcp-method=- mcp-name=- mode=methodnotfound -> 200")

    def test_a_file_a_verb_changes_mid_run_is_replaced_whole(self):
        # FLAKE-MCPMODE's class: a file the run changes while something reads
        # it -- the binary's clock and settings (`reseed`, `rehomefile`), the
        # fake gateway's usage answers, second catalog and releases,
        # mcphttp's mode, the fake osascript's image --
        # is renamed into place, so a reader finds it old or new and never
        # half written. A descriptor opened on each before the verbs tells the
        # two apart: after a rename it still reads the old file; a file
        # rewritten in place shows it the new bytes.
        names = ("$HOME/.fxcap-usage-clock", "$HOME/../usage.json", "$HOME/../models2.json",
                 "$HOME/../gw-release", "$HOME/../mcphttp.mode", "$HOME/../clipimage",
                 "$HOME/.fx/settings.json", "$HOME/.fxcap-rehome")
        opened = " ".join('%d<"%s"' % (3 + i, n) for i, n in enumerate(names))
        paths = " ".join('"%s"' % n for n in names)
        # Each step waits for the shell's answer on the screen (`until`), not
        # for a moment on the clock: the descriptors are open before any
        # verb runs, and both reads are done before the snap, however late
        # the shell runs. The typed `printf 'x-%s'` never shows `x-all` itself.
        r = capture(
            "clock 1\nusage u1.json\nmcphttp ready\nwait 1\nmodels2 m1.json\nrelease\nclipimage i1.png\n"
            "reseed s1.json\nrehomefile .fxcap-rehome h1.json\n"
            "type exec " + opened + "; printf 'opened-%s\\n' all\nkey Enter\nuntil shown opened-all\n"
            "clock 2\nusage u2.json\nmodels2 m2.json\nrelease\nmcphttp fail\nclipimage i2.png\n"
            "reseed s2.json\nrehomefile .fxcap-rehome h2.json\n"
            "type for fd in 3 4 5 6 7 8 9 10; do printf 'fd%s=%s|' $fd \"$(tr '\\n' / <&$fd)\"; done; printf 'held-%s\\n' all\n"
            "key Enter\nuntil shown held-all\n"
            "type for f in " + paths + "; do printf 'now=%s|' \"$(tr '\\n' / < \"$f\")\"; done; printf 'now-%s\\n' all\n"
            "key Enter\nuntil shown now-all\nsnap a\n",
            {"u1.json": '{"u":1}\n', "u2.json": '{"u":2}\n', "m1.json": '{"m":1}\n', "m2.json": '{"m":2}\n',
             "i1.png": "i1\n", "i2.png": "i2\n", "s1.json": '{"s":1}\n', "s2.json": '{"s":2}\n',
             "h1.json": '{"h":1}\n', "h2.json": '{"h":2}\n'},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        a = r.screen("a")
        olds = ("1/", '{"u":1}/', '{"m":1}/', "release/", "ready/", "i1/", '{"s":1}/', '{"h":1}/')
        news = ("2/", '{"u":2}/', '{"m":2}/', "release/release/", "fail/", "i2/", '{"s":2}/', '{"h":2}/')
        for fd, (name, old, new) in enumerate(zip(names, olds, news), 3):
            with self.subTest(file=name):
                self.assertIn("now=%s|" % new, a, "the verb changed it")
                self.assertIn("fd%d=%s|" % (fd, old), a, "a reader that had it open still reads the old file whole")

    def test_held_waits_for_the_hold_and_release_lets_it_go(self):
        # The stub's curl prints each event as it arrives: two before the hold, the rest after.
        r = capture(
            "gateway g.json\nwait 0.5\n"
            "type curl -sN -d '{}' \"$FX_GATEWAY_CHAT_URL\" | grep --line-buffered -o 'delta\":\"[a-z]*\\|DONE'; echo ' rc:'$?\n"
            "key Enter\nheld\nwait 0.5\nsnap held\nrelease\nwait 1\nsnap after\ngwlog\n",
            {"g.json": json.dumps([{"chunks": ["aa", "bb", "cc", "dd"], "delayMs": 20, "holdAt": 2}])},
        )
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("[fakegw] hold 1 after 2 event(s)", r.stdout)
        held = r.file("held")
        self.assertIn('delta":"bb', held)
        self.assertNotIn('delta":"cc', held)
        self.assertNotIn(" rc:0", held, "the stream is still open")
        after = r.file("after")
        self.assertIn('delta":"dd', after)
        self.assertIn("DONE", after)
        self.assertIn(" rc:0", after)
        self.assertIn("release 1", r.file("gateway", ".log"))

    def test_held_fails_the_run_when_no_hold_comes(self):
        # holdAt past the turn's events: the gateway says so, and `held` fails on it.
        r = capture(
            "gateway g.json\nwait 0.5\ntype curl -sN -d '{}' \"$FX_GATEWAY_CHAT_URL\" > /dev/null\nkey Enter\nheld\nsnap a\n",
            {"g.json": json.dumps([{"chunks": ["aa"], "holdAt": 2}])},
        )
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertIn("held: the fake gateway made no hold 1 in 30 s", r.stderr)
        self.assertIn("holdAt 2 not held: this turn has 2 event(s): holdAt is 1 to 1", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        self.assertIn("guards:", r.stdout, "the guards still ran")

    # A row the stub's shell puts up for a while and takes away, as a binary's
    # timer does: `X[1]*Y`, printed a second after Enter and cleared 1.5 s
    # later. The typed line holds `X[%s]*Y`, never the row itself; and the
    # row has `[` and `*` in it, which a glob would not match as themselves.
    ROW_FOR_A_WHILE = "type sleep 1; printf 'X[%s]*Y\\n' 1; sleep 1.5; clear; printf 'after-%s\\n' it\nkey Enter\n"

    def test_until_waits_for_a_row_to_come_and_then_to_go(self):
        r = capture("wait 0.5\n" + self.ROW_FOR_A_WHILE + "until shown X[1]*Y\nsnap on\nuntil gone X[1]*Y\nsnap off\n")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("X[1]*Y", r.file("on").split("\n"))
        self.assertNotIn("X[1]*Y", r.file("off"))
        shown = re.search(r"until shown 'X\[1\]\*Y': after (\d+\.\d\d) s, (\d+\.\d\d) s since launch\n", r.stdout)
        gone = re.search(r"until gone 'X\[1\]\*Y': after (\d+\.\d\d) s, (\d+\.\d\d) s since launch\n", r.stdout)
        self.assertTrue(shown and gone, r.stdout)
        # Each waited for the screen, not for a second count: the row came
        # about a second after Enter and went about 1.5 s after that.
        self.assertGreater(float(shown.group(1)), 0.5)
        self.assertGreater(float(gone.group(1)), 1.0)
        self.assertGreater(float(gone.group(2)), float(shown.group(2)))

    def test_until_fails_the_run_when_the_row_never_comes(self):
        r = capture("wait 0.5\nuntil shown never-on-this-screen\nsnap a\n")
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertRegex(r.stderr, r"until shown 'never-on-this-screen': still not shown after (\d+\.\d\d) s \(\d+ looks\); the screen's rows that are not blank:")
        # Thirty seconds of elapsed time, however many looks that took.
        waited = float(re.search(r"still not shown after (\d+\.\d\d) s", r.stderr).group(1))
        self.assertGreaterEqual(waited, 29.0)
        self.assertLess(waited, 32.0)
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        self.assertIn("guards:", r.stdout, "the guards still ran")

    def test_until_fails_the_run_when_the_row_never_goes(self):
        r = capture("wait 0.5\ntype printf 'stays-%s\\n' put\nkey Enter\nuntil shown stays-put\nuntil gone stays-put\nsnap a\n")
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertRegex(r.stderr, r"until gone 'stays-put': still shown after \d+\.\d\d s \(\d+ looks\); the screen's rows that are not blank:")
        self.assertIn("  | stays-put", r.stderr, "the screen it last read")
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        self.assertIn("guards:", r.stdout, "the guards still ran")

    def test_until_gone_says_never_seen_when_its_text_was_not_there_at_all(self):
        # The row is up for 1.5 s: gone before the `until gone` first looks.
        # An `until shown` of this run saw it, so it came and went; the other
        # text no `until` ever saw.
        r = capture("wait 0.5\n" + self.ROW_FOR_A_WHILE + "until shown X[1]*Y\nwait 2.5\nuntil gone X[1]*Y\nuntil gone not-on-this-screen\nsnap a\n")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"until gone 'X\[1\]\*Y': after \d+\.\d\d s, \d+\.\d\d s since launch\n")
        self.assertRegex(r.stdout, r"until gone 'not-on-this-screen': after \d+\.\d\d s, \d+\.\d\d s since launch; never seen\n")

    def test_absent_passes_a_screen_without_its_text_and_fails_one_with_it(self):
        # The injected hint (FLAKE-HINT4 r2): a stub that puts up the Ctrl-C
        # exit hint where the scenario says none may be. `absent` fails the
        # run there, with the screen, and nothing after it is snapped.
        ok = capture("wait 0.5\nabsent press ctrl+c again to exit\nsnap a\n")
        self.assertEqual(ok.code, 0, ok.stdout + ok.stderr)
        self.assertRegex(ok.stdout, r"absent 'press ctrl\+c again to exit': not on the screen, \d+\.\d\d s since launch\n")
        r = capture("wait 0.5\ntype printf 'press ctrl+c %s to exit\\n' again\nkey Enter\n"
                    "until shown press ctrl+c again to exit\nabsent press ctrl+c again to exit\nsnap a\n")
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertIn("absent 'press ctrl+c again to exit': it is on the screen", r.stderr)
        self.assertIn("  | press ctrl+c again to exit", r.stderr, "the screen it read")
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        self.assertIn("guards:", r.stdout, "the guards still ran")

    def test_expect_checks_the_frame_a_snap_wrote(self):
        r = capture("wait 0.5\n" + self.ROW_FOR_A_WHILE + "until shown X[1]*Y\nsnap on\nexpect on X[1]*Y\nuntil gone X[1]*Y\nsnap off\nexpect off X[1]*Y\nsnap never\n")
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertIn("expect on 'X[1]*Y': the snap holds it", r.stdout)
        self.assertIn("expect off 'X[1]*Y': the snap does not hold it; its rows that are not blank:", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-never.txt")), "the run went on")
        self.assertIn("guards:", r.stdout, "the guards still ran")
        missing = capture("wait 0.5\nexpect nothing X\n")
        self.assertEqual(missing.code, 1, missing.stdout + missing.stderr)
        self.assertIn("expect nothing: no snap nothing was written before it", missing.stderr)

    def test_expect_on_one_side_only(self):
        # LAUNCH-SETTLE-F (opus r2 SHOULD 1): a divergence pair's snap says
        # each side's own row, so a side that flips under load fails the run
        # instead of the pair turning identical without a sound. A line for
        # one binary runs on that binary only (capture.sh `for_this`).
        cap = ("wait 0.5\ntype printf 'side-%s\\n' one\nkey Enter\nuntil shown side-one\nsnap a\n"
               "fx: expect a side-one\nxfx: expect a side-two\nsnap after\n")
        fx = capture(cap, which="fx")
        self.assertEqual(fx.code, 0, fx.stdout + fx.stderr)
        self.assertIn("expect a 'side-one': the snap holds it", fx.stdout)
        self.assertNotIn("side-two", fx.stdout + fx.stderr, "xfx's line did not run on fx")
        # The side whose row is not the one its line names: the run fails
        # there, with the frame, and takes no snap after.
        xfx = capture(cap, which="xfx")
        self.assertEqual(xfx.code, 1, xfx.stdout + xfx.stderr)
        self.assertIn("expect a 'side-two': the snap does not hold it; its rows that are not blank:", xfx.stderr)
        self.assertIn("  | side-one", xfx.stderr, "the frame it read")
        self.assertNotIn("expect a 'side-one'", xfx.stdout + xfx.stderr, "fx's line did not run on xfx")
        self.assertFalse(os.path.exists(os.path.join(xfx.out, "xfx-t-30x200-after.txt")), "the run went on")

    def test_expect_reads_only_a_frame_this_run_wrote(self):
        # opus r2 SHOULD 3: capture.sh never clears the out directory, so a
        # frame an earlier run left there holds whatever that run saw.
        stale = {"fx-t-30x200-a.txt": "a row an earlier run saw\n"}
        r = capture("wait 0.5\nexpect a an earlier run saw\nsnap after\n", stale=stale)
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertIn("expect a: no snap a was written before it in this run", r.stderr)
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-after.txt")), "the run went on")
        # The same label snapped in this run is read, over the stale frame;
        # and a `snapfull` label is `<label>.full`, as the sweep names it.
        ok = capture("wait 0.5\ntype printf 'fresh-%s\\n' row\nkey Enter\nuntil shown fresh-row\nsnap a\nexpect a fresh-row\n"
                     "snapfull b\nexpect b.full fresh-row\n", stale=stale)
        self.assertEqual(ok.code, 0, ok.stdout + ok.stderr)
        self.assertIn("expect a 'fresh-row': the snap holds it", ok.stdout)
        self.assertIn("expect b.full 'fresh-row': the snap holds it", ok.stdout)

    def test_never_seen_is_of_the_binary_since_its_last_launch(self):
        # opus r2 SHOULD 4: a relaunch starts the binary again on a cleared
        # screen, so what an `until` saw before it says nothing of what the
        # new one showed.
        r = capture("keep\nwait 0.5\ntype printf 'X%sY\\n' 1\nkey Enter\nuntil shown X1Y\n"
                    "relaunch\nwait 0.5\nuntil gone X1Y\nsnap a\n")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"until gone 'X1Y': after \d+\.\d\d s, \d+\.\d\d s since launch; never seen\n")

    def test_sockets(self):
        r = capture("env P=@PORT@\nwait 1\ntype exec 3<>/dev/tcp/127.0.0.1/$P\nkey Enter\nwait 0.5\nsockets s\n")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("1 socket(s)", r.stdout)
        self.assertRegex(r.file("s", ".sockets"), r"TCP 127\.0\.0\.1:\d+->127\.0\.0\.1:\d+ \(ESTABLISHED\)")


class LaunchWait(unittest.TestCase):
    """A scenario's first line, and the line after each `relaunch`, wait for
    the binary to be up (registry #19, 15th sighting: xfx's first frame 2.13 s
    after the launch of a freshly copied binary, and `interrupt`'s Ctrl-C at
    2 s ended it by SIGINT, `.prd/tui-phase5/receipts/flake-hint4/r2/
    launch-probe.txt`)."""

    # A binary that comes up late and then leaves ISIG, as fx and xfx do when
    # they go raw before their first frame: until then a Ctrl-C is SIGINT.
    SLOW = "#!/bin/sh\nsleep 1.5\nstty -isig\nprintf 'stub-is-%s\\n' up\nexec /bin/cat\n"

    def slow(self):
        d = scratch()
        script = os.path.join(d, "slow")
        write(script, self.SLOW)
        os.chmod(script, 0o755)
        return script

    def test_the_first_line_waits_for_the_first_frame(self):
        r = capture("keep\nkey C-c\nwait 0.5\nsnap a\n", binary=self.slow())
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertNotIn("Pane is dead", r.file("a"), "the Ctrl-C came before the binary was up")
        self.assertIn("stub-is-up", r.file("a").split("\n"))
        up = re.search(r"up: a first frame after (\d+\.\d\d) s \(\d+ looks\)\n", r.stdout)
        self.assertTrue(up, r.stdout)
        self.assertGreater(float(up.group(1)), 1.0)

    def test_a_relaunch_waits_for_its_first_frame_too(self):
        r = capture("keep\nwait 2\nrelaunch\nkey C-c\nwait 0.5\nsnap a\n", binary=self.slow())
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertNotIn("Pane is dead", r.file("a"), "the Ctrl-C came before the relaunched binary was up")
        self.assertIn("stub-is-up", r.file("a").split("\n"))
        self.assertEqual(len(re.findall(r"up: a first frame after \d+\.\d\d s \(\d+ looks\)\n", r.stdout)), 2, r.stdout)

    def test_since_launch_counts_from_the_first_frame(self):
        # The full-access warning's 4 s start at the first frame (xfx
        # src/tui/permissions.rs `observe`), and so does a scenario's clock.
        r = capture("until shown stub-is-up\nsnap a\n", binary=self.slow())
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        since = re.search(r"until shown 'stub-is-up': after \d+\.\d\d s, (\d+\.\d\d) s since launch\n", r.stdout)
        self.assertTrue(since, r.stdout)
        self.assertLess(float(since.group(1)), 1.0)

    def test_a_binary_that_never_draws_fails_the_run(self):
        r = capture("snap a\n", binary="/bin/cat")
        self.assertEqual(r.code, 1, r.stdout + r.stderr)
        self.assertRegex(r.stderr, r"up: nothing on the screen after \d+\.\d\d s \(\d+ looks\)")
        waited = float(re.search(r"up: nothing on the screen after (\d+\.\d\d) s", r.stderr).group(1))
        self.assertGreaterEqual(waited, 29.0)
        self.assertLess(waited, 32.0)
        self.assertFalse(os.path.exists(os.path.join(r.out, "fx-t-30x200-a.txt")))
        self.assertIn("guards:", r.stdout, "the guards still ran")

    def test_a_binary_gone_at_once_is_final_under_keep_and_fails_the_run_without(self):
        kept = capture("keep\nsnap a\n", binary="/bin/echo")
        self.assertEqual(kept.code, 0, kept.stdout + kept.stderr)
        self.assertRegex(kept.stdout, r"up: the pane is dead after \d+\.\d\d s \(\d+ looks\)\n")
        self.assertRegex(kept.file("a"), r"Pane is dead \(status 0, ")
        # `home` reads no pane: the run stops at the wait, not at a later
        # line that happens to need one.
        gone = capture("home h\n", binary="/bin/echo")
        self.assertEqual(gone.code, 1, gone.stdout + gone.stderr)
        self.assertIn("up: the pane is gone", gone.stderr)
        self.assertFalse(os.path.exists(os.path.join(gone.out, "fx-t-30x200-h.files")), "the run went on")
        self.assertIn("guards:", gone.stdout, "the guards still ran")

    def test_nodraw_runs_a_stub_that_draws_nothing_at_once(self):
        r = capture("nodraw\nkeep\nsnap a\nrelaunch\nsnap b\n", binary="/bin/cat")
        self.assertEqual(r.code, 0, r.stdout + r.stderr)
        self.assertIn("up: not waited for (nodraw)\n", r.stdout)
        self.assertNotIn("up: a first frame", r.stdout)
        self.assertEqual(r.file("b").strip(), "")


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

    def wait_for(self, text, timeout=10):
        """Until a line holding `text` is in the log: a request is logged
        when it is answered, which load can put off."""
        end = time.time() + timeout
        while text not in self.lines():
            if time.time() > end:
                raise AssertionError("no %r in %d s: %s" % (text, timeout, self.lines()))
            time.sleep(0.05)

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
        clean = {k: "" for k in ("FAKEGW_MODELS", "FAKEGW_MODELS_DOWN_MS", "FAKEGW_MODELS_DELAY_MS", "FAKEGW_MODELS2", "FAKEGW_CREDITS", "FAKEGW_TITLE", "FAKEGW_USAGE", "FAKEGW_RELEASE")}
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

    def open_completion(self, g):
        """A completion on a socket of its own, its bytes read as they come."""
        s = socket.create_connection(("127.0.0.1", g.port), timeout=5)
        self.addCleanup(s.close)
        body = json.dumps({"prompt": []}).encode()
        s.sendall(b"POST /v4/ai/language-model HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n"
                  b"Content-Length: %d\r\n\r\n" % len(body) + body)
        return s

    def read_until(self, s, needle, timeout=5):
        got, end = b"", time.time() + timeout
        while needle not in got:
            left = end - time.time()
            self.assertGreater(left, 0, got)
            if select.select([s], [], [], left)[0]:
                chunk = s.recv(65536)
                self.assertTrue(chunk, b"closed before %s: %s" % (needle, got))
                got += chunk
        return got

    def quiet(self, s, secs=0.6):
        """Whether nothing more arrives for `secs`."""
        return not select.select([s], [], [], secs)[0]

    def test_a_held_stream_stops_after_its_events_until_a_line_lets_it_go(self):
        release = os.path.join(scratch(), "release")
        g = self.gw([{"chunks": ["aa", "bb", "cc"], "holdAt": 2}, {"chunks": ["next", "last"], "holdAt": 1}], FAKEGW_RELEASE=release)
        s = self.open_completion(g)
        got = self.read_until(s, b'"delta":"bb"')
        self.assertTrue(self.quiet(s), "held: nothing after the second event")
        self.assertNotIn(b'"delta":"cc"', got)
        self.assertIn("hold 1 after 2 event(s)", g.lines())
        swap(release, "release\n")
        rest = self.read_until(s, b"[DONE]")
        self.assertIn(b'"delta":"cc"', rest)
        self.assertIn(b'"type":"finish"', rest)
        self.assertIn("release 1", g.lines())
        # The second hold is the second line's: the one already written lets nothing go.
        s = self.open_completion(g)
        self.read_until(s, b'"delta":"next"')
        self.assertTrue(self.quiet(s), "held: nothing after the first event")
        self.assertIn("hold 2 after 1 event(s)", g.lines())
        swap(release, slurp(release) + "release\n")
        self.assertIn(b'"delta":"last"', self.read_until(s, b"[DONE]"))
        self.assertIn("release 2", g.lines())

    def test_a_held_stream_whose_client_goes_stops_holding(self):
        g = self.gw([{"chunks": ["aa", "bb"], "holdAt": 1}], FAKEGW_RELEASE=os.path.join(scratch(), "release"))
        s = self.open_completion(g)
        self.read_until(s, b'"delta":"aa"')
        s.close()
        end = time.time() + 5
        while "hold 1: stream closed by client" not in g.lines():
            self.assertLess(time.time(), end, g.lines())
            time.sleep(0.05)
        time.sleep(0.3)
        self.assertNotIn("stream closed by client:", g.lines(), "nothing is written to it after the hold")

    def test_a_hold_a_stream_cannot_make_is_said(self):
        g = self.gw([{"chunks": ["aa"], "holdAt": 5}, {"chunks": ["bb"], "holdAt": 1}], FAKEGW_RELEASE=os.path.join(scratch(), "release"))
        status, _, body = self.completion(g)
        self.assertEqual(status, 200)
        self.assertIn(b"[DONE]", body)
        status, _, body = g.ask("POST", "/v1/messages", body="{}", headers={"content-type": "application/json"})
        self.assertEqual(status, 200)
        time.sleep(0.2)
        log = g.lines()
        self.assertIn("holdAt 5 not held: this turn has 2 event(s): holdAt is 1 to 1", log)
        self.assertIn("holdAt 1 not held: the llmux wire is not paced", log)
        self.assertNotIn("hold 1 after", log)

    def test_models2_swaps_the_catalog_with_the_times_logged(self):
        d = scratch()
        write(os.path.join(d, "one.json"), '{"data":[{"id":"fake/one"}]}')
        write(os.path.join(d, "two.json"), '{"data":[{"id":"fake/two"}]}')
        # FAKEGW_MODELS2 names a file not there yet: the first catalog until
        # it is put in place, the second after, whenever that is.
        second = os.path.join(d, "models2.json")
        g = self.gw(FAKEGW_MODELS=os.path.join(d, "one.json"), FAKEGW_MODELS2=second, FAKEGW_MODELS_DELAY_MS="0")
        self.assertIn(b"fake/one", g.ask("GET", "/coding-agent/v1/models")[2])
        self.assertIn(b"fake/one", g.ask("GET", "/coding-agent/v1/models")[2])
        os.rename(os.path.join(d, "two.json"), second)
        self.assertIn(b"fake/two", g.ask("GET", "/coding-agent/v1/models")[2])
        log = g.lines()
        self.assertEqual(log.count("-> second catalog"), 1)
        self.assertEqual(len(re.findall(r"GET /coding-agent/v1/models bearer=no at=\d+ms", log)), 3)

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

    def test_a_usage_file_or_second_catalog_caught_empty_is_read_again(self):
        # FLAKE-MCPMODE's class: fakegw reads the usage answers and the second
        # catalog on every request, and a file rewritten in place is empty for
        # a moment. Here each is empty when the request comes and for 400 ms
        # after; the answer is the file written then, never the empty one (a
        # usage read that fails to parse, a catalog with nothing in it).
        d = scratch()
        usage, second = os.path.join(d, "usage.json"), os.path.join(d, "models2.json")
        write(usage, "")
        write(second, "")
        g = self.gw(FAKEGW_USAGE=usage, FAKEGW_MODELS2=second, FAKEGW_MODELS_DELAY_MS="0")
        for path, file, text, want in (
            ("/v1/report?start_date=2026-10-12&end_date=2026-10-12", usage, '{"/v1/report":{"rows":[{"n":1}]}}',
             (200, b'{"results":[{"n":1}]}')),
            ("/coding-agent/v1/models", second, '{"data":[{"id":"fake/two"}]}', (200, b'{"data":[{"id":"fake/two"}]}')),
        ):
            with self.subTest(path=path):
                later = threading.Timer(0.4, swap, (file, text))
                later.start()
                self.addCleanup(later.cancel)
                status, _, body = g.ask("GET", path)
                later.join()
                self.assertEqual((status, body), want)

    def test_a_usage_file_that_never_parses_is_answered_500_at_the_deadline(self):
        # settled()'s re-read gives up (FLAKE-MCPMODE follow-up F): a usage
        # file that is not JSON is read again for 2 s (fakegw.ts SETTLE_MS),
        # then taken as it is and answered as base answered it, a 500 naming
        # the parse error -- not held for ever.
        d = scratch()
        usage = os.path.join(d, "usage.json")
        write(usage, '{"/v1/report":{"rows":[]}}')
        g = self.gw(FAKEGW_USAGE=usage)
        swap(usage, "not json")
        status, _, body = g.ask("GET", "/v1/report?start_date=2026-10-12&end_date=2026-10-12", timeout=4)
        self.assertEqual(status, 500)
        self.assertTrue(body.startswith(b"fakegw: SyntaxError"), body)
        end = time.time() + 5
        while "request GET /v1/report" not in g.lines():
            self.assertLess(time.time(), end, g.lines())
            time.sleep(0.05)
        m = re.search(r"request GET /v1/report\S* at=(\d+)ms -> 500 \(fakegw: SyntaxError[^\n]*\) at=(\d+)ms", g.lines())
        self.assertIsNotNone(m, g.lines())
        self.assertGreaterEqual(int(m.group(2)) - int(m.group(1)), 2000, "it read again until the deadline")

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
        swap(usage, json.dumps({"/v1/report": {"status": 429, "delayMs": 100}}))
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
        self.assertRegex(log, r"\+\d+ms POST /mcp rpc=tools/call id=5 session=yes version=2025-06-18 mcp-method=- mcp-name=- mode=ready -> 200 \+\d+ms")
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
        swap(self.mode, "ready\n")
        self.assertEqual(self.rpc(s, "ping", 2)[2], {"jsonrpc": "2.0", "id": 2, "result": {}})
        s.wait_for("rpc=ping id=1 ")
        self.assertRegex(s.lines(), r"rpc=ping id=1 session=no version=- mcp-method=- mcp-name=- mode=ready -> 200")

    def test_a_held_request_reads_past_a_mode_file_caught_empty(self):
        # FLAKE-MCPMODE (registry #19, PR #82's sweep job): the mode file was
        # rewritten in place, truncated and then written, and the held request
        # read it in between: `mode=unknown() -> 500`. Here the file stays
        # empty for 400 ms, so a request asking every 50 ms is sure to read
        # it so; it is still answered in the mode written after, and no
        # request is answered in a mode that is no mode.
        s = self.server("hang")
        with self.assertRaises(socket.timeout):
            s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"ping"}', timeout=0.8)
        open(self.mode, "w").close()
        time.sleep(0.4)
        swap(self.mode, "ready\n")
        s.wait_for("rpc=ping id=1 ")
        self.assertRegex(s.lines(), r"rpc=ping id=1 session=no version=- mcp-method=- mcp-name=- mode=ready -> 200")
        self.assertNotIn("unknown(", s.lines())

    # mcphttp.ts MODE_SETTLE_MS: a read that finds no mode is read again for
    # this long, then answered as no mode.
    SETTLE_MS = 2000

    def answered(self, s, rpc):
        """The server-side time a request took, from its log line: the
        moment it came and the moment it was answered."""
        s.wait_for("rpc=%s " % rpc)
        m = re.search(r"\+(\d+)ms POST /mcp rpc=%s .* -> \d+ \+(\d+)ms" % re.escape(rpc), s.lines())
        self.assertIsNotNone(m, s.lines())
        return int(m.group(2)) - int(m.group(1))

    def test_a_mode_file_that_never_holds_a_mode_is_answered_unknown_at_the_deadline(self):
        # The re-read gives up (FLAKE-MCPMODE follow-up F): a word that is no
        # mode stays the file's, and the request is answered as base answered
        # it, `unknown(bogus)` and 500, once the deadline passes -- not held
        # for ever.
        s = self.server("ready")
        swap(self.mode, "bogus\n")
        status, _, body = s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"ping"}', timeout=self.SETTLE_MS / 1000 + 2)
        self.assertEqual((status, body), (500, b"fixture failing"))
        self.assertGreaterEqual(self.answered(s, "ping id=1"), self.SETTLE_MS, "it read again until the deadline")
        self.assertRegex(s.lines(), r"rpc=ping id=1 .* mode=unknown\(bogus\) -> 500 \+\d+ms\n")

    def test_a_mode_file_that_cannot_be_read_is_answered_unknown_with_its_error(self):
        # The same deadline for a file that is gone: `unknown()` and 500 as
        # before, and the log line says why the read found no word.
        s = self.server("ready")
        os.remove(self.mode)
        status, _, body = s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"ping"}', timeout=self.SETTLE_MS / 1000 + 2)
        self.assertEqual((status, body), (500, b"fixture failing"))
        self.assertRegex(s.lines(), r"rpc=ping id=1 .* mode=unknown\(\) -> 500 \+\d+ms mode-file=ENOENT\n")
        swap(self.mode, "ready\n")
        self.assertEqual(self.rpc(s, "ping", 2)[0], 200)
        self.assertRegex(s.lines(), r"rpc=ping id=2 .* mode=ready -> 200 \+\d+ms\n", "no error once the file reads")

    def test_malformed_and_hostile_are_modes_and_answer_without_the_wait(self):
        # The two modes whose answers are garbage are still modes: read once,
        # answered at once, never held to the deadline. The bound sits just
        # under the deadline, not near the few ms an answer takes: a request
        # held to the deadline takes at least SETTLE_MS, so this still tells
        # the two apart, and a scheduling stall on a loaded runner (registry
        # #19's class) has 1.8 s of room instead of none.
        for mode in ("malformed", "hostile"):
            with self.subTest(mode=mode):
                s = self.server(mode)
                self.assertEqual(self.rpc(s, "server/discover", 1)[0], 200)
                self.assertLess(self.answered(s, "server/discover id=1"), self.SETTLE_MS - 200)
                self.assertRegex(s.lines(), r"rpc=server/discover id=1 .* mode=%s -> 200 \+\d+ms\n" % mode)

    def test_fail_and_rpcerror(self):
        s = self.server("fail")
        self.assertEqual(s.ask("POST", "/mcp", body='{"jsonrpc":"2.0","id":1,"method":"initialize"}')[0::2], (500, b"fixture failing"))
        swap(self.mode, "rpcerror\n")
        self.assertEqual(self.rpc(s, "initialize", 2)[2]["error"], {"code": -32603, "message": "fixture refuses"})
        self.assertEqual(self.rpc(s, "notifications/initialized")[0], 202)


    MODERN_META = {"progressToken": 2, "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                   "io.modelcontextprotocol/clientCapabilities": {}, "io.modelcontextprotocol/clientInfo": {"name": "t", "version": "1"}}

    def test_modern_lists_2026_and_holds_every_request_to_its_meta_and_header(self):
        s = self.server("modern")
        status, h, body = self.rpc(s, "server/discover", 1, {"_meta": dict(self.MODERN_META, progressToken=1)},
                                   {"mcp-protocol-version": "2026-07-28", "mcp-method": "server/discover"})
        self.assertEqual((status, body["result"]["supportedVersions"], body["result"]["resultType"]), (200, ["2026-07-28"], "complete"))
        self.assertIsNone(h.get("MCP-Session-Id") or h.get("mcp-session-id"))
        self.assertEqual(self.rpc(s, "initialize", 2, {"protocolVersion": "2025-11-25"})[2]["error"]["code"], -32601)
        self.assertEqual(self.rpc(s, "tools/list", 3, {"_meta": {"progressToken": 3}}, {"mcp-protocol-version": "2026-07-28"})[0::2],
                         (400, {"jsonrpc": "2.0", "id": 3, "error": {"code": -32602, "message": "Missing modern request metadata"}}))
        self.assertEqual(self.rpc(s, "tools/list", 4, {"_meta": self.MODERN_META})[0], 400, "no version header")
        status, _, body = self.rpc(s, "tools/list", 5, {"_meta": self.MODERN_META}, {"mcp-protocol-version": "2026-07-28", "mcp-method": "tools/list"})
        self.assertEqual((status, [t["name"] for t in body["result"]["tools"]]), (200, ["echo"]))
        call = self.rpc(s, "tools/call", 6, {"_meta": self.MODERN_META, "name": "echo", "arguments": {"text": "hi"}},
                        {"mcp-protocol-version": "2026-07-28", "mcp-method": "tools/call", "mcp-name": "echo"})[2]
        self.assertEqual(call["result"]["content"][0]["text"], "MCPHTTP_MODERN_ECHO: hi")
        self.assertRegex(s.lines(), r"rpc=tools/call id=6 session=no version=2026-07-28 mcp-method=tools/call mcp-name=echo mode=modern -> 200")

    def test_notimpl_answers_discover_404_with_the_rpc_error(self):
        s = self.server("notimpl")
        self.assertEqual(self.rpc(s, "server/discover", 1)[0::2],
                         (404, {"jsonrpc": "2.0", "id": 1, "error": {"code": -32601, "message": "Method not found"}}))
        self.assertEqual(self.rpc(s, "tools/list", 2, {"_meta": self.MODERN_META}, {"mcp-protocol-version": "2026-07-28"})[2]["result"]["tools"][0]["name"], "echo")

    def test_old2024_initializes_at_2024_11_05(self):
        s = self.server("old2024")
        self.assertEqual(self.rpc(s, "server/discover", 1)[0::2], (404, None))
        self.assertEqual(self.rpc(s, "initialize", 2, {"protocolVersion": "2025-11-25"})[2]["result"]["protocolVersion"], "2024-11-05")

    def test_malformed_and_hostile_answer_discover_and_then_serve_as_ready(self):
        s = self.server("malformed")
        status, _, body = self.rpc(s, "server/discover", 1)
        self.assertEqual((status, body["jsonrpc"], body["id"]), (200, "2.0", 1))
        self.assertIsInstance(body["result"], str)
        self.assertIn("\x1b]0;", body["result"])
        self.assertEqual(self.rpc(s, "initialize", 2, {"protocolVersion": "2025-11-25"})[2]["result"]["protocolVersion"], "2025-11-25")
        swap(self.mode, "hostile\n")
        result = self.rpc(s, "server/discover", 3)[2]["result"]
        self.assertEqual(result["resultType"], "complete")
        self.assertNotIn("2026-07-28", [v for v in result["supportedVersions"]])
        self.assertEqual(len(result["supportedVersions"][3]), 65536)
        self.assertIn("\x1b", result["instructions"])


    def test_the_sse_modes_answer_discover_on_an_event_stream(self):
        def events(mode):
            swap(self.mode, mode + "\n")
            msg = {"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}}
            c = http.client.HTTPConnection("127.0.0.1", s.port, timeout=5)
            try:
                c.request("POST", "/mcp", body=json.dumps(msg), headers={"content-type": "application/json"})
                r = c.getresponse()
                kind = r.getheader("content-type")
                # The first event, read as it comes: `ssehold` never ends.
                line = r.fp.readline()
                while not line.startswith(b"data: "):
                    line = r.fp.readline()
                return r.status, kind, json.loads(line[len(b"data: "):])
            finally:
                c.close()

        s = self.server("ssehold")
        status, kind, frame = events("ssehold")
        self.assertEqual((status, kind, frame["id"], frame["result"]["supportedVersions"]), (200, "text/event-stream", 1, ["2026-07-28"]))
        self.assertEqual(events("ssestrid")[2]["id"], "1")
        self.assertEqual(events("ssefail")[0::2][0], 500)
        self.assertEqual(events("ssefail")[2]["id"], 1)
        self.assertEqual(events("sseidless")[2], {"jsonrpc": "2.0", "id": None, "error": {"code": -32601, "message": "Method not found"}})
        # Anything but discover is the mode's plain server, and a mode that is
        # no SSE mode answers discover as before.
        self.assertEqual(self.rpc(s, "initialize", 2, {"protocolVersion": "2025-11-25"})[2]["result"]["protocolVersion"], "2025-11-25")
        swap(self.mode, "ready\n")
        self.assertEqual(self.rpc(s, "server/discover", 3)[0::2], (404, None))


class McpDiscoverStdio(unittest.TestCase):
    """scenarios/p5-mcp-discover-stdio.ts, the p5-mcp-discover scenarios' stdio server."""

    def start(self, mode):
        self.log = os.path.join(scratch(), "wire.ndjson")
        self.p = subprocess.Popen([BUN, DISCOVER_STDIO, mode], stdin=subprocess.PIPE, stdout=subprocess.PIPE, env=dict(os.environ, MCPSTDIO_WIRE_LOG=self.log))
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

    META = {"progressToken": 2, "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}, "io.modelcontextprotocol/clientInfo": {"name": "t", "version": "1"}}

    def test_modern_lists_2026_refuses_initialize_and_holds_requests_to_their_meta(self):
        self.start("modern")
        result = self.ask({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": dict(self.META, progressToken=1)}})["result"]
        self.assertEqual((result["supportedVersions"], result["resultType"]), (["2026-07-28"], "complete"))
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})["error"]["code"], -32601)
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"_meta": {"progressToken": 3}}})["error"],
                         {"code": -32602, "message": "Missing modern request metadata"})
        tools = self.ask({"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {"_meta": self.META}})["result"]
        self.assertEqual((tools["resultType"], tools["tools"][0]["name"]), ("complete", "echo"))
        call = self.ask({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"_meta": self.META, "name": "echo", "arguments": {"text": "hi"}}})
        self.assertEqual(call["result"]["content"][0]["text"], "DISCOVER_MODERN_RESULT:hi")
        with open(self.log) as f:
            self.assertEqual([json.loads(line)["sequence"] for line in f], [1, 2, 3, 4, 5])

    def test_silent_says_nothing_to_discover_then_initializes(self):
        self.start("silent")
        self.send({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}})
        init = self.ask({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})
        self.assertEqual((init["id"], init["result"]["protocolVersion"]), (2, "2025-06-18"))

    def test_old2024_refuses_discover_and_initializes_at_2024_11_05(self):
        self.start("old2024")
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 1, "method": "server/discover"})["error"]["code"], -32601)
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})["result"]["protocolVersion"], "2024-11-05")

    def test_malformed_and_hostile_answers(self):
        self.start("malformed")
        reply = self.ask({"jsonrpc": "2.0", "id": 1, "method": "server/discover"})
        self.assertIsInstance(reply["result"], str)
        self.assertIn("\x1b", reply["result"])
        self.assertEqual(self.ask({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})["result"]["protocolVersion"], "2025-06-18")
        self.stop()
        self.start("hostile")
        result = self.ask({"jsonrpc": "2.0", "id": 1, "method": "server/discover"})["result"]
        self.assertNotIn("2026-07-28", result["supportedVersions"])
        self.assertIn("\x1b", result["instructions"])

    def test_banner_asks_escaped_doubled_and_deep(self):
        self.start("banner")
        self.send({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}})
        self.assertEqual(self.p.stdout.readline(), b"Server started on stdio\n")
        self.assertEqual(json.loads(self.p.stdout.readline())["result"]["supportedVersions"], ["2026-07-28"])
        self.stop()
        self.start("asks")
        self.send({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}})
        self.assertEqual(json.loads(self.p.stdout.readline()), {"jsonrpc": "2.0", "id": "q1", "method": "ping"})
        self.assertEqual(json.loads(self.p.stdout.readline())["error"]["code"], -32601)
        self.send({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}})
        self.assertEqual(json.loads(self.p.stdout.readline()), {"jsonrpc": "2.0", "id": "q2", "method": "ping"})
        self.assertEqual(json.loads(self.p.stdout.readline())["result"]["protocolVersion"], "2025-06-18")
        self.stop()
        for mode, raw in (
            ("escaped", b'"supportedVersions":["2026\\u002d07-28","2025\\u002d11-25"]'),
            ("doubled", b'"supportedVersions":["2026-07-28"],"supportedVersions":[]'),
            ("deep:3", b'"supportedVersions":[[[]]]'),
        ):
            self.start(mode)
            self.send({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {}})
            self.assertIn(raw, self.p.stdout.readline(), mode)
            self.stop()

    def test_an_unknown_mode_is_refused(self):
        p = subprocess.run([BUN, DISCOVER_STDIO, "bogus"], stdin=subprocess.DEVNULL, capture_output=True, timeout=30)
        self.assertEqual(p.returncode, 2)


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
