"""Each -tc scenario against its 256-colour twin, on the same side of one sweep.

usage: python3 -I tc_vs_256.py <sweep runs dir> <fx|xfx>

For every snap of every `<sc>-tc` scenario with a twin `<sc>` in the runs
dir: whether the two `capture-pane -e` files are byte-identical, and if not,
each row that differs (1-based), both sides printed with escapes shown, and
whether it still differs once each run of digits is masked (a clock, a wall
time). A row that differs only in digits is a clock, not a colour.
"""
import pathlib, re, sys
runs = pathlib.Path(sys.argv[1]); side = sys.argv[2]
digits = re.compile(r"\d+")
sgr = re.compile(r"\x1b\[[0-9;]*m")
total = same = masked_same = 0
for tc in sorted(p for p in runs.iterdir() if p.name.endswith("-tc")):
    twin = runs / tc.name[:-3]
    if not twin.is_dir():
        continue
    for f in sorted((tc / side).glob("%s-%s-40x120-*.ansi" % (side, tc.name))):
        snap = f.name[len("%s-%s-40x120-" % (side, tc.name)):-len(".ansi")]
        g = twin / side / ("%s-%s-40x120-%s.ansi" % (side, twin.name, snap))
        total += 1
        a, b = f.read_bytes(), g.read_bytes()
        if a == b:
            same += 1
            print("%s %s: identical" % (tc.name, snap))
            continue
        ra, rb = a.decode().split("\n"), b.decode().split("\n")
        rows = [i for i in range(max(len(ra), len(rb)))
                if (ra[i] if i < len(ra) else None) != (rb[i] if i < len(rb) else None)]
        colour = [i for i in rows
                  if sgr.findall(ra[i] if i < len(ra) else "") != sgr.findall(rb[i] if i < len(rb) else "")]
        textual = [i for i in rows
                   if digits.sub("#", ra[i] if i < len(ra) else "") != digits.sub("#", rb[i] if i < len(rb) else "")]
        if not textual:
            masked_same += 1
        print("%s %s: %d rows differ %s; differing in their SGR sequences: %s; after masking digits: %s" % (
            tc.name, snap, len(rows), [i + 1 for i in rows], [i + 1 for i in colour] or "none",
            [i + 1 for i in textual] or "none"))
        for i in rows:
            print("    row %d -tc : %r" % (i + 1, ra[i] if i < len(ra) else None))
            print("    row %d 256 : %r" % (i + 1, rb[i] if i < len(rb) else None))
print("# %s: %d snaps, %d byte-identical, %d more identical once digits are masked" % (side, total, same, masked_same))
