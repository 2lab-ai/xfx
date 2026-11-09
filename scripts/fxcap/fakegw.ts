// Loopback fake AI Gateway for side-by-side fx/xfx captures.
// Usage: [FAKEGW_MODELS=<catalog.json>] [FAKEGW_MODELS_DOWN_MS=<ms>] [FAKEGW_MODELS_DELAY_MS=<ms>] [FAKEGW_CREDITS=<credits.json>] [FAKEGW_TITLE=<title.json>]
//        [FAKEGW_MODELS2=<file>] [FAKEGW_MODELS_PAD="<bytes> <head|tail>"] [FAKEGW_MODELS_STALL_MS=<ms>]
//        [FAKEGW_USAGE=<usage.json>]
//        [FAKEGW_RELEASE=<file>]
//        bun fakegw.ts <port> <script.json> [bodies-dir]
// script.json: array of turns. Each turn is
//   {chunks?: string[], delayMs?: number, tool?: {id?, name, input}, tools?: [{id?, name, input}],
//    status?: number, body?: string, hangMs?: number, holdAt?: number, serviceTier?: string,
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
// FAKEGW_MODELS2 names a file the models path serves instead while it exists (`capture.sh`'s
// `models2` line renames a catalog into place): a model released while the binary runs, at a
// point in the scenario rather than a moment on the gateway's clock, which load moves.
// Both files are read whole (`settled`): one read empty or half written is read again.
// FAKEGW_MODELS_PAD (`modelspad <bytes> <head|tail>`) pads the catalog the models path serves with
// spaces, before it or after it, to exactly <bytes>: JSON either way. Past a client's cap, a body
// padded after is one whose first bytes still read as the catalog; one padded before is none once
// cut. Built in memory per answer, never a file. FAKEGW_MODELS_STALL_MS (`modelsstall <ms>`)
// sends that catalog's head and the first half of its body at once, and the rest <ms> later --
// or never, when the client goes first: a read stalled mid-body, past a deadline or not.
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
// the others 404. `status` other than 200 with no `json`/`body` answers `fake status <n>` on every
// one of them, the report's and the budget's too (so a 401, 429 or 5xx carries no rows); `delayMs`
// holds the answer that long. Without the file every one of those paths is unknown here, and answers 404.
// `billing` is what the Gateway sends for a turn it billed, and what both binaries need before
// they count one: a `response-metadata` event with the billing time ahead of the text, and a
// finish whose usage has the cache and reasoning counts and whose `providerMetadata.gateway`
// names the canonical slug, the cost (a decimal string) and the generation id
// (vercel-labs/fx@c1db9191 src/gateway/client.zig:3313-3395, :3428-3456).
// `deferred` is a turn whose bill comes later: the stream names its generation id and says
// nothing of its cost, which fx c1db919 (and xfx before US-U5) recorded as pending
// (vercel-labs/fx@c1db9191 src/builtins/gateway.zig:650-660), and the bill is then served by
// id from GET /v1/generation?id=<gen> (src/gateway/client.zig:285-372) in the record shape
// upstream read (src/gateway/generation_usage.zig:93-178): 404 -- "not written yet", which
// both asked again -- for the first `notYet` asks, then the record. fx v0.0.16 and xfx since
// US-U5 ask for no bill (a41fc2d4), so for them a deferred turn is a turn and nothing more;
// every ask is logged, so a log with none says so.
// FAKEGW_CREDITS names the JSON served for GET /coding-agent/v1/credits; without it that path
// answers 404 `not found`, which is the answer a capture of the error notice needs.
// `tools` is several tool calls in one step, and `chunks` beside a tool or tools is text the
// step says before its calls. `status` answers with that HTTP status (and `body`); `hangMs` holds
// the stream open that long before its last event (for interrupt captures). `holdAt: <n>` holds
// the Gateway stream after its first n events -- a billed turn's response-metadata first, then
// one per chunk, one per tool call, the finish last; n from 1 to one less than their count --
// until it is let go: the gateway logs `hold <k> after <n> event(s)` (k counts the holds since
// it started), and the stream goes on once the file FAKEGW_RELEASE names has k lines (capture.sh's
// `held` waits for that log line, its `release` writes a line), or the client goes, or
// HOLD_CAP_MS pass. A snap taken while the stream is held reads the same n events on both
// binaries, whatever the machine's load: a point in the stream, not a moment on the clock. Only
// the Gateway wire is paced; on the llmux and Codex wires `holdAt` is logged as not held.
// `serviceTier` is the tier the finish event confirms in `providerMetadata.gateway.serviceTier`
// (fx's `parseSseServiceTier`, gateway/client.zig:3451-3463); absent, nothing is confirmed.
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

