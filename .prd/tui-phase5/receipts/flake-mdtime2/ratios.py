"""Summarizes the `linear-time` lines the markdown linear-time tests print.

    python3 ratios.py <log>...

Each log is one run of a test binary with `--show-output` (or `--nocapture`).
It reads both forms of the line:

- this check's: `linear-time <shape>: round <r>, 16 x<n/16> <time>, x<n> <time>,
  ratio <r> (limit 4)`, where the short time is 16 short lines pushed at once;
- flake-mdtime's: `linear-time <shape>: x<n/4> <time>, x<n> <time>, ratio <r>
  (limit 8)`, one short line.

A GitHub Actions timestamp in front of a line is skipped.

For every shape it prints how many runs read it, the smallest, median and
largest first-round ratio, how far the largest is above what a linear render
reads (1 for this check, 4 for the old one), the margin from the largest to
the limit, the cheapest short-side time, and how many runs needed a second
round. A run whose log has no line for a shape is counted as missing, so a
renamed shape cannot pass by being absent.
"""

import re
import statistics
import sys

NEW = re.compile(
    r"^(?:\S+Z )?linear-time (?P<label>.*): round (?P<round>\d+), (?P<g>\d+) x(?P<sn>\d+) "
    r"(?P<st>\S+), x(?P<n>\d+) (?P<lt>\S+), ratio (?P<ratio>[0-9.]+) \(limit (?P<limit>[0-9.]+)\)$"
)
OLD = re.compile(
    r"^(?:\S+Z )?linear-time (?P<label>.*): x(?P<sn>\d+) (?P<st>\S+), x(?P<n>\d+) "
    r"(?P<lt>\S+), ratio (?P<ratio>[0-9.]+) \(limit (?P<limit>[0-9.]+)\)$"
)
UNITS = {"ns": 1e-9, "µs": 1e-6, "ms": 1e-3, "s": 1.0}
DURATION = re.compile(r"^(?P<value>[0-9.]+)(?P<unit>ns|µs|ms|s)$")


def seconds(text):
    m = DURATION.match(text)
    if not m:
        raise ValueError(f"not a duration: {text!r}")
    return float(m["value"]) * UNITS[m["unit"]]


def show(value):
    for unit, scale in (("s", 1.0), ("ms", 1e-3), ("µs", 1e-6)):
        if value >= scale:
            return f"{value / scale:.3g} {unit}"
    return f"{value / 1e-9:.3g} ns"


def main(paths):
    first = {}  # label -> list of (ratio, linear, short seconds), one per run
    retried = {}
    order = []
    limit = None
    for path in paths:
        seen = set()
        with open(path, encoding="utf-8", errors="replace") as f:
            for raw in f:
                line = raw.rstrip("\n")
                m = NEW.match(line)
                if m:
                    linear = int(m["n"]) / (int(m["g"]) * int(m["sn"]))
                    rnd = int(m["round"])
                else:
                    m = OLD.match(line)
                    if not m:
                        continue
                    linear = int(m["n"]) / int(m["sn"])
                    rnd = 1
                label = m["label"]
                limit = float(m["limit"])
                if label not in first:
                    first[label] = []
                    retried[label] = 0
                    order.append(label)
                if rnd == 1:
                    first[label].append((float(m["ratio"]), linear, seconds(m["st"])))
                    seen.add(label)
                elif rnd == 2:
                    retried[label] += 1
    if not order:
        sys.exit("no linear-time lines")
    print(f"{len(paths)} logs, limit {limit:g}")
    print(
        f"{'shape':<46} {'runs':>4} {'min':>5} {'median':>6} {'max':>5} "
        f"{'max/lin':>7} {'lim/max':>7} {'min base':>9} {'retried':>7}"
    )
    worst = 0.0
    for label in order:
        values = first[label]
        ratios = [r for r, _, _ in values]
        top = max(ratios)
        linear = values[0][1]
        worst = max(worst, top / linear)
        base = min(s for _, _, s in values)
        print(
            f"{label:<46} {len(values):>4} {min(ratios):>5.2f} {statistics.median(ratios):>6.2f} "
            f"{top:>5.2f} {top / linear:>7.2f} {limit / top:>7.2f} {show(base):>9} {retried[label]:>7}"
        )
    print(f"largest reading over linear {worst:.2f}x")
    missing = [f"{label} ({len(paths) - len(first[label])})" for label in order if len(first[label]) != len(paths)]
    if missing:
        print("shapes missing from some logs: " + ", ".join(missing))
        sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])
