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
#   homefile <path> <file> copies scenarios/<file> to HOME/<path> before
#                          launch, owner-only (the file 0600, every directory
#                          made for it 0700) -- a saved sign-in, say
#                          (`.fx/chatgpt-auth.json`); <path> is relative and
#                          has no `..`
#
# `@PORT@` in a seeded file (`seed`, `homefile`, `reseed`, `rehomefile`) and in
# an `env` value is the fake gateway's port, written in once it listens: the
# same port also answers as a llmux daemon and as a Codex endpoint (fakegw.ts),
# so a profile can name `"llmux_url":"http://127.0.0.1:@PORT@"` and a provider
# switch reaches it. `@HOME@` there is the run's HOME, and `@MCPPORT@` the
# port of the run's HTTP MCP server (`mcphttp`; a scenario that names it
# without one is refused). `@SCENARIOS@` is the scenario directory this run
# reads, as an absolute path, and `@BUN@` the absolute path of the `bun` that
# runs fakegw.ts: a stdio MCP server the scenario keeps beside it,
# `"command":"@BUN@","args":["@SCENARIOS@/p5-mcp-stdio.ts"]`, starts
# on any checkout (the binary's PATH is /usr/bin:/bin, which has no bun). A run
# whose scenario names either, and whose path has a character outside
# `A-Za-z0-9._/+-`, is refused: such a path cannot be written into JSON, a
# shell word and a typed line as itself (and a scenario that names neither
# runs whatever its path). All five are also written into every file a `work`
# or `dir` line copies (a regular text file inside the run: one with no NUL;
# a link is left a link, and nothing is written through one), and into the
# text of a `type`, `paste` or `relaunch` line when it runs, so
# `type /mcp add two -- @BUN@ @SCENARIOS@/p5-mcp-stdio.ts` names the
# fixture on this machine.
#   models <catalog.json>  what the fake gateway answers for
#                          GET /coding-agent/v1/models (default: one model)
#   modelsdown <ms>        that path answers 503 for the first <ms> after the
#                          gateway starts, and the catalog after: a catalog
#                          the launch could not read that is back by the time
#                          `/model` asks again
#   modelsdelay <ms>       a catalog that path serves answers <ms> after it
#                          is asked (a `modelsdown` 503 answers at once), so a
#                          read can still be on its way when the model asks
#                          its question; with it (`0` too) the gateway log's
#                          catalog lines say when, in ms since it started
#   models2 <catalog.json> <ms>
#                          that path answers <catalog.json> instead once <ms>
#                          have passed since the gateway started, by the time
#                          it answers (after any `modelsdelay`): a model
#                          released while the binary runs (fakegw logs each
#                          ask that got it as `-> second catalog`)
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
#                          written as <scratch>, and the paths `@SCENARIOS@`
#                          and `@BUN@` stand for as <scenarios> and <bun> --
#                          what a settings write left
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
#   reseed <settings.json> writes scenarios/<settings.json> over the binary's
#                          own profile now, `@PORT@` written in, owner-only --
#                          between an exit and a `relaunch`, so the second
#                          launch reads a profile the first one did not leave
#   rmhome <path>          deletes HOME/<path> now (a relative path, no `..`)
#                          -- a saved sign-in, between an exit and a
#                          `relaunch`
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
#   resize <rows>x<cols>   resizes the window now (tmux's resize-window, which
#                          sets the window's `window-size` to `manual`, so no
#                          client can size it back), and fails the run unless
#                          the pane is then that size; the kernel sends the
#                          terminal's foreground group SIGWINCH. A later
#                          `snap` keeps the launch size in its file name.
#   jobctl                 runs the binary under jobctl.pl, as a job-control
#                          shell runs a foreground job: in a process group of
#                          its own that owns the terminal, with a parent in its
#                          session. Without it the binary is the pane's session
#                          leader, its group is orphaned, and the kernel
#                          discards a stop signal: SIGTSTP (fx's own Ctrl-Z
#                          `raise` too) does not stop it. With it, SIGTSTP
#                          stops it and SIGCONT continues it.
#   signal <SIG>           sends SIGTSTP, SIGCONT, SIGTERM or SIGHUP (`SIG` may
#                          be left off) to the binary's own pid and nothing
#                          else: the pane's pid -- the binary, `exec`'d -- or,
#                          under `jobctl`, the pid its child wrote before it
#                          exec'd, checked to be that child of the pane's.
#                          Refused unless that pid's executable (lsof's `txt`)
#                          is the binary. Waits up to 3 s for the signal to
#                          show (stopped, running again, gone) and prints the
#                          pid's state, and the pane's dead status when it has
#                          one. Put `keep` first for SIGTERM/SIGHUP: without it
#                          the pane closes with the binary and so does the run.
#   rehomefile <path> <file>
#                          `homefile` now: writes scenarios/<file> over
#                          HOME/<path>, placeholders written in, owner-only
#                          (the file 0600, every directory made for it 0700;
#                          an existing file is written in place), and prints
#                          the mode and size -- a settings file made malformed
#                          after launch; the same <path> rules as `homefile`
#   clock <ms>             pins fx's usage clock (FX_E2E_USAGE_CLOCK_FILE,
#                          vercel-labs/fx@481104fd src/core/app/usage_host.zig:
#                          20-22, :205-219) to <ms> since the epoch, through
#                          HOME/.fxcap-usage-clock (owner-only): with any
#                          `clock` line, fx is launched with that variable set
#                          and the file holding the first one's <ms>; each
#                          `clock` line writes its <ms> there when the script
#                          reaches it. fx's own e2e time is 1791819000000
#                          (2026-10-12 15:30 UTC, usage-fakes.ts:10). xfx is
#                          launched with XFX_E2E_USAGE_CLOCK_FILE naming the
#                          same file: xfx's usage clock (src/usage/clock.rs),
#                          which only a build with debug assertions reads --
#                          target/debug/xfx, the binary this script runs by
#                          default and the CI sweep builds -- and a release
#                          build ignores.
#   usage <usage.json>|off what the fake gateway answers fx v0.0.16's usage
#                          reads with (/v1/report, /ai-gateway/usage-report,
#                          the three /backend-api/wham paths, /billing,
#                          /settings; the file's shape is in fakegw.ts): the
#                          first `usage` line is in place before launch, and
#                          each one swaps it when the script reaches it; `off`
#                          (and no `usage` line at all) leaves those paths
#                          unknown, 404
#   mcphttp <mode>         starts the run's HTTP MCP server (mcphttp.ts) before
#                          launch, on a loopback port the OS picks, in <mode>
#                          -- `ready`, `methodnotfound`, `hang`, `fail` or
#                          `rpcerror` (mcphttp.ts says what each answers) -- and
#                          writes its port where `@MCPPORT@` is, so a
#                          `homefile .fx/mcp.json` naming
#                          `http://127.0.0.1:@MCPPORT@/mcp` reaches it; each
#                          `mcphttp` line sets the mode when the script reaches
#                          it (a held request answers in the new one). Its log
#                          is copied by `gwlog`, as
#                          <bin>-<scenario>-<size>-mcphttp.log.
#   sockets <label>        writes the binary's internet sockets (lsof -i on its
#                          pid) to <bin>-<scenario>-<size>-<label>.sockets, and
#                          fails the run loudly if one is connected to any
#                          address but 127.0.0.1 or ::1
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
#
# fx also upgrades itself: ten seconds after launch its auto-upgrade thread
# fetches the latest release from https://releases.fx.sh and copies it over its
# own executable (vercel-labs/fx@c1db9191 src/main.zig:727-733,
# src/core/upgrade/auto_upgrade.zig:344), unless FX_AUTO_UPGRADE=0 or the binary
# sits under a `/zig-out/bin/` path. A reference binary that upgrades itself is
# no longer the reference, and the run reaches the network. Every fx run is
# launched with FX_AUTO_UPGRADE=0, last in its environment so no scenario `env`
# can lift it, and the run fails loudly if the binary's bytes changed under it.
# Both checks run on every way out of a run that launched fx -- a refused verb,
# a failed `resize`, a `signal` that found no binary -- once the tmux server
# is gone, and a run that passes them says so in one `guards:` line.
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

