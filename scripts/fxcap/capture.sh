#!/usr/bin/env bash
# Captures one scenario on fx or xfx, the same way for both, so a parity row
# closes on a pair of screens rather than on a description of one.
#
# usage: scripts/fxcap/capture.sh <fx|xfx> <scenario> <rows>x<cols> [out-dir]
#
#   FX_BIN   the reference fx binary (required for `fx`), e.g. a c1db919 build
#   XFX_BIN  the xfx binary (default: target/debug/xfx in this checkout)
#
# Each run gets its own tmux server on its own socket (never the user's), its
# own empty HOME, a fake key, and a loopback fake gateway (fakegw.ts) fed the
# scenario's turns. A scenario is a file in scenarios/, one command per line:
#
#   gateway <turns.json>   turns for the fake gateway (default: none)
#   wait <seconds>
#   type <text>            literal keys
#   key <tmux key>...      named keys (Enter, Escape, C-c, Down, ...)
#   snap <label>           writes <bin>-<scenario>-<size>-<label>.{txt,ansi}
set -euo pipefail

main() {
  local which=${1:?fx or xfx} scenario=${2:?scenario} size=${3:?rowsxcols}
  local here root out
  here=$(cd "$(dirname "$0")" && pwd)
  root=$(cd "$here/../.." && pwd)
  out=${4:-$root/.prd/tui-phase4/receipts/cap}
  local rows=${size%x*} cols=${size#*x}
  local file=$here/scenarios/$scenario.cap
  [[ -f $file ]] || { echo "no scenario $file" >&2; exit 2; }
  mkdir -p "$out"

  local run port='' bin
  run=$(mktemp -d "${TMPDIR:-/tmp}/fxcap.XXXXXX")
  mkdir -p "$run/home" "$run/work"
  case $which in
    fx) bin=${FX_BIN:?set FX_BIN to the reference fx binary} ;;
    xfx) bin=${XFX_BIN:-$root/target/debug/xfx} ;;
    *) echo "fx or xfx, not $which" >&2; exit 2 ;;
  esac
  [[ -x $bin ]] || { echo "not executable: $bin" >&2; exit 2; }

  local turns='[]'
  local line
  while IFS= read -r line; do
    [[ $line == gateway\ * ]] && turns=$(cat "$here/scenarios/${line#gateway }")
  done < "$file"
  printf '%s' "$turns" > "$run/turns.json"
  # Port 0: the OS picks a free one, and the gateway says which.
  bun "$here/fakegw.ts" 0 "$run/turns.json" > "$run/gateway.log" 2>&1 &
  # Globals, not locals: the EXIT trap runs after main has returned.
  gateway=$!
  sock=$run/s
  trap 'tmux -f /dev/null -S "$sock" kill-server 2>/dev/null || true; kill "$gateway" 2>/dev/null || true' EXIT
  local tries=0
  until grep -q listening "$run/gateway.log" 2>/dev/null; do
    kill -0 "$gateway" 2>/dev/null || { cat "$run/gateway.log" >&2; exit 1; }
    (( ++tries < 100 )) || { echo "the fake gateway never listened" >&2; exit 1; }
    sleep 0.1
  done
  port=$(sed -n 's/.*listening \([0-9]*\).*/\1/p' "$run/gateway.log" | head -1)

  # Nothing a binary does under capture may reach the user's desktop: a shim
  # directory ahead of the system PATH answers `open` (browser sign-in),
  # `pbcopy`/`pbpaste` (clipboard) and `afplay` (sound) with no-ops, and fx's
  # own switch keeps it from launching a browser at all.
  mkdir -p "$run/shim"
  local tool
  for tool in open pbpaste afplay osascript; do
    printf '#!/bin/sh\nexit 0\n' > "$run/shim/$tool"
  done
  # pbcopy is fed through a pipe: drain it, keep a copy for the receipt.
  printf '#!/bin/sh\ncat > "%s/clipboard"\n' "$run" > "$run/shim/pbcopy"
  chmod +x "$run/shim/"*
  local base="HOME=$run/home TERM=xterm-256color PATH=$run/shim:/usr/bin:/bin LANG=en_US.UTF-8 AI_GATEWAY_API_KEY=fake-key-not-real FX_NO_OPEN_BROWSER=1"
  local launch
  if [[ $which == fx ]]; then
    launch="env -i $base FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 \
FX_GATEWAY_BASE_URL=http://127.0.0.1:$port \
FX_GATEWAY_CHAT_URL=http://127.0.0.1:$port/v4/ai/language-model \
FX_E2E_GATEWAY_MODELS_URL=http://127.0.0.1:$port/coding-agent/v1/models \
FX_MODEL=fake/model-a $bin"
  else
    launch="env -i $base XFX_GATEWAY_URL=http://127.0.0.1:$port/v3/ai/language-model \
XFX_MODEL=fake/model-a $bin"
  fi
  tmux -f /dev/null -S "$sock" new-session -d -s cap -x "$cols" -y "$rows" \
    "cd $run/work && exec $launch"

  while IFS= read -r line; do
    local verb=${line%% *} rest=${line#* }
    case $verb in
      ''|'#'*|gateway) ;;
      wait) sleep "$rest" ;;
      type) tmux -f /dev/null -S "$sock" send-keys -t cap -l "$rest" ;;
      key) # shellcheck disable=SC2086
        tmux -f /dev/null -S "$sock" send-keys -t cap $rest ;;
      snap)
        local name=$out/$which-$scenario-$size-$rest
        tmux -f /dev/null -S "$sock" capture-pane -p -t cap > "$name.txt"
        tmux -f /dev/null -S "$sock" capture-pane -e -p -t cap > "$name.ansi"
        echo "$name.txt"
        ;;
      *) echo "unknown verb $verb in $file" >&2; exit 2 ;;
    esac
  done < "$file"
}

main "$@"
