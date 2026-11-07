## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-58b463c [debug] · Run /help for commands`, sha256 `467422d8ac8a4b4fc8662d677dde63180b196657b35795dcf8b2a174ad689fe0`
- size 40x120, `--jobs 2`, 42 scenarios -- **PARTIAL**: `--only p5-toolgroup-* p5-toolstop-* p5-inline-code-* p5-transcript-*`, wall 14m07s, load 3.9 at the start and 10.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `b1a27fb6e62412ffd3a5eae08e50a9aacd1ab0eb3d6ea49aaf4bfd39fc789a02`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 42 -- ran 42, failed 0, timeout 0. **pass = 55 / 169 = 32.5%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 169 | 169 | 10 | 45 | 2 | 0 | 0 | 112 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 42 | 169 | 169 | 10 | 45 | 2 | 0 | 0 | 112 | 0 | 0 | 32.5% |

Causes of the unresolved snaps:

112 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| truecolor-grey-notation | b | 24 | 18 | `Under COLORTERM=truecolor xfx spells its own greys -- the card's and the composer's rail,…` | `TC-NOTATION builtin greys spelt 38;5 at every depth` |
| session-record | d | 22 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37272 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| tool-schema | d | 21 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| cancelled-marker | b | 18 | 2 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| toolgroup-retint-256 | b | 8 | 4 | `At 256 colours a cell of a tool group's row that holds the statusline grey of the mode in…` | `TG-REDRAW tool rows drawn again on a theme switch` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37272 / fx src/core/execution/comman…` |
| ctrl-o-diff-row-clipped | d | 2 | 2 | `fx reflows a Ctrl-O diff row longer than the screen onto continuation rows under its gutt…` | `DIFF-WRAP -- xfx src/tui/full_transcript.rs:2240 / fx src/ui/render_engine/transcript_blocks.zig:718-766@c1db9191` |
| approval-grant-scope | a | 1 | 1 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |

Rows no cause covers (30 snaps):
- `p5-toolgroup-ctrlo/full-dark` xfx row 34: `(blank)`
- `p5-toolgroup-ctrlo/full-light` xfx row 34: `(blank)`
- `p5-toolgroup-ctrlo/light` fx row 6: `└ Ran echo "hi" 42 \| grep hi # note`
- `p5-toolgroup-ctrlo/full-dark-again` xfx row 34: `(blank)`
- `p5-toolgroup-ctrlo/dark-again` fx row 6: `└ Ran echo "hi" 42 \| grep hi # note`
- `p5-toolgroup-ctrlo-tc/dark` fx row 3: `┃ run the command`
- `p5-toolgroup-ctrlo-tc/full-dark` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/full-light` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/light` fx row 3: `┃ run the command`
- `p5-toolgroup-ctrlo-tc/full-dark-again` fx row 3: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-toolgroup-ctrlo-tc/dark-again` fx row 3: `┃ run the command`
- `p5-toolgroup-live/running-light` fx row 6: `└ Running sleep 5 && echo "hi" 42 # note`
- `p5-toolgroup-live/dark` fx row 6: `└ Ran sleep 5 && echo "hi" 42 # note`
- `p5-toolgroup-live-tc/running-dark` fx row 3: `┃ run the command`
- `p5-toolgroup-live-tc/running-light` fx row 3: `┃ run the command`
- `p5-toolgroup-live-tc/light` fx row 3: `┃ run the command`
- `p5-toolgroup-live-tc/dark` fx row 3: `┃ run the command`
- `p5-transcript-cancel/mid` fx row 9: `full access · model-a`
- `p5-transcript-cancel/esc` fx row 9: `full access · model-a`
- `p5-transcript-cancel/next` fx row 15: `full access · model-a`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-transcript-resize` 13/14 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-transcript-cancel` 7/7 -- mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched)
- `p5-transcript-tools` 7/8 -- full-running row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched)
- `p5-toolgroup-ctrlo-tc` 6/6 -- dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched)
- `p5-transcript-anchormid` 6/8 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-toolgroup-ctrlo` 5/6 -- full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched)
- `p5-toolgroup-live-tc` 4/4 -- running-dark row 3: `┃ run the command` / `┃ run the command` (aligned: 6/6 rows unmatched)
- `p5-transcript-anchor` 4/6 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-transcript-diffctl` 4/4 -- edited row 7: `● 1 tool call · 1 edit` / `● 2 tool calls · 1 read · 1 edit` (aligned: 3/3 rows unmatched)
- `p5-transcript-question` 4/5 -- asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched)
