# W9 receipt: generated session titles

Three scenarios in `scripts/fxcap/scenarios/`, each run at 40x120 on both
binaries with the repo's driver:

```
FX_BIN=<fx c1db919 build, 0.0.13> scripts/fxcap/capture.sh fx  <scenario> 40x120 .prd/tui-phase4/receipts/w9
                                  scripts/fxcap/capture.sh xfx <scenario> 40x120 .prd/tui-phase4/receipts/w9
```

The driver gained one verb, `titled <title.json>`: the fake gateway answers a
session-title request (a POST whose body carries "Generate a short title") from
that file -- `{chunks, delayMs, status, body}`, a turn's fields -- instead of
the finish-only stream it always answered with, which is why every earlier
capture showed the title derived from the first prompt. A title request still
consumes no turn of the script, and its log line now names the model it was
asked of (`ai-language-model-id`), never the bearer. The research harness's
fake (`.prd/research`) is unchanged.

| scenario | what it drives |
|---|---|
| `titles` | `statusLine.session` seeded on; the title request answered `"Fix the ` + `renderer loop"\n` + `because the prompt asked` (`titles-answer.json`: a quoted first line and a second line, so the cleaning shows); one prompt (`fix the renderer loop, it redraws every row twice`) answered `The renderer answer.`; `/new`; `/resume`. `gwlog`, `home files`. |
| `titles-off` | the same, with `session_titles: false` seeded: the `/settings` row's off. |
| `titles-settings` | `/settings` bare, `titles` typed into its search, Right (off), Right (on); `home` after each change. |

The xfx binary is `target/debug/xfx` built at `11b0618` (W9 with
`plan/tui-phase4` 835b565 merged), which is what its banner says, run once
before the first capture so macOS's first-launch assessment was not inside
one. The same shims, `FX_NO_OPEN_BROWSER=1`, the fx sound guard, an isolated
tmux socket and an empty `HOME` as every capture; `xfx: env XFX_SOUND=off`
gives xfx the start fx's `FX_SOUND=0` gives fx. No file was edited after
capture; `.files` dumps write the run's scratch directory as `<scratch>` (the
driver's own rule). `.ansi` files are tmux's re-serialization of cell
attributes (`capture-pane -e`), not the programs' wire bytes (see
`../c0/README.md`).

## `titles`

| snap | fx vs xfx |
|---|---|
| `launch` (+ `.title`) | rows 2-40 byte-identical; row 1 is each product's own banner. Window title `fx v0.0.13 \| work` / `xfx v0.1.0 \| work`: no session yet, so each names its build and the workspace (`syncTerminalTitleWith`, `app_session_runtime.zig:3608-3628`). The session segment is on and empty: `auto · model-a` on both. |
| `answered` (+ `.title`) | rows 2-40 byte-identical except row 6, the turn's summary time (`2s` / `3s`, the clock). The status line's session segment is the generated title on both, byte for byte: `ESC[38;5;252mauto ESC[38;5;245m · model-a · Fix the renderer loop ESC[39m`. **Window title `Fix the renderer loop` on both** -- the model's first line, quotes off, the second line dropped (`sanitizeGeneratedTitle`, `session_title_generation.zig:67-87`). |
| `resume` | rows 2-40 byte-identical: `/new` put the titled session behind, and the `/resume` picker lists it as `Fix the renderer loop    work · now · 1 turn` on both. |
| `bodies/001-title.json` | **byte-identical** between fx and xfx: upstream's instructions as the system message, the prompt as the user's, `"tools":[]`, `"toolChoice":{"type":"none"}`, `"maxOutputTokens":128`. The title request is the first request on both sides (`001-title`, `002-completion`). |
| `gateway.log` | `title /v4/ai/language-model model=openai/gpt-5.6-luna -> text` on both, then the completion. fx also reads the model catalog at launch (`GET /coding-agent/v1/models`), which xfx reads only when something needs it (`ssot.md` Tensions, "The model catalog is read when something needs it"). |
| `files.files` | fx: the session's `session.json` carries `"title":"Fix the renderer loop"` (`renameConversation`). xfx: the session's `events.jsonl` carries `{"kind":"session_renamed","title":"Fix the renderer loop","source":"generated"}` (seq 7, after the turn's frames), and `session.json` publishes the same title. Neither product's usage counts the title request: fx's `usage-v2.json` has `request_count: 0` (its turn had no billing metadata either), xfx's turn frame has one request, the turn's. |

## `titles-off`

| snap | fx vs xfx |
|---|---|
| `gateway.log`, `bodies/` | **no title request on either side**: one `completion`, `001-completion.json` alone. |
| `answered` (+ `.title`) | rows 2-40 byte-identical except the summary time (`0s` / `2s`). The window and the status line keep the title the first prompt gives the session. |
| `resume` | rows 2-40 identical except the listed title: fx `fix the renderer loop, it redraws every row`, xfx `fix the renderer loop, it redraws every row twice`. **Not W9's**: that is the derived title, which fx cuts by its own rule (`deriveFromHistory`, `session_display_metadata.zig`) and xfx at 80 bytes (`store.rs` `title_of`); the window titles differ the same way (`.title`). D1's follow-up "fx title derivation" (`loop.md` R5) owns it. |

## `titles-settings`

| snap | fx vs xfx |
|---|---|
| `bare` | fx `Settings 14`, xfx `Settings 11`: xfx still leaves out `Fast mode`, `Ultra mode` and `Startup scrollback`, and draws `Sound level` as `off  on` (`ssot.md` Tensions). **The `Session titles` row is byte-identical**, in the same place (after `Permission mode`, before `Sound level`): `  ESC[38;5;245mSession titles ESC[39m           ESC[38;5;245moff ESC[39m  ESC[1m ESC[38;5;255mon ESC[0m` -- on by default. |
| `filtered` | `titles` finds the row alone on both; `.txt` rows 2-40 identical, and the `.ansi` rows identical once joined -- tmux puts one `ESC[39m` at the end of the header row on one side and at the start of the next row on the other. |
| `off`, `on` | Right flips it and both say `* session titles: saved to user settings (scope=user)`; rows as `filtered`. |
| `off.files`, `on.files` | fx `{"session_titles":false}` then `{"session_titles":true}` (plus a backup of the first); xfx the same key and values, written by its own profile writer (`{\n  "session_titles": false\n}\n`). fx's settings backups are not ported (`docs/parity.md` `/settings`). |
