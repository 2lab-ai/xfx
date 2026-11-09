#!/usr/bin/env bash
# Train 9: the file checks with the receipt in its final form (train 8's
# gates-final.txt), in a `git clone --no-local` at <head>: `cargo fmt
# --check` and the three tracked-file checks, each exit from `$?` right after
# its command; then the dispatcher's check over `git diff <base>..HEAD`'s
# added lines.
# usage: suite-lock.sh gates-files.sh <clone> <base>
set -u
clone=$1 base=$2
cd "$clone" || exit 2
echo "# The file checks with the receipt in its final form, at $(git rev-parse --short=8 HEAD), in a \`git clone --no-local\`; each exit from \`\$?\` right after its command."
echo "# tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
failed=0
log=$(mktemp)
for cmd in "cargo fmt --check" ./scripts/check-xfx-identity.sh ./scripts/check-no-stubs.sh ./scripts/check-no-secrets.sh; do
  $cmd > "$log" 2>&1
  rc=$?
  echo "$cmd: exit $rc; $(tail -1 "$log")"
  [ "$rc" -eq 0 ] || failed=$((failed + 1))
done
# The four forms spelt with a bracket class, so this line does not match itself.
hits=$(git diff "$base"..HEAD | grep -E '^[+]' | grep -cE '/User[s]/zhugehyu[k]|/privat[e]/tmp/claude-|claude-50[1]|zhugehyu[k]')
echo "# added lines of git diff $base..HEAD holding the home root, the account's name, the scratch root or the uid fragment: $hits"
rm -f "$log"
exit $(( failed + (hits > 0) ))
