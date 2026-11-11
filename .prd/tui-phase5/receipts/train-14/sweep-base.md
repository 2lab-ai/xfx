# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.2.0 (debug 2026-11-10-1858-dcfa5146b520) · Run /help for commands`, sha256 `344a72b06f9fae5d7999003453a8273b751267ddd33095f94beb5f44c8e40845`
- size 40x120, `--jobs 2`, 67 scenarios -- **PARTIAL**: `--only filepicker* p5-input-at* p5-input-row-limit* workspace* p5-ctrlo* p5-page-keys* settings* image* p5-image* model-column* p5-panels-help fulltranscript* p5-question-resume-ctrlo p5-at-rank provider-ctrlo skills-ctrlo-dollar skills-ctrlo approvals h1-resize links model-parked p5-panels-badsettings p5-toolgroup-ctrlo p5-toolgroup-ctrlo-tc p5-transcript-anchor p5-transcript-anchormid p5-transcript-cancel p5-transcript-question p5-transcript-records p5-transcript-resize p5-transcript-safe p5-transcript-safetab read-failures resume-tools timeout`, wall 20m31s, load 9.1 at the start and 23.5 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `1b43f82882f0593ec170d212df735b06c5f4f42299a8d1bdaf6a47cb685be932`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 67 -- ran 67, failed 0, timeout 0.

