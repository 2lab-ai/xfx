#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in whatever text is
# piped in, wherever it names `481104fd` or no pin, must fall inside one of
# the ranges below. Citations at another pin (`c1db919`) are not this file's.
# COMPOSER-SUPER-ARROWS' checker, with this unit's ranges.
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
  # The amendment's normalization; who owns a paste and its capture bound
  # (beginPaste); the paste's end (settlePasteDeliveryEpoch, finishPaste:
  # the overflow, each owner's normalization, the UTF-8 check, each owner's
  # insertion and its limit); the composer's large paste, image path and
  # selection (finalizePastedBlock, handlePastedBytes); fx's own case.
  src/core/app/input_paste_runtime.zig:1-25
  src/core/app/input_paste_runtime.zig:97-360
  src/core/app/input_paste_runtime.zig:362-552
  src/core/app/input_paste_runtime.zig:684-689
  # Which bytes are content per owner, the two limits, and how the buffer
  # counts its overflow.
  src/core/input/paste_framing.zig:1-35
  src/core/input/paste_framing.zig:91-177
  # The line-ending rule and the UTF-8 check, with their tests.
  src/core/shared/text_utils.zig:28-31
  src/core/shared/text_utils.zig:82-98
  src/core/shared/text_utils.zig:1035-1040
  src/core/shared/text_utils.zig:1543-1562
  # The refusal's sentence per owner.
  src/core/app/input_limit_feedback.zig:1-43
  # The amendment's insertion and its budget; the question's.
  src/core/permissions/approval_decision.zig:18-31
  src/core/permissions/approval_decision.zig:178-212
  src/core/permissions/approval_amendment.zig:53-58
  # Round 2: the draft's Home and End (cursorHome/cursorEnd), and the
  # once-per-run rule a refusal follows.
  src/core/permissions/approval_amendment.zig:84-92
  src/core/input/input_limit_rejection.zig:1-30
  src/core/input/editor_state.zig:217-225
  src/core/agent/question_prompt.zig:572-585
  # The route: the decoded paste start (menus dismissed, beginPaste with the
  # composer's bytes), the settle with each owner's limit, a modal's bytes.
  src/core/app/app_input_runtime.zig:600-654
  src/core/app/app_input_runtime.zig:789-812
  src/core/app/app_input_runtime.zig:896-944
  src/core/app/app_input_runtime.zig:1126-1186
  src/core/app/app_input_runtime.zig:1992-1996
  # The amendment's caret keys (`.cursor_home`), and the approval's route.
  src/ui/input/runtime.zig:60-110
  src/core/app/input_approval_runtime.zig:71-130
  # How fx paints a draft: one row, a window ending at the caret.
  src/ui/footer/approval_ui.zig:1601-1673
  # `/rename` takes its title as a command's argument, in the composer.
  src/builtins/commands.zig:448-448
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
