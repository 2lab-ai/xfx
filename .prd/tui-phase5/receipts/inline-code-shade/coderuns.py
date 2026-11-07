"""Inline-code runs in a capture: every run of text written under one 256-colour
foreground whose text is one of the scenario's code spans, as (fg, text), in
screen order. usage: python3 -I coderuns.py <fx.ansi> <xfx.ansi>"""
import re, sys
SPANS = re.compile(r"^(zig build test|src/ui/render\.zig|cmd-\d+ --flag|code\d+|inline|everything)$")
SGR = re.compile(r"\x1b\[([0-9;]*)m")
def runs(path):
    data = open(path, encoding="utf-8", errors="replace").read()
    out, fg, pos = [], None, 0
    for m in SGR.finditer(data):
        text = data[pos:m.start()]
        if fg is not None and SPANS.match(text):
            out.append((fg, text))
        p = m.group(1)
        if p in ("", "0", "39"):
            fg = None
        elif p.startswith("38;5;"):
            fg = int(p[5:])
        pos = m.end()
    return out
a, b = runs(sys.argv[1]), runs(sys.argv[2])
print(f"fx {len(a)} runs, colours {sorted(set(f for f, _ in a))}; xfx {len(b)} runs, colours {sorted(set(f for f, _ in b))}")
print("SAME" if a == b else "DIFF")
if a != b:
    for x, y in zip(a, b):
        if x != y: print(" ", x, y)
    print(" lengths", len(a), len(b))
