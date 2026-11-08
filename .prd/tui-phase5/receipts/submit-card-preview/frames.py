"""Replays the terminal bytes kept on both sides of an Enter into a fresh tmux
pane and keeps the screen each frame after Enter leaves -- what a snap cannot
catch, because a frame lasts a few milliseconds.

usage: python3 -I -B frames.py <capture-dir> <socket-dir> [frames]

For every `<stem>-<pair>-before.wire` in <capture-dir> (searched recursively)
with its `<stem>-<pair>-after.wire`, and every `<stem>-before-submit.wire`
with its `<stem>-after-submit.wire` (the bytes the binary had written before
Enter, and the same bytes and more after it): the frames after Enter are the
synchronized updates (`ESC[?2026h` ... `ESC[?2026l`) past the before-wire's
end. For k = 0 (the before-wire's end) up to <frames> (default 3), every byte
up to the end of frame k is written into a new 40x120 pane of a tmux server of
its own (`-f /dev/null`, a socket in <socket-dir>, never the user's), by `cat`
of a file, and the pane is captured as `capture.sh` captures a snap
(`capture-pane -p -e`), to `<stem>-<pair>-frame<k>.ansi`; the whole after-wire
is replayed to `<stem>-<pair>-wireend.ansi`. Each frame's own bytes go to
`<stem>-<pair>-frame<k>.bytes`. The pane's terminal does not echo (`stty
-echo`): a wire holds the binary's own queries, and tmux's answers to them
would otherwise be written onto the screen being kept. The server is killed by
its socket when each replay is read; nothing else is started.
"""
import glob
import os
import subprocess
import sys
import time

capture, sockdir = (os.path.realpath(p) for p in sys.argv[1:3])
upto = int(sys.argv[3]) if len(sys.argv) > 3 else 3
START, END = b"\x1b[?2026h", b"\x1b[?2026l"


def frames(before, after):
    assert after.startswith(before), "the before-wire is not where the after-wire starts"
    spans, pos = [], len(before)
    while True:
        s = after.find(START, pos)
        if s < 0:
            return spans
        e = after.index(END, s) + len(END)
        spans.append((s, e))
        pos = e


def replay(data, out):
    feed = os.path.join(sockdir, "feed.bin")
    with open(feed, "wb") as f:
        f.write(data)
    sock = os.path.join(sockdir, "frames.sock")
    tm = ["tmux", "-f", "/dev/null", "-S", sock]
    subprocess.run(tm + ["new-session", "-d", "-x", "120", "-y", "40", "-s", "r",
                         "stty -echo; cat '%s'; exec sleep 30" % feed], check=True)
    try:
        last, same = None, 0
        for _ in range(50):
            time.sleep(0.1)
            shot = subprocess.run(tm + ["capture-pane", "-p", "-e", "-t", "r"],
                                  check=True, capture_output=True).stdout
            same = same + 1 if shot == last else 0
            last = shot
            if same >= 3:
                break
        with open(out, "wb") as f:
            f.write(last)
    finally:
        subprocess.run(tm + ["kill-server"], check=False)


pairs = [(p, p[: -len("-before.wire")] + "-after.wire", p[: -len("-before.wire")])
         for p in glob.glob(os.path.join(capture, "**", "*-before.wire"), recursive=True)]
pairs += [(p, p[: -len("-before-submit.wire")] + "-after-submit.wire", p[: -len("-before-submit.wire")])
          for p in glob.glob(os.path.join(capture, "**", "*-before-submit.wire"), recursive=True)]
for before_path, after_path, stem in sorted(pairs):
    if not os.path.exists(after_path):
        continue
    before = open(before_path, "rb").read()
    after = open(after_path, "rb").read()
    spans = frames(before, after)
    replay(before, stem + "-frame0.ansi")
    for k in range(1, min(upto, len(spans)) + 1):
        s, e = spans[k - 1]
        with open("%s-frame%d.bytes" % (stem, k), "wb") as f:
            f.write(after[s:e])
        replay(after[:e], "%s-frame%d.ansi" % (stem, k))
    replay(after, stem + "-wireend.ansi")
    print("%s: %d frames after Enter; frames 0..%d and the whole wire replayed"
          % (os.path.relpath(stem, capture), len(spans), min(upto, len(spans))))
