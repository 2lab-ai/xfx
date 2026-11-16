#!/usr/bin/env bash
# Train 16, round 5 (new): the Linux rows' Lints step, from this Mac, for
# every Linux target CI has: `cargo clippy --locked --all-targets --target
# <t> -- -D warnings`, plain and with `--features fault-injection`, through
# rustup's proxies and the toolchain CI pins (1.99.1, its Linux std
# installed). A check builds nothing to link, but build scripts run: ring's
# compiles C for the target and needs a C compiler for it, which this Mac
# has none of but Apple's clang, without Linux libc headers. So: clang as
# the target's CC (cc-rs gives it --target), and ring's own cross-building
# switch, RING_CORE_NOSTDLIBINC, with -nostdlibinc (and -ffreestanding on
# x86_64, whose intrinsics headers want stdlib.h when hosted). Those flags
# reach ring's C alone (CFLAGS_<target>); the Rust clippy checks is the
# same as CI's. Run inside one hold of the suite lock, in a clone.
# usage: suite-lock.sh linux-clippy.sh <clone> <rustup-proxy-dir>
set -u
cd "$1" || exit 2
proxies=$2
failed=0
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  u=$(printf '%s' "$t" | tr - _)
  flags="-DRING_CORE_NOSTDLIBINC -nostdlibinc"
  [ "$t" = x86_64-unknown-linux-gnu ] && flags="$flags -ffreestanding"
  for features in "" "--features fault-injection"; do
    echo "### cargo clippy --locked --all-targets --target $t $features -- -D warnings"
    # shellcheck disable=SC2086 # $features is one flag and its value, or none
    env PATH="$proxies:$PATH" RUSTUP_TOOLCHAIN=1.99.1 "CC_$u=clang" "AR_$u=ar" "CFLAGS_$u=$flags" \
      cargo clippy --locked --all-targets --target "$t" $features -- -D warnings 2>&1 \
      | grep -vE '^ +(Compiling|Checking|Fresh|Blocking) '
    rc=${PIPESTATUS[0]}
    echo "### exit $rc"
    [ "$rc" -eq 0 ] || failed=$((failed + 1))
  done
done
echo "# $failed of 4 runs failed"
exit "$failed"
