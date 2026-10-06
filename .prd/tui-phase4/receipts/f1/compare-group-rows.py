import sys, re, difflib
d = sys.argv[1]  # this directory
snaps = ["cmd-ask","cmd-running","edit-ask","write-ask","exit-ask","end","again-ask","again-stopped","long-ask","long-running","long-stopped","long-later","end.full"]
marks = ("●","├","└","■")
strip = re.compile(r"\x1b\[[0-9;]*m")
def rows(path):
    return [l for l in open(path, encoding="utf-8").read().split("\n") if strip.sub("", l).startswith(marks)]
same = total = 0
for snap in snaps:
    fx = rows(f"{d}/fx-toolgroup-40x120-{snap}.ansi"); xfx = rows(f"{d}/xfx-toolgroup-40x120-{snap}.ansi")
    m = difflib.SequenceMatcher(a=fx, b=xfx, autojunk=False)
    eq = sum(b.size for b in m.get_matching_blocks())
    same += eq; total += max(len(fx), len(xfx))
    print(f"{snap}: fx {len(fx)} group rows, xfx {len(xfx)}, byte-identical {eq}")
    for tag, i1, i2, j1, j2 in m.get_opcodes():
        if tag == "equal": continue
        for a in fx[i1:i2]: print("   fx : " + a.replace("\x1b","ESC"))
        for b in xfx[j1:j2]: print("   xfx: " + b.replace("\x1b","ESC"))
print(f"TOTAL byte-identical group rows (tmux-reserialized ANSI): {same}/{total}")
