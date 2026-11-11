#!/usr/bin/env bash
# Every line of the tree in the current directory that the check's patterns
# match, counted by one `git grep -c` and nothing else: the number the check's
# marked, placeholder and refused lines must add up to.
#
#   CHECK=<the check at d2b2ca75> count.sh
set -euo pipefail
# LC_ALL, the patterns and grep_args: lines 94, 121-132 and 193-196 at d2b2ca75.
eval "$(sed -n '94p;121,132p;193,196p' "${CHECK:?CHECK names the check at d2b2ca75}")"
git -c core.fsmonitor=false -c grep.column=false -c grep.fullName=false -C . --work-tree=. \
	grep -a --no-color -c -E "${grep_args[@]}" -- |
	awk -F: '{ n += $NF; f++ } END { printf "%d file(s), %d matching line(s)\n", f, n }'
