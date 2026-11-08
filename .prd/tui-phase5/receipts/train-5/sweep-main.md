# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-884497c [debug] · Run /help for commands`, sha256 `b9242e06400a716c6c0ae90ab3c16d53243dd9fce2f4a42afeccb1904f32547a`
- size 40x120, `--jobs 2`, 45 scenarios -- **PARTIAL**: `--only p5-lifecycle-osc-* copy recall-edge p5-panels-sweep-a p5-panels-sweep-b trace trace-nowhere titles-idle titles-rename titles-steer image-paste answered links rename reset reset-wire resume resume-launch resume-wire table-wrap usage usage-session p5-transcript-submit-multi p5-transcript-anchor p5-transcript-anchormid p5-transcript-resize p5-transcript-records p5-transcript-question p5-toolgroup-ctrlo p5-toolgroup-ctrlo-tc p5-transcript-safe p5-transcript-safetab fulltranscript* read-failures resume-tools timeout image-composer p5-effort-fork-fork rules-read-auto interrupt-retry model-column-reask model-parked model-reask p5-input-model-fail resume-paging`, wall 21m20s, load 6.9 at the start and 21.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `04585fb3d6ede7462ee0d8a4a9f448ed15a03e66fb7c01b53203ce9985257118`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 45 -- ran 45, failed 0, timeout 0.

