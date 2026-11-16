#!/usr/bin/env bash
# Train 16's merge checks (train 15's merge-checks.sh, its merges, sides,
# files and functions this train's), each written into this folder through
# redacted.py: the merge trees' build checks (check-merge.sh's runs, kept in
# this folder's cm/ as each merge was made); every resolution redone in a
# clone with each resolver's lines (reproduce.sh); each merge from both ends,
# file by file (merge_ends.py); the token check, its self-tests,
# token_control.py and word_control.py; merge_keep.py; the remerge-diffs;
# the five reference scans before the re-points, at the first and at <head>;
# the content check (content.sh) at the base, at each unit's tip, before the
# re-points, at the first re-point and at <head> both ways; the re-point
# plans and their check; join check 3's functions (shared.py over every `.rs`
# file both sides of a merge changed, functions.py across the commits,
# moved_edits.py for merge 5's moved comment); the
# overlaps; the test counts and every side's tests by name (tests_kept.py).
# Round 2 (opus's MUST 1): the scans read comma lists (refs_content.py,
# refs_bare.py); the third re-point adb809f5 (TEXT's span) joins REPOINTS
# and is checked by extra_check.py --text, at it and at <head>, into
# extra-check.txt with the second's; `masked` masks a number with its list;
# and the plan is derived at <head> too (repoint-plan-head.txt: what is
# still WRONG there, and from where).
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MERGES=(b2d044c9 e8b004a5 849f5017 dd2ac93d 71787971 1bce5348)
PRE=622b2865          # the six merges and the join commit: before any re-point
REPOINTS=(c637ed4c 15c9744b adb809f5)
BASE=866747ff         # train 15's head, this train's base
SIDES=(base=$BASE profile=bacb1052 slash=32eb858f test-env=7c3f6c1e lpw=dd9f5f0a aps=ca496c10 mpd=6d94fbc0)
{ echo "# each merge tree's checks before its commit (check-merge.sh, no lock), as run: merges 1-6 (m1..m6; merge 3 twice, its first run before semantic.py's fixes: m3-first), the join commit 622b2865 (j1)"
  for f in m1 m2 m3-first m3 m4 m5 m6 j1; do echo "## $f"; cat "$here/cm/$f.out"; done; } 2>&1 | rd "$here/check-merges.txt"
{ echo "# every merge redone by this folder's scripts in a clone (reproduce.sh), each resolver's lines under it"
  bash "$here/reproduce.sh" "$repo" "$scratch"; echo "# exit $?"; } 2>&1 | rd "$here/resolutions.txt"
{ for m in "${MERGES[@]}"; do
    b=$(git -C "$repo" merge-base "$m^1" "$m^2")
    echo "## $m: base $(git -C "$repo" rev-parse --short=8 "$b"), ours $(git -C "$repo" rev-parse --short=8 "$m^1"), theirs $(git -C "$repo" rev-parse --short=8 "$m^2")"
    $P/merge_ends.py "$repo" "$b" "$m^1" "$m^2" "$m"; echo "# exit $?"
  done; } 2>&1 | rd "$here/merge-ends.txt"
{ echo "# token_check.py (train 7's, unchanged: $(cmp -s "$here/token_check.py" "$here/../train-7/token_check.py" && echo 'byte for byte the same file' || echo 'DIFFERENT from train 7s')) over the merges"
  $P/token_check.py "$repo" "${MERGES[@]}"; echo "# exit $?"; } 2>&1 | rd "$here/token-check.txt"
{ echo "# token_check16.py's check over the merges, against token_check.py's (train 7's): the same output: $(diff <($P/token_check.py "$repo" "${MERGES[@]}" 2>&1) <($P/token_check16.py "$repo" "${MERGES[@]}" 2>&1) > /dev/null && echo yes || echo NO)"
  for m in "${MERGES[@]}"; do
    echo "## token_check.py --self-test $m (train 7's, as it is; its last three lines)"
    $P/token_check.py "$repo" --self-test "$m" > "$scratch/self7.$m" 2>&1; echo "# exit $?"
    tail -3 "$scratch/self7.$m"
    echo "## token_check16.py --self-test $m (its two plants fixed)"
    $P/token_check16.py "$repo" --self-test "$m"; echo "# exit $?"
  done
  echo
  echo "# token_control.py: the two token cases planted on every merged row both sides changed"
  $P/token_control.py "$repo" "${MERGES[@]}"; echo "# exit $?"
  echo
  echo "# word_control.py: a word planted in \`why\` of every ledger entry both sides changed in numbers alone"
  for m in "${MERGES[@]}"; do echo "## $m"; $P/word_control.py "$repo" "$m"; echo "# exit $?"; done; } 2>&1 | rd "$here/token-self-test.txt"
{ echo "# merge_keep.py (the dispatcher's, train 14's review): every unit (cell, sentence, line; >= 25 chars) new on a side is in the merge"
  $P/merge_keep.py "$repo" "${MERGES[@]}"; echo "# exit $?"
  $P/merge_keep.py --normalize "$repo" "${MERGES[@]}"; echo "# exit $?"; } 2>&1 | rd "$here/merge-keep.txt"
