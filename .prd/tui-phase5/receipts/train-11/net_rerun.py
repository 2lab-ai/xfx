#!/usr/bin/env python3
"""Train 11: the lib tests net.py's run failed, run again alone, three times
each, under the same proxy variables and loopback logger as net.py (its
child, started and ended here), in the same clone; each run's verdict and the
connections logged during it.

    suite-lock.sh python3 -I net_rerun.py <clone> <connect_logger.py> <out-dir> <test>...
"""
import os
import subprocess
import sys
import time

clone, logger, out, tests = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
os.makedirs(out, exist_ok=True)
log = os.path.join(out, "connect-rerun.log")
open(log, "w").close()
child = subprocess.Popen([sys.executable, "-I", logger, "0", log, "3600"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(100):
    if os.path.exists(log + ".port"):
        break
    time.sleep(0.1)
port = open(log + ".port").read().strip()
proxy = f"http://127.0.0.1:{port}"
env = {k: v for k, v in os.environ.items() if k not in ("NO_PROXY", "no_proxy", "ALL_PROXY", "all_proxy")}
env.update(HTTPS_PROXY=proxy, https_proxy=proxy, HTTP_PROXY=proxy, http_proxy=proxy)
print(f"# logger pid {child.pid}, 127.0.0.1:<ephemeral>; the four proxy variables at it, the four others unset", flush=True)
try:
    for test in tests:
        for n in (1, 2, 3):
            before = sum(1 for _ in open(log))
            load = os.getloadavg()[0]
            r = subprocess.run(["cargo", "test", "--locked", "--lib", "--", "--exact", test], cwd=clone, env=env,
                               capture_output=True, text=True)
            text = r.stdout + r.stderr
            verdict = next((l.split(" ... ")[1] for l in text.splitlines() if l.startswith(f"test {test} ... ")), "?")
            lines = open(log).read().splitlines()[before:]
            print(f"{test.split('::')[-1]} run {n}: exit {r.returncode}, {verdict}; load {load:.1f}; "
                  f"connections logged: {len(lines)}", flush=True)
finally:
    child.terminate()
    child.wait(timeout=10)
    print(f"# logger {child.pid} ended ({child.returncode})")
