"""How many cells the terminal this runs in advances for each scalar.

Run inside a tmux pane (scripts/drawn-wider/sweep.sh starts one on a private
server): for each hexadecimal code point in <in>, print `[` scalar `]` at the
first column, ask the terminal where the cursor is (a cursor-position report)
and write `hex cells` to <out>, the two markers taken off.
"""
import os
import sys
import termios
import tty

in_path, out_path = sys.argv[1], sys.argv[2]
fd = sys.stdin.fileno()
saved = termios.tcgetattr(fd)
tty.setraw(fd)
results = []
try:
    for line in open(in_path):
        code = line.strip()
        if not code:
            continue
        os.write(1, ("\r\x1b[2K[" + chr(int(code, 16)) + "]\x1b[6n").encode())
        reply = b""
        while not reply.endswith(b"R"):
            reply += os.read(fd, 1)
        column = int(reply.split(b";")[-1][:-1])
        results.append(f"{code} {column - 3}")
finally:
    termios.tcsetattr(fd, termios.TCSADRAIN, saved)
with open(out_path, "w") as out:
    out.write("\n".join(results) + "\n")
