# D2c receipt: `/model`'s speed column, `/fast` and `/ultrafast`, the speed on the wire, fx c1db919 beside xfx

Captured on 2026-10-07 with the repository's harness. Each run had its own tmux socket, an empty HOME, the fake key, and a loopback fake gateway. The catalog is the research one (`scenarios/models.json`): `openai/gpt-6-astra` with levels `low medium high xhigh`, Fast (its `priority` tier) and Ultra (its `ultrafast` tier); `anthropic/claude-opus-5` with `low medium high max` and Fast (`fast_options` toggle); `fake/model-a` and `xai/grok-4.7` with neither.

```
scripts/fxcap/capture.sh {fx,xfx} model-column     40x120 <dir>   # D2b's column, retaken: the ⚡︎ preview
scripts/fxcap/capture.sh {fx,xfx} model-speed      40x120 <dir>   # the speed column, the turns at Ultra and Fast, the explicit form
scripts/fxcap/capture.sh {fx,xfx} model-anchor     40x120 <dir>   # the column under a /model line the caret has left (Alt-Enter)
scripts/fxcap/capture.sh {fx,xfx} fast-unsup       40x120 <dir>   # /fast and every /ultrafast form on a model with neither
scripts/fxcap/capture.sh {fx,xfx} fast-opus        40x120 <dir>   # /fast on and off, /ultrafast on refused, on a model with Fast
scripts/fxcap/capture.sh {fx,xfx} fast-astra       40x120 <dir>   # Ultra on, its status, /fast over it, a model change turning both off
scripts/fxcap/capture.sh {fx,xfx} fast-unconfirmed 40x120 <dir>   # a turn in Ultra whose finish confirms no tier
scripts/fxcap/capture.sh {fx,xfx} fast-unavailable 40x120 <dir>   # a turn in Fast while the catalog cannot be read
scripts/fxcap/capture.sh {fx,xfx} fast-override    40x120 <dir>   # /fast under FX_MODEL / XFX_MODEL: the save's report above the change
```

