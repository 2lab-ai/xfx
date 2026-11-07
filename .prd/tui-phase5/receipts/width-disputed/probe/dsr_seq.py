"""tmux's advance for each line of space-separated hex code points, printed
between `[` and `]` (a cursor-position report): scripts/drawn-wider/dsr_probe.py
for clusters. Run inside a pane of a private tmux server."""
import os, sys, termios, tty
in_path, out_path = sys.argv[1], sys.argv[2]
fd = sys.stdin.fileno(); saved = termios.tcgetattr(fd); tty.setraw(fd); results = []
try:
    for line in open(in_path):
        line = line.strip()
        if not line: continue
        text = "".join(chr(int(c, 16)) for c in line.split())
        os.write(1, ("\r\x1b[2K[" + text + "]\x1b[6n").encode())
        reply = b""
        while not reply.endswith(b"R"): reply += os.read(fd, 1)
        results.append("%s|%d" % (line, int(reply.split(b";")[-1][:-1]) - 3))
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, saved)
open(out_path, "w").write("\n".join(results) + "\n")
