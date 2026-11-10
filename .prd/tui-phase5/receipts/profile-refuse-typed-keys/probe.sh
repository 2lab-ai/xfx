#!/bin/bash
# PROFILE-REFUSE-AT-LAUNCH's probe (its receipt), with this unit's fixtures
# and `status` among the entries every run takes.
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

all="truncated nonobject toolarge at-ceiling model-space model-nonstring model-inner-space model-long \
ws-nonobject entry-nonobject entry-model typed-key provider models-bad dir-settings \
link-settings link-profile unreadable project-malformed retired-then-model group-writable \
group-writable-torn group-writable-model other-writable-torn dir-group-writable-torn write-only \
dir-unlistable hardlink profile-file fifo home-unlistable dup-key valid \
hardlink-unreadable home-unlistable-empty \
typed-permission typed-entry typed-steps typed-steps-negative typed-effort typed-mode \
typed-notify-max typed-statusline entry-statusline entry-statusline-ws typed-history \
typed-authorities authorities-then-typed limits-then-typed typed-xfx-only typed-fx-only \
additional-global dup-nested dup-unread dup-entry dup-escaped dup-project \
provider-type provider-badid provider-codex provider-llmux provider-upper provider-entry \
provider-salvaged provider-typed-unknown models-type models-key models-value-type \
models-llmux model-then-models model-bad-with-provider models-inner-space \
retired-then-provider mode-upper mode-padded steps-float steps-huge effort-bad typed-statusline-ws \
provider-entry-fixes limits-ok-then-typed limits-off-then-typed limits-underscore-then-typed \
limits-plus-then-typed limits-negative-then-typed limits-unknown-then-typed \
limits-double-underscore-then-typed limits-leading-underscore-then-typed limits-empty-then-typed \
model-then-provider \
provider-missing-empty provider-missing-other provider-defined providers-type providers-null \
providers-many providers-bad-def models-35 models-36 model-and-35 models-split models-case \
models-order drift-auto-compact json-big-float json-big-int json-deep entry-providers"
fixtures=${*:-$all}

