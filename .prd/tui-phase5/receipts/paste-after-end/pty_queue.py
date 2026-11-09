"""How much a raw-mode pty takes before its writer must wait: write to the master,
non-blocking, until EAGAIN. Run: python3 -I -B pty_queue.py (output: pty-queue.txt)."""
import os, pty, tty, fcntl, errno
m, s = pty.openpty()
tty.setraw(s)
fl = fcntl.fcntl(m, fcntl.F_GETFL); fcntl.fcntl(m, fcntl.F_SETFL, fl | os.O_NONBLOCK)
total = 0
chunk = b"w" * 64
while True:
    try:
        n = os.write(m, chunk)
        total += n
        if n < len(chunk): break
    except OSError as e:
        if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK): break
        raise
    if total > 1 << 20: break
print("raw-mode slave input queue accepted", total, "bytes before EAGAIN")
# one write of 5000 bytes on a fresh pty: how much does a single write take?
m2, s2 = pty.openpty(); tty.setraw(s2)
fcntl.fcntl(m2, fcntl.F_SETFL, fcntl.fcntl(m2, fcntl.F_GETFL) | os.O_NONBLOCK)
try:
    print("one 5000-byte write took", os.write(m2, b"x" * 5000))
except OSError as e:
    print("one 5000-byte write:", e)
