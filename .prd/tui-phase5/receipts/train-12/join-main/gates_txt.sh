#!/bin/bash
# The join's gate logs, cut to the lines a verdict rests on, with every
# machine path replaced by a placeholder.
#
#   bash gates_txt.sh <log-dir> <repo> > gates.txt
#
# <log-dir> holds join-*.raw, one per step of the gate batch: a header
# (`== <step>: <command> (at <head>)`), start, the command's output, `exit=`,
# end. Kept from each: the header, start, end, exit, and the lines that
# count -- cargo's `test result:` lines and any failure, clippy's and fmt's
# errors, and the harness scripts' summaries and failures.
set -euo pipefail
dir=$1
repo=$2
uid=$(id -u)
for step in fmt clippy clippy-faults test test-faults test-sweep test-capture test-preview-contract build-release smoke-tui; do
	f="$dir/join-$step.raw"
	grep -E "^(== |start |end |exit=)|^test result:|FAILED|panicked|^error|^warning|^failures:|^    [a-z_:]+$|[0-9]+ (passed|failed|checks?|cases?|mutants?)|^Ran [0-9]+ tests|^OK( |$)|^ok( |$)|: ok( |$)|^PASS|^FAIL|caught|scenario" "$f" |
		grep -vE "^test .* \.\.\. ok$" |
		sed -e "s#$repo#<repo>#g" -e "s#$dir#<scratch>#g" -e "s#/private/tmp/claude-$uid#<tmpdir>#g" -e "s#/tmp/claude-$uid#<tmpdir>#g" -e "s#$HOME#<home>#g"
	echo
done
