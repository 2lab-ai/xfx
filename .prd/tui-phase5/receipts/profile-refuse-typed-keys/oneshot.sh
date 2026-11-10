#!/bin/bash
# One command of fx v0.0.16 or xfx on one profile, under a fresh fake HOME:
# exit status, stdout and stderr, with the run root printed as <run>.
# usage: oneshot.sh <fx|xfx> <binary> <run root> <profile JSON> <args...>
set -u
product=$1 bin=$2 root=$3 profile=$4; shift 4
FX_SHA=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
if [ "$product" = fx ] && [ "$(shasum -a 256 "$bin" | cut -d' ' -f1)" != "$FX_SHA" ]; then echo "not the oracle" >&2; exit 2; fi
chmod -R u+rwx "$root" 2>/dev/null; rm -rf "${root:?}"; mkdir -p "$root/work" "$root/tmp"
dir=$root/home/.$product; [ "$product" = fx ] && dir=$root/home/.fx
mkdir -p "$dir"; chmod 700 "$dir"; printf '%s' "$profile" > "$dir/settings.json"; chmod 600 "$dir/settings.json"
if [ "$product" = fx ]; then extra="FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 FX_GATEWAY_BASE_URL=http://127.0.0.1:9 FX_AUTO_UPGRADE=0"
else extra="XFX_GATEWAY_URL=http://127.0.0.1:9/v1/chat/completions XFX_SOUND=0"; fi
(cd "$root/work" && env -i HOME="$root/home" TERM=xterm-256color PATH=/usr/bin:/bin TMPDIR="$root/tmp" $extra perl -e 'alarm 20; exec @ARGV or die "cannot run"' "$bin" "$@" </dev/null >"$root/out" 2>"$root/err"); code=$?
real=$(cd "$root" && pwd -P)
echo "== $product $* with $profile"
echo "exit=$code"; sed -e "s#$real#<run>#g" -e "s#$root#<run>#g" -e 's/^/  out: /' "$root/out"; sed -e "s#$real#<run>#g" -e "s#$root#<run>#g" -e 's/^/  err: /' "$root/err"
