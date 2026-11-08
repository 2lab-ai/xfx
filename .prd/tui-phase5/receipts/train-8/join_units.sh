#!/usr/bin/env bash
# Train 8, join check 1 on each side alone: GRID-ROW-COST's tip (a3f90ef5)
# and main (58c92ba2, which holds STOP-NEXT-2 and T-U4-F). On a `git clone
# --no-local` of each: the join test appended unchanged (apply_join.py),
# `cargo test --no-run` -- does it build? -- and then the test with the other
# side's half taken out, run:
#   - main: the grid's two checks, which main has no record for
#     (`stale_laid_rows` read as empty, `longest_laid` as long enough);
#   - GRID-ROW-COST: STOP-NEXT's names it does not have (`start_turn()` as
#     `apply(UiEvent::TurnStarted)`, no `ui_events_taken`, the stop's end
#     spelled as its string).
# usage: join_units.sh <repo> <scratch>   (inside one hold of the suite lock)
set -u
repo=$1 scratch=$2
here=$(cd "$(dirname "$0")" && pwd)
TEST=tui::event_loop::tests::a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays
for side in grid-row-cost=a3f90ef5 main=58c92ba2; do
  name=${side%%=*} rev=${side#*=}
  clone=$scratch/alone-$name
  rm -rf "$clone"
  git clone -q --no-local "$repo" "$clone" && git -C "$clone" checkout -q "$rev"
  python3 -I -B "$here/apply_join.py" "$clone"
  ( cd "$clone" && cargo test --locked --lib --no-run ) > "$clone.build.log" 2>&1
  rc=$?
  errors=$(grep -c '^error\[' "$clone.build.log")
  echo "## $name $(git -C "$clone" rev-parse --short=8 HEAD), the join test unchanged: build exit $rc, $errors errors"
  grep -A4 '^error\[' "$clone.build.log" | grep -E '^(error|  *-->)' | sed 's/^/    /' | head -24
  python3 -I -B - "$clone/src/tui/event_loop.rs" "$name" <<'PY'
import sys
path, name = sys.argv[1], sys.argv[2]
text = open(path).read()
start = text.index("fn a_stops_held_rows_over_a_groups_long_rows_are_written_as_their_text_lays")
head, test = text[:start], text[start:]
if name == "main":
    swaps = [("band.shadow().stale_laid_rows(&shell.geometry)", "Vec::<u16>::new()", 1),
             ("band.shadow().longest_laid() > marks.len()", "true", 2)]
else:
    swaps = [("shell.start_turn();", "shell.apply(UiEvent::TurnStarted);", 1),
             ("shell.ui_events_taken(false);\n", "", 1),
             ("shell.ui_events_taken(true);\n", "", 2),
             ("super::super::bridge::TURN_CANCELLED.to_string()", '"the turn was cancelled".to_string()', 1)]
for old, new, n in swaps:
    assert test.count(old) == n, (old, test.count(old))
    test = test.replace(old, new)
open(path, "w").write(head + test)
PY
  ( cd "$clone" && cargo test --locked --lib -- --exact "$TEST" ) > "$clone.run.log" 2>&1
  rc=$?
  echo "## $name, the other side's half taken out: test exit $rc; $(grep -E '^test result' "$clone.run.log" | head -1)"
  sed -n '/^---- /,/^failures:$/p' "$clone.run.log" | grep -v '^failures:$' | sed 's/^/    /' | head -30
done
