"""Summarizes the `linear-time` lines the markdown linear-time tests print.

    python3 ratios.py <log>...

Each log is one run of a test binary with `--show-output` (or `--nocapture`).
For every shape it prints how many readings there were, the smallest, median
and largest ratio, and the margin from the largest to the limit. A run whose
log has no `linear-time` line for a shape is counted, so a renamed shape
cannot pass by being missing.
"""

import re
import statistics
import sys

LINE = re.compile(r"^linear-time (?P<label>.*): x\d+ \S+, x\d+ \S+, ratio (?P<ratio>[0-9.]+) \(limit (?P<limit>[0-9.]+)\)$")


def main(paths):
    ratios = {}
    order = []
    limit = None
    for path in paths:
        with open(path, encoding="utf-8", errors="replace") as f:
            for raw in f:
                m = LINE.match(raw.rstrip("\n"))
                if not m:
                    continue
                label = m["label"]
                if label not in ratios:
                    ratios[label] = []
                    order.append(label)
                ratios[label].append(float(m["ratio"]))
                limit = float(m["limit"])
    if not order:
        sys.exit("no linear-time lines")
    print(f"{len(paths)} logs, limit {limit:g}")
    print(f"{'shape':<46} {'n':>4} {'min':>5} {'median':>6} {'max':>5} {'limit/max':>9}")
    worst = 0.0
    for label in order:
        values = ratios[label]
        top = max(values)
        worst = max(worst, top)
        print(f"{label:<46} {len(values):>4} {min(values):>5.2f} {statistics.median(values):>6.2f} {top:>5.2f} {limit / top:>9.2f}")
    missing = [label for label in order if len(ratios[label]) != len(paths)]
    print(f"largest ratio {worst:.2f}; margin to the limit {limit / worst:.2f}x")
    if missing:
        print("shapes missing from some logs: " + ", ".join(missing))
        sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])
