# W9 receipt: generated session titles

Six scenarios in `scripts/fxcap/scenarios/`, each run at 40x120 on both
binaries with the repo's driver:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 .prd/tui-phase4/receipts/w9
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 .prd/tui-phase4/receipts/w9
```

The driver's `titled <title.json>` verb makes the fake gateway answer a
session-title request (a POST whose body carries "Generate a short title")
from that file -- `{chunks, delayMs, status, body}`, a turn's fields --
instead of the finish-only stream it answers with by default. A title request
consumes no turn of the script, and its log line names the model it was
asked of (`ai-language-model-id`), never the bearer.

| scenario | what it drives |
|---|---|
| `titles` | `statusLine.session` seeded on; the title request answered `"Fix the ` + `renderer loop"\n` + `because the prompt asked` (`titles-answer.json`: a quoted first line and a second line, so the cleaning shows); one prompt answered `The renderer answer.`; `/new`; `/resume`. `gwlog`, `home files`. |
| `titles-off` | the same, with `session_titles: false` seeded: the `/settings` row's off. |
| `titles-settings` | `/settings` bare, `titles` typed into its search, Right (off), Right (on); `home` after each change. |
| `titles-steer` | every title request refused (`titles-refused.json`, 500), so no title comes back and the gate stays open; a first prompt whose answer is held open four seconds (`titles-steer-turns.json`), and a second prompt typed into it 1.5 s in -- a steer (F4) -- answered `Took the steer.`; then `/trace`. `reports`, `gwlog`. |
| `titles-rename` | the title answered `Generated Name` two seconds late (`titles-late-answer.json`); a first prompt whose answer is held open five seconds; `/rename Mine` typed one second in, while the turn runs; then `/trace`. `reports`, `gwlog`, `home files`. |
| `titles-idle` | `titles`, with the first prompt typed 14 s after launch instead of 3 s; then `/trace`. `reports`, `gwlog`. Taken at round 5, on plan `276e805`, both sides. |

The fx side was taken once, at round 3. The xfx side was retaken at round 4:
`target/debug/xfx` built from the tree of the round-4 commit (W9 at `51fa35a`
plus round 4's title drain and network records); its banner names `51fa35a`
because those changes were not yet committed when it was built. The same shims, `FX_NO_OPEN_BROWSER=1`, the fx sound guard, an isolated tmux
socket and an empty `HOME` as every capture; `xfx: env XFX_SOUND=off` gives
xfx the start fx's `FX_SOUND=0` gives fx. No file was edited after capture;
`.files` dumps and reports write the run's scratch directory as `<scratch>`
(the driver's own rule). `.ansi` files are tmux's re-serialization of cell
attributes (`capture-pane -e`), not the programs' wire bytes (see
`../c0/README.md`).

## `titles`

| snap | fx vs xfx |
|---|---|
| `launch` (+ `.title`) | rows 2-40 byte-identical; row 1 is each product's own banner. Window title `fx v0.0.13 \| work` / `xfx v0.1.0 \| work`: no session yet, so each names its build and the workspace (`syncTerminalTitleWith`, `app_session_runtime.zig:3608-3628`). |
| `answered` (+ `.title`) | rows 2-40 byte-identical. The status line's session segment is the generated title on both. **Window title `Fix the renderer loop` on both** -- the model's first line, quotes off, the second line dropped (`sanitizeGeneratedTitle`, `session_title_generation.zig:67-87`). |
| `resume` | rows 2-40 byte-identical: the `/resume` picker lists the titled session as `Fix the renderer loop` on both. |
| `bodies/001-title.json` | **byte-identical** between fx and xfx: upstream's instructions as the system message, the prompt as the user's, `"tools":[]`, `"toolChoice":{"type":"none"}`, `"maxOutputTokens":128`. The title request is the first on both sides (`001-title`, `002-completion`): asked at the keystroke, ahead of the turn's own request. |
| `gateway.log` | `title ... model=openai/gpt-5.6-luna -> text` on both, then the completion. fx also reads the model catalog at launch, which xfx reads only when something needs it (`ssot.md` Tensions). |
| `files.files` | fx: `session.json` carries `"title":"Fix the renderer loop"` (`renameConversation`). xfx: `events.jsonl` carries `{"kind":"session_renamed","title":"Fix the renderer loop","source":"generated"}` and `session.json` publishes the same title. |

## `titles-off`

| snap | fx vs xfx |
|---|---|
| `gateway.log`, `bodies/` | **no title request on either side**: `001-completion.json` alone. |
| `answered` (+ `.title`), `resume` | rows 2-40 byte-identical, window titles identical: the title the first prompt gives the session, `fix the renderer loop, it redraws every row`. Since D1's derivation (first eight words, `session_display_metadata.zig:33-147`) the derived titles agree too, where round 1's differed. |

## `titles-settings`

| snap | fx vs xfx |
|---|---|
| `bare` | `Settings 14` on both: with D4d's rows, `Session titles` makes xfx's menu fx's fourteen, in fx's order (`settings_catalog.zig:261-276`). Rows 2-40 byte-identical except `Sound level`, which xfx draws `off  on` without `max` (`ssot.md` Tensions). The `Session titles` row is byte-identical in the `.ansi`, `on` bold: on by default. |
| `filtered`, `off`, `on` | rows 2-40 byte-identical: `titles` finds the row alone; Right flips it and both say `* session titles: saved to user settings (scope=user)`. |
| `off.files`, `on.files` | the same key and values on both, `{"session_titles":false}` then `true`, each by its own profile writer. |

## `titles-steer`: a steer asks at its own keystroke

| snap | fx vs xfx |
|---|---|
| `gateway.log`, `bodies/` | **the same four requests in the same order on both**: `001-title`, `002-completion`, `003-title`, `004-completion`. The first prompt asks at its keystroke; its 500 leaves the session untitled and no turn has ended, so the prompt steered into the running turn asks again at its own keystroke (fx: `snapshotAndAdmitInteractivePromptWithSkillBindings` -> `snapshotPrompt` -> `admitInteractivePrompt` -> `admitPrompt(steer_if_active)`, `src/main.zig:1407-1420`, `:1479-1481`, `worker_runtime.zig:966-968`, `:1064-1104`). |
| `steered` (+ `.title`) | rows 2-40 byte-identical; window title `fix the renderer loop` on both: no generated title, the derived one. |
| `report-1.md` `## Session Title` | identical but for the session id and the elapsed time: `setting: true`, `title: fix the renderer loop`, `generation: status=failed session=<id> model=openai/gpt-5.6-luna reason=provider_failure detail=server_error elapsed=<n>ms` (`app_commands.zig:2779-2816`; the detail is fx's `FailureKind` tag for a 500, `builtins/gateway.zig:710-723`). `## Problems` carries `- session title generation failed reason=provider_failure detail=server_error` on both (`:2596-2611`). |
| `report-1.md` `## Network Calls`, `## Problems` | both list the two refused title requests beside the turn's, in the order they were made: fx `model=openai/gpt-5.6-luna source=parent status=500 duration=14ms ...` in its own line format, xfx `! network: provider: gateway · model: openai/gpt-5.6-luna · 12ms failed: server_error` in the turns' record's; both put the refusals under `## Problems` too. Each section keeps its own product's summary line and per-call format. |

