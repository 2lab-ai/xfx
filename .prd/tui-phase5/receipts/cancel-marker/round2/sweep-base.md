# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-884497c [debug] · Run /help for commands`, sha256 `98e31b3b4872361a47d4b4a5ae5cbc4cee59c566253d286421a54caa8d5562ec`
- size 40x120, `--jobs 2`, 132 scenarios -- **PARTIAL**: `--only p5-transcript-* p5-toolgroup-* p5-toolstop-* p5-inline-code-* p5-call-row-* p5-mcp-* p5-input-question p5-effort-fork-* fulltranscript* resume-tools read-failures toolgroup* interrupt* question* approvals* rules-read* compact-cancel links timeout`, wall 51m10s, load 5.9 at the start and 22.5 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `64857db73331782b8350316dea3954cbfd93b7b3544c990c5fa06b3bb1cad09b`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 132 -- ran 132, failed 0, timeout 0.

**pass = 400 / 651 = 61.4%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 651 | 651 | 14 | 386 | 4 | 2 | 0 | 245 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 1 | 0 | 0 | 0 | 17 | 0 | 0 | 5.6% |
| compact | 1 | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| interrupt | 12 | 61 | 61 | 0 | 28 | 0 | 0 | 0 | 33 | 0 | 0 | 45.9% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| p | 102 | 456 | 456 | 11 | 301 | 4 | 2 | 0 | 138 | 0 | 0 | 68.4% |
| question | 3 | 26 | 26 | 0 | 22 | 0 | 0 | 0 | 4 | 0 | 0 | 84.6% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% |
| rules | 2 | 16 | 16 | 1 | 4 | 0 | 0 | 0 | 11 | 0 | 0 | 31.2% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| toolgroup | 3 | 17 | 17 | 0 | 6 | 0 | 0 | 0 | 11 | 0 | 0 | 35.3% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 2 | 27 | 27 | 1 | 24 | 0 | 2 | 0 | 0 | 0 | 0 | 92.6% |
| `/version` | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| `/quit` | 6 | 47 | 47 | 2 | 32 | 0 | 2 | 0 | 11 | 0 | 0 | 72.3% |
| `/new` | 4 | 44 | 44 | 2 | 25 | 0 | 1 | 0 | 16 | 0 | 0 | 61.4% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 8 | 69 | 69 | 2 | 39 | 0 | 2 | 0 | 26 | 0 | 0 | 59.4% |
| `/rename` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fork` | 4 | 33 | 33 | 1 | 30 | 0 | 2 | 0 | 0 | 0 | 0 | 93.9% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | 1 | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/effort` | 8 | 51 | 51 | 0 | 51 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 2 | 16 | 16 | 1 | 4 | 0 | 0 | 0 | 11 | 0 | 0 | 31.2% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 28 | 147 | 147 | 0 | 106 | 0 | 0 | 0 | 41 | 0 | 0 | 72.1% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 9 | 145 | 145 | 1 | 140 | 0 | 2 | 0 | 2 | 0 | 0 | 97.2% |
| `slash: enter use` | 51 | 304 | 304 | 4 | 213 | 0 | 2 | 0 | 85 | 0 | 0 | 71.4% |
| `slash: esc close` | 20 | 232 | 232 | 2 | 166 | 0 | 2 | 0 | 62 | 0 | 0 | 72.4% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 0.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| compact-cancel | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| interrupt | ran | 12 | 12 | 0 | 3 | 0 | 0 | 0 | 9 | 0 | 0 | 25.0% | cc-0.3 row 5: `▪ Cancelled · What can xfx do differently?` / `■ Cancelled · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-cmd | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | cmd-cc row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-next-question | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| interrupt-question | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | q-cc row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovery | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` / `! recovery: ⚠ Provider unavailable · HTTP 500 · s…` (aligned: 10/10 rows unmatched) |
| interrupt-retry | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | r1.05 row 5: `⚠ Provider unavailable · HTTP 500 · server_error:…` / `  Recovered answer.` (aligned: 3/3 rows unmatched) |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer-retract | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt2 | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | mid row 1: `  line 5 of the streamed answer` / `  line 4 of the streamed answer` (aligned: 1/1 rows unmatched) |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| p5-call-row-long | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | end row 7: `● 1 tool call · 1 command` / `● 1 tool call · 1 command · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-call-row-safe | ran | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  tokens: 1200 in · 40 out` / `  finish: tool_calls` (aligned: 8/8 rows unmatched) |
| p5-effort-fork-effort-backtab | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-down | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-loading | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-nolevels | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-short | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork | ran | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |  |
| p5-effort-fork-fork-continue | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork-edge | ran | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |  |
| p5-effort-fork-fork-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-oldread-src | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | picker-enter-fork row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 4/4 rows unmatched) |
| p5-effort-fork-oldread-v1 | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-oldread-v2 | ran | 7 | 7 | 0 | 2 | 0 | 0 | 0 | 5 | 0 | 0 | 28.6% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-slash-rank | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | e row 5: `Commands 23                                      …` / `Commands 21                                      …` (aligned: 1/1 rows unmatched) |
| p5-inline-code-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-question | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | q2-cancelled row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| p5-mcp-config | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched) |
| p5-mcp-config-ignored | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-mcp-config-vars | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched) |
| p5-mcp-direct | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-hostile | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-malformed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-modern | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-notimpl | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-old2024 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssefail | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssehold | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-sseidless | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssestrid | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-asks | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-banner | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-depth | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | list row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-discover-stdio-escaped | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-discover-stdio-hostile | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-malformed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-modern | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-modern-tc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-old2024 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-silent | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-dismiss | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-empty | ran | 24 | 24 | 0 | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-errdata | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-fail | ran | 6 | 6 | 0 | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 66.7% | show-bad row 3: `┃` / `* mcp: bad` (aligned: 7/7 rows unmatched) |
| p5-mcp-failed | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-known | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-live | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched) |
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 34 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-menu-old | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-notes | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-rpcerror | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-unavailable | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched) |
| p5-mcp-wire-new | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-wire-old | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-live | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-live-tc | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
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
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tallfirst | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tools | ran | 8 | 8 | 0 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 12.5% | full-running row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-tz | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tzsys | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-field | ran | 10 | 10 | 0 | 8 | 0 | 0 | 0 | 2 | 0 | 0 | 80.0% | q2-cc row 16: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| question-other | ran | 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 | 71.4% | q2-esc row 14: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| rules-read | ran | 7 | 7 | 1 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | ask row 18: (blank) / `  2 = allow every future read_file of 'secret.txt…` (aligned: 2/2 rows unmatched) |
| rules-read-auto | ran | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% | grep-ask row 18: (blank) / `  2 = allow every future grep_files below '.' for…` (aligned: 2/2 rows unmatched) |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| toolgroup | ran | 13 | 13 | 0 | 2 | 0 | 0 | 0 | 11 | 0 | 0 | 15.4% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| toolgroup-noop | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| toolgroup-wire | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