# Globals, not locals: the EXIT trap runs after main has returned. The guard_*
# three are set once fx's chime names are made, so a run that never launched fx
# is not checked for what it could not have done.
gateway='' sock='' mcphttp='' guard_run='' guard_bin='' guard_sha=''
# The paths `@SCENARIOS@` and `@BUN@` stand for (main sets them), what
# `fill` and `fill_text` write for each, and the one line `fill_text` leaves
# its text in.
path_scenarios='' path_bun='' fill_scenarios='' fill_bun='' filled=''
# A path a placeholder can be written as: absolute, and every character one
# that sed, JSON, a shell word and a typed line all carry as itself.
writable='^/[A-Za-z0-9._/+-]+$'

# fx's sound and upgrade guards (above), every one checked and every breach
# named; the one line that says they held.
guards() {
  local chime breached=0
  # The chime names only: fx writes other files here too (`/trace`'s
  # fx-trace-*.md), and those are not sound.
  for chime in success error bloom press release toggle; do
    [[ -d $guard_run/fxtmp/fx-$chime.m4a ]] || { echo "SOUND GUARD BREACHED: fx wrote fx-$chime.m4a" >&2; breached=1; }
  done
  [[ -d $guard_run/fxtmp/fx-click.caf ]] || { echo "SOUND GUARD BREACHED: fx wrote fx-click.caf" >&2; breached=1; }
  [[ $(shasum -a 256 "$guard_bin" | cut -d' ' -f1) == "$guard_sha" ]] || { echo "UPGRADE GUARD BREACHED: $guard_bin changed during the run" >&2; breached=1; }
  (( breached == 0 )) || return 1
  echo "guards: $guard_bin sha256 ${guard_sha:0:12} unchanged; fx-{success,error,bloom,press,release,toggle}.m4a and fx-click.caf still directories"
}

