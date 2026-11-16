#!/usr/bin/env python3
"""Can the same-load control tell the two builds' rejection rates apart?
Reads the rejected and total sessions from each build's counts file
(round3/count_paced.py's output, every session kind summed) and gives
Fisher's exact two-sided p for the 2x2 table, and the smallest increase in
rejected sessions at the joined build the run could have shown at p < 0.05
with the same totals. Prints no path.

    python3 -I -B control_test.py <counts-pre> <counts-head>
"""
import math
import re
import sys


def tally(path):
    rows = re.findall(r"sessions with pauses of \S+: (\d+), accepted \d+, rejected (\d+)", open(path).read())
    return sum(int(r) for _, r in rows), sum(int(n) for n, _ in rows)


def fisher(a, n1, b, n2):
    """Two-sided p: every table with these margins at most as likely as this one."""
    k, n = a + b, n1 + n2
    def prob(x):
        return math.comb(n1, x) * math.comb(n2, k - x) / math.comb(n, k)
    here = prob(a)
    lo, hi = max(0, k - n2), min(k, n1)
    return min(1.0, sum(prob(x) for x in range(lo, hi + 1) if prob(x) <= here * (1 + 1e-9)))


(pre_r, pre_n), (head_r, head_n) = tally(sys.argv[1]), tally(sys.argv[2])
print(f"pre (ef595df2): {pre_r} of {pre_n} rejected ({100 * pre_r / pre_n:.1f}%)")
print(f"head (ba133c25): {head_r} of {head_n} rejected ({100 * head_r / head_n:.1f}%)")
print(f"Fisher's exact two-sided p: {fisher(pre_r, pre_n, head_r, head_n):.2f}")
smallest = next((x for x in range(head_n + 1) if x > head_n * pre_r / pre_n and fisher(pre_r, pre_n, x, head_n) < 0.05), None)
if smallest is not None:
    print(f"an increase the run could have shown at p < 0.05: head rejecting {smallest} of {head_n} "
          f"({100 * smallest / head_n:.1f}%) or more, against pre's {100 * pre_r / pre_n:.1f}%")
