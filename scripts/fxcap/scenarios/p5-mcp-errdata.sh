# A stdio MCP server for p5-mcp-errdata: one tool, `echo`, whose every call
# is refused with a JSON-RPC error that carries `data`, written as the call's
# `text` argument picks: `compact`, `spaced` (white space and a key order a
# re-serialization would change), `nul` (a NUL in the data). It answers
# `server/discover` with Method not found, so a v0.0.16 client falls back to
# the legacy `initialize`, and speaks `2025-06-18`.
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/^{"jsonrpc":"2.0","id":\([0-9][0-9]*\),.*/\1/p')
  case "$line" in
    *'"method":"initialize"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"errdata","version":"1"}}}\n' "$id" ;;
    *'"method":"tools/list"'*)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Refuses every call with error data","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}\n' "$id" ;;
    *'"method":"tools/call"'*'"text":"compact"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"fixture refuses","data":{"a":1,"b":[1,2]}}}\n' "$id" ;;
    *'"method":"tools/call"'*'"text":"spaced"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"fixture refuses","data": { "b" : [ 2 , 1 ] ,  "a":1.0 } }}\n' "$id" ;;
    *'"method":"tools/call"'*'"text":"nul"'*)
      printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32603,"message":"fixture refuses","data":{"k":"a\\u0000b"}}}\n' "$id" ;;
    *)
      if [ -n "$id" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"
      fi ;;
  esac
done
