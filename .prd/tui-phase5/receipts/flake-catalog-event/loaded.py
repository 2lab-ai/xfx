#!/usr/bin/env python3
"""loaded.py <command...>: runs the command with one `yes` spinner per core
(stdout to /dev/null) beside it, then kills exactly the spinners it started.
Exits with the command's status."""
import os, subprocess, sys
n = os.cpu_count() or 1
kids = [subprocess.Popen(["/usr/bin/yes"], stdout=subprocess.DEVNULL) for _ in range(n)]
print(f"spinners: {n} started, pids {kids[0].pid}..{kids[-1].pid}", flush=True)
try:
    rc = subprocess.run(sys.argv[1:]).returncode
finally:
    for k in kids:
        k.kill()
    for k in kids:
        k.wait()
    print(f"spinners: {n} killed", flush=True)
sys.exit(rc)
