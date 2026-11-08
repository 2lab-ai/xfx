#!/usr/bin/env bash
# MODELS-REASK round 2: each RED case on the checkout of the commit that added
# it (it fails there), and every round-2 case at the head (each passes): the
# test's line and, for a failure, its panic message.
# usage: red_green2.sh <clone> <head-sha>
set -u
cd "$1" || exit 2
lib_test() {
  cargo test --locked --lib --no-run --message-format=json 2>/dev/null | python3 -c 'import sys,json
for l in sys.stdin:
    try: m=json.loads(l)
    except Exception: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["kind"]==["lib"]: print(m["executable"])' | tail -1
}
run_cases() {
  local T=$1; shift
  for t in "$@"; do
    timeout 120 "$T" "$t" 2>&1 | grep -E -A3 "^test .*(FAILED|ok)$|panicked at" | grep -v "^note:\|^--$\|^$\|^failures:$\|^---- " | sed 's/ ([0-9]*) panicked/ panicked/'
  done
}
R1="a_read_that_finds_no_runtime_tells_its_failure_as_every_read_tells_its_answer a_read_that_finds_no_runtime_tells_the_band_its_failure_and_is_read_again_past_its_backoff every_answer_lets_activity_ask_again_a_refresh_failure_over_a_failed_band_too"
R2="a_read_asked_for_mid_work_that_finds_no_runtime_is_told_at_once_and_once"
R3="rows_a_slow_read_brought_are_read_again_five_minutes_after_it_began a_refresh_that_failed_for_good_is_read_again_when_the_runtimes_clock_says the_band_is_told_rows_are_due_again_five_minutes_after_their_read_began a_failed_refresh_is_due_again_when_the_runtimes_clock_says"
R4="a_request_carrying_tool_images_waits_for_the_retry_behind_a_failed_catalog the_image_route_waits_two_seconds_at_most_and_once_a_turn a_prompts_images_wait_for_the_retry_behind_a_failed_catalog"
PINS="a_read_that_found_no_runtime_ends_the_menus_wait_and_activity_asks_again_past_its_hold activity_is_due_exactly_when_the_clock_says_it_is the_historys_images_wait_at_the_image_route_and_not_at_the_turns_start the_catalog_as_held_is_told_with_the_runtimes_clock"
for pair in "92ad8f13:$R1" "38a96c55:$R2" "3ad38109:$R3" "39ccc85d:$R4"; do
  sha=${pair%%:*}; cases=${pair#*:}
  git checkout -q "$sha" || exit 3
  echo "# RED at $(git rev-parse --short HEAD): $(git log -1 --format=%s | cut -c1-90)"
  T=$(lib_test); [ -n "$T" ] || { echo "no test binary"; exit 4; }
  run_cases "$T" $cases
done
git checkout -q "$2" || exit 3
echo "# GREEN at $(git rev-parse --short HEAD): $(git log -1 --format=%s | cut -c1-90)"
T=$(lib_test); [ -n "$T" ] || { echo "no test binary"; exit 4; }
run_cases "$T" $R1 $R2 $R3 $R4
echo "# beside them at the head: the band's half of the no-runtime read, and the pins added after the fixes"
run_cases "$T" $PINS
