#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in whatever text is
# piped in, wherever it names `481104fd` or no pin, must fall inside one of
# the ranges below. Citations at another pin (`c1db919`) are not this file's.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
#   the unit's added lines, piped:
#   { git diff ebb04642 -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain ebb04642 -- docs/parity.md | grep '^+'; } |
#     fx_src.sh <fx checkout>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  # The accumulator every digit of every field goes through, the modifier
  # bits and composerMove, and the modified arrow table.
  src/ui/input/escape_parser.zig:19-23
  src/ui/input/escape_parser.zig:58-71
  src/ui/input/escape_parser.zig:73-108
  # The keypad's page keys; Delete under a modifier, which the tilde stage
  # hands the raw modifiers;
  # the key-report resolver the third field reaches, and its lock-bit mask.
  src/ui/input/escape_parser.zig:169-173
  src/ui/input/escape_parser.zig:175-180
  src/ui/input/escape_parser.zig:185-266
  # CSI's first stages (a digit starts the tilde stage, 3; the paste
  # markers; the bare tilde keys), the stage after `;` (5, whose `~` is the
  # modified tilde keys), and the third field's (6).
  src/ui/input/escape_parser.zig:561-636
  src/ui/input/escape_parser.zig:662-755
  # What an action is to the composer, and the event that carries both.
  src/ui/input/shortcuts.zig:43-65
  src/ui/input/terminal_action_decoder.zig:214-224
  # The approval's and the question's folds of a composer move; fx's own
  # test of the five tilde codes.
  src/ui/input/runtime.zig:75-133
  src/ui/input/runtime.zig:155-197
  src/ui/input/runtime.zig:4774-4795
  # The route a decoded action takes: the full transcript's keys (a page
  # key's action, not a composer move to a page), then the composer's.
  src/core/app/input_full_transcript_runtime.zig:40-71
  src/core/app/input_full_transcript_runtime.zig:156-178
  src/core/app/app_input_runtime.zig:296-330
  src/core/app/app_input_runtime.zig:895-1047
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
