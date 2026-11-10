#!/bin/bash
# The lines that decide `connecting` against `retrying`, on both sides.
# usage: code.sh <xfx checkout> <fx source checkout>
set -u
x=$1 g=$2
show() { echo "== xfx $1:$2-$3"; sed -n "$2,$3p" "$x/$1" | nl -ba -v "$2"; }
fxs() { echo "== fx $1:$2-$3@481104fd"; git -C "$g" show "481104fd:$1" | sed -n "$2,$3p" | nl -ba -v "$2"; }
show src/mcp/runtime.rs 1324 1355
show src/mcp/runtime.rs 88 106
show src/mcp/era.rs 60 68
show src/mcp/runtime.rs 1586 1642
fxs src/mcp/core/era.zig 19 26
fxs src/mcp/core/request.zig 76 79
fxs src/mcp/core/engine.zig 193 211
fxs src/mcp/core/http_modern.zig 21 24
show src/mcp/config.rs 453 461
fxs src/core/mcp/project_config.zig 1050 1054
fxs src/core/mcp/project_config.zig 1130 1140
echo "== train 12 (df46777e..HEAD) and xfx's MCP code: git diff --stat -- src/mcp"
git -C "$x" diff --stat df46777e HEAD -- src/mcp
echo "(empty above: no change)"
echo "== train 12 and capture.sh: the per-snap publish LOCAL-PATHS added (55d993a7^1 has none)"
git -C "$x" show 55d993a7^1:scripts/fxcap/capture.sh | grep -c 'publish ' | sed 's/^/publish calls at 55d993a7^1: /'
grep -c 'publish ' "$x/scripts/fxcap/capture.sh" | sed 's/^/publish calls now: /'
grep -n '^publish()' "$x/scripts/fxcap/capture.sh"
