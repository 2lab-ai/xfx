#!/usr/bin/env bash
# The fourteen RED cases, one by one, on a checkout: each test's line and,
# for a failure, its panic message. usage: red_green.sh <checkout> <label>
set -u
cd "$1" || exit 2
echo "# $2: $(git rev-parse --short HEAD), $(git log -1 --format=%s | cut -c1-80)"
T=$(cargo test --locked --lib --no-run --message-format=json 2>/dev/null | python3 -c 'import sys,json
for l in sys.stdin:
    try: m=json.loads(l)
    except Exception: continue
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["kind"]==["lib"]: print(m["executable"])' | tail -1)
for t in a_failure_holds_every_read_off_for_upstreams_backoff a_held_catalog_is_read_again_by_the_menu a_refresh_that_fails_keeps_the_rows_says_so a_retry_reads_behind_the_failure the_retryable_failure_sentences_say_xfx a_revalidated_catalog_keeps_the_tab a_prompt_inside_a_failed_catalogs_backoff the_menu_over_a_held_catalog_reads_it_again a_turn_waits_two_seconds_at_most the_menu_over_a_held_catalog_lists_its_rows a_model_inside_a_failures_backoff_opens a_refresh_that_failed_keeps_the_rows_and_says a_prompt_past_a_failed_catalogs_backoff the_columns_frames_wait_out; do
  timeout 60 "$T" "$t" 2>&1 | grep -E -A3 "^test .*(FAILED|ok)$|panicked at" | grep -v "^note:\|^--$\|^$\|^failures:$" | sed 's/ ([0-9]*) panicked/ panicked/'
done
