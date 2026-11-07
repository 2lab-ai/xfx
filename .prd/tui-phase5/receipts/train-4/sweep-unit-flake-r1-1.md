# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ba24235 [debug] · Run /help for commands`, sha256 `edcb02407bfbb3c084cafc619cce12d52de82eb1f69b23984099498f110037b3`
- size 40x120, `--jobs 2`, 29 scenarios -- **PARTIAL**: `--only approvals compact interrupt-cmd interrupt-help interrupt-next-cmd interrupt-recovered interrupt-steer-retract interrupt-steer interrupt p5-effort-fork-effort-ctrlc p5-lifecycle-compactok p5-mcp-config-ignored p5-mcp-config-vars p5-mcp-live p5-mcp-menu-new p5-mcp-notes p5-mcp-turn p5-mcp-verbs p5-panels-sweep-a p5-toolstop-retint-light-tc p5-toolstop-retint-light p5-toolstop-retint-tc p5-toolstop-retint p5-transcript-cancel p5-transcript-records p5-transcript-resize permissions-btab permissions-midturn settings-collapse`, wall 12m47s, load 17.0 at the start and 28.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `a864bd1564dabffd182f3fb199bffee165c5fc169a54b01461ed3d32cd29a42c`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 29 -- ran 29, failed 0, timeout 0.

**pass = 92 / 236 = 39.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 236 | 236 | 1 | 91 | 1 | 0 | 0 | 143 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 1 | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% |
| compact | 1 | 10 | 10 | 1 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| interrupt | 7 | 39 | 39 | 0 | 22 | 0 | 0 | 0 | 17 | 0 | 0 | 56.4% |
| p | 17 | 158 | 158 | 0 | 44 | 1 | 0 | 0 | 113 | 0 | 0 | 27.8% |
| permissions | 2 | 11 | 11 | 0 | 10 | 0 | 0 | 0 | 1 | 0 | 0 | 90.9% |
| settings | 1 | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `/clear` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/quit` | 2 | 43 | 43 | 1 | 14 | 0 | 0 | 0 | 28 | 0 | 0 | 34.9% |
| `/new` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/reset` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/resume` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/rename` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/fork` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/undo` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/copy` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/compact` | 3 | 46 | 46 | 1 | 17 | 0 | 0 | 0 | 28 | 0 | 0 | 39.1% |
| `/login` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/logout` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/provider` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/stats` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/model` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/effort` | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/ultrafast` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/settings` | 2 | 41 | 41 | 0 | 11 | 0 | 0 | 0 | 30 | 0 | 0 | 26.8% |
| `/statusline` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/allowlist` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/workspace` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/shell` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/image` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/images` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 5 | 74 | 74 | 0 | 27 | 1 | 0 | 0 | 46 | 0 | 0 | 36.5% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/feedback` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/trace` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 2 | 42 | 42 | 0 | 28 | 1 | 0 | 0 | 13 | 0 | 0 | 66.7% |
| `slash: enter use` | 12 | 140 | 140 | 1 | 59 | 1 | 0 | 0 | 79 | 0 | 0 | 42.9% |
| `slash: esc close` | 7 | 115 | 115 | 0 | 38 | 1 | 0 | 0 | 76 | 0 | 0 | 33.0% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| compact | ran | 10 | 10 | 1 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt | ran | 12 | 12 | 0 | 3 | 0 | 0 | 0 | 9 | 0 | 0 | 25.0% | cc-0.3 row 5: `▪ Cancelled · What can xfx do differently?` / `■ Cancelled · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-cmd | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | cmd-cc row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer-retract | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-compactok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-config-ignored | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-mcp-config-vars | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched) |
| p5-mcp-live | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full row 1: `* network: provider: gateway · model: fake/model-…` / `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` (aligned: 11/11 rows unmatched) |
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 22 | 1 | 0 | 0 | 11 | 0 | 0 | 64.7% | list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched) |
| p5-mcp-notes | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched) |
| p5-panels-sweep-a | ran | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% | stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2655, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 3: `┃ run cmd` / `┃ run cmd` (aligned: 6/6 rows unmatched) |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 3: `┃ run cmd` / `┃ run cmd` (aligned: 6/6 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| permissions-btab | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-midturn | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | second-asks row 16: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future writes to this file f…` (aligned: 3/3 rows unmatched) |
| settings-collapse | ran | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% | collapse-on row 28: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

143 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 37 | 19 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| transcript-page-offset | d | 16 | 10 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| stats-bytes | d | 16 | 9 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1583 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| approval-grant-scope | a | 10 | 7 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| tool-schema | d | 7 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| session-record | d | 4 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37424 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| C7-help-entries | b | 3 | 0 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-count | a+b | 3 | 0 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 3 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| provider-label | d | 2 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| sound-max | d | 2 | 1 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |

Rows no cause covers (85 snaps):
- `approvals/after-esc` xfx row 15: `(blank)`
- `approvals/after-esc.full` xfx row 15: `(blank)`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/list` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/end.full` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-live/full` xfx row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice`
- `p5-mcp-menu-new/list` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/list-filter` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/list-unfilter` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/server-remote` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-inplace` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-no` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/remove-cancel` fx row 8: `  status    connecting`
- `p5-mcp-menu-new/left-back` fx row 7: `› remote                  connecting              profile`
- `p5-mcp-menu-new/list-proj` fx row 7: `  remote                  connecting              profile`
- `p5-mcp-menu-new/list-after-approve` fx row 8: `  remote                  connecting              profile`
- `p5-mcp-menu-new/reopened` fx row 8: `  remote                  connecting              profile`

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
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 0 | 0 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 1 | 1 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `filler-modifier-count`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