245 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| tool-schema | d | 38 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| approval-grant-scope | a | 33 | 19 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| session-record | d | 31 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37424 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37424; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| grant-scope-push | a | 7 | 4 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| K-recover | b | 4 | 4 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| menu-count | a+b | 4 | 1 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| question-cap-four | b | 3 | 0 | `fx v0.0.16 asks up to 20 questions in a batch ('max_questions = 20', 'question_answer.zig…` | `I-U6 ask_user_question to 20 (input cluster I11), not yet a branch` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| amendment-card-after-stop | b+d | 2 | 0 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:9665 / fx src/core…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| call-row-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-call-row-safe''s tail page: fx's command entry has two a…` | `unowned (follows tool-schema) -- xfx src/tui/approval_view.rs:685; src/tui/toolgroup.rs:843 / fx src/core/execution/command_environment.zig…` |
| command-size-bound | b | 1 | 1 | `xfx refuses a command whose text, once trimmed, passes 8 KiB ('terminal exec refuses a co…` | `CMD-SIZE (after train 2; raising the bound is an S change)` |

Rows no cause covers (130 snaps):
- `approvals/after-esc` fx row 12: `▪ Cancelled echo again · What can xfx do differently?`
- `approvals/after-esc.full` fx row 12: `▪ Cancelled echo again · What can xfx do differently?`
- `approvals-amend-limit/end` fx row 9: `▪ Cancelled file · What can xfx do differently?`
- `interrupt/cc-0.3` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/cc-1.5` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/cc-3.5` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/next` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/mid2` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/esc1` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/esc2` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/esc2-later` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt/end.full` fx row 5: `▪ Cancelled · What can xfx do differently?`
- `interrupt-cmd/cmd-cc` fx row 8: `▪ Cancelled sleep 6 · What can xfx do differently?`
- `interrupt-cmd/cmd-cc-2` fx row 8: `▪ Cancelled sleep 6 · What can xfx do differently?`
- `interrupt-cmd/cmd-done` fx row 8: `▪ Cancelled sleep 6 · What can xfx do differently?`
- `interrupt-cmd/end.full` fx row 8: `▪ Cancelled sleep 6 · What can xfx do differently?`
- `interrupt-next-cmd/next` fx row 8: `▪ Cancelled sleep 6 · What can xfx do differently?`
- `interrupt-next-question/next` fx row 5: `▪ Cancelled`
- `interrupt-question/q-cc` fx row 5: `▪ Cancelled`
- `interrupt-question/end.full` fx row 5: `▪ Cancelled`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 4 | 2 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 4 | 2 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 8 | 2 |
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

Approved for this pin but covering no row here: `ctrl-o-arabic-letter-mark`, `mcp-approval-wrap-led`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
