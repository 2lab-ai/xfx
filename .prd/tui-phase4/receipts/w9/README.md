# W9 receipt: generated session titles

Five scenarios in `scripts/fxcap/scenarios/`, each run at 40x120 on both
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

The xfx binary is `target/debug/xfx` built from the tree of the commit that
adds this README (W9 round 3 with `plan/tui-phase4` 8891d04f merged, at
`bcb13ad`, plus that commit's `/trace` detail and `session=` change); its
banner names `bcb13ad` because the change was not yet committed when it was
built. The
same shims, `FX_NO_OPEN_BROWSER=1`, the fx sound guard, an isolated tmux
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

## `titles-rename`: fx's install rule against a rename typed mid-turn

| snap | fx vs xfx |
|---|---|
| `renamed` (+ `.title`) | rows 2-40 byte-identical: `* session: renamed to "Mine"`, the answer, the summary, and `auto · model-a · Mine` in the status line; window title `Mine` on both. The title came back after the rename and found the user's title on record (`installGeneratedTitle`, `session_title_generation.zig:378-394`). xfx writes a rename typed during a turn once the turn ends (D1); the answer read meanwhile is dropped all the same. |
| `report-1.md` `## Session Title` | identical but for the session id and the elapsed time: `title: Mine`, `generation: status=dropped session=<id> model=openai/gpt-5.6-luna reason=user_title_present elapsed=<n>ms`, and `- session title generation dropped reason=user_title_present` under `## Problems`. |
| `files.files` | fx's `session.json` title `Mine`; xfx's `events.jsonl` has one `session_renamed`, the user's `Mine`: the generated title was never written. |

## What differs and is not W9's

- **Elapsed.** fx's `elapsed` in `titles-rename` (about 4 s) counts to the fake's `[DONE]`, which comes `delayMs` after the finish event; xfx's (about 2 s) stops at the finish event, as its gateway client does for every stream.
- **`## Network Calls`.** fx's lists every request its gateway client made, the title calls among them (`model=openai/gpt-5.6-luna ... status=500`), and puts the failed ones under `## Problems`; xfx's section is its own summary of the turn's calls (`calls=1 ok=1 refused=0`) and does not list side calls. That section belongs to `/trace`, not to this workstream's port.
- **`Sound level`'s `max`**, as in every capture.
