"""Log raw key bytes a terminal sends after xfx's own key-mode negotiation.

Emits exactly the key-related prefix of xfx's MODE_SET (src/tui/term.rs:56):
modifyOtherKeys 2 and the kitty progressive-enhancement push of flag 1, then
queries the kitty flags (CSI ? u) so the log records what the terminal agreed
to. Raw mode mirrors term.rs raw_from. Every read is appended to the log as a
hex record with a monotonic timestamp. Pops both modes and restores on exit.
"""

import json
import os
import select
import sys
import termios
import time

log_path = sys.argv[1]
seconds = float(sys.argv[2])
fd = sys.stdin.fileno()
saved = termios.tcgetattr(fd)
raw = termios.tcgetattr(fd)
raw[0] &= ~(termios.BRKINT | termios.ICRNL | termios.INPCK | termios.ISTRIP | termios.IXON | termios.IXOFF)
raw[2] |= termios.CS8
raw[3] &= ~(termios.ECHO | termios.ICANON | termios.IEXTEN | termios.ISIG)
raw[6][termios.VMIN] = 1
raw[6][termios.VTIME] = 0
out = sys.stdout.fileno()
with open(log_path, "w", encoding="utf-8") as log:
    termios.tcsetattr(fd, termios.TCSAFLUSH, raw)
    try:
        os.write(out, b"\x1b[>4;2m\x1b[>1u\x1b[?u")
        os.write(out, b"KEYLOG-READY\r\n")
        log.write(json.dumps({"event": "ready", "t": time.monotonic()}) + "\n")
        log.flush()
        deadline = time.monotonic() + seconds
        while True:
            left = deadline - time.monotonic()
            if left <= 0:
                break
            ready, _, _ = select.select([fd], [], [], left)
            if not ready:
                continue
            data = os.read(fd, 4096)
            if not data:
                break
            log.write(json.dumps({"event": "read", "t": time.monotonic(), "hex": data.hex()}) + "\n")
            log.flush()
    finally:
        os.write(out, b"\x1b[<u\x1b[>4m")
        termios.tcsetattr(fd, termios.TCSAFLUSH, saved)
        log.write(json.dumps({"event": "done", "t": time.monotonic()}) + "\n")
