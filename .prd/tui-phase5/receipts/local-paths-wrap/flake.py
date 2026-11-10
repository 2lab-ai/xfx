"""LOCAL-PATHS-WRAP: how often the tail rule could refuse a run for its random
run directory's name. A run's scratch root is `<TMPDIR>/fxcap.XXXXXX` (six of
A-Za-z0-9), its work root that and `/work`; a row behind a mark that starts
with the root's last 1 to 6 bytes, ending whole, is refused. Over the tracked
files' rows behind a mark: the rows that start with 1 to 6 letters or digits
ending whole, by length and token, and what a run with one such row of each
length risks (62**-n). Prints tokens, never a path.

usage: git ls-files -z | python3 -I flake.py <scripts/fxcap>
"""
import collections
import re
import sys

sys.path.insert(0, sys.argv[1])
sys.dont_write_bytecode = True
import roots  # noqa: E402

ALNUM = re.compile(rb"[A-Za-z0-9]{1,7}")
rows = 0
first = collections.Counter()
by_len = collections.Counter()
tokens = collections.Counter()
files = collections.defaultdict(set)
for n in sys.stdin.buffer.read().split(b"\0"):
    if not n:
        continue
    try:
        data = open(n, "rb").read()
    except OSError:
        continue
    if not any(m in data for m in roots.MARKS):
        continue
    v = roots.View(data)
    for a in v.marked:
        rows += 1
        ch = v.text[a : a + 4].decode("utf-8", "ignore")[:1]
        first["U+%04X %s" % (ord(ch), ch) if ch else "(the row ends there)"] += 1
        m = ALNUM.match(v.text, a)
        if m and len(m.group()) <= 6 and v.ends_whole(m.end()):
            k = len(m.group())
            by_len[k] += 1
            tokens[m.group().decode()] += 1
            files[k].add(n)
print("rows behind a mark: %d; what each starts with: %s" % (rows, ", ".join("%s x%d" % t for t in first.most_common(12))))
for k in sorted(by_len):
    print("starting with %d letter(s)/digit(s), ending whole: %d rows in %d file(s); one such row refuses a run with p = 62^-%d = %.2g"
          % (k, by_len[k], len(files[k]), k, 62.0 ** -k))
print("tokens: %s" % ", ".join("%s x%d" % t for t in tokens.most_common(20)))
print("of them a work root's end (k, rk, ork, work): %d" % sum(c for t, c in tokens.items() if t in ("k", "rk", "ork", "work")))