# The fixture's profile: <home>/<prof>/settings.json, or what stands there.
seed() { # fixture home work
  local f=$1 h=$2 w=$3 p=$2/$prof s=$2/$prof/settings.json
  mkdir -p "$p"; chmod 700 "$p"
  local key; key=$(cd "$w" && pwd -P)
  case $f in
    truncated) printf '{"permission_mode": "auto", "sandbox": ' > "$s" ;;
    nonobject) printf '[1, 2]' > "$s" ;;
    toolarge) { printf '{"pad":"'; head -c 65527 /dev/zero | tr '\0' x; printf '"}'; } > "$s" ;;
    at-ceiling) { printf '{"pad":"'; head -c 65526 /dev/zero | tr '\0' x; printf '"}'; } > "$s" ;;
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
    group-writable-torn) printf '{"permission_mode": "auto", "sandbox": ' > "$s"; chmod 664 "$s" ;;
    group-writable-model) printf '{"model":" a"}' > "$s"; chmod 664 "$s" ;;
    other-writable-torn) printf '{"permission_mode": "auto", "sandbox": ' > "$s"; chmod 602 "$s" ;;
    dir-group-writable-torn) printf '{"permission_mode": "auto", "sandbox": ' > "$s"; chmod 600 "$s"; chmod 770 "$p" ;;
    write-only) printf '{"permission_mode": "auto"}' > "$s"; chmod 222 "$s" ;;
    dir-unlistable) printf '{"permission_mode": "auto"}' > "$s"; chmod 600 "$s"; chmod 300 "$p" ;;
    hardlink) printf '{"permission_mode": "auto"}' > "$s"; chmod 600 "$s"; ln "$s" "$h/other.json" ;;
    profile-file) rmdir "$p"; printf '{}' > "$p" ;;
    fifo) mkfifo -m 600 "$s" ;;
    home-unlistable) printf '{"permission_mode": "auto"}' > "$s"; chmod 600 "$s"; chmod 300 "$h" ;;
    home-unlistable-empty) rmdir "$p"; chmod 300 "$h" ;;
    hardlink-unreadable) printf '{"permission_mode": "auto"}' > "$s"; ln "$s" "$h/other.json"; chmod 000 "$s" ;;
    dup-key) printf '{"permission_mode": "auto", "permission_mode": "ask"}' > "$s" ;;
    typed-permission) printf '{"permission": {"bash": "perhaps"}}' > "$s" ;;
    typed-entry) printf '{"workspaces": {"%s": {"fast_mode": "yes"}}}' "$key" > "$s" ;;
    typed-steps) printf '{"max_agent_steps": "seven"}' > "$s" ;;
    typed-steps-negative) printf '{"max_agent_steps": -1}' > "$s" ;;
    typed-effort) printf '{"effort": 5}' > "$s" ;;
    typed-mode) printf '{"permission_mode": "sometimes"}' > "$s" ;;
    typed-notify-max) printf '{"notifications": {"max": "loud"}}' > "$s" ;;
    typed-statusline) printf '{"statusLine": 5}' > "$s" ;;
    entry-statusline) printf '{"workspaces": {"%s": {"statusLine": 5}}}' "$key" > "$s" ;;
    entry-statusline-ws) printf '{"workspaces": {"%s": {"statusLine": {"workspace": "x"}}}}' "$key" > "$s" ;;
    typed-history) printf '{"prompt_history": {"enabled": "x"}}' > "$s" ;;
    typed-authorities) printf '{"skill_symlink_authorities": 5}' > "$s" ;;
    authorities-then-typed) printf '{"skill_symlink_authorities": 5, "notifications": 5}' > "$s" ;;
    limits-then-typed) printf '{"context_limits": "x", "fast_mode": "yes"}' > "$s" ;;
    typed-xfx-only) printf '{"backend": 5, "llmux_url": 5}' > "$s" ;;
    typed-fx-only) printf '{"theme": 5}' > "$s" ;;
    additional-global) printf '{"additional_directories": 5}' > "$s" ;;
    dup-nested) printf '{"permission": {"bash": "allow", "bash": "deny"}}' > "$s" ;;
    dup-unread) printf '{"zzz": {"a": 1, "a": 2}}' > "$s" ;;
    dup-entry) printf '{"workspaces": {"%s": {"model": "a/b", "model": "a/c"}}}' "$key" > "$s" ;;
    dup-escaped) printf '{"fast_mode": true, "fast\\u005fmode": true}' > "$s" ;;
    dup-project) printf '{"permission_mode": "auto"}' > "$s"; printf '{"max_agent_steps": 1, "max_agent_steps": 2}' > "$w/$proj" ;;
    provider-type) printf '{"provider": 5}' > "$s" ;;
    provider-badid) printf '{"provider": "not a provider"}' > "$s" ;;
    provider-codex) printf '{"provider": "codex"}' > "$s" ;;
    provider-llmux) printf '{"provider": "llmux"}' > "$s" ;;
    provider-upper) printf '{"provider": "GATEWAY"}' > "$s" ;;
    provider-entry) printf '{"workspaces": {"%s": {"provider": "not-a-provider"}}}' "$key" > "$s" ;;
    provider-salvaged) printf '{"fast_mode": "yes", "provider": "gateway"}' > "$s" ;;
    provider-typed-unknown) printf '{"fast_mode": "yes", "provider": "not-a-provider"}' > "$s" ;;
    models-type) printf '{"models": 5}' > "$s" ;;
    models-key) printf '{"models": {"not a provider": "openai/gpt-5"}}' > "$s" ;;
    models-value-type) printf '{"models": {"gateway": 5}}' > "$s" ;;
    models-llmux) printf '{"models": {"llmux": "anthropic/claude-x"}}' > "$s" ;;
    model-then-models) printf '{"model": " a", "models": {"gateway": " b"}}' > "$s" ;;
    model-bad-with-provider) printf '{"model": " a", "provider": "gateway"}' > "$s" ;;
    models-inner-space) printf '{"models": {"gateway": "openai/gpt 5"}}' > "$s" ;;
    retired-then-provider) printf '{"skill_match_fuzzy": true, "provider": 5}' > "$s" ;;
    mode-upper) printf '{"permission_mode": "ASK"}' > "$s" ;;
    mode-padded) printf '{"permission_mode": " ask"}' > "$s" ;;
    steps-float) printf '{"max_agent_steps": 5.0}' > "$s" ;;
    steps-huge) printf '{"max_agent_steps": 9223372036854775808}' > "$s" ;;
    effort-bad) printf '{"effort": "!!"}' > "$s" ;;
    typed-statusline-ws) printf '{"statusLine": {"workspace": "x"}}' > "$s" ;;
    provider-entry-fixes) printf '{"provider": "not-a-provider", "workspaces": {"%s": {"provider": "gateway"}}}' "$key" > "$s" ;;
    limits-ok-then-typed) printf '{"context_limits": {"skill_chunk_bytes": 5}, "fast_mode": "yes"}' > "$s" ;;
    limits-off-then-typed) printf '{"context_limits": {"skill_chunk_bytes": "OFF"}, "fast_mode": "yes"}' > "$s" ;;
    limits-underscore-then-typed) printf '{"context_limits": {"skill_chunk_bytes": "1_000"}, "fast_mode": "yes"}' > "$s" ;;
    limits-plus-then-typed) printf '{"context_limits": {"skill_chunk_bytes": "+5"}, "fast_mode": "yes"}' > "$s" ;;
    limits-negative-then-typed) printf '{"context_limits": {"skill_chunk_bytes": -1}, "fast_mode": "yes"}' > "$s" ;;
    limits-unknown-then-typed) printf '{"context_limits": {"nope": 5}, "fast_mode": "yes"}' > "$s" ;;
    limits-double-underscore-then-typed) printf '{"context_limits": {"skill_chunk_bytes": "1__0"}, "fast_mode": "yes"}' > "$s" ;;
    limits-leading-underscore-then-typed) printf '{"context_limits": {"skill_chunk_bytes": "_1"}, "fast_mode": "yes"}' > "$s" ;;
    limits-empty-then-typed) printf '{"context_limits": {"skill_chunk_bytes": ""}, "fast_mode": "yes"}' > "$s" ;;
    model-then-provider) printf '{"model": " a", "provider": 5}' > "$s" ;;
    provider-missing-empty) printf '{"provider": "missing", "providers": {}}' > "$s" ;;
    provider-missing-other) printf '{"provider": "missing", "providers": {"other": %s}}' "$DEF" > "$s" ;;
    provider-defined) printf '{"provider": "local", "providers": {"local": %s}}' "$DEF" > "$s" ;;
    providers-type) printf '{"providers": 5}' > "$s" ;;
    providers-null) printf '{"providers": null}' > "$s" ;;
    providers-many) printf '{"providers": {%s}}' "$(names 01 33 1)" > "$s" ;;
    providers-bad-def) printf '{"providers": {"x": 5}}' > "$s" ;;
    models-35) printf '{"models": {%s}}' "$(names 01 35 '"m/x"')" > "$s" ;;
    models-36) printf '{"models": {%s}}' "$(names 01 36 '"m/x"')" > "$s" ;;
    model-and-35) printf '{"model": "a/b", "models": {%s}}' "$(names 01 35 '"m/x"')" > "$s" ;;
    models-split) printf '{"models": {%s}, "workspaces": {"%s": {"models": {%s}}}}' "$(names 01 20 '"m/x"')" "$key" "$(names 21 40 '"m/x"')" > "$s" ;;
    models-case) printf '{"models": {"GATEWAY": "a/b", "gateway": "a/c"}}' > "$s" ;;
    models-order) printf '{"models": {"zz": 5, "aa": " x"}}' > "$s" ;;
    drift-auto-compact) printf '{"auto_compact_percent": "x", "provider": 5}' > "$s" ;;
    json-big-float) printf '{"zzz": 1e400}' > "$s" ;;
    json-big-int) printf '{"zzz": %s}' "$(head -c 400 /dev/zero | tr '\0' 7)" > "$s" ;;
    json-deep) printf '{"zzz": %s%s}' "$(head -c 129 /dev/zero | tr '\0' '[')" "$(head -c 129 /dev/zero | tr '\0' ']')" > "$s" ;;
    entry-providers) printf '{"workspaces": {"%s": {"providers": 5}}}' "$key" > "$s" ;;
    valid) printf '{"permission_mode": "auto"}' > "$s" ;;
    *) echo "unknown fixture $f" >&2; return 1 ;;
  esac
  [ -e "$s" ] || [ -L "$s" ] || return 0
  case $f in unreadable|group-writable*|other-writable*|dir-*|write-only|hardlink*|fifo|home-*) return 0 ;; esac
  [ -L "$s" ] || [ -d "$s" ] || chmod 600 "$s"
}

