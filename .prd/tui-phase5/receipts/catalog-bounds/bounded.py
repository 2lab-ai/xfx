#!/usr/bin/env python3
"""Runs a command in a process group of its own and kills the group once it
has run <seconds>, or once the group's resident memory passes <MiB>.

`ulimit -v` is RLIMIT_AS, which Darwin does not enforce (a 500 MB bytearray
under `ulimit -v 100000` succeeds on this machine), so a RED run is bounded
here by watching it instead.

usage: python3 -I bounded.py <seconds> <MiB> -- <command> [args...]
Exit status: the command's, or 124 (time) / 125 (memory) when killed."""
import os, signal, subprocess, sys, time

def group_rss_kib(pgid):
    out = subprocess.run(["ps", "-A", "-o", "pgid=,rss="], capture_output=True, text=True).stdout
    return sum(int(rss) for g, rss in (line.split() for line in out.splitlines() if line.strip()) if int(g) == pgid)

def main():
    seconds, mib = float(sys.argv[1]), int(sys.argv[2])
    assert sys.argv[3] == "--"
    child = subprocess.Popen(sys.argv[4:], start_new_session=True)
    start, peak = time.monotonic(), 0
    while child.poll() is None:
        peak = max(peak, group_rss_kib(child.pid))
        over_time = time.monotonic() - start > seconds
        if over_time or peak > mib * 1024:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
            print(f"bounded: killed after {time.monotonic() - start:.1f}s, peak rss {peak // 1024} MiB", file=sys.stderr)
            return 124 if over_time else 125
        time.sleep(0.2)
    print(f"bounded: exit {child.returncode} after {time.monotonic() - start:.1f}s, peak rss {peak // 1024} MiB", file=sys.stderr)
    return child.returncode

sys.exit(main())
