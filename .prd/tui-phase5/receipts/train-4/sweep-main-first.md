# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-e6cde01 [debug] · Run /help for commands`, sha256 `9ad52be11730e3b85799153f7752fbabbbcbeec0284204955c485de2fa5d1897`
- size 40x120, `--jobs 2`, 89 scenarios -- **PARTIAL**: `--only p5-mcp-* p5-input-mcp *-tc p5-inline-code-* p5-toolgroup-* p5-toolstop-* p5-transcript-* approvals compact h1-models2 interrupt interrupt-cmd interrupt-help interrupt-next-cmd interrupt-recovered interrupt-steer interrupt-steer-retract p5-effort-fork-effort-ctrlc p5-lifecycle-compactok p5-panels-sweep-a permissions-btab permissions-midturn settings-collapse p5-call-row-* fulltranscript* links read-failures resume-tools timeout`, wall 83m00s, load 4.5 at the start and 23.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `9ba4002765044afa46e13226624578db0fea181cd89398f4da2ffc6ac32ef108`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 89 -- ran 85, failed 0, timeout 4.

**pass = 210 / 502 = 41.8%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 502 | 446 | 12 | 198 | 4 | 0 | 0 | 232 | 0 | 56 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 1 | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% |
| compact | 1 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 10 | 0.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| h | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 7 | 39 | 39 | 0 | 22 | 0 | 0 | 0 | 17 | 0 | 0 | 56.4% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| p | 70 | 371 | 325 | 10 | 140 | 4 | 0 | 0 | 171 | 0 | 46 | 40.4% |
| permissions | 2 | 11 | 11 | 0 | 10 | 0 | 0 | 0 | 1 | 0 | 0 | 90.9% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% |
| settings | 1 | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/clear` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | 2 | 37 | 37 | 0 | 6 | 0 | 0 | 0 | 31 | 0 | 0 | 16.2% |
| `/quit` | 3 | 49 | 39 | 1 | 9 | 0 | 0 | 0 | 29 | 0 | 10 | 20.4% |
| `/new` | 3 | 57 | 57 | 1 | 12 | 0 | 0 | 0 | 44 | 0 | 0 | 22.8% |
| `/reset` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/resume` | 3 | 57 | 57 | 1 | 12 | 0 | 0 | 0 | 44 | 0 | 0 | 22.8% |
| `/rename` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/fork` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/undo` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/copy` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/compact` | 3 | 46 | 36 | 0 | 8 | 0 | 0 | 0 | 28 | 0 | 10 | 17.4% |
| `/login` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/logout` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/provider` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/stats` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/model` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/effort` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/ultrafast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/settings` | 2 | 41 | 41 | 0 | 11 | 0 | 0 | 0 | 30 | 0 | 0 | 26.8% |
| `/statusline` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/allowlist` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/workspace` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/shell` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/image` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/images` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 10 | 132 | 108 | 0 | 52 | 0 | 0 | 0 | 56 | 0 | 24 | 39.4% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/feedback` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/trace` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 6 | 97 | 73 | 0 | 52 | 0 | 0 | 0 | 21 | 0 | 24 | 53.6% |
| `slash: enter use` | 22 | 234 | 200 | 2 | 89 | 0 | 0 | 0 | 109 | 0 | 34 | 38.9% |
| `slash: esc close` | 15 | 199 | 175 | 1 | 72 | 0 | 0 | 0 | 102 | 0 | 24 | 36.7% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| compact | timeout | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 10 | 0.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| h1-models2 | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt | ran | 12 | 12 | 0 | 3 | 0 | 0 | 0 | 9 | 0 | 0 | 25.0% | cc-0.3 row 5: `▪ Cancelled · What can xfx do differently?` / `■ Cancelled · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-cmd | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | cmd-cc row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer-retract | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| p5-call-row-long | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | end row 7: `● 1 tool call · 1 command` / `● 1 tool call · 1 command · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-call-row-safe | ran | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  tokens: 1200 in · 40 out` / `  finish: tool_calls` (aligned: 8/8 rows unmatched) |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ show me the code` / `┃ show me the code` (aligned: 4/4 rows unmatched) |
| p5-inline-code-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ show me the code` / `┃ show me the code` (aligned: 4/4 rows unmatched) |
| p5-inline-code-table | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ show me the table` / `┃ show me the table` (aligned: 4/4 rows unmatched) |
| p5-inline-code-table-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ show me the table` / `┃ show me the table` (aligned: 4/4 rows unmatched) |
| p5-input-mcp | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-compactok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-config | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 4/4 rows unmatched) |
| p5-mcp-config-ignored | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-mcp-config-vars | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched) |
| p5-mcp-direct | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-dismiss | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-empty | timeout | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 24 | 0.0% |  |
| p5-mcp-errdata | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-fail | ran | 6 | 6 | 0 | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 66.7% | show-bad row 3: `┃` / `* mcp: bad` (aligned: 7/7 rows unmatched) |
| p5-mcp-failed | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-known | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-live | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched) |
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 23 | 0 | 0 | 0 | 11 | 0 | 0 | 67.6% | list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched) |
| p5-mcp-menu-old | ran | 16 | 16 | 0 | 10 | 0 | 0 | 0 | 6 | 0 | 0 | 62.5% | list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched) |
| p5-mcp-notes | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-rpcerror | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-unavailable | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched) |
| p5-mcp-wire-new | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-wire-old | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-panels-sweep-a | ran | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% | stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2655, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-live | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-live-tc | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | running-dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ run cmd` / `┃ run cmd` (aligned: 6/6 rows unmatched) |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ run cmd` / `┃ run cmd` (aligned: 6/6 rows unmatched) |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-transcript-diffask | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | ask row 21: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 5/5 rows unmatched; under approved row 12, review-diff-spelling) |
| p5-transcript-diffbad | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | edited row 7: `● 2 tool calls · 1 read · 1 edit` / `● 2 tool calls · 1 read · 1 edit · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-transcript-diffctl | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | edited row 7: `● 1 tool call · 1 edit` / `● 2 tool calls · 1 read · 1 edit` (aligned: 3/3 rows unmatched) |
| p5-transcript-diffodd | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwide | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwide2 | ran | 3 | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 66.7% |  |
| p5-transcript-diffwrap | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwrap2 | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-md | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-md-light | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | timeout | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 14 | 0.0% |  |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tallfirst | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tools | timeout | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 8 | 0.0% |  |
| p5-transcript-tz | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tzsys | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-btab | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-midturn | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | second-asks row 16: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future writes to this file f…` (aligned: 3/3 rows unmatched) |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| settings-collapse | ran | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% | collapse-on row 28: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |

