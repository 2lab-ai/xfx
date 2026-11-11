#!/usr/bin/env bash
# Train 14 (new; train 13 ran refs_content.py over seven named files): the
# content check over every reference -- refs_content.py over every tracked
# file at <commit> that holds one by its own pattern (outside `.prd/` and the
# historical plans, `docs/superpowers/plans/`; a `.zig` path is fx's and not
# counted), each file's verdict lines and counts, then the sums over all of
# them. Extra arguments go to refs_content.py (`--ignore <commit>`).
# usage: content.sh <repo> <commit> [--ignore <commit>]...
set -u
repo=$1 commit=$2; shift 2
here=$(cd "$(dirname "$0")" && pwd)
files=$(python3 -I -B - "$repo" "$commit" "$here" <<'EOF'
import re, subprocess, sys
repo, commit, here = sys.argv[1:]
sys.path.insert(0, here)
import refs_content as rc
rc.setup(repo, commit, [])
names = subprocess.run(["git", "-C", repo, "ls-tree", "-r", "--name-only", commit], capture_output=True, text=True, check=True).stdout.split("\n")
for f in names:
    if not f or f.startswith(".prd/") or f.startswith("docs/superpowers/plans/"):
        continue
    if not re.search(r"\.(rs|sh|py|md|toml|ts|json|txt|cap|yml|rb)$", f):
        continue
    text = rc.show(commit, f) or []
    if any(not r[2].endswith(".zig") for line in text for r in rc.refs_of(line)):
        print(f)
EOF
)
total=0
for f in $files; do
  total=$((total + 1))
  echo "## $f"
  python3 -I -B "$here/refs_content.py" "$repo" "$commit" "$f" "$@"
  echo "# exit $?"
done | python3 -I -B -c '
import re, sys
text = sys.stdin.read()
sys.stdout.write(text)
sums, files = {}, 0
for m in re.finditer(r"^# SAME (\d+); SAME-ROW (\d+); SAME-LINES (\d+); WRONG (\d+); other product (\d+); unread (\d+); refs (\d+)$", text, re.M):
    files += 1
    for k, v in zip(["SAME", "SAME-ROW", "SAME-LINES", "WRONG", "other product", "unread", "refs"], m.groups()):
        sums[k] = sums.get(k, 0) + int(v)
print(f"# all {files} files: " + "; ".join(f"{k} {v}" for k, v in sums.items()))
'
