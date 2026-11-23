"""Every fxcap scenario that seeds a profile before launch, judged by xfx
itself: each seed laid down as `~/.xfx/settings.json` (`0o600` in a `0o700`
profile, as capture.sh writes it) under a fresh HOME, then `ask` run there
with no credential; the verdict is the error a refusal names
(`xfx: <Name>`), or `starts`. Run with two binaries, it prints the
scenarios whose verdict moved. No path is printed. (PROFILE-REFUSE-AT-
LAUNCH's seed_scan.py judged by its own copy of the rule; this one asks the
binaries.)

usage: python3 -I seed_scan.py <scenarios dir> <run root> <base xfx> <head xfx>
"""

import os
import re
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
SEED = re.compile(r"^(?:(?:fx|xfx):\s*)?(?:seed\s+(\S+)|homefile\s+\.x?fx/settings\.json\s+(\S+))\s*$")


def verdict(binary, seed, root):
    shutil.rmtree(root, ignore_errors=True)
    home, work = os.path.join(root, "home"), os.path.join(root, "work")
    os.makedirs(os.path.join(home, ".xfx"), mode=0o700)
    os.chmod(os.path.join(home, ".xfx"), 0o700)
    os.makedirs(work)
    target = os.path.join(home, ".xfx", "settings.json")
    shutil.copyfile(seed, target)
    os.chmod(target, 0o600)
    env = {"HOME": home, "PATH": "/usr/bin:/bin", "TMPDIR": root, "XFX_SOUND": "0",
           "XFX_GATEWAY_URL": "http://127.0.0.1:9/v1/chat/completions"}
    run = subprocess.run([binary, "ask", "--no-save", "x"], cwd=work, env=env,
                         capture_output=True, text=True, timeout=20, stdin=subprocess.DEVNULL)
    named = re.search(r"^xfx: ((?:Invalid|Unknown|Retired)[A-Za-z]+)$", run.stderr, re.M)
    return named.group(1) if named else "starts"


def main():
    scen, root, base, head = sys.argv[1:5]
    caps = sorted(f for f in os.listdir(scen) if f.endswith(".cap"))
    seeding = refused = moved = 0
    for cap in caps:
        seeds = [m.group(1) or m.group(2) for line in open(os.path.join(scen, cap))
                 for m in [SEED.match(line.strip())] if m]
        if not seeds:
            continue
        seeding += 1
        pairs = [(s, verdict(base, os.path.join(scen, s), root), verdict(head, os.path.join(scen, s), root))
                 for s in seeds]
        refused += any(h != "starts" for _, _, h in pairs)
        if any(b != h for _, b, h in pairs):
            moved += 1
            print(f"MOVED {cap[:-4]}: " + " ".join(f"{s}={b}->{h}" for s, b, h in pairs))
        else:
            print(f"{cap[:-4]}: " + " ".join(f"{s}={h}" for s, _, h in pairs))
    print(f"== {len(caps)} scenarios, {seeding} seed a profile, {refused} seed one the head refuses, {moved} moved")


if __name__ == "__main__":
    main()
