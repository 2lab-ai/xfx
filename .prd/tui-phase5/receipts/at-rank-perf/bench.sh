#!/bin/bash
# usage: bench.sh <test binary> <label> [tree]   -- one run of the @ benchmark, alone
# (--test-threads=1, exact filter), load printed before and after. With no
# tree, the test's own inventory.
bin=$1 label=$2 tree=${3:-}
echo "# $label"
if [ -n "$tree" ]; then
  export XFX_AT_BENCH_TREE="$tree"; echo "# XFX_AT_BENCH_TREE=<scratch>/$(basename "$tree")"
else
  unset XFX_AT_BENCH_TREE; echo "# XFX_AT_BENCH_TREE unset (fx's inventory, in memory)"
fi
echo "# before: $(uptime | sed 's/.*load/load/') ($(sysctl -n hw.ncpu) cores)"
"$bin" --include-ignored --exact --nocapture --test-threads=1 \
  tui::file_index::tests::a_keystroke_over_a_hundred_thousand_entries_costs_less_than_a_frame 2>&1 |
  grep -v '^$' | sed 's/ ([0-9]*) panicked/ panicked/'
echo "# after: $(uptime | sed 's/.*load/load/')"
