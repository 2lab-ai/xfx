## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-1b15dad [debug] · Run /help for commands`, sha256 `b94a89566ce02c951e6827caf9b8127a07c086d1ebe15334e3f53af050c9aea6`
- size 40x120, `--jobs 2`, 125 scenarios -- **PARTIAL**: `--only startup composer-wrap streaming statusline-turn question allowlist-remember model-column settings-scrollback h1-resize quit model-column-hold model-column-idle model-column-reask model-floor model-parked model-reask p5-effort-fork-effort-down p5-input-model-fail p5-input-row-limit-menus p5-input-row-limit-short p5-input-row-limit p5-stop-in-catalog-wait status-version-stats alias read-failures rename rename-midturn resume resume-empty resume-launch resume-paging resume-tools search-roots timeout statusline approvals approvals-big fulltranscript rules-read skills skills-ghost skills-paste skills-slash image-paste links links-image credits-busy usage-billed usage-deferred approvals-amend-limit image image-composer image-inline image-nonnative image-readfile image-undo image-unknown image-withheld paste-block-undo paste-card notice-wrap trace trace-nowhere workspace workspace-index titles titles-idle titles-off titles-rename titles-steer p5-input-paste paste-suffix p5-panels-keys p5-cli-ledger-update-help p5-effort-fork-effort-backtab p5-effort-fork-effort-column p5-effort-fork-effort-ctrlc p5-effort-fork-effort-loading p5-effort-fork-effort-menu p5-effort-fork-effort-nolevels p5-effort-fork-effort-short p5-effort-fork-fork p5-effort-fork-fork-continue p5-effort-fork-fork-edge p5-effort-fork-fork-empty p5-effort-fork-oldread-src p5-effort-fork-oldread-v1 p5-effort-fork-oldread-v2 p5-effort-fork-slash-rank p5-input-mcp p5-mcp-config p5-mcp-config-ignored p5-mcp-config-vars p5-mcp-direct p5-mcp-dismiss p5-mcp-empty p5-mcp-errdata p5-mcp-fail p5-mcp-failed p5-mcp-known p5-mcp-live p5-mcp-menu-new p5-mcp-menu-old p5-mcp-notes p5-mcp-rpcerror p5-mcp-turn p5-mcp-unavailable p5-mcp-verbs p5-mcp-wire-new p5-mcp-wire-old p5-panels-badsettings p5-panels-help p5-panels-noauth p5-panels-permissions p5-panels-permissions-saved p5-panels-slash p5-panels-status p5-panels-status-badsettings p5-panels-status-first p5-panels-status-short p5-panels-sweep-a p5-panels-sweep-b p5-permissions-early p5-permissions-early-probe-ask-after p5-session-early-prefs`, wall 46m35s, load 13.2 at the start and 13.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `ec2649a71e91906eedbdd390476637813608a3dd7c7b959c03eecd9cadc468cf`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 125 -- ran 124, failed 1, timeout 0. **pass = 576 / 816 = 70.6%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 816 | 814 | 19 | 557 | 4 | 26 | 0 | 208 | 2 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | 1 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| allowlist | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| composer | 1 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| credits | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 1 | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% |
| h | 1 | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| links | 2 | 9 | 9 | 2 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 88.9% |
| model | 7 | 31 | 31 | 0 | 30 | 0 | 0 | 0 | 1 | 0 | 0 | 96.8% |
| notice | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 60 | 442 | 440 | 5 | 286 | 4 | 25 | 0 | 120 | 2 | 0 | 65.8% |
| paste | 3 | 17 | 17 | 1 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| question | 1 | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| quit | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| rename | 2 | 8 | 8 | 0 | 7 | 0 | 0 | 0 | 1 | 0 | 0 | 87.5% |
| resume | 5 | 47 | 47 | 6 | 23 | 0 | 0 | 0 | 18 | 0 | 0 | 61.7% |
| rules | 1 | 7 | 7 | 1 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% |
| search | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| settings | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| skills | 4 | 44 | 44 | 0 | 23 | 0 | 0 | 0 | 21 | 0 | 0 | 52.3% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| status | 1 | 8 | 8 | 0 | 1 | 0 | 1 | 0 | 6 | 0 | 0 | 12.5% |
| statusline | 2 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 5 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 2 | 13 | 13 | 0 | 12 | 0 | 0 | 0 | 1 | 0 | 0 | 92.3% |

