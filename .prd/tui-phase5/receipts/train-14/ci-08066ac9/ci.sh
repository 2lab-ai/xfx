#!/usr/bin/env bash
# Train 14, ci-08066ac9 (new): PR #93's CI Sweep (macOS arm64) at 08066ac9,
# from its artifact as the dispatcher unpacked it: report.md's head lines
# and its regressions table, and result.json's two `workspace-index` pairs.
# Output: ci.txt, through redacted.py.
# usage: ci.sh <artifact-dir>
set -u
a=$1
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# report.md, lines 1-12"
  sed -n 1,12p "$a/report.md"
  echo
  echo "# result.json: head, base, and the workspace-index pairs"
  python3 -I - "$a/result.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print("head", d["head"], "base", d["base"], "regressions", d["regressions"])
for p in d["pairs"]:
    if p["scenario"] == "workspace-index":
        print(json.dumps(p, ensure_ascii=False))
PY
} 2>&1 | python3 -I -B "$here/../redacted.py" "$here/ci.txt" "$a=<artifact>"
