"""Drive one xfx TUI session in a pty: type each line, wait, then /quit.

usage: drive_tui.py <xfx> <home> <work> <line>...

Bounded: the whole run is killed after 40 s (alarm), the child's process
group after the quit. Output goes to <home>/../tui.out, never a browser,
clipboard or speaker.
"""
import fcntl
import os
import pty
import signal
import struct
import sys
import termios
import time

xfx, home, work, *lines = sys.argv[1:]
signal.alarm(40)
pid, fd = pty.fork()
if pid == 0:
    os.chdir(work)
    env = {
        "PATH": "/usr/bin:/bin",
        "HOME": home,
        "TMPDIR": "<tmp>",
        "TERM": "xterm-256color",
        "XFX_TUI": "1",
        "XFX_THEME": "dark",
    }
    os.execve(xfx, [xfx], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
out = bytearray()


def pump(seconds):
    end = time.time() + seconds
    while time.time() < end:
        try:
            import select

            ready, _, _ = select.select([fd], [], [], 0.1)
            if ready:
                chunk = os.read(fd, 65536)
                if not chunk:
                    return
                out.extend(chunk)
                # Answer a cursor-position query so the TUI does not wait.
                if b"\x1b[6n" in chunk:
                    os.write(fd, b"\x1b[1;1R")
        except OSError:
            return


pump(3)
for line in lines:
    if line == "!rmprofile":
        # The profile home taken away under a running session.
        import shutil

        shutil.rmtree(os.path.join(home, ".xfx"))
        continue
    os.write(fd, line.encode() + b"\r")
    pump(3)
os.write(fd, b"/quit\r")
pump(3)
done, status = os.waitpid(pid, os.WNOHANG)
if done == 0:
    # Still running after /quit: this script's own forked child, nothing else.
    os.kill(pid, signal.SIGTERM)
    _, status = os.waitpid(pid, 0)
with open(os.path.join(os.path.dirname(home), "tui.out"), "wb") as f:
    f.write(bytes(out))
print("child status", status)
