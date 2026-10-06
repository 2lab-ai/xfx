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
```

All but `model-column` are new in this unit; the four `fast-*` are the account-model research's (§5-§6), ported, with one fix: `fast-astra` waits between Escape and Enter -- sent together they are Alt-Enter, a line break, so the research capture's "model change clears" never changed the model (that capture is what `model-anchor` now records on purpose). One harness change: `fakegw.ts` takes an optional `serviceTier` per turn, which the finish event confirms in `providerMetadata.gateway.serviceTier` (fx's `parseSseServiceTier`, `gateway/client.zig:3451-3463`); `model-speed`'s turns confirm their tier, `fast-unconfirmed`'s does not. The shims (`open`, `pbcopy`, `pbpaste`, `afplay`, `osascript`) and `FX_NO_OPEN_BROWSER=1` are unchanged.

`fx` is the c1db919 build. `xfx` is `target/debug/xfx` at `901471d` (the banner's revision): this branch after merging `plan/tui-phase4` (`2b5967f` and later, merge `7909f83`). Both were started once before the captured set. Each snap has a `.txt` and an `.ansi` (`capture-pane -e`); `*.files` are the `home` verb's dumps. "Rows identical" counts the non-blank rows **after the banner** that are byte-equal in the two `.ansi` files. `wire.txt` lists every request body's keys other than `prompt` and `tools` (the bodies themselves are not kept).

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
| `model-speed fast-on` | 6 of 7 | `* fast: on`, the footer `· high · ⚡︎` in the row's grey; the same trailing-cell difference as `commit` |
| `model-speed turn-fast` | 9 of 10 | the answer at Fast, `providerOptions.gateway.speed "fast"` from both; the differing row is the turn's elapsed seconds (`0s` / `1s`) |
| `model-speed explicit-bad` | 13 of 14 | `/model anthropic/claude-opus-5 high ultrafast` pasted: `✗ usage: /model <id> <effort> [normal|fast|ultrafast]`, composer kept; the elapsed-seconds row above |
| `model-speed explicit-ok` | 11 of 12 | `/model openai/gpt-6-astra xhigh fast`: `* Switched to openai/gpt-6-astra (effort: xhigh, speed: fast)` (the research's `paste-ok`); the elapsed-seconds row |
| `model-anchor typed`, `alt-enter` | **5/5, 6/6** | after an Alt-Enter the column (`no matching models`) hangs from the caret row's start, column 2 (`projectedAnchorColumn`) -- xfx drew it at column 9 before this unit |
| `fast-unsup fast-typed`, `fast`, `fast-on-arg`, `uf-typed`, `uf-bare`, `uf-space`, `uf-forms` | **7/7, 3/3, 4/4, 8/8, 5/5, 5/5, 9/9** | the menu rows `/fast  toggle Fast mode when supported` and `/ultrafast  request Ultra mode when supported`; `* fast: This model does not come with a fast mode.`; `/fast on` is `✗ command: Unknown command. Try /help.`; `* ultrafast: requested: off` (bare and `status`); `! ultrafast: Ultra mode is unavailable for this model and may increase cost.`; `* ultrafast: requested off`; `✗ usage: /ultrafast [on|off|status]`; no argument picker after `/ultrafast ` |
| `fast-opus start`, `fast-on`, `fast-off`, `uf-on-unsup` | **2/2, 3/3, 4/4, 5/5** | `* fast: on` with `auto · opus 5 · ⚡︎`, `* fast: off`, Ultra refused with the warning |
| `fast-astra start`, `uf-on`, `uf-status`, `fast-after-uf`, `model-change-clears` | **2/2, 3/3, 4/4, 5/5, 7/7** | `* ultrafast: requested on` and the yellow `⚡︎`; `requested: on`; `/fast` turns Ultra off (grey `⚡︎`); `/model xai/grok-4.7` then `Switched to xai/grok-4.7 (effort: default, speed: normal)` and no `⚡︎` |
| `fast-unconfirmed turn` | 2 of 8 | **not implemented**: fx says `Ultrafast was requested, but Gateway did not confirm it was served; this response may have used a standard or lower tier.` above the answer when the finish event confirms no `ultrafast` tier (`orchestrator.zig:8015-8022`), once per turn; xfx says nothing. The rows below shift by two. See below. |

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

xfx's home also holds `settings.lock` (0 bytes), the cross-process lock every settings write takes -- now `xfx setup`'s too.

## What does not match, and why

- **The unconfirmed-Ultra line** (`fast-unconfirmed`): fx parses `providerMetadata.gateway.serviceTier` from the finish event and, for a turn in Ultra that the Gateway did not confirm as `ultrafast`, writes the sentence above as operational text, once per turn. xfx's gateway client does not read the served tier, so it says nothing. Not built in this unit: it needs the served tier carried out of the v4 stream parser and an operational line placed above the answer, as fx's is. Reported for the backlog.
- **The trailing blank cells of the footer row** (`commit`, `fast-on`): fx's own frames differ from each other there; text and colours match.
- **The elapsed seconds** of a turn: a clock.
- **The banner**: xfx's brand line (WU-A).
