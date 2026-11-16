"""LOCAL-PATHS-WRAP: the committed captures in which a wrap left the scenario
directory's tail on the row after its redacted head (`<local-path>`), the
tail written as `<scenarios>`, the token scripts/fxcap/redact.py writes for
that directory whole (the same captures at 40x120: `↳ <scenarios>/p5-mcp-stdio.ts`).

usage: python3 -I fix_tails.py <tree>

- mcp-menu/xfx-base, old xfx's trust prompt at 40x60: the row
  `↳ cripts/fxcap/scenarios/p5-mcp-stdio.ts` under `↳ <local-path>`.
- mcp-menu/xfx-head, MCP-M's approval at 40x60, and its fx pair (fx/), so
  the pair reads as it did: `/scripts/fxcap/scenarios/p5-mcp-stdio.ts` at
  the start of the row under `<local-path>`, behind `  ↳ ` or four spaces.

Prints each directory's count of files and rows changed; refuses (exit 1)
where a file holds no such row, or the tail anywhere else.
"""
import glob
import os
import re
import sys

tree = sys.argv[1]
base = os.path.join(tree, ".prd", "tui-phase5", "receipts", "mcp-menu")
TAIL = rb"/?s?cripts/fxcap/scenarios(?=/p5-mcp-stdio\.ts\n)"
ROW = re.compile(rb"(?m)^((?:  \xe2\x86\xb3 |\xe2\x86\xb3 |    ))" + TAIL)
sets = {
    "xfx-base": sorted(glob.glob(os.path.join(base, "xfx-base", "*-40x60-*.txt"))),
    "xfx-head": sorted(glob.glob(os.path.join(base, "xfx-head", "xfx-p5-mcp-menu-new-40x60-approve-confirm.*"))),
    "fx": sorted(glob.glob(os.path.join(base, "fx", "fx-p5-mcp-menu-new-40x60-approve-confirm.*"))),
}
bad = 0
for name, files in sets.items():
    changed = rows = 0
    for path in files:
        with open(path, "rb") as f:
            data = f.read()
        new, n = ROW.subn(rb"\1<scenarios>", data)
        if b"cripts/fxcap/scenarios" in new:
            print("%s: the tail is still there, elsewhere" % os.path.relpath(path, tree))
            bad += 1
        if n:
            with open(path, "wb") as f:
                f.write(new)
            changed, rows = changed + 1, rows + n
    print("%s: %d of %d file(s) changed, %d row(s)" % (name, changed, len(files), rows))
sys.exit(1 if bad else 0)
