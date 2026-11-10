#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in the lines this unit
# adds (piped in, below), wherever it names `481104fd` or no pin, must fall
# inside one of the ranges below. Citations at another pin (`c1db919`) are
# not this file's.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
#   with the unit's added text on stdin: its added lines in src, tests and
#   the scenario, and its whole sentence in docs/parity.md (one row is one
#   line, and a word diff would leave a bare ref after the wrong file):
#   { git diff a37a969e -- src tests scripts/fxcap/scenarios | grep '^+';
#     grep -o '[*][*]When an affirmative may be taken.*grant scope.s rows)[.]' docs/parity.md; } | fx_src.sh <fx>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  src/core/app/input_approval_runtime.zig:28-30
  src/core/app/input_approval_runtime.zig:51-130
  src/core/app/input_approval_runtime.zig:159-237
  src/ui/render_request.zig:93-121
  src/ui/render_request.zig:172-177
  src/ui/render_request.zig:216-218
  src/ui/render_request.zig:272-351
  src/ui/render_request.zig:363-370
  src/ui/render_request.zig:591-627
  src/ui/footer/approval_readiness.zig:15-39
  src/ui/footer/approval_readiness.zig:65-75
  src/ui/approval_screen.zig:600-607
  src/core/permissions/approval_decision.zig:129-176
  src/core/permissions/approval_decision.zig:196-225
  src/core/permissions/permission_request.zig:17-22
  src/core/permissions/permission_request.zig:29-42
  src/core/app/app_input_runtime.zig:1126-1185
  src/core/app/app_input_runtime.zig:1229-1258
  src/core/app/app_render_runtime.zig:78-88
  src/core/app/app_render_runtime.zig:924-1032
  src/core/app/app_render_runtime.zig:2048-2053
  src/ui/render_engine/terminal_diff.zig:15-23
  src/ui/event_loop.zig:16-16
  src/ui/event_loop.zig:76-122
)
{
  echo "# fx v0.0.16 ($pin), every line this receipt cites, as git show $pin:<path> prints it, numbered."
  for r in "${ranges[@]}"; do
    path=${r%%:*} span=${r##*:}
    echo
    echo "## $r"
    git -C "$fx" show "$pin:$path" | awk -v a="${span%-*}" -v b="${span#*-}" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
  done
} > "$here/fx-src.txt"

inputs=("$here/README.md")
if [[ ! -t 0 ]]; then
  piped=$(mktemp "${TMPDIR:-/tmp}/fx_src.XXXXXX")
  trap 'rm -f "$piped"' EXIT
  cat > "$piped"
  inputs+=("$piped")
fi
python3 -I - "$here/fx-src.txt" "${inputs[@]}" <<'PY'
import re, sys
src = open(sys.argv[1], encoding="utf-8").read()
have = {}
for path, a, b in re.findall(r"^## (\S+?):(\d+)-(\d+)$", src, re.M):
    have.setdefault(path.rsplit("/", 1)[-1], []).append((int(a), int(b)))
text = ""
for name in sys.argv[2:]:
    text += open(name, encoding="utf-8").read() + "\n\n"
cite = re.compile(r"([a-z_]+\.zig)((?::\d+(?:-\d+)?)+)|(?<![\w.])(:\d+(?:-\d+)?)(?![\d\w])")
bad, n, current = [], 0, None
for line in text.splitlines():
    if "c1db919" in line:
        current = None
        continue
    for m in cite.finditer(line):
        if m.group(1):
            current, spans = m.group(1), m.group(2)
        elif current:
            spans = m.group(3)
        else:
            continue
        if current not in have:
            bad.append("%s (no range for this file): %s" % (current, line.strip()[:100]))
            continue
        for s in re.findall(r"\d+(?:-\d+)?", spans):
            a, _, b = s.partition("-")
            a, b = int(a), int(b or a)
            n += 1
            if not any(lo <= a and b <= hi for lo, hi in have[current]):
                bad.append("%s:%s: %s" % (current, s, line.strip()[:100]))
    if not line.strip():
        current = None
for b in bad:
    print("OUT OF RANGE", b)
print("%d citations checked, %d outside fx-src.txt's ranges" % (n, len(bad)))
sys.exit(1 if bad else 0)
PY
