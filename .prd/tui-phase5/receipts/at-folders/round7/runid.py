"""compare: cells.py's comparison with each capture's run-directory name
(the six characters after `fxcap.`) written as XXXXXX, cell by cell, styles
kept. redact: the scratch root and the run directory written as <tmp>, with
any SGR sequence inside kept in order after it."""
import importlib.util, re, sys
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("cells", sys.argv[2])
cells = importlib.util.module_from_spec(spec); spec.loader.exec_module(cells)

def masked(path, product):
    out = []
    for row in cells.rows(path, product):
        text = "".join(ch for _, ch, _ in row)
        hide = set()
        for m in re.finditer(r"fxcap\.([A-Za-z0-9]{6})", text):
            hide.update(range(m.start(1), m.end(1)))
        out.append([(c, "X" if i in hide else ch, s) for i, (c, ch, s) in enumerate(row)])
    return out

def compare(fx, xfx):
    a, b = masked(fx, True), masked(xfx, False)
    differ = 0
    for i in range(1, max(len(a), len(b))):
        p = a[i] if i < len(a) else []
        q = b[i] if i < len(b) else []
        if p != q:
            differ += 1
            print("row %d differs" % (i + 1))
    print("%d rows differ in their cells (row 1 not compared; run-directory names masked)" % differ)

SGR = r"(?:\x1b\[[0-9;]*m)*"
def redact(src, dst, root):
    text = open(src, encoding="utf-8").read()
    lit = root + "/fxcap."
    pat = "".join(re.escape(ch) + SGR for ch in lit) + "".join("[A-Za-z0-9]" + SGR for _ in range(6))
    def sub(m):
        return "<tmp>" + "".join(re.findall(r"\x1b\[[0-9;]*m", m.group(0)))
    text = re.sub(pat, sub, text)
    open(dst, "w", encoding="utf-8").write(text)

if sys.argv[1] == "compare":
    compare(sys.argv[3], sys.argv[4])
else:
    redact(sys.argv[3], sys.argv[4], sys.argv[5])
