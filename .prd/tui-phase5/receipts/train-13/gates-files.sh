#!/usr/bin/env bash
# Train 13 (train 12's gates-files.sh, its folder train-13 and the pattern
# train 13's brief's eight terms; train 12's: LOCAL-PATHS's check among the
# file checks and the brief's whole pattern): the file checks with the
# receipt in its final form, in a `git clone --no-local` at <head>: `cargo fmt
# --check` and the four tracked-file checks, each exit from `$?` right after
# its command; the code since <gate> (`git diff --stat <gate> HEAD` outside
# this folder), empty when no code changed after the gate; then the brief's
# check over `git diff <base>..HEAD`'s added lines (0 for this train's range).
# The pattern is spelt with bracket classes, so this file does not match itself.
# usage: suite-lock.sh gates-files.sh <clone> <base> <gate>
set -u
clone=$1 base=$2 gate=$3
cd "$clone" || exit 2
echo "# The file checks with the receipt in its final form, at $(git rev-parse --short=8 HEAD), in a \`git clone --no-local\`; each exit from \`\$?\` right after its command."
echo "# tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
echo "# code since the gate $gate, outside .prd/tui-phase5/receipts/train-13: $(git diff --stat "$gate" HEAD -- . ':!.prd/tui-phase5/receipts/train-13' | tail -1 | grep . || echo none)"
failed=0
log=$(mktemp)
for cmd in "cargo fmt --check" ./scripts/check-xfx-identity.sh ./scripts/check-no-stubs.sh ./scripts/check-no-secrets.sh ./scripts/check-no-local-paths.sh; do
  $cmd > "$log" 2>&1
  rc=$?
  echo "$cmd: exit $rc; $(tail -1 "$log")"
  [ "$rc" -eq 0 ] || failed=$((failed + 1))
done
P='/User[s]/[^/<]+/|/privat[e]/tm[p]/claude-[0-9]+|/tm[p]/claude-[0-9]+|/var/folder[s]/|zhugehyu[k]|claud[e]-[0-9]+/|-[0-9]{3}/[t][0-9]+/|fx[s][w]\.[A-Za-z0-9_]{6}'
echo "# added lines of git diff $base..HEAD holding the brief's pattern: $(git diff "$base"..HEAD | grep -E '^[+]' | grep -cE "$P")"
echo "# of them in this folder: $(git diff "$base"..HEAD -- .prd/tui-phase5/receipts/train-13 | grep -E '^[+]' | grep -cE "$P")"
rm -f "$log"
exit "$failed"
