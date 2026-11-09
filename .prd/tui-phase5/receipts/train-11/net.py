#!/usr/bin/env python3
"""Join check 5 (train 10's net.py, unchanged but for this line): the lib tests and the tui tests at the train head with every
https and http proxy variable naming a loopback logger on an ephemeral port
(FLAKE-CATALOG-EVENT's connect_logger.py: it logs each connection's first
request line, answers 403 and forwards nothing). The logger is this script's
own child, started with Popen and ended by it.

    suite-lock.sh python3 -I net.py <clone> <connect_logger.py> <out-dir>
"""
import os
import subprocess
import sys
import time

clone, logger, out = sys.argv[1:4]
os.makedirs(out, exist_ok=True)
log = os.path.join(out, "connect.log")
open(log, "w").close()
child = subprocess.Popen([sys.executable, "-I", logger, "0", log, "10800"],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
for _ in range(100):
    if os.path.exists(log + ".port"):
        break
    time.sleep(0.1)
port = open(log + ".port").read().strip()
proxy = f"http://127.0.0.1:{port}"
env = {k: v for k, v in os.environ.items()
       if k not in ("NO_PROXY", "no_proxy", "ALL_PROXY", "all_proxy")}
env.update(HTTPS_PROXY=proxy, https_proxy=proxy, HTTP_PROXY=proxy, http_proxy=proxy)
print(f"# logger pid {child.pid}, 127.0.0.1:<ephemeral>; HTTPS_PROXY, https_proxy, HTTP_PROXY, "
      f"http_proxy at it; NO_PROXY, no_proxy, ALL_PROXY, all_proxy unset", flush=True)
try:
    for name, args in (("lib", ["cargo", "test", "--locked", "--lib", "--no-fail-fast"]),
                       ("tui", ["cargo", "test", "--locked", "--test", "tui", "--no-fail-fast"])):
        before = sum(1 for _ in open(log))
        started = time.monotonic()
        with open(os.path.join(out, f"net-{name}.log"), "w") as f:
            rc = subprocess.run(args, cwd=clone, env=env, stdout=f, stderr=subprocess.STDOUT).returncode
        lines = open(log).read().splitlines()[before:]
        results = [l for l in open(os.path.join(out, f"net-{name}.log")) if l.startswith("test result:")]
        print(f"## {name}: `{' '.join(args)}` exit {rc}, {time.monotonic() - started:.0f} s", flush=True)
        for r in results:
            print("   " + r.strip())
        print(f"   connections logged during it: {len(lines)}")
        for l in sorted(set(lines)):
            print(f"     {lines.count(l)} x {l}")
finally:
    child.terminate()
    child.wait(timeout=10)
    print(f"# logger {child.pid} ended ({child.returncode})")