// The catalog the models path answers with, padded and stalled as FAKEGW_MODELS_PAD and
// FAKEGW_MODELS_STALL_MS say; null when neither is set, so every other run answers as before.
function shapedCatalog(text: string, logLine: (line: string) => void): Response | null {
  const pad = process.env.FAKEGW_MODELS_PAD;
  const stall = Number(process.env.FAKEGW_MODELS_STALL_MS || 0);
  if (!pad && stall <= 0) return null;
  let bytes = new TextEncoder().encode(text);
  if (pad) {
    const [size, where] = pad.split(" ");
    const fill = new TextEncoder().encode(" ".repeat(Math.max(0, Number(size) - bytes.length)));
    const padded = new Uint8Array(bytes.length + fill.length);
    padded.set(where === "head" ? fill : bytes, 0);
    padded.set(where === "head" ? bytes : fill, where === "head" ? fill.length : bytes.length);
    bytes = padded;
    logLine(`padded to ${bytes.length} bytes, ${where}`);
  }
  const headers = { "content-type": "application/json" };
  if (stall <= 0) return new Response(bytes, { headers });
  const half = Math.floor(bytes.length / 2);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const body = new ReadableStream({
    start(controller) {
      controller.enqueue(bytes.slice(0, half));
      logLine(`stalled after ${half} of ${bytes.length} bytes for ${stall}ms`);
      timer = setTimeout(() => {
        try {
          controller.enqueue(bytes.slice(half));
          controller.close();
        } catch {}
      }, stall);
    },
    cancel() {
      clearTimeout(timer);
      logLine("stalled read: the client went");
    },
  });
  return new Response(body, { headers });
}

// A JSON file a run swaps while the gateway serves it (FAKEGW_USAGE, FAKEGW_MODELS2), read whole:
// its text, or null while it is absent. A read that does not parse -- the file empty or half
// written by a writer that truncates and then writes -- is read again every 10 ms, for up to
// SETTLE_MS, before it is taken as it is (FLAKE-MCPMODE, registry #19). One read, no `exists()`
// first: a file removed between the two (`usage off`) is absent, not a failed read.
const SETTLE_MS = 2000;
async function settled(path: string): Promise<string | null> {
  const until = Date.now() + SETTLE_MS;
  for (;;) {
    let text: string;
    try {
      text = await Bun.file(path).text();
    } catch (e: any) {
      if (e?.code === "ENOENT") return null;
      throw e;
    }
    try {
      JSON.parse(text);
      return text;
    } catch {
      if (Date.now() >= until) return text;
    }
    await Bun.sleep(10);
  }
}

