#!/bin/bash
# What fx v0.0.16 or xfx does at launch with each profile fixture, entry by
# entry: the exit status, standard error, how many bytes reached standard output
# or the terminal, and which files appeared or changed under the home, the
# workspace and the TMPDIR. Prints no path: every one is <run>-relative.
#
# usage: probe.sh <fx|xfx> <binary> <run root, short> [fixture...]
#   The run root holds one directory per fixture and the private tmux socket,
#   so keep it short (a socket path is at most 103 bytes). The interactive
#   entries run in a pane of a tmux server on that private socket, under
#   script(1), whose typescript is every byte the binary wrote to the terminal.
set -u
product=$1 bin=$2 root=$3
shift 3
case $product in
  fx) prof=.fx proj=.fx.json ;;
  xfx) prof=.xfx proj=.xfx.json ;;
  *) echo "product must be fx or xfx" >&2; exit 2 ;;
esac
FX_SHA=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
if [ "$product" = fx ] && [ "$(sha "$bin")" != "$FX_SHA" ]; then
  echo "refusing: the fx binary is not the v0.0.16 oracle" >&2; exit 2
fi
echo "binary sha256 before: $(sha "$bin" | cut -c1-12)"

all="truncated nonobject toolarge model-space model-nonstring model-inner-space model-long \
ws-nonobject entry-nonobject entry-model typed-key provider models-bad dir-settings \
link-settings link-profile unreadable project-malformed retired-then-model group-writable valid"
fixtures=${*:-$all}

# The fixture's profile: <home>/<prof>/settings.json, or what stands there.
seed() { # fixture home work
  local f=$1 h=$2 w=$3 p=$2/$prof s=$2/$prof/settings.json
  mkdir -p "$p"; chmod 700 "$p"
  local key; key=$(cd "$w" && pwd -P)
  case $f in
    truncated) printf '{"permission_mode": "auto", "sandbox": ' > "$s" ;;
    nonobject) printf '[1, 2]' > "$s" ;;
    toolarge) { printf '{"pad":"'; head -c 65537 /dev/zero | tr '\0' x; printf '"}'; } > "$s" ;;
    model-space) printf '{"model": " openai/gpt-5"}' > "$s" ;;
    model-nonstring) printf '{"model": 5}' > "$s" ;;
    model-inner-space) printf '{"model": "openai/gpt 5"}' > "$s" ;;
    model-long) printf '{"model": "openai/%s"}' "$(head -c 300 /dev/zero | tr '\0' m)" > "$s" ;;
    ws-nonobject) printf '{"workspaces": []}' > "$s" ;;
    entry-nonobject) printf '{"workspaces": {"%s": 5}}' "$key" > "$s" ;;
    entry-model) printf '{"workspaces": {"%s": {"model": " openai/gpt-5"}}}' "$key" > "$s" ;;
    typed-key) printf '{"fast_mode": "yes"}' > "$s" ;;
    provider) printf '{"provider": "not-a-provider"}' > "$s" ;;
    models-bad) printf '{"models": {"gateway": " openai/gpt-5"}}' > "$s" ;;
    dir-settings) mkdir -p "$s" ;;
    link-settings) printf '{"permission_mode": "auto"}' > "$h/real.json"; ln -s ../real.json "$s" ;;
    link-profile) rmdir "$p"; mkdir -p "$h/real-profile"; chmod 700 "$h/real-profile"
                  printf '{"permission_mode": "auto"}' > "$h/real-profile/settings.json"
                  ln -s real-profile "$p" ;;
    unreadable) printf '{"permission_mode": "auto"}' > "$s"; chmod 000 "$s" ;;
    project-malformed) printf '{"permission_mode": "auto"}' > "$s"; printf '{"max_agent_steps": ' > "$w/$proj" ;;
    retired-then-model) printf '{"skill_match_fuzzy": true, "model": " openai/gpt-5"}' > "$s" ;;
    group-writable) printf '{"permission_mode": "auto"}' > "$s"; chmod 664 "$s" ;;
    valid) printf '{"permission_mode": "auto"}' > "$s" ;;
    *) echo "unknown fixture $f" >&2; return 1 ;;
  esac
  [ -e "$s" ] || [ -L "$s" ] || return 0
  [ "$f" = unreadable ] || [ "$f" = group-writable ] || [ -L "$s" ] || [ -d "$s" ] || chmod 600 "$s"
}

envs() { # home tmp
  local common="HOME=$1 TERM=xterm-256color PATH=/usr/bin:/bin LANG=en_US.UTF-8 TMPDIR=$2 AI_GATEWAY_API_KEY="
  if [ "$product" = fx ]; then
    echo "$common FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 FX_GATEWAY_BASE_URL=http://127.0.0.1:9 FX_AUTO_UPGRADE=0"
  else
    echo "$common XFX_GATEWAY_URL=http://127.0.0.1:9/v1/chat/completions XFX_SOUND=0"
  fi
}

