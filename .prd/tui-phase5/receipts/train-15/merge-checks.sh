#!/usr/bin/env bash
# Train 15's merge checks (train 14's merge-checks.sh, its merges, sides and
# checks this train's), each written into this folder through redacted.py:
# the merge trees' build checks (check-merge.sh's runs, kept in <scratch>/cm
# as each merge was made); every resolution redone in a clone with each
# resolver's lines (reproduce.sh); each merge from both ends, file by file
# (merge_ends.py); the token check, its self-tests, token_control.py and
# word_control.py; merge_keep.py; the remerge-diffs; the five reference scans
# before the re-point and at <head>; the content check (content.sh) at main
# (both), at each unit's tip, before the re-point, at the join of main and
# at <head> both ways; the two re-point plans and their checks; join check
# 1's and 3's functions (shared.py at each merge, functions.py across the
# commits, moved_edits.py); the overlaps; the test counts and every side's
# tests by name (tests_kept.py).
# usage: merge-checks.sh <repo> <head> <scratch>   (no lock: git and python only)
set -u
repo=$1 head=$2 scratch=$3
here=$(cd "$(dirname "$0")" && pwd)
rd() { python3 -I -B "$here/redacted.py" "$1" "$repo=<repo>" "$scratch=<scratch>"; }
P="python3 -I -B $here"
MERGES=(2ff302c5 ed92e515 b10ca771 88e83a68 07eefa89 a14df710)
PRE=0341dd2b          # the five merges and the join commit: before any re-point
JOIN=a14df710         # the join of main, before its re-point
REPOINTS=(da8e4c1a 2c575f52)
SIDES=(main=753b0a72 catalog-wait-clock=72fc9672 decoder=df45025c ftrh=fffb3cfe approval=7c9dfb2e question-ctrl-j=b5cb6f46)
{ echo "# each merge tree's checks before its commit (check-merge.sh, no lock), as run: merges 1-5 (m1..m5), the join commit 0341dd2b (j1), the join of main a14df710 (jm)"
  for f in m1 m2 m3 m4 m5 j1 jm; do echo "## $f"; cat "$scratch/cm/$f.out"; done; } 2>&1 | rd "$here/check-merges.txt"
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
  $P/$s.py "$repo" "$head" main=e2fc7e94 train=bc9ea199 "${SIDES[@]:1}" 2>&1 | rd "$here/$(echo "$s" | tr _ -)-head.txt"
