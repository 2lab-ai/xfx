// A loopback MCP server over streamable HTTP, for capture.sh's `mcphttp`: one server per run, on
// a port the OS picks on 127.0.0.1 and nowhere else, that is ready, hangs or fails on cue.
// Usage: bun mcphttp.ts <port> <mode-file>
//
// The mode is the first word of <mode-file>, read again for every request, so capture.sh changes
// it mid-run by renaming a new file into place (a file read empty or half written is read again:
// `mode` below):
//   ready     answers as the legacy streamable HTTP server of fx's own e2e fixture does
//             (vercel-labs/fx@481104fd tests/e2e/fixtures/mcp-legacy-remote.ts): POST
//             `server/discover` 404, so fx falls back to `initialize`, which is answered with the
//             version asked for when it is one of the three legacy HTTP versions (2025-11-25
//             otherwise) and an `MCP-Session-Id`; notifications 202; `tools/list` one tool,
//             `echo`; `tools/call` its text back as `MCPHTTP_ECHO: <text>`; `resources/list`,
//             `resources/templates/list` and `prompts/list` empty; `ping` an empty result; any
//             other method `Method not found`. GET (a listen stream) 405, DELETE (the session's
//             end) 204.
//   methodnotfound
//             `ready`, except that `server/discover` gets HTTP 200 and the JSON-RPC error
//             -32601 `Method not found` rather than a bare 404: the server the MCP research asks
//             for (.prd/tui-phase5/research/mcp.md, Harness needs 1). fx v0.0.16 reads any error
//             but three modern ones as a legacy answer and goes on to `initialize`
//             (vercel-labs/fx@481104fd src/mcp/core/era.zig:387-402); the bare 404 is a legacy
//             answer too, by another path (`classifyDiscoverHttp`, :408-413), and a 404 *with*
//             -32601 would be a modern server without discovery, which this fixture never is.
//   hang      holds every request open, answering nothing, until the mode is something else --
//             then answers it as that mode would have: a server that is slow to come up, or
//             comes up on cue (`hang`, then `ready`)
//   fail      every request 500 `fixture failing`
//   rpcerror  every request with an id gets a JSON-RPC error, -32603 `fixture refuses`
// The p5-mcp-discover modes, how each kind of server meets fx v0.0.16's version detection
// (vercel-labs/fx@481104fd src/mcp/core/era.zig), which POSTs `server/discover` first:
//   modern    a 2026-07-28 server: `server/discover` gets a complete result listing
//             `2026-07-28` (fx's own tests/e2e/fixtures/mcp-modern-http.ts shape); no session
//             and no `initialize` (-32601); every other request must carry the 2026 `_meta`
//             (an integer progressToken, the version, client capabilities and info) and the
//             `MCP-Protocol-Version: 2026-07-28` header, or it gets -32602; results say
//             `resultType: "complete"`
//   notimpl   `modern`, except that `server/discover` gets HTTP 404 with the JSON-RPC error
//             -32601: a 2026 server without discovery, which fx serves at the probe's version
//             (`classifyDiscoverHttp`, era.zig:411-415)
//   old2024   `ready`, except that `initialize` is answered with 2024-11-05, a version fx
//             v0.0.16 no longer speaks
//   malformed `ready`, except that `server/discover` gets HTTP 200 and a body that is not valid
//             JSON-RPC: its result a string of terminal escapes
//   hostile   `ready`, except that `server/discover` gets a complete result listing only near
//             misses (escapes around "2026-07-28", a 64 KiB string) and instructions that are
//             terminal escapes
//   ssehold   `modern`, except that `server/discover` is answered on an event stream that stays
//             open after its one event: fx reads each event as it comes (`readEvents`,
//             src/mcp/io/http.zig:643-692@481104fd) and is ready at once
//   sseidless `ready`, except that `server/discover` is answered on an event stream with an
//             error that has no id, which answers the POST it came on (src/mcp/server.zig:
//             1155-1158@481104fd): a legacy answer, so `initialize` follows at once
//   ssestrid  `modern`, except that the discover answer comes on an event stream and names
//             its id as a string of digits, which fx reads as its own (`ownId`, wire.zig:305-313)
//   ssefail   `modern`, except that the discover answer comes on an event stream under HTTP
//             500: fx reads an event stream's answers whatever the status (`readEvents`,
//             src/mcp/io/http.zig:643-692@481104fd), so it is ready at 2026-07-28
//
// Every request is one line on stdout, once answered: the time in ms since the server started,
// the HTTP method and path, the JSON-RPC method and id, whether it carried a session id, which
// protocol version header, which `Mcp-Method` and `Mcp-Name` headers, the mode it was answered
// in, and the status -- and, after an `unknown(...)` the mode file could not be read for, the
// read's error code (`mode-file=ENOENT`). Header values other than those three are never written.
const port = Number(process.argv[2] ?? 0);
const modeFile = process.argv[3];
if (!modeFile) {
  console.error("usage: bun mcphttp.ts <port> <mode-file>");
  process.exit(2);
}
const started = Date.now();
const log = (m: string) => console.log(`[mcphttp] ${m}`);
const modes = new Set(["ready", "methodnotfound", "hang", "fail", "rpcerror", "modern", "notimpl", "old2024", "malformed", "hostile", "ssehold", "sseidless", "ssestrid", "ssefail"]);
const versions = ["2025-11-25", "2025-06-18", "2025-03-26"];
const session = "fxcap-session-1";

