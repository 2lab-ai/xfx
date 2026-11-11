#!/usr/bin/env bash
# AT-LAUNCH-WALK's checks at the checkout's head, inside one hold of the
# suite lock (`suite-lock.sh checks.sh <repo>`), TMPDIR as the caller set it
# (a directory named with every letter a-z in order): the unit's modules,
# the parity suite, and the 13 cases of `red-unit.txt`. One line per step:
# its exit and seconds; then its summary and failing tests (every test
# line for the 13 cases).
#
# usage: suite-lock.sh checks.sh <repo>
set -u
repo=$1
cd "$repo" || exit 2
here=$(cd "$(dirname "$0")" && pwd)
echo "# head $(git rev-parse --short=8 HEAD); src clean: $([ -z "$(git status --porcelain -- src)" ] && echo yes || echo no)"
step() { # name keep-ok command...
  local name=$1 keep=$2; shift 2
  local start=$SECONDS out rc drop='^test .* ok$'
  [ "$keep" = all ] && drop='^$'
  out=$("$@" 2>&1); rc=$?
  echo "## $name: exit $rc, $((SECONDS - start)) s"
  printf '%s\n' "$out" | grep -E '^test .*(FAILED|ok)$|^test result|panicked|^# exit' | grep -vE "$drop" || true
}
step unit failures python3 -I "$here/bounded.py" 1800 cargo test --locked --lib -- tui::file_index tui::file_picker tui::shell tui::event_loop tui::check tui::frame tui::tests
step parity failures python3 -I "$here/bounded.py" 900 cargo test --locked --test parity
step cases all python3 -I "$here/bounded.py" 900 cargo test --locked --lib -- --exact \
  tui::file_index::tests::the_launch_walks_a_root_once_and_an_empty_root_never \
  tui::file_index::tests::the_launch_walk_keeps_the_launch_epoch_through_a_change \
  tui::shell::tests::a_fresh_sessions_first_at_after_the_launch_walk_is_whole_shows_its_rows_at_once \
  tui::shell::tests::a_change_before_the_launch_walk_is_whole_says_files_unavailable_until_the_new_scopes_walk_is \
  tui::shell::tests::a_change_after_the_launch_walk_failed_lists_the_new_walks_prefix \
  tui::shell::tests::a_first_at_after_the_launch_walk_ended_whole_but_before_the_poll_refreshes \
  tui::shell::tests::a_first_at_after_the_launch_walk_failed_but_before_the_poll_says_indexing \
  tui::shell::tests::a_first_at_after_a_change_whose_walk_failed_before_the_poll_says_files_unavailable \
  tui::shell::tests::the_first_at_while_the_old_roots_walk_runs_skips_its_refresh \
  tui::shell::tests::the_first_at_after_the_old_roots_walk_was_adopted_refreshes \
  tui::shell::tests::the_first_at_after_the_old_roots_walk_failed_skips_its_refresh \
  tui::shell::tests::tab_in_the_interim_retries_and_still_says_files_unavailable \
  tui::tests::the_launch_starts_the_index_walk_fresh_or_resumed