# Every process the run started, by the pid it started, and then the guards.
cleanup() {
  local code=$?
  [[ -z $sock ]] || tmux -f /dev/null -S "$sock" kill-server 2>/dev/null || true
  [[ -z $gateway ]] || kill "$gateway" 2>/dev/null || true
  [[ -z $mcphttp ]] || kill "$mcphttp" 2>/dev/null || true
  if [[ -n $guard_run ]]; then guards || code=3; fi
  exit "$code"
}

# `@PORT@`, `@HOME@`, `@MCPPORT@`, `@SCENARIOS@` and `@BUN@`, written in: stdin
# to stdout.
fill() {
  local port=$1 home=$2 mcpport=$3
  sed -e "s/@PORT@/$port/g" -e "s#@HOME@#$home#g" -e "s/@MCPPORT@/$mcpport/g" \
    -e "s#@SCENARIOS@#$fill_scenarios#g" -e "s#@BUN@#$fill_bun#g"
}

# The same five in one line of a scenario's own text (`type`, `paste`,
# `relaunch`), left in `filled`: no subshell, so a trailing space stays.
fill_text() {
  local port=$1 home=$2 mcpport=$3
  filled=${4//@PORT@/$port}
  filled=${filled//@HOME@/$home}
  filled=${filled//@MCPPORT@/$mcpport}
  filled=${filled//@SCENARIOS@/$fill_scenarios}
  filled=${filled//@BUN@/$fill_bun}
}

# Refuses a path `@SCENARIOS@` or `@BUN@` would be written as, unless it is
# `writable`.
writable_path() {
  local name=$1 value=$2
  [[ $value =~ $writable ]] && return 0
  echo "@$name@ would be written as '$value': a placeholder is an absolute path of A-Za-z0-9._/+- only" >&2
  exit 2
}

# Refuses a file that names a placeholder this run cannot write in:
# `@MCPPORT@` with no HTTP MCP server, `@SCENARIOS@` or `@BUN@` whose path
# writable_path refuses.
can_fill() {
  local mcpport=$1 file
  shift
  for file in "$@"; do
    if [[ -z $mcpport ]] && grep -q '@MCPPORT@' "$file" 2>/dev/null; then
      echo "$file names @MCPPORT@, and the scenario starts no mcphttp server" >&2
      exit 2
    fi
    if grep -q '@SCENARIOS@' "$file" 2>/dev/null; then writable_path SCENARIOS "$path_scenarios"; fi
    if grep -q '@BUN@' "$file" 2>/dev/null; then writable_path BUN "$path_bun"; fi
  done
  return 0
}

# The binary's own pid, or a failure that says why not: the pane's pid when
# the binary is `exec`'d into it, the pid jobctl.pl's child wrote under
# `jobctl` (that child of the pane's, and no other) -- and in either case one
# whose executable is the binary. Asked again for a moment, so a pid read
# between jobctl.pl's fork and its exec is not taken for an answer.
binary_pid() {
  local sock=$1 run=$2 jobctl=$3 want=$4 pane pid exe tries=0
  local lsof=/usr/sbin/lsof
  [[ -x $lsof ]] || lsof=lsof
  while :; do
    if [[ $(tmux -f /dev/null -S "$sock" display-message -p -t cap '#{pane_dead}') == 1 ]]; then
      echo "the binary is not running: its pane is dead" >&2
      return 1
    fi
    pane=$(tmux -f /dev/null -S "$sock" display-message -p -t cap '#{pane_pid}')
    pid=$pane exe=''
    if [[ -n $jobctl ]]; then
      pid=$(cat "$run/binpid" 2>/dev/null || true)
      [[ -n $pid && $(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ') == "$pane" ]] || pid=''
    fi
    [[ -z $pid ]] || exe=$("$lsof" -nP -a -p "$pid" -d txt -Fn 2>/dev/null | sed -n 's/^n//p' | head -1)
    if [[ -n $pid && $exe == "$want" ]]; then
      echo "$pid"
      return 0
    fi
    if (( ++tries >= 20 )); then
      echo "no pid runs $want: pane ${pane:-?}, pid ${pid:-none}, executable ${exe:-none}" >&2
      return 1
    fi
    sleep 0.1
  done
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
  # What `@SCENARIOS@` and `@BUN@` stand for: checked where a line or a
  # copied file names one. What `fill` writes is the path, or, when the path
  # is not `writable`, the placeholder itself: such a path never reaches sed's
  # program (a `#` ends its expression; `#w <file>` would empty <file> when
  # sed reads it), so a scenario that names neither runs as before, and one
  # that names it is refused where it does.
  path_scenarios=$(cd "$scenarios" && pwd -P)
  path_bun=$(command -v bun || true)
  fill_scenarios=@SCENARIOS@ fill_bun=@BUN@
  if [[ $path_scenarios =~ $writable ]]; then fill_scenarios=$path_scenarios; fi
  if [[ $path_bun =~ $writable ]]; then fill_bun=$path_bun; fi

  local run port='' bin
  run=$(mktemp -d "${TMPDIR:-/tmp}/fxcap.XXXXXX")
  mkdir -p "$run/home" "$run/work"
  case $which in
    fx) bin=${FX_BIN:?set FX_BIN to the reference fx binary} ;;
    xfx) bin=${XFX_BIN:-$root/target/debug/xfx} ;;
    *) echo "fx or xfx, not $which" >&2; exit 2 ;;
  esac
  [[ -x $bin ]] || { echo "not executable: $bin" >&2; exit 2; }
  local bin_sha='' bin_real
  [[ $which == fx ]] && bin_sha=$(shasum -a 256 "$bin" | cut -d' ' -f1)
  # What lsof names as the executable of the binary's pid, for `signal`.
  bin_real=$(realpath "$bin")

  local turns='[]' extra_env='' models='' credits='' titled='' nomodel='' modelsdown='' modelsdelay=''
  local models2='' models2ms='' jobctl='' clock='' usage='' mcpmode='' keep=''
  # The directories `work` and `dir` copied into, whose files get the
  # placeholders too; and whether a line the run fills names `@MCPPORT@`.
  local copied=() names_mcpport=''
  local raw line
  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    # A line whose text is written in when it runs: refused now, not halfway.
    case $line in
      env\ *|type\ *|paste\ *|relaunch\ *)
        [[ $line != *@SCENARIOS@* ]] || writable_path SCENARIOS "$path_scenarios"
        [[ $line != *@BUN@* ]] || writable_path BUN "$path_bun"
        [[ $line != *@MCPPORT@* || $line == env\ * ]] || names_mcpport=1 ;;
    esac
    [[ $line == gateway\ * ]] && turns=$(cat "$scenarios/${line#gateway }")
    [[ $line == keep ]] && keep=1
    [[ $line == env\ * ]] && extra_env="$extra_env ${line#env }"
    # Owner-only, as each product keeps its own profile: xfx refuses to read
    # a profile directory anyone else can reach.
    [[ $line == seed\ * ]] && { mkdir -p -m 700 "$run/home/.$which"; cp "$scenarios/${line#seed }" "$run/home/.$which/settings.json"; chmod 600 "$run/home/.$which/settings.json"; }
    if [[ $line == homefile\ * ]]; then
      local dest from
      read -r dest from <<< "${line#homefile }"
      case $dest in ''|/*|..|../*|*/..|*/../*) echo "homefile takes a relative path inside HOME, not $dest" >&2; exit 2 ;; esac
      (umask 077 && mkdir -p "$run/home/$(dirname "$dest")")
      cp "$scenarios/$from" "$run/home/$dest"
      chmod 600 "$run/home/$dest"
    fi
    [[ $line == models\ * ]] && models=$scenarios/${line#models }
    [[ $line == modelsdown\ * ]] && modelsdown=${line#modelsdown }
    [[ $line == modelsdelay\ * ]] && modelsdelay=${line#modelsdelay }
    [[ $line == credits\ * ]] && credits=$scenarios/${line#credits }
    [[ $line == titled\ * ]] && titled=$scenarios/${line#titled }
    [[ $line == nomodel ]] && nomodel=1
    [[ $line == work\ * ]] && { cp -R "$scenarios/${line#work }/." "$run/work/"; copied+=("$run/work"); }
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
      [[ -n $from ]] && { cp -R "$scenarios/$from/." "$run/$name/"; copied+=("$run/$name"); }
    fi
    # Checked here, before anything is launched: a verb used later in the
    # script is refused now rather than halfway through a run. For `clock`,
    # `usage` and `mcphttp` the first line is the one in place at launch.
    case $line in
      models2\ *)
        read -r models2 models2ms <<< "${line#models2 }"
        [[ -f $scenarios/$models2 && $models2ms =~ ^[0-9]+$ ]] || { echo "models2 takes <catalog.json> <ms>, not ${line#models2 }" >&2; exit 2; }
        models2=$scenarios/$models2 ;;
      jobctl) jobctl=1 ;;
      modelsdelay\ *)
        [[ ${line#modelsdelay } =~ ^[0-9]+$ ]] || { echo "modelsdelay takes milliseconds, not ${line#modelsdelay }" >&2; exit 2; } ;;
      clock\ *)
        [[ ${line#clock } =~ ^[0-9]+$ ]] || { echo "clock takes milliseconds since the epoch, not ${line#clock }" >&2; exit 2; }
        [[ -n $clock ]] || clock=${line#clock } ;;
      usage\ *)
        [[ ${line#usage } == off || -f $scenarios/${line#usage } ]] || { echo "usage takes a file in $scenarios or off, not ${line#usage }" >&2; exit 2; }
        [[ -n $usage ]] || usage=${line#usage } ;;
      mcphttp\ *)
        case ${line#mcphttp } in ready|methodnotfound|hang|fail|rpcerror) ;; *) echo "mcphttp takes ready, methodnotfound, hang, fail or rpcerror, not ${line#mcphttp }" >&2; exit 2 ;; esac
        [[ -n $mcpmode ]] || mcpmode=${line#mcphttp } ;;
      resize\ *)
        [[ ${line#resize } =~ ^[1-9][0-9]{0,3}x[1-9][0-9]{0,3}$ ]] || { echo "resize takes <rows>x<cols>, not ${line#resize }" >&2; exit 2; } ;;
      signal\ *)
        case ${line#signal } in SIGTSTP|SIGCONT|SIGTERM|SIGHUP|TSTP|CONT|TERM|HUP) ;; *) echo "signal takes SIGTSTP, SIGCONT, SIGTERM or SIGHUP, not ${line#signal }" >&2; exit 2 ;; esac ;;
      rehomefile\ *)
        local dest from
        read -r dest from <<< "${line#rehomefile }"
        case $dest in ''|/*|..|../*|*/..|*/../*) echo "rehomefile takes a relative path inside HOME, not $dest" >&2; exit 2 ;; esac
        [[ -n $from && -f $scenarios/$from ]] || { echo "rehomefile takes <path> <file in $scenarios>, not ${line#rehomefile }" >&2; exit 2; } ;;
      jobctl\ *) echo "jobctl takes no argument, not ${line#jobctl }" >&2; exit 2 ;;
      modelsdelay|models2|clock|usage|mcphttp|resize|signal|rehomefile|sockets)
        echo "$line takes an argument: see the verbs at the top of capture.sh" >&2; exit 2 ;;
    esac
  done < "$file"
  printf '%s' "$turns" > "$run/turns.json"
  # Port 0: the OS picks a free one, and the gateway says which.
  mkdir -p "$run/bodies"
  # The usage answers are a file fakegw reads on every request, so a `usage`
  # line later in the script swaps them; absent, those paths are unknown there.
  [[ -n $usage && $usage != off ]] && cp "$scenarios/$usage" "$run/usage.json"
  FAKEGW_MODELS=$models FAKEGW_MODELS_DOWN_MS=$modelsdown FAKEGW_MODELS_DELAY_MS=$modelsdelay FAKEGW_MODELS2=$models2 FAKEGW_MODELS2_MS=$models2ms FAKEGW_CREDITS=$credits FAKEGW_TITLE=$titled FAKEGW_USAGE=$run/usage.json bun "$here/fakegw.ts" 0 "$run/turns.json" "$run/bodies" > "$run/gateway.log" 2>&1 &
  gateway=$!
  sock=$run/s
  trap cleanup EXIT
  local tries=0
  until grep -q listening "$run/gateway.log" 2>/dev/null; do
    kill -0 "$gateway" 2>/dev/null || { cat "$run/gateway.log" >&2; exit 1; }
    (( ++tries < 100 )) || { echo "the fake gateway never listened" >&2; exit 1; }
    sleep 0.1
  done
  port=$(sed -n 's/.*listening \([0-9]*\).*/\1/p' "$run/gateway.log" | head -1)
  # The HTTP MCP server, when the scenario has one: loopback, its own port, its
  # mode a file it reads on every request.
  local mcpport=''
  if [[ -n $mcpmode ]]; then
    printf '%s\n' "$mcpmode" > "$run/mcphttp.mode"
    bun "$here/mcphttp.ts" 0 "$run/mcphttp.mode" > "$run/mcphttp.log" 2>&1 &
    mcphttp=$!
    tries=0
    until grep -q listening "$run/mcphttp.log" 2>/dev/null; do
      kill -0 "$mcphttp" 2>/dev/null || { cat "$run/mcphttp.log" >&2; exit 1; }
      (( ++tries < 100 )) || { echo "the HTTP MCP server never listened" >&2; exit 1; }
      sleep 0.1
    done
    mcpport=$(sed -n 's/.*listening \([0-9]*\).*/\1/p' "$run/mcphttp.log" | head -1)
  fi
  # The placeholders, into every seeded file under HOME that names one and
  # every `env` value; and into every text file (`-I`: one with a NUL, a sparse
  # image say, is left as it is) under each directory a `work` or `dir` line
  # copied into. The names come from `find -type f -print0` and no `-L`: a
  # name with a newline in it is one name, a link is never listed (`cp -R`
  # copied it as a link, and its target is outside the run) and no directory
  # is entered through one. The name read and written is the listed one,
  # never one derived from it: `$(realpath x<NL>)` would lose the newline and
  # name the sibling `x`, maybe a link. Before the write the name is checked
  # once more -- still no link, still a file, and resolving (newline kept)
  # under its own tree -- and refused otherwise. The rewrite goes through
  # `cat >` so a file keeps the mode it was seeded or copied with.
  local root root_real seeded real names='@(PORT|HOME|MCPPORT|SCENARIOS|BUN)@'
  for root in "$run/home" ${copied[@]+"${copied[@]}"}; do
    root_real=$(cd "$root" && pwd -P)
    while IFS= read -r -d '' seeded; do
      if [[ $root == "$run/home" ]]; then
        grep -qE "$names" "$seeded" 2>/dev/null || continue
      else
        grep -qIE "$names" "$seeded" 2>/dev/null || continue
      fi
      real=$(realpath -- "$seeded"; printf x)
      real=${real%$'\nx'}
      if [[ -L $seeded || ! -f $seeded || $real != "$root_real"/* ]]; then
        echo "refusing to rewrite $seeded: it is not a file under $root" >&2
        exit 2
      fi
      can_fill "$mcpport" "$seeded"
      fill "$port" "$run/home" "$mcpport" < "$seeded" > "$run/seed.tmp"
      cat "$run/seed.tmp" > "$seeded"
      rm -f "$run/seed.tmp"
    done < <(find "$root" -type f -print0)
  done
  [[ -n $mcpport || $extra_env != *@MCPPORT@* ]] || { echo "an env value names @MCPPORT@, and the scenario starts no mcphttp server" >&2; exit 2; }
  [[ -n $mcpport || -z $names_mcpport ]] || { echo "a type, paste or relaunch line names @MCPPORT@, and the scenario starts no mcphttp server" >&2; exit 2; }
  extra_env=${extra_env//@PORT@/$port}
  extra_env=${extra_env//@HOME@/$run/home}
  extra_env=${extra_env//@MCPPORT@/$mcpport}
  extra_env=${extra_env//@SCENARIOS@/$fill_scenarios}
  extra_env=${extra_env//@BUN@/$fill_bun}
  # The usage clock (fx's and xfx's), owner-only in HOME, holding the first
  # `clock` line's time.
  local clockfile=''
  if [[ -n $clock ]]; then
    clockfile=$run/home/.fxcap-usage-clock
    (umask 077 && printf '%s\n' "$clock" > "$clockfile")
  fi

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
  # What env runs last: the binary, or under `jobctl` the wrapper that runs it.
  # After FX_AUTO_UPGRADE=0 either way, which stays the last word of the
  # environment.
  local runner=$bin
  [[ -n $jobctl ]] && runner="/usr/bin/perl $here/jobctl.pl $run/binpid $bin"
  if [[ $which == fx ]]; then
    mkdir -p "$run/fxtmp"
    local cue
    for cue in success error bloom press release toggle; do mkdir -p "$run/fxtmp/fx-$cue.m4a"; done
    mkdir -p "$run/fxtmp/fx-click.caf"
    guard_run=$run guard_bin=$bin guard_sha=$bin_sha
    launch="env -i $base FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 TMPDIR=$run/fxtmp \
FX_GATEWAY_BASE_URL=http://127.0.0.1:$port \
FX_GATEWAY_CHAT_URL=http://127.0.0.1:$port/v4/ai/language-model \
FX_E2E_GATEWAY_MODELS_URL=http://127.0.0.1:$port/coding-agent/v1/models \
$fx_model ${clockfile:+FX_E2E_USAGE_CLOCK_FILE=$clockfile} $extra_env FX_AUTO_UPGRADE=0 $runner"
  else
    mkdir -p "$run/xfxtmp"
    launch="env -i $base TMPDIR=$run/xfxtmp XFX_GATEWAY_URL=http://127.0.0.1:$port/v4/ai/language-model \
$xfx_model ${clockfile:+XFX_E2E_USAGE_CLOCK_FILE=$clockfile} $extra_env $runner"
  fi
  # Every bell the pane rings, one line each, for `bells`. The hook is set on a
  # server started for it, **before** the session that runs the binary: fx
  # rings its start-up cue as soon as it is up (`app_entry_runtime.zig:327`),
  # and a hook set after `new-session` returned can be set after that bell.
  # The pane's output is piped to `$run/wire` (for `wire`) in the same command
  # list, so the server has read none of it before the pipe is attached.
  # A scenario that `keep`s its pane keeps it from the first byte, as the
  # pipe is attached: a binary that refuses at once (no usable HOME, say)
  # can be gone before the scenario's own `keep` line runs, and the server
  # with it.
  local hold=()
  [[ -z $keep ]] || hold=(set-option -gw remain-on-exit on ';')
  tmux -f /dev/null -S "$sock" start-server \; \
    set-hook -g alert-bell "run-shell 'echo bell >> $run/bells'" \; \
    ${hold[@]+"${hold[@]}"} \
    new-session -d -s cap -x "$cols" -y "$rows" "cd $run/work && exec $launch" \; \
    pipe-pane -o -t cap "cat >> $run/wire"

  while IFS= read -r raw; do
    line=$(for_this "$which" "$raw")
    local verb=${line%% *} rest=${line#* }
    # A verb with nothing after it has no rest: `relaunch` alone starts the
    # binary again with no arguments, rather than with the word `relaunch`.
    [[ $line == *' '* ]] || rest=''
    case $verb in
      ''|'#'*|gateway|env|seed|homefile|work|models|credits|titled|modelsdown|modelsdelay|models2|jobctl|nomodel|githead|sparsepng|pnghdr|dir) ;;
      wait) sleep "$rest" ;;
      type) fill_text "$port" "$run/home" "$mcpport" "$rest"; tmux -f /dev/null -S "$sock" send-keys -t cap -l "$filled" ;;
      paste) fill_text "$port" "$run/home" "$mcpport" "$rest"; tmux -f /dev/null -S "$sock" set-buffer -b capbuf -- "$(printf '%b' "$filled")"; tmux -f /dev/null -S "$sock" paste-buffer -p -b capbuf -t cap ;;
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
      gwlog)
        cp "$run/gateway.log" "$out/$which-$scenario-$size-gateway.log"; rm -rf "$out/$which-$scenario-$size-bodies"; cp -R "$run/bodies" "$out/$which-$scenario-$size-bodies"
        [[ ! -f $run/mcphttp.log ]] || cp "$run/mcphttp.log" "$out/$which-$scenario-$size-mcphttp.log" ;;
      # One name per line rather than a word-split `$(find)`, so a file
      # with a space in its name is listed whole.
      workls)
        (cd "$run/work" && find . -type f | sort | while IFS= read -r f; do
          echo "== $f"; head -c 2000 "$f"; echo
        done) > "$out/$which-$scenario-$size-work-$rest.txt"
        ;;
      home)
        local name=$out/$which-$scenario-$size-$rest.files
        # The run's own directory, and the paths `@SCENARIOS@` and `@BUN@`
        # wrote, by those names rather than as this machine's paths.
        local redact=(-e "s#$run#<scratch>#g")
        [[ $fill_scenarios == @SCENARIOS@ ]] || redact+=(-e "s#${fill_scenarios//./\\.}#<scenarios>#g")
        [[ $fill_bun == @BUN@ ]] || redact+=(-e "s#${fill_bun//./\\.}#<bun>#g")
        (
          cd "$run/home"
          find . -type f ! -path '*/file-index/*' | LC_ALL=C sort | while IFS= read -r f; do
            echo "=== ~/${f#./} ($(wc -c < "$f" | tr -d ' ') bytes)"
            case $f in
              *.lock|*.log|*.sqlite*) ;;
              # A binary file (an image fx copied) is named, not printed:
              # sed under a UTF-8 locale stops on it, and so did the run.
              *) if LC_ALL=C grep -Iq . "$f" 2>/dev/null || [[ ! -s $f ]]; then
                   head -c 4000 "$f" | LC_ALL=C sed "${redact[@]}"; echo
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
        fill_text "$port" "$run/home" "$mcpport" "$rest"
        tmux -f /dev/null -S "$sock" respawn-pane -k -t cap "cd $run/work && exec $launch $filled"
        ;;
      reseed)
        can_fill "$mcpport" "$scenarios/$rest"
        (umask 077 && mkdir -p "$run/home/.$which" && fill "$port" "$run/home" "$mcpport" < "$scenarios/$rest" > "$run/home/.$which/settings.json")
        chmod 600 "$run/home/.$which/settings.json"
        ;;
      rehomefile)
        local dest from
        read -r dest from <<< "$rest"
        can_fill "$mcpport" "$scenarios/$from"
        (umask 077 && mkdir -p "$run/home/$(dirname "$dest")" && fill "$port" "$run/home" "$mcpport" < "$scenarios/$from" > "$run/home/$dest")
        chmod 600 "$run/home/$dest"
        echo "rehomefile ~/$dest: mode $(stat -f %Lp "$run/home/$dest" 2>/dev/null || stat -c %a "$run/home/$dest"), $(wc -c < "$run/home/$dest" | tr -d ' ') bytes"
        ;;
      resize)
        local got
        tmux -f /dev/null -S "$sock" resize-window -t cap -x "${rest#*x}" -y "${rest%x*}"
        got=$(tmux -f /dev/null -S "$sock" display-message -p -t cap '#{pane_height}x#{pane_width}')
        [[ $got == "$rest" ]] || { echo "resize $rest: the pane is $got" >&2; exit 1; }
        echo "resize: the pane is now $got"
        ;;
      signal)
        local sig=${rest#SIG} pid state='' tries=0 dead=''
        pid=$(binary_pid "$sock" "$run" "$jobctl" "$bin_real") || exit 1
        kill -s "$sig" "$pid"
        while (( tries++ < 30 )); do
          state=$(ps -o stat= -p "$pid" 2>/dev/null | tr -d ' ') || state=gone
          case $sig in
            TSTP) [[ $state == T* ]] && break ;;
            CONT) [[ $state != T* ]] && break ;;
            *) [[ $state == gone || $state == Z* ]] && dead=$(tmux -f /dev/null -S "$sock" display-message -p -t cap '#{pane_dead}' 2>/dev/null || echo closed)
               [[ $dead == 1 || $dead == closed ]] && break ;;
          esac
          sleep 0.1
        done
        case $dead in
          1) dead="; $(tmux -f /dev/null -S "$sock" display-message -p -t cap 'pane dead, #{?pane_dead_signal,signal #{pane_dead_signal},status #{pane_dead_status}}')" ;;
          closed) dead='; pane closed' ;;
        esac
        echo "signal SIG$sig -> pid $pid: ${state}$dead"
        ;;
      clock) (umask 077 && printf '%s\n' "$rest" > "$run/home/.fxcap-usage-clock") ;;
      usage)
        # Renamed into place, so fakegw never reads a file half written.
        case $rest in
          off) rm -f "$run/usage.json" ;;
          *) cp "$scenarios/$rest" "$run/usage.tmp"; mv -f "$run/usage.tmp" "$run/usage.json" ;;
        esac
        ;;
      mcphttp) printf '%s\n' "$rest" > "$run/mcphttp.tmp"; mv -f "$run/mcphttp.tmp" "$run/mcphttp.mode" ;;
      sockets)
        local name=$out/$which-$scenario-$size-$rest.sockets pid lsof=/usr/sbin/lsof
        [[ -x $lsof ]] || lsof=lsof
        pid=$(binary_pid "$sock" "$run" "$jobctl" "$bin_real") || exit 1
        # lsof exits 1 when the pid has no internet socket at all.
        { echo "pid $pid"; "$lsof" -nP -a -p "$pid" -i 2>/dev/null | awk 'NR > 1 { print $5, $8, $9, $10 }' || true; } > "$name"
        echo "$name: $(( $(wc -l < "$name") - 1 )) socket(s)"
        # Every address on every line, the local and the remote: 127.0.0.1,
        # [::1], or `*` (a port listening on nothing yet).
        if awk 'NR > 1 { n = split($3, ends, "->"); for (i = 1; i <= n; i++) { a = ends[i]; sub(/:[0-9*]+$/, "", a); if (a != "127.0.0.1" && a != "[::1]" && a != "*") bad = 1 } } END { exit !bad }' "$name"; then
          echo "NETWORK GUARD BREACHED: the binary has a socket off loopback:" >&2
          cat "$name" >&2
          exit 3
        fi
        ;;
      rmhome)
        case $rest in ''|/*|..|../*|*/..|*/../*) echo "rmhome takes a relative path inside HOME, not $rest" >&2; exit 2 ;; esac
        rm -f "$run/home/$rest"
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
  # fx's guards run in the EXIT trap (cleanup), after the tmux server is gone.
}

main "$@"
