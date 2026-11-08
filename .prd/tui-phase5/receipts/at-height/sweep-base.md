## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-712dd8d [debug] · Run /help for commands`, sha256 `fd09f29c72b682b6d13af8439a6d6674f9e09236dc2c464bd34032d260458d32`
- size 40x120, `--jobs 2`, 24 scenarios -- **PARTIAL**: `--only p5-input-* filepicker* composer* workspace-index startup h1-resize model-column settings-scrollback p5-mcp-config p5-mcp-verbs`, wall 6m50s, load 8.1 at the start and 10.4 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `692843f07e2e616522ed93a44555fc8f84631fef238621c58f1527c8aba0f18b`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 24 -- ran 24, failed 0, timeout 0. **pass = 64 / 146 = 43.8%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 146 | 146 | 0 | 64 | 0 | 0 | 0 | 82 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| filepicker | 3 | 20 | 20 | 0 | 8 | 0 | 0 | 0 | 12 | 0 | 0 | 40.0% |
| h | 1 | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% |
| model | 1 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 14 | 85 | 85 | 0 | 22 | 0 | 0 | 0 | 63 | 0 | 0 | 25.9% |
| settings | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |

Causes of the unresolved snaps:

82 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| at-folder-row | b | 8 | 8 | `fx v0.0.16 lists a folder query's folder as its first row and Enter on a folder row menti…` | `I-U3 @ folders: Enter selects a folder, a listed folder is row 1 (input cluster I4), not yet a branch` |
| at-ranking | b | 5 | 5 | `fx v0.0.16 ranks '@st' and '@stat' with its placement score (55e13fad; 'scoreBetter', 'fi…` | `I-U2 @ fzf-style ranking (input cluster I3), not yet a branch` |
| at-resize-band-at-the-bottom | d | 5 | 5 | `The resize answer 'resize-document-repaint' names: fx clears the screen and writes the do…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16353 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| question-cap-four | b | 3 | 3 | `fx v0.0.16 asks up to 20 questions in a batch ('max_questions = 20', 'question_answer.zig…` | `I-U6 ask_user_question to 20 (input cluster I11), not yet a branch` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| resize-document-repaint | d | 2 | 2 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16353 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| approval-grant-scope | a | 1 | 0 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| tool-schema | d | 1 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |

Rows no cause covers (56 snaps):
- `filepicker/at` fx row 11: `  src/deep/sched.rs`
- `filepicker/at-sch` fx row 11: `(blank)`
- `filepicker/at-claim` fx row 17: `(blank)`
- `filepicker-keys/at-pgdn` fx row 11: `  src/deep/sched.rs`
- `filepicker-keys/at-ck` fx row 5: `  Cargo.toml`
- `filepicker-narrow/at-alpha` fx row 11: `(blank)`
- `filepicker-narrow/at-alpha-down` fx row 11: `(blank)`
- `filepicker-narrow/at-deep` fx row 11: `(blank)`
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-at-dir/dir-query` fx row 11: `(blank)`
- `p5-input-at-height/at` fx row 11: `  items/f-07.txt`
- `p5-input-at-height/down12` fx row 5: `  items/f-04.txt`
- `p5-input-at-height/up6` fx row 5: `  items/f-04.txt`
- `p5-input-at-height/up9` fx row 11: `  items/f-10.txt`
- `p5-input-at-height/pgup` fx row 5: `  items/f-21.txt`
- `p5-input-at-resize/down12` fx row 5: `  items/f-04.txt`
- `p5-input-model/q-cs45` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`
- `p5-input-model/q-anthropic-opus` fx row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Others`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched)
- `filepicker` 7/10 -- at row 11: `  src/deep/sched.rs` / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched)
- `p5-input-at` 7/7 -- at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 10/10 rows unmatched)
- `p5-input-at-resize` 6/6 -- down12 row 5: `  items/f-04.txt` / `  items/f-08.txt` (aligned: 4/4 rows unmatched)
- `p5-input-model` 6/7 -- q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched)
- `p5-input-at-height` 5/5 -- at row 11: `  items/f-07.txt` / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched)
- `composer-keys` 3/11 -- help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `filepicker-narrow` 3/3 -- at-alpha row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched)
- `p5-input-approval` 3/3 -- cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched)
- `p5-input-question` 3/6 -- q6 row 9: `─────────────────────────────────────────────────…` / `  Six done.` (aligned: 7/7 rows unmatched)
