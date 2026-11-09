#!/usr/bin/env bash
# Train 11, round 3: the young-boot-clock runs, each in a `git clone
# --no-local` of its own (never the worktree):
#   red <commit> <secs>  -- young_clock.py's harness, then `cargo test --lib`
#                          with XFX_YOUNG_BOOT_SECS=<secs>; every failing test
#                          and its panic's message. One test is skipped:
#                          `the_system_catalog_clock_is_the_one_that_counts_sleep`
#                          compares the system catalog clock with the
#                          machine's awake clock, which the harness moves
#                          apart by construction.
#   sub <commit>         -- young_clock.py --sub-test-only, then `cargo check
#                          --locked --lib` with and without fault-injection: a
#                          non-test build that subtracted from a CatalogTime
#                          would not compile.
# usage: suite-lock.sh young-clock.sh <part> <repo> <scratch> <commit> [<secs>]
set -u
part=$1 repo=$2 scratch=$3 commit=$4 secs=${5:-150}
here=$(cd "$(dirname "$0")" && pwd)
c="$scratch/young-$part-$secs-$commit"
rm -rf "$c" && git clone -q --no-local "$repo" "$c" && git -C "$c" checkout -q --detach "$commit" || exit 2
echo "# young-clock.sh $part at $(git -C "$c" rev-parse --short=8 HEAD), $(date -u +%FT%TZ), load $(sysctl -n vm.loadavg | awk '{print $2}')"
case "$part" in
red)
  python3 -I -B "$here/young_clock.py" "$c" || exit 2
  ( cd "$c" && XFX_YOUNG_BOOT_SECS="$secs" cargo test --locked --lib --no-fail-fast -- \
      --skip provider::catalog_clock::tests::the_system_catalog_clock_is_the_one_that_counts_sleep ) > "$c.log" 2>&1
  echo "cargo test --lib, XFX_YOUNG_BOOT_SECS=$secs: exit $?"
  grep -E '^test result' "$c.log" | sed 's/^/  /'
  for t in $(grep -E '^test .* FAILED$' "$c.log" | sed 's/^test //; s/ \.\.\. FAILED$//'); do
    echo "  FAILED $t"
    # The test's own block: from its `---- <name> stdout ----` line to the
    # next block's, its panic line, its message and an assertion's left and right.
    awk -v t="---- $t stdout ----" '$0 == t {on = 1; next} on && (/^---- / || /^failures:/) {exit}
      on && (/panicked at/ || /^  left:/ || /^ right:/ || /^[a-z]/) && !/^note:/ && !/^stack/ {print "      " $0}' "$c.log"
  done
  ;;
sub)
  python3 -I -B "$here/young_clock.py" --sub-test-only "$c" || exit 2
  for f in "" "--features fault-injection"; do
    ( cd "$c" && cargo check --locked --lib $f ) > "$c.check.log" 2>&1
    echo "cargo check --locked --lib $f: exit $?; $(grep -E '^error' "$c.check.log" | head -3 | tr '\n' ' ')$(tail -1 "$c.check.log")"
  done
  ;;
esac
git -C "$c" checkout -q -- . && echo "# clone restored: $([ -z "$(git -C "$c" status --porcelain)" ] && echo yes || echo no)"
