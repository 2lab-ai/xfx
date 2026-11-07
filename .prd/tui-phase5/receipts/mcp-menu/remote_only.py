"""For each pair of a run that differs, whether the difference is only
`remote`'s connecting/retrying (MCP-S's transport timing): xfx's side with
`retrying` read as `connecting` and its `error` row for the refused connect
taken out, then compared as tdiff does (banner and fx->xfx normalized)."""
import os, re, sys
fxdir, xdir, scen, size, cap = sys.argv[1:6]
labels = [m.group(2) + ('.full' if m.group(1) else '') for m in (re.match(r'^snap(full)? (\S+)', l) for l in open(cap)) if m]
def norm(rows, fx):
    out = []
    for r in rows:
        r = r.rstrip()
        if re.match(r'^x?𝒇x v\S+.*Run /help for commands$', r): r = '<banner>'
        elif fx: r = re.sub(r'\bfx\b', 'xfx', r)
        out.append(r)
    return out
only = bad = same = 0
for l in labels:
    f = norm(open(f'{fxdir}/fx-{scen}-{size}-{l}.txt').read().split('\n'), True)
    x = norm(open(f'{xdir}/xfx-{scen}-{size}-{l}.txt').read().split('\n'), False)
    if f == x: same += 1; continue
    y = [r for r in x if not r.startswith("  error     it refused or never answered")]
    y = [re.sub(r'retrying(  )?', lambda m: 'connecting', r) if 'remote' in r or r.startswith('  status') else r for r in y]
    n = max(len(f), len(y)); f2 = f + [''] * (n - len(f)); y += [''] * (n - len(y))
    if f2 == y: only += 1
    else:
        bad += 1; print('OTHER', l, [(i, a, b) for i, (a, b) in enumerate(zip(f2, y)) if a != b][:3])
print(f'{scen} {size}: identical {same}, remote-status only {only}, other {bad}, of {len(labels)}')
