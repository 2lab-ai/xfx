# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-1a0adae [debug] · Run /help for commands`, sha256 `6756fc63249072a9c991197df21d8529102d88efa6bd3508b489b296269654c9`
- size 40x120, `--jobs 4`, 30 scenarios -- **PARTIAL**: `--only approvals* p5-approval* p5-input-approval* permissions* p5-permissions* p5-panels-perm* allowlist* rules-* settings-perm p5-rule-confirm*`, wall 5m04s, load 46.4 at the start and 59.9 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `1669c2831070c5941b6e44953764d0f2be3a6d7e9506da75a52e31419fee32cc`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 30 -- ran 29, failed 1, timeout 0.

**pass = 92 / 238 = 38.7%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 238 | 225 | 3 | 89 | 1 | 4 | 0 | 128 | 13 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| allowlist | 5 | 25 | 25 | 0 | 23 | 0 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| p | 14 | 152 | 139 | 2 | 32 | 1 | 4 | 0 | 100 | 13 | 0 | 22.4% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| rules | 4 | 19 | 19 | 1 | 12 | 0 | 0 | 0 | 6 | 0 | 0 | 68.4% |
| settings | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 2 | 24 | 24 | 2 | 14 | 0 | 4 | 0 | 4 | 0 | 0 | 66.7% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/new` | 1 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
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
| `/settings` | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 13 | 102 | 89 | 3 | 65 | 1 | 1 | 0 | 19 | 13 | 0 | 66.7% |
| `/allowlist` | 6 | 40 | 40 | 2 | 32 | 0 | 1 | 0 | 5 | 0 | 0 | 85.0% |
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
| `slash: ↑↓ navigate` | 3 | 33 | 33 | 2 | 24 | 0 | 1 | 0 | 6 | 0 | 0 | 78.8% |
| `slash: enter use` | 19 | 127 | 114 | 3 | 81 | 1 | 4 | 0 | 25 | 13 | 0 | 66.1% |
| `slash: esc close` | 9 | 79 | 66 | 2 | 47 | 0 | 1 | 0 | 16 | 13 | 0 | 62.0% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| allowlist | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist-remember | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist2 | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | after-reset row 1: (blank) / `<banner>` (aligned: 1/1 rows unmatched) |
| allowlist3 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| p5-approval-ctrl-j | ran | 8 | 8 | 0 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 12.5% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-ctrl-j-panel | ran | 4 | 4 | 0 | 0 | 0 | 3 | 0 | 1 | 0 | 0 | 0.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-approval-draft-keys | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-horizontal-keys | ran | 29 | 29 | 0 | 0 | 0 | 0 | 0 | 29 | 0 | 0 | 0.0% | ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-readiness-settled | ran | 11 | 11 | 0 | 1 | 0 | 0 | 0 | 10 | 0 | 0 | 9.1% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-approval-vertical-keys | ran | 27 | 27 | 0 | 0 | 0 | 0 | 0 | 27 | 0 | 0 | 0.0% | ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-panels-permissions | ran | 20 | 20 | 2 | 14 | 0 | 1 | 0 | 3 | 0 | 0 | 80.0% | approval row 29: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-permissions-saved | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | approval row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-permissions-early | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-permissions-early-probe-ask-after | ran | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-rule-confirm-keys | failed | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 13 | 0 | 0.0% |  |
| p5-rule-confirm-keys-cmd | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | prompt row 8: `─────────────────────────────────────────────────…` / `~ Running (<s>) (↑2 ↓40)` (aligned: 2/2 rows unmatched) |
| p5-rule-confirm-keys-turn | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | prompt row 5: `─────────────────────────────────────────────────…` / `~ Generating (<s>) (↑4 ↓3)` (aligned: 2/2 rows unmatched) |
| permissions | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-btab | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-midturn | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | second-asks row 16: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future writes to this file f…` (aligned: 3/3 rows unmatched) |
| rules-hidden | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-hidden-all | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-read | ran | 7 | 7 | 1 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | ask row 18: (blank) / `  2 = allow every future read_file of 'secret.txt…` (aligned: 2/2 rows unmatched) |
| rules-read-auto | ran | 9 | 9 | 0 | 6 | 0 | 0 | 0 | 3 | 0 | 0 | 66.7% | grep-ask row 18: (blank) / `  2 = allow every future grep_files below '.' for…` (aligned: 2/2 rows unmatched) |
| settings-perm | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

128 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 106 | 91 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| tool-schema | d | 14 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| sound-max | d | 3 | 3 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10435 / fx src/cor…` |
| rule-confirm-card-after-stop | b+d | 2 | 0 | `A user-visible row, 'amendment-card-after-stop''s class. Escape at a rule confirmation ov…` | `AMEND-CARD-ROW the blank row xfx keeps under a card that ends the document -- xfx src/tui/shell.rs:1840 / fx src/core/app/app_worker_runtim…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| allowlist-band | d | 2 | 2 | `Once /allowlist's notices fill the screen fx's band stands three rows above the bottom an…` | `unowned (not traced to the line) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:39-60@c1db9191` |
| approval-settled-every-approval | a | 1 | 1 | `fx v0.0.16 holds an affirmative back while a modal frame is pending only at a file approv…` | `user decision approval readiness scope (the dispatcher's interim choice, 2026-11-10)` |

Rows no cause covers (14 snaps):
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-rule-confirm-keys-cmd/prompt` xfx row 8: `~ Running (<s>) (↑2 ↓40)`
- `p5-rule-confirm-keys-cmd/esc` fx row 5: `● 1 tool call · 1 command · 1 cancelled`
- `p5-rule-confirm-keys-cmd/esc-2` fx row 5: `● 1 tool call · 1 command · 1 cancelled`
- `p5-rule-confirm-keys-cmd/done` fx row 5: `● 1 tool call · 1 command · 1 cancelled`
- `p5-rule-confirm-keys-turn/prompt` xfx row 5: `~ Generating (<s>) (↑4 ↓3)`
- `p5-rule-confirm-keys-turn/esc` xfx row 5: `~ Generating (<s>) (↑4 ↓3)`
- `p5-rule-confirm-keys-turn/end` xfx row 5: `  Working on it.`
- `rules-read/ask` xfx row 19: `  <session-id>' of this saved session`
- `rules-read/ask2` xfx row 28: `  <session-id>' of this saved session`
- `rules-read/listed` xfx row 1: `└ Read secret.txt`
- `rules-read-auto/read-ask` xfx row 30: `  <session-id>' of this saved session`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 8 | 4 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 8 | 4 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 16 | 4 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 10 | 1 |
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
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `answer-drawn-wider-named`, `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `inline-band-row-anchor`, `mcp-approval-wrap-led`, `review-diff-spelling`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
