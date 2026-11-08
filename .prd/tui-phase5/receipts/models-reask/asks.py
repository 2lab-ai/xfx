#!/usr/bin/env python3
"""Every catalog ask each side's fake gateway logged, per scenario of a sweep:
when it was asked (ms after the gateway started) and how it was answered, and
the turn requests' times beside them. usage: asks.py <sweep-dir> <scenario>..."""
import os, re, sys
sweep = sys.argv[1]
for sc in sys.argv[2:]:
    for side in ("fx", "xfx"):
        d = os.path.join(sweep, "runs", sc, side)
        log = os.path.join(d, f"{side}-{sc}-40x120-gateway.log")
        if not os.path.exists(log):
            print(f"{sc} {side}: no gateway log (the run did not reach `gwlog`)")
            continue
        asks, posts, second = [], [], False
        for line in open(log, encoding="utf-8", errors="replace"):
            m = re.match(r"\[fakegw\] request GET /coding-agent/v1/models at=(\d+)ms -> (\d+) at=(\d+)ms", line)
            if m:
                asks.append(f"{m.group(1)} ms -> {m.group(2)} at {m.group(3)} ms" + (" (second catalog)" if second else ""))
                second = False
            m = re.match(r"\[fakegw\] request POST (\S+) at=(\d+)ms", line)
            if m:
                posts.append(f"{m.group(2)} ms")
            if "-> second catalog" in line:
                second = True
        print(f"{sc} {side}: catalog asks: {'; '.join(asks) or 'none'}" + (f" | POSTs: {', '.join(posts)}" if posts else ""))
