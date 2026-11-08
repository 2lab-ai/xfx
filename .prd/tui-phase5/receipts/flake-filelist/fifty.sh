#!/usr/bin/env bash
#
# The receipt's 50 runs: the whole lib suite, fault-injection build, one run
# at a time, with `delay.patch` applied to the tree in the current directory.
#
#   git apply .prd/tui-phase5/receipts/flake-filelist/delay.patch
#   TMPDIR="${TMPDIR:-/tmp}/flake-filelist" \
#     bash .prd/tui-phase5/receipts/flake-filelist/fifty.sh <out-dir>
#   git checkout -- src/tui/file_index.rs
#
# Each run is bounded (`timeout -k 10 600`); one tab-separated row per run goes
# to <out-dir>/fifty.tsv, with the two cases the CI run failed named apart, and
# a failed run's whole output is kept beside it. The copy that ran had this
# machine's worktree and output paths written in; this one takes them from
# the current directory and <out-dir>, and is otherwise the same.

set -u

out="${1:?usage: fifty.sh <out-dir>}"
mkdir -p "$out"
: >"$out/fifty.tsv"
for i in $(seq 1 50); do
	log="$out/fifty-run-$i.txt"
	start=$(date +%s)
	timeout -k 10 600 cargo test --locked --lib --features fault-injection >"$log" 2>&1
	code=$?
	end=$(date +%s)
	case_of() {
		grep -E "^test tui::shell::tests::$1 \.\.\. " "$log" | awk '{print $NF}'
	}
	a=$(case_of a_choice_in_the_middle_of_a_line_reuses_the_space_after_it)
	d=$(case_of a_directory_choice_keeps_the_list_up_on_that_directory)
	result=$(grep -E "^test result" "$log" | tail -1)
	failed=$(grep -E " \.\.\. FAILED$" "$log" | awk '{print $2}' | tr '\n' ',')
	load=$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}')
	printf '%s\t%s\t%ss\tload=%s\ta_choice=%s\ta_directory=%s\t%s\t%s\n' \
		"$i" "$code" "$((end - start))" "$load" "$a" "$d" "$result" "$failed" >>"$out/fifty.tsv"
	if [ "$code" = 0 ]; then rm -f "$log"; fi
done
