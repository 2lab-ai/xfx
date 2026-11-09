# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-32afd30 [debug] · Run /help for commands`, sha256 `1756f4fa269bd9cd18c4843976692422ae7fe0291000db978934a8844be02b2f`
- size 41x120, `--jobs 2`, 18 scenarios -- **PARTIAL**: `--only p5-input-row-limit* composer* filepicker* p5-input-at* p5-panels-keys fulltranscript* card-wrap* p5-ctrlo-wider-inline-fam`, wall 5m33s, load 46.0 at the start and 46.0 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `7217599763e1ee4011e4a5ff76a3c3467e793ad07be0c6c1160cf3adea2bf8a2`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 18 -- ran 18, failed 0, timeout 0.

**pass = 61 / 120 = 50.8%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 120 | 120 | 0 | 61 | 6 | 7 | 0 | 46 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| filepicker | 3 | 20 | 20 | 0 | 16 | 0 | 0 | 0 | 4 | 0 | 0 | 80.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| p | 10 | 57 | 57 | 0 | 17 | 6 | 7 | 0 | 27 | 0 | 0 | 29.8% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 2 | 20 | 20 | 0 | 9 | 0 | 0 | 0 | 11 | 0 | 0 | 45.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 1 | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% |
| `/version` | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| `/quit` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/new` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
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
| `/model` | 1 | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% |
| `/effort` | 1 | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% |
| `/fast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 1 | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 2 | 20 | 20 | 0 | 9 | 0 | 0 | 0 | 11 | 0 | 0 | 45.0% |
| `help: esc close` | 2 | 20 | 20 | 0 | 9 | 0 | 0 | 0 | 11 | 0 | 0 | 45.0% |
| `slash: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: enter use` | 5 | 38 | 38 | 0 | 14 | 0 | 7 | 0 | 17 | 0 | 0 | 36.8% |
| `slash: esc close` | 4 | 34 | 34 | 0 | 13 | 0 | 7 | 0 | 14 | 0 | 0 | 38.2% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| card-wrap | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| composer-keys | ran | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker | ran | 10 | 10 | 0 | 6 | 0 | 0 | 0 | 4 | 0 | 0 | 60.0% | at-dir row 5: `      src/` / `      src/deep/` (aligned: 2/2 rows unmatched) |
| filepicker-keys | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker-narrow | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-ctrlo-wider-inline-fam | ran | 7 | 7 | 0 | 0 | 6 | 0 | 0 | 1 | 0 | 0 | 0.0% | resized row 18: `  <wall> (↑1 ↓40)` / (blank) (aligned: 15/15 rows unmatched; under approved row 17, answer-drawn-wider-named) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 8/8 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | dir-enter row 4: (blank) / `─────────────────────────────────────────────────…` (aligned: 11/11 rows unmatched) |
| p5-input-at-gate | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-height | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-at-resize | ran | 6 | 6 | 0 | 1 | 0 | 0 | 0 | 5 | 0 | 0 | 16.7% | short row 3: `┃ @f-` / (blank) (aligned: 4/4 rows unmatched) |
| p5-input-row-limit | ran | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% | at-pgdn row 5: `  items/f-11.txt` / `  items/f-12.txt` (aligned: 2/2 rows unmatched) |
| p5-input-row-limit-menus | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | model-pgdn row 7: `  openai/gpt-6-astra       400K context · 128K ou…` / `  anthropic/claude-opus-5  1M context · 64K outpu…` (aligned: 2/2 rows unmatched) |
| p5-input-row-limit-short | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-panels-keys | ran | 10 | 10 | 0 | 2 | 0 | 7 | 0 | 1 | 0 | 0 | 20.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |

## Causes of the unresolved snaps

46 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| menu-count | a+b | 9 | 2 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| full-transcript-shift | d | 9 | 0 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:39279; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| at-folder-row | b | 8 | 8 | `fx v0.0.16 lists a folder query's folder as its first row and Enter on a folder row menti…` | `I-U3 @ folders: Enter selects a folder, a listed folder is row 1 (input cluster I4), not yet a branch` |
| C7-help-entries | b | 7 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-shift | a+b | 7 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| tool-schema | d | 7 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16413 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| session-record | d | 5 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:39279 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| at-ranking | b | 4 | 4 | `fx v0.0.16 ranks '@st' and '@stat' with its placement score (55e13fad; 'scoreBetter', 'fi…` | `I-U2 @ fzf-style ranking (input cluster I3), not yet a branch` |
| approval-grant-scope | a | 4 | 1 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| resize-document-repaint | d | 1 | 1 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16413 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |

Rows no cause covers (6 snaps):
- `p5-input-row-limit/at-pgdn` fx row 5: `  items/f-11.txt`
- `p5-input-row-limit/grown` fx row 3: `┃↑line-07`
- `p5-input-row-limit/grown-pgup` xfx row 22: `┃ line-20`
- `p5-input-row-limit/grown-pgup2` xfx row 22: `┃ line-20`
- `p5-input-row-limit-menus/model-pgdn` fx row 7: `  openai/gpt-6-astra       400K context · 128K output · Fast · Ultra`
- `p5-input-row-limit-menus/effort-pgdn` fx row 7: `  default  current`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 14 | 7 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 14 | 7 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 28 | 7 |
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
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 79 | 7 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 4 | 3 |

Approved for this pin but covering no row here: `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
