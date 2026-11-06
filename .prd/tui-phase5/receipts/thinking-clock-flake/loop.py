#!/usr/bin/env python3
"""Bounded repro loop: run one test N times, J at a time.

Each run is its own session (process group), killed whole after LIMIT
seconds. Prints pass/fail counts and keeps each failure's output.

usage: loop.py <test-binary> <test-name> <runs> <jobs> <limit-s> <outdir> [env K=V ...]
"""
import os
import signal
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

binary, name, runs, jobs, limit, outdir = sys.argv[1:7]
runs, jobs, limit = int(runs), int(jobs), float(limit)
extra = dict(kv.split("=", 1) for kv in sys.argv[7:])
os.makedirs(outdir, exist_ok=True)
env = dict(os.environ, RUST_BACKTRACE="0", **extra)


def one(index):
    started = time.monotonic()
    proc = subprocess.Popen(
        [binary, name, "--exact", "--nocapture", "--test-threads=1"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        process_group=0,
        env=env,
    )
    try:
        out, _ = proc.communicate(timeout=limit)
        verdict = "pass" if proc.returncode == 0 else "fail"
    except subprocess.TimeoutExpired:
        verdict = "timeout"
    finally:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    if verdict == "timeout":
        out, _ = proc.communicate()
    took = time.monotonic() - started
    if verdict != "pass":
        with open(os.path.join(outdir, f"run-{index:04d}-{verdict}.txt"), "wb") as f:
            f.write(out)
    return verdict, took, out


burners = []
for _ in range(int(os.environ.get("BURNERS", "0"))):
    burners.append(subprocess.Popen(
        [sys.executable, "-c", "import time\nend=time.monotonic()+%d\nwhile time.monotonic()<end: pass" % int(os.environ.get("BURN_LIMIT", "900"))],
        start_new_session=True))
counts = {"pass": 0, "fail": 0, "timeout": 0}
messages = {}
with ThreadPoolExecutor(max_workers=jobs) as pool:
    for verdict, took, out in pool.map(one, range(runs)):
        counts[verdict] += 1
        if verdict != "pass":
            text = out.decode("utf-8", "replace")
            line = next((l for l in text.splitlines() if "panicked at" in l), "?")
            nxt = text.split(line, 1)[-1].splitlines()[1:2] if line != "?" else []
            key = line.split("panicked at", 1)[-1].strip() + " | " + (nxt[0][:120] if nxt else "")
            messages[key] = messages.get(key, 0) + 1
for b in burners:
    try:
        os.killpg(b.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    b.wait()
print(f"burners={len(burners)} runs={runs} jobs={jobs} " + " ".join(f"{k}={v}" for k, v in counts.items()))
for key, n in sorted(messages.items(), key=lambda kv: -kv[1]):
    print(f"  {n:4d}  {key}")