envs() { # home tmp
  local common="HOME=$1 TERM=xterm-256color PATH=/usr/bin:/bin LANG=en_US.UTF-8 TMPDIR=$2 AI_GATEWAY_API_KEY="
  if [ "$product" = fx ]; then
    echo "$common FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 FX_GATEWAY_BASE_URL=http://127.0.0.1:9 FX_AUTO_UPGRADE=0"
  else
    echo "$common XFX_GATEWAY_URL=http://127.0.0.1:9/v1/chat/completions XFX_SOUND=0"
  fi
}

listing() { # dir: every entry with its size and mtime, relative. A directory
  # its owner may not list is listed with u+r given for the look, then its mode
  # put back: a mode change moves no mtime. (No run path holds a blank.)
  local d undo="" pass
  for pass in 1 2 3; do # a locked directory inside another shows once that one opens
    for d in $(find "$1" -type d ! -perm -u+r 2>/dev/null); do
      undo="chmod $(stat -f %Lp "$d") $d; $undo"; chmod u+r "$d"
    done
  done
  (cd "$1" && find . -mindepth 1 \( -name '*.sock' -prune \) -o -print0 2>/dev/null \
    | xargs -0 stat -f '%N %z %m' 2>/dev/null | sort)
  eval "$undo"
}

# Round 2: a loopback connection definition, and N distinct connection names.
DEF='{"protocol": "openai-chat-completions", "base_url": "http://127.0.0.1:9/v1", "auth": {"type": "none"}}'
names() { # first last value: "pFF": value, ... for FF in first..last
  local i out=""; for i in $(seq -w "$1" "$2"); do out="$out\"p$i\": $3, "; done; printf '%s' "${out%, }"
}

