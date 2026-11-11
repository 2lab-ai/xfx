#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP: the path scan, commit by commit, over every commit
# this branch has that main does not: each non-merge commit's added lines and
# its message, each merge commit's message and its diff against its first
# parent, and the branch's whole diff over main; each under the brief's rule
# and the common brief's shorter one.
#
#   scan.sh <checkout> [<main ref>]     (default origin/main)
#
# One line per commit and a total; a commit's line names its counts, never a
# matched text.
set -u
cd "$1" || exit 2
main=${2:-origin/main}
# The two rules, put together from parts: written whole, three of their
# alternatives match their own text, and this file is tracked and scanned.
home='/Us''ers/' folders='/var/''folders/' user='zhuge''hyuk'
short="${home}[^/<]+/|/private/tmp/claude-[0-9]+|/tmp/claude-[0-9]+|${folders}"
long="${short}|${user}|claude-[0-9]+/|-[0-9]{3}/t[0-9]+/|fxs[w]\.[A-Za-z0-9_]{6}"
count() { grep -c -E "$1" || true; }
# The rules' own control: a line holding a home path, built from parts here
# (this file is tracked), must count once under each rule; else every 0 below
# proves nothing.
sample="+ a path: /Us""ers/some-one/x"
if [ "$(printf '%s\n' "$sample" | count "$long")" != 1 ] || [ "$(printf '%s\n' "$sample" | count "$short")" != 1 ]; then
	echo "scan: the rules do not match their own sample; the scan proves nothing"
	exit 1
fi
echo "# head $(git rev-parse --short HEAD), main $(git rev-parse --short "$main"); the control sample counts 1 under each rule"
total=0
for c in $(git rev-list --no-merges HEAD --not "$main"); do
	added=$(git show --format= "$c" | grep -E '^[+]')
	msg=$(git log -1 --format=%B "$c")
	a=$(printf '%s\n' "$added" | count "$long") b=$(printf '%s\n' "$added" | count "$short")
	m=$(printf '%s\n' "$msg" | count "$long")
	total=$((total + a + b + m))
	printf '%s added long=%s short=%s, message %s  %s\n' "$(git rev-parse --short "$c")" "$a" "$b" "$m" "$(git log -1 --format=%s "$c" | cut -c 1-60)"
done
for c in $(git rev-list --merges HEAD --not "$main"); do
	added=$(git diff "$c^1" "$c" | grep -E '^[+]')
	msg=$(git log -1 --format=%B "$c")
	a=$(printf '%s\n' "$added" | count "$long") b=$(printf '%s\n' "$added" | count "$short")
	m=$(printf '%s\n' "$msg" | count "$long")
	total=$((total + a + b + m))
	printf '%s merge, vs first parent long=%s short=%s, message %s  %s\n' "$(git rev-parse --short "$c")" "$a" "$b" "$m" "$(git log -1 --format=%s "$c" | cut -c 1-60)"
done
added=$(git diff "$main..HEAD" | grep -E '^[+]')
a=$(printf '%s\n' "$added" | count "$long") b=$(printf '%s\n' "$added" | count "$short")
total=$((total + a + b))
printf '%s..HEAD long=%s short=%s\n' "$main" "$a" "$b"
echo "total: $total"
