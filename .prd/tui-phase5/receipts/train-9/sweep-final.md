# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ac0279c [debug] · Run /help for commands`, sha256 `f8ee3a538ebe4aa4e1fdfaa96cdc9e5a755b068bee8e5719d5bdad8dbca07847`
- size 40x120, `--jobs 2`, 124 scenarios -- **PARTIAL**: `--only p5-lifecycle-* h1-models* model* p5-input-model* settings-model* interrupt* compact-cancel p5-transcript-cancel *stop* p5-input-question* *question* *approval* p5-stop-in-catalog-wait approvals approvals-amend-limit approvals-big card-wrap compact-toolarge composer-wrap image-composer image-inline image-undo model-effort model-router model-speed p5-effort-fork-effort-column p5-effort-fork-effort-menu p5-effort-fork-effort-nolevels p5-effort-fork-fork-edge p5-input-approval p5-input-at p5-input-at-dir p5-input-ctrlbs p5-input-mcp p5-input-model p5-input-model-fail p5-input-paste p5-input-paste-amend p5-input-paste-behind p5-input-paste-field p5-input-question p5-input-question20 p5-input-question21 p5-input-smoke p5-panels-keys p5-session-early-prefs p5-transcript-submit p5-transcript-submit-multi p5-transcript-tallfirst paste-block-undo paste-card paste-suffix question question-field question-other skills-paste interrupt* p5-stop-in-catalog-wait p5-lifecycle-osc-stopnext p5-lifecycle-osc-interrupt* p5-toolstop-* provider* resume* undo-resume permissions-midturn p5-effort-fork-effort-ctrlc p5-lifecycle-v1resume p5-lifecycle-relhome-resume`, wall 43m34s, load 3.3 at the start and 33.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `09d2657eaf92f3fdaf2c692d8541137e3ea535ac67d90ecab8c6298321ebe345`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 124 -- ran 124, failed 0, timeout 0.

