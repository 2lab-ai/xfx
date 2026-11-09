"""What of one paste with an embedded end marker reaches xfx's decoder as keys?

A real xfx (`XFX_TUI=1`, an empty HOME, a fake key, a loopback fake gateway
this script runs) is sent one paste:

    ESC[200~ early-paste-text ESC[201~ K0000000 ... K0000001 ... CR ESC[201~

-- a clipboard holding `early-paste-text ESC[201~ K… CR` on a terminal that
does not strip the embedded marker (`payload`). If its CR is read as a key,
the paste submits a prompt by itself (`paste_submitted`); if not, once xfx is
quiet one Enter of the driver's own is sent. The fake gateway keeps the
prompt: `early-paste-text` is the paste, and every `K…` token after it is a
byte the decoder read **as a typed key**. The first such token says where the
delivery that carried the marker ended; the bytes between the marker and it
were dropped as input behind the paste.

Modes (round 2 adds `double`):
  pty        xfx on a raw pty; the paste is one blocking write to the master,
             as a terminal writes it.
  tmux       xfx in a pane of a private tmux server (`tmux -L paste-gap-<pid>
             -f /dev/null`); the paste is `load-buffer` + `paste-buffer -p`
             of the text between the outer markers, so tmux adds the outer
             ones -- tmux's own paste path.
  double     xfx on a raw pty, two benign pastes: `early-paste-text`, then at
             once a second one, `second-` and -- after a pause of <size> ms,
             the size argument read as milliseconds -- `paste CR /version CR`
             and its end marker. Its second part reaching the keys sends
             `paste` by itself (`paste_submitted`); kept, the prompt the
             driver's own Enter sends holds both pastes (`prompt`). The
             second paste's start marker is in the first one's delivery.
  late       `double`, but the first paste is written on its own and the
             second 10 ms later: its start marker comes after the first one
             has settled, inside its quarantine.
  outer      the same pane, attached by a tmux client on a pty this script
             is the terminal of; the paste, outer markers included, is
             written to that client's input -- a terminal that does not strip
             the marker, in front of tmux.

Bounded: fixed sizes, a deadline per step, every process it starts is killed
by its own PID or its own tmux socket, and every file is under a fresh
directory in $TMPDIR. No clipboard, browser or sound: a shim directory ahead
of PATH answers open/pbcopy/pbpaste/afplay/osascript with no-ops.

Run: python3 -I -B paste_xfx.py <xfx-binary> <mode> <size>... (output: one line per run)
"""
import fcntl, http.server, json, os, re, shlex, shutil, struct
import subprocess, sys, tempfile, termios, threading, time

ROWS, COLS = 40, 120
START, END = b"\x1b[200~", b"\x1b[201~"
HEAD = b"early-paste-text"
WARNING = "Input that arrived right after the paste was ignored."


FILL = b"\xff"


