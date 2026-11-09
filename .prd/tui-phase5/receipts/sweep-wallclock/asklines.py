"""Every catalog line the fake gateway logged, in each sweep of a set
(sweeps.sh's run-<i> directories), for each scenario that keeps its gateway
log (`gwlog`): the asks as they came (with `modelsdelay`, a line when asked
and one when answered) and the `request` line each answered ask gets, which
verdicts.py's ask column is read from -- as they were logged (ms since that
gateway started; the path and the status, never a value). A read still on its
way when the log was copied has its asked line and no request line.

usage: python3 -I asklines.py <sweeps-root>
"""

import glob
import os
import re
import sys

root = sys.argv[1]
runs = [p for p in glob.glob(os.path.join(root, "run-*")) if os.path.isdir(p)]
for run in sorted(runs, key=lambda p: int(p.rsplit("run-", 1)[1])):
    i = run.rsplit("run-", 1)[1]
    for log in sorted(glob.glob(os.path.join(run, "runs", "*", "*", "*-gateway.log"))):
        side = os.path.basename(os.path.dirname(log))
        scen = os.path.basename(os.path.dirname(os.path.dirname(log)))
        with open(log, encoding="utf-8", errors="replace") as f:
            for line in f:
                if re.search(r"GET /coding-agent/v1/models[ ?]", line):
                    print("run %s %s %s: %s" % (i, scen, side, line.rstrip("\n")))
