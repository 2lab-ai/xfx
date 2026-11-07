"""Every row of p5-mcp-verbs left unmatched once the two sides are aligned as
sequences (the sweep's `pairs.json` `unattributed`), by cause."""
import json, re, sys, collections
CAUSES = [
    ("C1 launch prompt", re.compile(r"Project MCP server|\[1\] approve|dismiss remaining|prompts dismissed|^  command: |^↳ ")),
    ("list table cell (SOURCE a cell wider)", re.compile(r"^\* mcp: NAME +SOURCE|^  \S+ +(~/\.xfx/mcp\.json|\.mcp\.json) +(stdio|http) ")),
    ("connecting vs retrying", re.compile(r"connecting|retrying|it refused or never answered")),
    ("show order (A12, typed order)", re.compile(r"^\* mcp: (fixture|remote|proj)$|^    (status|version|transport|command|url|source|error|next|tools) |^      echo|^  p5-mcp-stdio\.ts|^  <scenarios>|^    command ")),
    ("login gated", re.compile(r"Interactive MCP authentication is unavailable here|Couldn't sign in to remote")),
]
for path in sys.argv[1:]:
    pairs = json.load(open(path))
    count = collections.Counter(); other = []
    for p in pairs:
        if p["scenario"] != "p5-mcp-verbs":
            continue
        for side, row, text in p.get("unattributed", []):
            for name, pat in CAUSES:
                if pat.search(text):
                    count[name] += 1; break
            else:
                count["other"] += 1; other.append((p["label"], side, row, text))
    print(path.split("/")[-2], dict(count))
    for o in other[:30]:
        print("   other:", o)