**pass = 215 / 389 = 55.3%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 389 | 389 | 5 | 210 | 13 | 0 | 0 | 161 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 1 | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% |
| filepicker | 3 | 20 | 20 | 0 | 16 | 0 | 0 | 0 | 4 | 0 | 0 | 80.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 6 | 0 | 0 | 0 | 13 | 0 | 0 | 31.6% |
| h | 1 | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| model | 5 | 24 | 24 | 0 | 23 | 0 | 0 | 0 | 1 | 0 | 0 | 95.8% |
| p | 28 | 144 | 144 | 0 | 59 | 13 | 0 | 0 | 72 | 0 | 0 | 41.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% |
| settings | 12 | 72 | 72 | 0 | 30 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| workspace | 2 | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 2 | 26 | 26 | 0 | 15 | 0 | 0 | 0 | 11 | 0 | 0 | 57.7% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | 1 | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% |
| `/quit` | 3 | 24 | 24 | 4 | 18 | 0 | 0 | 0 | 2 | 0 | 0 | 91.7% |
| `/new` | 2 | 24 | 24 | 1 | 11 | 0 | 0 | 0 | 12 | 0 | 0 | 50.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 2 | 24 | 24 | 1 | 11 | 0 | 0 | 0 | 12 | 0 | 0 | 50.0% |
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
| `/model` | 7 | 36 | 36 | 0 | 27 | 0 | 0 | 0 | 9 | 0 | 0 | 75.0% |
| `/effort` | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 12 | 72 | 72 | 0 | 30 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | 2 | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | 4 | 25 | 25 | 3 | 20 | 0 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| `/images` | 1 | 17 | 17 | 3 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/paste` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/mcp` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | 1 | 17 | 17 | 0 | 10 | 0 | 0 | 0 | 7 | 0 | 0 | 58.8% |
| `help: enter open` | 2 | 26 | 26 | 0 | 15 | 0 | 0 | 0 | 11 | 0 | 0 | 57.7% |
| `help: esc close` | 2 | 26 | 26 | 0 | 15 | 0 | 0 | 0 | 11 | 0 | 0 | 57.7% |
| `slash: ↑↓ navigate` | 13 | 86 | 86 | 0 | 44 | 0 | 0 | 0 | 42 | 0 | 0 | 51.2% |
| `slash: enter use` | 29 | 195 | 195 | 5 | 116 | 0 | 0 | 0 | 74 | 0 | 0 | 62.1% |
| `slash: esc close` | 20 | 147 | 147 | 1 | 81 | 0 | 0 | 0 | 65 | 0 | 0 | 55.8% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| filepicker | ran | 10 | 10 | 0 | 6 | 0 | 0 | 0 | 4 | 0 | 0 | 60.0% | at-dir row 5: `      src/` / `      src/deep/` (aligned: 2/2 rows unmatched) |
| filepicker-keys | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker-narrow | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | end row 9: `* version: <build>` / `* version: <build>` (aligned: 1/1 rows unmatched) |
| h1-resize | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | small row 1: `<banner>` / (blank) (aligned: 6/6 rows unmatched) |
| image | ran | 17 | 17 | 3 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-composer | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-inline | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-nonnative | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | sent row 7: `  It is a small red square.` / `✗ system: request failed: SubscriptionNativeImage…` (aligned: 3/3 rows unmatched) |
| image-paste | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-readfile | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-undo | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-unknown | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-withheld | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-hold | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-reask | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched) |
| p5-ctrlo-wider-diff | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-diff-fam | ran | 3 | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 66.7% |  |
| p5-ctrlo-wider-fence | ran | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-ctrlo-wider-inline | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-inline-fam | ran | 7 | 7 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0.0% | resized row 18: `  <wall> (↑1 ↓40)` / (blank) (aligned: 15/15 rows unmatched; under approved row 17, answer-drawn-wider-named) |
| p5-ctrlo-wider-mod | ran | 2 | 2 | 0 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-ctrlo-wider-ri | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | answer row 5: `  ri 🇿🇺🇸 end  )` / `  ri 🇿🇺🇸 end` (aligned: 3/3 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 8/8 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | dir-enter row 4: (blank) / `─────────────────────────────────────────────────…` (aligned: 11/11 rows unmatched) |
| p5-input-at-gate | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-height | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-resize | ran | 6 | 6 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 16.7% | short row 3: `┃ @f-` / (blank) (aligned: 4/4 rows unmatched) |
| p5-input-row-limit | ran | 9 | 9 | 0 | 5 | 0 | 0 | 0 | 4 | 0 | 0 | 55.6% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-input-row-limit-menus | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-row-limit-short | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-panels-badsettings | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | launch row 1: (blank) / `<banner>` (aligned: 10/10 rows unmatched) |
| p5-panels-help | ran | 17 | 17 | 0 | 10 | 0 | 0 | 0 | 7 | 0 | 0 | 58.8% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-question-resume-ctrlo | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | resumed-ctrlo row 9: `│  questions: [{"question":"Solo 1?","options":[{…` / (blank) (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 85.7% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
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
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| workspace | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| workspace-index | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

161 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| sound-max | d | 38 | 38 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| tool-schema | d | 32 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:46711 / fx src/core/execution/comman…` |
| session-record | d | 26 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:46711 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:46711; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| approval-grant-scope | a | 16 | 9 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| menu-count | a+b | 14 | 4 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 9 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| C7-help-entries | b | 7 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:46711 / fx src/core/execution/comman…` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:17389 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| grant-scope-push | a | 3 | 2 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:46711 / fx src/core/execution/comman…` |
| ri-run-pairing | d | 3 | 3 | `Every regional indicator is counted otherwise than the terminals draw it, and the painter…` | `RI-COUNT -- xfx src/tui/wrap.rs:673-693; src/tui/grid.rs:1088 / fx src/core/shared/display_width.zig:234-263@c1db9191` |
| resize-document-repaint | d | 3 | 3 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:17389 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| vision-route | a | 2 | 2 | `For a model without native image input fx forces a 'vision' call answered by a second mod…` | `user decision second-model image route (not yet on the program's decision list)` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10947 / fx src/cor…` |
| profile-refuse-at-launch | b | 1 | 1 | `fx v0.0.16 refuses a profile whose user layer is malformed, too large or names an invalid…` | `PROFILE-REFUSE-AT-LAUNCH (a new queue unit): xfx refuses a malformed user profile at launch, as fx does` |
| C8-shell | b | 1 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| question-resume-ctrlo-detail | b | 1 | 1 | `fx v0.0.16 hands each replayed 'ask_user_question' call its saved detail ('attachHistoric…` | `QUESTION-RESUME-CTRLO a resumed ask's saved detail in Ctrl-O (docs/parity.md:162), not yet a branch` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (16 snaps):
- `filepicker/at-dir` fx row 5: `      src/`
- `filepicker/at-dir-down` fx row 5: `      src/`
- `filepicker/at-dir-cj` fx row 5: `      src/`
- `filepicker/at-tab` fx row 3: `┃ see @src/lib.rs`
- `fulltranscript-notice/end` fx row 9: `* version: <build>`
- `p5-input-at/at-st` fx row 5: `  start.sh`
- `p5-input-at/at-st-down12` xfx row 5: `  spikes/s-04-trace.txt`
- `p5-input-at/at-st-up6` xfx row 5: `  spikes/s-04-trace.txt`
- `p5-input-at/at-stat` fx row 5: `  STATUS.md`
- `p5-input-at/at-src` fx row 5: `  src/`
- `p5-input-at/at-src-enter` fx row 3: `┃ @src/`
- `p5-input-at/at-src-tab` fx row 3: `┃ @src/`
- `p5-input-at-dir/dir-enter` xfx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-transcript-question/asked` fx row 9: `    1) Postgres with the             Relational, strong consistency`
- `p5-transcript-question/narrow` fx row 1: `<banner>`
- `timeout/live` fx row 10: `  <wall> (↑4 ↓80)`

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
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 14 | 1 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 79 | 7 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 40 | 4 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 4 | 3 |

Approved for this pin but covering no row here: `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `filler-modifier-count`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
