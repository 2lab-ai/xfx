"""Compares the frames fx and xfx sent the stdio fixture, in order.

usage: python3 -I wire_compare.py <fx .files> <xfx .files>

Reads the `~/p5-mcp-wire.ndjson` section of each capture's `.files` dump
(the fixture's log, `MCPSTDIO_WIRE_LOG`) and prints each frame side by side
by method, then whether each method's frames are byte-identical. Nothing is
normalized.
"""
import json, sys


def frames(path):
    out, keep = [], False
    for line in open(path, encoding="utf-8"):
        if line.startswith("=== "):
            keep = line.startswith("=== ~/p5-mcp-wire.ndjson")
            continue
        if keep and line.strip():
            out.append(json.loads(line)["message"])
    return out


def text(message):
    return json.dumps(message, separators=(",", ":"), ensure_ascii=False)


fx, xfx = frames(sys.argv[1]), frames(sys.argv[2])
print(f"frames: fx={len(fx)} xfx={len(xfx)}")
for side, items in (("fx ", fx), ("xfx", xfx)):
    for message in items:
        print(f"  {side} {text(message)}")
methods = []
for message in fx + xfx:
    if message.get("method") not in methods:
        methods.append(message.get("method"))
same_all = True
for method in methods:
    a = [m for m in fx if m.get("method") == method]
    b = [m for m in xfx if m.get("method") == method]
    same = [text(m) for m in a] == [text(m) for m in b]
    same_all &= same
    print(f"{method}: fx={len(a)} xfx={len(b)} identical={same}")
print("RESULT:", "identical" if same_all else "DIFFERS")
