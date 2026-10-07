# P1 receipt: `/provider` mid-session keeps the conversation, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness, one run per binary and scenario, each on its own tmux socket with an empty HOME, the fake key and one loopback fake:

```
scripts/fxcap/capture.sh {fx,xfx} provider-carry 40x120 <dir>
scripts/fxcap/capture.sh {fx,xfx} provider-carry-back 40x120 <dir>
```

`fx` is the c1db919 build (`v0.0.13`). `xfx` is `target/debug/xfx` built from `6bd4e9a` (its banner says `v0.1.0-6bd4e9a`): this branch after `plan/tui-phase4` 82400635 was merged. (A first take on `8caaaa8`, before the plan's background catalog read, differed only in the launch turn's meter of `provider-carry-back`; every xfx file here is the `6bd4e9a` take.) Each snap has a `.txt` and an `.ansi` (`capture-pane -e`); `*-bodies/` holds every request the fake received, `NNN-<wire>.json` in arrival order; `*.files` is every file under HOME at that point (`home`).

**The second provider.** fx's providers are `vercel`, `codex`, `grok`; xfx's are `gateway`, `llmux`. So the pair is fx's Gateway → Codex against xfx's Gateway → llmux, and back. The fake (`scripts/fxcap/fakegw.ts`) answers on one port as the Gateway, as a llmux daemon (`GET /` → `llmux`, `GET /models`, `POST /v1/messages` as Anthropic SSE) and as a Codex endpoint (`GET /codex/models`, `POST /codex/responses` as Responses SSE, fx's own e2e shapes, `tests/e2e/tui-auth-source-selection.test.ts:84-118`). fx reaches Codex through its own e2e overrides, `FX_E2E_OPENAI_CODEX_MODELS_URL`/`FX_E2E_OPENAI_CODEX_RESPONSES_URL` (`openai_codex_models.zig:18`, `openai_codex.zig:16`, loopback only), with a seeded sign-in, `~/.fx/chatgpt-auth.json` (a fake token, no network, `FX_NO_OPEN_BROWSER=1` as every capture): nothing reached a browser or the network. Harness additions in this unit: `homefile <path> <file>` and `@PORT@` in seeded files and `env` values. One queue of turns serves whichever wire asks, so the same answers reach both binaries.

**Scenarios.** Both profiles have the context meter and the session title on (`statusLine`), so the footer shows what a switch keeps.

- `provider-carry`: on the Gateway, `read the notes and tell me what they say` -- a `read_file` call under the id `functions.read:0` (upstream's own non-portable vector, `tool_call_ids.zig:139`), its result, an answer. `/provider`, Down, Enter (fx `codex`, xfx `llmux`). `what day was it again?` on the new provider. `/quit`, `--resume` (fx `--resume-last`, xfx `--resume last`), `and after the resume?`.
- `provider-carry-back`: starts on the provider that seals its reasoning (fx `codex`: a `reasoning` item with `encrypted_content`; xfx `llmux`: a signed `thinking` block), the same tool turn, then `/provider` to the Gateway -- fx's Gateway row opens its sign-in columns (Enter, Down + Enter `api-key`, Enter `env`), xfx's commits -- and two prompts there.

**Sanitized**: every file was scanned for the run directory (`fxcap.*`), `/private/tmp`, `/private/var`, `/Users/` and the fake key. The screens and xfx's bodies held none. fx's request bodies carry the run directory inside fx's own system prompt (`workspace_root`, `current_directory`, `home_directory`): in the eight fx `*-completion.json`/`*-codex.json` files under `*-bodies/` it is written `<scratch>`. Nothing else was edited.

## Screens

Compared row by row in the `.ansi` files, every row after the banner (the resumed screens have none), first as captured, then with fx's provider words mapped to xfx's (`Codex subscription` → `llmux`, `gpt-5.6-sol` → `model-a`, `1k/272k` → `1k/200k`), then with the two rows this unit does not claim taken out of both (fx's gated `* auth: Switched credential …` leaf, xfx's one-time `! session:` notice, each with its blank row) and tmux's `\e[39m` placement ignored (see below).

| Snap | Identical as captured | After the provider map | And the two rows out, `\e[39m` ignored | Rows that still differ, and why |
|---|---|---|---|---|
| carry `start` | 4/4 | 4/4 | 4/4 | -- |
| carry `turn1` | 13/13 | 13/13 | 13/13 | -- (the Gateway turn: call group, answer, footer `auto · model-a · read the notes and tell me what  · 1k`) |
| carry `open` | 17/20 | 17/20 | 17/20 | 14-16: the provider rows (`vercel · current`/`codex`/`grok` against `gateway · current`/`llmux`/blank), D3's label map |
| carry `switched` | 14/17 | 16/17 | **17/17** | -- `* provider: Preparing <name>.`, `* provider: Switched to <name> with <model>.` and nothing after it; the footer keeps the title and the tokens and measures them against the new window at once: fx `· 1k/272k 0%`, xfx `· 1k/200k 0%`. Row 14 as captured: the `\e[39m` below |
| carry `turn2` | 20/23 | 22/23 | **23/23** | -- the first turn after the switch, under the same transcript (row 14 as above) |
| carry `quit` | 35/39 | 36/39 | 37/39 | 22: `Continue session with: fx --resume <id>` / `xfx --resume <id>`, each product's id; 40: tmux's `Pane is dead` timestamp |
| carry `resumed` | 19/20 | 20/20 | **20/20** | -- one session: `* session resumed: read the notes and tell me what they`, both turns replayed, the switch notices not (neither product replays notices) |
| carry `turn3` | 25/26 | 26/26 | **26/26** | -- |
| back `start` | 3/4 | 4/4 | 4/4 | -- |
| back `turn1` | 12/13 | 13/13 | **13/13** | -- (the turn on the provider that seals its reasoning; fx `· 1k/272k 0%`, xfx `· 1k/200k 0%`) |
| back `open` | 16/20 | 17/20 | 17/20 | 14-16: the provider rows, `codex · current` against `llmux · current` |
| back `switched` | 13/19 | 13/19 | **17/17** | fx's extra `* auth: Switched credential to AI_GATEWAY_API_KEY.` is its gated sign-in leaf (D3, zbrain #88) |
| back `turn2` | 13/26 | 13/26 | **23/23** | xfx's `! session: this session recorded reasoning on the anthropic_messages wire and is continuing on vercel_gateway, so that reasoning was not carried over` -- fx drops the same reasoning without a word (below) |
| back `turn3` | 13/32 | 13/32 | **29/29** | -- the notice is not said again |

The two rows taken out of both sides for the third column are fx's `* auth: Switched credential …` and xfx's `! session: …` (each with the blank row after it); nothing else is removed. The counts are `python3 compare.py .` (the first two columns) and `python3 compare.py . --drop` (the third), run in this directory.

**`\e[39m` on the `Switched to` row.** In carry `switched`/`turn2` fx's capture closes that row's colour on the next row and xfx's on the row itself; in back `switched` both close it on the next row. It is where tmux re-serializes the colour of the blank cells after the text (whichever later write touched them), not the text or its colour: xfx produces both variants.

## The next request after the switch

**carry, fx** (`fx-provider-carry-40x120-bodies/004-codex.json`, Responses `input`) and **xfx** (`xfx-provider-carry-40x120-bodies/004-llmux.json`, Anthropic `messages`):

| # | fx (Codex) | xfx (llmux) |
|---|---|---|
| 1 | `user` `input_text` `read the notes and tell me what they say` | `user` `text` the same |
| 2 | `function_call` `call_id` **`fx_c216cab2de2e56d547b4e6340edfc11a16bd7a07_0`** `read_file` | `assistant` `tool_use` `id` **`fx_c216cab2de2e56d547b4e6340edfc11a16bd7a07_0`** `read_file` |
| 3 | `function_call_output` the same id, the file | `user` `tool_result` `tool_use_id` the same id, the file |
| 4 | `user` `Session file evidence from previous tool execution. …` | -- (fx's file-evidence message, a feature xfx does not have; it is in fx's Gateway bodies too, `003-completion.json`) |
| 5 | `message` `assistant` `The notes say the meeting moved to Thursday.` | `assistant` `text` the same |
| 6 | `user` `what day was it again?` | `user` `text` the same |

The call id the Gateway minted, `functions.read:0`, goes to the new wire under the **same alias on both**: SHA-256 of the id, first 20 bytes, `fx_<hex>_0` (`Projection`, `tool_call_ids.zig:14-87`; xfx `src/gateway/tool_call_ids.rs`). Both logs keep `functions.read:0` (fx `sessions/*/events.jsonl` `tool_call.call_id`; xfx `events.jsonl` `assistant_message.tool_calls[0].id`, in `*-end.files`). After the resume (`005-codex.json`, `005-llmux.json`) every turn is there again, the switched turn's answer included.

**back, fx** (`fx-provider-carry-back-40x120-bodies/004-completion.json`) and **xfx** (`xfx-provider-carry-back-40x120-bodies/003-completion.json`), the Gateway's `prompt` without its system messages: `user`, `assistant` `tool-call` `toolCallId` `toolu_01` `read_file`, `tool` `tool-result` `text`, (fx: the file-evidence `user`), `assistant` `text`, `user` `what day was it again?`. Neither carries the sealed reasoning: fx's `SEALED-SIGNATURE-1` `reasoning` item went to Codex (`003-codex.json`) and not to the Gateway, xfx's signed `thinking` block went to llmux (`002-llmux.json`) and not to the Gateway.

## The session log

Both record the switch in the session: fx c1db919's v2 session writes the preferences into its `session.json` (`"provider":"codex","model":"gpt-5.6-sol"`, `v2.setPreferences`, `app_session_runtime.zig:3513-3521`); xfx writes the event upstream's event-log path writes (`:3539-3550`), `{"kind":"preferences_changed","model":"fake/model-a","permission_mode":null,"provider":"llmux"}`, between the two turns (`xfx-provider-carry-40x120-end.files`). Both profiles end on the new provider (`"provider":"codex"` / `"provider":"llmux"`), which is where both resumes continue.

## What does not match, and why

- **Provider rows and names** (`Codex subscription`/`gpt-5.6-sol`/272k against `llmux`/`fake/model-a`/200k): the products' providers.
- **fx's `* auth: Switched credential to AI_GATEWAY_API_KEY.`** (back): fx's Gateway row leads through its sign-in columns to a credential switch first; those are gated for xfx (#88) and xfx's row commits (D3 receipt).
- **xfx's one-time `! session:` notice** (back `turn2`): fx sends a message's replay state only to the provider and model that produced it and says nothing on the screen (`projectProviderReplay`, `types.zig:1548-1563`; a debug trace, `builtins/gateway.zig:183-185`). xfx drops the same state and, by its own rule (`.prd/04-providers.md` §Provenance), says so -- once per conversation. Tension in `ssot.md`.
- **fx's file-evidence user message**: fx's feature, absent from xfx's requests on every wire; not this unit's.

## Translation table

What each message kind of xfx's history is on each wire, and fx's rule for it (unit cases: `src/session/store/carry_tests.rs`, `src/gateway/tool_call_ids.rs`).

| Message kind | Gateway shape | llmux shape | fx's rule |
|---|---|---|---|
| system | `{"role":"system","content":"…"}` | top-level `system`, sections joined | each wire's own place (`writeChatMessageJsonInner`, `vercel_protocol.zig:753-777`; Codex `instructions`, `openai_codex.zig:65-73`) |
| user text | `user` `[{"type":"text"}]` | `user` `[{"type":"text"}]` | portable content, each wire's part |
| user image | `file` part after the text, `mediaType` + base64 | `image` block, base64 `source` | `appendUserImageParts` (`vercel_protocol.zig:778-817`); Responses `input_image` |
| assistant text | `assistant` `text` part | `assistant` `text` block | portable |
| assistant tool call | `tool-call` `toolCallId` `toolName` `input` | `tool_use` `id` `name` `input` | portable; **the id projected** to `fx_<40 hex>_<n>` when not 1-64 of `[A-Za-z0-9_-]` (`tool_call_ids.zig:14-109`), on every wire |
| tool result | `tool` `tool-result` `output` `text` | `user` `tool_result` leading the message, `content` string | portable, id projected the same |
| tool result, failure | `output` `error-text` | `is_error: true` | `error-text` (`vercel_protocol.zig:717-751`) |
| tool result, refusal | `output` `execution-denied` `reason` | `is_error: true`, the refusal as content | `execution-denied` (`:742-744`) |
| tool result with images | `content` output, the images in a `user` message after the run of results | `tool_result` `content` array, `image` blocks | `write_tool_image_user_message` (`:679-713`) |
| steer | `user` text | `user` text block, merged after the results | a plain user message (`restoredSteeringChatMessage`, `session.zig:2878-2885`; `../f4/fx-interrupt-steer-40x120-bodies/005-completion.json`) |
| compaction checkpoint | `user` text, the rendered summary | the same | a user message of text (`formatCompactedContinuationMessage` → `Message.userOwned`, `session.zig:2679-2683`) |
| llmux's signed `thinking` (RawBlocks) | **dropped**; the step's text and calls go | replayed verbatim, signature and all | `projectProviderReplay`: kept only for the provider and model that produced it (`types.zig:1512-1515`, `:1548-1563`) |
| a call inside replayed blocks | -- (blocks reduced to text) | never renamed; a rename that would reach one is refused | `ProtectedToolCallId` (`tool_call_ids.zig:54-56`) |
