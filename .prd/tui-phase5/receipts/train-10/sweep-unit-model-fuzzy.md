# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-fc8ec00 [debug] · Run /help for commands`, sha256 `746268cedc1bad002eda74b59453f63375f05f70ee61c7ad48f67562efe47ece`
- size 40x120, `--jobs 2`, 39 scenarios -- **PARTIAL**: `--only *model* *effort* settings-model*`, wall 11m03s, load 6.4 at the start and 25.2 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `923cb2dc69057675e98920b87859782d52ed6f927a14ced4dde254e62b1c8ce1`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 39 -- ran 39, failed 0, timeout 0.

**pass = 204 / 233 = 87.6%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 233 | 233 | 1 | 203 | 0 | 2 | 0 | 27 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| h | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| model | 15 | 73 | 73 | 0 | 71 | 0 | 0 | 0 | 2 | 0 | 0 | 97.3% |
| p | 20 | 142 | 142 | 1 | 128 | 0 | 2 | 0 | 11 | 0 | 0 | 90.8% |
| settings | 3 | 16 | 16 | 0 | 2 | 0 | 0 | 0 | 14 | 0 | 0 | 12.5% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | 2 | 27 | 27 | 1 | 24 | 0 | 2 | 0 | 0 | 0 | 0 | 92.6% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | 5 | 41 | 41 | 1 | 28 | 0 | 2 | 0 | 10 | 0 | 0 | 70.7% |
| `/new` | 2 | 20 | 20 | 1 | 18 | 0 | 1 | 0 | 0 | 0 | 0 | 95.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | 6 | 45 | 45 | 1 | 32 | 0 | 2 | 0 | 10 | 0 | 0 | 73.3% |
| `/rename` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/fork` | 4 | 33 | 33 | 1 | 30 | 0 | 2 | 0 | 0 | 0 | 0 | 93.9% |
| `/undo` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/copy` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/compact` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/login` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/logout` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/provider` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/stats` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/usage` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/credits` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/model` | 21 | 123 | 123 | 0 | 113 | 0 | 0 | 0 | 10 | 0 | 0 | 91.9% |
| `/effort` | 8 | 51 | 51 | 0 | 51 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 1 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/ultrafast` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/settings` | 3 | 16 | 16 | 0 | 2 | 0 | 0 | 0 | 14 | 0 | 0 | 12.5% |
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
| `slash: ↑↓ navigate` | 14 | 151 | 151 | 1 | 133 | 0 | 2 | 0 | 15 | 0 | 0 | 88.7% |
| `slash: enter use` | 34 | 217 | 217 | 1 | 188 | 0 | 2 | 0 | 26 | 0 | 0 | 87.1% |
| `slash: esc close` | 23 | 147 | 147 | 1 | 122 | 0 | 2 | 0 | 22 | 0 | 0 | 83.7% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| h1-models2 | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model | ran | 12 | 12 | 0 | 11 | 0 | 0 | 0 | 1 | 0 | 0 | 91.7% | typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| model-anchor | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-hold | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-reask | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-effort | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-floor | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Thinking (<s>) (↑4 ↓0)` (aligned: 1/1 rows unmatched) |
| model-reask | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-router | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-backtab | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-down | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-loading | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-nolevels | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-short | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork | ran | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |  |
| p5-effort-fork-fork-continue | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork-edge | ran | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |  |
| p5-effort-fork-fork-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-oldread-src | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | picker-enter-fork row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 4/4 rows unmatched) |
| p5-effort-fork-oldread-v1 | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-oldread-v2 | ran | 7 | 7 | 0 | 2 | 0 | 0 | 0 | 5 | 0 | 0 | 28.6% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-slash-rank | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | e row 5: `Commands 23                                      …` / `Commands 21                                      …` (aligned: 1/1 rows unmatched) |
| p5-input-model | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-fail | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-rank | ran | 27 | 27 | 0 | 27 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model-swap | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-effort | ran | 8 | 8 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 0.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

27 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| sound-max | d | 14 | 14 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| C7-help-entries | b | 1 | 1 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| menu-count | a+b | 1 | 1 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| activity-tokens-before-request | b | 1 | 1 | `fx v0.0.16 publishes a turn's first token progress when its first request is prepared ('p…` | `ACTIVITY-TOKENS (a new queue unit): the activity row's token suffix appears with the turn's first request` |

Rows no cause covers (10 snaps):
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

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 4 | 2 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 4 | 2 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 8 | 2 |
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
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `answer-drawn-wider-named`, `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `inline-band-row-anchor`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `usage-no-sign-in`.