Causes of the unresolved snaps:

208 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| stats-bytes | d | 29 | 22 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1854 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| approval-grant-scope | a | 26 | 13 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| menu-count | a+b | 25 | 7 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| tool-schema | d | 22 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| menu-shift | a+b | 18 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| full-transcript-shift | d | 15 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:41113; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| C7-help-entries | b | 9 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| session-age | c | 7 | 7 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| session-record | d | 6 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:41113 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| C8-shell | b | 4 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| upgrade-update-alias | d | 3 | 3 | `The scenario records what fx v0.0.16 prints for 'fx update --help', 'fx upgrade --help' a…` | `unowned (the upgrade command, deferred: docs/parity.md 'upgrade') -- xfx docs/parity.md:63 / fx src/builtins/commands.zig:257-260@c1db9191` |
| C7-help-search | b | 2 | 1 | `/help's search: fx matches a query against each command's help entry (helpCatalogSpecMatc…` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| grant-scope-push | a | 2 | 1 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| vision-route | a | 2 | 2 | `For a model without native image input fx forces a 'vision' call answered by a second mod…` | `user decision second-model image route (not yet on the program's decision list)` |
| provider-label | d | 2 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:117 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10480 / fx src/cor…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| resize-document-repaint | d | 2 | 2 | `fx v0.0.16 answers a resize that changes the screen's size by clearing the screen and its…` | `RESIZE-REPAINT -- xfx src/tui/shell.rs:16840 / fx src/ui/resize_runtime.zig:766-767@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| at-ranking | b | 1 | 1 | `fx v0.0.16 ranks '@st' and '@stat' with its placement score (55e13fad; 'scoreBetter', 'fi…` | `I-U2 @ fzf-style ranking (input cluster I3), not yet a branch` |
| sound-max | d | 1 | 0 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (94 snaps):
- `p5-effort-fork-oldread-src/picker-enter-fork` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-src/resume-source` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-src/doctor.full` fx row 2: `[doctor] ok=5 warn=3 fail=0`
- `p5-effort-fork-oldread-v1/picker-all` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-v1/resume-id` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/picker-all` fx row 5: `Sessions 2  Current workspace  [All workspaces]`
- `p5-effort-fork-oldread-v2/resume-id` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/resume-id.full` xfx row 2: `xfx: there is no session 'HU7p6L-En72Q'`
- `p5-effort-fork-oldread-v2/continued` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-effort-fork-oldread-v2/quit` fx row 1: `xfx: saved session is unreadable. Run 'xfx doctor'; if it is recoverable, use '…`
- `p5-input-row-limit/at` fx row 5: `  items/f-01.txt`
- `p5-input-row-limit/at-pgdn` fx row 5: `  items/f-11.txt`
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/list` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/end.full` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-panels-sweep-a` 28/33 -- stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2682, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched)
- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched)
- `skills` 16/18 -- menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched)
- `resume-tools` 11/18 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/bash` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `approvals` 10/10 -- edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `p5-panels-slash` 10/15 -- slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched)
- `fulltranscript` 9/15 -- cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/bash` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `p5-panels-help` 7/17 -- open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `p5-panels-sweep-b` 7/11 -- stats row 17: `* stats: ansi_bytes=5035, redraws=0, debounced_re…` / `* stats: ansi_bytes=4072, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched)
- `resume-paging` 7/8 -- down14 row 22: `  question three        work ·  1m · 1 turn` / `  question three        work · now · 1 turn` (aligned: 1/1 rows unmatched)
- `p5-panels-badsettings` failed: fx: exit 1