## `titles-rename`: fx's install rule against a rename typed mid-turn

| snap | fx vs xfx |
|---|---|
| `renamed` (+ `.title`) | rows 2-40 byte-identical: `* session: renamed to "Mine"`, the answer, the summary, and `auto · model-a · Mine` in the status line; window title `Mine` on both. The title came back after the rename and found the user's title on record (`installGeneratedTitle`, `session_title_generation.zig:378-394`). xfx writes a rename typed during a turn once the turn ends (D1); the answer read meanwhile is dropped all the same. |
| `report-1.md` `## Session Title` | identical but for the session id: `title: Mine`, `generation: status=dropped session=<id> model=openai/gpt-5.6-luna reason=user_title_present elapsed=<n>ms` -- fx 4027 ms, xfx 4017 ms: both read the answer's body to its end after the finish, which the fake ends two seconds later (fx's pooled drain, `client.zig:1777-1810`, budget `:218-221`) -- and `- session title generation dropped reason=user_title_present` under `## Problems`. `## Network Calls` lists the title request (about 4 s, answered) before the turn's on both. |
| `files.files` | fx's `session.json` title `Mine`; xfx's `events.jsonl` has one `session_renamed`, the user's `Mine`: the generated title was never written. |

## `titles-idle`: fx's title lost on the connection it warmed at launch

