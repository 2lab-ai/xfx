#!/usr/bin/env python3
"""Train 14's join of main (new): token_check.py's self-test plants "a word
changed" in a numbers-only ledger entry by writing `owner = "X...`, and an
entry with no `owner` field -- every [[normalization]] -- takes no plant,
so the case reads MISSED with nothing planted. This plants the word where
every entry has one, in `why` (its first word doubled), on each ledger
entry both sides of <merge> changed in numbers alone, and runs
token_check.py's own check_ledger on it: each must be found. token_check.py
is read as it is and run without its last line, `main()`.

    python3 -I -B word_control.py <repo> <merge>
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, "..", "token_check.py"), encoding="utf-8").read()
assert src.endswith("\n\nmain()\n")
tc = {"__name__": "token_check"}
exec(compile(src[: -len("main()\n")], "token_check.py", "exec"), tc)
repo, merge = sys.argv[1:3]


def show(rev, path):
    return subprocess.run(["git", "-C", repo, "show", f"{rev}:{path}"], check=True, capture_output=True, text=True).stdout


L = "scripts/fxcap/parity-ledger.toml"
base = subprocess.run(["git", "-C", repo, "merge-base", f"{merge}^1", f"{merge}^2"], check=True, capture_output=True,
                      text=True).stdout.strip()
lb, lo, lt, lm = (show(r, L) for r in (base, f"{merge}^1", f"{merge}^2", merge))
NUMS = re.compile(r":\d+(?:-\d+)?")
ids = re.findall(r'^id = "([^"]+)"', lm, re.M)
cases = found = 0
for ident in ids:
    def body(text):
        at = text.find(f'id = "{ident}"')
        if at < 0:
            return None
        end = text.find("\n[[", at)
        return text[at:end if end >= 0 else len(text)]
    b, o, t, m = (body(x) for x in (lb, lo, lt, lm))
    if None in (b, o, t, m) or o == b or t == b or NUMS.sub(":N", o) != NUMS.sub(":N", t):
        continue
    planted = re.sub(r'^(why = ")(\S+)', r"\1\2 \2", m, count=1, flags=re.M)
    if planted == m:
        continue
    out = []
    bad = tc["check_ledger"](lb, lo, lt, lm.replace(m, planted, 1), out)
    cases += 1
    found += bool(bad)
    print(f"  {ident}: `why`'s first word doubled: {'found' if bad else 'MISSED'}")
print(f"# {found} of {cases} found")
sys.exit(0 if cases and found == cases else 1)
