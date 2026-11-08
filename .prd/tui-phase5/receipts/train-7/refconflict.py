#!/usr/bin/env python3
"""Resolve conflict blocks whose sides differ only in path:line numbers.
usage: refconflict.py <file> <old>=<new>...
Merge or diff3 markers. Each block's sides (and base, if shown) must be equal
once every `.ext:N[-M]` is masked; the result is ours with each <old>
replaced by <new> (exact strings). Prints every block."""
import re, sys
path, *maps = sys.argv[1:]
maps = [m.split('=', 1) for m in maps]
mask = lambda t: re.sub(r':\d+(?:-\d+)?', ':N', t)
lines = open(path).read().split('\n')
out, i, n = [], 0, 0
while i < len(lines):
    if lines[i].startswith('<<<<<<< '):
        j = next(x for x in range(i, len(lines)) if lines[x] == '=======')
        k = next(x for x in range(j, len(lines)) if lines[x].startswith('>>>>>>> '))
        b = next((x for x in range(i, j) if lines[x].startswith('||||||| ')), None)
        ours = lines[i+1:(b if b is not None else j)]
        base = lines[b+1:j] if b is not None else None
        theirs = lines[j+1:k]
        for other in (theirs, base):
            if other is not None:
                assert [mask(x) for x in ours] == [mask(x) for x in other], (ours, other)
        res = []
        for x in ours:
            for a, c in maps:
                x = x.replace(a, c)
            res.append(x)
        print(f'block {n+1} (line {i+1}):\n  ours   {ours}\n  base   {base}\n  theirs {theirs}\n  ->     {res}')
        out += res; i = k + 1; n += 1
    else:
        out.append(lines[i]); i += 1
open(path, 'w').write('\n'.join(out))
print(f'{n} blocks')
