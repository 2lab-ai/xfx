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
#   seed <settings.json>   copied to the binary's own profile before launch,
#                          owner-only: ~/.fx/settings.json for fx,
#                          ~/.xfx/settings.json for xfx
#   models <catalog.json>  what the fake gateway answers for
#                          GET /coding-agent/v1/models (default: one model)
#   credits <credits.json> what it answers for GET /coding-agent/v1/credits
#                          (default: 404 `not found`)
#   titled <title.json>    what it answers a session-title request with
#                          ({chunks, delayMs, status, body}; default: a
#                          stream with no text, so no title is generated)
#   nomodel                launch without FX_MODEL / XFX_MODEL, so the model
#                          comes from the profile and no process override
#                          shadows what a command saves there
#   work <dir>             a directory under scenarios/ copied into the
#                          workspace before launch
#   githead <branch>       makes the workspace a checkout on <branch>: a
#                          `.git/HEAD` naming it, and nothing else
#   dir <name> [<from>]    a directory beside the workspace, <run>/<name>, made
#                          before launch -- empty, or a copy of scenarios/<from>
#                          -- so `/workspace add ../<name>` names a directory
#                          inside the run and nowhere else (a plain name: no
#                          `/`, not `.` or `..`)
#   sparsepng <name> <bytes>
#                          a file of <bytes> in the workspace that begins with
#                          the PNG signature and is a hole after it -- an image
#                          over a size limit, without one in the repository
#   pnghdr <name> <width> <height>
#                          a PNG signature and an IHDR declaring <width> x
#                          <height> and nothing else, in the workspace -- an
#                          image over a pixel limit, as fx's own tests make one
#   wait <seconds>
#   type <text>            literal keys
#   paste <text>           a bracketed paste (printf %b escapes)
#   pastex <bytes>         a bracketed paste of <bytes> `x`s, through a file in
#                          the run directory -- a paste too large for a
#                          command line, such as one past the prompt's budget
#   key <tmux key>...      named keys (Enter, Escape, C-c, Down, ...)
#   snap <label>           writes <bin>-<scenario>-<size>-<label>.{txt,ansi}
#   snapfull <label>       the same, with the whole tmux history (.full.*)
#   title <label>          writes the pane's window title (OSC 2) to
#                          <bin>-<scenario>-<size>-<label>.title
#   cursor <label>         writes where the terminal's cursor is, `x,y` from
#                          the pane's top left, to
#                          <bin>-<scenario>-<size>-<label>.cursor
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
#   clipfail on|off        makes the fake pbcopy read its input and exit 1,
#                          and the fake osascript exit 1
#   clipfile <label>       moves the paths the fake osascript was handed as its
#                          last argument -- a file copy, `/trace`'s -- to
#                          <bin>-<scenario>-<size>-<label>.clipfile, the run's
#                          directory written as <scratch>, or says none was
#   reports <label>        copies every `/trace` report the binary wrote into
#                          its own TMPDIR to
#                          <bin>-<scenario>-<size>-<label>-<n>.md, oldest
#                          first, the run's directory written as <scratch>,
#                          and prints the mode each was written with
#   clipimage <file>|off   makes the fake osascript answer a clipboard-image
#                          read (`-e` with `POSIX file "<path>"` and
#                          `class PNGf`) by copying <file> (under scenarios/)
#                          to <path>, as the real clipboard would write its
#                          image; `off` (the default) writes nothing and exits
#                          0. Such a read is never kept for `clipfile`.
#   relaunch [<args>...]   starts the binary again in the same pane, with the
#                          same HOME and environment and these arguments --
#                          after `keep` and an exit, so a second launch sees
#                          what the first one saved (`fx: relaunch
#                          --resume-last`, `xfx: relaunch --resume last`)
#   bells <label>          writes how many terminal bells the pane has rung so
#                          far (tmux's alert-bell hook, one line per bell; an
#                          OSC's BEL terminator is not one) and the clients
#                          attached to this server -- none, so no bell reaches
#                          any real terminal -- to
#                          <bin>-<scenario>-<size>-<label>.bells
#   wire <label>           copies every byte the binary has written to its
#                          terminal so far, exactly as written (tmux's
#                          pipe-pane, attached in the command that creates the
#                          pane), to <bin>-<scenario>-<size>-<label>.wire --
#                          what a screen capture cannot show: which writes a
#                          synchronized update brackets, and where a scroll
#                          lands among them
#
# fx plays its sound cues through `/usr/bin/afplay`, by absolute path, which
# the PATH shims below do not reach, and FX_SOUND=0 only sets the level at
# launch: a scenario that types `/sound on` (or moves the /settings sound row)
# turns it back on. So every fx run gets a TMPDIR of its own in which each
# chime's file name is already a directory: fx cannot write the chime there,
# rings the terminal bell instead (vercel-labs/fx@c1db9191
# src/core/notifications/sound.zig:40-62, :176-181), and the bell reaches no
# real terminal (no client is attached). The run checks afterwards that every
# one of those names is still a directory, and fails loudly if not.
# (2026-10-06 23:09 and 23:14: a research copy of this script, without the
# guard, let fx write /tmp/fx-toggle.m4a and /tmp/fx-bloom.m4a and play them.)
#
# Each binary gets a TMPDIR inside the run (fx `$run/fxtmp`, xfx
# `$run/xfxtmp`), so what either writes there -- `/trace`'s report -- stays in
# the run and is read out with `reports`.
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
  # FXCAP_SCENARIOS points at another scenario directory (research keeps its
  # scenarios out of the repo); the default is the one beside this script.
  local scenarios=${FXCAP_SCENARIOS:-$here/scenarios}
  local file=$scenarios/$scenario.cap
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

  local turns='[]' extra_env='' models='' credits='' titled='' nomodel=''
  local raw line
  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    [[ $line == gateway\ * ]] && turns=$(cat "$scenarios/${line#gateway }")
    [[ $line == env\ * ]] && extra_env="$extra_env ${line#env }"
    # Owner-only, as each product keeps its own profile: xfx refuses to read
    # a profile directory anyone else can reach.
    [[ $line == seed\ * ]] && { mkdir -p -m 700 "$run/home/.$which"; cp "$scenarios/${line#seed }" "$run/home/.$which/settings.json"; chmod 600 "$run/home/.$which/settings.json"; }
    [[ $line == models\ * ]] && models=$scenarios/${line#models }
    [[ $line == credits\ * ]] && credits=$scenarios/${line#credits }
    [[ $line == titled\ * ]] && titled=$scenarios/${line#titled }
    [[ $line == nomodel ]] && nomodel=1
    [[ $line == work\ * ]] && cp -R "$scenarios/${line#work }/." "$run/work/"
    [[ $line == githead\ * ]] && { mkdir -p "$run/work/.git"; printf 'ref: refs/heads/%s\n' "${line#githead }" > "$run/work/.git/HEAD"; }
    if [[ $line == sparsepng\ * ]]; then
      # Not `name`/`size`: `size` is this run's terminal size.
      local png=${line#sparsepng }
      dd if=/dev/zero of="$run/work/${png%% *}" bs=1 count=0 seek="${png#* }" 2>/dev/null
      printf '\211PNG\r\n\032\n' | dd of="$run/work/${png%% *}" conv=notrunc 2>/dev/null
    fi
    if [[ $line == pnghdr\ * ]]; then
      local header name width height value bits
      read -r name width height <<< "${line#pnghdr }"
      header='\211PNG\r\n\032\n\000\000\000\015IHDR'
      for value in "$width" "$height"; do
        for bits in 24 16 8 0; do
          header+=$(printf '\\%03o' $(( (value >> bits) & 255 )))
        done
      done
      # shellcheck disable=SC2059 # the format is the bytes, built above
      printf "$header" > "$run/work/$name"
    fi
    if [[ $line == dir\ * ]]; then
      local name from
      read -r name from <<< "${line#dir }"
      case $name in ''|.|..|*/*) echo "dir takes a plain name, not $name" >&2; exit 2 ;; esac
      mkdir -p "$run/$name"
      [[ -n $from ]] && cp -R "$scenarios/$from/." "$run/$name/"
    fi
  done < "$file"
  printf '%s' "$turns" > "$run/turns.json"
  # Port 0: the OS picks a free one, and the gateway says which.
  mkdir -p "$run/bodies"
  FAKEGW_MODELS=$models FAKEGW_CREDITS=$credits FAKEGW_TITLE=$titled bun "$here/fakegw.ts" 0 "$run/turns.json" "$run/bodies" > "$run/gateway.log" 2>&1 &
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
  for tool in open pbpaste afplay; do
    printf '#!/bin/sh\nexit 0\n' > "$run/shim/$tool"
  done
  # osascript writes nothing either; it keeps the path a file copy hands it as
  # its last argument (`/trace`), for `clipfile`, and exits 0 -- or, while
  # `clipfail on`, exits 1 and keeps nothing. A clipboard-image read (`/paste`,
  # Ctrl-V) is never kept: it writes the `clipimage` file, when one is set, to
  # the path its script names, and nothing otherwise.
  cat > "$run/shim/osascript" <<SHIM
#!/bin/sh
if [ -f "$run/clipboard-fail" ]; then exit 1; fi
case \$2 in
  *'POSIX file "'*'class PNGf'*)
    if [ "\$1" = "-e" ] && [ -f "$run/clipimage" ]; then
      out=\${2#*POSIX file \\"}
      out=\${out%%\\"*}
      cat "$run/clipimage" > "\$out"
    fi
    exit 0
    ;;
esac
for arg in "\$@"; do last=\$arg; done
printf '%s\\n' "\$last" >> "$run/clipboard-file"
exit 0
SHIM
  # pbcopy is fed through a pipe: drain it, keep a copy for the receipt -- or,
  # while `clipfail on`, drain it and fail.
  printf '#!/bin/sh\nif [ -f "%s/clipboard-fail" ]; then cat >/dev/null; exit 1; fi\ncat > "%s/clipboard"\n' \
    "$run" "$run" > "$run/shim/pbcopy"
  chmod +x "$run/shim/"*
  # macOS assesses an executable the first time it runs, which costs a second
  # or more and would land inside a scenario's `wait`: run pbcopy and osascript
  # once now.
  : | "$run/shim/pbcopy"
  "$run/shim/osascript" warm-up
  rm -f "$run/clipboard" "$run/clipboard-file"
  local base="HOME=$run/home TERM=xterm-256color PATH=$run/shim:/usr/bin:/bin LANG=en_US.UTF-8 AI_GATEWAY_API_KEY=fake-key-not-real FX_NO_OPEN_BROWSER=1"
  local launch fx_model=FX_MODEL=fake/model-a xfx_model=XFX_MODEL=fake/model-a
  [[ -n $nomodel ]] && { fx_model=''; xfx_model=''; }
  if [[ $which == fx ]]; then
    mkdir -p "$run/fxtmp"
    local cue
    for cue in success error bloom press release toggle; do mkdir -p "$run/fxtmp/fx-$cue.m4a"; done
    mkdir -p "$run/fxtmp/fx-click.caf"
    launch="env -i $base FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 TMPDIR=$run/fxtmp \
FX_GATEWAY_BASE_URL=http://127.0.0.1:$port \
FX_GATEWAY_CHAT_URL=http://127.0.0.1:$port/v4/ai/language-model \
FX_E2E_GATEWAY_MODELS_URL=http://127.0.0.1:$port/coding-agent/v1/models \
$fx_model $extra_env $bin"
  else
    mkdir -p "$run/xfxtmp"
    launch="env -i $base TMPDIR=$run/xfxtmp XFX_GATEWAY_URL=http://127.0.0.1:$port/v4/ai/language-model \
$xfx_model $extra_env $bin"
  fi
  # Every bell the pane rings, one line each, for `bells`. The hook is set on a
  # server started for it, **before** the session that runs the binary: fx
  # rings its start-up cue as soon as it is up (`app_entry_runtime.zig:327`),
  # and a hook set after `new-session` returned can be set after that bell.
  # The pane's output is piped to `$run/wire` (for `wire`) in the same command
  # list, so the server has read none of it before the pipe is attached.
  tmux -f /dev/null -S "$sock" start-server \; \
    set-hook -g alert-bell "run-shell 'echo bell >> $run/bells'" \; \
    new-session -d -s cap -x "$cols" -y "$rows" "cd $run/work && exec $launch" \; \
    pipe-pane -o -t cap "cat >> $run/wire"

  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    local verb=${line%% *} rest=${line#* }
    # A verb with nothing after it has no rest: `relaunch` alone starts the
    # binary again with no arguments, rather than with the word `relaunch`.
    [[ $line == *' '* ]] || rest=''
    case $verb in
      ''|'#'*|gateway|env|seed|work|models|credits|titled|nomodel|githead|sparsepng|pnghdr|dir) ;;
      wait) sleep "$rest" ;;
      type) tmux -f /dev/null -S "$sock" send-keys -t cap -l "$rest" ;;
      paste) tmux -f /dev/null -S "$sock" set-buffer -b capbuf -- "$(printf '%b' "$rest")"; tmux -f /dev/null -S "$sock" paste-buffer -p -b capbuf -t cap ;;
      pastex)
        head -c "$rest" /dev/zero | tr '\0' x > "$run/pastex"
        tmux -f /dev/null -S "$sock" load-buffer -b capbuf "$run/pastex"
        tmux -f /dev/null -S "$sock" paste-buffer -p -b capbuf -t cap
        rm -f "$run/pastex" ;;
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
              # A binary file (an image fx copied) is named, not printed:
              # sed under a UTF-8 locale stops on it, and so did the run.
              *) if LC_ALL=C grep -Iq . "$f" 2>/dev/null || [[ ! -s $f ]]; then
                   head -c 4000 "$f" | LC_ALL=C sed "s#$run#<scratch>#g"; echo
                 else
                   echo "(binary)"
                 fi ;;
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
      cursor)
        local name=$out/$which-$scenario-$size-$rest
        tmux -f /dev/null -S "$sock" display-message -p -t cap '#{cursor_x},#{cursor_y}' > "$name.cursor"
        echo "$name.cursor"
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
      bells)
        local name=$out/$which-$scenario-$size-$rest.bells
        # The hook runs a shell of its own; give it a moment to append.
        sleep 0.3
        {
          echo "bells=$( [[ -f $run/bells ]] && wc -l < "$run/bells" | tr -d ' ' || echo 0)"
          echo "clients=$(tmux -f /dev/null -S "$sock" list-clients | wc -l | tr -d ' ')"
        } > "$name"
        echo "$name: $(tr '\n' ' ' < "$name")"
        ;;
      wire)
        local name=$out/$which-$scenario-$size-$rest.wire
        # The pipe is a `cat` of the server's own; give it a moment to write
        # what the server has already handed it.
        sleep 0.3
        cp "$run/wire" "$name"
        echo "$name ($(wc -c < "$name" | tr -d ' ') bytes)"
        ;;
      relaunch)
        tmux -f /dev/null -S "$sock" respawn-pane -k -t cap "cd $run/work && exec $launch $rest"
        ;;
      clipimage)
        case $rest in
          off) rm -f "$run/clipimage" ;;
          *) cp "$scenarios/$rest" "$run/clipimage" ;;
        esac
        ;;
      clipfile)
        local name=$out/$which-$scenario-$size-$rest
        if [[ -f $run/clipboard-file ]]; then
          LC_ALL=C sed "s#$run#<scratch>#g" "$run/clipboard-file" > "$name.clipfile"
          rm -f "$run/clipboard-file"
          echo "$name.clipfile"
        else
          echo "clipfile $rest: no file was copied"
        fi
        ;;
      reports)
        local tmp=$run/${which}tmp n=0 report
        # Oldest first: the names start with the moment each was written, and
        # two in one second are told apart by their order on disk.
        while IFS= read -r report; do
          n=$((n + 1))
          LC_ALL=C sed "s#$run#<scratch>#g" "$report" > "$out/$which-$scenario-$size-$rest-$n.md"
          # The copy is the receipt's; the mode is the report's own.
          echo "$out/$which-$scenario-$size-$rest-$n.md (written mode $(stat -f %Lp "$report" 2>/dev/null || stat -c %a "$report"))"
        done < <(ls -tr "$tmp"/*-trace-*.md 2>/dev/null)
        (( n > 0 )) || echo "reports $rest: none was written"
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
  if [[ $which == fx ]]; then
    # The chime names only: fx writes other files here too (`/trace`'s
    # fx-trace-*.md), and those are not sound.
    local chime
    for chime in success error bloom press release toggle; do
      [[ -d $run/fxtmp/fx-$chime.m4a ]] || { echo "SOUND GUARD BREACHED: fx wrote fx-$chime.m4a" >&2; exit 3; }
    done
    [[ -d $run/fxtmp/fx-click.caf ]] || { echo "SOUND GUARD BREACHED: fx wrote fx-click.caf" >&2; exit 3; }
  fi
}

main "$@"
