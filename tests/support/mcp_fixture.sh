#!/bin/sh
# A stdio MCP server written for xfx's tests: newline-delimited JSON-RPC on
# standard input and output, one answer per request, nothing on the network.
# It speaks the legacy handshake (`initialize`, then `notifications/initialized`)
# and has one tool, one resource and one prompt. When MCP_FIXTURE_LOG names a
# file, every frame it reads is appended there, so a test can read the wire.
while IFS= read -r line; do
  if [ -n "${MCP_FIXTURE_LOG:-}" ]; then
    printf '%s\n' "$line" >> "$MCP_FIXTURE_LOG"
  fi
  id=$(printf '%s' "$line" | sed -n 's/^{"jsonrpc":"2.0","id":\([0-9][0-9]*\),.*/\1/p')
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  case "$method" in
    initialize)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{},"resources":{},"prompts":{},"completions":{}},"serverInfo":{"name":"xfx-test-fixture","version":"1"}}}\n' "$id"
      ;;
    notifications/*) ;;
    tools/list)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"echo","description":"Echo text back through the test fixture","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}}]}}\n' "$id"
      ;;
    tools/call)
      text=$(printf '%s' "$line" | sed -n 's/.*"arguments":{"text":"\([^"]*\)".*/\1/p')
      printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"FIXTURE_ECHO:%s"}]}}\n' "$id" "$text"
      ;;
    resources/list)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"resources":[{"uri":"fixture://alpha","name":"alpha","mimeType":"text/plain"}]}}\n' "$id"
      ;;
    resources/templates/list)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"resourceTemplates":[]}}\n' "$id"
      ;;
    resources/read)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"contents":[{"uri":"fixture://alpha","mimeType":"text/plain","text":"FIXTURE_RESOURCE_TEXT"}]}}\n' "$id"
      ;;
    prompts/list)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"prompts":[{"name":"review","description":"Review prompt","arguments":[{"name":"tone","required":true}]}]}}\n' "$id"
      ;;
    prompts/get)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"description":"Review prompt","messages":[{"role":"user","content":{"type":"text","text":"FIXTURE_PROMPT_TEXT"}}]}}\n' "$id"
      ;;
    completion/complete)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"completion":{"values":["brief","thorough"],"total":2,"hasMore":false}}}\n' "$id"
      ;;
    *)
      if [ -n "$id" ]; then
        printf '{"jsonrpc":"2.0","id":%s,"error":{"code":-32601,"message":"Method not found"}}\n' "$id"
      fi
      ;;
  esac
done
