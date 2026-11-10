#!/bin/bash
# Why `fx p5-panels-badsettings` failed in 0 s. The scenario as it was at
# 1b15dad9 (PR #90's head), run twice at the runner-length TMPDIR:
#   1. by capture.sh, as the sweep runs it;
#   2. fx alone in a tmux pane of its own, the scenario's environment, its
#      standard error kept (capture.sh's pane shows none of it).
# The oracle's sha256 is read before and after. Prints no path.
# usage: badsettings-repro.sh <repo> <fx v0.0.16> <runner-length tmpdir> <scratch dir>
set -u
repo=$1 fx=$2 long=$3 s=$4/bs
rm -rf "$s" && mkdir -p "$s/scen" "$s/run/home/.fx" "$s/run/work" "$s/run/fxtmp"
for f in p5-panels-badsettings.cap p5-panels-badsettings.json; do
  git -C "$repo" show "1b15dad9:scripts/fxcap/scenarios/$f" > "$s/scen/$f"
done
echo "fx sha256 before: $(shasum -a 256 "$fx" | cut -d' ' -f1)"
echo "== 1. capture.sh fx p5-panels-badsettings 40x120"
( cd "$repo" && env -u TMUX -u TMUX_PANE TMPDIR=$long FX_BIN=$fx FX_AUTO_UPGRADE=0 FXCAP_SCENARIOS=$s/scen \
    bash scripts/fxcap/capture.sh fx p5-panels-badsettings 40x120 "$s/out" 2>&1 ) | grep -E '^(roots|up|guards):' | sed -E 's#/[^ ]+/fx-v0\.0\.16/bin/fx#<fx>#'
echo "exit=${PIPESTATUS[0]}"
echo "== 2. fx alone: the seeded profile, AI_GATEWAY_API_KEY empty, FX_AUTO_UPGRADE=0"
r=$s/run
cp "$s/scen/p5-panels-badsettings.json" "$r/home/.fx/settings.json"
chmod 700 "$r/home/.fx"; chmod 600 "$r/home/.fx/settings.json"
for cue in success error bloom press release toggle; do mkdir -p "$r/fxtmp/fx-$cue.m4a"; done
mkdir -p "$r/fxtmp/fx-click.caf"
# Under the TMPDIR, not the scratch dir: a scratch dir's path can be past
# the socket's 103 bytes.
sock=${long%/}/bs-repro.sock
tmux -f /dev/null -S "$sock" new-session -d -x 120 -y 40 -s d \
  "cd $r/work && env -i HOME=$r/home TERM=xterm-256color PATH=/usr/bin:/bin LANG=en_US.UTF-8 FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 TMPDIR=$r/fxtmp FX_GATEWAY_BASE_URL=http://127.0.0.1:9 AI_GATEWAY_API_KEY= FX_AUTO_UPGRADE=0 $fx 2> $r/stderr; echo \$? > $r/exit"
tries=0
until [ -s "$r/exit" ] || [ $tries -ge 100 ]; do sleep 0.1; tries=$((tries + 1)); done
tmux -f /dev/null -S "$sock" kill-server 2>/dev/null
echo "fx exit status: $(cat "$r/exit" 2>/dev/null)"
echo "fx standard error: $(cat "$r/stderr")"
echo "the seeded settings file, whole: $(cat "$r/home/.fx/settings.json")"
echo "fx sha256 after:  $(shasum -a 256 "$fx" | cut -d' ' -f1)"
