#!/usr/bin/env python3
"""FLAKE-HINT4's timer-window lint over L-U1+U2's 13 `p5-lifecycle-osc-*`
scenarios at the head, three ways, the lint itself unchanged:

1. as the repository runs it (`capfile.races`, which counts a `wire` line as
   0.3 s spent and a `snap`/`snapfull` or a pausing line as what can race);
2. widened for these scenarios only: every `wire <label>` read as a
   `snap <label>`, since an osc scenario captures by `wire` alone (a copy of
   each file in a scratch directory; the repository's lint is not changed);
3. the negative control: `p5-lifecycle-osc-interruptnext` with full access
   added to each binary's environment (`fx: env FX_PERMISSION_MODE=full-access`,
   `xfx: env XFX_PERMISSION_MODE=yolo`, the form FLAKE-HINT4's converted
   scenarios use) and, apart, with a `key C-c`, a
   `wait 2.5` and a `snap` after its first `wire` (2.5 s after a Ctrl-C, the
   hint's 3 s near): the lint must refuse each, so it reads these files.

Also: every line of the 13 that could arm one of the two timers (a
`PERMISSION_MODE` env, a seed or home file naming a permission mode, a
`C-c` key).

    python3 -I -B lint_osc.py <repo-worktree>
"""
import os
import re
import shutil
import sys
import tempfile


def main():
    repo = sys.argv[1]
    where = os.path.join(repo, "scripts", "fxcap", "scenarios")
    sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
    import capfile

    names = sorted(f[:-4] for f in os.listdir(where) if f.startswith("p5-lifecycle-osc-") and f.endswith(".cap"))
    print(f"# {len(names)} scenarios: {' '.join(n[len('p5-lifecycle-osc-'):] for n in names)}")
    print(f"# timers: {', '.join(t.name for t in capfile.TIMERS)}")
    bad = 0

    def races_in(directory, only):
        out = []
        for u in capfile.units(directory):
            if u.name in only:
                for side in ("fx", "xfx"):
                    out += capfile.races(u.reading(side))
        return out

    plain = races_in(where, set(names))
    print(f"1. as the repository runs it: {len(plain)} races")
    for r in plain:
        print("   ", r)
    bad += len(plain)

    with tempfile.TemporaryDirectory() as d:
        for f in os.listdir(where):
            if os.path.isfile(os.path.join(where, f)):
                shutil.copy(os.path.join(where, f), d)
        wires = 0
        for n in names:
            p = os.path.join(d, n + ".cap")
            text = open(p, encoding="utf-8").read()
            wires += len(re.findall(r"(?m)^wire ", text))
            open(p, "w", encoding="utf-8").write(re.sub(r"(?m)^wire ", "snap ", text))
        widened = races_in(d, set(names))
        print(f"2. every `wire` read as a `snap` ({wires} wire lines): {len(widened)} races")
        for r in widened:
            print("   ", r)
        bad += len(widened)

    for name, edit in (
        ("full access at launch",
         lambda t: t.replace("xfx: env XFX_SOUND=off\n",
                             "xfx: env XFX_SOUND=off\nfx: env FX_PERMISSION_MODE=full-access\nxfx: env XFX_PERMISSION_MODE=yolo\n", 1)),
        ("a Ctrl-C, 2.5 s, and a snap after the first wire",
         lambda t: t.replace("wire n00-working\n", "wire n00-working\nkey C-c\nwait 2.5\nsnap n00-hint\n", 1)),
    ):
        with tempfile.TemporaryDirectory() as d:
            for f in os.listdir(where):
                if os.path.isfile(os.path.join(where, f)):
                    shutil.copy(os.path.join(where, f), d)
            p = os.path.join(d, "p5-lifecycle-osc-interruptnext.cap")
            before = open(p, encoding="utf-8").read()
            after = edit(before)
            assert after != before, name
            open(p, "w", encoding="utf-8").write(after)
            found = races_in(d, {"p5-lifecycle-osc-interruptnext"})
            print(f"3. control, interruptnext with {name}: {len(found)} races -- {'refused' if found else 'MISSED'}")
            for r in found[:2]:
                print("   ", r)
            bad += not found

    arming = re.compile(r"PERMISSION_MODE|permission_mode|\bC-c\b")
    print("arming lines in the 13 (and the files their seeds name):")
    for n in names:
        p = os.path.join(where, n + ".cap")
        for i, line in enumerate(open(p, encoding="utf-8").read().splitlines(), 1):
            if arming.search(line):
                print(f"    {n}.cap:{i}: {line}")
            m = re.match(r"(?:fx: |xfx: )?(?:seed|homefile|reseed|rehomefile) (\S+)", line)
            if m:
                body = open(os.path.join(where, m.group(1)), encoding="utf-8").read()
                for mode in re.findall(r'"permission_mode"\s*:\s*"([^"]*)"', body):
                    print(f"    {n}.cap:{i} seeds {m.group(1)}: permission_mode {mode!r}")
    print("OK" if not bad else f"{bad} problems")
    return 1 if bad else 0


sys.exit(main())
