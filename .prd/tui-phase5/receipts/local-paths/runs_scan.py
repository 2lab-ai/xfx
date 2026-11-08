"""Counts the capture files under a sweep's runs/ that hold this machine's
roots: the harness TMPDIR, the user's home, and each fxcap run directory's
prefix. Prints counts by file kind, never a path's text."""
import collections, os, re, sys
runs = sys.argv[1]
tmp = os.environ["SCAN_TMP"].encode()
home = os.environ["HOME"].encode()
# Bracketed, so this tracked file does not match its own pattern.
shape = re.compile(rb"/tmp/claud[e]-[0-9]+|/var/folder[s]/")
total, holding = collections.Counter(), collections.Counter()
for top, _, names in os.walk(runs):
    for n in names:
        if n in ("capture.log", "status.json"):
            continue  # the sweep's own log and status, not a capture
        kind = n.rsplit(".", 1)[-1] if "." in n else "(none)"
        if "-bodies" in top:
            kind = "body"
        data = open(os.path.join(top, n), "rb").read()
        total[kind] += 1
        if tmp in data or home in data or shape.search(data):
            holding[kind] += 1
for k in sorted(total):
    print("%-8s %4d files, %4d hold a machine root" % (k, total[k], holding[k]))
print("%-8s %4d files, %4d hold a machine root" % ("all", sum(total.values()), sum(holding.values())))
