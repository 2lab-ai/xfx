#!/bin/bash
# PROFILE-JSON-LIMITS's probe: PROFILE-REFUSE-TYPED-KEYS's (its receipt), with
# this unit's fixtures -- nesting depths and numbers past f64 -- and `status`
# written out whole. What fx v0.0.16 or xfx does with each, entry by entry:
# the exit status, standard error, standard output, and which files appeared
# or changed under the home, the workspace and the TMPDIR. Prints no path:
# every one is <run>-relative.
#
# Depth N is N containers one inside another, the settings object counted:
# `{"zzz": [[...]]}` holds N-1 arrays. serde_json takes 127 and refuses 128.
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

all="deep-127 deep-128 deep-129 deep-1000 deep-max deep-100000 deep-objects-1000 \
deep-entry deep-steps deep-dup deep-torn \
num-big num-neg-big num-int-30 num-neg-int-30 num-int-400 num-tiny num-neg-zero \
steps-big steps-neg-big steps-int-30 steps-tiny steps-neg-zero steps-i64-max steps-ok \
proj-big proj-deep proj-steps-big proj-steps-float"
fixtures=${*:-$all}

rep() { head -c "$1" /dev/zero | tr '\0' "$2"; } # count char
# `{"zzz": ` + depth-1 arrays + `}`: the deepest a file within 64 KiB holds.
max_depth=$(( (65536 - 9) / 2 + 1 ))
deep() { printf '{"zzz": %s%s}' "$(rep $(($1 - 1)) '[')" "$(rep $(($1 - 1)) ']')"; }

# The fixture's profile: <home>/<prof>/settings.json, and its project file.
seed() { # fixture home work
  local f=$1 h=$2 w=$3 p=$2/$prof s=$2/$prof/settings.json
  mkdir -p "$p"; chmod 700 "$p"
  local key; key=$(cd "$w" && pwd -P)
  case $f in
    deep-127) deep 127 > "$s" ;;
    deep-128) deep 128 > "$s" ;;
    deep-129) deep 129 > "$s" ;;
    deep-1000) deep 1000 > "$s" ;;
    deep-max) deep $max_depth > "$s" ;;
    deep-100000) deep 100000 > "$s" ;;
    deep-objects-1000) { printf '{"zzz": '; for _ in $(seq 998); do printf '{"a": '; done
                         printf '0'; rep 999 '}'; } > "$s" ;;
    deep-entry) printf '{"workspaces": {"%s": {"zzz": %s%s}}}' "$key" "$(rep 200 '[')" "$(rep 200 ']')" > "$s" ;;
    deep-steps) printf '{"max_agent_steps": %s%s}' "$(rep 200 '[')" "$(rep 200 ']')" > "$s" ;;
    deep-dup) printf '{"zzz": %s{"a": 1, "a": 2}%s}' "$(rep 200 '[')" "$(rep 200 ']')" > "$s" ;;
    deep-torn) printf '{"zzz": %s%s}' "$(rep 200 '[')" "$(rep 199 ']')" > "$s" ;;
    num-big) printf '{"zzz": 1e400}' > "$s" ;;
    num-neg-big) printf '{"zzz": -1e400}' > "$s" ;;
    num-int-30) printf '{"zzz": 123456789012345678901234567890}' > "$s" ;;
    num-neg-int-30) printf '{"zzz": -123456789012345678901234567890}' > "$s" ;;
    num-int-400) printf '{"zzz": %s}' "$(rep 400 7)" > "$s" ;;
    num-tiny) printf '{"zzz": 1e-400}' > "$s" ;;
    num-neg-zero) printf '{"zzz": -0}' > "$s" ;;
    steps-big) printf '{"max_agent_steps": 1e400}' > "$s" ;;
    steps-neg-big) printf '{"max_agent_steps": -1e400}' > "$s" ;;
    steps-int-30) printf '{"max_agent_steps": 123456789012345678901234567890}' > "$s" ;;
    steps-tiny) printf '{"max_agent_steps": 1e-400}' > "$s" ;;
    steps-neg-zero) printf '{"max_agent_steps": -0}' > "$s" ;;
    steps-i64-max) printf '{"max_agent_steps": 9223372036854775807}' > "$s" ;;
    steps-ok) printf '{"max_agent_steps": 3, "zzz": 1e400, "yyy": %s%s}' "$(rep 200 '[')" "$(rep 200 ']')" > "$s" ;;
    proj-big) printf '{"permission_mode": "auto"}' > "$s"; printf '{"zzz": 1e400, "max_agent_steps": 3}' > "$w/$proj" ;;
    proj-deep) printf '{"permission_mode": "auto"}' > "$s"
               printf '{"zzz": %s%s, "max_agent_steps": 3}' "$(rep 200 '[')" "$(rep 200 ']')" > "$w/$proj" ;;
    proj-steps-big) printf '{"permission_mode": "auto"}' > "$s"; printf '{"max_agent_steps": 1e400}' > "$w/$proj" ;;
    proj-steps-float) printf '{"permission_mode": "auto"}' > "$s"; printf '{"max_agent_steps": 5.0}' > "$w/$proj" ;;
    *) echo "unknown fixture $f" >&2; return 1 ;;
  esac
  chmod 600 "$s"
}