listing() { # dir: every entry with its size and mtime, relative
  (cd "$1" && find . -mindepth 1 \( -name '*.sock' -prune \) -o -print0 2>/dev/null \
    | xargs -0 stat -f '%N %z %m' 2>/dev/null | sort)
}

redact() { sed -e "s#$root#<run>#g" | tr -d '\r'; }

touched() { # before after
  comm -13 "$1" "$2" | awk '{print $1}' | tr '\n' ' '
}

# One non-interactive entry: stdin empty, a 20 s bound.
run_cli() { # label home work tmp args...
  local label=$1 h=$2 w=$3 t=$4 d; shift 4
  d=$(dirname "$h")
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/before"
  (cd "$w" && env -i $(envs "$h" "$t") ${EXTRA_ENV:-} perl -e 'alarm 20; exec @ARGV or die' "$bin" "$@" \
     </dev/null >"$d/out" 2>"$d/err"); local code=$?
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/after"
  printf '  %-22s exit=%s stdout=%sB stderr=%s | touched: %s\n' "$label" "$code" \
    "$(wc -c < "$d/out" | tr -d ' ')" "$(head -c 300 "$d/err" | tr '\n' '|' | redact)" \
    "$(touched "$d/before" "$d/after" | redact)"
  [ -s "$d/out" ] && printf '    stdout: %s\n' "$(head -c 300 "$d/out" | tr '\n' '|' | redact)"
  return 0
}

# One interactive entry in a tmux pane under script(1).
run_tty() { # label home work tmp [env...]
  local label=$1 h=$2 w=$3 t=$4 d sock; shift 4
  d=$(dirname "$h"); sock=$root/s.sock
  rm -f "$d/ts" "$d/err" "$d/exit"
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/before"
  tmux -f /dev/null -S "$sock" new-session -d -x 120 -y 40 -s p \
    "cd $w && script -F -q $d/ts sh -c 'env -i $(envs "$h" "$t") $* $bin ${TTY_ARGS:-} 2>$d/err; echo \$? > $d/exit'"
  local tries=0
  until [ -s "$d/exit" ] || [ $tries -ge 60 ]; do sleep 0.1; tries=$((tries + 1)); done
  local state=exited; [ -s "$d/exit" ] || state="still running after 6 s"
  tmux -f /dev/null -S "$sock" kill-server 2>/dev/null
  sleep 0.3
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/after"
  printf '  %-22s %s exit=%s tty=%sB stderr=%s | touched: %s\n' "$label" "$state" \
    "$(cat "$d/exit" 2>/dev/null | tr -d '\n')" "$(wc -c < "$d/ts" 2>/dev/null | tr -d ' ')" \
    "$(head -c 300 "$d/err" 2>/dev/null | tr '\n' '|' | redact)" "$(touched "$d/before" "$d/after" | redact)"
  return 0
}

fresh() { # fixture -> sets H W T
  local d=$root/$1
  chmod -R u+rwx "$d" 2>/dev/null; rm -rf "$d"
  H=$d/home W=$d/work T=$d/tmp
  mkdir -p "$H" "$W" "$T"
  if [ "$product" = fx ]; then
    for cue in success error bloom press release toggle; do mkdir -p "$T/fx-$cue.m4a"; done
    mkdir -p "$T/fx-click.caf"
  fi
  seed "$1" "$H" "$W"
}

for f in $fixtures; do
  echo "== $f"
  if [ "$product" = fx ]; then
    fresh "$f"; run_cli "permissions" "$H" "$W" "$T" permissions
    fresh "$f"; run_cli "ask" "$H" "$W" "$T" ask hi
  else
    fresh "$f"; run_cli "ask" "$H" "$W" "$T" ask hi
  fi
  if [ "${PROBE_ENTRIES:-}" = all ]; then
    fresh "$f"; run_cli "ask --json" "$H" "$W" "$T" ask --json hi
    fresh "$f"; run_cli "status" "$H" "$W" "$T" status
    fresh "$f"; run_cli "doctor" "$H" "$W" "$T" doctor
    fresh "$f"; run_cli "sessions" "$H" "$W" "$T" sessions
    fresh "$f"; run_cli "session last" "$H" "$W" "$T" session last
    fresh "$f"; run_cli "--version" "$H" "$W" "$T" --version
    fresh "$f"; run_cli "help" "$H" "$W" "$T" help
    fresh "$f"; run_cli "bare, no terminal" "$H" "$W" "$T"
  fi
  fresh "$f"; run_tty "interactive" "$H" "$W" "$T"
  if [ "${PROBE_ENTRIES:-}" = all ]; then
    fresh "$f"; TTY_ARGS=--version run_tty "--version, terminal" "$H" "$W" "$T"
  fi
  if [ "$product" = xfx ]; then
    fresh "$f"; run_tty "line shell" "$H" "$W" "$T" XFX_TUI=0
  fi
done
echo "binary sha256 after: $(sha "$bin" | cut -c1-12)"
