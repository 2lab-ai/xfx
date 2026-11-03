// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: [FAKEGW_MODELS=<catalog.json>] [FAKEGW_MODELS_DOWN_MS=<ms>] [FAKEGW_CREDITS=<credits.json>] [FAKEGW_TITLE=<title.json>]
//        [FAKEGW_MODELS2=<catalog.json> FAKEGW_MODELS2_MS=<ms>] [FAKEGW_USAGE=<usage.json>]
//        bun fakegw.ts <port> <script.json> [bodies-dir]
// script.json: array of turns. Each turn is
//   {chunks?: string[], delayMs?: number, tool?: {id?, name, input}, tools?: [{id?, name, input}],
//    status?: number, body?: string, hangMs?: number, serviceTier?: string,
//    reset?: true, refuse?: number,
//    billing?: {gen, slug, cost, input?, output?, cacheRead?, cacheWrite?, reasoning?},
//    deferred?: {gen, slug, cost, input?, output?, cacheRead?, cacheWrite?, reasoning?, notYet?}}
// Every request, whatever its path, gets one line once it is answered:
//   [fakegw] request <METHOD> <path><?query> at=<ms>ms -> <status> at=<ms>ms headers=<names>
// the two times in ms since the gateway started (when it was asked, when it was answered), and
// which of `authorization`, `x-vercel-ai-gateway-team`, `chatgpt-account-id` and `x-userid` it
// carried -- the names only, never a value. A GET for a path nothing here knows is answered 404
// (a POST is a completion, as it always was) and logged like any other, so a request nobody
// planned for still leaves a line.
// `reset` cuts the connection instead of finishing the turn: the Gateway stream sends the turn's
// text (none, if it has no `chunks`) and the connection closes before the finish, a reset
// mid-stream; on the llmux and Codex wires it closes before any byte. `refuse: <ms>` cuts the
// connection before any byte and then refuses every connection for <ms> -- nothing listens on
// the port -- so a retry meets a refused connect, which is what fx calls `connectivity_lost`
// (vercel-labs/fx@481104fd src/gateway/client.zig:39-49); after it the same port listens again.
// Bun.serve has no way to close one connection, so a cut closes every connection open at that
// moment (`server.stop(true)`): a turn that cuts should not share the moment with another stream.
// FAKEGW_MODELS2 is a second catalog the models path serves instead once FAKEGW_MODELS2_MS have
// passed since the gateway started: a model released while the binary runs.
// FAKEGW_USAGE names a file read on every request (`capture.sh`'s `usage` verb swaps it): while it
// exists, the usage reads fx v0.0.16 makes are answered from it, in the shapes of its own e2e fake
// (vercel-labs/fx@481104fd tests/e2e/usage-fakes.ts) -- GET /v1/report (`{"results":[rows]}`),
// /ai-gateway/usage-report (`{"users":[...]}`), /backend-api/wham/usage, /backend-api/wham/profiles/me,
// /backend-api/wham/usage/daily-token-usage-breakdown, /billing and /settings. The file is an
// object keyed by those paths; each value is
//   {json?: any, status?: number, body?: string, delayMs?: number}
// and /v1/report's may also carry `rows` (the default), `byStart: {<start_date>: rows}` and
// `byRange: {"<start_date>..<end_date>": rows}`, chosen most specific first. A path the file does
// not name answers as upstream's fake does: /v1/report no rows, /ai-gateway/usage-report no users,
// the others 404. `status` with no `json`/`body` answers `fake status <n>`; `delayMs` holds the
// answer that long. Without the file every one of those paths is unknown here, and answers 404.
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
// Every POST to a language-model path consumes the next turn, except two side calls that are
// answered apart and consume nothing: fx's permission classifier gets a `permission_decision`
// tool call that clears the action (the shape fx's own e2e harness answers with,
// tests/e2e/tmux-helpers.ts:173-183), and a session-title request (both binaries ask one after a
// fresh session's first prompt, vercel-labs/fx@c1db9191 src/core/session/session_title_generation.zig)
// gets FAKEGW_TITLE's answer -- {chunks?: string[], delayMs?: number, status?: number, body?: string},
// the same fields as a turn's -- or, without one, a finish-only stream: no title, so the session
// keeps the one its first prompt gives it. Its log line names the model asked
// (`ai-language-model-id`) -- never the bearer.
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
const started = Date.now();
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

