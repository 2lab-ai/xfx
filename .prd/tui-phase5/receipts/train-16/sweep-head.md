# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.2.0 (debug 2026-11-16-0759-2b78aab658dd) · Run /help for commands`, sha256 `45bb0fd8242c060228a5ec5d6969b6f749da9e55c1f1705051a548f5e8612f1a`
- size 40x120, `--jobs 2`, 223 scenarios -- **PARTIAL**: `--only approval* p5-approval* composer* p5-modified-* p5-cancel* cancel* slash* p5-slash* p5-mode-* profile* p5-profile* settings* p5-ctrlo-wider-inline-fam p5-lifecycle-relhome-resume p5-panels-badsettings fulltranscript fulltranscript-notice h1-resize links model-parked p5-input-at-resize p5-mcp-fail p5-rule-confirm-keys-turn p5-toolgroup-ctrlo p5-toolgroup-ctrlo-tc p5-transcript-anchor p5-transcript-anchormid p5-transcript-cancel p5-transcript-md p5-transcript-md-light p5-transcript-question p5-transcript-records p5-transcript-resize p5-transcript-safe p5-transcript-safetab read-failures resume-tools timeout cmd-wrap fast-astra fast-opus fast-override fast-unavailable fast-unconfirmed fast-unsup h1-rehomefile model model-anchor model-busy model-busy-known model-column model-effort model-empty model-router model-speed p5-effort-fork-effort-backtab p5-effort-fork-effort-column p5-effort-fork-effort-ctrlc p5-effort-fork-effort-down p5-effort-fork-effort-loading p5-effort-fork-effort-menu p5-effort-fork-effort-short p5-input-approval p5-input-at p5-input-at-dir p5-input-at-folders p5-input-at-gate p5-input-at-height p5-input-at-paste p5-input-ctrlbs p5-input-mcp p5-input-model p5-input-model-fail p5-input-model-rank p5-input-model-swap p5-input-paste p5-input-paste-amend p5-input-paste-behind p5-input-paste-field p5-input-question p5-input-question20 p5-input-question21 p5-input-row-limit p5-input-row-limit-menus p5-input-row-limit-short p5-input-smoke p5-lifecycle-osc p5-lifecycle-osc-hostile p5-lifecycle-osc-states p5-mcp-config p5-mcp-config-ignored p5-mcp-config-vars p5-mcp-direct p5-mcp-discover-http-hostile p5-mcp-discover-http-malformed p5-mcp-discover-http-modern p5-mcp-discover-http-notimpl p5-mcp-discover-http-old2024 p5-mcp-discover-http-ssefail p5-mcp-discover-http-ssehold p5-mcp-discover-http-sseidless p5-mcp-discover-http-ssestrid p5-mcp-discover-stdio-asks p5-mcp-discover-stdio-banner p5-mcp-discover-stdio-depth p5-mcp-discover-stdio-escaped p5-mcp-discover-stdio-hostile p5-mcp-discover-stdio-malformed p5-mcp-discover-stdio-modern p5-mcp-discover-stdio-modern-tc p5-mcp-discover-stdio-old2024 p5-mcp-discover-stdio-silent p5-mcp-dismiss p5-mcp-empty p5-mcp-errdata p5-mcp-fail-held p5-mcp-failed p5-mcp-known p5-mcp-live p5-mcp-menu-new p5-mcp-menu-old p5-mcp-notes p5-mcp-rpcerror p5-mcp-turn p5-mcp-unavailable p5-mcp-verbs p5-mcp-wire-new p5-mcp-wire-old p5-page-keys-anchor p5-page-keys-column p5-page-keys-mcp p5-page-keys-settings p5-panels-help p5-panels-keys p5-panels-noauth p5-panels-permissions p5-panels-permissions-saved p5-panels-slash p5-panels-status p5-panels-status-badsettings p5-panels-status-first p5-panels-status-short p5-panels-sweep-a p5-panels-sweep-b p5-paragraph-move p5-paragraph-move-column p5-paragraph-move-menus p5-question-ctrl-j p5-question-ctrl-j-panel p5-question-resume p5-question-resume-ctrlo p5-question-word-left p5-session-early-prefs p5-stop-in-catalog-wait p5-super-arrows p5-super-arrows-approval p5-super-arrows-question p5-usage-codex paste-block-undo paste-card paste-suffix permissions permissions-btab permissions-midturn provider provider-carry provider-carry-back provider-carry-back-stop provider-resume provider-resume-missing provider-switch question question-field question-other rules-hidden rules-hidden-all rules-read rules-read-auto skills skills-ctrlo skills-ctrlo-dollar skills-ghost skills-paste skills-slash startup statusline statusline-turn titles titles-idle titles-off titles-rename titles-steer toolgroup toolgroup-noop`, wall 74m48s, load 9.0 at the start and 46.6 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `123720b4079e9bce0dd4a9b472c3c91b139b2e9c6e4e8298e71118ff64e3626d`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/Help`, `/al`, `/balance`, `/co`, `/cost`, `/de`, `/dem`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/git`, `/he`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/notes`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`, `/wrap-th`, `/x`.

