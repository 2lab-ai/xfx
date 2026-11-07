"""Notice-by-notice comparison of two end.full.txt transcripts: each notice
(a block of rows between blank rows) of fx, with fx's own name read as xfx,
matched in order against xfx's. usage: python3 -I notices.py <fx> <xfx>"""
import re, sys
def blocks(path, fx):
    text = open(path, encoding="utf-8").read()
    rows = [r.rstrip() for r in text.split("\n")]
    out, cur = [], []
    for r in rows[1:]:  # row 1: the banner
        if r.startswith("┃") or r.startswith("auto ·"):
            break
        if r == "":
            if cur: out.append("\n".join(cur)); cur = []
            continue
        if fx:
            r = r.replace("/.fx/", "/.xfx/")
            r = re.sub(r"\bfx\b", "xfx", r)
        cur.append(r)
    if cur: out.append("\n".join(cur))
    return out
f, x = blocks(sys.argv[1], True), blocks(sys.argv[2], False)
import difflib
sm = difflib.SequenceMatcher(a=f, b=x, autojunk=False)
same = sum(b.size for b in sm.get_matching_blocks())
print(f"fx notices {len(f)}, xfx notices {len(x)}, identical in order {same}")
for tag, i1, i2, j1, j2 in sm.get_opcodes():
    if tag == "equal": continue
    for k in range(i1, i2): print(f"--- fx only [{k}]\n{f[k]}")
    for k in range(j1, j2): print(f"+++ xfx only [{k}]\n{x[k]}")