// A usage read answered from FAKEGW_USAGE, or null while that file is absent (the path is then
// unknown here) or for a path that is not one of them.
async function usage(url: URL): Promise<Response | null> {
  if (!process.env.FAKEGW_USAGE || !usagePaths.has(url.pathname)) return null;
  const text = await settled(process.env.FAKEGW_USAGE);
  if (text === null) return null;
  const route: any = JSON.parse(text)[url.pathname] ?? {};
  if (route.delayMs) await Bun.sleep(route.delayMs);
  let json = route.json;
  let what = "";
  // The default answers (rows, no users) are a 200's: a `status` alone answers
  // `fake status <n>` on these two paths as on the others.
  const plain = json === undefined && route.body === undefined && (route.status ?? 200) === 200;
  if (plain && url.pathname === "/v1/report") {
    const from = url.searchParams.get("start_date") ?? "", to = url.searchParams.get("end_date") ?? "";
    const rows = route.byRange?.[`${from}..${to}`] ?? route.byStart?.[from] ?? route.rows ?? [];
    json = { results: rows };
    what = ` rows=${rows.length}`;
  }
  if (plain && url.pathname === "/ai-gateway/usage-report") json = { users: [] };
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
      unheld(turn, "the llmux wire is not paced");
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
      unheld(turn, "the Codex wire is not paced");
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
        const asked = Date.now();
        // FAKEGW_MODELS_DELAY_MS (`capture.sh`'s `modelsdelay` verb): a catalog served waits
        // that long before it answers -- and, set at all (`0` too), the catalog's log lines say
        // when, in ms since the gateway started. Unset, the lines are as they always were.
        const timed = Boolean(process.env.FAKEGW_MODELS_DELAY_MS);
        const at = (ms: number) => (timed ? ` at=${ms}ms` : "");
        log(`GET ${url.pathname} bearer=${req.headers.get("authorization")?.startsWith("Bearer ") ? "yes" : "no"}${at(asked - started)}`);
        // FAKEGW_MODELS_DOWN_MS (`modelsdown`): unavailable for that long after the gateway
        // starts, so a catalog the launch could not read is back for a later ask. A 503 answers
        // at once.
        if (asked - started < Number(process.env.FAKEGW_MODELS_DOWN_MS || 0)) {
          log(`GET ${url.pathname} -> 503 (down)${at(Date.now() - started)}`);
          return new Response("unavailable", { status: 503 });
        }
        const delay = Number(process.env.FAKEGW_MODELS_DELAY_MS || 0);
        if (delay > 0) await Bun.sleep(delay);
        if (timed) log(`GET ${url.pathname} -> 200${at(Date.now() - started)}`);
        // FAKEGW_MODELS2 (`models2`): the second catalog, once its file is in place by the time
        // the answer goes (after any `modelsdelay`).
        const second = process.env.FAKEGW_MODELS2 ? await settled(process.env.FAKEGW_MODELS2) : null;
        if (second !== null) {
          log(`GET ${url.pathname} -> second catalog`);
          return new Response(second, { headers: { "content-type": "application/json" } });
        }
        const catalog = process.env.FAKEGW_MODELS
          ? await Bun.file(process.env.FAKEGW_MODELS).text()
          : JSON.stringify({ data: [{ id: "fake/model-a", type: "language", tags: ["tool-use"] }] });
        const shaped = shapedCatalog(catalog, (line) => log(`GET ${url.pathname} -> ${line}`));
        if (shaped) return shaped;
        if (process.env.FAKEGW_MODELS) return new Response(catalog, { headers: { "content-type": "application/json" } });
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
    if (turn.reset) {
      unheld(turn, "a reset turn is cut, not held");
      return resetStream(events, turn.delayMs ?? 0);
    }
    events.push(done);
    if (turn.holdAt !== undefined && !(Number.isInteger(turn.holdAt) && turn.holdAt >= 1 && turn.holdAt < events.length)) {
      unheld(turn, `this turn has ${events.length} event(s): holdAt is 1 to ${events.length - 1}`);
      return sse(events, turn.delayMs ?? 0, turn.hangMs ?? 0);
    }
    return sse(events, turn.delayMs ?? 0, turn.hangMs ?? 0, turn.holdAt);
}

// A turn whose `holdAt` this stream does not honour: said in the log, so the `held` that waits
// for it fails on a line that names why.
function unheld(turn: any, why: string) {
  if (turn.holdAt !== undefined) log(`holdAt ${turn.holdAt} not held: ${why}`);
}

// The held streams so far, and how long one waits at most to be let go.
let holds = 0;
const HOLD_CAP_MS = 120000;

// Holds a stream after its first `after` events (the turn's `holdAt`) until the file
// FAKEGW_RELEASE names has as many lines as there have been holds, the client goes, or
// HOLD_CAP_MS pass -- each said in the log.
async function hold(after: number, gone: () => boolean) {
  const k = ++holds;
  const file = process.env.FAKEGW_RELEASE;
  log(`hold ${k} after ${after} event(s)`);
  const until = Date.now() + HOLD_CAP_MS;
  for (;;) {
    if (gone()) return log(`hold ${k}: stream closed by client`);
    if (file && (await released(file)) >= k) return log(`release ${k}`);
    if (Date.now() >= until) return log(`hold ${k} not released in ${HOLD_CAP_MS}ms; going on`);
    await Bun.sleep(20);
  }
}

// Whole lines only: a read that finds the file empty or a line half written counts fewer, never
// more, so it holds one poll longer at most (the FLAKE-MCPMODE class needs nothing more here).
async function released(path: string) {
  const file = Bun.file(path);
  if (!(await file.exists())) return 0;
  return ((await file.text()).match(/\n/g) ?? []).length;
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

function sse(events: object[], delayMs: number, hangMs: number, holdAt?: number) {
  const enc = new TextEncoder();
  let gone = false;
  return new Response(
    new ReadableStream({
      cancel() {
        gone = true;
      },
      async start(ctl) {
        try {
          for (let i = 0; i < events.length; i++) {
            if (i === holdAt) {
              await hold(i, () => gone);
              // A client that went while the stream was held gets nothing more; hold's line says so.
              if (gone) return;
            }
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
