#!/bin/bash
# PROFILE-REFUSE-TYPED-KEYS's probe (its receipt's probe.sh: the same
# environment, the same fresh run per entry, the same 20 s bound), narrowed to
# the mode each run applies. For every fixture -- a spelling of a mode at one
# surface -- each entry prints its exit status and the mode it reports, whole
# (TYPED-KEYS's cut stdout at 300 bytes, before `[status] permission_mode=`).
# Prints no path: every one is <run>-relative.
#
# usage: probe.sh <fx|xfx> <binary> <run root, short> [fixture...]
#
# Fixtures, <surface>-<word>:
#   key-*    the profile's `permission_mode`
#   entry-*  this workspace's entry, over a global mode that is not the word's
#   env-*    FX_PERMISSION_MODE (fx) or XFX_PERMISSION_MODE (xfx), over a
#            profile's mode that is not the word's
#   flag-*   `ask <flag> hi` with no profile; the mode it ran in is the
#            warning full access prints on the way in, or the parser's refusal
# A word that does not read leaves what was below it: the default `auto` for
# key-*, the base mode for entry-* and env-*.
set -u
product=$1 bin=$2 root=$3
shift 3
case $product in
  fx) prof=.fx var=FX_PERMISSION_MODE ;;
  xfx) prof=.xfx var=XFX_PERMISSION_MODE ;;
  *) echo "product must be fx or xfx" >&2; exit 2 ;;
esac
FX_SHA=8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
if [ "$product" = fx ] && [ "$(sha "$bin")" != "$FX_SHA" ]; then
  echo "refusing: the fx binary is not the v0.0.16 oracle" >&2; exit 2
fi
echo "binary sha256 before: $(sha "$bin" | cut -c1-12)"

words="upper-ask cap-auto upper-full-access full-access-space upper-yolo lower-yolo invalid padded full"
flags="full-access yolo upper-full-access cap-yolo auto upper-auto full-access-then-yolo full-access-then-auto"
all=""
for s in key entry env; do for w in $words; do all="$all $s-$w"; done; done
for f in $flags; do all="$all flag-$f"; done
fixtures=${*:-$all}

word() { # word id -> the spelling
  case $1 in
    upper-ask) echo "ASK" ;;
    cap-auto) echo "Auto" ;;
    upper-full-access) echo "FULL-ACCESS" ;;
    full-access-space) echo "Full Access" ;;
    upper-yolo) echo "YOLO" ;;
    lower-yolo) echo "yolo" ;;
    invalid) echo "sometimes" ;;
    padded) echo " ask" ;;
    full) echo "full" ;;
    *) return 1 ;;
  esac
}
# The mode below an entry or a variable: one the word does not mean.
below() { case $1 in upper-ask|padded) echo auto ;; *) echo ask ;; esac; }
flag_args() {
  case $1 in
    full-access) echo "--full-access" ;;
    yolo) echo "--yolo" ;;
    upper-full-access) echo "--FULL-ACCESS" ;;
    cap-yolo) echo "--Yolo" ;;
    auto) echo "--auto" ;;
    upper-auto) echo "--AUTO" ;;
    full-access-then-yolo) echo "--full-access --yolo" ;;
    full-access-then-auto) echo "--full-access --auto" ;;
    *) return 1 ;;
  esac
}

envs() { # home tmp
  local common="HOME=$1 TERM=xterm-256color PATH=/usr/bin:/bin LANG=en_US.UTF-8 TMPDIR=$2 AI_GATEWAY_API_KEY="
  if [ "$product" = fx ]; then
    echo "$common FX_NO_OPEN_BROWSER=1 FX_DISABLE_KEYCHAIN=1 FX_SKIP_ONBOARDING=1 FX_SOUND=0 FX_GATEWAY_BASE_URL=http://127.0.0.1:9 FX_AUTO_UPGRADE=0"
  else
    echo "$common XFX_GATEWAY_URL=http://127.0.0.1:9/v1/chat/completions XFX_SOUND=0"
  fi
}