// The headers whose presence the request line names: the credential, the team, the ChatGPT
// account and the Grok user each usage read carries (usage-fakes.ts:21-28). Never their values.
const watched = ["authorization", "x-vercel-ai-gateway-team", "chatgpt-account-id", "x-userid"];

function request(req: Request, asked: number, answer: string) {
  const url = new URL(req.url);
  const carried = watched.filter((name) => req.headers.has(name)).join(",") || "-";
  log(`request ${req.method} ${url.pathname}${url.search} at=${asked - started}ms -> ${answer} at=${Date.now() - started}ms headers=${carried}`);
}

// Closes every open connection and stops listening, then listens again on the same port --
// at once for a reset, after `refuseMs` for a refusal. The first `listening` line is the one
// capture.sh reads the port from; this one is worded so it cannot be taken for it.
function cut(refuseMs: number) {
  const same = server.port;
  // Not awaited: the stop settles only once every request has, and a request being cut never
  // does -- awaited, nothing else would be left to run and the process would exit.
  void server.stop(true);
  if (refuseMs > 0) log(`refusing connections for ${refuseMs}ms`);
  setTimeout(() => {
    try {
      server = Bun.serve({ ...options, port: same });
      log(`accepting again on ${same}`);
    } catch (e) {
      log(`could not listen again on ${same}: ${e}`);
      process.exit(1);
    }
  }, refuseMs);
}

// A request this turn answers by cutting its connection: never answered, so its line is
// written here, before the cut.
function cutNow(req: Request, asked: number, refuseMs: number): Promise<Response> {
  request(req, asked, refuseMs > 0 ? `cut, refusing ${refuseMs}ms` : "cut");
  setTimeout(() => cut(refuseMs), 0);
  return new Promise(() => {});
}

const usagePaths = new Set([
  "/v1/report",
  "/ai-gateway/usage-report",
  "/backend-api/wham/usage",
  "/backend-api/wham/profiles/me",
  "/backend-api/wham/usage/daily-token-usage-breakdown",
  "/billing",
  "/settings",
]);

// A usage read answered from FAKEGW_USAGE, or null while that file is absent (the path is then
// unknown here) or for a path that is not one of them.
async function usage(url: URL): Promise<Response | null> {
  if (!process.env.FAKEGW_USAGE || !usagePaths.has(url.pathname)) return null;
  const file = Bun.file(process.env.FAKEGW_USAGE);
  if (!(await file.exists())) return null;
  const route: any = JSON.parse(await file.text())[url.pathname] ?? {};
  if (route.delayMs) await Bun.sleep(route.delayMs);
  let json = route.json;
  let what = "";
  if (json === undefined && route.body === undefined && url.pathname === "/v1/report") {
    const from = url.searchParams.get("start_date") ?? "", to = url.searchParams.get("end_date") ?? "";
    const rows = route.byRange?.[`${from}..${to}`] ?? route.byStart?.[from] ?? route.rows ?? [];
    json = { results: rows };
    what = ` rows=${rows.length}`;
  }
  if (json === undefined && route.body === undefined && url.pathname === "/ai-gateway/usage-report") json = { users: [] };
  const status = route.status ?? (json === undefined && route.body === undefined ? 404 : 200);
  log(`usage ${url.pathname} -> ${status}${what}${route.delayMs ? ` held ${route.delayMs}ms` : ""}`);
  if (route.body !== undefined) return new Response(route.body, { status });
  if (json !== undefined) return Response.json(json, { status });
  return new Response(status === 404 ? "not found" : `fake status ${status}`, { status });
}

