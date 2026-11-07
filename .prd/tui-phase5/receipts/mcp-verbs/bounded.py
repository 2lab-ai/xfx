"""Runs a command in its own process group under a wall-clock limit and a
resident-memory cap over the whole group; kills the group (by its id) on
either. macOS refuses `ulimit -v`/`-d`, so the cap is watched, every 100 ms.
usage: python3 -I bounded.py <seconds> <max-rss-MiB> -- <command...>"""
import os, signal, subprocess, sys, time
secs, cap = float(sys.argv[1]), int(sys.argv[2]) * 1024
cmd = sys.argv[sys.argv.index("--") + 1:]
p = subprocess.Popen(cmd, start_new_session=True)
start, peak, why = time.time(), 0, None
while p.poll() is None:
    out = subprocess.run(["ps", "-o", "rss=", "-g", str(p.pid)], capture_output=True, text=True).stdout
    rss = sum(int(x) for x in out.split() if x.isdigit())
    peak = max(peak, rss)
    if rss > cap: why = f"rss {rss // 1024} MiB > cap"
    elif time.time() - start > secs: why = f"past {secs:.0f} s"
    if why:
        os.killpg(p.pid, signal.SIGKILL); p.wait(); break
    time.sleep(0.1)
print(f"bounded: exit {p.returncode}, {time.time() - start:.1f} s, peak rss {peak // 1024} MiB" + (f", KILLED: {why}" if why else ""))
sys.exit(p.returncode if not why else 137)