// The mode file's first word. A read that finds no mode -- the file empty or half written by a
// writer that truncates and then writes, or gone for a moment -- is read again every 10 ms, for up
// to MODE_SETTLE_MS, before a word that is no mode is answered as `unknown(<word>)`: a held
// request polls this, and one empty read used to end its hold with a 500 (FLAKE-MCPMODE,
// registry #19). capture.sh's `mcphttp` verb renames the file into place, so it never takes a
// retry; a writer by hand may. A read that fails is an empty word too; its error's code is kept
// and, when the deadline answers `unknown(...)`, given beside it for the log line (`readError`).
const MODE_SETTLE_MS = 2000;
async function mode(): Promise<{ now: string; readError: string }> {
  const until = Date.now() + MODE_SETTLE_MS;
  let readError = "";
  for (;;) {
    const text = await Bun.file(modeFile).text().then(
      (t) => ((readError = ""), t),
      (e) => ((readError = String(e?.code ?? e?.name ?? "error")), ""),
    );
    const word = text.trim().split(/\s+/)[0] ?? "";
    if (modes.has(word)) return { now: word, readError: "" };
    if (Date.now() >= until) return { now: `unknown(${word})`, readError };
    await Bun.sleep(10);
  }
}

const json = (frame: object, headers: Record<string, string> = {}) =>
  Response.json(frame, { headers: { ...headers, "content-type": "application/json" } });
const result = (id: unknown, value: object) => json({ jsonrpc: "2.0", id, result: value });

