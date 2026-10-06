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
#   env <K=V>...           extra environment for the binary
#   seed <settings.json>   copied to ~/.fx/settings.json before launch (fx's own
#                          settings file; xfx does not read it)
#   work <dir>             a directory under scenarios/ copied into the
#                          workspace before launch
#   wait <seconds>
#   type <text>            literal keys
#   paste <text>           a bracketed paste (printf %b escapes)
#   key <tmux key>...      named keys (Enter, Escape, C-c, Down, ...)
#   snap <label>           writes <bin>-<scenario>-<size>-<label>.{txt,ansi}
#   snapfull <label>       the same, with the whole tmux history (.full.*)
#   title <label>          writes the pane's window title (OSC 2) to
#                          <bin>-<scenario>-<size>-<label>.title
#   keep                   keeps the pane after the program exits, so a title
#                          or a screen can still be read once it has
#   gwlog                  the gateway log and every request body it received
#   workls <label>         the workspace's files after the run
#   home <label>           every file under the run's HOME, as it stands now,
#                          to <bin>-<scenario>-<size>-<label>.files: the path
#                          relative to HOME, its size, and its bytes (locks
#                          and logs by size only), the run's own directory
#                          written as <scratch> -- what a settings write left
#   clip <label>           moves the bytes the fake pbcopy was last fed to
#                          <bin>-<scenario>-<size>-<label>.clip, or says that
#                          nothing was copied since the last clip
#   clipfail on|off        makes the fake pbcopy read its input and exit 1
#
# A line prefixed `fx: ` or `xfx: ` applies to that binary only. The two
# products name their tools and settings differently (fx's `shell` with
# `request.action: run` is xfx's `terminal` with `action: exec`,
# `FX_PERMISSION_MODE` is `XFX_PERMISSION_MODE`), so one keystroke script can
# drive both with a turns file and an environment of each's own.
set -euo pipefail

