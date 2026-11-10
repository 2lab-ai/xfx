#!/bin/bash
# The refusal test against capture.sh without its new socket check: a copy of
# scripts/fxcap with the `if (( sock_len > 103 ))` block taken out.
# usage: mutant.sh <repo> <scratch dir> <runner-length tmpdir>
set -u
repo=$1 m=$2/mutant long=$3
rm -rf "$m" && mkdir -p "$m/scripts" && cp -R "$repo/scripts/fxcap" "$m/scripts/"
python3 -I - "$m/scripts/fxcap/capture.sh" <<'PY'
import sys
p = sys.argv[1]; s = open(p).read()
a = s.index("  if (( sock_len > 103 )); then"); b = s.index("  fi\n", a) + len("  fi\n")
open(p, "w").write(s[:a] + s[b:])
PY
echo "guard lines left: $(grep -c 'sock_len > 103' "$m/scripts/fxcap/capture.sh")"
TMPDIR=$long python3 -I "$m/scripts/fxcap/test_capture.py" Refusals.test_a_tmpdir_too_long_for_the_socket 2>&1
echo "exit=$?"
rm -rf "$m"
