// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: [FAKEGW_MODELS=<catalog.json>] [FAKEGW_CREDITS=<credits.json>] bun fakegw.ts <port> <script.json> [bodies-dir]
// script.json: array of turns. Each turn is
//   {chunks?: string[], delayMs?: number, tool?: {id?, name, input}, tools?: [{id?, name, input}],
//    status?: number, body?: string, hangMs?: number,
//    billing?: {gen, slug, cost, input?, output?, cacheRead?, cacheWrite?, reasoning?},
//    deferred?: {gen, slug, cost, input?, output?, cacheRead?, cacheWrite?, reasoning?, notYet?}}
// `billing` is what the Gateway sends for a turn it billed, and what both binaries need before
// they count one: a `response-metadata` event with the billing time ahead of the text, and a
// finish whose usage has the cache and reasoning counts and whose `providerMetadata.gateway`
// names the canonical slug, the cost (a decimal string) and the generation id
// (vercel-labs/fx@c1db9191 src/gateway/client.zig:3313-3395, :3428-3456).
// `deferred` is a turn whose bill comes later: the stream names its generation id and says
// nothing of its cost, which both binaries record as pending (vercel-labs/fx@c1db9191
// src/builtins/gateway.zig:650-660), and the bill is then served by id from
// GET /v1/generation?id=<gen> (src/gateway/client.zig:285-372) in the record shape upstream
// reads (src/gateway/generation_usage.zig:93-178): 404 -- "not written yet", which both ask
// again -- for the first `notYet` asks, then the record.
// FAKEGW_CREDITS names the JSON served for GET /coding-agent/v1/credits; without it that path
// answers 404 `not found`, which is the answer a capture of the error notice needs.
// `tools` is several tool calls in one step, and `chunks` beside a tool or tools is text the
// step says before its calls. `status` answers with that HTTP status (and `body`); `hangMs` holds
// the stream open that long before its last event (for interrupt captures).
// Every POST to a language-model path consumes the next turn; title calls get a finish-only stream,
// and fx's permission classifier gets a `permission_decision` tool call that clears the action
// (the shape fx's own e2e harness answers with, tests/e2e/tmux-helpers.ts:173-183).
// Every request body is written to <bodies-dir>/NNN-<kind>.json when a bodies dir is given.
const port = Number(process.argv[2] ?? 0);
const script: any[] = JSON.parse(await Bun.file(process.argv[3]).text());
const bodies = process.argv[4];
let seq = 0;
const log = (m: string) => console.log(`[fakegw] ${m}`);
// Generation id -> the record a later GET /v1/generation answers, and how many asks to turn away first.
const generations = new Map<string, { record: object; notYet: number }>();

function finish(reason = "stop") {
  return { type: "finish", finishReason: { unified: reason, raw: reason }, usage: { inputTokens: { total: 1200 }, outputTokens: { total: 40 } } };
}

// The finish of a turn whose bill comes later: its usage and its generation id, and no cost.
function deferredFinish(reason: string, d: any) {
  return {
    type: "finish",
    finishReason: { unified: reason, raw: reason },
    usage: {
      inputTokens: { total: d.input ?? 1200, cacheRead: d.cacheRead ?? 0, cacheWrite: d.cacheWrite ?? 0 },
      outputTokens: { total: d.output ?? 40, reasoning: d.reasoning ?? 0 },
    },
    providerMetadata: { gateway: { generationId: d.gen } },
  };
}

// The record GET /v1/generation answers for a deferred turn: native counts, so the prompt is
// the input without its cache reads and writes and the completion the output without reasoning.
function generationRecord(d: any, createdAt: string) {
  const cacheRead = d.cacheRead ?? 0, cacheWrite = d.cacheWrite ?? 0, reasoning = d.reasoning ?? 0;
  return {
    data: {
      id: d.gen,
      model: d.slug,
      created_at: createdAt,
      total_cost: Number(d.cost),
      native_tokens_prompt: (d.input ?? 1200) - cacheRead - cacheWrite,
      native_tokens_completion: (d.output ?? 40) - reasoning,
      native_tokens_cached: cacheRead,
      native_tokens_cache_creation: cacheWrite,
      native_tokens_reasoning: reasoning,
      billable_web_search_calls: 0,
    },
  };
}

// The finish of a turn the Gateway billed: the usage it was billed on, and who billed it for what.
function billedFinish(reason: string, b: any) {
  return {
    type: "finish",
    finishReason: { unified: reason, raw: reason },
    usage: {
      inputTokens: { total: b.input ?? 1200, cacheRead: b.cacheRead ?? 0, cacheWrite: b.cacheWrite ?? 0 },
      outputTokens: { total: b.output ?? 40, reasoning: b.reasoning ?? 0 },
    },
    providerMetadata: { gateway: { routing: { canonicalSlug: b.slug }, cost: b.cost, generationId: b.gen } },
  };
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
      // The credits balance (`credits` verb): the scenario's answer, or the 404 an account
      // without one gets. Logged like the catalog: whether a bearer came, never the bearer.
      if (url.pathname.endsWith("/credits")) {
        log(`GET ${url.pathname}${url.search} bearer=${req.headers.get("authorization")?.startsWith("Bearer ") ? "yes" : "no"}`);
        if (process.env.FAKEGW_CREDITS) return new Response(await Bun.file(process.env.FAKEGW_CREDITS).text(), { headers: { "content-type": "application/json" } });
        return new Response("not found", { status: 404 });
      }
      // A deferred turn's bill, by generation id: turned away `notYet` times, then served.
      if (url.pathname.endsWith("/v1/generation")) {
        const id = url.searchParams.get("id") ?? "";
        const known = generations.get(id);
        const answer = !known ? "unknown" : known.notYet > 0 ? "not yet" : "record";
        log(`GET ${url.pathname}?id=${id} bearer=${req.headers.get("authorization")?.startsWith("Bearer ") ? "yes" : "no"} -> ${answer}`);
        if (!known || known.notYet > 0) {
          if (known) known.notYet -= 1;
          return new Response("not found", { status: 404 });
        }
        return Response.json(known.record);
      }
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
    log(`completion ${url.pathname} -> ${turn ? (turn.status ? "status " + turn.status : turn.tool || turn.tools ? "tool " + (turn.tools ?? [turn.tool]).map((t: any) => t.name).join(",") : turn.deferred ? "text, bill deferred" : "text") : "EMPTY"}`);
    if (!turn) return new Response("unexpected request", { status: 500 });
    if (turn.status) return new Response(turn.body ?? `fake status ${turn.status}`, { status: turn.status });
    const events: object[] = [];
    const billedAt = new Date().toISOString();
    if (turn.billing) events.push({ type: "response-metadata", modelId: turn.billing.slug, timestamp: billedAt });
    if (turn.deferred) {
      events.push({ type: "response-metadata", modelId: turn.deferred.slug, timestamp: billedAt });
      generations.set(turn.deferred.gen, { record: generationRecord(turn.deferred, billedAt), notYet: turn.deferred.notYet ?? 0 });
    }
    for (const c of turn.chunks ?? []) events.push({ type: "text-delta", id: "answer_1", delta: c });
    const tools = turn.tools ?? (turn.tool ? [turn.tool] : []);
    tools.forEach((t: any, i: number) => events.push({ type: "tool-call", toolCallId: t.id ?? `call_${seq}_${i}`, toolName: t.name, input: t.input }));
    const reason = tools.length ? "tool-calls" : "stop";
    events.push(turn.billing ? billedFinish(reason, turn.billing) : turn.deferred ? deferredFinish(reason, turn.deferred) : finish(reason));
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
