#!/usr/bin/env bash
# Train 14's merge checks (train 13's merge-checks.sh and its join-main's
# checks.sh, this train's merges, sides and checks), each written into this
# folder through redacted.py: the merge trees' build checks (check-merge.sh's
# runs, kept in <scratch> as each merge was made); every resolution redone in
# a clone with each resolver's lines (reproduce.sh); each merge from both
# ends, file by file (merge_ends.py); the token check, its self-tests and
# token_control.py; the remerge-diffs; the five reference scans before the
# re-point and at <head>; the content check (content.sh) at main, at each
# unit's tip, before the re-point and at <head> both ways; the re-point plan
# and its check; join check 3's functions (shared.py at each merge, and
# functions.py across the commits, join check 4's `input_row_limit` too); the
# overlaps; the test counts.
# A fourth argument `refs` starts at the reference scans (the parts before
# them are the merges', which no later commit changes).
# usage: merge-checks.sh <repo> <head> <scratch> [refs]   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3 from=${4:-all}
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MERGES=(d548c078 ecad8518 7a1773d9 fb6dfdb8)
PRE=8f622949          # the merges, the join commit and the first ledger commit: before any re-point
REPOINTS=(04e4491d 6bc758a0 3a2830fb b3dffdad)  # the plan's two; the six the test fix 725b3c90 moved; two bare continuations refs_bare.py read MOVED
PLANNED=6bc758a0      # the commit the plan's numbers stand at (725b3c90 moved six of them again)
SIDES=(main=dcfa5146 at-stack=4130f4ac ctrlo-other-pickers=e7e1bcf6 page-keys=3ea16930 image-loss=bde1ddfa)
if [ "$from" = all ]; then
{ echo "# each tree's checks before its commit (check-merge.sh, no lock), as run"
  for f in check-m1 check-m2 check-m3 check-m4 check-j1 check-rp check-rp2; do cat "$scratch/$f.txt"; done; } 2>&1 | rd "$here/check-merges.txt"
{ echo "# every merge redone by this folder's scripts in a clone (reproduce.sh), each resolver's lines under it"
  bash "$here/reproduce.sh" "$repo" "$scratch"; echo "# exit $?"; } 2>&1 | rd "$here/resolutions.txt"
{ for m in "${MERGES[@]}"; do
    b=$(git -C "$repo" merge-base "$m^1" "$m^2")
    echo "## $m: base $(git -C "$repo" rev-parse --short=8 "$b"), ours $(git -C "$repo" rev-parse --short=8 "$m^1"), theirs $(git -C "$repo" rev-parse --short=8 "$m^2")"
    $P/merge_ends.py "$repo" "$b" "$m^1" "$m^2" "$m"; echo "# exit $?"
  done; } 2>&1 | rd "$here/merge-ends.txt"
{ echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  $P/token_check.py "$repo" "${MERGES[@]}"; echo "# exit $?"; } 2>&1 | rd "$here/token-check.txt"
{ for m in "${MERGES[@]}"; do $P/token_check.py "$repo" --self-test "$m"; echo "# exit $?"; done
  echo
  echo "# token_control.py: the two token cases planted on every merged row both sides changed"
  $P/token_control.py "$repo" "${MERGES[@]}"; echo "# exit $?"; } 2>&1 | rd "$here/token-self-test.txt"
i=0
for m in "${MERGES[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
fi
for s in refs refs_all refs_short refs_bare refs_prd; do
  $P/$s.py "$repo" "$PRE" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-pre.txt"
  $P/$s.py "$repo" "$head" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
