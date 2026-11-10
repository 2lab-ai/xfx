#!/usr/bin/env bash
# Train 13 (train 12's jobs.sh, its parts this train's), one hold of the suite
# lock per call, each part in a `git clone --no-local` of its own (never the
# worktree):
#   net     -- join check 5: net.py (the lib, tui and sessions tests through
#              a loopback proxy logger) at <head>;
#   mutant  -- the two join tests' teeth: at <head>, each unit's change the
#              join test rests on taken out, the join test run alone and
#              bounded by `timeout -k 10 900`, the clone restored after each:
#              j1-history (store.rs's N4 notice back, HISTORY-NOTICES-COPY-FX's
#              deleted lines), j1-resume (a saved document never decodes:
#              QUESTION-RESUME's decode_answers returns None), j2-word-left
#              (QUESTION-WORD-LEFT's Alt-b read as the plain word left).
# usage: suite-lock.sh jobs.sh <part> <repo> <head> <scratch> [<connect_logger.py>]
set -u
part=$1 repo=$2 head=$3 scratch=$4 logger=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
clone() { # <name> <commit>
  rm -rf "$scratch/$1"
  git clone -q --no-local "$repo" "$scratch/$1" && git -C "$scratch/$1" checkout -q --detach "$2"
  echo "# $1 at $(git -C "$scratch/$1" rev-parse --short=8 HEAD)"
}
echo "# jobs.sh $part at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
mutant)
  clone mutant-clone "$head"
  c="$scratch/mutant-clone"
  for m in j1-history j1-resume j2-word-left; do
    python3 -I -B "$here/join_mutants.py" "$c" "$m" || { echo "$m: not applied"; continue; }
    echo "$m: $(git -C "$c" diff --stat | tail -1)"
    case "$m" in
      j2-word-left) t=tui::shell::tests::a_question_after_a_resume_that_replayed_a_batch_takes_alt_b_as_before ;;
      *) t=tui::worker::tests::a_resumed_sealed_step_that_asked_replays_its_batch_and_says_nothing_of_its_reasoning ;;
    esac
    start=$SECONDS
    ( cd "$c" && timeout -k 10 900 cargo test --locked --lib -- --exact "$t" ) > "$scratch/mutant-$m.log" 2>&1
    rc=$?
    echo "$m: exit $rc, $((SECONDS - start)) s; $(grep -E '^test result' "$scratch/mutant-$m.log" | tail -1)"
    grep -E 'panicked at|^  left|^ right|assertion' "$scratch/mutant-$m.log" | head -4 | cut -c1-220 | sed 's/^/    /'
    git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
  done
  echo "# the clone clean after: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
