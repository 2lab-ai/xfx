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
# Round 2 (opus's S2): the attempts too. The kernel reports each connection
# the profile refuses in the unified log (`Sandbox: <name>(<pid>) deny(1)
# network-outbound remote:*:<port>`, the address withheld), and `log stream`
# (this script's own child) records every such report for the whole of part
# 2. Planted under the profile: a connection to 192.0.2.1 and one to
# 2001:db8::1 (RFC 3849's documentation prefix) before the run, the same two
# 60 s into it while it runs, and the IPv4 one after it; each planted pid
# must have its report, and every other report is listed by name, port and
# time into the run. The IPv6 control is the report: outside the profile a
# curl to 2001:db8::1 fails at once too (no route here), with no report.
"""Join check 5 (train 10's to 12's net.py, with one change: the `sessions`
test binary too, which HISTORY-NOTICES-COPY-FX changed): the lib tests, the
tui tests and the sessions tests at the train head with every
https and http proxy variable naming a loopback logger on an ephemeral port
(FLAKE-CATALOG-EVENT's connect_logger.py: it logs each connection's first
request line, answers 403 and forwards nothing). The logger is this script's
own child, started with Popen and ended by it.

    suite-lock.sh python3 -I net.py <clone> <connect_logger.py> <out-dir>
"""
import datetime
import json
import os
import re
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
reports = os.path.join(out, "deny-reports.ndjson")
stream = subprocess.Popen(["/usr/bin/log", "stream", "--style", "ndjson", "--predicate",
                           'eventMessage CONTAINS "network-outbound"'],
                          stdout=open(reports, "w"), stderr=subprocess.DEVNULL)
for _ in range(100):
    if "Filtering" in open(reports).read():
        break
    time.sleep(0.1)
print(f"# log stream pid {stream.pid}: every report whose message names network-outbound, from here", flush=True)
V4, V6 = "http://192.0.2.1/", "http://[2001:db8::1]/"
planted = []  # (name, pid, under the profile, seconds since part 2's start)
part2 = time.monotonic()


def curl(name, url, sandboxed):
    cmd = (["sandbox-exec", "-p", PROFILE] if sandboxed else []) + ["/usr/bin/curl", "-sS", "-m", "3", url]
    started = time.monotonic()
    child = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    err = child.communicate()[1]
    planted.append((name, child.pid, sandboxed, started - part2))
    print(f"## {name}: curl exit {child.returncode}, {time.monotonic() - started:.1f} s: {err.strip()[:120]}", flush=True)


curl("control, IPv4, under it", V4, True)
curl("control, IPv4, outside it", V4, False)
curl("control, IPv6, under it", V6, True)
curl("control, IPv6, outside it", V6, False)
args = ["sandbox-exec", "-p", PROFILE, "cargo", "test", "--locked", "--all-targets", "--no-fail-fast"]
run_from, run_from_wall = time.monotonic(), time.time()
with open(os.path.join(out, "net-sandboxed.log"), "w") as f:
    run = subprocess.Popen(args, cwd=clone, stdout=f, stderr=subprocess.STDOUT)
    while run.poll() is None and time.monotonic() - run_from < 60:
        time.sleep(0.5)
    if run.poll() is None:
        curl("planted 60 s into the run, IPv4, under it", V4, True)
        curl("planted 60 s into the run, IPv6, under it", V6, True)
        print(f"   the run still going after both: {run.poll() is None}", flush=True)
    else:
        print("   the run ended before 60 s: nothing planted inside it", flush=True)
    rc = run.wait()
run_to, run_to_wall = time.monotonic(), time.time()
curl("planted after the run, IPv4, under it", V4, True)
time.sleep(5)
stream.terminate()
stream.wait(timeout=10)
print(f"# log stream {stream.pid} ended ({stream.returncode})")
results = [l for l in open(os.path.join(out, "net-sandboxed.log")) if l.startswith("test result:")]
print(f"## every test binary under the profile: `cargo test --locked --all-targets --no-fail-fast` exit {rc}, "
      f"{run_to - run_from:.0f} s; {len(results)} binaries", flush=True)
for r in results:
    print("   " + r.strip())
passed = sum(int(r.split()[3]) for r in results)
failed = sum(int(r.split()[5]) for r in results)
print(f"   summed: {passed} passed, {failed} failed")
REPORT = re.compile(r"^Sandbox: (.+?)\((\d+)\) deny\((\d+)\) network-outbound (.*)$")
seen, other, unparsed = {}, [], 0
for line in open(reports):
    try:
        event = json.loads(line)
    except ValueError:
        continue  # the stream's own first line
    m = REPORT.match(event.get("eventMessage", ""))
    if not m:
        unparsed += 1
        continue
    name, pid, count, what = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)
    if not what.startswith("remote:"):
        unparsed += 1  # another process's own sandbox, about a socket file: not this profile's
        continue
    at = datetime.datetime.strptime(event["timestamp"], "%Y-%m-%d %H:%M:%S.%f%z").timestamp()
    seen.setdefault(pid, []).append((name, count, what))
    other.append((at, name, pid, count, what))
planted_pids = {pid for _, pid, _, _ in planted}
print(f"## the denial reports: {sum(len(v) for v in seen.values())} naming a remote address; "
      f"{unparsed} others (no `remote:`, or not a deny line) not counted")
for name, pid, sandboxed, at in planted:
    got = seen.get(pid, [])
    print(f"   {name} (at {at:.0f} s): {'reported: ' + '; '.join(f'{n} deny({c}) {w}' for n, c, w in got) if got else 'no report'}"
          f"{'' if sandboxed == bool(got) else '   <-- NOT AS EXPECTED'}")
rest = [o for o in other if o[2] not in planted_pids]
inside = [o for o in rest if run_from_wall <= o[0] <= run_to_wall]
print(f"   reports from any other process: {len(rest)}, {len(inside)} of them during the run "
      f"({run_to_wall - run_from_wall:.0f} s)")
for at, name, pid, count, what in sorted(rest):
    print(f"     {name} deny({count}) {what} ({at - run_from_wall:.0f} s from the run's start)")
