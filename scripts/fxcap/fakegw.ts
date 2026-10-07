// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: [FAKEGW_MODELS=<catalog.json>] [FAKEGW_CREDITS=<credits.json>] bun fakegw.ts <port> <script.json> [bodies-dir]
// script.json: array of turns. Each turn is
//   {chunks?: string[], delayMs?: number, tool?: {id?, name, input}, tools?: [{id?, name, input}],
//    status?: number, body?: string, hangMs?: number, serviceTier?: string,
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
// the stream open that long before its last event (for interrupt captures). `serviceTier` is the
// tier the finish event confirms in `providerMetadata.gateway.serviceTier` (fx's
// `parseSseServiceTier`, gateway/client.zig:3451-3463); absent, nothing is confirmed.
// Every POST to a language-model path consumes the next turn; title calls get a finish-only stream,
// and fx's permission classifier gets a `permission_decision` tool call that clears the action
// (the shape fx's own e2e harness answers with, tests/e2e/tmux-helpers.ts:173-183).
// Every request body is written to <bodies-dir>/NNN-<kind>.json when a bodies dir is given.
//
// The same port is also a llmux daemon and a Codex subscription endpoint, so a capture can switch
// provider mid-session and read what the next request carried (`/provider`):
//   llmux (xfx): GET / answers `llmux` and GET /models (exactly) a one-model catalog,
//     `fake/model-a` -- the two probes `xfx setup llmux` makes; POST /v1/messages takes the next
//     turn as Anthropic SSE.
//   Codex (fx): GET /codex/models a catalog of `gpt-5.6-sol` and the reviewer `gpt-5.6-luna`, in
//     the shape fx's codex reader takes; POST /codex/responses the next turn as Responses SSE (fx's own e2e shapes,
//     tests/e2e/tui-auth-source-selection.test.ts:84-118, :142-176), and a title request a
//     completed empty response.
//   A turn's `thinking: {text, signature}` is the reasoning each of those two wires seals: a signed
//   `thinking` block ahead of the text on llmux, a `reasoning` item whose `encrypted_content` is the
//   signature on Codex. The Gateway's stream ignores it.
// Turns are one queue, consumed in order whichever wire asks; bodies are kept as NNN-llmux.json and
// NNN-codex.json.
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
    if (req.method === "GET" && url.pathname === "/") {
      log("GET / (llmux probe)");
      return new Response("llmux");
    }
    if (req.method === "GET" && url.pathname === "/models") {
      log("GET /models (llmux catalog)");
      return Response.json({ models: [{ id: "fake/model-a", name: "fake/model-a", aliases: [], group: "anthropic", efforts: [], max_context: 200000 }] });
    }
    if (req.method === "GET" && url.pathname === "/codex/models") {
      log(`GET /codex/models bearer=${req.headers.get("authorization")?.startsWith("Bearer ") ? "yes" : "no"}`);
      // `gpt-5.6-luna` too: fx refuses a Codex catalog without its permission reviewer
      // (`reviewer_model`, src/gateway/openai_codex_models.zig:20, :128-137).
      const row = (slug: string, context_window: number) => ({
        slug, visibility: "list", supported_in_api: true, supported_reasoning_levels: [{ effort: "high" }],
        additional_speed_tiers: [], input_modalities: ["text"], context_window,
      });
      return Response.json({ models: [row("gpt-5.6-sol", 272000), row("gpt-5.6-luna", 128000)] });
    }
    if (req.method === "POST" && url.pathname === "/v1/messages") {
      const body = await req.text();
      await keep("llmux", body);
      const turn = script.shift();
      log(`llmux ${url.pathname} -> ${turn ? describe(turn) : "EMPTY"}`);
      if (!turn) return new Response("unexpected request", { status: 500 });
      if (turn.status) return new Response(turn.body ?? `fake status ${turn.status}`, { status: turn.status });
      return anthropic(turn);
    }
    if (req.method === "POST" && url.pathname === "/codex/responses") {
      const body = await req.text();
      if (body.includes("Generate a short title")) {
        log("codex title");
        await keep("codex-title", body);
        return responses([{ type: "response.completed", response: { id: "title", status: "completed", usage: { input_tokens: 1, output_tokens: 1 } } }]);
      }
      await keep("codex", body);
      const turn = script.shift();
      log(`codex ${url.pathname} -> ${turn ? describe(turn) : "EMPTY"}`);
      if (!turn) return new Response("unexpected request", { status: 500 });
      if (turn.status) return new Response(turn.body ?? `fake status ${turn.status}`, { status: turn.status });
      return responses(responsesEvents(turn));
    }
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
    const done: any = turn.billing ? billedFinish(reason, turn.billing) : turn.deferred ? deferredFinish(reason, turn.deferred) : finish(reason);
    if (turn.serviceTier) {
      done.providerMetadata = { ...(done.providerMetadata ?? {}), gateway: { ...(done.providerMetadata?.gateway ?? {}), serviceTier: turn.serviceTier } };
    }
    events.push(done);
    return sse(events, turn.delayMs ?? 0, turn.hangMs ?? 0);
  },
});
log(`listening ${server.port}`);

