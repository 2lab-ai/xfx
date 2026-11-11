#!/bin/bash
set -uo pipefail
# The check's own definitions, read from the check at d2b2ca75 (CHECK names it),
# lines 94, 121-132, 181-186 and 193-196: LC_ALL, the patterns, scan_git, grep_args.
eval "$(sed -n '94p;121,132p;181,186p;193,196p' "${CHECK:?CHECK names the check at d2b2ca75}")"
n="$1"; th="$2"; bad=0; t="$(cd "$(dirname "$0")" && pwd)/fixed/shapes"; o="$(dirname "$t")/B.$th"; mkdir -p "$o"
cd "$t"
git -c grep.threads="$th" -c core.fsmonitor=false -C . --work-tree=. grep -a --no-color -l -z -E "${grep_args[@]}" -- >"$o/ref"
for i in $(seq 1 "$n"); do
	git -c grep.threads="$th" -c core.fsmonitor=false -c grep.column=false -c grep.fullName=false -c color.grep=never -c submodule.recurse=false -C . --work-tree=. grep -a --no-color -l -z -E "${grep_args[@]}" -- >"$o/got" 2>"$o/err"
	if ! cmp -s "$o/got" "$o/ref" || [ -s "$o/err" ]; then bad=$((bad+1)); cp "$o/got" "$o/bad.$i"; fi
	# the per-file pass for the two w3 files
	for p in w3/trace-report.md w3/xfx-trace-40x120-report-1.md; do
		c="$(git -c grep.threads="$th" -c core.fsmonitor=false -C . --work-tree=. grep -a --no-color -h -n -E "${grep_args[@]}" -- ":(literal)$p" | wc -l | tr -d ' ')"
		[ "$c" = 1 ] || { bad=$((bad+1)); echo "B-bad perfile iter=$i $p count=$c"; }
	done
done
echo "stageB threads=$th: $bad bad of $n (listing $(tr -cd '\0' <"$o/ref" | wc -c | tr -d ' ') entries)"
