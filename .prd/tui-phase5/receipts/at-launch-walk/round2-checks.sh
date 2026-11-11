#!/usr/bin/env bash
# AT-LAUNCH-WALK round 2's checks at the checkout's head, inside one hold of
# the suite lock (`suite-lock.sh round2-checks.sh <repo>`), TMPDIR as the
# caller set it (a directory named with every letter a-z in order): fmt,
# both clippy runs, the unit's seven modules, the parity suite, and the
# round's two cases. One line per step: its exit and seconds; then its
# summary and failing tests (every test line for the two cases).
#
# usage: suite-lock.sh round2-checks.sh <repo>
set -u
repo=$1
cd "$repo" || exit 2
here=$(cd "$(dirname "$0")" && pwd)
echo "# head $(git rev-parse --short=8 HEAD); src clean: $([ -z "$(git status --porcelain -- src)" ] && echo yes || echo no)"
step() { # name keep command...
  local name=$1 keep=$2; shift 2
  local start=$SECONDS out rc drop='^test .* ok$'
  [ "$keep" = all ] && drop='^$'
  out=$("$@" 2>&1); rc=$?
  echo "## $name: exit $rc, $((SECONDS - start)) s"
  printf '%s\n' "$out" | grep -E '^test .*(FAILED|ok)$|^test result|panicked|^# exit|^error|^warning' | grep -vE "$drop" || true
}
step fmt failures cargo fmt --check
step clippy failures cargo clippy --locked --all-targets -- -D warnings
step clippy-fault failures cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step unit failures python3 -I "$here/bounded.py" 1800 cargo test --locked --lib -- tui::file_index tui::file_picker tui::shell tui::event_loop tui::check tui::frame tui::tests
step parity failures python3 -I "$here/bounded.py" 900 cargo test --locked --test parity
step cases all python3 -I "$here/bounded.py" 900 cargo test --locked --lib -- --exact \
  tui::shell::tests::a_first_at_after_a_change_whose_walk_ended_whole_before_the_poll_lists_its_rows \
  tui::tests::the_launch_walk_covers_the_saved_directories_with_the_launch_epoch_fresh_or_resumed