## Causes of the unresolved snaps

232 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 43 | 27 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| truecolor-grey-notation | b | 34 | 25 | `Under COLORTERM=truecolor xfx spells its own greys -- the card's and the composer's rail,…` | `TC-NOTATION builtin greys spelt 38;5 at every depth` |
| session-record | d | 29 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37458 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| tool-schema | d | 27 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| stats-bytes | d | 22 | 9 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1583 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| approval-grant-scope | a | 18 | 11 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37458; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| transcript-page-offset | d | 15 | 5 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| toolgroup-ctrlo-record-short | d | 3 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| grant-scope-push | a | 2 | 0 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| provider-label | d | 2 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| sound-max | d | 2 | 1 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| call-row-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-call-row-safe''s tail page: fx's command entry has two a…` | `unowned (follows tool-schema) -- xfx src/tui/approval_view.rs:685; src/tui/toolgroup.rs:750 / fx src/core/execution/command_environment.zig…` |
| command-size-bound | b | 1 | 1 | `xfx refuses a command whose text, once trimmed, passes 8 KiB ('terminal exec refuses a co…` | `CMD-SIZE (after train 2; raising the bound is an S change)` |

Rows no cause covers (91 snaps):
- `approvals/after-esc` xfx row 15: `(blank)`
- `approvals/after-esc.full` xfx row 15: `(blank)`
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/list` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/end.full` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-fail/show-bad` xfx row 3: `* mcp: bad`
- `p5-mcp-fail/end.full` xfx row 3: `* mcp: bad`
- `p5-mcp-menu-new/list` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/list-filter` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/list-unfilter` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/server-remote` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-inplace` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-no` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-cancel` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/left-back` fx row 7: `› remote                  connecting              profile`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 0 | 0 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 0 | 0 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 0 | 0 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 0 | 0 |
| status-panel-session-id-wrap | user-decision | `user decision session id shape` | `date:2026-12-04` | 0 | 0 |
| status-panel-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 0 | 0 |
| usage-no-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 9 | 2 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 10 | 2 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 1 | 1 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 2 | 1 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 4 | 1 |

Approved for this pin but covering no row here: `ctrl-o-arabic-letter-mark`, `mcp-approval-wrap-led`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
