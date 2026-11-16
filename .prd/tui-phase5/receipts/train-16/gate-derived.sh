#!/usr/bin/env bash
# Train 16, round 2 (new; round 1 ran the same calls inline): the files read
# from the gate's two `cargo test` logs -- unit-tests.txt (unit-tests.sh),
# unit-tests-notes.txt (the five TEST-ENV-ISOLATION tests no run has an `ok`
# for, each one's lines in both logs and its attributes at <head>) and
# run-absent.txt (run_absent.py over both logs) -- each through redacted.py.
# usage: gate-derived.sh <repo> <head> <base> <gate-log-dir> <scratch>
set -u
repo=$1 head=$2 base=$3 logs=$4 T=$5
here=$(cd "$(dirname "$0")" && pwd)
bash "$here/unit-tests.sh" "$repo" "$head" "$logs" "$T"
{ echo "# unit-tests.txt's five TEST-ENV-ISOLATION tests 'not found' in a run, read: each one's lines in the gate's two cargo test logs, and its attributes at $head"
  for spec in src/mcp/transport.rs:sentinel_runner_a_docker_cleanup_runs_with_the_servers_environment \
              src/mcp/transport.rs:sentinel_runner_a_stdio_server_sees_six_names_and_its_own_env \
              src/tui/drawn_wider.rs:xfx_counts tests/isolation.rs:a_case_marks_where_it_runs \
              tests/tui.rs:quitting_mid_stream_with_a_slow_ui_still_exits_and_still_publishes_the_log; do
    f=${spec%%:*} n=${spec#*:}
    echo "## $f $n"
    echo "  test run:       $(grep -E "^test (\S*::)?$n \.\.\. " "$logs/06-test.log" | cut -c1-150 || true)"
    echo "  test-fault run: $(grep -E "^test (\S*::)?$n \.\.\. " "$logs/07-test-fault.log" | cut -c1-150 || true)"
    git -C "$repo" show "$head:$f" | grep -n -B4 "fn $n\b" | grep -E "#\[|mod |fn " | sed 's/^/  /'
  done
  echo "## tests/tui.rs: the module that test stands in"
  git -C "$repo" show "$head:tests/tui.rs" | grep -n -B2 "^mod faults" | sed 's/^/  /'
  echo "## the tests that run the three ignored roles, in both runs"
  for n in a_server_and_its_docker_cleanup_see_only_what_they_are_given \
           a_marker_set_before_the_copy_runs_no_case_under_a_home_it_was_not_given; do
    r() { grep -E "^test (\S*::)?$n \.\.\. " "$logs/$1.log" | sed -E 's/.* \.\.\. //' | head -1; }
    echo "  $n: test $(r 06-test); test-fault $(r 07-test-fault)"
  done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/unit-tests-notes.txt" "$T=<scratch>" "$repo=<repo>"
{ echo "# run_absent.py at $head against $base, over the gate's two cargo test logs: the test functions with no result line (a cfg the run lacks, a fault-only module), and whether the base holds each"
  for l in 06-test 07-test-fault; do python3 -I -B "$here/run_absent.py" "$repo" "$head" "$base" "$logs/$l.log"; done
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/run-absent.txt" "$T=<scratch>" "$repo=<repo>"
echo "gate-derived: done"
