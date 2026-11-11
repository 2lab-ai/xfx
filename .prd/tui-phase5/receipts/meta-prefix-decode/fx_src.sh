#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in whatever text is
# piped in, wherever it names `481104fd` or no pin, must fall inside one of
# the ranges below. Citations at another pin (`c1db919`) are not this file's.
# COMPOSER-SUPER-ARROWS' checker (COMPOSER-PARAGRAPH-MOVE's before it),
# with this unit's ranges.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
#   the unit's added lines, piped:
#   { git diff df45025c -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain df45025c -- docs/parity.md | grep '^+'; } |
#     fx_src.sh <fx checkout>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  # The decoder's state: a saturating field, the meta bit in the stage byte,
  # the modifier bits, composerMove.
  src/ui/input/escape_parser.zig:19-35
  src/ui/input/escape_parser.zig:61-71
  # The modified arrow table, where meta_prefixed is read beside Alt's bit;
  # ctrlOKeyAction.
  src/ui/input/escape_parser.zig:73-114
  # The keypad's translation, the plain navigation table,
  # navigationKeyAction, the page keys, Delete.
  src/ui/input/escape_parser.zig:116-180
  # The key report resolver, each of its meta_prefixed reads.
  src/ui/input/escape_parser.zig:182-266
  # The stage helpers: the meta bit kept across a stage change, the bare
  # stage read without it, the discard.
  src/ui/input/escape_parser.zig:283-293
  src/ui/input/escape_parser.zig:327-362
  # The machine: an ESC in a pending stage, the bare stage (the second ESC
  # sets the bit), CSI, its first field, SS3, the modifier field, the third
  # field, the event type.
  src/ui/input/escape_parser.zig:478-780
  # The decoder around it: the ESC that begins a stage, the ESC that
  # restarts one (no event), the replay after a bare ESC, the timed flush.
  src/ui/input/terminal_action_decoder.zig:7-232
  # fx's own tests of the prefix, and their helper.
  src/ui/input/runtime.zig:2266-2284
  src/ui/input/runtime.zig:2488-2500
  src/ui/input/runtime.zig:3585-3604
  # Who sees a byte first: the theme monitor, started at every launch, ahead
  # of the key decoder. It holds a lone ESC 75 ms, hands a second one on at
  # once, and delivers the held one after its deadline, through the loop's
  # collected bytes.
  src/core/app/app_bootstrap_runtime.zig:481-492
  src/ui/input/runtime.zig:46-55
  src/ui/terminal/theme_monitor.zig:1-174
  src/ui/terminal/theme_monitor.zig:208-225
  src/ui/terminal/theme_monitor.zig:261-301
  src/ui/input/runtime.zig:685-712
  src/main.zig:2804-2806
  src/main.zig:3096-3140
  src/main.zig:3160-3163
  # The clock: the timeout, the poll, the flush once a loop turn, the loop.
  src/main.zig:174-176
  src/main.zig:187-187
  src/main.zig:2975-2982
  src/ui/event_loop.zig:76-122
  # What an Escape does: the route, the windows, resolveEscape.
  src/core/app/app_input_runtime.zig:658-677
  src/core/app/app_input_runtime.zig:896-925
  src/core/app/app_input_runtime.zig:2687-2800
  src/core/input/gesture_state.zig:1-6
  # The approval's and the question's readings of a decoded key.
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