S1's `skills` capture found fx sending no title request for a first prompt
(`$demo run it`) where xfx sent one. fx's gate is not why: fx asks for that
prompt, and for any, when it is typed soon after launch, and loses the
request when it is typed later. Experiments, fx only, gateway log's `title`
lines: `$demo run it` as the first thing typed asks; a plain prompt in the
skills workspace asks; each third of S1's `/skills` commands, then a plain
prompt, asks; any two thirds, then a plain prompt, do not; **no commands at
all and 6 s of waiting asks, 14 s does not.**

| snap | fx vs xfx |
|---|---|
| `gateway.log` | fx: the catalog `GET`, then `completion` -- **no `title` line**. xfx: `title`, then `completion`. |
| `report-1.md` `## Session Title` | fx asked: `generation: status=failed session=<id> model=openai/gpt-5.6-luna reason=transport_error detail=HttpConnectionClosing elapsed=13ms`, and its `## Network Calls` has the title call at `err=HttpConnectionClosing duration=13ms bytes=0`. xfx: `status=installed`. |
| `answered` (+ `.title`) | rows 2-40 identical but the status line's session segment: fx keeps the derived title (`fix the renderer loop, it redraws every row`), xfx shows the generated one (`Fix the renderer loop`). |

Why, from fx's own debug log (`FX_TRACE_LOG`, an experiment run of this shape
that failed as `ReadFailed`):

```
[gateway] pool_warm result=ok elapsed_ms=2                                   (launch)
[gateway] event=pool_borrow attempt=1 free_connections=1                     (+17 s: the title call)
[gateway] event=receive_head_error attempt=1 err=ReadFailed
[session] event=title_generation result=unavailable err=ReadFailed
[gateway] event=pool_borrow turn_id=1 step_id=1 attempt=1 free_connections=0 (the turn: a new connection)
```

- fx opens one connection to the Gateway at launch and parks it in its pool
  without sending anything on it (`warmAsync`, `src/main.zig:706`; `warm`,
  `src/core/shared/http_pool.zig:76-102`). It reuses a parked connection idle
  for up to 30 s (`default_ttl_ms`, `:14`; `drainIfStale`, `:144-160`).
- The fake resets a connection that has sent no request for about 10 s,
  although it keeps a used keep-alive connection open (Bun 1.3.14, `fakegw.ts`
  with `idleTimeout: 0`). A probe against the fake: a connection left silent
  for 6 s and then sent a request is answered `200`; one left silent for 14 s
  is reset (`ECONNRESET`); a connection that already carried one request and
  then waits 14 s is answered `200`.
- The first prompt's title call is the first request, so it borrows that dead
  connection, and it makes one attempt (`retry_count = 1`,
  `session_title_generation.zig:168`). It fails before the fake reads a byte.
  The turn after it borrows a fresh connection.

S1's first prompt comes after its `/skills` commands, well past 10 s. xfx does
not match this: its title question makes its own connection, and a request
xfx sends reaches the Gateway. Copying fx's loss would mean failing a request
on purpose. This is a property of the capture harness, not of either
product's title rule. Against the real Gateway, whether the warm connection
survives depends on the Gateway's idle policy, which these captures do not
show. A capture that wants fx's title types its first prompt within about
10 s of launch, as `titles` does.

## What differs and is not W9's

- **`## Network Calls`' own lines.** fx's summary is `last= ok= errors= avg= min= max=` with session totals and per-turn rollups, and each call is `model= source= status= duration= bytes= ...` stamped when it started; xfx's is `calls= ok= refused=` and each call is its `* network:` record. A turn's request is stamped when the band hears of it, and a title's when it started. fx also lists `titles-steer`'s first turn stream, which the steer cut (`err=Cancelled`); xfx records no stream that settled with no answer (`network_record`). Those are `/trace`'s and F4's, not this workstream's.
- **`Sound level`'s `max`**, as in every capture.
