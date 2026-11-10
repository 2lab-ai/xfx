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
#   { git diff afd891af -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain afd891af -- docs/parity.md | grep '^+'; } |
#     fx_src.sh <fx checkout>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  # The decoder: the bits and composerMove; the modified arrow table (its H
  # and F rows); the plain navigation table and the keypad's route to it;
  # `ESC ESC`, the meta prefix; CSI's first stages (a digit starts the tilde
  # stage, 3); and the stage after `;` (5), whose `~` branch is the modified
  # tilde keys.
  src/ui/input/escape_parser.zig:58-71
  src/ui/input/escape_parser.zig:73-108
  src/ui/input/escape_parser.zig:134-167
  # Delete under a modifier, which the tilde stage hands the raw modifiers.
  src/ui/input/escape_parser.zig:175-180
  src/ui/input/escape_parser.zig:550-555
  src/ui/input/escape_parser.zig:561-636
  src/ui/input/escape_parser.zig:662-755
  # What an action is to the composer, and the event that carries both.
  src/ui/input/shortcuts.zig:43-65
  src/ui/input/terminal_action_decoder.zig:214-224
  # The approval's and the question's folds of a composer move, and the
  # question's wrapper; fx's own test of the five tilde codes.
  src/ui/input/runtime.zig:75-133
  src/ui/input/runtime.zig:155-197
  src/ui/input/runtime.zig:228-257
  src/ui/input/runtime.zig:4774-4795
  # The composer's line and draft moves.
  src/core/input/horizontal_navigation.zig:19-38
  src/core/app/app_input_runtime.zig:244-247
  src/core/app/app_input_runtime.zig:296-309
  # The route a decoded action takes, surface by surface.
  src/core/app/app_input_runtime.zig:895-1047
  src/core/app/app_input_runtime.zig:2223-2263
  src/core/app/input_full_transcript_runtime.zig:156-177
  src/core/app/input_approval_runtime.zig:71-130
  src/core/app/input_question_runtime.zig:139-201
)
{
  echo "# fx v0.0.16 ($pin), every line this receipt cites, as git show $pin:<path> prints it, numbered."
  for r in "${ranges[@]}"; do
    path=${r%%:*} span=${r##*:}
    echo
    echo "## $r"
    git -C "$fx" show "$pin:$path" | awk -v a="${span%-*}" -v b="${span#*-}" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
  done
  # Who reads the `.home` and `.end` actions at the pin, as switch arms,
  # comparisons or arguments: the decoder that makes them, `fromEscapeAction`,
  # which makes each a composer move, and the composer's switch, where they
  # are `unreachable` because that move took them first; app_input_runtime's
  # other two are its test of a question's field. (native_session.zig's are a
  # different enum, the keys fx sends a child terminal; editor_state,
  # horizontal_navigation and session are a selection's or a read's `.end`.)
  echo
  echo "## git grep -n -E '<.home/.end as an arm, a comparison or an argument>' $pin -- src/ui src/core"
  git -C "$fx" grep -n -E '^[[:space:]]*\.(home|end)(,| =>)|=> \.(home|end)[,;]|== \.(home|end)[^_a-z]|\(\.(home|end)[,)]' "$pin" -- src/ui src/core | sed "s/^$pin://"
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
