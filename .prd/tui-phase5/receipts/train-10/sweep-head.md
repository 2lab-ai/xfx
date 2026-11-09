# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-e28e1b1 [debug] · Run /help for commands`, sha256 `fc425fa462ce0570c8b03eeea24357f32680f6fb4c31246190b7213c7b772466`
- size 40x120, `--jobs 2`, 120 scenarios -- **PARTIAL**: `--only model* p5-input-model* p5-catalog-bounds-* p5-input-at* filepicker* compact* *compact* *stop* interrupt* p5-transcript-cancel h1-models* settings-model* *model* *effort* p5-lifecycle-*compact* p5-input-* composer* workspace-index startup h1-resize model-column settings-scrollback p5-mcp-config p5-mcp-verbs model-parked approvals p5-input-at p5-input-at-dir filepicker p5-input-at-resize fulltranscript* links read-failures resume-tools timeout p5-ctrlo-wider-inline-fam p5-transcript-safe p5-transcript-safetab p5-transcript-submit-multi p5-toolgroup-ctrlo p5-toolgroup-ctrlo-tc p5-transcript-anchor p5-transcript-anchormid p5-transcript-resize p5-transcript-records p5-transcript-question`, wall 48m10s, load 60.5 at the start and 82.1 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `923cb2dc69057675e98920b87859782d52ed6f927a14ced4dde254e62b1c8ce1`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 120 -- ran 120, failed 0, timeout 0.

**pass = 451 / 640 = 70.5%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 640 | 640 | 8 | 443 | 8 | 2 | 0 | 179 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 1 | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% |
| compact | 7 | 25 | 25 | 2 | 23 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| filepicker | 3 | 20 | 20 | 0 | 16 | 0 | 0 | 0 | 4 | 0 | 0 | 80.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| h | 2 | 5 | 5 | 0 | 3 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% |
| interrupt | 12 | 61 | 61 | 0 | 52 | 0 | 0 | 0 | 9 | 0 | 0 | 85.2% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| model | 15 | 73 | 73 | 0 | 71 | 0 | 0 | 0 | 2 | 0 | 0 | 97.3% |
| p | 65 | 346 | 346 | 4 | 227 | 8 | 2 | 0 | 105 | 0 | 0 | 66.8% |
| provider | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% |
| settings | 4 | 21 | 21 | 0 | 7 | 0 | 0 | 0 | 14 | 0 | 0 | 33.3% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| workspace | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 2 | 14 | 14 | 0 | 8 | 0 | 0 | 0 | 6 | 0 | 0 | 57.1% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 2 | 27 | 27 | 1 | 24 | 0 | 2 | 0 | 0 | 0 | 0 | 92.6% |
| `/version` | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| `/quit` | 10 | 71 | 71 | 4 | 49 | 0 | 2 | 0 | 16 | 0 | 0 | 74.6% |
| `/new` | 4 | 44 | 44 | 2 | 29 | 0 | 1 | 0 | 12 | 0 | 0 | 70.5% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 8 | 69 | 69 | 2 | 43 | 0 | 2 | 0 | 22 | 0 | 0 | 65.2% |
| `/rename` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fork` | 4 | 33 | 33 | 1 | 30 | 0 | 2 | 0 | 0 | 0 | 0 | 93.9% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | 8 | 25 | 25 | 1 | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 25 | 128 | 128 | 0 | 118 | 0 | 0 | 0 | 10 | 0 | 0 | 92.2% |
| `/effort` | 8 | 51 | 51 | 0 | 51 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 1 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 4 | 21 | 21 | 0 | 7 | 0 | 0 | 0 | 14 | 0 | 0 | 33.3% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 3 | 40 | 40 | 0 | 11 | 0 | 0 | 0 | 29 | 0 | 0 | 27.5% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 2 | 14 | 14 | 0 | 8 | 0 | 0 | 0 | 6 | 0 | 0 | 57.1% |
| `help: esc close` | 1 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| `slash: ↑↓ navigate` | 16 | 163 | 163 | 1 | 142 | 0 | 2 | 0 | 18 | 0 | 0 | 87.7% |
| `slash: enter use` | 60 | 356 | 356 | 5 | 267 | 0 | 2 | 0 | 82 | 0 | 0 | 76.4% |
| `slash: esc close` | 32 | 235 | 235 | 2 | 165 | 0 | 2 | 0 | 66 | 0 | 0 | 71.1% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| compact | ran | 10 | 10 | 1 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-cancel | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-fail | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-inturn | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-overflow | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-text | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-toolarge | ran | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| composer-keys | ran | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker | ran | 10 | 10 | 0 | 6 | 0 | 0 | 0 | 4 | 0 | 0 | 60.0% | at-dir row 5: `      src/` / `      src/deep/` (aligned: 2/2 rows unmatched) |
| filepicker-keys | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker-narrow | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| h1-models2 | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-resize | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | small row 1: `<banner>` / (blank) (aligned: 6/6 rows unmatched) |
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
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| model | ran | 12 | 12 | 0 | 11 | 0 | 0 | 0 | 1 | 0 | 0 | 91.7% | typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| model-anchor | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-hold | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-reask | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-effort | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-floor | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Generating (<s>) (↑4 ↓3)` (aligned: 8/8 rows unmatched) |
| model-reask | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-router | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-catalog-bounds-2mib | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-catalog-bounds-at | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-catalog-bounds-over | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-catalog-bounds-stall | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-inline-fam | ran | 7 | 7 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0.0% | resized row 18: `  <wall> (↑1 ↓40)` / (blank) (aligned: 15/15 rows unmatched; under approved row 17, answer-drawn-wider-named) |
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
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 8/8 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | dir-enter row 4: (blank) / `─────────────────────────────────────────────────…` (aligned: 11/11 rows unmatched) |
| p5-input-at-gate | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-height | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-resize | ran | 6 | 6 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 16.7% | short row 3: `┃ @f-` / (blank) (aligned: 4/4 rows unmatched) |
| p5-input-ctrlbs | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-mcp | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-fail | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-rank | ran | 27 | 27 | 0 | 27 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-swap | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
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
| p5-lifecycle-osc-autocompactfail | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-compact | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-compactfail | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-stopnext | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-mcp-config | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched) |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched) |
| p5-stop-in-catalog-wait | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 85.7% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| provider-carry-back-stop | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn1 row 14: `auto · gpt-5.6-sol · read the notes and tell me w…` / `auto · model-a · read the notes and tell me what …` (aligned: 1/1 rows unmatched) |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| settings-effort | ran | 8 | 8 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 0.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-scrollback | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| startup | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| workspace-index | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

