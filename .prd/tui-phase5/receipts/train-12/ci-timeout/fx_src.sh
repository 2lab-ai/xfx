#!/bin/bash
# Every fx line this receipt cites, at the v0.0.16 pin 481104fd.
# usage: fx_src.sh <fx source checkout>
set -u
g=$1
show() { echo "== $1:$2-$3@481104fd"; git -C "$g" show "481104fd:$1" | sed -n "$2,$3p" | nl -ba -v "$2"; }
show src/core/auth/credentials.zig 28 31
show src/core/app/app_lifecycle.zig 688 701
show src/core/session/session_commands.zig 280 296
echo "== 8a964aca (the commit p5-panels-badsettings cites), in the pin's history?"
git -C "$g" merge-base --is-ancestor 8a964aca 481104fd && echo "yes: 8a964aca is an ancestor of 481104fd"
git -C "$g" log -1 --format='%h %ad %s' --date=short 8a964aca
git -C "$g" show -s --format=%b 8a964aca | grep -i "settings can't be read"
