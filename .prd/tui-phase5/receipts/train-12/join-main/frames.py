#!/usr/bin/env python3
"""repoint.py read a reference whose citing line both sides hold in the
merge base's frame. This reads each one again in the frame of the commit
that last wrote its citing line, mapped from there to the commit the plan
was taken at, and prints where the two readings disagree.

    python3 -I -B frames.py <repo>
"""
import os
import re
import sys

# this folder's repoint.py, not train 12's one level up
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import repoint  # noqa: E402
from refs_all import hunks, mapped  # noqa: E402


def main():
    repo = sys.argv[1]
    differ = 0
    for head, plan in (("0c39ce8c", repoint.PLAN), (repoint.LATE_HEAD, repoint.LATE)):
        repoint.HEAD = head
        for citing, at, ref, cited in plan:
            name, frame, blamed = repoint.frame(repo, citing, at)
            m = re.search(r":(\d+)(?:-(\d+))?$", ref)
            a, b = int(m.group(1)), int(m.group(2) or m.group(1))
            via_frame = (mapped(a, hunks(repo, frame, head, cited)), mapped(b, hunks(repo, frame, head, cited)))
            via_blamed = (mapped(a, hunks(repo, blamed, head, cited)), mapped(b, hunks(repo, blamed, head, cited)))
            same = via_frame == via_blamed
            differ += not same
            print("%s:%d  %s  %s %s -> %s | blamed %s -> %s  %s" % (
                citing, at, ref, name, frame[:8], via_frame, blamed[:8], via_blamed, "same" if same else "DIFFER"))
    print("# %d whose two readings differ" % differ)


main()