**pass = 419 / 590 = 71.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 590 | 590 | 20 | 399 | 0 | 9 | 0 | 162 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| compact | 2 | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 1 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| h | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| image | 3 | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 12 | 61 | 61 | 0 | 52 | 0 | 0 | 0 | 9 | 0 | 0 | 85.2% |
| model | 15 | 73 | 73 | 0 | 70 | 0 | 0 | 0 | 3 | 0 | 0 | 95.9% |
| p | 62 | 246 | 246 | 9 | 154 | 0 | 9 | 0 | 74 | 0 | 0 | 66.3% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| permissions | 1 | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% |
| provider | 7 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 8 | 24 | 0 | 0 | 0 | 17 | 0 | 0 | 65.3% |
| settings | 2 | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% |
| skills | 1 | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% |
| undo | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 3 | 30 | 30 | 0 | 20 | 0 | 9 | 0 | 1 | 0 | 0 | 66.7% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | 29 | 125 | 125 | 11 | 63 | 0 | 2 | 0 | 49 | 0 | 0 | 59.2% |
| `/new` | 5 | 50 | 50 | 6 | 26 | 0 | 1 | 0 | 17 | 0 | 0 | 64.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 10 | 73 | 73 | 6 | 43 | 0 | 2 | 0 | 22 | 0 | 0 | 67.1% |
| `/rename` | 2 | 22 | 22 | 4 | 17 | 0 | 1 | 0 | 0 | 0 | 0 | 95.5% |
| `/fork` | 2 | 20 | 20 | 0 | 18 | 0 | 2 | 0 | 0 | 0 | 0 | 90.0% |
| `/undo` | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/login` | 2 | 15 | 15 | 0 | 0 | 0 | 0 | 0 | 15 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | 7 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 18 | 84 | 84 | 0 | 75 | 0 | 0 | 0 | 9 | 0 | 0 | 89.3% |
| `/effort` | 4 | 42 | 42 | 0 | 42 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 1 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 2 | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 1 | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 1 | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 19 | 170 | 170 | 4 | 114 | 0 | 1 | 0 | 51 | 0 | 0 | 69.4% |
| `slash: enter use` | 65 | 342 | 342 | 13 | 208 | 0 | 9 | 0 | 112 | 0 | 0 | 64.6% |
| `slash: esc close` | 29 | 214 | 214 | 4 | 141 | 0 | 9 | 0 | 60 | 0 | 0 | 67.8% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| card-wrap | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-cancel | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-toolarge | ran | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-models2 | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-composer | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-inline | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-undo | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-cmd | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-next-question | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-question | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovery | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` / `! recovery: ⚠ Provider unavailable · HTTP 500 · s…` (aligned: 10/10 rows unmatched) |
| interrupt-retry | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | r1.05 row 5: `⚠ Provider unavailable · HTTP 500 · server_error:…` / `  Recovered answer.` (aligned: 3/3 rows unmatched) |
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
| p5-effort-fork-effort-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-nolevels | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork-edge | ran | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |  |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 10/10 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | dir-query row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| p5-input-ctrlbs | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-mcp | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched) |
| p5-input-model-fail | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-paste | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-paste-amend | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-input-paste-behind | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-paste-field | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-question | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-question20 | ran | 11 | 11 | 1 | 8 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% | resumed row 1: `  5) Pick 5?` / `* session resumed: ask twenty` (aligned: 32/32 rows unmatched) |
| p5-input-question21 | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-smoke | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
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
| p5-lifecycle-retryok | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | k00-0.5s row 5: `⚠ Provider unavailable · HTTP 503 · Synthetic una…` / `⚠ Provider unavailable · HTTP 503 · Synthetic una…` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-suspend | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-update | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | u00-update row 2: (blank) / `Usage: xfx [OPTIONS] [COMMAND]` (aligned: 2/2 rows unmatched) |
| p5-lifecycle-v1bad | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | b02-launch-refused.full row 1: (blank) / `xfx: there is no session 'cb1PK0RGxCmY'` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-v1resume | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | v00b-picker-all row 5: `Sessions 1  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 2/2 rows unmatched) |
| p5-panels-keys | ran | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-session-early-prefs | ran | 5 | 5 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-stop-in-catalog-wait | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 85.7% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tallfirst | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-block-undo | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-card | ran | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-suffix | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-midturn | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | second-asks row 16: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future writes to this file f…` (aligned: 3/3 rows unmatched) |
| provider | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | open row 14: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry-back | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | start row 5: `auto · gpt-5.6-sol` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| provider-carry-back-stop | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn1 row 14: `auto · gpt-5.6-sol · read the notes and tell me w…` / `auto · model-a · read the notes and tell me what …` (aligned: 1/1 rows unmatched) |
| provider-resume | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | switched row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-resume-missing | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn2 row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-switch | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | open row 5: `            vercel · current` / `            gateway` (aligned: 4/4 rows unmatched) |
| question | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-field | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-other | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume | ran | 13 | 13 | 4 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-launch | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-paging | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | page1 row 24: `  question one          work · now · 1 turn` / `  question one          work ·  1m · 1 turn` (aligned: 1/1 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| resume-wire | ran | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| skills-paste | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | paste row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| undo-resume | ran | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

162 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 26 | 18 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| provider-label | d | 12 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| tool-schema | d | 11 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| retry-budget-lifecycle | b | 6 | 6 | `Every request of the turn answers 502. fx backs off and is still retrying when Escape sto…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| full-transcript-shift | d | 6 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:38716; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| sound-max | d | 6 | 6 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| C7-help-entries | b | 4 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| K-recover | b | 4 | 4 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| session-record | d | 4 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:38716 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| P1-carry-notice | b | 3 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| question-resume-replay | b | 2 | 2 | `fx v0.0.16 replays a resumed session's answered 'ask_user_question' batch as its resoluti…` | `QUESTION-RESUME a resume replays a question batch's resolutions (docs/parity.md:162), not yet a branch` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| grant-scope-push | a | 2 | 1 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| transcript-page-offset | d | 2 | 0 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:38716 / fx src/core/execution/comman…` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10194 / fx src/cor…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |

Rows no cause covers (91 snaps):
- `model-anchor/alt-enter` fx row 6: `  xai/grok-4.7`
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-at/at-st` fx row 5: `  start.sh`
- `p5-input-at/at-st-down12` xfx row 5: `  start.sh`
- `p5-input-at/at-st-up6` xfx row 5: `  spikes/s-07-trace.txt`
- `p5-input-at/at-stat` fx row 5: `  STATUS.md`
- `p5-input-at/at-src` fx row 5: `  src/`
- `p5-input-at/at-src-enter` fx row 3: `┃ @src/`
- `p5-input-at/at-src-tab` fx row 3: `┃ @src/`
- `p5-input-at-dir/dir-query` fx row 11: `(blank)`
- `p5-input-at-dir/dir-enter` xfx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-sonnet45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-zai-glm` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-grok-fast` fx row 5: `Models 2  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/inline-gpt6` fx row 5: `         openai/gpt-6-astra`
- `p5-input-paste-amend/end` xfx row 17: `(blank)`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 18 | 9 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 18 | 9 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 36 | 9 |
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
