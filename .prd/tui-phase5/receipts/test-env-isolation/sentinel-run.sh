#!/usr/bin/env bash
#
# Runs a copy of xfx's unit-test binary in an environment built from nothing
# but sentinels, so "the real process environment" a case can reach is this
# one and never the developer's.
#
#   TMPDIR=<short> sentinel-run.sh <test binary> [libtest args...]
#
# The sentinel home holds a `.gitconfig` that configures a hook and a
# `url.<base>.insteadOf` whose base carries a token -- the shape a real
# credential takes in a global git configuration -- and the variables are
# xfx's own with sentinel values, beside `CARGO_PKG_NAME`, which
# cargo sets in every test process it runs. A case that reads the process
# (on the base) sees these; a case under the guard (the head) is refused, or
# hands a child nothing of them. Bounded: the binary is killed after
# SENTINEL_SECONDS (default 600); macOS refuses `ulimit -v`, and no case here
# reads an unbounded source.
set -euo pipefail

main() {
  local bin=$1
  shift
  : "${TMPDIR:?a short TMPDIR}"
  local root="${TMPDIR%/}/tei-sentinel"
  rm -rf "$root"
  mkdir -p "$root/home/.config/git" "$root/home/.xfx"
  chmod 700 "$root/home" "$root/home/.xfx"
  printf '[hook "sentinel"]\n\tcommand = true\n' > "$root/home/.gitconfig"
  printf '[url "https://x-access-token:xfx-sentinel-git-token@sentinel.invalid/"]\n\tinsteadOf = https://github.com/\n' \
    >> "$root/home/.gitconfig"
  printf '[hook "sentinel-xdg"]\n\tcommand = true\n' > "$root/home/.config/git/config"
  printf '{"provider":"gateway","model":"xfx-sentinel-profile-model"}\n' \
    > "$root/home/.xfx/settings.json"
  local status=0
  env -i \
    PATH="$PATH" \
    TMPDIR="$TMPDIR" \
    HOME="$root/home" \
    XDG_CONFIG_HOME="$root/home/.config" \
    XFX_MODEL=xfx-sentinel-model \
    XFX_GATEWAY_URL=https://sentinel.invalid/v1 \
    XFX_TUI=0 \
    CARGO_PKG_NAME=xfx \
    XFX_E2E_USAGE_CLOCK_FILE="$root/clock" \
    AI_GATEWAY_API_KEY=xfx-sentinel-gateway-key \
    VERCEL_OIDC_TOKEN=xfx-sentinel-oidc-token \
    timeout -k 10 "${SENTINEL_SECONDS:-600}" "$bin" "$@" || status=$?
  rm -rf "$root"
  echo "sentinel-run: exit $status"
  return 0
}

main "$@"