**pass = 165 / 300 = 55.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 300 | 300 | 13 | 152 | 2 | 1 | 0 | 132 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| answered | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| copy | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| image | 2 | 15 | 15 | 0 | 15 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 1 | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| model | 3 | 12 | 12 | 0 | 9 | 0 | 0 | 0 | 3 | 0 | 0 | 75.0% |
| p | 14 | 121 | 121 | 3 | 40 | 2 | 1 | 0 | 75 | 0 | 0 | 35.5% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| recall | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| rename | 1 | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% |
| reset | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 5 | 45 | 45 | 8 | 16 | 0 | 0 | 0 | 21 | 0 | 0 | 53.3% |
| rules | 1 | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% |
| table | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 3 | 6 | 6 | 0 | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 66.7% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 2 | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | 3 | 45 | 45 | 0 | 17 | 0 | 0 | 0 | 28 | 0 | 0 | 37.8% |
| `/status` | 1 | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |
| `/version` | 2 | 37 | 37 | 0 | 6 | 0 | 0 | 0 | 31 | 0 | 0 | 16.2% |
| `/quit` | 7 | 88 | 88 | 8 | 42 | 0 | 1 | 0 | 37 | 0 | 0 | 56.8% |
| `/new` | 9 | 117 | 117 | 8 | 51 | 0 | 1 | 0 | 57 | 0 | 0 | 50.4% |
| `/reset` | 2 | 43 | 43 | 0 | 15 | 0 | 0 | 0 | 28 | 0 | 0 | 34.9% |
| `/resume` | 8 | 107 | 107 | 8 | 41 | 0 | 1 | 0 | 57 | 0 | 0 | 45.8% |
| `/rename` | 5 | 64 | 64 | 4 | 24 | 0 | 0 | 0 | 36 | 0 | 0 | 43.8% |
| `/fork` | 1 | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |
| `/undo` | 2 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| `/copy` | 4 | 52 | 52 | 0 | 14 | 0 | 0 | 0 | 38 | 0 | 0 | 26.9% |
| `/compact` | 2 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| `/login` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/logout` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/provider` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/stats` | 3 | 47 | 47 | 0 | 9 | 0 | 0 | 0 | 38 | 0 | 0 | 19.1% |
| `/usage` | 2 | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/credits` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/model` | 4 | 14 | 14 | 0 | 10 | 0 | 0 | 0 | 4 | 0 | 0 | 71.4% |
| `/effort` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/ultrafast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/settings` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/statusline` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 1 | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% |
| `/allowlist` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/workspace` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/shell` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/image` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/images` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/paste` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/mcp` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/feedback` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/trace` | 7 | 55 | 55 | 0 | 18 | 0 | 0 | 0 | 37 | 0 | 0 | 32.7% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 6 | 58 | 58 | 5 | 43 | 0 | 1 | 0 | 9 | 0 | 0 | 82.8% |
| `slash: enter use` | 28 | 202 | 202 | 11 | 112 | 0 | 1 | 0 | 78 | 0 | 0 | 60.9% |
| `slash: esc close` | 11 | 129 | 129 | 6 | 63 | 0 | 1 | 0 | 59 | 0 | 0 | 53.5% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| answered | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| copy | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| image-composer | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-paste | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-retry | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | r1.05 row 5: `⚠ Provider unavailable · HTTP 500 · server_error:…` / `  Recovered answer.` (aligned: 3/3 rows unmatched) |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| model-column-reask | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | col-back row 9: `         unable to load models` / `         openai/gpt-6-astra` (aligned: 4/4 rows unmatched) |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Generating (<s>) (↑4 ↓3)` (aligned: 8/8 rows unmatched) |
| model-reask | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | menu-failed row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Could not reach AI Gateway; retry /model.` (aligned: 1/1 rows unmatched) |
| p5-effort-fork-fork | ran | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |  |
| p5-input-model-fail | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | fail-open row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Could not reach AI Gateway; retry /model.` (aligned: 1/1 rows unmatched) |
| p5-panels-sweep-a | ran | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% | stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2655, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-panels-sweep-b | ran | 11 | 11 | 0 | 4 | 0 | 0 | 0 | 7 | 0 | 0 | 36.4% | stats row 17: `* stats: ansi_bytes=5100, redraws=0, debounced_re…` / `* stats: ansi_bytes=4369, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| recall-edge | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | start row 5: `* stats: ansi_bytes=3525, redraws=0, debounced_re…` / `* stats: ansi_bytes=2675, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| rename | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | relaunched row 1: `xfx: no saved sessions for this workspace.` / `* session resumed: My Title` (aligned: 5/5 rows unmatched) |
| reset | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| reset-wire | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume | ran | 13 | 13 | 4 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-launch | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-paging | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | page1 row 24: `  question one          work · now · 1 turn` / `  question one          work ·  1m · 1 turn` (aligned: 1/1 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| resume-wire | ran | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-read-auto | ran | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% | grep-ask row 18: (blank) / `  2 = allow every future grep_files below '.' for…` (aligned: 2/2 rows unmatched) |
| table-wrap | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| titles-idle | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | answered row 11: `auto · model-a · fix the renderer loop, it redraw` / `auto · model-a · Fix the renderer loop` (aligned: 1/1 rows unmatched) |
| titles-rename | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-steer | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| trace | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| trace-nowhere | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage | ran | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage-session | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

132 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| tool-schema | d | 28 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37758 / fx src/core/execution/comman…` |
| stats-bytes | d | 26 | 19 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1644 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| session-record | d | 25 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37758 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37758; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| cancelled-marker | b | 11 | 9 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| approval-grant-scope | a | 10 | 5 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37758 / fx src/core/execution/comman…` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| K-recover | b | 3 | 3 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37758 / fx src/core/execution/comman…` |
| grant-scope-push | a | 2 | 0 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| provider-label | d | 2 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| sound-max | d | 1 | 0 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| catalog-window | c | 1 | 1 | `'modelsdown 6000' is wall-clock from the gateway's start: under load fx's /model asked af…` | `scenario + fakegw -- fix: an outage counted in catalog requests rather than milliseconds since the gateway started ('modelsdown', a fakegw/…` |

Rows no cause covers (27 snaps):
- `model-column-reask/col-back` fx row 9: `         unable to load models`
- `model-parked/menu-loading` fx row 5: `~ Thinking (<s>)`
- `p5-input-model-fail/fail-open` fx row 7: `Could not reach AI Gateway; xfx retries automatically.`
- `p5-panels-sweep-a/shell` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/rename` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/allowlist` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/compact-empty` fx row 1: `<banner>`
- `p5-panels-sweep-a/trace` fx row 1: `* version: <build>`
- `p5-panels-sweep-a/feedback` fx row 1: `* version: <build>`
- `p5-panels-sweep-a/logout` fx row 19: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/settings` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/settings-closed` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/statusline` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/statusline-closed` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/workspace` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/workspace-closed` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/resume` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/resume-closed` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/login` fx row 1: `* fast: This model does not come with a fast mode.`
- `p5-panels-sweep-a/login-closed` fx row 1: `* fast: This model does not come with a fast mode.`

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
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 9 | 2 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `filler-modifier-count`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
