#!/bin/bash
# Round 2's oracle pairs: the saved MCP choices (S1) and a settings file that
# is not UTF-8 (N2), each fixture in a scratch HOME on loopback.
#   fx v0.0.16: `mcp list` (which workspace servers it lists, and how) and
#     `status` (its `config <layer>: <cause>` lines on stderr);
#   xfx: `doctor` (its `config` checks) and `ask` (its `config` lines on
#     stderr). xfx has no `mcp` command: its side of S1 is the runtime test
#     `saved_choices_are_read_as_fxs_parse_reads_them`.
# Prints no path: every one is <run>-relative.
#
# usage: pairs-round2.sh <fx|xfx> <binary> <run root, short>
set -u
product=$1 bin=$2 root=$3
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
mkdir -p "$root"
real_root=$(cd "$root" && pwd -P)
redact() { sed -e "s#$real_root#<run>#g" -e "s#$root#<run>#g" | tr -d '\r'; }
rep() { head -c "$1" /dev/zero | tr '\0' "$2"; }

seed() { # fixture home work
  local f=$1 h=$2 w=$3 s=$2/$prof/settings.json key
  mkdir -p "$h/$prof"; chmod 700 "$h/$prof"
  key=$(cd "$w" && pwd -P)
  printf '{"mcpServers": {"s": {"command": "/usr/bin/true"}}}' > "$w/.mcp.json"
  local approve='"enabledMcpjsonServers": ["s"]'
  case $f in
    mcp-approved) printf '{"workspaces": {"%s": {%s}}}' "$key" "$approve" > "$s" ;;
    mcp-approved-big) printf '{"zzz": 1e400, "workspaces": {"%s": {%s}}}' "$key" "$approve" > "$s" ;;
    mcp-approved-deep) printf '{"zzz": %s%s, "workspaces": {"%s": {%s}}}' "$(rep 127 '[')" "$(rep 127 ']')" "$key" "$approve" > "$s" ;;
    mcp-none) printf '{"workspaces": {"%s": {}}}' "$key" > "$s" ;;
    mcp-bad-list) printf '{"workspaces": {"%s": {"enabledMcpjsonServers": "s"}}}' "$key" > "$s" ;;
    mcp-bad-name) printf '{"workspaces": {"%s": {"enabledMcpjsonServers": [""]}}}' "$key" > "$s" ;;
    mcp-bad-switch) printf '{"workspaces": {"%s": {%s, "enableAllProjectMcpServers": "yes"}}}' "$key" "$approve" > "$s" ;;
    utf8-profile) printf '{"zzz": "\377"}' > "$s" ;;
    utf8-project) printf '{"permission_mode": "auto"}' > "$s"; printf '{"zzz": "\377"}' > "$w/$proj" ;;
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

run() { # label filter fixture args...: stdout lines matching filter, and stderr
  local label=$1 filter=$2 f=$3 d=$root/$3 h w t; shift 3
  chmod -R u+rwx "$d" 2>/dev/null; rm -rf "$d"
  h=$d/home w=$d/work t=$d/tmp
  mkdir -p "$h" "$w" "$t"
  if [ "$product" = fx ]; then
    for cue in success error bloom press release toggle; do mkdir -p "$t/fx-$cue.m4a"; done
    mkdir -p "$t/fx-click.caf"
  fi
  seed "$f" "$h" "$w"
  (cd "$w" && env -i $(envs "$h" "$t") perl -e 'alarm 20; exec @ARGV or die' "$bin" "$@" \
     </dev/null >"$d/out" 2>"$d/err"); local code=$?
  printf '  %-7s exit=%s stderr=%s\n' "$label" "$code" "$(redact < "$d/err" | tr '\n' '|' | head -c 240)"
  [ -n "$filter" ] && redact < "$d/out" | grep -E "$filter" | sed 's/^/    | /'
  return 0
}

for f in mcp-approved mcp-approved-big mcp-approved-deep mcp-none mcp-bad-list mcp-bad-name \
  mcp-bad-switch utf8-profile utf8-project; do
  echo "== $f"
  if [ "$product" = fx ]; then
    case $f in mcp-*) run "mcp list" '.' "$f" mcp list ;; esac
    run status 'mcp_servers' "$f" status
  else
    run doctor 'config' "$f" doctor
    run ask '' "$f" ask hi
  fi
done
echo "binary sha256 after: $(sha "$bin" | cut -c1-12)"