i=0
for m in "${MERGES[@]}"; do
  i=$((i + 1))
  git -C "$repo" show --remerge-diff --format='commit %H%nMerge: %p%n%n%B' "$m" | rd "$here/remerge-$i.txt"
done
for s in refs refs_all refs_short refs_bare refs_prd; do
  $P/$s.py "$repo" "$PRE" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-pre.txt"
  $P/$s.py "$repo" "${REPOINTS[0]}" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-repoint1.txt"
  $P/$s.py "$repo" "$head" "${SIDES[@]}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
for spec in "${SIDES[@]}" "pre=$PRE" "repoint1=${REPOINTS[0]}"; do
  c=${spec#*=}
  { echo "# content.sh at $c ($spec)"; bash "$here/content.sh" "$repo" "$c"; } 2>&1 | rd "$here/content-$c.txt"
done
{ echo "# content.sh at $head, blame ignoring the re-points ${REPOINTS[*]} (each re-pointed ref read against the frame that wrote its numbers)"
  bash "$here/content.sh" "$repo" "$head" $(printf -- '--ignore %s ' "${REPOINTS[@]}"); } 2>&1 | rd "$here/content-head-ignoring.txt"
{ echo "# content.sh at $head, blame as it is"; bash "$here/content.sh" "$repo" "$head"; } 2>&1 | rd "$here/content-head.txt"
SC=()
for spec in "${SIDES[@]}"; do SC+=(--side "${spec%%=*}=$here/content-${spec#*=}.txt"); done
$P/repoint.py "$repo" "$PRE" --content "$here/content-$PRE.txt" "${SC[@]}" --derive 2>&1 | rd "$here/repoint-plan.txt"
$P/repoint.py "$repo" "${REPOINTS[0]}" --content "$here/content-${REPOINTS[0]}.txt" "${SC[@]}" --derive 2>&1 | rd "$here/repoint-plan-2.txt"
$P/repoint.py "$repo" "$head" --content "$here/content-head.txt" "${SC[@]}" --derive 2>&1 | rd "$here/repoint-plan-head.txt"
masked() { # <commit>: lines out, lines in, the same once numbers are masked
  echo "# $(git -C "$repo" rev-parse --short=8 "$1"): $(git -C "$repo" show --format= -U0 "$1" | grep -c '^-[^-]') lines out, $(git -C "$repo" show --format= -U0 "$1" | grep -c '^+[^+]') in; the same once numbers are masked: $(diff <(git -C "$repo" show --format= -U0 "$1" | grep '^-[^-]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*/:N/g' | sort) <(git -C "$repo" show --format= -U0 "$1" | grep '^+[^+]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*/:N/g' | sort) > /dev/null && echo True || echo False)"
}
{ echo "# the first plan as applied (repoint-plan-applied.txt: derived at $PRE before ${REPOINTS[0]}, train 14's two LEAVE entries alone), at ${REPOINTS[0]}, where its numbers stand"
  $P/repointed.py "$repo" "${REPOINTS[0]}" "$PRE" "$here/repoint-plan-applied.txt" "${REPOINTS[0]}"; echo "# exit $?"
  masked "${REPOINTS[0]}"
  echo "# the second plan, EXTRA's four read by hand, at ${REPOINTS[1]}: extra_check.py (repointed.py has no file for a bare :N)"
  $P/extra_check.py "$repo" "${REPOINTS[1]}"; echo "# exit $?"
  masked "${REPOINTS[1]}"
  echo "# the pinned ref put back, 2b78aab6"
  masked 2b78aab6
  echo "# the third re-point (round 2), TEXT's span, at ${REPOINTS[2]}: extra_check.py --text"
  $P/extra_check.py "$repo" "${REPOINTS[2]}" --text | grep -A 20 '^TEXT'; echo "# exit ${PIPESTATUS[0]}"
  masked "${REPOINTS[2]}"; } 2>&1 | rd "$here/repointed.txt"
{ for c in "${REPOINTS[1]}" "${REPOINTS[2]}" "$head"; do
    echo "# extra_check.py at $(git -C "$repo" rev-parse --short=8 "$c")$([ "$c" = "${REPOINTS[1]}" ] && echo ' (the second re-point: EXTRA alone)' || echo ' --text (EXTRA and TEXT)')"
    $P/extra_check.py "$repo" "$c" $([ "$c" = "${REPOINTS[1]}" ] || echo --text); echo "# exit $?"
    echo
  done; } 2>&1 | rd "$here/extra-check.txt"
{ for m in "${MERGES[@]}"; do
    b=$(git -C "$repo" merge-base "$m^1" "$m^2")
    for f in $(comm -12 <(git -C "$repo" diff --name-only "$b" "$m^1" | sort) <(git -C "$repo" diff --name-only "$b" "$m^2" | sort) | grep -E '\.rs$'); do
      echo "## merge $m ($f): base $(git -C "$repo" rev-parse --short=8 "$b"), ours $(git -C "$repo" rev-parse --short=8 "$m^1"), theirs $(git -C "$repo" rev-parse --short=8 "$m^2")"
      $P/shared.py "$repo" "$b" "$m" "$f" ours="$m^1" theirs="$m^2"; echo "# exit $?"
    done
  done; } 2>&1 | rd "$here/shared.txt"
{ echo "# functions.py: each function's sha256 (12 hex digits, and its length in lines) per commit; then the lines that differ between the named pairs"
  $P/functions.py "$repo" src/tui/approval_amendment.rs:apply:4 src/tui/shell.rs:offer_to_mcp_args src/tui/shell.rs:approval_action \
    src/tui/shell.rs:consume_inputs src/tui/input.rs:after_escape src/tui/input.rs:flush src/tui/input.rs:kitty_key:0 \
    src/tui/input.rs:prefixed:0 src/tui/worker.rs:recording -- \
    base=$BASE decoder=df45025c aps=ca496c10 mpd=6d94fbc0 slash=32eb858f test-env=7c3f6c1e \
    m1=b2d044c9 m2=e8b004a5 m3=849f5017 m4=dd2ac93d m5=71787971 m6=1bce5348 join=$PRE head="$head" \
    --diff m4..m5 --diff m5..m6 --diff base..m6; echo "# exit $?"; } 2>&1 | rd "$here/functions.txt"
{ echo "# moved_edits.py: APPROVAL-PASTE-SPACES' edit of decide's translation, which train 15's RULE-CONFIRM-KEYS moved into approval_action (merge 5)"
  $P/moved_edits.py "$repo" src/tui/shell.rs decide@df45025c..ca496c10 approval_action@dd2ac93d..71787971; echo "# exit $?"; } 2>&1 | rd "$here/moved-edits.txt"
$P/overlap.py "$repo" "${MERGES[@]}" 2>&1 | rd "$here/overlap.txt"
{ bash "$here/test_counts.sh" "$repo" $BASE 3943d667 bacb1052 afd891af 32eb858f 32afd307 7c3f6c1e 71d3e02b dd9f5f0a df45025c ca496c10 6d94fbc0 \
    "${MERGES[@]}" $PRE "$head"
  echo
  $P/tests_kept.py "$repo" "$head" profile=3943d667..bacb1052 slash=afd891af..32eb858f test-env=32afd307..7c3f6c1e \
    lpw=71d3e02b..dd9f5f0a aps=df45025c..ca496c10 mpd=df45025c..6d94fbc0 join=1bce5348..$PRE
  echo "# exit $?"; } 2>&1 | rd "$here/test-counts.txt"
echo "merge-checks: done"
