#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in whatever text is
# piped in, wherever it names `481104fd` or no pin, must fall inside one of
# the ranges below. Citations at another pin (`c1db919`) are not this file's.
# MODIFIED-PAGE-KEYS' checker (TILDE-EMPTY-MODIFIER's before it), with this
# unit's ranges.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
#   the unit's added lines, piped:
#   { git diff c77a77ce -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain c77a77ce -- docs/parity.md | grep '^+'; } |
#     fx_src.sh <fx checkout>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  # The decoder: the modifier bits and composerMove; every held arrow a
  # composer move (Super, then Alt, then Ctrl, then Shift); the plain and
  # the keypad navigation table; the key report resolver (the kitty arrows,
  # Ctrl and a letter its control byte); the CSI stages (the modifier field,
  # the modified arrow before the plain one).
  src/ui/input/escape_parser.zig:56-71
  src/ui/input/escape_parser.zig:73-108
  src/ui/input/escape_parser.zig:116-119
  src/ui/input/escape_parser.zig:146-167
  src/ui/input/escape_parser.zig:182-266
  src/ui/input/escape_parser.zig:662-755
  # What a raw byte and a decoded action are to the composer and to a
  # focused editor.
  src/ui/input/shortcuts.zig:7-65
  src/ui/input/terminal_action_decoder.zig:207-224
  # The approval's and the question's readings of a key: the draft's edits,
  # the question's bytes, its field's moves, its choice move, the raw path.
  src/ui/input/runtime.zig:60-259
  # The route: the full transcript's keys, the raw byte's route past them,
  # the decoded action's route (the screen, the question, the approval, the
  # composer behind them), the modal bytes, the composer's move.
  src/core/app/input_full_transcript_runtime.zig:40-71
  src/core/app/input_full_transcript_runtime.zig:147-198
  src/core/app/input_question_runtime.zig:139-201
  src/core/app/input_approval_runtime.zig:71-130
  src/core/app/app_input_runtime.zig:244-330
  src/core/app/app_input_runtime.zig:840-1010
  src/core/app/app_input_runtime.zig:1126-1186
  src/core/app/input_completion_runtime.zig:211-292
  src/core/input/horizontal_navigation.zig:19-51
  src/core/permissions/approval_amendment.zig:84-92
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

# The check. Reads README.md, plus whatever text is piped in, if any.
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
    try:
        text += open(name, encoding="utf-8").read() + "\n\n"
    except FileNotFoundError:
        pass
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
