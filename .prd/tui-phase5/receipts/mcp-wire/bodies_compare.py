"""Compares what fx and xfx tell the model about MCP, request by request.

usage: python3 -I bodies_compare.py <fx-bodies-dir> <xfx-bodies-dir>

Pairs the completion requests in order (title requests set apart) and, for
each pair, compares byte for byte (as JSON text, keys in the order sent):
  - the MCP tools advertised (the meta-tools and every `mcp_*` tool), whole;
  - the `<mcp_servers>` system block;
  - every tool call and tool result the model is sent.
Nothing is normalized. The two deliberate advertisement differences M1
recorded (capability_search's description, mcp_select_tool's closed schema)
are reported as such, by key, not hidden.
"""
import json, os, re, sys

META = ("capability_search", "mcp_select_tool", "mcp_features")
KNOWN = {("capability_search", "description"), ("mcp_select_tool", "inputSchema")}


def completions(d):
    return sorted(f for f in os.listdir(d) if f.endswith("-completion.json"))


def mcp_tools(body):
    out = []
    for tool in body.get("tools", []):
        name = tool.get("name", "")
        if name in META or name.startswith("mcp_"):
            out.append(tool)
    return out


def blocks(body):
    found = []
    for message in body.get("prompt", []):
        content = message.get("content")
        if isinstance(content, str):
            found += re.findall(r"<mcp_servers>.*?</mcp_servers>", content, re.S)
    return found


def parts(body):
    out = []
    for message in body.get("prompt", []):
        content = message.get("content")
        if isinstance(content, list):
            for part in content:
                if part.get("type") in ("tool-call", "tool-result"):
                    out.append(json.dumps(part, ensure_ascii=False))
    return out


def main(fx_dir, xfx_dir):
    fx, xfx = completions(fx_dir), completions(xfx_dir)
    print(f"requests: fx={len(fx)} xfx={len(xfx)}")
    worst = 0 if len(fx) == len(xfx) else 1
    for a, b in zip(fx, xfx):
        A = json.load(open(os.path.join(fx_dir, a)))
        B = json.load(open(os.path.join(xfx_dir, b)))
        ta, tb = mcp_tools(A), mcp_tools(B)
        na, nb = [t["name"] for t in ta], [t["name"] for t in tb]
        print(f"{a} vs {b}")
        print(f"  mcp tools same names: {na == nb}  fx={na} xfx={nb}")
        if na != nb:
            worst = 1
        for t in ta:
            other = next((u for u in tb if u["name"] == t["name"]), None)
            if other is None:
                continue
            keys = sorted(k for k in set(t) | set(other) if t.get(k) != other.get(k))
            if not keys:
                print(f"    {t['name']}: identical")
                continue
            unexplained = [k for k in keys if (t["name"], k) not in KNOWN]
            print(f"    {t['name']}: differs in {keys}" + ("" if not unexplained else f"  UNEXPLAINED {unexplained}"))
            if unexplained:
                worst = 1
        ba, bb = blocks(A), blocks(B)
        same = ba == bb
        print(f"  <mcp_servers> same: {same}  ({len(ba)} vs {len(bb)})")
        if not same:
            worst = 1
            for x in ba:
                print("    fx : " + x.replace("\n", "\\n"))
            for x in bb:
                print("    xfx: " + x.replace("\n", "\\n"))
        pa, pb = parts(A), parts(B)
        same = pa == pb
        print(f"  tool calls/results same: {same}  ({len(pa)} vs {len(pb)})")
        if not same:
            worst = 1
            for x, y in zip(pa, pb):
                if x != y:
                    print("    fx : " + x)
                    print("    xfx: " + y)
            for x in pa[len(pb):]:
                print("    fx only : " + x)
            for y in pb[len(pa):]:
                print("    xfx only: " + y)
    print("RESULT:", "identical (but the known differences)" if worst == 0 else "DIFFERS")
    return worst


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
