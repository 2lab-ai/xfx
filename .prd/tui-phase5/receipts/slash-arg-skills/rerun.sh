#!/usr/bin/env bash
# The three catalog-wait cases alone, three times each, fault-injection build.
set -uo pipefail
cd "$1"
for i in 1 2 3; do
  for t in a_stop_in_the_catalog_wait_of_a_turn_that_would_ask_twenty_questions_ends_it_as_the_stop a_stop_in_a_turns_catalog_wait_ends_it_at_once_and_the_next_start_keeps_its_number a_stop_in_a_turns_catalog_wait_ends_it_as_a_stop_and_records_it_as_one; do
    out=$(cargo test --locked --features fault-injection --lib -- --exact "tui::worker::tests::$t" 2>&1 | grep -E '^test result')
    echo "run $i $t: $out"
  done
done