const options = {
  hostname: "127.0.0.1",
  idleTimeout: 0,
  fetch(req: Request): Promise<Response> {
    const asked = Date.now();
    return answer(req, asked).then(
      (res) => {
        request(req, asked, String(res.status));
        return res;
      },
      (e) => {
        request(req, asked, `500 (fakegw: ${e})`);
        return new Response(`fakegw: ${e}`, { status: 500 });
      },
    );
  },
};
let server = Bun.serve({ ...options, port });
log(`listening ${server.port}`);

async function answer(req: Request, asked: number): Promise<Response> {
    const url = new URL(req.url);
    const read = await usage(url);
    if (read) return read;
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
      if (turn.reset || turn.refuse) return cutNow(req, asked, turn.refuse ?? 0);
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
      if (turn.reset || turn.refuse) return cutNow(req, asked, turn.refuse ?? 0);
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
        // FAKEGW_MODELS_DOWN_MS (`capture.sh`'s `modelsdown` verb): unavailable for that long after
        // the gateway starts, so a catalog the launch could not read is back for a later ask.
        if (Date.now() - started < Number(process.env.FAKEGW_MODELS_DOWN_MS || 0)) {
          log(`GET ${url.pathname} -> 503 (down)`);
          return new Response("unavailable", { status: 503 });
        }
        // FAKEGW_MODELS2 (`models2`): the second catalog, once FAKEGW_MODELS2_MS have passed.
        if (process.env.FAKEGW_MODELS2 && Date.now() - started >= Number(process.env.FAKEGW_MODELS2_MS || 0)) {
          log(`GET ${url.pathname} -> second catalog`);
          return new Response(await Bun.file(process.env.FAKEGW_MODELS2).text(), { headers: { "content-type": "application/json" } });
        }
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
      const answer: any = process.env.FAKEGW_TITLE ? JSON.parse(await Bun.file(process.env.FAKEGW_TITLE).text()) : {};
      log(`title ${url.pathname} model=${req.headers.get("ai-language-model-id") ?? "-"} -> ${answer.status ? "status " + answer.status : (answer.chunks ?? []).length ? "text" : "no text"}`);
      await keep("title", body);
      if (answer.status) return new Response(answer.body ?? `fake status ${answer.status}`, { status: answer.status });
      const events: object[] = (answer.chunks ?? []).map((c: string) => ({ type: "text-delta", id: "title_1", delta: c }));
      events.push(finish());
      return sse(events, answer.delayMs ?? 0, 0);
    }
    await keep("completion", body);
    const turn = script.shift();
    log(`completion ${url.pathname} -> ${turn ? (turn.refuse ? `refuse ${turn.refuse}ms` : turn.reset ? "reset" : turn.status ? "status " + turn.status : turn.tool || turn.tools ? "tool " + (turn.tools ?? [turn.tool]).map((t: any) => t.name).join(",") : turn.deferred ? "text, bill deferred" : "text") : "EMPTY"}`);
    if (!turn) return new Response("unexpected request", { status: 500 });
    if (turn.refuse) return cutNow(req, asked, turn.refuse);
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
    if (turn.reset) return resetStream(events, turn.delayMs ?? 0);
    events.push(done);
    return sse(events, turn.delayMs ?? 0, turn.hangMs ?? 0);
}

function describe(turn: any) {
  if (turn.refuse) return `refuse ${turn.refuse}ms`;
  if (turn.reset) return "reset";
  return turn.tool || turn.tools ? "tool " + (turn.tools ?? [turn.tool]).map((t: any) => t.name).join(",") : "text";
}

// A Gateway turn that resets mid-stream: its events up to the finish, then the connection cut.
// Its request line says 200, the status its headers carried; the cut has a line of its own.
function resetStream(events: object[], delayMs: number) {
  const enc = new TextEncoder();
  return new Response(
    new ReadableStream({
      async start(ctl) {
        for (const event of events) {
          ctl.enqueue(enc.encode(`data: ${JSON.stringify(event)}\n\n`));
          if (delayMs) await Bun.sleep(delayMs);
        }
        // Long enough for what was enqueued to be written before the connection goes.
        await Bun.sleep(100);
        log(`reset after ${events.length} event(s)`);
        cut(0);
      },
    }),
    { headers: { "content-type": "text/event-stream" } },
  );
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
