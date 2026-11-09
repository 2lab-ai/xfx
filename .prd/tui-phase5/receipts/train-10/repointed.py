#!/usr/bin/env python3
"""Train 10's re-points, each checked against its code at the head, and the
re-point commits checked for numbers only.

    python3 -I -B repointed.py <repo> <head> <re-point-commit>...

For every reference in repoint.py's PLAN: the frame is the side that wrote
the citing line (repoint.py's `derive`, `git blame` at the code head); `was`
is the number the frame's text holds, `now` the number the head's citing
line holds **as read from the head's text** (the number at the token's place
among the line's numbers), not from the plan. Train 6's
check: the cited file's lines `was` .. `was + n - 1` in the frame, and two
either side, equal the head's `now` .. and two either side. Then, for each
commit given, every line it changed is the same once every `:N` / `:N-M` is
masked (numbers only), and it changed as many lines as it added.
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(here, "repoint.py"), encoding="utf-8").read()
assert src.endswith("\n\nmain()\n")
rp = {"__name__": "repoint", "__file__": os.path.join(here, "repoint.py")}
exec(compile(src[: -len("main()\n")], "repoint.py", "exec"), rp)
NUMS = re.compile(r":\d+(?:-\d+)?")


def show(repo, commit, path):
    return subprocess.run(["git", "-C", repo, "show", f"{commit}:{path}"], check=True,
                          capture_output=True, text=True).stdout.split("\n")


def main():
    repo, head, commits = sys.argv[1], sys.argv[2], sys.argv[3:]
    ok = total = 0
    ctx = 2
    for citing, at, token, new, cited, side, frame, blame, holders in rp["derive"](repo):
        total += 1
        line = show(repo, head, citing)[at - 1]
        before = show(repo, rp["CODE_HEAD"], citing)[at - 1]
        prefix = token[: re.search(r":\d+(?:-\d+)?$", token).start()]
        # The token's place among the line's numbers at the code head, read at
        # the same place in the head's line (a numbers-only edit keeps them).
        start = rp["token_re"](token).search(before).start() + len(prefix)
        place = [x.start() for x in NUMS.finditer(before)].index(start)
        m = list(re.finditer(r":(\d+)(?:-(\d+))?", line))[place] if NUMS.sub(":N", line) == NUMS.sub(":N", before) else None
        was = re.search(r":(\d+)(?:-(\d+))?$", token)
        w0 = int(was.group(1)); n = (int(was.group(2)) - w0 + 1) if was.group(2) else 1
        now = int(m.group(1)) if m else None
        a, b = show(repo, frame, cited), show(repo, head, cited)
        hit = now is not None and a[w0 - 1 - ctx:w0 - 1 + n + ctx] == b[now - 1 - ctx:now - 1 + n + ctx]
        ok += hit
        span = lambda x: f"{x}" + (f"-{x + n - 1}" if n > 1 else "")  # noqa: E731
        print(f"{'SAME' if hit else 'DIFFERENT'}  {citing}:{at} {prefix or '(bare)'}: {cited}:{span(w0)}@{side} "
              f"-> :{span(now) if now else '?'}@{head[:8]} (+-{ctx} lines)\n    first: {b[now - 1].strip()[:110] if now else ''}")
    print(f"# {ok} of {total} the same code")
    bad = 0
    for c in commits:
        diff = subprocess.run(["git", "-C", repo, "show", "--format=", "-U0", c], check=True,
                              capture_output=True, text=True).stdout.split("\n")
        minus = [l[1:] for l in diff if l.startswith("-") and not l.startswith("---")]
        plus = [l[1:] for l in diff if l.startswith("+") and not l.startswith("+++")]
        same = len(minus) == len(plus) and all(NUMS.sub(":N", x) == NUMS.sub(":N", y) for x, y in zip(minus, plus))
        bad += not same
        print(f"# {c[:8]}: {len(minus)} lines out, {len(plus)} in; "
              f"{'each the same once its numbers are masked' if same else 'NOT numbers only'}")
    sys.exit(0 if ok == total and not bad else 1)


main()
