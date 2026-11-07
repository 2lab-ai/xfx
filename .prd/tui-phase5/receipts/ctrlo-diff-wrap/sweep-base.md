# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-58b463c [debug] · Run /help for commands`, sha256 `253632f6c90eecd38af7a51b2c31376a168b34011b644f37f29603d319beb9f7`
- size 40x120, `--jobs 2`, 23 scenarios -- **PARTIAL**: `--only p5-transcript-*`, wall 8m10s, load 2.9 at the start and 5.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `b1a27fb6e62412ffd3a5eae08e50a9aacd1ab0eb3d6ea49aaf4bfd39fc789a02`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 23 -- ran 23, failed 0, timeout 0.

**pass = 39 / 104 = 37.5%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 104 | 104 | 10 | 29 | 2 | 0 | 0 | 63 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 23 | 104 | 104 | 10 | 29 | 2 | 0 | 0 | 63 | 0 | 0 | 37.5% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
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
| `/model` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/effort` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
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
| `/mcp` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: enter use` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
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
| p5-transcript-md | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-md-light | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | start row 5: `full access · model-a` / `full access · model-a                            …` (aligned: 1/1 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tallfirst | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tools | ran | 8 | 8 | 0 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 12.5% | full-running row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-tz | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tzsys | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |

## Causes of the unresolved snaps

63 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| session-record | d | 16 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37272 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| tool-schema | d | 15 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| cancelled-marker | b | 6 | 0 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| ctrl-o-diff-row-clipped | d | 2 | 2 | `fx reflows a Ctrl-O diff row longer than the screen onto continuation rows under its gutt…` | `DIFF-WRAP -- xfx src/tui/full_transcript.rs:2240 / fx src/ui/render_engine/transcript_blocks.zig:718-766@c1db9191` |
| approval-grant-scope | a | 1 | 1 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |

Rows no cause covers (15 snaps):
- `p5-transcript-cancel/mid` fx row 9: `full access · model-a`
- `p5-transcript-cancel/esc` fx row 9: `full access · model-a`
- `p5-transcript-cancel/next` fx row 15: `full access · model-a`
- `p5-transcript-cancel/cmd-running` fx row 15: `● 1 tool call · 1 command`
- `p5-transcript-cancel/cmd-esc` fx row 15: `● 1 tool call · 1 command · 1 cancelled`
- `p5-transcript-cancel/end.full` fx row 15: `● 1 tool call · 1 command · 1 cancelled`
- `p5-transcript-cancel/full` xfx row 1: `<banner>`
- `p5-transcript-diffwrap/full` xfx row 2: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-transcript-md/full` fx row 32: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response`
- `p5-transcript-md/full-pgup` xfx row 35: `  A list that starts at nine:`
- `p5-transcript-md-light/full` fx row 32: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response`
- `p5-transcript-md-light/full-pgup` xfx row 35: `  A list that starts at nine:`
- `p5-transcript-question/asked` fx row 9: `    1) Postgres with the             Relational, strong consistency`
- `p5-transcript-question/narrow` fx row 1: `<banner>`
- `p5-transcript-records/start` fx row 5: `full access · model-a`

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
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 2 | 1 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 9 | 1 |

Approved for this pin but covering no row here: `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `mcp-approval-wrap-led`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
