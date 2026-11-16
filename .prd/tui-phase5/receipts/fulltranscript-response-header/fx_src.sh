#!/usr/bin/env bash
# Writes fx-src.txt: every fx v0.0.16 line this receipt cites, read with
# `git show 481104fd:<path>` from a checkout of fx (never checked out or reset),
# numbered as the file numbers them.
#
# Then checks the citations: every `<file>.zig:<a>[-<b>]` and every bare
# `:<a>[-<b>]` after one (same file) in README.md and in whatever text is
# piped in, wherever it names `481104fd` or no pin, must fall inside one of
# the ranges below. Citations at another pin (`c1db919`) are not this file's.
# MODIFIED-PAGE-KEYS' checker, with this unit's ranges.
#
# usage: fx_src.sh <fx checkout>   (writes fx-src.txt beside this script)
#   the unit's added lines, piped:
#   { git diff 3f2d6e78 -- src tests scripts/fxcap/scenarios | grep '^+';
#     git diff --word-diff=porcelain 3f2d6e78 -- docs/parity.md | grep '^+'; } |
#     fx_src.sh <fx checkout>
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fx=${1:?usage: fx_src.sh <fx checkout>}
pin=481104fd
ranges=(
  # The screen: the header written before an entry, its time, and the kinds
  # that have one; the header's text.
  src/ui/full_transcript_screen.zig:5934-5974
  src/ui/full_transcript_screen.zig:6028-6056
  src/core/output/full_transcript_metadata.zig:1-38
  # The entries and their times; how each is rendered (an assistant entry's
  # head trimmed of line breaks); the full presentation's loop, which skips an
  # entry that renders nothing before it writes its header.
  src/ui/render_engine/transcript_blocks.zig:300-330
  src/ui/render_engine/transcript_blocks.zig:400-410
  src/ui/render_engine/transcript_blocks.zig:1997-2046
  src/ui/render_engine/transcript_blocks.zig:2073-2122
  src/ui/render_engine/transcript_blocks.zig:2226-2228
  src/ui/render_engine/transcript_blocks.zig:2844-2848
  # The store: a table, a code block, a rule each its own entry stamped when
  # it is appended; text joins the tail assistant entry, or opens one stamped
  # when its chunk lands.
  src/ui/transcript/store.zig:2947-2953
  src/ui/transcript/store.zig:2955-3075
  src/ui/transcript/store.zig:3088-3205
  src/ui/transcript/store.zig:3661-3715
  # The presenter: lines put together byte by byte, and where each block
  # completes; the stream hands the text before a block on first.
  src/core/agent/assistant_presentation.zig:45-50
  src/core/agent/assistant_presentation.zig:202-261
  src/core/agent/assistant_presentation.zig:263-418
  src/core/agent/assistant_presentation.zig:483-543
  src/core/agent/runtime/assistant_stream.zig:130-134
  src/core/agent/runtime/assistant_stream.zig:387-460
  src/core/agent/runtime/assistant_stream.zig:462-510
  src/core/agent/runtime/assistant_stream.zig:512-564
  # The road from the worker to the store: text through the pacer, a block
  # after the pacer is drained.
  src/core/app/app_callbacks.zig:1236-1245
  src/core/app/app_callbacks.zig:1450-1498
  src/core/app/app_worker_runtime.zig:551-578
  src/core/app/app_worker_runtime.zig:1024-1053
  src/main.zig:2552-2567
  src/ui/assistant/pacer.zig:193-196
  src/ui/assistant/pacer.zig:257-294
  src/ui/assistant/pacer.zig:348-364
  # A resumed answer: every entry of it stamped with the turn's completion.
  src/core/app/app_session_runtime.zig:3986-4003
  src/core/app/app_session_runtime.zig:4720-4805
  src/ui/transcript/resume_projection.zig:76-78
  src/ui/transcript/resume_projection.zig:150-193
  # `/trace`'s names for the entries, and the entries it lists (round 2:
  # the whole function, its skip of an entry with no text).
  src/core/app/app_commands.zig:2675-2747
  # Round 2 (opus SHOULD 1): until a step shows visible text, fx skips the
  # leading space, tab, CR and LF of each chunk and drops a chunk that is
  # nothing else; the stream's context is built afresh for every step. A
  # notice is an entry appended at the store's tail.
  src/core/agent/runtime/assistant_stream.zig:666-670
  src/core/agent/runtime/orchestrator.zig:7308-7325
  src/ui/transcript/store.zig:1301-1323
  # Round 2 (NIT 3): which entries the full projection skips (and marks):
  # the entries the tool-group projection hides -- a group's later
  # statuses, a call's attached output and diff rows -- and a command's
  # owned source; a question's status is kept.
  src/ui/full_transcript_screen.zig:5886-5897
  src/ui/transcript/tool_group_projection.zig:52-57
  src/ui/transcript/tool_group_projection.zig:235-251
  src/ui/transcript/tool_group_projection.zig:948-962
  src/ui/transcript/tool_group_projection.zig:1321-1335
  src/ui/transcript/tool_group_projection.zig:1355-1359
  src/ui/transcript/tool_group_projection.zig:1366-1383
  # Round 3: the reader across a rebuild -- the pin, the selection, the
  # bookmark's resolution (the nearest marked entry before, else after).
  src/core/output/transcript_presentation.zig:144-202
  src/core/output/transcript_presentation.zig:278-298
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
