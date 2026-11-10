#!/bin/bash
# The new test_capture.py against capture.sh as it was at f53606d1 (no socket
# check): RunRoots should pass on the test change alone, Refusals should fail.
# usage: mutant-test-only.sh <repo> <scratch dir> <runner-length tmpdir>
set -u
repo=$1 m=$2/mutant-t long=$3
rm -rf "$m" && mkdir -p "$m/scripts" && cp -R "$repo/scripts/fxcap" "$m/scripts/"
git -C "$repo" show f53606d1:scripts/fxcap/capture.sh > "$m/scripts/fxcap/capture.sh"
echo "guard lines in capture.sh: $(grep -c 'sock_len > 103' "$m/scripts/fxcap/capture.sh")"
TMPDIR=$long python3 -I "$m/scripts/fxcap/test_capture.py" -v \
  RunRoots.test_every_file_a_run_writes_names_its_roots_by_placeholder_at_any_tmpdir \
  Refusals.test_a_tmpdir_too_long_for_the_socket 2>&1
echo "exit=$?"
rm -rf "$m"