179 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| tool-schema | d | 32 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:39046 / fx src/core/execution/comman…` |
| session-record | d | 26 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:39046 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| approval-grant-scope | a | 21 | 14 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:39046; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| sound-max | d | 14 | 14 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| at-folder-row | b | 8 | 8 | `fx v0.0.16 lists a folder query's folder as its first row and Enter on a folder row menti…` | `I-U3 @ folders: Enter selects a folder, a listed folder is row 1 (input cluster I4), not yet a branch` |
| C7-help-entries | b | 7 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-count | a+b | 7 | 1 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 6 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:39046 / fx src/core/execution/comman…` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16413 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| K-recover | b | 4 | 4 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| at-ranking | b | 4 | 4 | `fx v0.0.16 ranks '@st' and '@stat' with its placement score (55e13fad; 'scoreBetter', 'fi…` | `I-U2 @ fzf-style ranking (input cluster I3), not yet a branch` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:39046 / fx src/core/execution/comman…` |
| resize-document-repaint | d | 3 | 3 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16413 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| question-resume-replay | b | 2 | 2 | `fx v0.0.16 replays a resumed session's answered 'ask_user_question' batch as its resoluti…` | `QUESTION-RESUME a resume replays a question batch's resolutions (docs/parity.md:162), not yet a branch` |
| grant-scope-push | a | 2 | 1 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10277 / fx src/cor…` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| P1-carry-notice | b | 1 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| activity-tokens-before-request | b | 1 | 0 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (50 snaps):
- `model-parked/menu-loading` xfx row 5: `~ Generating (<s>) (↑4 ↓3)`
- `p5-effort-fork-oldread-src/picker-enter-fork` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-src/resume-source` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-src/doctor.full` fx row 2: `[doctor] ok=5 warn=3 fail=0`
- `p5-effort-fork-oldread-v1/picker-all` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-v1/resume-id` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/picker-all` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-v2/resume-id` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/resume-id.full` xfx row 2: `xfx: there is no session 'HU7p6L-En72Q'`
- `p5-effort-fork-oldread-v2/continued` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/quit` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-paste-amend/end` xfx row 17: `(blank)`
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-fixture` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-remote` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

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
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 79 | 7 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 4 | 3 |

Approved for this pin but covering no row here: `answer-label-and-modifier-named`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