## Pass

Scenarios: 223 -- ran 223, failed 0, timeout 0.

**pass = 971 / 1514 = 64.1%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 1514 | 1514 | 27 | 944 | 12 | 30 | 0 | 501 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 6 | 0 | 0 | 0 | 13 | 0 | 0 | 31.6% |
| h | 2 | 5 | 5 | 0 | 2 | 0 | 1 | 0 | 2 | 0 | 0 | 40.0% |
| links | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| model | 10 | 56 | 56 | 0 | 54 | 0 | 0 | 0 | 2 | 0 | 0 | 96.4% |
| p | 143 | 1028 | 1028 | 22 | 651 | 12 | 29 | 0 | 314 | 0 | 0 | 65.5% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| provider | 7 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| resume | 1 | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% |
| rules | 4 | 19 | 19 | 1 | 12 | 0 | 0 | 0 | 6 | 0 | 0 | 68.4% |
| settings | 12 | 72 | 72 | 1 | 29 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| skills | 6 | 57 | 57 | 0 | 25 | 0 | 0 | 0 | 32 | 0 | 0 | 43.9% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| statusline | 2 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 5 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| toolgroup | 2 | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 7 | 86 | 86 | 0 | 64 | 0 | 0 | 0 | 22 | 0 | 0 | 74.4% |
| `/clear` | 2 | 59 | 59 | 0 | 30 | 0 | 0 | 0 | 29 | 0 | 0 | 50.8% |
| `/status` | 10 | 65 | 65 | 2 | 25 | 3 | 30 | 0 | 5 | 0 | 0 | 41.5% |
| `/version` | 2 | 37 | 37 | 0 | 4 | 0 | 0 | 0 | 33 | 0 | 0 | 10.8% |
| `/quit` | 17 | 114 | 114 | 7 | 37 | 0 | 0 | 0 | 70 | 0 | 0 | 38.6% |
| `/new` | 6 | 73 | 73 | 1 | 24 | 0 | 0 | 0 | 48 | 0 | 0 | 34.2% |
| `/reset` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/resume` | 6 | 73 | 73 | 1 | 24 | 0 | 0 | 0 | 48 | 0 | 0 | 34.2% |
| `/rename` | 4 | 72 | 72 | 0 | 36 | 0 | 0 | 0 | 36 | 0 | 0 | 50.0% |
| `/fork` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/undo` | 2 | 44 | 44 | 0 | 8 | 0 | 0 | 0 | 36 | 0 | 0 | 18.2% |
| `/copy` | 2 | 44 | 44 | 0 | 8 | 0 | 0 | 0 | 36 | 0 | 0 | 18.2% |
| `/compact` | 2 | 44 | 44 | 0 | 8 | 0 | 0 | 0 | 36 | 0 | 0 | 18.2% |
| `/login` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/logout` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/provider` | 9 | 86 | 86 | 0 | 22 | 0 | 0 | 0 | 64 | 0 | 0 | 25.6% |
| `/stats` | 2 | 44 | 44 | 0 | 8 | 0 | 0 | 0 | 36 | 0 | 0 | 18.2% |
| `/usage` | 1 | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% |
| `/credits` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/model` | 20 | 160 | 160 | 0 | 150 | 0 | 0 | 0 | 10 | 0 | 0 | 93.8% |
| `/effort` | 9 | 67 | 67 | 0 | 67 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 6 | 64 | 64 | 0 | 35 | 0 | 0 | 0 | 29 | 0 | 0 | 54.7% |
| `/ultrafast` | 5 | 50 | 50 | 0 | 21 | 0 | 0 | 0 | 29 | 0 | 0 | 42.0% |
| `/settings` | 14 | 124 | 124 | 1 | 40 | 0 | 0 | 0 | 83 | 0 | 0 | 33.1% |
| `/statusline` | 7 | 127 | 127 | 0 | 98 | 0 | 0 | 0 | 29 | 0 | 0 | 77.2% |
| `/sound` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/permissions` | 11 | 99 | 99 | 3 | 73 | 0 | 8 | 0 | 15 | 0 | 0 | 76.8% |
| `/allowlist` | 4 | 75 | 75 | 2 | 40 | 0 | 1 | 0 | 32 | 0 | 0 | 56.0% |
| `/workspace` | 2 | 53 | 53 | 0 | 24 | 0 | 0 | 0 | 29 | 0 | 0 | 45.3% |
| `/shell` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/image` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/images` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 32 | 189 | 189 | 0 | 146 | 1 | 0 | 0 | 42 | 0 | 0 | 77.2% |
| `/skills` | 3 | 51 | 51 | 0 | 29 | 0 | 0 | 0 | 22 | 0 | 0 | 56.9% |
| `/alias` | 2 | 48 | 48 | 0 | 9 | 0 | 0 | 0 | 39 | 0 | 0 | 18.8% |
| `/feedback` | 1 | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% |
| `/trace` | 5 | 50 | 50 | 0 | 12 | 0 | 0 | 0 | 38 | 0 | 0 | 24.0% |
| `help: ↑↓ navigate` | 2 | 19 | 19 | 0 | 12 | 0 | 0 | 0 | 7 | 0 | 0 | 63.2% |
| `help: tab category` | 3 | 53 | 53 | 0 | 44 | 0 | 0 | 0 | 9 | 0 | 0 | 83.0% |
| `help: enter open` | 6 | 82 | 82 | 0 | 61 | 0 | 0 | 0 | 21 | 0 | 0 | 74.4% |
| `help: esc close` | 6 | 82 | 82 | 0 | 61 | 0 | 0 | 0 | 21 | 0 | 0 | 74.4% |
| `slash: ↑↓ navigate` | 48 | 529 | 529 | 3 | 379 | 1 | 12 | 0 | 134 | 0 | 0 | 72.2% |
| `slash: enter use` | 132 | 903 | 903 | 12 | 582 | 4 | 30 | 0 | 275 | 0 | 0 | 65.8% |
| `slash: esc close` | 69 | 674 | 674 | 4 | 422 | 4 | 23 | 0 | 221 | 0 | 0 | 63.2% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| cmd-wrap | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | cmd-a row 1: `<banner>` / (blank) (aligned: 12/12 rows unmatched) |
| composer-keys | ran | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-astra | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-opus | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-override | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unavailable | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unconfirmed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unsup | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | end row 9: `* version: <build>` / `* version: <build>` (aligned: 1/1 rows unmatched) |
| h1-rehomefile | ran | 2 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 50.0% |  |
| h1-resize | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | small row 1: `<banner>` / (blank) (aligned: 6/6 rows unmatched) |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  +4 more)` (aligned: 1/1 rows unmatched) |
| model | ran | 12 | 12 | 0 | 11 | 0 | 0 | 0 | 1 | 0 | 0 | 91.7% | typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| model-anchor | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-effort | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched) |
| model-router | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-approval-ctrl-j | ran | 8 | 8 | 0 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 12.5% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-ctrl-j-panel | ran | 4 | 4 | 0 | 0 | 0 | 3 | 0 | 1 | 0 | 0 | 0.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-approval-draft-keys | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-horizontal-keys | ran | 29 | 29 | 0 | 0 | 0 | 0 | 0 | 29 | 0 | 0 | 0.0% | ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-paste-spaces | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-readiness-settled | ran | 11 | 11 | 0 | 1 | 0 | 0 | 0 | 10 | 0 | 0 | 9.1% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-vertical-keys | ran | 27 | 27 | 0 | 0 | 0 | 0 | 0 | 27 | 0 | 0 | 0.0% | ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-cancel-double-escape | ran | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | c-menu row 5: `Commands 2` / `Commands 1` (aligned: 3/3 rows unmatched) |
| p5-cancel-double-escape-turn | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-ctrlo-wider-inline-fam | ran | 7 | 7 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0.0% | resized row 18: `  <wall> (↑1 ↓40)` / (blank) (aligned: 15/15 rows unmatched; under approved row 17, answer-drawn-wider-named) |
| p5-effort-fork-effort-backtab | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-down | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-loading | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-short | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-dir | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-folders | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-gate | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-height | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-paste | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
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
| p5-input-question20 | ran | 11 | 11 | 2 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-question21 | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-row-limit | ran | 9 | 9 | 0 | 5 | 0 | 0 | 0 | 4 | 0 | 0 | 55.6% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-input-row-limit-menus | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-row-limit-short | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-input-smoke | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-osc | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | s00-start row 5: `ask · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-osc-hostile | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-osc-states | ran | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-lifecycle-relhome-resume | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | r00-resume.full row 2: (blank) / `xfx: HomeNotSet` (aligned: 1/1 rows unmatched) |
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
| p5-mcp-fail-held | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | held row 13: `  Loading tools…` / `  Its tools couldn't load: its process exited wit…` (aligned: 1/1 rows unmatched) |
| p5-mcp-failed | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-known | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-live | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 33 | 1 | 0 | 0 | 0 | 0 | 0 | 97.1% |  |
| p5-mcp-menu-old | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-notes | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-rpcerror | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-unavailable | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched) |
| p5-mcp-wire-new | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-wire-old | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mode-case-env | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mode-case-full | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mode-case-key | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-modified-arrows-full | ran | 11 | 11 | 6 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-modified-arrows-question | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-modified-meta-prefix | ran | 14 | 14 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-modified-page-keys | ran | 10 | 10 | 5 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 90.0% | full-pgup row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-page-keys-anchor | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-page-keys-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-page-keys-mcp | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-page-keys-settings | ran | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% | menu row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| p5-panels-badsettings | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-panels-help | ran | 17 | 17 | 0 | 10 | 0 | 0 | 0 | 7 | 0 | 0 | 58.8% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-panels-keys | ran | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-noauth | ran | 2 | 2 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-panels-permissions | ran | 20 | 20 | 2 | 14 | 0 | 1 | 0 | 3 | 0 | 0 | 80.0% | approval row 29: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-permissions-saved | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | approval row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-slash | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-panels-status | ran | 11 | 11 | 0 | 5 | 0 | 6 | 0 | 0 | 0 | 0 | 45.5% |  |
| p5-panels-status-badsettings | ran | 3 | 3 | 0 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-panels-status-first | ran | 3 | 3 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 33.3% |  |
| p5-panels-status-short | ran | 6 | 6 | 0 | 1 | 0 | 5 | 0 | 0 | 0 | 0 | 16.7% |  |
| p5-panels-sweep-a | ran | 33 | 33 | 0 | 4 | 0 | 0 | 0 | 29 | 0 | 0 | 12.1% | version row 3: `* version: <build>` / `* version: <build>` (aligned: 1/1 rows unmatched) |
| p5-panels-sweep-b | ran | 11 | 11 | 0 | 4 | 0 | 0 | 0 | 7 | 0 | 0 | 36.4% | stats row 17: `* stats: ansi_bytes=5100, redraws=0, debounced_re…` / `* stats: ansi_bytes=4390, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-paragraph-move | ran | 28 | 28 | 0 | 24 | 0 | 0 | 0 | 4 | 0 | 0 | 85.7% | full row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-paragraph-move-column | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-paragraph-move-menus | ran | 21 | 21 | 0 | 21 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-question-ctrl-j | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-question-ctrl-j-panel | ran | 4 | 4 | 0 | 1 | 0 | 3 | 0 | 0 | 0 | 0 | 25.0% |  |
| p5-question-resume | ran | 4 | 4 | 1 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-question-resume-ctrlo | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | resumed-ctrlo row 9: `│  questions: [{"question":"Solo 1?","options":[{…` / (blank) (aligned: 6/6 rows unmatched) |
| p5-question-word-left | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-rule-confirm-keys-turn | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | esc row 4: `┃` / (blank) (aligned: 1/1 rows unmatched) |
| p5-session-early-prefs | ran | 5 | 5 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-arg-skills | ran | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-leading-ws | ran | 10 | 10 | 0 | 8 | 0 | 0 | 0 | 2 | 0 | 0 | 80.0% | vers-run row 3: `* version: <build>` / `* version: <build>` (aligned: 1/1 rows unmatched) |
| p5-slash-leading-ws-he | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | spaces row 5: `Commands 2` / `Commands 1` (aligned: 3/3 rows unmatched) |
| p5-slash-leading-ws-short | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | four-all row 8: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-slash-leading-ws-skill | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-mixed-skills | ran | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-stop-in-catalog-wait | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-super-arrows | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-super-arrows-approval | ran | 16 | 16 | 0 | 0 | 0 | 0 | 0 | 16 | 0 | 0 | 0.0% | down row 19: `  ❯ 2  Apply + allow workspace file access for th…` / `  ❯ 2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-super-arrows-question | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 85.7% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-md | ran | 5 | 5 | 2 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | full-pgup row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-md-light | ran | 5 | 5 | 2 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | full-pgup row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-usage-codex | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | start row 5: `auto · gpt-5.6-sol` / `auto · glm-5.2` (aligned: 1/1 rows unmatched) |
| paste-block-undo | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-card | ran | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-suffix | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-btab | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
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
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| rules-hidden | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-hidden-all | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-read | ran | 7 | 7 | 1 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | ask row 18: (blank) / `  2 = allow every future read_file of 'secret.txt…` (aligned: 2/2 rows unmatched) |
| rules-read-auto | ran | 9 | 9 | 0 | 6 | 0 | 0 | 0 | 3 | 0 | 0 | 66.7% | grep-ask row 18: (blank) / `  2 = allow every future grep_files below '.' for…` (aligned: 2/2 rows unmatched) |
| settings | ran | 7 | 7 | 0 | 4 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-cats | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | off row 20: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse | ran | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% | collapse-on row 28: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse-open | ran | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | running row 1: `<banner>` / `┃ look` (aligned: 2/2 rows unmatched) |
| settings-effort | ran | 8 | 8 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 0.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-history | ran | 9 | 9 | 0 | 6 | 0 | 0 | 0 | 3 | 0 | 0 | 66.7% | marked row 24: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-perm | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-scrollback | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-speed | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | fast-marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings2 | ran | 12 | 12 | 0 | 5 | 0 | 0 | 0 | 7 | 0 | 0 | 41.7% | tab3 row 7: `  Sound level    off  on  max` / `  Sound level    off  on` (aligned: 1/1 rows unmatched) |
| skills | ran | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% | menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ctrlo | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | open row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ctrlo-dollar | ran | 6 | 6 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 16.7% | open row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ghost | ran | 15 | 15 | 0 | 14 | 0 | 0 | 0 | 1 | 0 | 0 | 93.3% | dollar-menu row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-paste | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | paste row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-slash | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | slash-all row 5: `Results 40 · type to filter                      …` / `Results 36 · type to filter                      …` (aligned: 1/1 rows unmatched) |
| slash | ran | 9 | 9 | 0 | 4 | 0 | 0 | 0 | 5 | 0 | 0 | 44.4% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| slash-filter | ran | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | filter-mo row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| startup | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| statusline | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| statusline-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| titles | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-idle | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | answered row 11: `auto · model-a · fix the renderer loop, it redraw` / `auto · model-a · Fix the renderer loop` (aligned: 1/1 rows unmatched) |
| titles-off | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-rename | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-steer | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| toolgroup | ran | 13 | 13 | 0 | 4 | 0 | 0 | 0 | 9 | 0 | 0 | 30.8% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| toolgroup-noop | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

501 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 153 | 122 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| sound-max | d | 51 | 50 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| tool-schema | d | 47 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| menu-count | a+b | 38 | 11 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| session-record | d | 33 | 1 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:50939 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:50939 / fx src/core/execution/comman…` |
| menu-shift | a+b | 26 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| stats-bytes | d | 23 | 7 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1863 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| full-transcript-shift | d | 17 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:50939; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| C7-help-entries | b | 16 | 4 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| grant-scope-push | a | 11 | 6 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| provider-label | d | 10 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:117 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| verbs-refused-port-word | c | 10 | 0 | `remote, web and web2 sit at a refused loopback port. Each try is a 10 s discover probe an…` | `MCP-V -- fix: point remote, web and web2 at a server whose state holds through the run (mcphttp 'hang' with a long startup_timeout_ms, as p…` |
| C8-shell | b | 6 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:50939 / fx src/core/execution/comman…` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:17644 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| session-record-page-bottom | d | 3 | 0 | `The document's head page, a page up from the tail ('full-pgup'): fx's '* session:' record…` | `unowned (follows session-record) -- xfx src/tui/shell.rs:50939 / fx src/core/app/app_bootstrap_runtime.zig:71@c1db9191; src/core/app/app_bo…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:50939 / fx src/core/execution/comman…` |
| amend-draft-window-tail | b+d | 3 | 0 | `'amend-draft-window' for the 4096-byte draft, 'x' then the pasted 'a b', which every late…` | `AMEND-DRAFT-WINDOW the rows an amendment draft longer than its row takes -- xfx src/tui/approval_amendment.rs:81; src/tui/approval_amendmen…` |
| resize-document-repaint | d | 3 | 3 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:17644 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:11127 / fx src/cor…` |
| rule-confirm-card-after-stop | b+d | 2 | 2 | `A user-visible row, 'amendment-card-after-stop''s class. Escape at a rule confirmation ov…` | `AMEND-CARD-ROW the blank row xfx keeps under a card that ends the document -- xfx src/tui/shell.rs:1938 / fx src/core/app/app_worker_runtim…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| approval-settled-every-approval | a | 1 | 1 | `fx v0.0.16 holds an affirmative back while a modal frame is pending only at a file approv…` | `user decision approval readiness scope (the dispatcher's interim choice, 2026-11-10)` |
| question-resume-ctrlo-detail | b | 1 | 1 | `fx v0.0.16 hands each replayed 'ask_user_question' call its saved detail ('attachHistoric…` | `QUESTION-RESUME-CTRLO a resumed ask's saved detail in Ctrl-O (docs/parity.md:162), not yet a branch` |
| cancel-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-transcript-cancel''s Ctrl-O page, read from the tail: fx…` | `session-record and tool-schema's owners: the page offset those two causes make, closed when both are -- xfx src/tui/approval_view.rs:685; s…` |
| amend-draft-window | b+d | 1 | 0 | `A draft longer than its row, the 4093 bytes of 'x' pasted into choice 3's draft. fx paint…` | `AMEND-DRAFT-WINDOW the rows an amendment draft longer than its row takes -- xfx src/tui/approval_amendment.rs:81; src/tui/approval_amendmen…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |
| mcp-tools-at-a-reconnect | b | 1 | 1 | `A failing stdio server's screen, opened near 9 s, between its tries near 7 and 15 s (1 s …` | `MCP-HOLD` |

Rows no cause covers (165 snaps):
- `fulltranscript-notice/end` fx row 9: `* version: <build>`
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-paste-amend/end` xfx row 17: `(blank)`
- `p5-lifecycle-osc/s00-start` fx row 5: `ask · model-a`
- `p5-lifecycle-osc/s01-working` fx row 9: `ask · model-a`
- `p5-lifecycle-osc/s02-done` fx row 11: `ask · model-a`
- `p5-lifecycle-osc/s04-error` fx row 17: `⚠ API request failed · HTTP 400 · Synthetic bad request`
- `p5-lifecycle-osc/s05-permission` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s06-after-permission` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s07-question` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s08-after-question` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s09-exit` xfx row 1: `<banner>`
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 60 | 30 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 60 | 30 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 119 | 30 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 0 | 0 |
| status-panel-session-id-wrap | user-decision | `user decision session id shape` | `date:2026-12-04` | 0 | 0 |
| status-panel-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 8 | 4 |
| usage-no-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 9 | 2 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 2 | 1 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 79 | 7 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 4 | 3 |

Approved for this pin but covering no row here: `answer-label-and-modifier-named`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `usage-no-sign-in`.
