"""Source lines added outside tests, against a base: for each changed file
under src/, the added lines above its `#[cfg(test)]` module (the whole file
when it has none), and how many of those are comments.

usage: python3 -I count.py <base> [<head>]   (run in the repo; head: the worktree)
"""

import re
import subprocess
import sys

sys.dont_write_bytecode = True


def main():
    base = sys.argv[1]
    head = sys.argv[2:] or []
    files = subprocess.run(["git", "diff", "--name-only", base, *head, "--", "src"],
                           capture_output=True, text=True, check=True).stdout.split()
    total = comments = 0
    for path in files:
        text = (subprocess.run(["git", "show", f"{head[0]}:{path}"], capture_output=True, text=True).stdout
                if head else open(path, encoding="utf-8").read())
        lines = text.split("\n")
        cut = next((i + 1 for i, line in enumerate(lines) if line.strip() == "#[cfg(test)]"
                    and i + 1 < len(lines) and re.match(r"\s*mod tests", lines[i + 1])), len(lines) + 1)
        diff = subprocess.run(["git", "diff", "-U0", base, *head, "--", path],
                              capture_output=True, text=True, check=True).stdout
        added = c = 0
        for hunk in re.finditer(r"^@@ -\S+ \+(\d+)(?:,(\d+))? @@.*\n((?:[-+\\].*\n?)*)", diff, re.M):
            start = int(hunk.group(1))
            new = [l for l in hunk.group(3).split("\n") if l.startswith("+")]
            for k, line in enumerate(new):
                if start + k < cut:
                    added += 1
                    c += line[1:].strip().startswith("//")
        print(f"{path}: {added} added above its tests, {c} of them comments")
        total += added
        comments += c
    print(f"== {total} added outside tests, {comments} comments")


if __name__ == "__main__":
    main()
