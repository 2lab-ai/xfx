# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-e51685d [debug] · Run /help for commands`, sha256 `bb125be910aaa49ce13c4d214c58c60bada6934d7c1ac3f863a167cbdff408f6`
- size 40x120, `--jobs 2`, 75 scenarios -- **PARTIAL**: `--only p5-transcript-* p5-toolgroup-* p5-toolstop-* p5-inline-code-* p5-call-row-* fulltranscript* links read-failures resume-tools timeout composer-* p5-input-* fast-* model-column* model-speed p5-session-early-prefs`, wall 25m44s, load 100.2 at the start and 100.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `9ba4002765044afa46e13226624578db0fea181cd89398f4da2ffc6ac32ef108`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 75 -- ran 75, failed 0, timeout 0.

**pass = 167 / 347 = 48.1%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 347 | 347 | 14 | 153 | 3 | 0 | 0 | 177 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| model | 4 | 26 | 26 | 0 | 24 | 0 | 0 | 0 | 2 | 0 | 0 | 92.3% |
| p | 57 | 226 | 226 | 12 | 70 | 3 | 0 | 0 | 141 | 0 | 0 | 36.3% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| `/quit` | 2 | 11 | 11 | 3 | 7 | 0 | 0 | 0 | 1 | 0 | 0 | 90.9% |
| `/new` | 2 | 24 | 24 | 1 | 7 | 0 | 0 | 0 | 16 | 0 | 0 | 33.3% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 2 | 24 | 24 | 1 | 7 | 0 | 0 | 0 | 16 | 0 | 0 | 33.3% |
| `/rename` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fork` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 7 | 40 | 40 | 0 | 31 | 0 | 0 | 0 | 9 | 0 | 0 | 77.5% |
| `/effort` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fast` | 5 | 31 | 31 | 0 | 31 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | 4 | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/settings` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 1 | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| `help: esc close` | 1 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| `slash: ↑↓ navigate` | 3 | 29 | 29 | 0 | 29 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `slash: enter use` | 16 | 108 | 108 | 4 | 74 | 0 | 0 | 0 | 30 | 0 | 0 | 72.2% |
| `slash: esc close` | 8 | 66 | 66 | 1 | 39 | 0 | 0 | 0 | 26 | 0 | 0 | 60.6% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| composer-keys | ran | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-astra | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-opus | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-override | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unavailable | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unconfirmed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unsup | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | col-typed row 5: `         unable to load models` / `         openai/gpt-6-astra` (aligned: 4/4 rows unmatched) |
| model-column-reask | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | col-back row 9: `         unable to load models` / `         openai/gpt-6-astra` (aligned: 4/4 rows unmatched) |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-call-row-long | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | end row 7: `● 1 tool call · 1 command` / `● 1 tool call · 1 command · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-call-row-safe | ran | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  tokens: 1200 in · 40 out` / `  finish: tool_calls` (aligned: 8/8 rows unmatched) |
| p5-inline-code-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ show me the code` / `┃ show me the code` (aligned: 4/4 rows unmatched) |
| p5-inline-code-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ show me the code` / `┃ show me the code` (aligned: 4/4 rows unmatched) |
| p5-inline-code-table | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ show me the table` / `┃ show me the table` (aligned: 4/4 rows unmatched) |
| p5-inline-code-table-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ show me the table` / `┃ show me the table` (aligned: 4/4 rows unmatched) |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 10/10 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | dir-query row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| p5-input-ctrlbs | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-mcp | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched) |
| p5-input-model-fail | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | fail-open row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Could not reach AI Gateway; retry /model.` (aligned: 1/1 rows unmatched) |
| p5-input-paste | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | paste-enter row 3: `! input: Input that arrived right after the paste…` / `✗ input: Paste was not applied because extra inpu…` (aligned: 2/2 rows unmatched) |
| p5-input-question | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | q2-cancelled row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| p5-input-smoke | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-session-early-prefs | ran | 5 | 5 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 16.7% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-live | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | running-light row 6: `└ Running sleep 5 && echo "hi" 42 # note` / `└ Running sleep 5 && echo "hi" 42 # note` (aligned: 1/1 rows unmatched) |
| p5-toolgroup-live-tc | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | running-dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-retint | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | light row 6: `└ Ran echo "hi" 42 \| grep hi # note` / `└ Ran echo "hi" 42 \| grep hi # note` (aligned: 1/1 rows unmatched) |
| p5-toolgroup-retint-light | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | dark row 6: `└ Ran echo "hi" 42 \| grep hi # note` / `└ Ran echo "hi" 42 \| grep hi # note` (aligned: 1/1 rows unmatched) |
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
| p5-transcript-diffwide | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | full row 1: `  finish: tool_calls` / `* network: provider: gateway · model: fake/model-…` (aligned: 1/1 rows unmatched) |
| p5-transcript-diffwide2 | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | full row 2: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Prompt` (aligned: 8/8 rows unmatched) |
| p5-transcript-diffwrap | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | full row 2: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Prompt` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` (aligned: 11/11 rows unmatched) |
| p5-transcript-diffwrap2 | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | full row 1: (blank) / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Prompt` (aligned: 17/17 rows unmatched) |
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
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |

## Causes of the unresolved snaps

177 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| truecolor-grey-notation | b | 34 | 25 | `Under COLORTERM=truecolor xfx spells its own greys -- the card's and the composer's rail,…` | `TC-NOTATION builtin greys spelt 38;5 at every depth` |
| tool-schema | d | 33 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| session-record | d | 31 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37458 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| cancelled-marker | b | 24 | 6 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37458; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| approval-grant-scope | a | 9 | 4 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| toolgroup-ctrlo-record-short | d | 3 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| grant-scope-push | a | 2 | 0 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| call-row-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-call-row-safe''s tail page: fx's command entry has two a…` | `unowned (follows tool-schema) -- xfx src/tui/approval_view.rs:685; src/tui/toolgroup.rs:750 / fx src/core/execution/command_environment.zig…` |
| command-size-bound | b | 1 | 1 | `xfx refuses a command whose text, once trimmed, passes 8 KiB ('terminal exec refuses a co…` | `CMD-SIZE (after train 2; raising the bound is an S change)` |

Rows no cause covers (57 snaps):
- `model-column-idle/col-typed` fx row 5: `         unable to load models`
- `model-column-reask/col-back` fx row 9: `         unable to load models`
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
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 10 | 1 |

Approved for this pin but covering no row here: `ctrl-o-arabic-letter-mark`, `mcp-approval-wrap-led`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
