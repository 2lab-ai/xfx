// A stdio MCP server for the p5-mcp scenarios: a profile starts it as
// `"command":"@BUN@","args":["@SCENARIOS@/p5-mcp-stdio.ts"]` (capture.sh writes
// both paths in). Written for this harness, as mcphttp.ts is: it answers what the
// research captures were taken against -- the default mode of fx's own e2e
// fixture (vercel-labs/fx@481104fd tests/e2e/fixtures/mcp-legacy-stdio.mjs), an
// old server that never heard of `server/discover`, at the oldest version fx
// speaks -- and nothing else of it.
//
// One JSON-RPC message per line on stdin, one answer per line on stdout:
//   server/discover            error -32601 `Method not found` (fx then sends
//                              `initialize`)
//   initialize                 protocol 2025-03-26, tools, serverInfo
//                              `legacy-fixture` 1.0.0; exits 2 unless it is the
//                              first message, or the second after a discover
//   notifications/initialized  nothing
//   tools/list                 one tool, `echo`
//   tools/call                 `LEGACY_MCP_TOOL_RESULT:<text>`; exits 4 unless the
//                              call carries an integer `_meta.progressToken`
//   resources/list, resources/read, prompts/list, prompts/get
//                              one resource `legacy-alpha`, one prompt `review`
//   any other method           error -32601 `Method not found`
// The two exits are the fixture's checks of what the client sent: a server that
// dies there is a client that broke the protocol the way the check names.
//
// MCPSTDIO_WIRE_LOG, when set, names a file each received message is appended to
// as `{"sequence":<n>,"message":<it>}`, n from 1 -- no pid and no time, so two
// runs of one scenario log the same bytes.
import { appendFileSync } from "node:fs";

const wireLog = process.env.MCPSTDIO_WIRE_LOG;
let count = 0;
let sawDiscover = false;
let pending = Buffer.alloc(0);

const send = (message: object) => process.stdout.write(`${JSON.stringify(message)}\n`);
const answer = (id: unknown, result: object) => send({ jsonrpc: "2.0", id, result });
const unknown = (id: unknown) => send({ jsonrpc: "2.0", id, error: { code: -32601, message: "Method not found" } });

function handle(message: any) {
  count += 1;
  if (wireLog) appendFileSync(wireLog, `${JSON.stringify({ sequence: count, message })}\n`);
  const id = message.id;
  switch (message.method) {
    case "server/discover":
      sawDiscover = true;
      return unknown(id);
    case "initialize":
      if (count !== 1 && !(sawDiscover && count === 2)) process.exit(2);
      return answer(id, {
        protocolVersion: "2025-03-26",
        capabilities: { tools: {} },
        serverInfo: { name: "legacy-fixture", version: "1.0.0" },
      });
    case "notifications/initialized":
      return;
    case "tools/list":
      return answer(id, {
        tools: [{
          name: "echo",
          description: "Echo text through the legacy MCP fixture",
          inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
        }],
      });
    case "tools/call":
      if (!Number.isInteger(message.params?._meta?.progressToken)) process.exit(4);
      return answer(id, { content: [{ type: "text", text: `LEGACY_MCP_TOOL_RESULT:${message.params?.arguments?.text ?? ""}` }] });
    case "resources/list":
      return answer(id, { resources: [{ uri: "legacy://alpha", name: "legacy-alpha", mimeType: "text/plain" }] });
    case "resources/read":
      return answer(id, { contents: [{ uri: message.params?.uri, mimeType: "text/plain", text: "LEGACY_STDIO_RESOURCE_TEXT" }] });
    case "prompts/list":
      return answer(id, { prompts: [{ name: "review", arguments: [{ name: "tone", required: true }] }] });
    case "prompts/get":
      return answer(id, {
        description: "Legacy stdio review prompt",
        messages: [{ role: "user", content: { type: "text", text: "LEGACY_STDIO_PROMPT_TEXT" } }],
      });
    default:
      return unknown(id);
  }
}

process.stdin.on("data", (chunk: Buffer) => {
  pending = Buffer.concat([pending, chunk]);
  for (let end = pending.indexOf("\n"); end >= 0; end = pending.indexOf("\n")) {
    const line = pending.subarray(0, end).toString("utf8").replace(/\r+$/, "");
    pending = pending.subarray(end + 1);
    if (line.length > 0) handle(JSON.parse(line));
  }
});
