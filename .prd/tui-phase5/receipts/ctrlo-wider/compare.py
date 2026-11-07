"""Every scalar's count: xfx before, xfx after, fx; and what the two terminals draw.

usage: python3 -I compare.py <base> <head> <drawn_wider.rs at the base> <wrap.rs at the head>
       python3 -I compare.py --packed sweeps.txt.xz <drawn_wider.rs at the base> <wrap.rs at the head>

<base> and <head> are two runs of `scripts/drawn-wider/sweep.sh`, each holding
`scalars.txt`, `xfx.txt` (`super::wrap::width` of `[` scalar `]`, less the
markers), `tmux.txt` and `ghostty.txt` (the advance tmux 3.7b and
libghostty-vt 1.3.2 measured for the same three characters); <base> also holds
`fx.txt`, fx v0.0.16's count of each scalar alone (`fx_rune_width.py`, read
from fx's own source at 481104fd, not from xfx's port).

`sweeps.txt.xz`, committed beside this, is the five columns of those runs in
one file -- `hex fx xfx-base xfx-head tmux libghostty-vt` per scalar -- and
`--packed` reads it in their place. The two runs drew every scalar the same on
both terminals (`sweeps.sha256`: the head's `tmux.txt` and `ghostty.txt` are
the base's bytes), so it keeps one copy of each; and it writes each column back
out as the file it came from and checks its sha256 against `sweeps.sha256`.

Prints the summary the receipt cites and exits 1 if any claim fails:
  * the base table is the base sweep, exactly;
  * P1 = the base table's scalars fx counts as more cells than xfx did;
  * the head's FX_COUNTED (wrap.rs) is P1, exactly;
  * the scalars whose count moved are P1, exactly, and each now counts fx's;
  * no scalar of P1 is drawn wider than fx's count by either terminal;
  * the terminals drew every scalar the same in both sweeps.
"""
import re
import sys

packed = sys.argv[1] == "--packed"
if packed:
    import hashlib
    import lzma
    import os

    source, base_table_rs, head_wrap_rs = sys.argv[2:5]
    columns = {"fx": {}, "before": {}, "after": {}, "tmux": {}, "ghostty": {}}
    texts = {name: [] for name in columns}
    with lzma.open(source, "rt", encoding="ascii") as f:
        for line in f:
            if line.startswith("#"):
                continue
            code, *cells = line.split()
            for name, cell in zip(columns, cells):
                columns[name][int(code, 16)] = int(cell)
                texts[name].append(f"{code} {cell}\n")
    sums = {}
    for line in open(os.path.join(os.path.dirname(os.path.abspath(source)), "sweeps.sha256")):
        digest, path = line.split()
        sums[path] = digest
    for name, path in (("fx", "sweep-base/fx.txt"), ("before", "sweep-base/xfx.txt"), ("after", "sweep-head/xfx.txt"),
                       ("tmux", "sweep-base/tmux.txt"), ("ghostty", "sweep-base/ghostty.txt")):
        digest = hashlib.sha256("".join(texts[name]).encode()).hexdigest()
        print(("ok   " if digest == sums[path] else "FAIL ") + f"the packed column {name} is {path}, sha256 {digest[:12]}")
        if digest != sums[path]:
            sys.exit(1)
else:
    base, head, base_table_rs, head_wrap_rs = sys.argv[1:5]


def load(directory, name):
    out = {}
    for line in open(f"{directory}/{name}.txt"):
        if line.strip():
            code, cells = line.split()
            out[int(code, 16)] = int(cells)
    return out


def table(source, name):
    body = source.split(f"const {name}")[1].split("];")[0]
    pairs = re.findall(r"\(0x([0-9a-f]+), 0x([0-9a-f]+)\)", body)
    return {c for a, b in pairs for c in range(int(a, 16), int(b, 16) + 1)}


def ranges(codes):
    out = []
    for c in sorted(codes):
        if out and out[-1][1] + 1 == c:
            out[-1][1] = c
        else:
            out.append([c, c])
    return out


