#!/bin/bash
set -uo pipefail
# The check's own definitions, read from the check at d2b2ca75 (CHECK names it),
# lines 94 and 121-158: LC_ALL, the patterns, the placeholders.
eval "$(sed -n '94p;121,158p' "${CHECK:?CHECK names the check at d2b2ca75}")"
n="$1"; bad=0; s=/
l1="Workspace: ${s}home${s}bob${s}work  # local-path-ok"
l2="Workspace: ${s}home${s}bob${s}work"
for i in $(seq 1 "$n"); do
  for text in "$l1" "$l2"; do
	bare="$text "
	while [[ $bare =~ $placeholder_home_re ]]; do found="${BASH_REMATCH[0]}"; bare="${bare%%"$found"*}${bare#*"$found"}"; done
	while [[ $bare =~ $allowed_re ]]; do found="${BASH_REMATCH[0]}" after="${BASH_REMATCH[2]}"; bare="${bare%%"$found"*}${after}${bare#*"$found"}"; done
	hit=""
	for entry in "${patterns[@]}"; do
		if [[ $bare =~ ${entry#*:} ]]; then hit="${hit:+$hit, }${entry%%:*}"; fi
	done
	[ "$hit" = "a Linux home directory" ] || { bad=$((bad+1)); echo "D-bad iter=$i text=$text hit=$hit"; }
	if [[ $l2 =~ $marked_re ]]; then bad=$((bad+1)); echo "D-bad marked false positive iter=$i"; fi
  done
done
echo "stageD: $bad bad of $((n*2))"