for spec in "${SIDES[@]}" "pre=$PRE" "test-fix=725b3c90"; do
  c=${spec#*=}
  { echo "# content.sh at $c ($spec)"; bash "$here/content.sh" "$repo" "$c"; } 2>&1 | rd "$here/content-$c.txt"
done
{ echo "# content.sh at $head, blame ignoring the re-points ${REPOINTS[*]} (each re-pointed ref read against the frame that wrote its numbers)"
  bash "$here/content.sh" "$repo" "$head" $(printf -- '--ignore %s ' "${REPOINTS[@]}"); } 2>&1 | rd "$here/content-head-ignoring.txt"
{ echo "# content.sh at $head, blame as it is"; bash "$here/content.sh" "$repo" "$head"; } 2>&1 | rd "$here/content-head.txt"
SC=()
for spec in "${SIDES[@]}"; do SC+=(--side "${spec%%=*}=$here/content-${spec#*=}.txt"); done
$P/repoint.py "$repo" "$PRE" --content "$here/content-$PRE.txt" "${SC[@]}" --derive 2>&1 | rd "$here/repoint-plan.txt"
{ echo "# at $PLANNED, where the plan's numbers stand; the third re-point's six are content-725b3c90.txt's WRONG and content-head.txt's not"
  $P/repointed.py "$repo" "$PLANNED" "$PRE" "$here/repoint-plan.txt" "${REPOINTS[@]:0:2}"; echo "# exit $?"
  for c in 3a2830fb b3dffdad; do
    echo "# $(git -C "$repo" rev-parse --short=8 "$c"): $(git -C "$repo" show --format= -U0 "$c" | grep -c '^-[^-]') lines out, $(git -C "$repo" show --format= -U0 "$c" | grep -c '^+[^+]') in; the same once numbers are masked: $(diff <(git -C "$repo" show --format= -U0 "$c" | grep '^-[^-]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) <(git -C "$repo" show --format= -U0 "$c" | grep '^+[^+]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) > /dev/null && echo True || echo False)"
  done; } 2>&1 | rd "$here/repointed.txt"
{ echo "## merge 1 d548c078: base af2b6137, main dcfa5146, the @ stack 4130f4ac"
  $P/shared.py "$repo" af2b6137 d548c078 src/tui/shell.rs main=dcfa5146 at-stack=4130f4ac; echo "# exit $?"
  echo "## merge 2 ecad8518: base cb7c9c18, merge 1 d548c078, CTRLO-OTHER-PICKERS e7e1bcf6"
  $P/shared.py "$repo" cb7c9c18 ecad8518 src/tui/shell.rs merge1=d548c078 ctrlo-other-pickers=e7e1bcf6; echo "# exit $?"
  echo "## merge 3 7a1773d9: base 3d3d56c8, merge 2 ecad8518, PAGE-KEYS-SETTINGS-COLUMN 3ea16930"
  $P/shared.py "$repo" 3d3d56c8 7a1773d9 src/tui/shell.rs merge2=ecad8518 page-keys=3ea16930; echo "# exit $?"; } 2>&1 | rd "$here/shared.txt"
{ echo "# functions.py: each function's sha256 (12 hex digits, and its length in lines) per commit; then the lines that differ between the named pairs"
  $P/functions.py "$repo" src/tui/shell.rs:consume_inputs src/tui/shell.rs:offer_to_pickers src/tui/shell.rs:composer_newline \
    src/tui/shell.rs:act src/tui/shell.rs:composer_control src/tui/shell.rs:offer_to_files src/tui/shell.rs:anchor_at \
    src/tui/layout.rs:input_row_limit:0 -- \
    base-stack=af2b6137 main=dcfa5146 ctrlo-at-keys=cb7c9c18 ctrlo-other-pickers=e7e1bcf6 base-pk=3d3d56c8 page-keys=3ea16930 \
    m1=d548c078 m2=ecad8518 m3=7a1773d9 m4=fb6dfdb8 join=594fddc8 test-fix=725b3c90 head="$head" \
    --diff ctrlo-at-keys..ctrlo-other-pickers --diff base-pk..page-keys --diff m2..m3 --diff m4..join; echo "# exit $?"; } 2>&1 | rd "$here/functions.txt"
$P/overlap.py "$repo" "${MERGES[@]}" 2>&1 | rd "$here/overlap.txt"
bash "$here/test_counts.sh" "$repo" dcfa5146 af2b6137 4130f4ac cb7c9c18 e7e1bcf6 3d3d56c8 3ea16930 714f6283 bde1ddfa \
  "${MERGES[@]}" 594fddc8 725b3c90 "$head" 2>&1 | rd "$here/test-counts.txt"
echo "merge-checks: done"
