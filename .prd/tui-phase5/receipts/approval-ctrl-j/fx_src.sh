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
#   the scenarios, and its whole sentence in docs/parity.md (one row is one
#   line, and a word diff would leave a bare ref after the wrong file):
#   { git diff d7916178 -- src tests scripts/fxcap/scenarios | grep '^+';
#     grep -o '[*][*]C-j answers a permission question as Enter does.*RULE-CONFIRM-KEYS, proposed)[.]' docs/parity.md; } | fx_src.sh <fx>
#
# APPROVAL-DRAFT-KEYS' fx_src.sh (itself APPROVAL-READINESS-SETTLED's), with
# this unit's ranges: the approval's byte map and the question's beside it,
# the decoder's Ctrl+letter and its stages, the routing that puts the
# approval ahead of every menu, and the rule confirmation's submit.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  src/ui/input/runtime.zig:55-153
  src/ui/input/shortcuts.zig:11-65
  src/ui/input/escape_parser.zig:146-274
  src/ui/input/escape_parser.zig:500-560
  src/ui/input/escape_parser.zig:596-628
  src/ui/input/escape_parser.zig:655-780
  src/ui/input/terminal_action_decoder.zig:74-124
  src/ui/input/terminal_action_decoder.zig:207-232
  src/core/app/app_input_runtime.zig:590-656
  src/core/app/app_input_runtime.zig:736-757
  src/core/app/app_input_runtime.zig:840-1000
  src/core/app/app_input_runtime.zig:1093-1116
  src/core/app/app_input_runtime.zig:1126-1258
  src/core/app/app_input_runtime.zig:1812-1846
  src/core/app/input_approval_runtime.zig:28-130
  src/core/app/input_approval_runtime.zig:159-237
  src/core/app/input_full_transcript_runtime.zig:26-52
  src/core/app/app_auth_runtime.zig:688-728
  src/core/permissions/approval_decision.zig:129-176
  src/core/permissions/approval_prompt.zig:50-56
  src/core/permissions/approval_prompt.zig:136-146
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
