"""Replays a submit scenario's terminal bytes into a fresh tmux pane and keeps
the screen each frame after Enter leaves -- what a snap cannot catch, because
a frame lasts a few milliseconds.

usage: python3 -I -B frames.py <receipt-dir> <socket-dir>

For each side (`fx/`, `xfx/`, `red/`) and each scenario holding a
`before-submit.wire` and an `after-submit.wire` (the bytes the binary had
written before Enter, and the same bytes and more, 3 s after it): the frames
after Enter are the synchronized updates (`ESC[?2026h` ... `ESC[?2026l`)
past the before-wire's end. For k = 1 and 2, every byte up to the end of frame
k is written into a new 40x120 pane of a tmux server of its own (`-f
/dev/null`, a socket in <socket-dir>, never the user's), by `cat` of a file,
and the pane is captured as `capture.sh` captures a snap (`capture-pane -p
-e`), to `<prefix>-<unit>-frame<k>.ansi`. The whole after-wire is replayed too,
to `<prefix>-<unit>-wireend.ansi`: it must be the `second` snap taken just
before that wire was copied, which is what says the replay is the capture's
terminal.

The pane's terminal does not echo (`stty -echo`): a wire holds the binary's
own queries (a cursor-position report, `ESC[6n`), tmux answers them into the
pane's input as it would have to the binary, and nothing reads them here --
echoed, they would be written onto the screen being kept.

The server is killed by its socket when each replay is read; nothing else is
started.
"""
import glob
import os
import subprocess
import sys
import time

receipt, sockdir = (os.path.realpath(p) for p in sys.argv[1:3])
START, END = b"\x1b[?2026h", b"\x1b[?2026l"


def frame_ends(before, after):
    assert after.startswith(before), "the before-wire is not where the after-wire starts"
    ends, pos = [], len(before)
    while True:
        s = after.find(START, pos)
        if s < 0:
            return ends
        e = after.index(END, s) + len(END)
        ends.append(e)
        pos = e


def replay(data, out):
    feed = os.path.join(sockdir, "feed.bin")
    with open(feed, "wb") as f:
        f.write(data)
    sock = os.path.join(sockdir, "frames.sock")
    tm = ["tmux", "-f", "/dev/null", "-S", sock]
    subprocess.run(tm + ["new-session", "-d", "-x", "120", "-y", "40", "-s", "r", "stty -echo; cat '%s'; exec sleep 30" % feed], check=True)
    try:
        # The bytes are all in the file: wait until the pane has drawn them.
        last, same = None, 0
        for _ in range(50):
            time.sleep(0.1)
            shot = subprocess.run(tm + ["capture-pane", "-p", "-e", "-t", "r"], check=True, capture_output=True).stdout
            same = same + 1 if shot == last else 0
            last = shot
            if same >= 3:
                break
        with open(out, "wb") as f:
            f.write(last)
    finally:
        subprocess.run(tm + ["kill-server"], check=False)


for before_path in sorted(glob.glob(os.path.join(receipt, "*", "*", "*-before-submit.wire"))):
    after_path = before_path.replace("-before-submit.wire", "-after-submit.wire")
    before = open(before_path, "rb").read()
    after = open(after_path, "rb").read()
    ends = frame_ends(before, after)
    stem = before_path[: -len("-before-submit.wire")]
    for k in (1, 2):
        replay(after[: ends[k - 1]], "%s-frame%d.ansi" % (stem, k))
    replay(after, stem + "-wireend.ansi")
    print("%s: %d frames after Enter; frames 1, 2 and the whole wire replayed"
          % (os.path.relpath(stem, receipt), len(ends)))
