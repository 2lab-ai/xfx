"""How often a sweep meets the capture-trim class: a row whose `capture-pane -e`
text ends in an SGR that leaves reverse video, a background or an underline on,
with no cell after it -- tmux trimmed the blank that SGR painted, so
`cells.py`'s `parse`, which adds a cell only for a character, sees none.

For every fx/xfx snap pair of each sweep given (its `runs/`), the rows that
differ through `cells.py` as it is, and again with each such row given the one
styled blank its trailing SGR stands for. Prints each pair whose differing rows
change, shrink or grow.

usage: python3 -I trim_scan.py <repo root> <sweep dir>...   (the dirs sweep.sh --out wrote)
"""
import glob, importlib.util, os, re, sys

sys.dont_write_bytecode = True
root, sweeps = sys.argv[1], sys.argv[2:]
spec = importlib.util.spec_from_file_location("cells", os.path.join(root, "scripts/fxcap/cells.py"))
cells = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cells)
TAIL = re.compile(r"(?:\x1b\[[0-9;:]*m)+$")


def with_blank(text):
    out = []
    for line, row in zip(text.split("\n"), cells.parse(text)):
        if TAIL.search(line):
            state = {}
            for m in cells.SGR.finditer(line):
                cells.apply(state, m.group(1))
            if any(k in state for k in cells.SHOWS_ON_A_SPACE):
                col = row[-1][0] + cells.width(row[-1][1]) if row else 0
                row = row + [(col, " ", tuple(sorted(state.items(), key=str)))]
        out.append(row)
    return out


def differ(a, b):
    return [i + 1 for i in range(1, max(len(a), len(b)))
            if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]


for sweep in sweeps:
    shrink = grow = pairs = 0
    for fxf in sorted(glob.glob(os.path.join(sweep, "runs", "*", "fx", "*.ansi"))):
        if fxf.endswith(".full.ansi"):
            continue
        xf = fxf.replace(os.sep + "fx" + os.sep + "fx-", os.sep + "xfx" + os.sep + "xfx-")
        if not os.path.exists(xf):
            continue
        pairs += 1
        a = open(fxf, encoding="utf-8").read().replace("/.fx/", "/.xfx/")
        b = open(xf, encoding="utf-8").read()
        before, after = differ(cells.parse(a), cells.parse(b)), differ(with_blank(a), with_blank(b))
        if before != after:
            if len(after) < len(before):
                shrink += 1
            else:
                grow += 1
            print("  %s: rows %s -> %s" % (os.path.basename(fxf)[3:-5], before, after))
    print("%s: %d pairs, %d shrink, %d grow" % (os.path.basename(sweep.rstrip("/")), pairs, shrink, grow))
