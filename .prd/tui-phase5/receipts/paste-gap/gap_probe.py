"""Does one paste reach a reader shaped like xfx's loop as one delivery?

A raw-mode pty pair, nothing else. A writer process does what a terminal does
with a paste: blocking writes of the whole thing to the master. The reader is
the shape of `event_loop.rs`'s turn (`burst`, fx's `event_loop.zig:94-119`):
wait for input, then up to 32 reads of 128 bytes, a zero-time readiness check
before each, and **a check that finds nothing ends the delivery**; still
readable after the 32nd read, the delivery goes on into the next turn.

The paste is `ESC[200~` + 16 bytes + an embedded `ESC[201~` + payload + the
terminal's own `ESC[201~`, at 1 KiB, 4 KiB, 64 KiB and 1 MiB. Per size it
reports how many deliveries the paste took, where the first one ended
relative to the embedded marker (the bytes after that point are what a
decoder with fx's rule reads as keys), and how long each gap lasted (from the
check that found nothing to the next byte being readable).

With `--load`, one `yes >/dev/null` per core runs beside it for the whole
measurement, started by this script and killed by it (by its own handle) when
it ends, however it ends -- run that under `SUITE_LOCK_EXCLUSIVE=1
suite-lock.sh`, which keeps every other suite off the machine meanwhile.

Bounded: fixed sizes, a 60 s deadline per run, no file or network.
Run: python3 -I -B gap_probe.py [repeats] [--load]   (output: gap-probe.txt)
"""
import os, select, statistics, subprocess, sys, time, tty

BURST_BYTES, READ_BURSTS, TICK = 128, 32, 0.008
START, END = b"\x1b[200~", b"\x1b[201~"


def paste(size):
    head = START + b"e" * 16 + END
    body = size - len(head) - len(END)
    return head + (b"K" * body) + END, len(head)


def run(size):
    data, marker_end = paste(size)
    m, s = os.openpty()
    tty.setraw(s)
    # The writer is a process of its own, blocking, as a terminal's is: a
    # thread would share the reader's interpreter lock and its scheduling.
    pid = os.fork()
    if pid == 0:
        os.close(s)
        view = memoryview(data)
        while view:
            n = os.write(m, view)
            view = view[n:]
        os._exit(0)
    deadline = time.monotonic() + 60
    poller = select.poll()
    poller.register(s, select.POLLIN)
    consumed, ends, gaps = 0, [], []
    quiet_at = None
    while consumed < len(data):
        if time.monotonic() > deadline:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
            raise SystemExit(f"deadline: {size} bytes, {consumed} read")
        # The wait between turns: up to a tick, as the loop's is.
        if not poller.poll(int(TICK * 1000)):
            continue
        if quiet_at is not None:
            gaps.append(time.monotonic() - quiet_at)
            quiet_at = None
        for reads in range(READ_BURSTS + 1):
            if not poller.poll(0):
                ends.append(consumed)
                quiet_at = time.monotonic()
                break
            if reads == READ_BURSTS:
                break
            consumed += len(os.read(s, BURST_BYTES))
    os.waitpid(pid, 0)
    os.close(m)
    os.close(s)
    if not ends or ends[-1] != consumed:
        ends.append(consumed)
    first = ends[0]
    keys = max(0, len(data) - max(first, marker_end))
    return len(ends), first, marker_end, keys, gaps


def main():
    args = [a for a in sys.argv[1:] if a != "--load"]
    load = "--load" in sys.argv[1:]
    repeats = int(args[0]) if args else 20
    spinners = []
    try:
        if load:
            for _ in range(os.cpu_count()):
                spinners.append(subprocess.Popen(["/usr/bin/yes"], stdout=subprocess.DEVNULL))
        measure(repeats, load, len(spinners))
    finally:
        for spinner in spinners:
            spinner.kill()
            spinner.wait()


def measure(repeats, load, spinners):
    print(f"# python3 -I -B gap_probe.py {repeats}{' --load' if load else ''}; {os.uname().sysname}"
          f" {os.uname().release}; {spinners} spinners; load avg {os.getloadavg()[0]:.1f}")
    print("# size | runs | deliveries min/med/max | runs whose first delivery ended inside the paste"
          " | bytes after the first delivery (min/med/max) | gap ms med/p95/max")
    for size in (1 << 10, 4 << 10, 64 << 10, 1 << 20):
        runs = repeats if size < (1 << 20) else max(3, repeats // 4)
        rows = [run(size) for _ in range(runs)]
        dels = sorted(r[0] for r in rows)
        inside = sum(1 for r in rows if r[1] < size)
        keys = sorted(r[3] for r in rows)
        gaps = sorted(g * 1000 for r in rows for g in r[4])
        p95 = gaps[int(len(gaps) * 0.95)] if gaps else 0.0
        print(f"{size:>8} | {runs:>2} | {dels[0]}/{statistics.median(dels):g}/{dels[-1]} | {inside}/{runs}"
              f" | {keys[0]}/{statistics.median(keys):g}/{keys[-1]}"
              f" | {statistics.median(gaps) if gaps else 0:.3f}/{p95:.3f}/{gaps[-1] if gaps else 0:.3f}")


main()
