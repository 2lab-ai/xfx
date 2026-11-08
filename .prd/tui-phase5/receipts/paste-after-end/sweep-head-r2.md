## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-fd67ed2 [debug] · Run /help for commands`, sha256 `371dcf0831e83c076181b8d3ee1666bb11a6e59c16b4a9a2bbd7cdbb55e7aad1`
- size 40x120, `--jobs 2`, 41 scenarios -- **PARTIAL**: `--only approvals-amend-limit approvals-big approvals card-wrap compact-toolarge composer-wrap image-composer image-inline image-undo model-effort model-router model-speed p5-effort-fork-effort-column p5-effort-fork-effort-menu p5-effort-fork-effort-nolevels p5-effort-fork-fork-edge p5-input-approval p5-input-at-dir p5-input-at p5-input-ctrlbs p5-input-mcp p5-input-model-fail p5-input-model p5-input-paste-amend p5-input-paste-behind p5-input-paste-field p5-input-paste p5-input-question p5-input-smoke p5-panels-keys p5-session-early-prefs p5-transcript-submit-multi p5-transcript-submit p5-transcript-tallfirst paste-block-undo paste-card paste-suffix question-field question-other question skills-paste`, wall 13m17s, load 6.5 at the start and 15.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `530ac71a1831b725c62f2afd16fc02fc6e7a8695c1f60a4af84cc6b85a358c56`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 41 -- ran 41, failed 0, timeout 0. **pass = 200 / 253 = 79.1%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 253 | 253 | 6 | 194 | 0 | 8 | 0 | 45 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| compact | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 1 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| image | 3 | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| model | 3 | 23 | 23 | 0 | 23 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 22 | 134 | 134 | 4 | 94 | 0 | 8 | 0 | 28 | 0 | 0 | 73.1% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| skills | 1 | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% |

Causes of the unresolved snaps:

45 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 21 | 15 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| tool-schema | d | 4 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| question-cap-four | b | 3 | 3 | `fx v0.0.16 asks up to 20 questions in a batch ('max_questions = 20', 'question_answer.zig…` | `I-U6 ask_user_question to 20 (input cluster I11), not yet a branch` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10194 / fx src/cor…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |

Rows no cause covers (20 snaps):
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
- `p5-input-paste-amend/end` xfx row 17: `(blank)`
- `skills-paste/paste` fx row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  Agents  OpenCode  Claw`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `approvals` 10/10 -- edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `p5-input-at` 7/7 -- at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 10/10 rows unmatched)
- `p5-input-model` 6/7 -- q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched)
- `p5-input-paste-amend` 6/6 -- asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `approvals-amend-limit` 4/5 -- asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `p5-input-approval` 3/3 -- cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched)
- `p5-input-question` 3/6 -- q6 row 9: `─────────────────────────────────────────────────…` / `  Six done.` (aligned: 7/7 rows unmatched)
- `approvals-big` 2/3 -- write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched)
- `p5-input-at-dir` 2/2 -- dir-query row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched)
- `p5-panels-keys` 1/10 -- approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
