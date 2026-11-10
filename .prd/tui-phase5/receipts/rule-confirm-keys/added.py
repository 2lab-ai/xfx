"""The added lines of a unified diff on stdin that are not moves: an added
line whose text, stripped, is also a removed line's in the same diff is left
out. Prints the kept lines with their `+`.

usage: git diff <base> -- <paths> | python3 -I added.py
"""
import sys

lines = sys.stdin.read().split("\n")
removed = {l[1:].strip() for l in lines if l.startswith("-") and not l.startswith("---")}
for l in lines:
    if l.startswith("+") and not l.startswith("+++") and (l[1:].strip() not in removed or not l[1:].strip()):
        print(l)
