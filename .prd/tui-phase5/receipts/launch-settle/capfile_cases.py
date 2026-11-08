"""Each changed Capfile case through one capfile.py: the races it finds.

usage: python3 -I capfile_cases.py <scripts/fxcap dir>
"""
import os, sys, tempfile
sys.dont_write_bytecode = True
sys.path.insert(0, sys.argv[1])
import capfile

YOLO = "fx: env FX_PERMISSION_MODE=yolo\nxfx: env XFX_PERMISSION_MODE=yolo\n"
CASES = [
    ("launch-armed, absent at 2 s", YOLO + "wait 2\nabsent Full access\nwait 2\nsnap a\n", "race"),
    ("launch-armed, absent at 3 s (test_sweep.py:714)", YOLO + "wait 3\nabsent Full access\nwait 1\nsnap a\n", "race"),
    ("launch-armed, absent at 3.5 s", YOLO + "wait 3.5\nabsent Full access\nsnap a\n", "race"),
    ("relaunch-armed, absent at 3 s", "keep\n" + YOLO + "until shown Full access\nwait 1\nrelaunch\nwait 3\nabsent Full access\nwait 1\nsnap a\n", "race"),
    ("seen, absent 0.5 s after", YOLO + "until shown Full access\nwait 0.5\nabsent Full access\nwait 3.5\nsnap a\n", "none"),
    ("seen, absent 3.5 s after", YOLO + "until shown Full access\nwait 3.5\nabsent Full access\nsnap start\n", "none"),
    ("seen, absent of a longer text", YOLO + "until shown Full access\nwait 3.5\nabsent Full access never-rendered\nsnap start\n", "race"),
    ("seen then paused, absent", YOLO + "until shown Full access\nwait 1\ntype /model\nkey Escape\nabsent Full access\nwait 9\nsnap a\n", "race"),
    ("hint seen, absent at once", "key C-c\nuntil shown press ctrl+c again to exit\nabsent press ctrl+c again to exit\nwait 2.5\nsnap after\n", "none"),
    ("hint, absent at 1.25 s (settle 1.0)", "key C-c\nwait 1.25\nabsent press ctrl+c again to exit\nwait 1.25\nsnap after\n", "none"),
    ("hint, absent at 0.5 s (inside settle)", "key C-c\nwait 0.5\nabsent press ctrl+c again to exit\nwait 2\nsnap after\n", "race"),
    ("launch-armed, until gone at 3.5 s", YOLO + "wait 3.5\nuntil gone Full access\nsnap start\n", "none"),
]
for name, cap, want in CASES:
    d = tempfile.mkdtemp(prefix="cases.")
    with open(os.path.join(d, "t.cap"), "w") as f:
        f.write(cap)
    (unit,) = capfile.units(d)
    got = [l for side in ("fx", "xfx") for l in capfile.races(unit.reading(side))]
    have = "race" if got else "none"
    print("%-6s want %-4s got %-4s  %s" % ("ok" if have == want else "DIFFER", want, have, name))
settle = {t.name: t.settle for t in capfile.TIMERS}
print("settle: %s" % ", ".join("%s %s" % (k, v) for k, v in settle.items()))