function describe(turn: any) {
  return turn.tool || turn.tools ? "tool " + (turn.tools ?? [turn.tool]).map((t: any) => t.name).join(",") : "text";
}

// A turn as the Anthropic Messages stream a llmux daemon writes (the frames of
// tests/support/fake_llmux.rs): an optional signed thinking block, the text, the tool calls.
function anthropic(turn: any) {
  const frames: string[] = [];
  const event = (name: string, payload: object) => frames.push(`event: ${name}\ndata: ${JSON.stringify(payload)}\n\n`);
  event("message_start", { type: "message_start", message: { id: "msg_fake", type: "message", role: "assistant", model: "fake", content: [], stop_reason: null, usage: { input_tokens: 1200, output_tokens: 1 } } });
  let index = 0;
  if (turn.thinking) {
    event("content_block_start", { type: "content_block_start", index, content_block: { type: "thinking", thinking: "" } });
    event("content_block_delta", { type: "content_block_delta", index, delta: { type: "thinking_delta", thinking: turn.thinking.text } });
    event("content_block_delta", { type: "content_block_delta", index, delta: { type: "signature_delta", signature: turn.thinking.signature } });
    event("content_block_stop", { type: "content_block_stop", index });
    index += 1;
  }
  if (turn.chunks?.length) {
    event("content_block_start", { type: "content_block_start", index, content_block: { type: "text", text: "" } });
    for (const c of turn.chunks) event("content_block_delta", { type: "content_block_delta", index, delta: { type: "text_delta", text: c } });
    event("content_block_stop", { type: "content_block_stop", index });
    index += 1;
  }
  const tools = turn.tools ?? (turn.tool ? [turn.tool] : []);
  tools.forEach((t: any, i: number) => {
    event("content_block_start", { type: "content_block_start", index, content_block: { type: "tool_use", id: t.id ?? `toolu_${seq}_${i}`, name: t.name } });
    event("content_block_delta", { type: "content_block_delta", index, delta: { type: "input_json_delta", partial_json: JSON.stringify(t.input) } });
    event("content_block_stop", { type: "content_block_stop", index });
    index += 1;
  });
  event("message_delta", { type: "message_delta", delta: { stop_reason: tools.length ? "tool_use" : "end_turn", stop_sequence: null }, usage: { input_tokens: 1200, output_tokens: 40 } });
  event("message_stop", { type: "message_stop" });
  return new Response(frames.join(""), { headers: { "content-type": "text/event-stream" } });
}

// A turn as the Responses stream fx's codex reader takes: text deltas, function calls, completed.
function responsesEvents(turn: any) {
  const events: object[] = [];
  if (turn.thinking) {
    events.push({ type: "response.output_item.done", output_index: 0, item: { id: "rs_fake", type: "reasoning", summary: [{ type: "summary_text", text: turn.thinking.text }], encrypted_content: turn.thinking.signature } });
  }
  for (const c of turn.chunks ?? []) events.push({ type: "response.output_text.delta", delta: c });
  // Each output item has its own index: fx refuses two kinds at one
  // (`check_output_kind`, src/gateway/responses_protocol.zig:1070-1078).
  const first = turn.thinking ? 1 : 0;
  const tools = turn.tools ?? (turn.tool ? [turn.tool] : []);
  tools.forEach((t: any, i: number) => {
    events.push({ type: "response.output_item.added", output_index: first + i, item: { type: "function_call", call_id: t.id ?? `call_${seq}_${i}`, name: t.name } });
    events.push({ type: "response.function_call_arguments.done", output_index: first + i, arguments: JSON.stringify(t.input) });
  });
  events.push({ type: "response.completed", response: { id: `response-${seq}`, status: "completed", usage: { input_tokens: 1200, output_tokens: 40 } } });
  return events;
}

function responses(events: object[]) {
  return new Response(events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join(""), { headers: { "content-type": "text/event-stream" } });
}

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
