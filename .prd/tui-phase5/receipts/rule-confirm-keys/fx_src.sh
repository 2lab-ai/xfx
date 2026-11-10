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
#   the scenarios, and its two sentences in docs/parity.md (one row is one
#   line, and a word diff would leave a bare ref after the wrong file):
#   { git diff 1a0adaec -- src tests scripts/fxcap/scenarios | python3 -I added.py;
#     grep -o "[*][*]Its keys are the approval's[*][*].*RULE-CONFIRM-SCREEN)[.]" docs/parity.md;
#     grep -o "fx's .\/permissions. rule confirmation is an approval prompt[^)]*)[^)]*)[.]" docs/parity.md; } | fx_src.sh <fx>
#
# added.py keeps the added lines that are not a removed line moved: the fix
# moves `Shell::decide`'s head, whose old citations are not this unit's.
#
# APPROVAL-CTRL-J's fx_src.sh, with this unit's ranges: the confirmation-only
# prompt and its decision, the approval's byte and escape routes, Escape's
# route and the decoder's latch of an operation in hand, the rule change's
# submit, and its screen.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  src/ui/input/runtime.zig:55-153
  src/ui/input/shortcuts.zig:11-65
  src/ui/input/escape_parser.zig:146-274
  src/ui/input/terminal_action_decoder.zig:29-45
  src/ui/input/terminal_action_decoder.zig:174-204
  src/core/app/app_input_runtime.zig:555-584
  src/core/app/app_input_runtime.zig:896-971
  src/core/app/app_input_runtime.zig:1126-1227
  src/core/app/app_input_runtime.zig:2687-2764
  src/core/app/input_approval_runtime.zig:71-147
  src/core/app/input_approval_runtime.zig:179-237
  src/core/app/input_approval_runtime.zig:257-328
  src/core/app/input_approval_runtime.zig:401-436
  src/core/app/input_interrupt_runtime.zig:6-21
  src/core/app/input_interrupt_runtime.zig:46-134
  src/core/app/input_interrupt_runtime.zig:178-187
  src/core/app/input_submit_runtime.zig:715-729
  src/core/app/app_commands.zig:598-750
  src/core/app/app_worker_runtime.zig:680-690
  src/core/app/app_worker_runtime.zig:728-760
  src/core/permissions/approval_prompt.zig:50-56
  src/core/permissions/approval_prompt.zig:94-146
  src/core/permissions/approval_decision.zig:53-247
  src/ui/approval_screen.zig:208-223
  src/ui/footer/approval_ui.zig:1688-1748
  src/ui/footer/approval_ui.zig:1841-1852
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
