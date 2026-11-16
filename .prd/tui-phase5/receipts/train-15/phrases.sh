#!/usr/bin/env bash
# Train 15 (new), the join of main: six phrasings #96 (v0.2.0's docs) took
# out of docs/parity.md, counted in the file at the head, at main, at the
# train before the join and at the train's base. The train added none, so
# the head holds as many as main (0). Output: phrases.txt.
# usage: phrases.sh <repo>
set -u
repo=$1
here=$(cd "$(dirname "$0")" && pwd)
{ echo "# docs/parity.md: occurrences at the head 2c575f52, main e2fc7e94, the train before the join bc9ea199, the base 753b0a72"
  for p in 'XFX_TUI=1' 'preview channel' 'preview-channel' 'zbrain' 'open-loop' 'Opt-in'; do
    printf '%-16s' "$p:"
    for c in 2c575f52 e2fc7e94 bc9ea199 753b0a72; do printf ' %s %s' "$c" "$(git -C "$repo" show "$c:docs/parity.md" | grep -o "$p" | wc -l | tr -d ' ')"; done
    echo
  done; } > "$here/phrases.txt"
