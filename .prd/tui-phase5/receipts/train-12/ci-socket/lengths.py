"""Byte counts behind the receipt, no path printed.

usage: python3 -I lengths.py <repo> <runner-length tmpdir> <gate tmpdir> <red-one-long.raw> <CI sweep log>"""
import os
import re
import sys
import tempfile

repo, long, short, red, ci = sys.argv[1:6]
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import test_capture as t  # noqa: E402  (the code under test: its constants and helpers)

def n(p):
    return len(os.fsencode(p))

print("socket limit (test_capture.SOCKET_MAX): %d bytes; tail %r: %d bytes" % (t.SOCKET_MAX, t.SOCKET_TAIL, n(t.SOCKET_TAIL)))
m = re.search(r"error connecting to (\S+?)(fxcapt\.\S+) \(File name too long\)", open(ci, encoding="utf-8-sig").read())
print("CI run (PR #90): runner TMPDIR %d bytes; socket path tmux refused %d bytes" % (n(m.group(1)), n(m.group(1) + m.group(2))))
for name, tmp in (("runner-length", long), ("gate", short)):
    os.environ["TMPDIR"] = tmp
    tempfile.tempdir = None
    d = t.scratch()
    room = t.headroom(d)
    print("%s TMPDIR: %d bytes given, %d resolved; scratch dir %d bytes; headroom %d; extras %s; socket at each %s"
          % (name, n(tmp), n(os.path.realpath(tmp) + ("/" if tmp.endswith("/") else "")), n(d), room,
             (0, room // 2, room), [n(d) + e + n(t.SOCKET_TAIL) for e in (0, room // 2, room)]))
    print("  old fixed extras (0, 9, 26): socket at each %s" % [n(d) + e + n(t.SOCKET_TAIL) for e in (0, 9, 26)])
m = re.search(r"error connecting to (\S+) \(File name too long\)", open(red).read())
print("RED socket path tmux refused: %s" % ("%d bytes" % n(m.group(1)) if m else "no such line"))
