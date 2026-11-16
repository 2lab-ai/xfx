#!/usr/bin/env bash
# Train 16 (new): every test each test binary holds, at two commits, so the
# gate's counts can be tied to the test functions the train added -- a
# `git clone --no-local` of each, `cargo test --locked --all-targets
# --no-run` then each binary's `--list --format=terse` (and the same with
# `--features fault-injection`), one `<binary> <test>` line a test, sorted;
# then what the head lists that the base does not, and the reverse, per
# feature set. Run inside one hold of the suite lock.
# usage: suite-lock.sh test_lists.sh <repo> <base> <head> <scratch>
set -u
repo=$1 base=$2 head=$3 T=$4
mkdir -p "$T"
for spec in base:$base head:$head; do
  n=${spec%%:*} c=${spec#*:}
  rm -rf "$T/list-$n"
  git clone -q --no-local "$repo" "$T/list-$n" && git -C "$T/list-$n" checkout -q --detach "$c" || exit 2
  for fs in plain fault; do
    feat=(); [ "$fs" = fault ] && feat=(--features fault-injection)
    ( cd "$T/list-$n" && cargo test --locked --all-targets ${feat[@]+"${feat[@]}"} -- --list --format=terse 2>&1 ) \
      | python3 -I -B -c '
import re, sys
binary = None
for line in sys.stdin:
    m = re.search(r"Running (?:unittests )?(\S+) \(", line)
    if m:
        binary = m.group(1)
        continue
    m = re.match(r"^(\S+): test$", line.rstrip("\n"))
    if m and binary:
        print(binary, m.group(1))' | sort > "$T/list-$n-$fs.txt"
    echo "# $n $c $fs: $(wc -l < "$T/list-$n-$fs.txt" | tr -d ' ') tests listed"
  done
done
for fs in plain fault; do
  echo "## $fs: in the head's list and not the base's: $(comm -13 "$T/list-base-$fs.txt" "$T/list-head-$fs.txt" | wc -l | tr -d ' '); in the base's and not the head's: $(comm -23 "$T/list-base-$fs.txt" "$T/list-head-$fs.txt" | wc -l | tr -d ' ')"
  comm -23 "$T/list-base-$fs.txt" "$T/list-head-$fs.txt" | sed 's/^/  gone: /'
done
