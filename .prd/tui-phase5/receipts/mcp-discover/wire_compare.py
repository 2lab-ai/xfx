"""Compares the frames fx and xfx sent a stdio fixture, frame by frame.

usage: python3 -I wire_compare.py <fx .files> <xfx .files>

Reads the `~/p5-mcp-wire.ndjson` section of each capture's `.files` dump (the
fixture's log, `MCPSTDIO_WIRE_LOG`) and compares the frames in order, byte for
byte as compact JSON, after one rewrite on both sides: who the client is --
`clientInfo` and `io.modelcontextprotocol/clientInfo` -- and what it declares --
`capabilities` and `io.modelcontextprotocol/clientCapabilities` -- become
`<client>` and `<capabilities>`. Those are M1's two approved differences (xfx
is not fx, and answers no elicitation, `docs/parity.md` MCP client runtime).
Everything else -- order, ids, methods, `_meta`, params -- is compared.
"""
import json, sys

CLIENT = ("clientInfo", "io.modelcontextprotocol/clientInfo")
CAPS = ("capabilities", "io.modelcontextprotocol/clientCapabilities")


def frames(path):
    out, keep = [], False
    for line in open(path, encoding="utf-8"):
        if line.startswith("=== "):
            keep = line.startswith("=== ~/p5-mcp-wire.ndjson")
            continue
        if keep and line.strip():
            out.append(json.loads(line)["message"])
    return out


def scrub(value):
    if isinstance(value, dict):
        return {
            k: ("<client>" if k in CLIENT else "<capabilities>" if k in CAPS else scrub(v))
            for k, v in value.items()
        }
    if isinstance(value, list):
        return [scrub(v) for v in value]
    return value


def text(message):
    return json.dumps(scrub(message), separators=(",", ":"), ensure_ascii=False)


fx, xfx = frames(sys.argv[1]), frames(sys.argv[2])
print(f"frames: fx={len(fx)} xfx={len(xfx)}")
same = True
for i in range(max(len(fx), len(xfx))):
    a = text(fx[i]) if i < len(fx) else "-"
    b = text(xfx[i]) if i < len(xfx) else "-"
    mark = "=" if a == b else "!"
    same = same and a == b
    print(f"{mark} {i + 1:2d} fx  {a}")
    if a != b:
        print(f"       xfx {b}")
print("RESULT: IDENTICAL" if same else "RESULT: DIFFERS")
