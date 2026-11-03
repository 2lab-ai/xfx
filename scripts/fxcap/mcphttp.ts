// A loopback MCP server over streamable HTTP, for capture.sh's `mcphttp`: one server per run, on
// a port the OS picks on 127.0.0.1 and nowhere else, that is ready, hangs or fails on cue.
// Usage: bun mcphttp.ts <port> <mode-file>
//
// The mode is the first word of <mode-file>, read again for every request, so capture.sh changes
// it mid-run by rewriting that file:
//   ready     answers as the legacy streamable HTTP server of fx's own e2e fixture does
//             (vercel-labs/fx@481104fd tests/e2e/fixtures/mcp-legacy-remote.ts): POST
//             `server/discover` 404, so fx falls back to `initialize`, which is answered with the
//             version asked for when it is one of the three legacy HTTP versions (2025-11-25
//             otherwise) and an `MCP-Session-Id`; notifications 202; `tools/list` one tool,
//             `echo`; `tools/call` its text back as `MCPHTTP_ECHO: <text>`; `resources/list`,
//             `resources/templates/list` and `prompts/list` empty; `ping` an empty result; any
//             other method `Method not found`. GET (a listen stream) 405, DELETE (the session's
//             end) 204.
//   hang      holds every request open, answering nothing, until the mode is something else --
//             then answers it as that mode would have: a server that is slow to come up, or
//             comes up on cue (`hang`, then `ready`)
//   fail      every request 500 `fixture failing`
//   rpcerror  every request with an id gets a JSON-RPC error, -32603 `fixture refuses`
//
// Every request is one line on stdout, once answered: the time in ms since the server started,
// the HTTP method and path, the JSON-RPC method and id, whether it carried a session id and which
// protocol version header, the mode it was answered in, and the status. Header values other than
// the version are never written.
const port = Number(process.argv[2] ?? 0);
const modeFile = process.argv[3];
if (!modeFile) {
  console.error("usage: bun mcphttp.ts <port> <mode-file>");
  process.exit(2);
}
const started = Date.now();
const log = (m: string) => console.log(`[mcphttp] ${m}`);
const modes = new Set(["ready", "hang", "fail", "rpcerror"]);
const versions = ["2025-11-25", "2025-06-18", "2025-03-26"];
const session = "fxcap-session-1";

async function mode(): Promise<string> {
  const word = (await Bun.file(modeFile).text()).trim().split(/\s+/)[0] ?? "";
  return modes.has(word) ? word : `unknown(${word})`;
}

const json = (frame: object, headers: Record<string, string> = {}) =>
  Response.json(frame, { headers: { ...headers, "content-type": "application/json" } });
const result = (id: unknown, value: object) => json({ jsonrpc: "2.0", id, result: value });

function ready(httpMethod: string, message: any): Response {
  if (httpMethod === "GET") return new Response("Method Not Allowed", { status: 405, headers: { Allow: "POST, DELETE" } });
  if (httpMethod === "DELETE") return new Response(null, { status: 204 });
  const id = message?.id;
  const method = message?.method;
  if (method === "server/discover") return new Response(null, { status: 404 });
  if (id === undefined) return new Response(null, { status: 202 });
  switch (method) {
    case "initialize": {
      const asked = message.params?.protocolVersion;
      return json(
        {
          jsonrpc: "2.0",
          id,
          result: {
            protocolVersion: versions.includes(asked) ? asked : versions[0],
            capabilities: { tools: {} },
            serverInfo: { name: "fxcap-mcphttp", version: "1" },
          },
        },
        { "MCP-Session-Id": session },
      );
    }
    case "tools/list":
      return result(id, {
        tools: [{
          name: "echo",
          description: "Echo the text back (fxcap HTTP fixture)",
          inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
        }],
      });
    case "tools/call":
      return result(id, { content: [{ type: "text", text: `MCPHTTP_ECHO: ${message.params?.arguments?.text ?? ""}` }], isError: false });
    case "resources/list":
      return result(id, { resources: [] });
    case "resources/templates/list":
      return result(id, { resourceTemplates: [] });
    case "prompts/list":
      return result(id, { prompts: [] });
    case "ping":
      return result(id, {});
    default:
      return json({ jsonrpc: "2.0", id, error: { code: -32601, message: "Method not found" } });
  }
}

function answerIn(now: string, httpMethod: string, message: any): Response {
  if (now === "ready") return ready(httpMethod, message);
  if (now === "rpcerror" && message?.id !== undefined) {
    return json({ jsonrpc: "2.0", id: message.id, error: { code: -32603, message: "fixture refuses" } });
  }
  if (now === "rpcerror") return new Response(null, { status: 202 });
  return new Response("fixture failing", { status: 500 });
}

const server = Bun.serve({
  port,
  hostname: "127.0.0.1",
  idleTimeout: 0,
  async fetch(req) {
    const asked = Date.now();
    const url = new URL(req.url);
    const text = req.method === "POST" ? await req.text() : "";
    let message: any = null;
    try {
      message = text ? JSON.parse(text) : null;
    } catch {
      message = null;
    }
    let now = await mode();
    while (now === "hang") {
      await Bun.sleep(50);
      now = await mode();
    }
    const res = answerIn(now, req.method, message);
    const rpc = message ? `${message.method ?? (message.result !== undefined ? "(result)" : "(error)")} id=${message.id ?? "-"}` : "-";
    log(
      `+${asked - started}ms ${req.method} ${url.pathname} rpc=${rpc} session=${req.headers.has("mcp-session-id") ? "yes" : "no"} version=${req.headers.get("mcp-protocol-version") ?? "-"} mode=${now} -> ${res.status} +${Date.now() - started}ms`,
    );
    return res;
  },
});
log(`listening ${server.port}`);
