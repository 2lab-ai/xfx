#!/usr/bin/env bash
# Fetches what the parity sweep needs in CI that the runner does not have, each
# by sha256, and fails -- never warns -- on any mismatch:
#
#   fx   the reference binary: the public release vercel-labs/fx v0.0.16,
#        asset fx-macos-aarch64.tar.gz. Both the tarball and the executable in
#        it are checked, the tarball before anything reads it. The executable's
#        sha256 is the ledger's `v0.0.16` [[pin]] (parity-ledger.toml), which
#        sweep.sh checks again before and after every run.
#   bun  the TypeScript runtime fakegw.ts and mcphttp.ts run on, the release
#        the local captures were taken with (1.3.14), not whatever a tap holds
#        that day.
#
# usage: scripts/fxcap/ci-fetch.sh <dir>     (<dir> must not exist yet)
# writes <dir>/bin/fx and <dir>/bin/bun, both read-only.
set -euo pipefail

FX_URL=https://github.com/vercel-labs/fx/releases/download/v0.0.16/fx-macos-aarch64.tar.gz
FX_TARBALL_SHA256=2158a8d42e77457a774400d225d9802bb6c0e60a0bb0674a1116bc31252490d6
FX_SHA256=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
BUN_URL=https://github.com/oven-sh/bun/releases/download/bun-v1.3.14/bun-darwin-aarch64.zip
BUN_ZIP_SHA256=d8b96221828ad6f97ac7ac0ab7e95872341af763001e8803e8267652c2652620

main() {
  local dir=${1:?usage: ci-fetch.sh <dir>}
  [[ ! -e $dir ]] || { echo "ci-fetch: $dir exists; it must be made here, empty" >&2; exit 2; }
  mkdir -p "$dir/dl" "$dir/fx" "$dir/bun" "$dir/bin"

  fetch "$FX_URL" "$dir/dl/fx.tar.gz"
  check "$dir/dl/fx.tar.gz" "$FX_TARBALL_SHA256" "the fx v0.0.16 tarball"
  tar -xzf "$dir/dl/fx.tar.gz" -C "$dir/fx" fx
  [[ -f $dir/fx/fx && ! -L $dir/fx/fx ]] || { echo "ci-fetch: the tarball holds no regular file fx" >&2; exit 1; }
  check "$dir/fx/fx" "$FX_SHA256" "the fx v0.0.16 executable"

  fetch "$BUN_URL" "$dir/dl/bun.zip"
  check "$dir/dl/bun.zip" "$BUN_ZIP_SHA256" "the bun 1.3.14 archive"
  unzip -q -j "$dir/dl/bun.zip" bun-darwin-aarch64/bun -d "$dir/bun"
  [[ -f $dir/bun/bun && ! -L $dir/bun/bun ]] || { echo "ci-fetch: the archive holds no regular file bun" >&2; exit 1; }

  install -m 0555 "$dir/fx/fx" "$dir/bin/fx"
  install -m 0555 "$dir/bun/bun" "$dir/bin/bun"
  # The copy is what runs: checked again, so nothing between the check and
  # the run is trusted.
  check "$dir/bin/fx" "$FX_SHA256" "the installed fx"
  chmod a-w "$dir/bin"
  rm -rf "$dir/dl"
  echo "ci-fetch: fx $FX_SHA256 and bun $("$dir/bin/bun" --version) in $dir/bin"
}

fetch() {
  curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    --retry 3 --retry-delay 5 --output "$2" "$1"
}

check() {
  local file=$1 want=$2 what=$3 got
  got=$(shasum -a 256 "$file" | cut -d' ' -f1)
  if [[ $got != "$want" ]]; then
    echo "ci-fetch: $what has sha256 $got, not the pinned $want: refusing it" >&2
    exit 1
  fi
  echo "ci-fetch: $what sha256 $got matches the pin"
}

main "$@"