done
for spec in "${SIDES[@]}" "main-now=e2fc7e94" "pre=$PRE" "train=bc9ea199" "join=$JOIN"; do
  c=${spec#*=}
  { echo "# content.sh at $c ($spec)"; bash "$here/content.sh" "$repo" "$c"; } 2>&1 | rd "$here/content-$c.txt"
done
{ echo "# content.sh at $head, blame ignoring the re-points ${REPOINTS[*]} (each re-pointed ref read against the frame that wrote its numbers)"
  bash "$here/content.sh" "$repo" "$head" $(printf -- '--ignore %s ' "${REPOINTS[@]}"); } 2>&1 | rd "$here/content-head-ignoring.txt"
{ echo "# content.sh at $head, blame as it is"; bash "$here/content.sh" "$repo" "$head"; } 2>&1 | rd "$here/content-head.txt"
SC=()
for spec in "${SIDES[@]}"; do SC+=(--side "${spec%%=*}=$here/content-${spec#*=}.txt"); done
$P/repoint.py "$repo" "$PRE" --content "$here/content-$PRE.txt" "${SC[@]}" --derive 2>&1 | rd "$here/repoint-plan.txt"
$P/repoint.py "$repo" "$JOIN" --content "$here/content-$JOIN.txt" --side main=$here/content-e2fc7e94.txt --side train=$here/content-bc9ea199.txt --derive 2>&1 | rd "$here/repoint-plan-2.txt"
masked() { # <commit>: lines out, lines in, the same once numbers are masked
  echo "# $(git -C "$repo" rev-parse --short=8 "$1"): $(git -C "$repo" show --format= -U0 "$1" | grep -c '^-[^-]') lines out, $(git -C "$repo" show --format= -U0 "$1" | grep -c '^+[^+]') in; the same once numbers are masked: $(diff <(git -C "$repo" show --format= -U0 "$1" | grep '^-[^-]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) <(git -C "$repo" show --format= -U0 "$1" | grep '^+[^+]' | cut -c2- | sed -E 's/:[0-9]+(-[0-9]+)?/:N/g' | sort) > /dev/null && echo True || echo False)"
}
{ echo "# the first plan at da8e4c1a, where its numbers stand"
  $P/repointed.py "$repo" da8e4c1a "$PRE" "$here/repoint-plan.txt" da8e4c1a; echo "# exit $?"
  masked da8e4c1a
  echo "# the second plan at 2c575f52"
  $P/repointed.py "$repo" 2c575f52 "$JOIN" "$here/repoint-plan-2.txt" 2c575f52; echo "# exit $?"
  masked 2c575f52; } 2>&1 | rd "$here/repointed.txt"
{ for spec in "1 2ff302c5 afd891af src/tui/worker.rs main=753b0a72 catalog-wait-clock=72fc9672" \
              "2 ed92e515 afd891af src/tui/shell.rs merge1=2ff302c5 decoder=df45025c" \
              "3 b10ca771 3f2d6e78 src/tui/full_transcript.rs merge2=ed92e515 ftrh=fffb3cfe" \
              "4 88e83a68 a37a969e src/tui/shell.rs merge3=b10ca771 approval=7c9dfb2e" \
              "4 88e83a68 a37a969e src/tui/approval.rs merge3=b10ca771 approval=7c9dfb2e" \
              "5 07eefa89 afd891af src/tui/shell.rs merge4=88e83a68 question-ctrl-j=b5cb6f46" \
              "5 07eefa89 afd891af src/tui/input.rs merge4=88e83a68 question-ctrl-j=b5cb6f46" \
              "5 07eefa89 afd891af src/tui/question.rs merge4=88e83a68 question-ctrl-j=b5cb6f46"; do
    set -- $spec
    echo "## merge $1 $2: base $3, $4, $5 and $6"
    $P/shared.py "$repo" "$3" "$2" "$4" "$5" "$6"; echo "# exit $?"
  done; } 2>&1 | rd "$here/shared.txt"
{ echo "# functions.py: each function's sha256 (12 hex digits, and its length in lines) per commit; then the lines that differ between the named pairs"
  $P/functions.py "$repo" src/tui/shell.rs:consume_inputs src/tui/shell.rs:decide src/tui/shell.rs:approval_action \
    src/tui/shell.rs:offer_to_rule_confirm src/tui/shell.rs:offer_to_pickers src/tui/shell.rs:offer_to_mcp \
    src/tui/shell.rs:folded:0 src/tui/shell.rs:arrow_to_pickers:0 src/tui/approval.rs:edits:0 \
    src/tui/question.rs:of src/tui/input.rs:tilde_key:0 -- \
    base-a=a37a969e base-d=afd891af decoder=df45025c approval=7c9dfb2e question-ctrl-j=b5cb6f46 main=753b0a72 \
    m1=2ff302c5 m2=ed92e515 m3=b10ca771 m4=88e83a68 m5=07eefa89 join=0341dd2b main-now=e2fc7e94 head="$head" \
    --diff m3..m4 --diff m4..m5 --diff m5..join; echo "# exit $?"; } 2>&1 | rd "$here/functions.txt"
{ echo "# moved_edits.py: the decoder's edits of decide's translation, which RULE-CONFIRM-KEYS moved into approval_action"
  $P/moved_edits.py "$repo" src/tui/shell.rs decide@a37a969e..b10ca771 approval_action@7c9dfb2e..88e83a68; echo "# exit $?"; } 2>&1 | rd "$here/moved-edits.txt"
$P/overlap.py "$repo" "${MERGES[@]}" 2>&1 | rd "$here/overlap.txt"
{ bash "$here/test_counts.sh" "$repo" 753b0a72 e2fc7e94 afd891af 72fc9672 df45025c 3f2d6e78 fffb3cfe a37a969e 7c9dfb2e b5cb6f46 \
    "${MERGES[@]}" 0341dd2b "$head"
  echo
  $P/tests_kept.py "$repo" "$head" main=afd891af..753b0a72 main-now=753b0a72..e2fc7e94 catalog-wait-clock=afd891af..72fc9672 \
    decoder=afd891af..df45025c ftrh=3f2d6e78..fffb3cfe approval=a37a969e..7c9dfb2e question-ctrl-j=afd891af..b5cb6f46 join=07eefa89..0341dd2b
  echo "# exit $?"; } 2>&1 | rd "$here/test-counts.txt"
echo "merge-checks: done"
