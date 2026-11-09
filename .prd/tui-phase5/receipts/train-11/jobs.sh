#!/usr/bin/env bash
# Train 11 (train 10's jobs.sh, its parts and lists this train's), one hold of
# the suite lock per call, each part in a `git clone --no-local` of its own
# (never the worktree):
#   named   -- at <head>: the two join tests, the units' tests on each meeting
#              and merge 2's three semantic-fix tests, by name (lib, then the
#              pty tests);
#   alone   -- each join test on merge 1 (4a0b900b: main and
#              SUBMIT-CARD-PREVIEW, neither PASTE-GAP nor CATALOG-REFRESH),
#              put in as apply_join.py puts it, with the test-code changes
#              that side needs (said below), its verdict and its first failing
#              line;
#   mutants -- join_mutants.py at <head>;
#   net     -- join check 5: net.py (the lib and tui tests through a loopback
#              proxy logger) at <head>.
# usage: suite-lock.sh jobs.sh <part> <repo> <head> <scratch> [<connect_logger.py>]
set -u
part=$1 repo=$2 head=$3 scratch=$4 logger=${5:-}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$scratch"
clone() { # <name> <commit>
  rm -rf "$scratch/$1"
  git clone -q --no-local "$repo" "$scratch/$1" && git -C "$scratch/$1" checkout -q --detach "$2"
  echo "# $1 at $(git -C "$scratch/$1" rev-parse --short=8 HEAD)"
}
JP=tui::event_loop::tests::a_tall_paste_with_enters_behind_its_marker_and_in_its_quarantine_previews_once_after_it
JC=tui::event_loop::tests::a_submit_that_previews_asks_a_due_catalog_once_and_writes_the_bytes_it_writes_without_one
LIB=(
  # the join tests
  "$JP" "$JC"
  # join check 1's hops: PASTE-AFTER-END's and PASTE-GAP's decoder, SUBMIT-CARD-PREVIEW's preview
  tui::event_loop::tests::a_delivery_past_the_32nd_read_goes_on_into_the_next_turn_unsettled
  tui::event_loop::tests::a_readiness_gap_inside_one_paste_does_not_turn_the_rest_of_it_into_keys
  tui::event_loop::tests::a_turn_cut_short_by_a_signal_or_a_read_that_would_wait_settles_nothing
  tui::event_loop::tests::a_stream_that_fills_every_read_after_a_paste_is_still_said_at_the_read_cap
  tui::shell::tests::a_readiness_gap_behind_a_paste_s_end_marker_does_not_make_the_rest_a_prompt
  tui::event_loop::tests::the_tick_that_shows_a_preview_writes_the_card_in_the_next_frame
  tui::event_loop::tests::a_tall_preview_leaves_the_card_once_across_a_resize_after_its_tick
  tui::event_loop::tests::a_draft_and_its_enter_in_one_read_measure_the_room_the_last_frame_left
  # join check 2's: CATALOG-REFRESH's activity, clock and hold
  tui::shell::tests::a_prompt_past_a_failed_catalogs_backoff_asks_for_it_and_one_inside_asks_nothing
  tui::event_loop::tests::the_model_columns_frame_reads_the_catalogs_clock
  tui::worker::tests::the_catalog_is_read_by_a_first_load_the_menu_and_activity_only_and_never_while_idle
  tui::shell::tests::rows_and_a_failure_read_before_the_machine_slept_are_due_on_waking
  tui::shell::tests::a_reading_earlier_than_the_last_reads_start_owes_the_catalog_a_read
  # join check 3's: PASTE-GAP's reconcile at the end of every turn
  tui::shell::tests::the_quarantine_s_sentence_leaves_the_review_plane_when_the_quarantine_ends
  tui::shell::tests::the_plane_s_sentence_goes_in_a_turn_whose_burst_ended_with_neither_check
  tui::shell::tests::the_plane_s_sentence_goes_when_a_paste_begins_in_a_turn_that_ends_at_the_read_cap
  # join check 5's: the shared fixture's runtime reads only its own home
  tui::worker::tests::the_shared_fixtures_runtime_reads_only_its_own_home
  # merge 2's semantic fixes
  tui::shell::tests::a_catalog_read_that_fails_too_large_or_stalled_then_loads_ranks_in_three_tiers_and_keeps_its_mark_by_id
  tui::worker::tests::a_told_ask_put_aside_behind_a_switch_starts_the_first_load_once_and_never_a_refresh
  tui::worker::tests::a_switch_drops_the_old_providers_rows_read_and_backoff_and_keeps_the_clock
)
TUI=(
  a_paste_whose_end_marker_shares_a_read_with_enter_is_kept_and_submits_nothing
  an_enter_one_read_behind_a_paste_s_end_marker_is_behind_it_too
  a_mebibyte_of_hostile_clipboard_written_unpaced_types_nothing_and_sends_nothing
  a_mebibyte_of_hostile_clipboard_written_unpaced_answers_no_approval
)
echo "# jobs.sh $part at $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
named)
  clone named-clone "$head"
  ( cd "$scratch/named-clone" && cargo test --locked --lib --no-fail-fast -- --exact "${LIB[@]}" ) > "$scratch/named-lib.log" 2>&1
  echo "lib: exit $?"
  grep -E '^test |^test result' "$scratch/named-lib.log" | sed 's/^/    /'
  ( cd "$scratch/named-clone" && cargo test --locked --test tui --no-fail-fast -- --exact "${TUI[@]}" ) > "$scratch/named-tui.log" 2>&1
  echo "tui: exit $?"
  grep -E '^test |^test result' "$scratch/named-tui.log" | sed 's/^/    /'
  ;;
