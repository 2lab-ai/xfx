#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in the `src/` and
# `tests/` lines this unit adds, wherever it names `481104fd` or no pin, must
# fall inside one of the ranges below. Citations at another pin (`c1db919`)
# are not this file's.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  src/core/app/input_approval_runtime.zig:71-130
  src/core/permissions/approval_decision.zig:13-31
  src/core/permissions/approval_decision.zig:129-176
  src/core/permissions/approval_decision.zig:214-246
  src/core/permissions/approval_amendment.zig:84-92
  src/ui/input/runtime.zig:61-133
  src/ui/input/shortcuts.zig:11-105
  src/ui/input/escape_parser.zig:38-39
  src/ui/input/escape_parser.zig:61-109
  src/ui/input/escape_parser.zig:146-167
  src/ui/input/escape_parser.zig:185-217
  src/ui/input/escape_parser.zig:686-692
  src/ui/input/escape_parser.zig:721-737
  src/ui/input/terminal_action_decoder.zig:207-224
  src/core/input/input_action.zig:71-72
  src/core/app/app_input_runtime.zig:840-894
  src/core/app/app_input_runtime.zig:955-971
  src/core/app/app_input_runtime.zig:1004-1012
  src/core/app/app_input_runtime.zig:1126-1185
  src/core/app/app_input_runtime.zig:5931-5961
  src/core/app/input_paste_runtime.zig:19-25
  src/ui/input/escape_parser.zig:268-274
  src/ui/input/terminal_action_decoder.zig:103-124
  src/core/app/app_input_runtime.zig:623-656
  src/core/app/app_input_runtime.zig:1229-1258
  src/core/app/input_approval_runtime.zig:186-218
  src/ui/render_request.zig:172-177
  src/ui/render_request.zig:367-370
  src/ui/approval_screen.zig:600-607
  src/ui/footer/approval_readiness.zig:15-38
)
{
  echo "# fx v0.0.16 ($pin), every line this receipt cites, as git show $pin:<path> prints it, numbered."
  for r in "${ranges[@]}"; do
    path=${r%%:*} span=${r##*:}
    echo
    echo "## $r"
    git -C "$fx" show "$pin:$path" | awk -v a="${span%-*}" -v b="${span#*-}" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
  done
  # Who makes `.history_up`/`.history_down` at the pin: the enum declares them,
  # and no decoder under src/ui or src/core/input emits either.
  echo
  echo "## git grep -n 'history_up\|history_down' $pin -- src/ui src/core/input"
  git -C "$fx" grep -n 'history_up\|history_down' "$pin" -- src/ui src/core/input | sed "s/^$pin://"
} > "$here/fx-src.txt"

# The check. Reads README.md, plus whatever text is piped in, if any: the
# unit's added text, as
#   { git diff 2593f49c -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain 2593f49c -- docs/parity.md | grep '^+'; }
# (words for docs/parity.md, whose rows are one line each: a changed row's
# other units' citations are not this unit's).
inputs=("$here/README.md")
if [[ ! -t 0 ]]; then
  # The check's own source is a heredoc on python's stdin, so the piped
  # lines go through a file.
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
