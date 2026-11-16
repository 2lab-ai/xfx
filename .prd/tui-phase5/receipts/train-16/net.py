#!/usr/bin/env python3
# Train 16: train 15's net.py, with a second part. TEST-ENV-ISOLATION re-runs
# every integration test binary in a copy whose environment is PATH, TMPDIR,
# an empty HOME and a few cargo names (`tests/support/isolated.rs`), so no
# proxy variable reaches the tui and sessions runs' xfx: the logger is blind
# there by construction (the lib's tests still build reqwest clients in the
# test process, which reads the proxy itself: its known `w.example` CONNECTs
# are the method's positive control). So, second: every test binary --
# `cargo test --locked --all-targets --no-fail-fast` -- under `sandbox-exec`
# with a profile that refuses every outbound connection but loopback (unix
# sockets untouched), its result lines; and the profile's own controls, a
# `curl` to 192.0.2.1 (TEST-NET-1, RFC 5737: nobody's) refused at once under
# it, and the same curl outside it timing out (3 s), so the refusal is the
# profile's. A test that needed a non-loopback connection fails there.
"""Join check 5 (train 10's to 12's net.py, with one change: the `sessions`
test binary too, which HISTORY-NOTICES-COPY-FX changed): the lib tests, the
tui tests and the sessions tests at the train head with every
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
                       ("tui", ["cargo", "test", "--locked", "--test", "tui", "--no-fail-fast"]),
                       ("sessions", ["cargo", "test", "--locked", "--test", "sessions", "--no-fail-fast"])):
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
PROFILE = ('(version 1)(allow default)(deny network-outbound (remote ip "*:*"))'
           '(allow network-outbound (remote ip "localhost:*"))')
print(f"# part 2: sandbox-exec -p '{PROFILE}'", flush=True)
for name, cmd in (("control, under it", ["sandbox-exec", "-p", PROFILE, "/usr/bin/curl", "-sS", "-m", "3", "http://192.0.2.1/"]),
                  ("control, outside it", ["/usr/bin/curl", "-sS", "-m", "3", "http://192.0.2.1/"])):
    started = time.monotonic()
    r = subprocess.run(cmd, capture_output=True, text=True)
    print(f"## {name}: curl exit {r.returncode}, {time.monotonic() - started:.1f} s: {r.stderr.strip()[:120]}", flush=True)
args = ["sandbox-exec", "-p", PROFILE, "cargo", "test", "--locked", "--all-targets", "--no-fail-fast"]
started = time.monotonic()
with open(os.path.join(out, "net-sandboxed.log"), "w") as f:
    rc = subprocess.run(args, cwd=clone, stdout=f, stderr=subprocess.STDOUT).returncode
results = [l for l in open(os.path.join(out, "net-sandboxed.log")) if l.startswith("test result:")]
print(f"## every test binary under the profile: `cargo test --locked --all-targets --no-fail-fast` exit {rc}, "
      f"{time.monotonic() - started:.0f} s; {len(results)} binaries", flush=True)
for r in results:
    print("   " + r.strip())
passed = sum(int(r.split()[3]) for r in results)
failed = sum(int(r.split()[5]) for r in results)
print(f"   summed: {passed} passed, {failed} failed")
