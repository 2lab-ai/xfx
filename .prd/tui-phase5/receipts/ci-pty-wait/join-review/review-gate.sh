#!/bin/bash
# fmt, clippy both feature sets, the lib tests plain and fault-injection
# (the touched cases are lib tests), with Homebrew's cargo; then the whole
# tui test binary under rustup's own multiplexer (libexec/bin) at 1.99.1.
set -u
cd "$1" || exit 2
out=$2
: > "$out"
step() { local n=$1; shift; "$@" > "$out.$n.log" 2>&1; echo "$n: exit $?" >> "$out"; grep -E "^test result:" "$out.$n.log" | sed 's/^/    /' >> "$out"; grep -E "^test .* FAILED$" "$out.$n.log" | sed 's/^/    /' >> "$out"; }
echo "# head $(git rev-parse --short HEAD); $(cargo --version); $(rustc --version)" >> "$out"
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step lib cargo test --locked --lib
step lib-fault cargo test --locked --lib --features fault-injection
export PATH=/opt/homebrew/opt/rustup/libexec/bin:$PATH RUSTUP_TOOLCHAIN=1.99.1
echo "# rustup multiplexer: $(command -v cargo) -> $(readlink "$(command -v cargo)"); $(cargo --version); $(rustc --version)" >> "$out"
CARGO_TARGET_DIR=target/rustup step tui-rustup cargo test --locked --test tui