envs() { # home tmp
  local common="HOME=$1 TERM=xterm-256color PATH=/usr/bin:/bin LANG=en_US.UTF-8 TMPDIR=$2 AI_GATEWAY_API_KEY="
  if [ "$product" = fx ]; then
    echo "$common FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 FX_GATEWAY_BASE_URL=http://127.0.0.1:9 FX_AUTO_UPGRADE=0"
  else
    echo "$common XFX_GATEWAY_URL=http://127.0.0.1:9/v1/chat/completions XFX_SOUND=0"
  fi
}

listing() { # dir: every entry with its size and mtime, relative.
  (cd "$1" && find . -mindepth 1 \( -name '*.sock' -prune \) -o -print0 2>/dev/null \
    | xargs -0 stat -f '%N %z %m' 2>/dev/null | sort)
}

real_root=$(mkdir -p "$root" && cd "$root" && pwd -P)
# The run root, by both its spellings, before any line is cut short.
redact() { sed -e "s#$real_root#<run>#g" -e "s#$root#<run>#g" | tr -d '\r'; }

touched() { # before after
  comm -13 "$1" "$2" | awk '{print $1}' | tr '\n' ' '
}

# One non-interactive entry: stdin empty, a 20 s bound. `status` is written
# out whole, one line per line; any other stdout cut at 300 bytes.
run_cli() { # label home work tmp args...
  local label=$1 h=$2 w=$3 t=$4 d; shift 4
  d=$(dirname "$h")
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/before"
  (cd "$w" && env -i $(envs "$h" "$t") perl -e 'alarm 20; exec @ARGV or die' "$bin" "$@" \
     </dev/null >"$d/out" 2>"$d/err"); local code=$?
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/after"
  printf '  %-12s exit=%s stdout=%sB stderr=%s | touched: %s\n' "$label" "$code" \
    "$(wc -c < "$d/out" | tr -d ' ')" "$(redact < "$d/err" | tr '\n' '|' | head -c 300)" \
    "$(touched "$d/before" "$d/after" | redact)"
  if [ "$label" = status ]; then
    redact < "$d/out" | grep -E 'build_revision|agent_step_limit|max_agent_steps|config|settings|error' | sed 's/^/    | /'
  elif [ -s "$d/out" ]; then
    printf '    stdout: %s\n' "$(redact < "$d/out" | tr '\n' '|' | head -c 300)"
  fi
  return 0
}

# One interactive entry in a tmux pane under script(1).
run_tty() { # label home work tmp [env...]
  local label=$1 h=$2 w=$3 t=$4 d sock; shift 4
  d=$(dirname "$h"); sock=$root/s.sock
  rm -f "$d/ts" "$d/err" "$d/exit"
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/before"
  tmux -f /dev/null -S "$sock" new-session -d -x 120 -y 40 -s p \
    "cd $w && script -F -q $d/ts sh -c 'env -i $(envs "$h" "$t") $* $bin 2>$d/err; echo \$? > $d/exit'"
  local tries=0
  until [ -s "$d/exit" ] || [ $tries -ge 60 ]; do sleep 0.1; tries=$((tries + 1)); done
  local state=exited; [ -s "$d/exit" ] || state="still running after 6 s"
  tmux -f /dev/null -S "$sock" kill-server 2>/dev/null
  sleep 0.3
  for x in "$h" "$w" "$t"; do listing "$x"; done > "$d/after"
  printf '  %-12s %s exit=%s tty=%sB stderr=%s\n' "$label" "$state" \
    "$(cat "$d/exit" 2>/dev/null | tr -d '\n')" "$(wc -c < "$d/ts" 2>/dev/null | tr -d ' ')" \
    "$(redact < "$d/err" 2>/dev/null | tr '\n' '|' | head -c 300)"
  return 0
}

fresh() { # fixture -> sets H W T
  local d=$root/$1
  chmod -R u+rwx "$d" 2>/dev/null; rm -rf "$d"
  [ ! -e "$d" ] || { echo "cannot clear $1's run" >&2; exit 2; }
  H=$d/home W=$d/work T=$d/tmp
  mkdir -p "$H" "$W" "$T"
  if [ "$product" = fx ]; then
    for cue in success error bloom press release toggle; do mkdir -p "$T/fx-$cue.m4a"; done
    mkdir -p "$T/fx-click.caf"
  fi
  seed "$1" "$H" "$W"
}

for f in $fixtures; do
  fresh "$f"
  echo "== $f (profile $(wc -c < "$H/$prof/settings.json" | tr -d ' ') bytes$([ -e "$W/$proj" ] && printf ', project %s bytes' "$(wc -c < "$W/$proj" | tr -d ' ')"))"
  if [ "$product" = fx ]; then
    fresh "$f"; run_cli "permissions" "$H" "$W" "$T" permissions
  fi
  fresh "$f"; run_cli "ask" "$H" "$W" "$T" ask hi
  fresh "$f"; run_cli "status" "$H" "$W" "$T" status
  fresh "$f"; run_tty "interactive" "$H" "$W" "$T"
  if [ "$product" = xfx ]; then
    fresh "$f"; run_tty "line shell" "$H" "$W" "$T" XFX_TUI=0
  fi
done
echo "binary sha256 after: $(sha "$bin" | cut -c1-12)"
