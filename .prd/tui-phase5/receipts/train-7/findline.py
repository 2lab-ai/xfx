#!/usr/bin/env python3
"""findline.py <rev:path> <line> <rev:path> <line> ... -- <worktree path>
For each (rev, line): the cited line and two either side on rev, and where
that 5-line window (or failing that the cited line alone) is in the worktree
file."""
import subprocess, sys
args = sys.argv[1:]
cut = args.index('--')
pairs, target = args[:cut], args[cut+1]
W = open(target).read().split('\n')
for spec, line in zip(pairs[::2], pairs[1::2]):
    n = int(line)
    S = subprocess.run(['git', 'show', spec], capture_output=True, text=True, check=True).stdout.split('\n')
    win = S[n-3:n+2]
    hits = [i+3 for i in range(len(W)-4) if W[i:i+5] == win]
    single = [i+1 for i, x in enumerate(W) if x == S[n-1]]
    print(f'{spec}:{n} {S[n-1].strip()[:90]!r}\n  window hits in worktree: {hits}; line hits: {single[:12]}')
