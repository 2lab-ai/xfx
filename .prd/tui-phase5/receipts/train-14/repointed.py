#!/usr/bin/env python3
"""Train 14 (train 13's repointed.py, read over this train's plan file
instead of a PLAN list): each re-point of repoint-plan.txt checked against
its code at <head>, and the re-point commit checked for numbers only.

For every plan line, the citing line at <head> is found again: the line
of the citing file whose text, every `:N` masked, is the plan's line's at
<plan-commit> (the commit the plan was derived at; the plan's line number
there, when the text stands at several places), and on it the ref of the
plan's kind whose numbers are the new ones:
  auto   the frame's cited lines and two either side equal the head's at the
         new number (train 6's check);
  lines  the frame's cited lines equal the head's at the new number;
  hand / extra  the head's line at the new number starts with the text the
         plan names (its first 40 characters, `..` cut);
  text   the new span stands once in the citing file;
  left   the ref is as it was (its old numbers) at <head>.
Then the commit: every line it changed is the same once every `:N` / `:N-M`
is masked, but the plan's `text` spans, and it changed as many lines as it
added.

    python3 -I -B repointed.py <repo> <head> <plan-commit> <repoint-plan.txt> <re-point-commit>...
"""
import os
import re
import subprocess
import sys

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import refs_content as rc  # noqa: E402
import repoint as rp  # noqa: E402

repo, head, plan_commit, plan_file = sys.argv[1:5]
commits = sys.argv[5:]
rc.setup(repo, head, [])
NUMS = re.compile(r":\d+(?:-\d+)?")
ROW = re.compile(r"^(auto|lines|hand|left|text)\s+(\S+)\s+(\S+?):(\d+) (\S+) -> (\S+(?: \S+)*?) \(cites (\S+); "
                 r"frame ([0-9a-f]{8}) :(\d+)(?:-(\d+))?\): (.*)$")
EXTRA = re.compile(r"^extra\s+\S+\s+(\S+) (\S+) -> (\S+) \(read SAME; the head's :(\d+) holds '(.*)'\): ")
ok = bad = 0
for line in open(plan_file, encoding="utf-8"):
    line = line.rstrip("\n")
    m, e = ROW.match(line), EXTRA.match(line)
    if e:
        citing, old, new, n, text = e.groups()
        cited = rc.show(head, next(r for r in rc.refs_of(new)) [2]) if rc.refs_of(new) else None
        held = (cited or [""] * int(n))[int(n) - 1].strip()
        good = held.startswith(text[:40])
        print(f"{'ok ' if good else 'BAD'} extra {citing} {old} -> {new}: the head's line holds {held[:60]!r}")
        ok, bad = ok + good, bad + (not good)
        continue
    if not m:
        continue
    how, origin, citing, n, spelled, to, path, frame, f0, l0, why = m.groups()
    n, f0 = int(n), int(f0)
    l0 = int(l0) if l0 else None
    then = NUMS.sub(":N", rc.show(plan_commit, citing)[n - 1])
    lines_h = rc.show(head, citing)
    at = [i for i, l in enumerate(lines_h, 1) if NUMS.sub(":N", l) == then]
    n_h = at[0] if len(at) == 1 else min(at, key=lambda i: abs(i - n)) if at else n
    head_line = lines_h[n_h - 1]
    if how == "text":
        span = next(v[0] for (c, o), v in rp.TEXT.items() if c == citing and spelled in o)
        good = "".join(rc.show(head, citing)).count(span) == 1
        print(f"{'ok ' if good else 'BAD'} text  {citing}:{n}: the span {span[:60]!r} stands once")
    elif how == "left":
        good = any(head_line[r[0]:r[1]] == spelled for r in rc.refs_of(head_line))
        print(f"{'ok ' if good else 'BAD'} left  {citing}:{n} {spelled}: as it was")
    else:
        new = NUMS.search(to).group(0)[1:]
        a, b = (int(x) for x in (new.split("-") + [new])[:2])
        refs = [r for r in rc.refs_of(head_line) if (r[3], int(r[4]) if r[4] else r[3]) == (a, b) and r[2] == path]
        cited_h, cited_b = rc.show(head, path), rc.show(frame, path)
        if not refs:
            good, note = False, "no ref with the new numbers on the head's line"
        elif how in ("auto", "lines"):
            ctx = rc.CTX if how == "auto" else 0
            last = l0 or f0
            want = cited_b[max(f0 - 1 - ctx, 0):last + ctx]
            got = cited_h[max(a - 1 - ctx, 0):b + ctx]
            good, note = want == got, f"the frame's lines{' and two either side' if ctx else ''} at :{new}"
        else:
            text = why.split("holds ")[-1].strip("'\"")
            text = rp.HAND[(citing, spelled, refs[0][5])][1] if (citing, spelled, refs[0][5]) in rp.HAND else text
            first = text.split(" ..")[0].split("...")[0][:40]
            held = cited_h[a - 1].strip()
            good, note = held.startswith(first), f"the head's :{a} holds {held[:50]!r}"
        print(f"{'ok ' if good else 'BAD'} {how:5} {citing}:{n} {spelled} -> {to}: {note}")
    ok, bad = ok + good, bad + (not good)
print(f"# {ok + bad} plan lines checked at {head[:8]}: {ok} ok, {bad} bad")
# the commits: numbers only, but the text spans
diff = []
for commit in commits:
    diff += subprocess.run(["git", "-C", repo, "show", "--format=", "-U0", commit], capture_output=True, text=True,
                           check=True).stdout.split("\n")
minus = [l[1:] for l in diff if l.startswith("-") and not l.startswith("---")]
plus = [l[1:] for l in diff if l.startswith("+") and not l.startswith("+++")]
spans = [(o, v[0]) for (c, o), v in rp.TEXT.items()]
def unspan(l):
    for o, n in spans:
        l = l.replace(n, o)
    return NUMS.sub(":N", l)
same = sorted(map(unspan, minus)) == sorted(map(unspan, plus))
print(f"# {', '.join(c[:8] for c in commits)}: {len(minus)} lines out, {len(plus)} in; the same once numbers are masked and the "
      f"{len(spans)} text spans put back: {same}")
sys.exit(1 if bad or not same or len(minus) != len(plus) else 0)
