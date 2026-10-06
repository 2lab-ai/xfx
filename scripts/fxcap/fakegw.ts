// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: bun fakegw.ts <port> <script.json>
// script.json: array of turns; each turn is {chunks: string[], delayMs?: number, tool?: {name, input}}.
// Every POST to a language-model path consumes the next turn; title and classifier side calls get a finish-only stream.
const port = Number(process.argv[2] ?? 0);
const script: any[] = JSON.parse(await Bun.file(process.argv[3]).text());
const log = (m: string) => console.log(`[fakegw] ${m}`);

function finish(reason = "stop") {
  return { type: "finish", finishReason: { unified: reason, raw: reason }, usage: { inputTokens: { total: 1200 }, outputTokens: { total: 40 } } };
}

const server = Bun.serve({
  port,
  hostname: "127.0.0.1",
  idleTimeout: 0,
  async fetch(req) {
    const url = new URL(req.url);
    if (req.method === "GET") {
      if (url.pathname.endsWith("/models")) return Response.json({ data: [{ id: "fake/model-a", type: "language", tags: ["tool-use"] }] });
      return new Response("not found", { status: 404 });
    }
    const body = await req.text();
    if (body.includes("permission_decision")) {
      log(`classifier ${url.pathname}`);
      return sse([{ type: "text-delta", id: "d", delta: JSON.stringify({ decision: "clear" }) }, finish()], 0);
    }
    if (body.includes("Generate a short title")) {
      log(`title ${url.pathname}`);
      return sse([finish()], 0);
    }
    const turn = script.shift();
    log(`completion ${url.pathname} -> ${turn ? (turn.tool ? "tool " + turn.tool.name : "text") : "EMPTY"}`);
    if (!turn) return new Response("unexpected request", { status: 500 });
    const events: object[] = [];
    if (turn.tool) {
      events.push({ type: "tool-call", toolCallId: turn.tool.id ?? "call_1", toolName: turn.tool.name, input: turn.tool.input });
      events.push(finish("tool-calls"));
    } else {
      for (const c of turn.chunks) events.push({ type: "text-delta", id: "answer_1", delta: c });
      events.push(finish());
    }
    return sse(events, turn.delayMs ?? 0);
  },
});
log(`listening ${server.port}`);

function sse(events: object[], delayMs: number) {
  const enc = new TextEncoder();
  return new Response(
    new ReadableStream({
      async start(ctl) {
        for (const e of events) {
          ctl.enqueue(enc.encode(`data: ${JSON.stringify(e)}\n\n`));
          if (delayMs) await Bun.sleep(delayMs);
        }
        ctl.enqueue(enc.encode("data: [DONE]\n\n"));
        ctl.close();
      },
    }),
    { headers: { "content-type": "text/event-stream" } },
  );
}