# The line, if it is for this binary, with its prefix taken off; nothing if it
# is for the other one.
for_this() {
  local which=$1 line=$2
  case $line in
    'fx: '*) [[ $which == fx ]] && printf '%s' "${line#fx: }" ;;
    'xfx: '*) [[ $which == xfx ]] && printf '%s' "${line#xfx: }" ;;
    *) printf '%s' "$line" ;;
  esac
  return 0
}

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

  local turns='[]' extra_env=''
  local raw line
  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    [[ $line == gateway\ * ]] && turns=$(cat "$here/scenarios/${line#gateway }")
    [[ $line == env\ * ]] && extra_env="$extra_env ${line#env }"
    [[ $line == seed\ * ]] && { mkdir -p "$run/home/.fx"; cp "$here/scenarios/${line#seed }" "$run/home/.fx/settings.json"; }
    [[ $line == work\ * ]] && cp -R "$here/scenarios/${line#work }/." "$run/work/"
  done < "$file"
  printf '%s' "$turns" > "$run/turns.json"
  # Port 0: the OS picks a free one, and the gateway says which.
  mkdir -p "$run/bodies"
  bun "$here/fakegw.ts" 0 "$run/turns.json" "$run/bodies" > "$run/gateway.log" 2>&1 &
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
  # pbcopy is fed through a pipe: drain it, keep a copy for the receipt -- or,
  # while `clipfail on`, drain it and fail.
  printf '#!/bin/sh\nif [ -f "%s/clipboard-fail" ]; then cat >/dev/null; exit 1; fi\ncat > "%s/clipboard"\n' \
    "$run" "$run" > "$run/shim/pbcopy"
  chmod +x "$run/shim/"*
  # macOS assesses an executable the first time it runs, which costs a second
  # or more and would land inside a scenario's `wait`: run pbcopy once now.
  : | "$run/shim/pbcopy"
  rm -f "$run/clipboard"
  local base="HOME=$run/home TERM=xterm-256color PATH=$run/shim:/usr/bin:/bin LANG=en_US.UTF-8 AI_GATEWAY_API_KEY=fake-key-not-real FX_NO_OPEN_BROWSER=1"
  local launch
  if [[ $which == fx ]]; then
    launch="env -i $base FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 \
FX_GATEWAY_BASE_URL=http://127.0.0.1:$port \
FX_GATEWAY_CHAT_URL=http://127.0.0.1:$port/v4/ai/language-model \
FX_E2E_GATEWAY_MODELS_URL=http://127.0.0.1:$port/coding-agent/v1/models \
FX_MODEL=fake/model-a $extra_env $bin"
  else
    launch="env -i $base XFX_GATEWAY_URL=http://127.0.0.1:$port/v3/ai/language-model \
XFX_MODEL=fake/model-a $extra_env $bin"
  fi
  tmux -f /dev/null -S "$sock" new-session -d -s cap -x "$cols" -y "$rows" \
    "cd $run/work && exec $launch"

  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    local verb=${line%% *} rest=${line#* }
    case $verb in
      ''|'#'*|gateway|env|seed|work) ;;
      wait) sleep "$rest" ;;
      type) tmux -f /dev/null -S "$sock" send-keys -t cap -l "$rest" ;;
      paste) tmux -f /dev/null -S "$sock" set-buffer -b capbuf -- "$(printf '%b' "$rest")"; tmux -f /dev/null -S "$sock" paste-buffer -p -b capbuf -t cap ;;
      key) # shellcheck disable=SC2086
        tmux -f /dev/null -S "$sock" send-keys -t cap $rest ;;
      snap)
        local name=$out/$which-$scenario-$size-$rest
        tmux -f /dev/null -S "$sock" capture-pane -p -t cap > "$name.txt"
        tmux -f /dev/null -S "$sock" capture-pane -e -p -t cap > "$name.ansi"
        echo "$name.txt"
        ;;
      snapfull)
        local name=$out/$which-$scenario-$size-$rest.full
        tmux -f /dev/null -S "$sock" capture-pane -p -S - -t cap > "$name.txt"
        tmux -f /dev/null -S "$sock" capture-pane -e -p -S - -t cap > "$name.ansi"
        echo "$name.txt"
        ;;
      gwlog) cp "$run/gateway.log" "$out/$which-$scenario-$size-gateway.log"; rm -rf "$out/$which-$scenario-$size-bodies"; cp -R "$run/bodies" "$out/$which-$scenario-$size-bodies" ;;
      # One name per line rather than a word-split `$(find)`, so a file
      # with a space in its name is listed whole.
      workls)
        (cd "$run/work" && find . -type f | sort | while IFS= read -r f; do
          echo "== $f"; head -c 2000 "$f"; echo
        done) > "$out/$which-$scenario-$size-work-$rest.txt"
        ;;
      home)
        local name=$out/$which-$scenario-$size-$rest.files
        (
          cd "$run/home"
          find . -type f ! -path '*/file-index/*' | LC_ALL=C sort | while IFS= read -r f; do
            echo "=== ~/${f#./} ($(wc -c < "$f" | tr -d ' ') bytes)"
            case $f in
              *.lock|*.log|*.sqlite*) ;;
              *) head -c 4000 "$f" | sed "s#$run#<scratch>#g"; echo ;;
            esac
          done
        ) > "$name"
        echo "$name"
        ;;
      title)
        local name=$out/$which-$scenario-$size-$rest
        tmux -f /dev/null -S "$sock" display-message -p -t cap '#{pane_title}' > "$name.title"
        echo "$name.title"
        ;;
      keep) tmux -f /dev/null -S "$sock" set-option -w -t cap remain-on-exit on > /dev/null ;;
      clip)
        local name=$out/$which-$scenario-$size-$rest
        if [[ -f $run/clipboard ]]; then
          mv "$run/clipboard" "$name.clip"
          echo "$name.clip"
        else
          echo "clip $rest: nothing was copied"
        fi
        ;;
      clipfail)
        case $rest in
          on) : > "$run/clipboard-fail" ;;
          off) rm -f "$run/clipboard-fail" ;;
          *) echo "clipfail on|off, not $rest" >&2; exit 2 ;;
        esac
        ;;
      *) echo "unknown verb $verb in $file" >&2; exit 2 ;;
    esac
  done < "$file"
}

main "$@"
