#!/usr/bin/env bash
# Train 14, ci-08066ac9 (new): the code path behind row 13 of
# `workspace-index` `at-plan`, each line printed from git at the revision
# named (fx's from its upstream checkout at the pin). Output: trace.txt.
# usage: trace.sh <xfx-repo> <fx-upstream-repo>
set -u
x=$1 f=$2
here=$(cd "$(dirname "$0")" && pwd)
show() { # <repo> <rev> <path> <from> <to> <what>
  echo "## $6 -- $3:$4-$5@$2"
  git -C "$1" show "$2:$3" | sed -n "$4,$5p" | sed 's/^/    /'
}
{
  echo "# 1. Each side's run directory: the sweep's shared one, then each capture's own"
  for c in dcfa5146 b3dffdad; do
    show "$x" "$c" scripts/fxcap/sweep.py 151 151 "the sweep's root, \`fxsw.\` and eight random letters, one a sweep"
    show "$x" "$c" scripts/fxcap/sweep.py 178 191 "a unit's runs one after the other in one TMPDIR, <root>/<NNN>"
    n=$(git -C "$x" show "$c:scripts/fxcap/capture.sh" | grep -n 'run=$(mktemp -d "$tmpdir/fxcap.XXXXXX")' | cut -d: -f1)
    show "$x" "$c" scripts/fxcap/capture.sh "$n" "$n" "each capture's own run directory, \`fxcap.\` and six random letters, other for fx and for xfx"
  done
  echo
  echo "# 2. fx v0.0.16: an added root's entry is its absolute path; listed only if it holds the query's letters in order"
  show "$f" 481104fd src/core/workspace/file_index.zig 1095 1099 "fx: an added root's candidate is displayed (and matched) absolute"
  show "$f" 481104fd src/core/workspace/file_index.zig 1802 1804 "fx: no placement in the name and no subsequence -- not listed"
  show "$f" 481104fd src/core/workspace/file_index.zig 1815 1818 "fx: else the best placement anywhere in the whole path"
  show "$f" 481104fd src/core/workspace/file_index.zig 1821 1830 "fx: the subsequence test"
  echo
  echo "# 3. xfx at the code head b3dffdad (AT-RANK's placement, fx's rule)"
  show "$x" b3dffdad src/tui/file_index.rs 1556 1563 "an added root's entry is its absolute path"
  show "$x" b3dffdad src/tui/file_index.rs 121 124 "its letters, from that path, lower-cased"
  show "$x" b3dffdad src/tui/file_index.rs 1178 1182 "rejected unless every letter of the query is among the path's"
  show "$x" b3dffdad src/tui/file_index.rs 1104 1110 "no placement in the name and no subsequence -- not listed"
  show "$x" b3dffdad src/tui/file_index.rs 1132 1141 "the subsequence test"
  echo
  echo "# 4. xfx at the base dcfa5146 (main's ranking): the same test, by another road"
  show "$x" dcfa5146 src/tui/file_index.rs 838 846 "an added root's entry is its absolute path"
  show "$x" dcfa5146 src/tui/file_index.rs 547 575 "the name first, then the whole path from its first letter"
  show "$x" dcfa5146 src/tui/file_index.rs 576 586 "score_range: the query's letters in order from <start>, greedy"
  show "$x" dcfa5146 src/tui/file_index.rs 612 615 "...and none when they are not all found in order"
} > "$here/trace.txt" 2>&1
