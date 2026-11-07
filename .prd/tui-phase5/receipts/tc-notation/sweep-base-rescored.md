## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ec39007 [debug] · Run /help for commands`, sha256 `5088dff3eb4d8bfd3c9458c842dea9fb80c3b1732f90b247e8240cd8b76ce8be`
- size 40x120, `--jobs 2`, 42 scenarios -- **PARTIAL**: `--only p5-inline-code-* p5-toolgroup-* p5-toolstop-* p5-transcript-*`, wall 14m11s, load 21.4 at the start and 34.3 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `00dfa332d470b08a8d6b8606d6c4c3b01e18f5cf11af73ccce9b15dfebc847c5`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 42 -- ran 42, failed 0, timeout 0. **pass = 63 / 169 = 37.3%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 169 | 169 | 10 | 53 | 2 | 0 | 0 | 104 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 42 | 169 | 169 | 10 | 53 | 2 | 0 | 0 | 104 | 0 | 0 | 37.3% |

Causes of the unresolved snaps:

104 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| session-record | d | 22 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37272 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| tool-schema | d | 21 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| cancelled-marker | b | 18 | 6 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| ctrl-o-diff-row-clipped | d | 2 | 2 | `fx reflows a Ctrl-O diff row longer than the screen onto continuation rows under its gutt…` | `DIFF-WRAP -- xfx src/tui/full_transcript.rs:2240 / fx src/ui/render_engine/transcript_blocks.zig:718-766@c1db9191` |
| approval-grant-scope | a | 1 | 1 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |

Rows no cause covers (47 snaps):
- `p5-inline-code-retint-light-tc/light` fx row 3: `┃ show me the code`
- `p5-inline-code-retint-light-tc/dark` fx row 3: `┃ show me the code`
- `p5-inline-code-retint-light-tc/light-again` fx row 3: `┃ show me the code`
- `p5-inline-code-retint-tc/dark` fx row 3: `┃ show me the code`
- `p5-inline-code-retint-tc/light` fx row 3: `┃ show me the code`
- `p5-inline-code-retint-tc/dark-again` fx row 3: `┃ show me the code`
- `p5-inline-code-table-light-tc/light` fx row 3: `┃ show me the table`
- `p5-inline-code-table-light-tc/dark` fx row 3: `┃ show me the table`
- `p5-inline-code-table-light-tc/light-again` fx row 3: `┃ show me the table`
- `p5-inline-code-table-tc/dark` fx row 3: `┃ show me the table`
- `p5-inline-code-table-tc/light` fx row 3: `┃ show me the table`
- `p5-inline-code-table-tc/dark-again` fx row 3: `┃ show me the table`
- `p5-toolgroup-ctrlo-tc/dark` fx row 3: `┃ run the command`
- `p5-toolgroup-ctrlo-tc/full-dark` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/full-light` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/light` fx row 3: `┃ run the command`
- `p5-toolgroup-ctrlo-tc/full-dark-again` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/dark-again` fx row 3: `┃ run the command`
- `p5-toolgroup-live-tc/running-dark` fx row 3: `┃ run the command`
- `p5-toolgroup-live-tc/running-light` fx row 3: `┃ run the command`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-transcript-resize` 13/14 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-transcript-cancel` 7/7 -- mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched)
- `p5-transcript-tools` 7/8 -- full-running row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched)
- `p5-toolgroup-ctrlo-tc` 6/6 -- dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched)
- `p5-transcript-anchormid` 6/8 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-toolgroup-live-tc` 4/4 -- running-dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched)
- `p5-transcript-anchor` 4/6 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-transcript-diffctl` 4/4 -- edited row 7: `● 1 tool call · 1 edit` / `● 2 tool calls · 1 read · 1 edit` (aligned: 3/3 rows unmatched)
- `p5-transcript-question` 4/5 -- asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched)
- `p5-inline-code-retint-light-tc` 3/3 -- light row 3: `┃ show me the code` / `┃ show me the code` (aligned: 4/4 rows unmatched)
