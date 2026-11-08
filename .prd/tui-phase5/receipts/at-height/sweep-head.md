## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-79b8196 [debug] · Run /help for commands`, sha256 `da4f6eaea92c9afbea0c85d7ec26be96fb63ad1eae693eaaf4c1f577a4364f27`
- size 40x120, `--jobs 2`, 25 scenarios -- **PARTIAL**: `--only p5-input-* filepicker* composer* workspace-index startup h1-resize model-column settings-scrollback p5-mcp-config p5-mcp-verbs`, wall 6m47s, load 6.9 at the start and 12.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `a5ce29fab8961aa07cc2bfa1d054599c94401cdde6f530a0b464257eae96340e`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 25 -- ran 25, failed 0, timeout 0. **pass = 83 / 149 = 55.7%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 149 | 149 | 0 | 83 | 0 | 0 | 0 | 66 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| filepicker | 3 | 20 | 20 | 0 | 16 | 0 | 0 | 0 | 4 | 0 | 0 | 80.0% |
| h | 1 | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% |
| model | 1 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 15 | 88 | 88 | 0 | 32 | 0 | 0 | 0 | 56 | 0 | 0 | 36.4% |
| settings | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 1 | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% |

Causes of the unresolved snaps:

66 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| at-folder-row | b | 8 | 8 | `fx v0.0.16 lists a folder query's folder as its first row and Enter on a folder row menti…` | `I-U3 @ folders: Enter selects a folder, a listed folder is row 1 (input cluster I4), not yet a branch` |
| at-ranking | b | 5 | 5 | `fx v0.0.16 ranks '@st' and '@stat' with its placement score (55e13fad; 'scoreBetter', 'fi…` | `I-U2 @ fzf-style ranking (input cluster I3), not yet a branch` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16391 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| question-cap-four | b | 3 | 3 | `fx v0.0.16 asks up to 20 questions in a batch ('max_questions = 20', 'question_answer.zig…` | `I-U6 ask_user_question to 20 (input cluster I11), not yet a branch` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| resize-document-repaint | d | 2 | 2 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16391 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| approval-grant-scope | a | 1 | 0 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| tool-schema | d | 1 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |

Rows no cause covers (40 snaps):
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-sonnet45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-zai-glm` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-grok-fast` fx row 5: `Models 2  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/inline-gpt6` fx row 5: `         openai/gpt-6-astra`
- `p5-input-paste/paste-enter` fx row 3: `! input: Input that arrived right after the paste was ignored.`
- `p5-input-paste/paste-enter.full` fx row 3: `! input: Input that arrived right after the paste was ignored.`
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-fixture` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-remote` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-proj` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/login-stdio` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/login-remote` fx row 2: `  proj     .mcp.json       stdio      waiting for approval`
- `p5-mcp-verbs/auth-remote` fx row 2: `  proj     .mcp.json       stdio      waiting for approval`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched)
- `p5-input-at` 7/7 -- at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 8/8 rows unmatched)
- `p5-input-model` 6/7 -- q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched)
- `p5-input-at-resize` 5/6 -- short row 3: `┃ @f-` / (blank) (aligned: 4/4 rows unmatched)
- `filepicker` 4/10 -- at-dir row 5: `      src/` / `      src/deep/` (aligned: 2/2 rows unmatched)
- `composer-keys` 3/11 -- help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `p5-input-approval` 3/3 -- cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched)
- `p5-input-question` 3/6 -- q6 row 9: `─────────────────────────────────────────────────…` / `  Six done.` (aligned: 7/7 rows unmatched)
- `h1-resize` 2/3 -- small row 1: `<banner>` / (blank) (aligned: 6/6 rows unmatched)
- `p5-input-paste` 2/2 -- paste-enter row 3: `! input: Input that arrived right after the paste…` / `✗ input: Paste was not applied because extra inpu…` (aligned: 2/2 rows unmatched)
