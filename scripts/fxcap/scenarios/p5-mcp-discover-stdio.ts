// A stdio MCP server for the p5-mcp-discover scenarios: how each kind of server meets fx
// v0.0.16's version detection (vercel-labs/fx@481104fd src/mcp/core/era.zig), which asks
// `server/discover` before anything else. A profile starts it as
// `"command":"@BUN@","args":["@SCENARIOS@/p5-mcp-discover-stdio.ts","<mode>"]` (capture.sh
// writes both paths in). Written for this harness, as p5-mcp-stdio.ts is.
//
// One JSON-RPC message per line on stdin, one answer per line on stdout. The mode is the
// first argument:
//   modern     a 2026-07-28 server: `server/discover` gets `supportedVersions:["2026-07-28"]`
//              (the shape of fx's own fixture, tests/e2e/fixtures/mcp-modern-stdio.mjs
//              @481104fd); `initialize` is a method it does not have (-32601); every other
//              request must carry the 2026 `_meta` -- an integer `progressToken`, the
//              protocol version, client capabilities and info -- or it gets -32602
//              `Missing modern request metadata`; results say `resultType: "complete"`
//   silent     never answers `server/discover` (no line at all), then a 2025 server:
//              `initialize` 2025-06-18
//   old2024    `server/discover` -32601, then `initialize` answered with 2024-11-05, a
//              version fx v0.0.16 no longer speaks
//   malformed  answers `server/discover` with a reply that is not valid JSON-RPC -- its
//              result a string full of terminal escapes -- then a 2025 server
//   hostile    answers `server/discover` with a complete result whose version list holds
//              only near misses (escapes around "2026-07-28", a 64 KiB string) and whose
//              instructions are terminal escapes: it lists no version a client speaks
// In the 2025 modes `initialize` is answered at 2025-06-18 (2024-11-05 in old2024),
// `notifications/initialized` with nothing, `tools/list` with one tool `echo`, and
// `tools/call` with `DISCOVER_<MODE>_RESULT:<text>`. Any other method: -32601.
//
// MCPSTDIO_WIRE_LOG, when set, names a file each received message is appended to as
// `{"sequence":<n>,"message":<it>}`, n from 1 -- no pid and no time, so two runs of one
// scenario log the same bytes.
import { appendFileSync } from "node:fs";

const mode = process.argv[2] ?? "";
if (!["modern", "silent", "old2024", "malformed", "hostile"].includes(mode)) {
  console.error(`p5-mcp-discover-stdio: unknown mode ${JSON.stringify(mode)}`);
  process.exit(2);
}
const wireLog = process.env.MCPSTDIO_WIRE_LOG;
const resultTag = `DISCOVER_${mode.toUpperCase()}_RESULT`;
let count = 0;
let pending = Buffer.alloc(0);

const send = (message: object) => process.stdout.write(`${JSON.stringify(message)}\n`);
const answer = (id: unknown, result: object) =>
  send({ jsonrpc: "2.0", id, result: mode === "modern" ? { resultType: "complete", ...result } : result });
const failure = (id: unknown, code: number, message: string) => send({ jsonrpc: "2.0", id, error: { code, message } });

const echo = {
  name: "echo",
  description: `Echo text through the ${mode} discover fixture`,
  inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
};

function modernMeta(message: any): boolean {
  const meta = message.params?._meta;
  return Number.isInteger(meta?.progressToken) &&
    meta?.["io.modelcontextprotocol/protocolVersion"] === "2026-07-28" &&
    typeof meta?.["io.modelcontextprotocol/clientCapabilities"] === "object" &&
    typeof meta?.["io.modelcontextprotocol/clientInfo"]?.name === "string";
}

function discover(id: unknown) {
  switch (mode) {
    case "modern":
      return answer(id, {
        supportedVersions: ["2026-07-28"],
        capabilities: { tools: {} },
        _meta: { "io.modelcontextprotocol/serverInfo": { name: "discover-modern-fixture" } },
      });
    case "silent":
      return;
    case "malformed":
      return send({ jsonrpc: "2.0", id, result: "\u001b]0;pwned\u0007\u001b[2J\u001b[31mDISCOVER" });
    case "hostile":
      return send({
        jsonrpc: "2.0",
        id,
        result: {
          resultType: "complete",
          supportedVersions: ["\u001b[31m2026-07-28", "2026-07-28\u0007", "2026-07-28 ", "x".repeat(65536)],
          capabilities: { tools: {} },
          instructions: "\u001b]0;pwned\u0007\u001b[2J",
        },
      });
    default:
      return failure(id, -32601, "Method not found");
  }
}

function handle(message: any) {
  count += 1;
  if (wireLog) appendFileSync(wireLog, `${JSON.stringify({ sequence: count, message })}\n`);
  const id = message.id;
  if (message.method === "server/discover") return discover(id);
  if (id === undefined) return;
  if (mode === "modern") {
    if (message.method === "initialize") return failure(id, -32601, "Method not found");
    if (!modernMeta(message)) return failure(id, -32602, "Missing modern request metadata");
  }
  switch (message.method) {
    case "initialize":
      return answer(id, {
        protocolVersion: mode === "old2024" ? "2024-11-05" : "2025-06-18",
        capabilities: { tools: {} },
        serverInfo: { name: `discover-${mode}-fixture`, version: "1.0.0" },
      });
    case "tools/list":
      return answer(id, { tools: [echo] });
    case "tools/call":
      return answer(id, { content: [{ type: "text", text: `${resultTag}:${message.params?.arguments?.text ?? ""}` }] });
    default:
      return failure(id, -32601, "Method not found");
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
