#!/usr/bin/env python3
"""The join of main's re-points (train 13's repointed.py, read over this
folder's repoint.py; `..` on the path for refs_content.py and refs_all.py),
each checked against its code at the head, and the re-point commit checked
for numbers only.

    python3 -I -B repointed.py <repo> <head> <re-point-commit>...

For every reference in repoint.py's PLAN: `was` is the number the code
head before the re-point held, `now` the number the head's citing line
holds **as read from the head's text** (the token at the same place among
the line's numbers), not from the plan. Train 6's check: the cited file's
lines `was` .. in the frame (the commit that wrote the numbers), and two
either side, equal the head's `now` .. and two either side. For the one
entry repoint.py takes from APPROVAL-VERTICAL-SPELLINGS' refs.py table
instead (its frame holds the wrong line), the head's `now` line must hold
that table's text, on exactly one line. Then, for each commit given, every
line it changed is the same once every `:N` / `:N-M` is masked (numbers
only), and it changed as many lines as it added.
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(here, ".."))
sys.path.insert(0, here)
import repoint as rp  # noqa: E402
import refs_content as rc  # noqa: E402

NUMS = re.compile(r":\d+(?:-\d+)?")


def main():
    repo, head, commits = sys.argv[1], sys.argv[2], sys.argv[3:]
    ok = total = 0
    rows = rp.derive(repo)
    for citing, at, token, new, cited, frame, why, how in rows:
        total += 1
        before = rc.show(rp.CODE_HEAD, citing)[at - 1]
        line = rc.show(head, citing)[at - 1]
        prefix = token[: re.search(r":\d+(?:-\d+)?$", token).start()]
        start = rp.token_re(token).search(before).start() + len(prefix)
        place = [x.start() for x in NUMS.finditer(before)].index(start)
        m = list(re.finditer(r":(\d+)(?:-(\d+))?", line))[place] if NUMS.sub(":N", line) == NUMS.sub(":N", before) else None
        was = re.search(r":(\d+)(?:-(\d+))?$", token)
        w0 = int(was.group(1))
        n = (int(was.group(2)) - w0 + 1) if was.group(2) else 1
        now = int(m.group(1)) if m else None
        b = rc.show(head, cited)
        if (citing, at, token) in rp.EXPECTED:
            want = rp.EXPECTED[(citing, at, token)]
            hit = now is not None and b[now - 1].strip() == want and sum(x.strip() == want for x in b) == 1
            check = "the head's line holds refs.py's text, on one line"
        else:
            a = rc.show(frame, cited)
            ctx = rc.CTX
            hit = now is not None and a[max(w0 - 1 - ctx, 0):w0 - 1 + n + ctx] == b[max(now - 1 - ctx, 0):now - 1 + n + ctx]
            check = f"+-{ctx} lines the same as the frame's"
        ok += hit
        span = lambda x: f"{x}" + (f"-{x + n - 1}" if n > 1 else "")  # noqa: E731
        print(f"{'SAME' if hit else 'DIFFERENT'}  {citing}:{at} {prefix or '(bare)'}: {cited}:{span(w0)}@{frame[:8]} "
              f"-> :{span(now) if now else '?'}@{head[:8]} ({check})\n    first: {b[now - 1].strip()[:110] if now else ''}")
    print(f"# {ok} of {total} the same code")
    bad = 0
    for c in commits:
        diff = subprocess.run(["git", "-C", repo, "show", "--format=", "-U0", c], check=True,
                              capture_output=True, text=True).stdout.split("\n")
        minus = [x[1:] for x in diff if x.startswith("-") and not x.startswith("---")]
        plus = [x[1:] for x in diff if x.startswith("+") and not x.startswith("+++")]
        same = len(minus) == len(plus) and all(NUMS.sub(":N", x) == NUMS.sub(":N", y) for x, y in zip(minus, plus))
        bad += not same
        print(f"# {c[:8]}: {len(minus)} lines out, {len(plus)} in; "
              f"{'each the same once its numbers are masked' if same else 'NOT numbers only'}")
    sys.exit(0 if ok == total and not bad else 1)


main()
