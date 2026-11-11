#!/usr/bin/env bash
# Train 14, ci-08066ac9 (new): WORKSPACE-INDEX-AT-PLAN's branch as this
# investigation read it -- its head, its base on main, its diff outside its
# receipt, and the lines of its README this folder cites (§1-3: no commit
# caused it; the root cause; the RED table). Output: wiap.txt.
# usage: wiap.sh <xfx-repo>
set -u
x=$1 b=fix/p5-workspace-index-at-plan
here=$(cd "$(dirname "$0")" && pwd)
{
  echo "# $b at $(git -C "$x" rev-parse --short=8 "$b"); its merge base with dcfa5146: $(git -C "$x" merge-base dcfa5146 "$b" | cut -c1-8), on main: $(git -C "$x" merge-base --is-ancestor "$(git -C "$x" merge-base dcfa5146 "$b")" dcfa5146 && echo yes || echo no)"
  echo "# git diff --stat dcfa5146...$b outside .prd/"
  git -C "$x" diff --stat "dcfa5146...$b" -- . ':!.prd' | sed 's/^/    /'
  echo "# its README, lines 9-24 and 81-100"
  git -C "$x" show "$b:.prd/tui-phase5/receipts/workspace-index-at-plan/README.md" | sed -n '9,24p;81,100p' | sed 's/^/    /'
} > "$here/wiap.txt" 2>&1