function ready(httpMethod: string, message: any, discover = "404", asked2024 = false): Response {
  if (httpMethod === "GET") return new Response("Method Not Allowed", { status: 405, headers: { Allow: "POST, DELETE" } });
  if (httpMethod === "DELETE") return new Response(null, { status: 204 });
  const id = message?.id;
  const method = message?.method;
  if (method === "server/discover" && discover === "rpc") {
    return json({ jsonrpc: "2.0", id, error: { code: -32601, message: "Method not found" } });
  }
  if (method === "server/discover" && discover === "malformed") {
    return json({ jsonrpc: "2.0", id, result: "\u001b]0;pwned\u0007\u001b[2J\u001b[31mDISCOVER" });
  }
  if (method === "server/discover" && discover === "hostile") {
    return result(id, {
      resultType: "complete",
      supportedVersions: ["\u001b[31m2026-07-28", "2026-07-28\u0007", "2026-07-28 ", "x".repeat(65536)],
      capabilities: { tools: {} },
      instructions: "\u001b]0;pwned\u0007\u001b[2J",
    });
  }
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
            protocolVersion: asked2024 ? "2024-11-05" : versions.includes(asked) ? asked : versions[0],
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

// A 2026-07-28 server (`modern`), or one without discovery (`notimpl`): no session, every
// request checked for the 2026 `_meta` and version header.
function modern(httpMethod: string, message: any, version: string | null, notimpl: boolean): Response {
  if (httpMethod !== "POST") return new Response("Method Not Allowed", { status: 405, headers: { Allow: "POST" } });
  const id = message?.id;
  const method = message?.method;
  const fail = (status: number, code: number, text: string) =>
    Response.json({ jsonrpc: "2.0", id, error: { code, message: text } }, { status });
  if (method === "server/discover") {
    if (notimpl) return fail(404, -32601, "Method not found");
    return result(id, {
      resultType: "complete",
      supportedVersions: ["2026-07-28"],
      capabilities: { tools: {} },
      _meta: { "io.modelcontextprotocol/serverInfo": { name: "fxcap-mcphttp-modern" } },
    });
  }
  if (id === undefined) return new Response(null, { status: 202 });
  if (method === "initialize") return fail(200, -32601, "Method not found");
  const meta = message.params?._meta;
  const modernMeta = Number.isInteger(meta?.progressToken) &&
    meta?.["io.modelcontextprotocol/protocolVersion"] === "2026-07-28" &&
    typeof meta?.["io.modelcontextprotocol/clientCapabilities"] === "object" &&
    typeof meta?.["io.modelcontextprotocol/clientInfo"]?.name === "string";
  if (!modernMeta || version !== "2026-07-28") return fail(400, -32602, "Missing modern request metadata");
  switch (method) {
    case "tools/list":
      return result(id, {
        resultType: "complete",
        tools: [{
          name: "echo",
          description: "Echo the text back (fxcap HTTP fixture, 2026)",
          inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
        }],
      });
    case "tools/call":
      return result(id, { resultType: "complete", content: [{ type: "text", text: `MCPHTTP_MODERN_ECHO: ${message.params?.arguments?.text ?? ""}` }], isError: false });
    case "ping":
      return result(id, { resultType: "complete" });
    default:
      return fail(200, -32601, "Method not found");
  }
}

const encoder = new TextEncoder();
const event = (frame: object) => encoder.encode(`event: message\ndata: ${JSON.stringify(frame)}\n\n`);
const discoverResult = (id: unknown) => ({
  jsonrpc: "2.0",
  id,
  result: {
    resultType: "complete",
    supportedVersions: ["2026-07-28"],
    capabilities: { tools: {} },
    _meta: { "io.modelcontextprotocol/serverInfo": { name: "fxcap-mcphttp-modern" } },
  },
});
const sse = (stream: ReadableStream, status = 200) =>
  new Response(stream, { status, headers: { "content-type": "text/event-stream" } });

// The SSE discover modes: the answer as an event, the stream held open (`ssehold`) or ended.
const sseModes = ["ssehold", "sseidless", "ssestrid", "ssefail"];
function sseDiscover(now: string, message: any): Response | null {
  if (!sseModes.includes(now) || message?.method !== "server/discover") return null;
  if (now === "ssehold") {
    return sse(new ReadableStream({ start(c) { c.enqueue(event(discoverResult(message.id))); } }));
  }
  const frame = now === "sseidless"
    ? { jsonrpc: "2.0", id: null, error: { code: -32601, message: "Method not found" } }
    : discoverResult(now === "ssestrid" ? String(message.id) : message.id);
  return sse(new ReadableStream({ start(c) { c.enqueue(event(frame)); c.close(); } }), now === "ssefail" ? 500 : 200);
}

function answerIn(now: string, httpMethod: string, message: any, version: string | null): Response {
  const streamed = httpMethod === "POST" ? sseDiscover(now, message) : null;
  if (streamed) return streamed;
  if (now === "ssehold" || now === "ssestrid" || now === "ssefail") return modern(httpMethod, message, version, false);
  if (now === "sseidless") return ready(httpMethod, message);
  if (now === "ready") return ready(httpMethod, message);
  if (now === "modern" || now === "notimpl") return modern(httpMethod, message, version, now === "notimpl");
  if (now === "old2024") return ready(httpMethod, message, "404", true);
  if (now === "malformed" || now === "hostile") return ready(httpMethod, message, now);
  if (now === "methodnotfound") return ready(httpMethod, message, "rpc");
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
    let { now, readError } = await mode();
    while (now === "hang") {
      await Bun.sleep(50);
      ({ now, readError } = await mode());
    }
    const res = answerIn(now, req.method, message, req.headers.get("mcp-protocol-version"));
    const rpc = message ? `${message.method ?? (message.result !== undefined ? "(result)" : "(error)")} id=${message.id ?? "-"}` : "-";
    log(
      `+${asked - started}ms ${req.method} ${url.pathname} rpc=${rpc} session=${req.headers.has("mcp-session-id") ? "yes" : "no"} version=${req.headers.get("mcp-protocol-version") ?? "-"} mcp-method=${req.headers.get("mcp-method") ?? "-"} mcp-name=${req.headers.get("mcp-name") ?? "-"} mode=${now} -> ${res.status} +${Date.now() - started}ms${readError ? ` mode-file=${readError}` : ""}`,
    );
    return res;
  },
});
log(`listening ${server.port}`);
