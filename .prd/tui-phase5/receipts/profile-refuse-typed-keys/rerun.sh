#!/bin/bash
# Under one hold of the suite lock: the full cargo test once more, then each of
# the three catalog-wait clock tests three times alone.
set -u
W=$1 out=$2
cd "$W" || exit 2
echo "# head $(git rev-parse --short HEAD); load $(sysctl -n vm.loadavg | awk '{print $2}')"
cargo test --locked --no-fail-fast > "$out.full.log" 2>&1
echo "full cargo test exit $?: $(grep -E '^test result' "$out.full.log" | awk '{p+=$4; q+=$6} END {print p" passed, "q" failed"}')"
grep -E ' FAILED$' "$out.full.log"
for t in a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop \
  a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number \
  a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one; do
  for i in 1 2 3; do
    r=$(cargo test --locked -q --lib -- --exact "tui::worker::tests::$t" 2>&1 | grep -E '^test result' | cut -d';' -f1-2)
    echo "$t run $i (held): $r"
  done
done
