#!/bin/sh
# A stdio MCP server written for xfx's tests of version detection: how each
# kind of server meets fx v0.0.16's `server/discover` probe
# (vercel-labs/fx@481104fd src/mcp/core/era.zig). Newline-delimited JSON-RPC,
# nothing on the network. MCP_DISCOVER_MODE says which server it is:
#   modern      a 2026-07-28 server: discover lists 2026-07-28; `initialize` is
#               -32601; every other request must carry the 2026 `_meta`
#               (else -32602); results say `resultType: "complete"`; a call is
#               preceded by a `ping` of its own, which a 2026 client leaves
#               unanswered
#   silent      never answers discover, then a 2025 server
#   old2024     discover -32601, `initialize` answered with 2024-11-05
#   malformed   discover answered with a reply that is not valid JSON-RPC (its
#               result a string of terminal escapes), then a 2025 server
#   hostile     discover answered with a complete result listing only near
#               misses and terminal-escape instructions: no shared version
#   garbage     discover answered with a raw line of terminal escapes that is
#               not JSON, then a 2025 server
#   idless      discover answered with an error without an id, then a 2025
#               server
#   deep        discover answered with a result nested 4096 levels deep, then
#               a 2025 server
#   rejected    discover answered with -32021: a modern server that refuses
#   legacyonly  discover answered with -32022 listing only 2025-11-25, then a
#               2025 server
#   quiet       discover answered with -32022 listing 2026-07-28, then silence
# A 2025 server answers `initialize` with 2025-06-18 (2024-11-05 in old2024),
# takes notifications, and has one tool `echo`. When MCP_FIXTURE_LOG names a
# file, every frame it reads is appended there.
mode=${MCP_DISCOVER_MODE:-modern}
discovered=0
while IFS= read -r line; do
  if [ -n "${MCP_FIXTURE_LOG:-}" ]; then
    printf '%s\n' "$line" >> "$MCP_FIXTURE_LOG"
  fi
  id=$(printf '%s' "$line" | sed -n 's/^{"jsonrpc":"2.0","id":\([0-9][0-9]*\),.*/\1/p')
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"io.modelcontextprotocol/protocolVersion":"2026-07-28"'*) modern_meta=1 ;;
    *) modern_meta=0 ;;
  esac
  if [ "$method" = "server/discover" ]; then
    discovered=$((discovered + 1))
    case "$mode" in
      modern)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","supportedVersions":["2026-07-28"],"capabilities":{"tools":{}},"instructions":"modern fixture"}}\n' "$id" ;;
      silent) ;;
      malformed)
        printf '{"jsonrpc":"2.0","id":%s,"result":"\\u001b]0;pwned\\u0007\\u001b[2J"}\n' "$id" ;;
      hostile)
        printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","supportedVersions":["\\u001b[31m2026-07-28","2026-07-28\\u0007","2026-07-28 ","2026\\u002d07\\u002d28x"],"capabilities":{"tools":{}},"instructions":"\\u001b]0;pwned\\u0007"}}\n' "$id" ;;
      garbage)
        printf '\033[2J\033]0;pwned\007 not json\n' ;;
      idless)
        printf '{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}\n' ;;
      deep)
        awk -v id="$id" 'BEGIN { s = ""; for (i = 0; i < 4096; i++) s = s "["; for (i = 0; i < 4096; i++) s = s "]"; printf "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"supportedVersions\":%s}}\n", id, s }' ;;
      rejected)
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32021,"message":"\\u001b[31mmissing capability"}}\n' "$id" ;;
      legacyonly)
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32022,"message":"unsupported","data":{"supported":["2025-11-25"]}}}\n' "$id" ;;
      quiet)
        if [ "$discovered" -eq 1 ]; then
          printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32022,"message":"unsupported","data":{"supported":["2026-07-28"]}}}\n' "$id"
        fi ;;
      *)
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id" ;;
    esac
    continue
  fi
  [ -n "$id" ] || continue
  if [ "$mode" = "modern" ]; then
    if [ "$method" = "initialize" ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"
      continue
    fi
    if [ "$modern_meta" -ne 1 ]; then
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32602,"message":"Missing modern request metadata"}}\n' "$id"
      continue
    fi
  fi
  case "$method" in
    initialize)
      version=2025-06-18
      [ "$mode" = "old2024" ] && version=2024-11-05
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"%s","capabilities":{"tools":{}},"serverInfo":{"name":"discover-fixture","version":"1"}}}\n' "$id" "$version"
      ;;
    tools/list)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","tools":[{"name":"echo","description":"Echo text back through the discover fixture","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}}\n' "$id"
      ;;
    tools/call)
      if [ "$mode" = "modern" ]; then
        # A request of its own first, which a 2026 client never answers: an
        # answer would be the next line read, and logged.
        printf '{"jsonrpc":"2.0","id":"s1","method":"ping"}\n'
      fi
      text=$(printf '%s' "$line" | sed -n 's/.*"arguments":{"text":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"resultType":"complete","content":[{"type":"text","text":"DISCOVER_%s:%s"}]}}\n' "$id" "$mode" "$text"
      ;;
    *)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"
      ;;
  esac
done
