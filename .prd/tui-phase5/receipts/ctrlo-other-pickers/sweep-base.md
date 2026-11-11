# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-cb7c9c1 [debug] · Run /help for commands`, sha256 `df63cd3a44bc87507b9d2c1e05255bbf4eba8d3dec83f985d1752e995f17aa7e`
- size 40x120, `--jobs 2`, 85 scenarios -- **PARTIAL**: `--only p5-ctrlo* help* model* resume* p5-effort-fork-* skills* settings* slash* provider* p5-mcp-menu* p5-mcp-verbs fulltranscript*`, wall 30m52s, load 5.4 at the start and 22.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `30fc6132c854fdb0c7fb2f79868f47eac29fbfc5fcf461f40102c2b306b5033d`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 85 -- ran 85, failed 0, timeout 0.

**pass = 356 / 606 = 58.7%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 606 | 606 | 9 | 347 | 12 | 2 | 0 | 236 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| help | 1 | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% |
| model | 15 | 73 | 73 | 0 | 70 | 0 | 0 | 0 | 3 | 0 | 0 | 95.9% |
| p | 34 | 256 | 256 | 1 | 166 | 12 | 2 | 0 | 75 | 0 | 0 | 65.2% |
| provider | 7 | 46 | 46 | 0 | 9 | 0 | 0 | 0 | 37 | 0 | 0 | 19.6% |
| resume | 6 | 49 | 49 | 8 | 24 | 0 | 0 | 0 | 17 | 0 | 0 | 65.3% |
| settings | 12 | 72 | 72 | 0 | 30 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| skills | 6 | 57 | 57 | 0 | 23 | 0 | 0 | 0 | 34 | 0 | 0 | 40.4% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 3 | 35 | 35 | 0 | 14 | 0 | 0 | 0 | 21 | 0 | 0 | 40.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 2 | 27 | 27 | 1 | 24 | 0 | 2 | 0 | 0 | 0 | 0 | 92.6% |
| `/version` | 2 | 23 | 23 | 0 | 10 | 0 | 0 | 0 | 13 | 0 | 0 | 43.5% |
| `/quit` | 12 | 85 | 85 | 7 | 47 | 0 | 2 | 0 | 29 | 0 | 0 | 63.5% |
| `/new` | 7 | 68 | 68 | 7 | 40 | 0 | 1 | 0 | 20 | 0 | 0 | 69.1% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 12 | 97 | 97 | 7 | 58 | 0 | 2 | 0 | 30 | 0 | 0 | 67.0% |
| `/rename` | 1 | 13 | 13 | 4 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fork` | 5 | 40 | 40 | 1 | 31 | 0 | 2 | 0 | 6 | 0 | 0 | 80.0% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | 7 | 46 | 46 | 0 | 9 | 0 | 0 | 0 | 37 | 0 | 0 | 19.6% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 17 | 88 | 88 | 0 | 72 | 0 | 0 | 0 | 16 | 0 | 0 | 81.8% |
| `/effort` | 9 | 58 | 58 | 0 | 52 | 0 | 0 | 0 | 6 | 0 | 0 | 89.7% |
| `/fast` | 1 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 12 | 72 | 72 | 0 | 30 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 4 | 83 | 83 | 0 | 53 | 1 | 0 | 0 | 29 | 0 | 0 | 63.9% |
| `/skills` | 2 | 25 | 25 | 0 | 2 | 0 | 0 | 0 | 23 | 0 | 0 | 8.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | 3 | 35 | 35 | 0 | 14 | 0 | 0 | 0 | 21 | 0 | 0 | 40.0% |
| `help: tab category` | 2 | 26 | 26 | 0 | 10 | 0 | 0 | 0 | 16 | 0 | 0 | 38.5% |
| `help: enter open` | 3 | 35 | 35 | 0 | 14 | 0 | 0 | 0 | 21 | 0 | 0 | 40.0% |
| `help: esc close` | 3 | 35 | 35 | 0 | 14 | 0 | 0 | 0 | 21 | 0 | 0 | 40.0% |
| `slash: ↑↓ navigate` | 42 | 391 | 391 | 5 | 226 | 1 | 2 | 0 | 157 | 0 | 0 | 59.1% |
| `slash: enter use` | 64 | 499 | 499 | 9 | 287 | 1 | 2 | 0 | 200 | 0 | 0 | 59.3% |
| `slash: esc close` | 49 | 433 | 433 | 5 | 244 | 1 | 2 | 0 | 181 | 0 | 0 | 57.5% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| help | ran | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
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
| p5-ctrlo-at-keys | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-effort | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | held row 3: `┃` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` (aligned: 11/11 rows unmatched) |
| p5-ctrlo-fork | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | held row 1: `<banner>` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` (aligned: 20/20 rows unmatched) |
| p5-ctrlo-help | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-ctrlo-mcp-args | ran | 5 | 5 | 0 | 3 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | ck row 5: `       list    configured servers` / `       list    configured servers` (aligned: 2/2 rows unmatched) |
| p5-ctrlo-model | ran | 7 | 7 | 0 | 2 | 0 | 0 | 0 | 5 | 0 | 0 | 28.6% | held row 3: `┃` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` (aligned: 12/12 rows unmatched) |
| p5-ctrlo-sessions | ran | 7 | 7 | 0 | 4 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | held row 3: `┃` / (blank) (aligned: 9/9 rows unmatched) |
| p5-ctrlo-slash | ran | 5 | 5 | 0 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 0.0% | open row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-ctrlo-wider-diff | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-diff-fam | ran | 3 | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 66.7% |  |
| p5-ctrlo-wider-fence | ran | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-ctrlo-wider-inline | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-inline-fam | ran | 7 | 7 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0.0% | resized row 18: `  <wall> (↑1 ↓40)` / (blank) (aligned: 15/15 rows unmatched; under approved row 17, answer-drawn-wider-named) |
| p5-ctrlo-wider-mod | ran | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-ctrlo-wider-ri | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | answer row 7: `  wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww…` / `  wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww…` (aligned: 2/2 rows unmatched) |
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
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 33 | 1 | 0 | 0 | 0 | 0 | 0 | 97.1% |  |
| p5-mcp-menu-old | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched) |
| provider | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | open row 14: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry-back | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | start row 5: `auto · gpt-5.6-sol` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| provider-ctrlo | ran | 5 | 5 | 0 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 0.0% | open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-resume | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | switched row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-resume-missing | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn2 row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-switch | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | open row 5: `            vercel · current` / `            gateway` (aligned: 4/4 rows unmatched) |
| resume | ran | 13 | 13 | 4 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-launch | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-paging | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | page1 row 24: `  question one          work · now · 1 turn` / `  question one          work ·  1m · 1 turn` (aligned: 1/1 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| resume-wire | ran | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings | ran | 7 | 7 | 0 | 4 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-cats | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | off row 20: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse | ran | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% | collapse-on row 28: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse-open | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | running row 1: `<banner>` / `┃ look` (aligned: 2/2 rows unmatched) |
| settings-effort | ran | 8 | 8 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 0.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-history | ran | 9 | 9 | 0 | 6 | 0 | 0 | 0 | 3 | 0 | 0 | 66.7% | marked row 24: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-perm | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-scrollback | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-speed | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | fast-marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings2 | ran | 12 | 12 | 0 | 5 | 0 | 0 | 0 | 7 | 0 | 0 | 41.7% | tab3 row 7: `  Sound level    off  on  max` / `  Sound level    off  on` (aligned: 1/1 rows unmatched) |
| skills | ran | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% | menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ctrlo | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | open row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ctrlo-dollar | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | open row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ghost | ran | 15 | 15 | 0 | 14 | 0 | 0 | 0 | 1 | 0 | 0 | 93.3% | dollar-menu row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-paste | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | paste row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-slash | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | slash-all row 5: `Results 40 · type to filter                      …` / `Results 36 · type to filter                      …` (aligned: 1/1 rows unmatched) |
| slash | ran | 9 | 9 | 0 | 4 | 0 | 0 | 0 | 5 | 0 | 0 | 44.4% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| slash-filter | ran | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | filter-mo row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

236 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| sound-max | d | 38 | 38 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| menu-count | a+b | 28 | 9 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 19 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| C7-help-entries | b | 15 | 3 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| full-transcript-shift | d | 15 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:41857; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| provider-label | d | 14 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| tool-schema | d | 12 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| session-record | d | 11 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:41857 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| approval-grant-scope | a | 8 | 3 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| grant-scope-push | a | 3 | 2 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| ri-run-pairing | d | 3 | 3 | `Every regional indicator is counted otherwise than the terminals draw it, and the painter…` | `RI-COUNT -- xfx src/tui/wrap.rs:673-693; src/tui/grid.rs:933 / fx src/core/shared/display_width.zig:234-263@c1db9191` |
| C8-shell | b | 2 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| C7-help-search | b | 2 | 2 | `/help's search: fx matches a query against each command's help entry (helpCatalogSpecMatc…` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| P1-carry-notice | b | 2 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| help-login-args | a | 1 | 0 | `Enter on the Account tab's first row opens fx's /login picker; xfx's first row there is /…` | `user decision Codex/Grok sign-in (#88)` |
| resize-document-repaint | d | 1 | 1 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16705 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (129 snaps):
- `model-anchor/alt-enter` fx row 6: `  xai/grok-4.7`
- `p5-ctrlo-effort/held` fx row 3: `┃`
- `p5-ctrlo-effort/down` fx row 7: `  default  current`
- `p5-ctrlo-effort/ck` fx row 7: `  default  current`
- `p5-ctrlo-effort/cj` fx row 7: `  default  current`
- `p5-ctrlo-effort/tab` fx row 7: `  default  current`
- `p5-ctrlo-effort/esc` xfx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-ctrlo-fork/held` fx row 1: `<banner>`
- `p5-ctrlo-fork/up` fx row 24: `  second prompt beta`
- `p5-ctrlo-fork/ck` fx row 23: `  first prompt alpha`
- `p5-ctrlo-fork/cj` fx row 24: `  second prompt beta`
- `p5-ctrlo-fork/tab` fx row 24: `  second prompt beta`
- `p5-ctrlo-fork/esc` fx row 23: `auto · model-a`
- `p5-ctrlo-help/held` fx row 3: `┃`
- `p5-ctrlo-help/esc` xfx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-ctrlo-mcp-args/ck` fx row 5: `       list    configured servers`
- `p5-ctrlo-mcp-args/cj` fx row 4: `┃`
- `p5-ctrlo-model/held` fx row 3: `┃`
- `p5-ctrlo-model/down` fx row 7: `  openai/gpt-6-astra       400K context · 128K output · Fast · Ultra`
- `p5-ctrlo-model/ck` fx row 7: `  openai/gpt-6-astra       400K context · 128K output · Fast · Ultra`

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
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 1 | 1 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 14 | 1 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 79 | 7 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 40 | 4 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 4 | 3 |

Approved for this pin but covering no row here: `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `filler-modifier-count`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