alone)
  clone alone-clone 4a0b900b
  c="$scratch/alone-clone"
  one() { # <label> <test file> <test path> <helpers file or -> <python replacements: old=>new ...>
    local label=$1 test=$2 path=$3 helpers=$4; shift 4
    git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
    python3 -I -B - "$c" "$here/$test" "$helpers" "$@" <<'PY'
import sys
clone, test, helpers, *subs = sys.argv[1:]
path = f"{clone}/src/tui/event_loop.rs"
text = open(path, encoding="utf-8").read()
assert text.endswith("    }\n}\n")
body = (open(helpers, encoding="utf-8").read() if helpers != "-" else "") + open(test, encoding="utf-8").read()
for sub in subs:
    old, new = sub.split("=>", 1)
    assert body.count(old) == 1, (old, body.count(old))
    body = body.replace(old, new)
    print(f"#   test code: {old!r} -> {new!r}")
open(path, "w", encoding="utf-8").write(text[:-2] + body + "}\n")
PY
    ( cd "$c" && cargo test --locked --lib --no-fail-fast -- --exact "$path" ) > "$scratch/alone-$label.log" 2>&1
    local rc=$?
    local verdict why said
    verdict=$(grep -E "^test $path \.\.\. " "$scratch/alone-$label.log" | sed 's/.* \.\.\. //')
    why=$(grep -E "panicked at|^error(\[|:)" "$scratch/alone-$label.log" | head -1)
    said=$(grep -A3 'panicked at' "$scratch/alone-$label.log" | sed -n 2,4p | tr '\n' ' ' | cut -c1-240)
    echo "$label, merge 1 $(git -C "$c" rev-parse --short=8 HEAD), $test: exit $rc, ${verdict:-no verdict}; ${why:0:160} ${said}"
    git -C "$c" checkout -q -- . && git -C "$c" clean -qfd
  }
  # No PASTE-GAP here, so no quiet to wait for: a check that finds nothing ends any delivery.
  one paste join_test_paste.rs "$JP" - \
    'shell.end_delivery_at(Instant::now() + super::super::input::Decoder::PASTE_QUIET);=>shell.end_delivery();'
  # No CATALOG-REFRESH here: a catalog's due is an Instant (join_helpers_catalog.rs).
  one catalog join_test_catalog.rs "$JC" "$here/join_helpers_catalog.rs"
  echo "# alone-clone clean after: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
  ;;
mutants)
  clone mutants-clone "$head"
  python3 -I -B "$here/join_mutants.py" "$scratch/mutants-clone"
  ;;
net)
  clone net-clone "$head"
  python3 -I "$here/net.py" "$scratch/net-clone" "$logger" "$scratch/net"
  ;;
esac
echo "# jobs.sh $part done at $(date -u +%FT%TZ)"
