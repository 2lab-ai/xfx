import re, sys, statistics, collections
rows = collections.defaultdict(list)
for path in sys.argv[1:]:
    for line in open(path):
        m = re.match(r"(\S+) run\d PERF turns=(\d+) calls=(\d+): hydrate ([\d.]+) ms; queued ops (\d+) blocks (\d+) row-bytes (\d+) drawing-bytes (\d+); rows\(\) calls (\d+), highlight calls (\d+); drain ([\d.]+) ms, (\d+) bytes written", line)
        if not m:
            continue
        who, t, c, hyd, ops, blocks, rb, db, rc, hc, drain, wrote = m.groups()
        rows[(int(t), int(c), who)].append((float(hyd), int(ops), int(blocks), int(rb) + int(db), int(rc), int(hc), float(drain), int(wrote)))
print("| session | build | blocks queued | bytes held (rows + drawings) | `rows()` calls | highlighter calls | hydrate ms (median of 3) | drain ms (median of 3) | bytes written by the drain |")
print("|---|---|---|---|---|---|---|---|---|")
order = {"base-58b463c8": 0, "a765487d": 1, "head-f482d340": 2}
for key in sorted(rows, key=lambda k: ([(20, 10), (20, 50), (5, 200)].index((k[0], k[1])), order.get(k[2], 9))):
    t, c, who = key
    v = rows[key]
    hyd = statistics.median(x[0] for x in v); drain = statistics.median(x[6] for x in v)
    x = v[0]
    print("| %d turns x %d calls | `%s` | %d | %s | %d | %d | %.0f | %.0f | %s |" % (t, c, who.replace("base-", "").replace("head-", ""), x[2], format(x[3], ","), x[4], x[5], hyd, drain, format(x[7], ",")))
