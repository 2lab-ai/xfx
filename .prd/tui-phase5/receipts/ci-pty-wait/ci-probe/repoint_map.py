"""Maps line numbers of <file> from <old> to <new> through the diff (difflib's
unchanged blocks), and prints each number's text at both, which must be the
same line. For a cited line that stands at many places, which no window
around it can place.

    python3 -I -B repoint_map.py <repo> <old> <new> <file> <n>...
Prints no path but the repo-relative file's; exits 1 if a number is not in
an unchanged block.
"""
import difflib
import subprocess
import sys

repo, old, new, path, numbers = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5:]
show = lambda rev: subprocess.run(["git", "-C", repo, "show", f"{rev}:{path}"], capture_output=True,
                                  text=True, check=True).stdout.split("\n")
a, b = show(old), show(new)
blocks = difflib.SequenceMatcher(None, a, b, autojunk=False).get_matching_blocks()
bad = 0
for n in map(int, numbers):
    hit = [j + (n - 1 - i) + 1 for i, j, size in blocks if i <= n - 1 < i + size]
    if not hit:
        print(f"{path}:{n}: not in an unchanged block")
        bad += 1
        continue
    m = hit[0]
    same = a[n - 1] == b[m - 1]
    print(f"{path}:{n} -> :{m} (+{m - n}), the same line: {'yes' if same else 'NO'}: {b[m - 1].strip()}")
    bad += not same
sys.exit(1 if bad else 0)
