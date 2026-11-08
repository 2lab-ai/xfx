"""Every fx line a text cites (`<name>.zig:N` / `:N-M`, `<name>.ts:N-M`), against the
blocks fx-src.txt holds. usage: python3 -I fxcites.py <fx-src.txt> <file>...
A bare `:N-M` after a cite is read against the last file named."""
import re, sys
blocks = {}
for m in re.finditer(r"^== (\S+):(\d+)-(\d+)@481104fd", open(sys.argv[1]).read(), re.M):
    blocks.setdefault(m.group(1).split("/")[-1], []).append((int(m.group(2)), int(m.group(3))))
missing = []
for path in sys.argv[2:]:
    text = open(path).read()
    for line_no, line in enumerate(text.splitlines(), 1):
        last = None
        for m in re.finditer(r"([A-Za-z_./-]+\.(?:zig|ts))?:(\d+)(?:-(\d+))?", line):
            name = m.group(1)
            if name:
                last = name.split("/")[-1]
            elif last is None:
                continue
            lo = int(m.group(2)); hi = int(m.group(3) or lo)
            if last not in blocks and last not in ("compaction_activity.zig",):
                pass
            covered = any(a <= lo and hi <= b for a, b in blocks.get(last, []))
            if not covered:
                missing.append(f"{path.split('/')[-1]}:{line_no}: {last}:{lo}-{hi}")
for m in missing:
    print(m)
print(f"{len(missing)} cite(s) outside fx-src.txt")