real_root=$(mkdir -p "$root" && cd "$root" && pwd -P)
redact() { sed -e "s#$real_root#<run>#g" -e "s#$root#<run>#g" | tr -d '\r'; }

fresh() { # fixture -> sets H W T, the profile laid
  local d=$root/$1 s w b
  chmod -R u+rwx "$d" 2>/dev/null; rm -rf "$d"
  [ ! -e "$d" ] || { echo "cannot clear $1's run" >&2; exit 2; }
  H=$d/home W=$d/work T=$d/tmp
  mkdir -p "$H/$prof" "$W" "$T"; chmod 700 "$H/$prof"
  if [ "$product" = fx ]; then
    for cue in success error bloom press release toggle; do mkdir -p "$T/fx-$cue.m4a"; done
    mkdir -p "$T/fx-click.caf"
  fi
  s=$H/$prof/settings.json
  case $1 in
    key-*) w=$(word "${1#key-}") || return 1
           printf '{"permission_mode": "%s"}' "$w" > "$s" ;;
    entry-*) w=$(word "${1#entry-}") || return 1; b=$(below "${1#entry-}")
             printf '{"permission_mode": "%s", "workspaces": {"%s": {"permission_mode": "%s"}}}' \
               "$b" "$(cd "$W" && pwd -P)" "$w" > "$s" ;;
    env-*) w=$(word "${1#env-}") || return 1; b=$(below "${1#env-}")
           printf '{"permission_mode": "%s"}' "$b" > "$s"
           EXTRA="$var=$w" ;;
    flag-*) rmdir "$H/$prof" ;;
    *) echo "unknown fixture $1" >&2; return 1 ;;
  esac
  [ -e "$s" ] && chmod 600 "$s"
  return 0
}

# One entry: stdin empty, a 20 s bound; the mode lines it printed, whole.
run() { # label args...
  local label=$1 d=$root/$f code; shift
  if [ -n "${EXTRA:-}" ]; then
    (cd "$W" && env -i $(envs "$H" "$T") "$EXTRA" perl -e 'alarm 20; exec @ARGV or die' "$bin" "$@" \
       </dev/null >"$d/out" 2>"$d/err"); code=$?
  else
    (cd "$W" && env -i $(envs "$H" "$T") perl -e 'alarm 20; exec @ARGV or die' "$bin" "$@" \
       </dev/null >"$d/out" 2>"$d/err"); code=$?
  fi
  local modes
  modes=$(grep -E 'permission_mode[=":]|^\[permissions\] mode=' "$d/out" \
    | sed -E -e 's/.*"permission_mode":"([^"]*)".*/json=\1/' \
          -e 's/^\[(status|doctor)\] permission_mode=/\1=/' \
          -e 's/^\[permissions\] mode=/permissions=/' \
          -e 's/^\[ok\] startup: .*permission_mode=([^,]*),.*/startup=\1/' \
    | tr '\n' ';')
  printf '  %-14s exit=%s %s| stderr=%s\n' "$label" "$code" "$modes" \
    "$(redact < "$d/err" | head -1 | cut -c1-120)"
}

for f in $fixtures; do
  echo "== $f"
  EXTRA=""
  case $f in
    flag-*)
      a=$(flag_args "${f#flag-}") || { echo "unknown fixture $f" >&2; exit 2; }
      fresh "$f" || exit 2; run "ask $a" ask $a hi ;;
    *)
      if [ "$product" = fx ]; then fresh "$f" || exit 2; run "permissions" permissions; fi
      fresh "$f" || exit 2; run "status" status
      fresh "$f" || exit 2; run "status --json" status --json
      fresh "$f" || exit 2; run "doctor" doctor
      fresh "$f" || exit 2; run "ask" ask hi ;;
  esac
done
echo "binary sha256 after: $(sha "$bin" | cut -c1-12)"