def payload(size):
    """`size` bytes in all, outer markers included, ending in a CR.

    Behind the embedded marker: a token `K<7 digits>` every `spacing` bytes,
    the rest 0xff -- a byte no UTF-8 scalar starts with, which xfx's decoder
    drops without an event (`input.rs` `ground`, its `_ => {}` arm) -- so the
    tokens that reach the composer say where the delivery ended without the
    composer having to take every byte of a megabyte. The CR is the clipboard's
    newline: if it is read as a key, the paste submits a prompt by itself.
    """
    room = size - len(START) - len(HEAD) - len(END) - 1 - len(END)
    spacing = max(32, size // 256)
    body = bytearray()
    n = 0
    while len(body) < room:
        chunk = (b"K%07d" % n) + FILL * (spacing - 8)
        body += chunk
        n += 1
    body = bytes(body[:room])
    return START + HEAD + END + body + b"\r" + END, body


class Gateway:
    """Loopback: keeps every POST body, answers 500; GETs are 404."""

    def __init__(self):
        self.bodies = []
        gateway = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                length = int(self.headers.get("content-length", 0))
                gateway.bodies.append(self.rfile.read(length))
                self.send_response(500)
                self.send_header("content-type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"error":{"message":"fake gateway"}}')

            def do_GET(self):
                self.send_response(404)
                self.end_headers()

            def log_message(self, *_):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()


def turn_prompt(bodies):
    """The last user message of the first turn request among `bodies` -- not
    the session-title request, which a first prompt also sends."""
    for body in bodies:
        sent = json.loads(body)
        prompt = sent.get("prompt", [])
        system = prompt[0].get("content", "") if prompt else ""
        if isinstance(system, str) and system.startswith("Generate a short title"):
            continue
        users = [m for m in prompt if m.get("role") == "user"]
        if users:
            return "".join(p.get("text", "") for p in (users[-1].get("content") or [])
                           if isinstance(p, dict))
    return None


def spawn_on_pty(argv, env, cwd):
    """`argv` on a new pty, in a session of its own that does not take the
    pty as its controlling terminal -- the TUI suite's
    `spawn_without_taking_the_terminal`; (process, master)."""
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, env=env,
                            cwd=cwd, start_new_session=True)
    os.close(slave)
    return proc, master


class Drain:
    """Reads a master until it closes, so the program behind it never blocks
    on a full output queue; remembers the output and when it last moved."""

    def __init__(self, master):
        self.master, self.out, self.last = master, bytearray(), time.monotonic()
        self.lock = threading.Lock()
        threading.Thread(target=self.run, daemon=True).start()

    def run(self):
        while True:
            try:
                chunk = os.read(self.master, 65536)
            except OSError:
                return
            if not chunk:
                return
            with self.lock:
                self.out += chunk
                self.last = time.monotonic()

    def text(self):
        with self.lock:
            return bytes(self.out)


def write_all(fd, data, deadline):
    view = memoryview(data)
    while view:
        if time.monotonic() > deadline:
            raise TimeoutError("the terminal stopped taking the paste")
        view = view[os.write(fd, view):]


def wait(what, check, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if check():
            return True
        time.sleep(0.05)
    raise TimeoutError(what)


def kill(proc):
    """Only a process this script started, by its own handle."""
    for stop in (proc.terminate, proc.kill):
        if proc.poll() is not None:
            return
        stop()
        try:
            proc.wait(2)
            return
        except subprocess.TimeoutExpired:
            pass


def run(binary, mode, size):
    run_dir = tempfile.mkdtemp(prefix="pg-")
    for sub in ("home", "work", "tmp", "shim"):
        os.mkdir(os.path.join(run_dir, sub), 0o700)
    for tool in ("open", "pbcopy", "pbpaste", "afplay", "osascript"):
        path = os.path.join(run_dir, "shim", tool)
        with open(path, "w") as f:
            f.write("#!/bin/sh\ncat >/dev/null 2>&1 </dev/null\nexit 0\n")
        os.chmod(path, 0o700)
    gateway = Gateway()
    env = {
        "HOME": os.path.join(run_dir, "home"),
        "TERM": "xterm-256color",
        "PATH": os.path.join(run_dir, "shim") + ":/usr/bin:/bin",
        "LANG": "en_US.UTF-8",
        "TMPDIR": os.path.join(run_dir, "tmp"),
        "AI_GATEWAY_API_KEY": "fake-key-not-real",
        "XFX_GATEWAY_URL": f"http://127.0.0.1:{gateway.port}/v4/ai/language-model",
        "XFX_TUI": "1",
    }
    data, body = payload(size)
    sock = f"paste-gap-{os.getpid()}"
    tmux = ["tmux", "-L", sock, "-f", "/dev/null"]
    pids, result = [], {}
    started = time.monotonic()
    try:
        if mode in ("pty", "double", "late"):
            proc, master = spawn_on_pty([binary], env, os.path.join(run_dir, "work"))
            pids.append(proc)
            screen = Drain(master)
            wait("xfx took the terminal", lambda: b"\x1b[?2004h" in screen.text(), 30)
            time.sleep(1.0)
            if mode == "late":
                write_all(master, START + HEAD + END, time.monotonic() + 10)
                time.sleep(0.01)
                write_all(master, START + b"second-", time.monotonic() + 10)
                time.sleep(size / 1000)
                write_all(master, b"paste\r/version\r" + END, time.monotonic() + 10)
            elif mode == "double":
                write_all(master, START + HEAD + END + START + b"second-", time.monotonic() + 10)
                time.sleep(size / 1000)
                write_all(master, b"paste\r/version\r" + END, time.monotonic() + 10)
            else:
                write_all(master, data, time.monotonic() + 120)
            quiet = lambda: time.monotonic() - screen.last > 1.0
            wait("xfx went quiet", quiet, 300)
            enter = lambda: write_all(master, b"\r", time.monotonic() + 10)
            shown = lambda: screen.text().decode("utf-8", "replace")
        else:
            launch = "cd %s && exec env -i %s %s" % (
                shlex.quote(os.path.join(run_dir, "work")),
                " ".join(shlex.quote(f"{k}={v}") for k, v in env.items()),
                shlex.quote(binary),
            )
            subprocess.run(tmux + ["new-session", "-d", "-s", "m", "-x", str(COLS),
                                   "-y", str(ROWS), launch], check=True, env=clean_env())
            pane = lambda: subprocess.run(tmux + ["capture-pane", "-p", "-t", "m"],
                                          capture_output=True, text=True, env=clean_env()).stdout
            wait("xfx is up in the pane", lambda: "┃" in pane(), 30)
            time.sleep(1.0)
            if mode == "tmux":
                buf = os.path.join(run_dir, "paste")
                with open(buf, "wb") as f:
                    f.write(data[len(START):-len(END)])
                subprocess.run(tmux + ["load-buffer", "-b", "pg", buf], check=True, env=clean_env())
                subprocess.run(tmux + ["paste-buffer", "-p", "-r", "-b", "pg", "-t", "m"],
                               check=True, env=clean_env())
            else:
                env_client = clean_env()
                env_client["TERM"] = "xterm-256color"
                proc, master = spawn_on_pty([shutil.which("tmux")] + tmux[1:] + ["attach", "-t", "m"],
                                            env_client, run_dir)
                pids.append(proc)
                client = Drain(master)
                wait("the client drew the pane", lambda: "┃".encode() in client.text(), 30)
                time.sleep(1.0)
                write_all(master, data, time.monotonic() + 120)
            stable = {"last": None, "since": time.monotonic()}

            def settled():
                now = pane()
                if now != stable["last"]:
                    stable["last"], stable["since"] = now, time.monotonic()
                return time.monotonic() - stable["since"] > 1.5

            wait("the pane went quiet", settled, 300)
            enter = lambda: subprocess.run(tmux + ["send-keys", "-t", "m", "Enter"],
                                           check=True, env=clean_env())
            shown = pane
        # The paste's own CR, read as a key, has submitted a prompt by now.
        result["paste_submitted"] = turn_prompt(gateway.bodies) is not None
        if not result["paste_submitted"]:
            enter()
            wait("the Enter submitted a prompt",
                 lambda: turn_prompt(gateway.bodies) is not None, 60)
        text = turn_prompt(gateway.bodies)
        result["head"] = text.startswith(HEAD.decode())
        typed = text[len(HEAD):] if result["head"] else text
        tokens = re.findall(r"K(\d{7})", typed)
        result["typed_bytes"] = len(typed)
        if tokens:
            first = int(tokens[0])
            offset = body.find(b"K%07d" % first)
            result["dropped_before_keys"] = offset
            result["first_key_token"] = "K%07d" % first
        else:
            result["dropped_before_keys"] = len(body) if result["head"] else None
            result["first_key_token"] = None
        time.sleep(0.3)
        result["warning"] = WARNING in shown()
        result["other"] = re.sub(r"K\d{7} ?", "", typed)[:60]
        if mode in ("double", "late"):
            result["prompt"] = text[:80]
    except TimeoutError as err:
        result["timeout"] = str(err)
        # PASTE_XFX_DUMP names a file for the screen at the timeout.
        if os.environ.get("PASTE_XFX_DUMP") and "shown" in dir():
            with open(os.environ["PASTE_XFX_DUMP"], "w") as f:
                f.write(shown())
    finally:
        result["seconds"] = round(time.monotonic() - started, 1)
        if mode not in ("pty", "double", "late"):
            subprocess.run(tmux + ["kill-server"], capture_output=True, env=clean_env())
            # tmux 3.7b leaves the socket file of a server it killed; this
            # run's own, by its own name, is removed too.
            socket_dir = os.path.join(os.environ.get("TMUX_TMPDIR", "/tmp"), f"tmux-{os.getuid()}")
            try:
                os.unlink(os.path.join(socket_dir, sock))
            except FileNotFoundError:
                pass
        for proc in pids:
            kill(proc)
        gateway.close()
        shutil.rmtree(run_dir, ignore_errors=True)
    return result


def clean_env():
    keep = {k: v for k, v in os.environ.items() if k in ("PATH", "HOME", "LANG", "TMPDIR", "USER", "LOGNAME")}
    keep.pop("TMUX", None)
    return keep


def main():
    binary, mode, sizes = sys.argv[1], sys.argv[2], [int(s) for s in sys.argv[3:]]
    assert mode in ("pty", "tmux", "outer", "double", "late")
    for size in sizes:
        r = run(binary, mode, size)
        print(json.dumps({"mode": mode, "size": size, **r}), flush=True)


main()
