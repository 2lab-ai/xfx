#!/usr/bin/env python3
"""Train 10: a conflict where both sides of a merge appended lines at the same
place of the base (the end of a tests module, git interleaving the two blocks
around the few lines they share). The resolution: each block whole, ours
first, then theirs, as train 9 resolved its two; everything outside the
conflict stays as git merged it.

    python3 -I -B append_both.py <repo> <base> <ours> <theirs> <path> <base-line>

<base-line> is the base line both sides inserted after; each side's change
there must be a pure insertion. The worktree's file holds git's merge: the
text before its first conflict marker and after its last is kept, and the
span between is put back as ours' block (less the leading lines git already
put before the first marker, which both blocks share) and theirs' whole (less
the closing lines git put after the last marker, which both blocks share). It
stops when the kept lines before the first marker are not a start both blocks
share after base line <base-line>, or the text after the last marker is not
an end both blocks share followed by what follows ours' block in ours' file. filecheck.py then checks the result from both ends.
"""
import re
import subprocess
import sys

repo, base, ours, theirs, path, at = sys.argv[1:7]
at = int(at)


def git(*a):
    return subprocess.run(["git", "-C", repo, *a], check=True, capture_output=True, text=True).stdout


def insertion(b):
    for line in git("diff", "-U0", base, b, "--", path).split("\n"):
        m = re.match(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
        if m:
            old, old_n, new, new_n = (int(x) if x is not None else 1 for x in m.groups())
            if old == at and old_n == 0:
                return new, new_n
    sys.exit(f"{b}: no pure insertion after base line {at}")


o_new, o_n = insertion(ours)
t_new, t_n = insertion(theirs)
ours_lines = git("show", f"{ours}:{path}").split("\n")
theirs_lines = git("show", f"{theirs}:{path}").split("\n")
o_block = ours_lines[o_new - 1:o_new - 1 + o_n]
t_block = theirs_lines[t_new - 1:t_new - 1 + t_n]
after = ours_lines[o_new - 1 + o_n:]
work = open(f"{repo}/{path}", encoding="utf-8").read().split("\n")
first = next(i for i, l in enumerate(work) if l.startswith("<<<<<<< "))
last = max(i for i, l in enumerate(work) if l.startswith(">>>>>>> "))
pre, post = work[:first], work[last + 1:]
# The lines of both blocks' shared start that git kept before the marker.
k = next((k for k in range(min(o_n, t_n), -1, -1)
          if o_block[:k] == t_block[:k] and (k == 0 or pre[-k:] == o_block[:k])), None)
lead = ours_lines[o_new - 2]  # base line <at>, in ours
if pre[len(pre) - k - 1] != lead:
    sys.exit(f"the line before the shared start is not base line {at}: {pre[len(pre) - k - 1]!r}")
# The text after the last marker: what follows the insertion in ours' file,
# after the lines git kept there that both blocks share at their ends.
j = len(post) - len(after)
if j < 0 or post[j:] != after or (j and not (o_block[-j:] == t_block[-j:] == post[:j])):
    sys.exit("the text after the last marker is not a shared end of both blocks and what follows ours'")
merged = pre + o_block[k:] + t_block[:t_n - j] + post
open(f"{repo}/{path}", "w", encoding="utf-8").write("\n".join(merged))
print(f"{path}: ours' block {o_n} lines (ours {o_new}..{o_new + o_n - 1}), then theirs' {t_n} lines "
      f"(theirs {t_new}..{t_new + t_n - 1}), both after base line {at}; git had kept {k} first and {j} last "
      f"lines the two blocks share outside its markers; {len(merged)} lines")
