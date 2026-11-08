# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-a2c0265 [debug] · Run /help for commands`, sha256 `b45ca570cab20e50a32c251c130b20252421a03d623ac641214b31fa695f00fd`
- size 40x120, `--jobs 2`, 48 scenarios -- **PARTIAL**: `--only credits-busy fulltranscript-notice h1-mcphttp h1-mcphttp-fail h1-mcphttp-methodnotfound h1-models2 h1-rehomefile h1-usage h1-usage-codex h1-usage-grok image-paste interrupt-steer model-busy model-busy-known notice-midstream notice-wire p5-effort-fork-effort-menu p5-effort-fork-fork-edge p5-lifecycle-osc-interrupt p5-lifecycle-osc-interruptnext p5-lifecycle-osc-states p5-mcp-discover-http-hostile p5-mcp-discover-http-malformed p5-mcp-discover-http-modern p5-mcp-discover-http-notimpl p5-mcp-discover-http-old2024 p5-mcp-discover-http-ssefail p5-mcp-discover-http-ssehold p5-mcp-discover-http-sseidless p5-mcp-discover-http-ssestrid p5-mcp-live p5-mcp-rpcerror p5-panels-status-badsettings p5-session-early-prefs p5-transcript-cancel p5-transcript-submit p5-transcript-submit-multi p5-usage-budget p5-usage-cached p5-usage-many p5-usage-nokey p5-usage-turns provider provider-resume provider-resume-missing rename-midturn resume-empty streaming`, wall 17m04s, load 4.5 at the start and 5.9 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `3821cfc8eec3c9d710d109d3b0e47b4cea1a5fd965ea2540c9f517b08a97c0e3`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 48 -- ran 48, failed 0, timeout 0.

**pass = 164 / 214 = 76.6%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 214 | 214 | 8 | 156 | 6 | 3 | 0 | 41 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| credits | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| h | 8 | 17 | 17 | 0 | 7 | 0 | 1 | 0 | 9 | 0 | 0 | 41.2% |
| image | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| model | 2 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| notice | 2 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 26 | 135 | 135 | 7 | 108 | 6 | 2 | 0 | 12 | 0 | 0 | 85.2% |
| provider | 3 | 24 | 24 | 0 | 7 | 0 | 0 | 0 | 17 | 0 | 0 | 29.2% |
| rename | 1 | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 3 | 16 | 16 | 0 | 11 | 2 | 3 | 0 | 0 | 0 | 0 | 68.8% |
| `/version` | 3 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| `/quit` | 9 | 32 | 32 | 3 | 21 | 0 | 1 | 0 | 7 | 0 | 0 | 75.0% |
| `/new` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 2 | 15 | 15 | 0 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.3% |
| `/rename` | 1 | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fork` | 1 | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | 3 | 24 | 24 | 0 | 7 | 0 | 0 | 0 | 17 | 0 | 0 | 29.2% |
| `/stats` | 1 | 20 | 20 | 0 | 18 | 0 | 0 | 0 | 2 | 0 | 0 | 90.0% |
| `/usage` | 8 | 71 | 71 | 0 | 63 | 4 | 0 | 0 | 4 | 0 | 0 | 88.7% |
| `/credits` | 2 | 22 | 22 | 0 | 20 | 0 | 0 | 0 | 2 | 0 | 0 | 90.9% |
| `/model` | 3 | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| `/effort` | 1 | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/statusline` | 1 | 20 | 20 | 0 | 18 | 0 | 0 | 0 | 2 | 0 | 0 | 90.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/mcp` | 12 | 17 | 17 | 0 | 11 | 0 | 0 | 0 | 6 | 0 | 0 | 64.7% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 9 | 113 | 113 | 0 | 93 | 0 | 1 | 0 | 19 | 0 | 0 | 82.3% |
| `slash: enter use` | 41 | 184 | 184 | 3 | 141 | 6 | 3 | 0 | 31 | 0 | 0 | 78.3% |
| `slash: esc close` | 19 | 134 | 134 | 0 | 105 | 6 | 2 | 0 | 21 | 0 | 0 | 78.4% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| credits-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| h1-mcphttp | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | hanging row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-mcphttp-fail | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | failed row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-mcphttp-methodnotfound | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | ready row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-models2 | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | after row 5: `Models 2  [All]` / `Models 1  [All]` (aligned: 2/2 rows unmatched) |
| h1-rehomefile | ran | 2 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 50.0% |  |
| h1-usage | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-usage-codex | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | codex row 4: `usage: codex (ChatGPT), samdoe                   …` / `usage: API key (AI_GATEWAY_API_KEY)              …` (aligned: 26/26 rows unmatched) |
| h1-usage-grok | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | grok row 4: `usage: grok (xAI)                                …` / `usage: API key (AI_GATEWAY_API_KEY)              …` (aligned: 10/10 rows unmatched) |
| image-paste | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notice-midstream | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notice-wire | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork-edge | ran | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |  |
| p5-lifecycle-osc-interrupt | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-interruptnext | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-states | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-mcp-discover-http-hostile | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-malformed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-modern | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-notimpl | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-old2024 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssefail | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssehold | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-sseidless | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssestrid | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-live | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched) |
| p5-mcp-rpcerror | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-panels-status-badsettings | ran | 3 | 3 | 0 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-session-early-prefs | ran | 5 | 5 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-budget | ran | 14 | 14 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-cached | ran | 21 | 21 | 0 | 21 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-many | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-nokey | ran | 4 | 4 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-usage-turns | ran | 20 | 20 | 0 | 18 | 0 | 0 | 0 | 2 | 0 | 0 | 90.0% | stats row 27: `* stats: ansi_bytes=15652, redraws=0, debounced_r…` / `* stats: ansi_bytes=9168, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| provider | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-resume | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | switched row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-resume-missing | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn2 row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| rename-midturn | ran | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| streaming | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

41 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| provider-label | d | 5 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| session-record | d | 4 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:38615 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| full-transcript-shift | d | 3 | 0 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:38615; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| stats-bytes | d | 2 | 2 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1756 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| tool-schema | d | 1 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |

Rows no cause covers (36 snaps):
- `h1-mcphttp/hanging` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/failed` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/rpcerror` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-methodnotfound/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-models2/after` fx row 5: `Models 2  [All]`
- `h1-usage-codex/codex` fx row 4: `usage: codex (ChatGPT), samdoe                                                 …`
- `h1-usage-grok/grok` fx row 4: `usage: grok (xAI)                                                              …`
- `p5-mcp-live/cancel` fx row 19: `▪ Cancelled mcp_web_echo · What can xfx do differently?`
- `p5-mcp-live/end` fx row 19: `▪ Cancelled mcp_web_echo · What can xfx do differently?`
- `p5-mcp-live/failed` fx row 19: `▪ Cancelled mcp_web_echo · What can xfx do differently?`
- `p5-transcript-cancel/mid` fx row 9: `full access · model-a`
- `p5-transcript-cancel/esc` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `p5-transcript-cancel/next` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `p5-transcript-cancel/cmd-running` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `p5-transcript-cancel/cmd-esc` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `p5-transcript-cancel/end.full` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `p5-transcript-cancel/full` xfx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `provider/open` fx row 6: `            codex`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 6 | 3 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 6 | 3 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 12 | 3 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 0 | 0 |
| status-panel-session-id-wrap | user-decision | `user decision session id shape` | `date:2026-12-04` | 0 | 0 |
| status-panel-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 2 | 2 |
| usage-no-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 4 | 4 |
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `answer-drawn-wider-named`, `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `inline-band-row-anchor`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`.
