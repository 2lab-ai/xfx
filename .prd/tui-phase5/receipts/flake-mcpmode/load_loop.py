"""usage: load_loop.py <loop.sh> <tree> <label> <n> <outdir> <tests...>
Runs loop.sh with one `yes > /dev/null` per core beside it. Meant to run under
SUITE_LOCK_EXCLUSIVE=1 suite-lock.sh, which holds every slot: nothing else
heavy runs while the cores are loaded. The spinners are this process's own
children, each bounded by `timeout 1800`, and are stopped in `finally` --
also on SIGTERM or SIGHUP -- so they end with the command that started them.
Prints the window (UTC) and the 1-minute load average, sampled every 5 s."""
import datetime
import os
import signal
import subprocess
import sys
import threading

loop, tree, label, n, out, *tests = sys.argv[1:]
now = lambda: datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
for sig in (signal.SIGTERM, signal.SIGHUP):
    signal.signal(sig, lambda *_: sys.exit(1))
spinners, loads, done = [], [], threading.Event()


def sample():
    while not done.wait(5):
        loads.append(os.getloadavg()[0])


rc = 1
try:
    started = now()
    for _ in range(os.cpu_count()):
        spinners.append(subprocess.Popen(["timeout", "1800", "yes"], stdout=subprocess.DEVNULL))
    print("spinners %d started %s" % (len(spinners), started), flush=True)
    threading.Thread(target=sample, daemon=True).start()
    rc = subprocess.run(["bash", loop, tree, label, n, out, *tests]).returncode
finally:
    done.set()
    for p in spinners:
        p.terminate()
    for p in spinners:
        p.wait()
    print("spinners stopped %s; loop rc=%d; 1-min load avg min %.1f max %.1f over %d samples"
          % (now(), rc, min(loads or [0]), max(loads or [0]), len(loads)), flush=True)
sys.exit(rc)
