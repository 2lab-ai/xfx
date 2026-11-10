#!/usr/bin/env bash
# LOCAL-PATHS: the user's name (case-insensitive) and home path, built at run
# time, never written here, searched in every revision from <base> to <head>
# (`git grep -l`, all tracked files) and in every commit message after
# <base>. Prints the counts and, for a hit, the commit only.
#   names-scan.sh <repo> <base> <head>
set -u
repo=$1 base=$2 head=$3
name=$(id -un)
revs=0 bad=0 msgs=0 badmsg=0
for rev in "$base" $(git -C "$repo" rev-list --reverse "$base..$head"); do
	revs=$((revs + 1))
	a=$(git -C "$repo" grep -a -l -i -F -e "$name" "$rev" -- | wc -l | tr -d ' ')
	b=$(git -C "$repo" grep -a -l -F -e "$HOME" "$rev" -- | wc -l | tr -d ' ')
	[ "$a$b" = 00 ] || { bad=$((bad + 1)); echo "holds one: $(git -C "$repo" rev-parse --short=8 "$rev")"; }
	if [ "$rev" != "$base" ]; then
		msgs=$((msgs + 1))
		m=$(git -C "$repo" log -1 --format=%B "$rev")
		if printf '%s' "$m" | grep -q -i -F -e "$name" || printf '%s' "$m" | grep -q -F -e "$HOME"; then
			badmsg=$((badmsg + 1)); echo "message holds one: $(git -C "$repo" rev-parse --short=8 "$rev")"
		fi
	fi
done
echo "revisions $revs ($base to $head), holding the user's name or home path: $bad"
echo "commit messages $msgs, holding either: $badmsg"
