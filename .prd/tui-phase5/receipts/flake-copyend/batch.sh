#!/usr/bin/env bash
# One measured batch under load: N busy loops started here, the old test
# (base binary) and the new one (head binary) run M times each under
# background QoS (taskpolicy -b), and the loops stopped -- by the pids this
# script started -- before it exits, however it exits.
set -uo pipefail
S=$1 n=$2 m=$3
echo "# batch: $n busy loops, taskpolicy -b, $m runs each; start $(date +%T), load $(sysctl -n vm.loadavg)"
/usr/bin/perl "$S/busy.pl" "$n" 1200 "$S/batch.pids" &
gen=$!
trap 'kill $(cat "$S/batch.pids" 2>/dev/null) 2>/dev/null; kill $gen 2>/dev/null; wait $gen 2>/dev/null' EXIT
sleep 5
echo "## RED: base binary (e51685d6), an_ending_session_lets_a_copy_that_answers_land"
PREFIX="taskpolicy -b" bash "$S/runs.sh" "$S/lib-base" tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land "$m"
echo "## GREEN: head binary (042908ae), the same test"
PREFIX="taskpolicy -b" bash "$S/runs.sh" "$S/lib-head" tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land "$m"
echo "## GREEN: head binary, the two other converted tests"
PREFIX="taskpolicy -b" bash "$S/runs.sh" "$S/lib-head" tui::copy::tests::an_ending_session_kills_a_copy_whose_answer_comes_after_its_grace "$m"
PREFIX="taskpolicy -b" bash "$S/runs.sh" "$S/lib-head" tui::copy::tests::a_copy_is_started_and_answered_later_rather_than_waited_for "$m"
echo "# end $(date +%T), load $(sysctl -n vm.loadavg)"