def spell(codes):
    return ", ".join(f"U+{a:04X}" + (f"-{b:04X}" if b != a else "") for a, b in ranges(codes))


if packed:
    fx, before, after = columns["fx"], columns["before"], columns["after"]
    tmux, ghostty = columns["tmux"], columns["ghostty"]
    tmux2, ghostty2 = tmux, ghostty
else:
    fx = load(base, "fx")
    before, after = load(base, "xfx"), load(head, "xfx")
    tmux, ghostty = load(base, "tmux"), load(base, "ghostty")
    tmux2, ghostty2 = load(head, "tmux"), load(head, "ghostty")
failed = []


def claim(ok, text):
    print(("ok   " if ok else "FAIL ") + text)
    if not ok:
        failed.append(text)


scalars = set(fx)
claim(scalars == set(before) == set(after) == set(tmux) == set(ghostty), f"{len(scalars):,} scalars in every column")
claim(tmux == tmux2 and ghostty == ghostty2, "tmux and libghostty-vt drew every scalar the same in both sweeps")

old_table = table(open(base_table_rs).read(), "DRAWN_WIDER")
measured_wider = {c for c in scalars if max(tmux[c], ghostty[c]) > before[c]}
claim(old_table == measured_wider, f"the base table ({len(old_table):,} scalars) is the base sweep's scalars drawn wider than xfx counted")

p1 = {c for c in old_table if fx[c] > before[c]}
rest = old_table - p1
print(f"     P1: {len(p1):,} scalars in {len(ranges(p1))} ranges (the table's, fx counts more than xfx did)")
print(f"     left out: {len(rest)} scalars in {len(ranges(rest))} ranges, fx's count no more than xfx's: {spell(rest)}")
ledger = p1 - set(range(0x1F1E6, 0x1F200))
print(f"     P1 without U+1F1E6-1F1FF: {len(ledger):,} scalars in {len(ranges(ledger))} ranges (the ledger's selector)")

fx_counted = table(open(head_wrap_rs).read(), "FX_COUNTED")
claim(fx_counted == p1, "wrap.rs's FX_COUNTED is P1, exactly")

moved = {c for c in scalars if before[c] != after[c]}
claim(moved == p1, f"the scalars whose count moved ({len(moved):,}) are P1, exactly")
claim(all(after[c] == fx[c] for c in p1), "every scalar of P1 now counts fx's count")
claim(all(after[c] > before[c] for c in moved), "every count that moved grew")
over_table = {c for c in p1 if after[c] != fx[c]}
claim(not over_table, "no scalar of P1 counts more than fx because the width table counts it more")

under = {c for c in p1 if max(tmux[c], ghostty[c]) > fx[c]}
claim(not under, "no scalar of P1 is drawn wider than fx's count by either terminal")
buckets = {}
for c in sorted(p1):
    key = (
        "tmux " + ("exact" if tmux[c] == fx[c] else "narrower"),
        "libghostty-vt " + ("exact" if ghostty[c] == fx[c] else "narrower"),
    )
    buckets.setdefault(key, []).append(c)
for key, codes in sorted(buckets.items()):
    print(f"     P1, {key[0]}, {key[1]}: {len(codes):,} (e.g. U+{codes[0]:04X})")
print("     before -> after (fx) for P1: " + ", ".join(
    f"{n:,} x {a}->{b}" for (a, b), n in sorted(
        {(a, b): sum(1 for c in p1 if before[c] == a and after[c] == b) for a, b in {(before[c], after[c]) for c in p1}}.items()
    )
))

still = {c for c in scalars if max(tmux[c], ghostty[c]) > after[c]}
claim(still == rest, f"drawn wider than xfx's count at the head: {len(still)} scalars, the left-out family exactly")
for c in sorted(rest):
    pass
for a, b in ranges(rest):
    print(f"     U+{a:04X}" + (f"-{b:04X}" if b != a else "") + f": fx {fx[a]}, xfx {after[a]}, tmux {tmux[a]}, libghostty-vt {ghostty[a]}")

sys.exit(1 if failed else 0)
