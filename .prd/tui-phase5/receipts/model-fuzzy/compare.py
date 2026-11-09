"""Base vs head sweep: every snap pair's verdict on each side, what moved, and
whether any pair dropped. usage: python3 -I compare.py <base pairs.json> <head pairs.json>"""
import json, sys, collections
GOOD = ('identical', 'normalized-identical')
def load(p):
    return {(x['scenario'], x['label']): x for x in json.load(open(p))}
b, h = load(sys.argv[1]), load(sys.argv[2])
keys = sorted(set(b) | set(h))
def v(d, k): return d[k]['verdict'] if k in d else 'absent'
cb = collections.Counter(v(b, k) for k in keys); ch = collections.Counter(v(h, k) for k in keys)
good_b = sum(cb[g] for g in GOOD); good_h = sum(ch[g] for g in GOOD)
print(f'pairs: {len(keys)}  base pass {good_b}/{len(keys)}  head pass {good_h}/{len(keys)}')
print('base verdicts:', dict(cb)); print('head verdicts:', dict(ch))
up, down, changed = [], [], []
for k in keys:
    vb, vh = v(b, k), v(h, k)
    if vb == vh:
        if vb not in GOOD and b[k]['rows'] != h[k]['rows']:
            changed.append(k)
        continue
    (up if vh in GOOD else down if vb in GOOD else changed).append(k)
print(f'\nmoved to identical/normalized ({len(up)}):')
for s in sorted({k[0] for k in up}):
    print(f'  {s}: ' + ', '.join(l for (sc, l) in up if sc == s))
print(f'\ndropped from identical/normalized ({len(down)}):')
for k in down: print(f'  {k[0]} {k[1]}: {v(b,k)} -> {v(h,k)}')
print(f'\nstill unresolved on both, rows changed ({len(changed)}):')
for k in changed: print(f'  {k[0]} {k[1]}: {v(b,k)} -> {v(h,k)}')
same_bad = [k for k in keys if v(b,k) == v(h,k) and v(b,k) not in GOOD and k not in changed]
print(f'\nunresolved on both, the same rows ({len(same_bad)}):')
for s in sorted({k[0] for k in same_bad}):
    print(f'  {s}: ' + ', '.join(l for (sc, l) in same_bad if sc == s))
