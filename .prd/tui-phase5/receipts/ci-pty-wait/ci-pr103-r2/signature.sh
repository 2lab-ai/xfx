#!/bin/bash
# The paced hostile paste alone, <runs> times under each recipe given, on an
# otherwise idle machine; each run's sessions read back by their words. A
# session matches CI's signature (PR #103, run 43975129001) when it was
# rejected across a planned pause that slept 50 ms or more, with a write that
# ended it under a millisecond. A planned pause of any length counts
# (`planned <n>ms pause`), so a binary with another pause is read the same.
#
#   signature.sh <out-dir> <test-binary> <runs> <name>=<prefix>...
#
# <prefix> is a command put in front of the binary, `none` for nothing, with
# `+` for spaces (e.g. `taskpolicy+-b`). Writes <out-dir>/<name>.<n>.log and
# prints one line per recipe. Prints no path.
set -u
out=$1 bin=$2 runs=$3
shift 3
mkdir -p "$out"
case_name=a_mebibyte_of_hostile_clipboard_with_pauses_under_the_quiet_types_nothing_and_sends_nothing
for recipe in "$@"; do
	name=${recipe%%=*} prefix=${recipe#*=}
	[ "$prefix" = none ] && prefix=""
	prefix=${prefix//+/ }
	for n in $(seq 1 "$runs"); do
		# shellcheck disable=SC2086 # the prefix is words
		$prefix "$bin" --exact "$case_name" --nocapture >"$out/$name.$n.log" 2>&1
	done
	python3 -I - "$out" "$name" "$runs" <<'PY'
import re, sys
out, name, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
ms = lambda v, u: float(v) * {"ms": 1, "µs": 1e-3, "ns": 1e-6, "s": 1e3}[u]
sessions = matched = green = 0
sleeps = []
for n in range(1, runs + 1):
    text = open(f"{out}/{name}.{n}.log", errors="replace").read()
    green += "test result: ok." in text
    for line in re.findall(r"(?:the paste's longest pause|a timing-indeterminate session)[^\n]*", text):
        sessions += 1
        s = re.search(r"planned \d+ms pause that slept ([\d.]+)(ms|µs|ns|s)", line)
        if s:
            sleeps.append(ms(*s.groups()))
        w = re.search(r"the write that ended it took ([\d.]+)(ms|µs|ns|s)", line)
        if "timing-indeterminate" in line and s and ms(*s.groups()) >= 50 and w and ms(*w.groups()) < 1:
            matched += 1
sleeps.sort()
spread = f"{sleeps[0]:.1f}-{sleeps[-1]:.1f} ms, median {sleeps[len(sleeps) // 2]:.1f}" if sleeps else "none reported"
print(f"{name}: {green}/{runs} runs green; {sessions} sessions, {matched} with CI's signature; "
      f"the planned sleeps the sessions reported: {spread}")
PY
done
