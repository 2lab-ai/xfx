# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ccced1f [debug] · Run /help for commands`, sha256 `05d1464444d9f559c61806926535137152c4b58163e05ae9fcb2478308c4e9ca`
- size 40x120, `--jobs 2`, 77 scenarios -- **PARTIAL**: `--only p5-lifecycle-* h1-models* model* p5-input-model* settings-model* interrupt* compact-cancel p5-transcript-cancel *stop* p5-input-question* *question* *approval* p5-stop-in-catalog-wait`, wall 20m43s, load 8.4 at the start and 9.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `5a190712b78328b071acaf7158642739f7d15e48d3daa1829c071212574c3d01`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 77 -- ran 77, failed 0, timeout 0.

**pass = 212 / 305 = 69.5%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 305 | 305 | 0 | 212 | 0 | 1 | 0 | 92 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| compact | 1 | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| h | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 12 | 61 | 61 | 0 | 51 | 0 | 0 | 0 | 10 | 0 | 0 | 83.6% |
| model | 15 | 73 | 73 | 0 | 70 | 0 | 0 | 0 | 3 | 0 | 0 | 95.9% |
| p | 40 | 111 | 111 | 0 | 53 | 0 | 1 | 0 | 57 | 0 | 0 | 47.7% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| settings | 2 | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 1 | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | 16 | 43 | 43 | 0 | 17 | 0 | 1 | 0 | 25 | 0 | 0 | 39.5% |
| `/new` | 1 | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 4 | 17 | 17 | 0 | 11 | 0 | 1 | 0 | 5 | 0 | 0 | 64.7% |
| `/rename` | 1 | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |
| `/fork` | 1 | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/login` | 2 | 15 | 15 | 0 | 0 | 0 | 0 | 0 | 15 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 18 | 84 | 84 | 0 | 75 | 0 | 0 | 0 | 9 | 0 | 0 | 89.3% |
| `/effort` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fast` | 1 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 2 | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 7 | 53 | 53 | 0 | 40 | 0 | 0 | 0 | 13 | 0 | 0 | 75.5% |
| `slash: enter use` | 41 | 153 | 153 | 0 | 95 | 0 | 1 | 0 | 57 | 0 | 0 | 62.1% |
| `slash: esc close` | 19 | 91 | 91 | 0 | 58 | 0 | 1 | 0 | 32 | 0 | 0 | 63.7% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| compact-cancel | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-models2 | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-cmd | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-next-question | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-question | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovery | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` / `! recovery: ⚠ Provider unavailable · HTTP 500 · s…` (aligned: 10/10 rows unmatched) |
| interrupt-retry | ran | 8 | 8 | 0 | 4 | 0 | 0 | 0 | 4 | 0 | 0 | 50.0% | r1.05 row 5: `⚠ Provider unavailable · HTTP 500 · server_error:…` / `  Recovered answer.` (aligned: 3/3 rows unmatched) |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer-retract | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt2 | ran | 9 | 9 | 0 | 7 | 0 | 0 | 0 | 2 | 0 | 0 | 77.8% | mid row 1: `  line 5 of the streamed answer` / `  line 4 of the streamed answer` (aligned: 1/1 rows unmatched) |
| model | ran | 12 | 12 | 0 | 11 | 0 | 0 | 0 | 1 | 0 | 0 | 91.7% | typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| model-anchor | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | alt-enter row 6: `  xai/grok-4.7` / `  no matching models` (aligned: 1/1 rows unmatched) |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-hold | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-reask | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-effort | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-floor | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched) |
| model-reask | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-router | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-model | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched) |
| p5-input-model-fail | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-question | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | q6 row 9: `─────────────────────────────────────────────────…` / `  Six done.` (aligned: 7/7 rows unmatched) |
| p5-lifecycle-compactfail | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-compactok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-login | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | l00-picker row 3: `┃ /login` / `✗ command: Unknown command. Try /help.` (aligned: 7/7 rows unmatched) |
| p5-lifecycle-login2 | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | m00-vercel-methods row 3: `┃ /login vercel` / `✗ command: Unknown command. Try /help.` (aligned: 7/7 rows unmatched) |
| p5-lifecycle-osc | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | s00-start row 5: `ask · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-osc-ask | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-autocompactfail | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-compact | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-compactfail | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-hostile | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-hup | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-interrupt | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-interruptnext | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-picker | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-retry | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-states | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-stopnext | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-suspend | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-term | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-relhome | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-image | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-made | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-resume | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | r00-resume.full row 2: (blank) / `xfx: HomeNotSet` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-relhome-unsaved | ran | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |  |
| p5-lifecycle-retry | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | r01-2.1s row 5: `⚠ Provider unavailable · HTTP 502 · Synthetic bad…` / `✗ system: the Gateway returned HTTP 502: {"error"…` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-retryok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-suspend | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-update | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | u00-update row 2: (blank) / `Usage: xfx [OPTIONS] [COMMAND]` (aligned: 2/2 rows unmatched) |
| p5-lifecycle-v1bad | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | b02-launch-refused.full row 1: (blank) / `xfx: there is no session 'cb1PK0RGxCmY'` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-v1resume | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | v00b-picker-all row 5: `Sessions 1  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 2/2 rows unmatched) |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 85.7% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| question | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-field | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-other | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

92 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 15 | 10 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| retry-budget-lifecycle | b | 6 | 6 | `Every request of the turn answers 502. fx backs off and is still retrying when Escape sto…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| sound-max | d | 6 | 6 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| K-recover | b | 5 | 5 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| tool-schema | d | 5 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| C7-help-entries | b | 4 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| provider-label | d | 4 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| session-record | d | 3 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:38615 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| transcript-page-offset | d | 2 | 0 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:38615 / fx src/core/execution/comman…` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10194 / fx src/cor…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |

Rows no cause covers (48 snaps):
- `model-anchor/alt-enter` fx row 6: `  xai/grok-4.7`
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-sonnet45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-zai-glm` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-grok-fast` fx row 5: `Models 2  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/inline-gpt6` fx row 5: `         openai/gpt-6-astra`
- `p5-input-question/q6` fx row 9: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-input-question/q6-after` fx row 9: `▪ Cancelled`
- `p5-input-question/q6-after.full` fx row 9: `▪ Cancelled`
- `p5-lifecycle-login/l00-picker` fx row 3: `┃ /login`
- `p5-lifecycle-login/l01-down` fx row 3: `┃ /login`
- `p5-lifecycle-login/l02-down2` fx row 3: `┃ /login`
- `p5-lifecycle-login/l03-down3` fx row 3: `┃ /login`
- `p5-lifecycle-login/l04-first-enter` fx row 3: `┃ /login vercel`
- `p5-lifecycle-login/l05-esc` fx row 3: `┃ /login vercel`
- `p5-lifecycle-login2/m00-vercel-methods` fx row 3: `┃ /login vercel`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 2 | 1 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 2 | 1 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 4 | 1 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 0 | 0 |
| status-panel-session-id-wrap | user-decision | `user decision session id shape` | `date:2026-12-04` | 0 | 0 |
| status-panel-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 0 | 0 |
| usage-no-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 0 | 0 |
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

Approved for this pin but covering no row here: `answer-drawn-wider-named`, `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `inline-band-row-anchor`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