All but `model-column` are new in this unit; `fast-unsup`, `fast-opus`, `fast-astra` and `fast-unconfirmed` are the account-model research's (§5-§6), ported, with one fix: `fast-astra` waits between Escape and Enter -- sent together they are Alt-Enter, a line break, so the research capture's "model change clears" never changed the model (that capture is what `model-anchor` now records on purpose). One harness change: `fakegw.ts` takes an optional `serviceTier` per turn, which the finish event confirms in `providerMetadata.gateway.serviceTier` (fx's `parseSseServiceTier`, `gateway/client.zig:3451-3463`); `model-speed`'s turns confirm their tier, `fast-unconfirmed`'s does not. `fast-unavailable` (added with the follow-up) seeds Fast on and bound for `claude-opus-5` and serves a catalog that does not parse (`scenarios/models-broken.json`), so fx's catalog read fails (`isModelCacheFailed`) and the turn goes at standard speed. The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1` are unchanged.

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` at `901471d` (the banner's revision): this branch after merging `plan/tui-phase4` (`2b5967f` and later, merge `7909f83`). **Retaken at `5b3d292`** (the follow-up: plan `835b565` merged at `9e7a51e`, then the operational lines, the speed save and the restored binding): every `fast-unconfirmed`, `fast-unavailable`, `fast-astra` and `fast-opus` file, fx's beside them retaken in the same run. **Round 2, retaken at `06def9e`** (the provider on the session frames at `bcb4b1d`, then a speed change said once its save answers and every pick writing fx's fields): every `fast-override` (new), `fast-opus`, `fast-astra`, `fast-unconfirmed`, `fast-unavailable` and `model-speed` file, fx's again beside them in the same run; the verdicts below are this run's where it covers them. Both were started once before the captured set. Each snap has a `.txt` and an `.ansi` (`capture-pane -e`); `*.files` are the `home` verb's dumps. "Rows identical" counts the non-blank rows **after the banner** that are byte-equal in the two `.ansi` files. `wire.txt` lists every request body's keys other than `prompt` and `tools` (the bodies themselves are not kept).

**Sanitized**: every file was scanned for the run directory (`fxcap.*`), home and temp paths, the user name and the fake key. None matched; the harness's own `.log` files are not copied (the fake gateway's logs are).

## Verdicts, 40x120

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `model-column space`, `filter-gpt`, `left`, `esc`, `menu-enter-left` | **8/8, 5/5, 5/5, 2/2, 5/5** | none |
| `model-column gpt-enter` | **9 of 9** | none -- D2b's 8 of 9: the footer previews `⚡︎` for astra's marked Fast from the moment its levels open |
| `model-column effort-down` | **9 of 9** | none (D2b: 8 of 9) |
| `model-column menu-enter` | **9 of 9** | none (D2b: 8 of 9) |
| `model-speed effort` | **9 of 9** | astra's levels, `auto · gpt-6-astra · ⚡︎` |
| `model-speed speed-open` | **7 of 7** | Enter on `default`: `/model openai/gpt-6-astra auto `, `normal fast ultrafast` from column 34, `fast` marked |
| `model-speed speed-down` | **7 of 7** | `ultrafast` marked, the footer's `⚡︎` in `\e[38;2;255;204;0m` |
| `model-speed speed-tab` | **5 of 5** | Tab writes `/model openai/gpt-6-astra auto ultrafast` |
| `model-speed speed-left` | **9 of 9** | Left back to the levels, the session's Normal marked: no `⚡︎` |
| `model-speed speed-space` | **7 of 7** | `high` and a space open the speeds again |
| `model-speed commit` | 2 of 3 | `* Switched to openai/gpt-6-astra (effort: high, speed: ultrafast)`, byte-equal. The footer row's text and colours are byte-equal; fx's row ends without the `\e[39m` that its other frames of the same row end with (`speed-down`), i.e. the blank cells after the sign keep fx's statusline grey in this frame -- invisible, and fx's own frame-to-frame difference |
| `model-speed turn-ultra` | **6 of 6** | the answer, at Ultra; the request carried `providerOptions.gateway.only ["openai"]` and `providerOptions.openai.serviceTier "ultrafast"` from both (`wire.txt`) |
| `model-speed fast-on` | **7 of 7** (round 2; 6 of 7 before, fx's trailing-cell difference, which this run's fx frame did not show) | `* fast: on`, the footer `· high · ⚡︎` in the row's grey |
| `model-speed turn-fast` | 9 of 10 (round 2: 10 of 10, the same second) | the answer at Fast, `providerOptions.gateway.speed "fast"` from both; the differing row is the turn's elapsed seconds (`0s` / `1s`) |
| `model-speed explicit-bad` | 13 of 14 (round 2: 14 of 14) | `/model anthropic/claude-opus-5 high ultrafast` pasted: `✗ usage: /model <id> <effort> [normal|fast|ultrafast]`, composer kept; the elapsed-seconds row above |
| `model-speed explicit-ok` | 11 of 12 | `/model openai/gpt-6-astra xhigh fast`: `* Switched to openai/gpt-6-astra (effort: xhigh, speed: fast)` (the research's `paste-ok`); the elapsed-seconds row |
| `model-anchor typed`, `alt-enter` | **5/5, 6/6** | after an Alt-Enter the column (`no matching models`) hangs from the caret row's start, column 2 (`projectedAnchorColumn`) -- xfx drew it at column 9 before this unit |
| `fast-unsup fast-typed`, `fast`, `fast-on-arg`, `uf-typed`, `uf-bare`, `uf-space`, `uf-forms` | **7/7, 3/3, 4/4, 8/8, 5/5, 5/5, 9/9** | the menu rows `/fast  toggle Fast mode when supported` and `/ultrafast  request Ultra mode when supported`; `* fast: This model does not come with a fast mode.`; `/fast on` is `✗ command: Unknown command. Try /help.`; `* ultrafast: requested: off` (bare and `status`); `! ultrafast: Ultra mode is unavailable for this model and may increase cost.`; `* ultrafast: requested off`; `✗ usage: /ultrafast [on|off|status]`; no argument picker after `/ultrafast ` |
| `fast-opus start`, `fast-on`, `fast-off`, `uf-on-unsup` | **2/2, 3/3, 4/4, 5/5** | `* fast: on` with `auto · opus 5 · ⚡︎`, `* fast: off`, Ultra refused with the warning |
| `fast-astra start`, `uf-on`, `uf-status`, `fast-after-uf`, `model-change-clears` | **2/2, 3/3, 4/4, 5/5, 7/7** | `* ultrafast: requested on` and the yellow `⚡︎`; `requested: on`; `/fast` turns Ultra off (grey `⚡︎`); `/model xai/grok-4.7` then `Switched to xai/grok-4.7 (effort: default, speed: normal)` and no `⚡︎` |
| `fast-unconfirmed turn` | 6 of 8 | `Ultrafast was requested, but Gateway did not confirm it was served; this response may have used a standard or lower tier.` in the answer, directly above `MARKER-ULTRA-UNCONFIRMED` with no blank row, once (`orchestrator.zig:8015-8022`) -- from both; the two rows it wraps onto differ by one word: fx's `… a standard or` / `lower tier.`, xfx's `… a standard or lower` / `tier.` (below) |
| `fast-unavailable start`, `turn` | **2/2, 6/6** | `auto · opus 5 · ⚡︎` from the seed; the turn says `Fast mode is unavailable for this model right now; continuing at standard speed.` directly above `MARKER-STANDARD-SPEED` (`orchestrator.zig:7253-7262`), and the request carries no speed from either (`wire.txt`) |
| `fast-override start`, `fast-on`, `fast-off` | **2/2, 4/4, 6/6** | round 2, new: under a process override of the model, `* fast: saved to user settings (scope=user); fresh sessions here use higher-precedence model=process_override` is the line **above** `* fast: on`, and again above `* fast: off` -- fx persists before it announces (`applyFastMode`, `session_commands.zig:781-797`); xfx's band holds the announcement until its writer thread's save answers (`src/tui/speed_save.rs`) |

## The wire

`wire.txt`. Ultra: `"providerOptions":{"gateway":{"only":["openai"]},"openai":{"serviceTier":"ultrafast"}}`; Fast: `"providerOptions":{"gateway":{"speed":"fast"}}`; the effort `"reasoning":"high"` beside them -- from both. fx's bodies also carry `providerOptions.gateway.caching:"auto"`, `maxOutputTokens` and (title) a separate title request: other units' features, unchanged here. llmux is not captured (fx has no llmux); llmux reads no per-request speed (`2lab-ai/llmux src/config/schema.rs:545-550`: Fast is the daemon's codex shape setting; `src/provider/codex.rs:422-437`; its `/models` publishes no speed), so no llmux row offers one and xfx sends nothing there (`src/llmux/protocol.rs`'s `body_with` test pins the absence).

## Settings after each write

Every dump agrees key for key (fx writes compact JSON in insertion order, xfx pretty JSON in sorted order):

| After | fx `~/.fx/settings.json` | xfx `~/.xfx/settings.json` |
|---|---|---|
| `model-speed after-commit` | `effort "high"`, `fast_mode false`, `ultrafast_mode true`, `fast_mode_model_bound true`, astra | the same |
| `model-speed after-fast` | `fast_mode true`, `ultrafast_mode false` | the same |
| `model-speed end` | `effort "xhigh"`, `fast_mode true` (the explicit form's `fast`) | the same |
| `fast-opus fast-on` | `fast_mode true`, `ultrafast_mode false`, bound | the same |
| `fast-astra uf-on` / `fast-after-uf` / `end` | Ultra on / Fast on / both off with grok | the same |
| `fast-unsup end` | `fast_mode false`, `ultrafast_mode false`, bound (written by `/ultrafast off`) | the same |

Round 2 (`06def9e`): every `fast-override` and `model-speed` dump parses to fx's too -- under the override the model in force (`anthropic/claude-opus-5`) is written beside the switches, as fx's is. Retaken at `5b3d292`: every `fast-astra` and `fast-opus` dump parses to the same JSON value as fx's, `models.gateway` and `provider` included -- a speed save now writes the model in force and the provider beside the switches, as fx's `persistPreferenceTargets` does (`session_commands.zig:781-791`).

xfx's home also holds `settings.lock` (0 bytes), the cross-process lock every settings write takes -- now `xfx setup`'s too.

## Round 3 (`ba7fd53`, after merging plan `20d1482`)

- **`/settings`' model list** (D4d's `settings-model-list`, its `used` snap copied here): Left on `anthropic/claude-opus-5` says `* Switched to anthropic/claude-opus-5 (effort: default, speed: normal)` then `* model picker: saved to user settings (scope=user); fresh sessions here use higher-precedence model=process_override`, in fx's order (`selectModelFromPicker` announces before it persists, `session_commands.zig:913-914`), and the profile now parses to fx's exactly: `{"models":{"gateway":"anthropic/claude-opus-5"},"provider":"gateway","effort":"auto","fast_mode":false,"ultrafast_mode":false,"fast_mode_model_bound":true}` -- the leveled row's effort is the session's, as `applyInlineSettingsModelSelection` passes `app.effort` (`app_input_runtime.zig:2517-2529`). D4d's receipt recorded `{"models":…}` alone before.
- **`/sound`** says a failed save before its level now (`app_commands.zig:4014-4027`); a capture cannot make the profile unwritable without a seed the harness does not have, so `src/tui/shell.rs` `a_save_that_failed_says_so_before_the_level` is the receipt. The `sound` scenario retaken at `ba7fd53` shows the same notice rows as D4c's (`bare` 6/6, `on` 7/7, `off` 8/9 -- the one differing row is the `max` line above it, `* sound: max` against xfx's usage line, D4c's by-design difference); not copied.
- **A reset behind a held save**: `src/tui/event_loop.rs` `a_reset_hands_to_scrollback_what_a_held_speed_save_said_before_it` -- the save's line and `* fast: on` land on the old screen, and the handoff's scroll carries them; `src/tui/shell.rs` `a_reset_typed_behind_a_save_in_flight_waits_for_it_and_keeps_what_it_said_for_the_old_screen` with the real writers. A capture types too slowly to land a `/clear` inside a save, so none is kept for it.
- `fast-override` and `fast-opus` retaken at `ba7fd53`: every row and every settings dump as round 2's.

## Round 4 (`bcd72b8`, on plan `82400635`): `/model` asks again mid-turn

```
scripts/fxcap/capture.sh {fx,xfx} model-reask 40x120 <dir>
```

The catalog cannot be read at launch -- the fake gateway's `/models` answers 503 for its first six seconds (the harness's new `modelsdown <ms>` verb, `FAKEGW_MODELS_DOWN_MS` in `fakegw.ts`) -- a turn starts and keeps streaming (`hangMs`), and `/model` asks again while it runs. fx starts the load on a thread of its own at once (`ensureModelCache` from `openModelBrowseCatalog`, `app_input_runtime.zig:2346-2349`; again after a failure, `beginLoad`, `model_cache_runtime.zig:405-458`) and the open menu takes the rows when they land (`pollLoadTransition`, `:549-557`). xfx's ask is now a control message (`TurnControl::Catalog`) whose read starts beside the turn.

| Snap | Rows identical | What differs, and why |
|---|---|---|
| `start` | **2/2** | -- |
| `menu-failed` (`/model` at 2.5 s, still down) | **6/6** | `Models 0  [All]` and `Could not reach AI Gateway; retry /model.` from both |
| `turn-running` | 3 of 4 | the activity row's clock (`3s` / `4s`) and its pulse phase |
| `reask-midturn` (`/model` while the turn streams, the gateway back) | 11 of 12 | `Models 4  [All]  Anthropic  OpenAI  xAI  Others`, the four rows and the note, **with the turn still `Generating`** -- from both; the differing row is the activity row's pulse (`•` in fx's frame, between pulses in xfx's) |

The gateway logs are line for line the same: the launch read and the first `/model` answered 503, the turn's title and completion requests, then the mid-turn read served -- before the turn ended. Before this round xfx queued that read behind the turn, so its menu said `Loading models…` until the turn was over (`src/tui/worker.rs` `a_catalog_asked_for_again_while_a_turn_runs_is_read_at_once_and_told_before_it_ends` fails with the request put aside until the work ends, which is what queueing it amounted to).

## What does not match, and why

- **The unconfirmed-Ultra line's wrap** (`fast-unconfirmed`): closed in the follow-up but for one word. fx's assistant wrap keeps no word alone on a line's last row -- when the word that overflows is the line's final one, the word before it moves down too (`assistant_wrap.zig:219-267`, `:269-298`, `isFinalWordOnLine` `:862-883`) -- and `tui::wrap::wrap` has no such rule, so at 120 columns xfx fits `lower` on the first row. The rule is fx's for every assistant row; recorded in `ssot.md` Tensions for the prose wrap.
- **The trailing blank cells of the footer row** (`commit`, `fast-on`): fx's own frames differ from each other there; text and colours match.
- **The elapsed seconds** of a turn: a clock.
- **The banner**: xfx's brand line (WU-A).