real_root=$(mkdir -p "$root" && cd "$root" && pwd -P)
# The run root, by both its spellings, before any line is cut short.
redact() { sed -e "s#$real_root#<run>#g" -e "s#$root#<run>#g" | tr -d '\r'; }

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
    "$(wc -c < "$d/out" | tr -d ' ')" "$(redact < "$d/err" | tr '\n' '|' | head -c 300)" \
    "$(touched "$d/before" "$d/after" | redact)"
  [ -s "$d/out" ] && printf '    stdout: %s\n' "$(redact < "$d/out" | tr '\n' '|' | head -c 300)"
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
    "$(redact < "$d/err" 2>/dev/null | tr '\n' '|' | head -c 300)" "$(touched "$d/before" "$d/after" | redact)"
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
  echo "== $f"
  if [ "$product" = fx ]; then
    fresh "$f"; run_cli "permissions" "$H" "$W" "$T" permissions
    fresh "$f"; run_cli "ask" "$H" "$W" "$T" ask hi
  else
    fresh "$f"; run_cli "ask" "$H" "$W" "$T" ask hi
  fi
  fresh "$f"; run_cli "status" "$H" "$W" "$T" status
  if [ "${PROBE_ENTRIES:-}" = all ]; then
    fresh "$f"; run_cli "ask --json" "$H" "$W" "$T" ask --json hi
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
