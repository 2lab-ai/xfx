// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: [FAKEGW_MODELS=<catalog.json>] bun fakegw.ts <port> <script.json> [bodies-dir]
// script.json: array of turns. Each turn is
//   {chunks?: string[], delayMs?: number, tool?: {id?, name, input}, tools?: [{id?, name, input}],
//    status?: number, hangMs?: number}
// `tools` = several tool calls in one step; chunks + tool(s) = text before the calls; `status` = answer
// with that HTTP status (error body); `hangMs` = hold the stream open that long after the chunks before
// finishing (for interrupt captures). Classifier calls get a permission_decision tool call "clear" (the
// shape fx's own e2e harness answers with, tests/e2e/tmux-helpers.ts:173-183).
// Every request body is written to <bodies-dir>/NNN-<kind>.json when a bodies dir is given.
const port = Number(process.argv[2] ?? 0);
const script: any[] = JSON.parse(await Bun.file(process.argv[3]).text());
const bodies = process.argv[4];
let seq = 0;
const log = (m: string) => console.log(`[fakegw] ${m}`);

function finish(reason = "stop") {
  return { type: "finish", finishReason: { unified: reason, raw: reason }, usage: { inputTokens: { total: 1200 }, outputTokens: { total: 40 } } };
}

async function keep(kind: string, body: string) {
  if (!bodies) return;
  seq += 1;
  await Bun.write(`${bodies}/${String(seq).padStart(3, "0")}-${kind}.json`, body);
}

const server = Bun.serve({
  port,
  hostname: "127.0.0.1",
  idleTimeout: 0,
  async fetch(req) {
    const url = new URL(req.url);
    if (req.method === "GET") {
      // The model catalog: FAKEGW_MODELS names a scenario file to serve
      // (`capture.sh`'s `models` verb), so both binaries list the same one.
      // Logged with whether a bearer came with it -- never the bearer.
      if (url.pathname.endsWith("/models")) {
        log(`GET ${url.pathname} bearer=${req.headers.get("authorization")?.startsWith("Bearer ") ? "yes" : "no"}`);
        if (process.env.FAKEGW_MODELS) return new Response(await Bun.file(process.env.FAKEGW_MODELS).text(), { headers: { "content-type": "application/json" } });
        return Response.json({ data: [{ id: "fake/model-a", type: "language", tags: ["tool-use"] }] });
      }
      return new Response("not found", { status: 404 });
    }
    const body = await req.text();
    if (body.includes("permission_decision")) {
      log(`classifier ${url.pathname}`);
      await keep("classifier", body);
      return sse([
        { type: "tool-call", toolCallId: "permission_decision_1", toolName: "permission_decision", input: { risk: "low", decision: "clear", rationale: "fixture" } },
        finish("tool-calls"),
      ], 0, 0);
    }
    if (body.includes("Generate a short title")) {
      log(`title ${url.pathname}`);
      await keep("title", body);
      return sse([finish()], 0, 0);
    }
    await keep("completion", body);
    const turn = script.shift();
    log(`completion ${url.pathname} -> ${turn ? (turn.status ? "status " + turn.status : turn.tool || turn.tools ? "tool " + (turn.tools ?? [turn.tool]).map((t: any) => t.name).join(",") : "text") : "EMPTY"}`);
    if (!turn) return new Response("unexpected request", { status: 500 });
    if (turn.status) return new Response(turn.body ?? `fake status ${turn.status}`, { status: turn.status });
    const events: object[] = [];
    for (const c of turn.chunks ?? []) events.push({ type: "text-delta", id: "answer_1", delta: c });
    const tools = turn.tools ?? (turn.tool ? [turn.tool] : []);
    tools.forEach((t: any, i: number) => events.push({ type: "tool-call", toolCallId: t.id ?? `call_${seq}_${i}`, toolName: t.name, input: t.input }));
    events.push(finish(tools.length ? "tool-calls" : "stop"));
    return sse(events, turn.delayMs ?? 0, turn.hangMs ?? 0);
  },
});
log(`listening ${server.port}`);

function sse(events: object[], delayMs: number, hangMs: number) {
  const enc = new TextEncoder();
  return new Response(
    new ReadableStream({
      async start(ctl) {
        try {
          for (let i = 0; i < events.length; i++) {
            if (i === events.length - 1 && hangMs) await Bun.sleep(hangMs);
            ctl.enqueue(enc.encode(`data: ${JSON.stringify(events[i])}\n\n`));
            if (delayMs) await Bun.sleep(delayMs);
          }
          ctl.enqueue(enc.encode("data: [DONE]\n\n"));
          ctl.close();
        } catch (e) {
          log(`stream closed by client: ${e}`);
        }
      },
    }),
    { headers: { "content-type": "text/event-stream" } },
  );
}
