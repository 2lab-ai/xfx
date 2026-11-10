# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-b9b1159 [debug] · Run /help for commands`, sha256 `852b87398480e1af876213cfc357af36802accfc77d2325137bdb9123ee2cef7`
- size 40x120, `--jobs 3`, 12 scenarios -- **PARTIAL**: `--only slash* p5-slash* skills* p5-skills* p5-input-skill*`, wall 2m37s, load 13.3 at the start and 14.9 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `61e95c14978906f0d910a7480defc0a7a90f46ef3ca5c9a0636abff6f7117592`: 14 normalizations; 19 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/Help`, `/al`, `/balance`, `/co`, `/cost`, `/de`, `/dem`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/git`, `/he`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/notes`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`, `/x`.

## Pass

Scenarios: 12 -- ran 12, failed 0, timeout 0.

**pass = 68 / 124 = 54.8%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 124 | 124 | 0 | 68 | 0 | 0 | 0 | 56 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 6 | 65 | 65 | 0 | 36 | 0 | 0 | 0 | 29 | 0 | 0 | 55.4% |
| skills | 4 | 44 | 44 | 0 | 23 | 0 | 0 | 0 | 21 | 0 | 0 | 52.3% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 4 | 49 | 49 | 0 | 17 | 0 | 0 | 0 | 32 | 0 | 0 | 34.7% |
| `/clear` | 1 | 26 | 26 | 0 | 0 | 0 | 0 | 0 | 26 | 0 | 0 | 0.0% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/new` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/rename` | 1 | 26 | 26 | 0 | 0 | 0 | 0 | 0 | 26 | 0 | 0 | 0.0% |
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
| `/statusline` | 2 | 46 | 46 | 0 | 20 | 0 | 0 | 0 | 26 | 0 | 0 | 43.5% |
| `/sound` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/permissions` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/allowlist` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/workspace` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 1 | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/skills` | 2 | 44 | 44 | 0 | 2 | 0 | 0 | 0 | 42 | 0 | 0 | 4.5% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | 2 | 19 | 19 | 0 | 14 | 0 | 0 | 0 | 5 | 0 | 0 | 73.7% |
| `help: tab category` | 2 | 36 | 36 | 0 | 10 | 0 | 0 | 0 | 26 | 0 | 0 | 27.8% |
| `help: enter open` | 3 | 45 | 45 | 0 | 14 | 0 | 0 | 0 | 31 | 0 | 0 | 31.1% |
| `help: esc close` | 3 | 45 | 45 | 0 | 14 | 0 | 0 | 0 | 31 | 0 | 0 | 31.1% |
| `slash: ↑↓ navigate` | 5 | 71 | 71 | 0 | 21 | 0 | 0 | 0 | 50 | 0 | 0 | 29.6% |
| `slash: enter use` | 7 | 93 | 93 | 0 | 43 | 0 | 0 | 0 | 50 | 0 | 0 | 46.2% |
| `slash: esc close` | 6 | 89 | 89 | 0 | 41 | 0 | 0 | 0 | 48 | 0 | 0 | 46.1% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p5-slash-arg-skills | ran | 20 | 20 | 0 | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-leading-ws | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-leading-ws-he | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | spaces row 5: `Commands 2` / `Commands 1` (aligned: 3/3 rows unmatched) |
| p5-slash-leading-ws-short | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | four-all row 8: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-slash-leading-ws-skill | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-slash-mixed-skills | ran | 26 | 26 | 0 | 0 | 0 | 0 | 0 | 26 | 0 | 0 | 0.0% | git-log row 4: `─────────────────────────────────────────────────…` / (blank) (aligned: 10/10 rows unmatched) |
| skills | ran | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% | menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ghost | ran | 15 | 15 | 0 | 14 | 0 | 0 | 0 | 1 | 0 | 0 | 93.3% | dollar-menu row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-paste | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | paste row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-slash | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | slash-all row 5: `Results 40 · type to filter                      …` / `Results 36 · type to filter                      …` (aligned: 1/1 rows unmatched) |
| slash | ran | 9 | 9 | 0 | 4 | 0 | 0 | 0 | 5 | 0 | 0 | 44.4% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| slash-filter | ran | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | filter-mo row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |

## Causes of the unresolved snaps

56 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| menu-count | a+b | 20 | 4 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 6 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| C7-help-entries | b | 4 | 2 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| C8-shell | b | 3 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |

Rows no cause covers (44 snaps):
- `p5-slash-mixed-skills/git-log` fx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-slash-mixed-skills/git-log-tab` fx row 3: `┃ release-notes`
- `p5-slash-mixed-skills/git-log-enter` fx row 3: `┃ release-notes`
- `p5-slash-mixed-skills/slash-space` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/notes` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/notes-down` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/notes-down-tab` fx row 3: `┃ release-notes`
- `p5-slash-mixed-skills/rename-the-help` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/rename-the-help-enter` fx row 3: `┃ tidy`
- `p5-slash-mixed-skills/help` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/clear-the-notes` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/statusline` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/statusline-bell` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/cap-help` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/git-log-esc` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/git-log-esc-x` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/lead` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/lead-tab` fx row 3: `┃   release-notes`
- `p5-slash-mixed-skills/skills-x` xfx row 3: `┃ /git log`
- `p5-slash-mixed-skills/help-x` xfx row 3: `┃ /git log`

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
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 0 | 0 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 0 | 0 |
| ctrl-o-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-drawn-wider-named | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |
| answer-label-and-modifier-named | unsafe-to-copy | `CTRLO-WIDER-F` | `date:2026-12-04` | 0 | 0 |
| inline-band-row-anchor | unsafe-to-copy | `CTRLO-WIDER` | `date:2026-12-04` | 0 | 0 |

Approved for this pin but covering no row here: `answer-drawn-wider-named`, `answer-label-and-modifier-named`, `call-row-arabic-letter-mark`, `call-row-raw-argument`, `call-row-source-cut`, `ctrl-o-arabic-letter-mark`, `ctrl-o-command-marks`, `ctrl-o-drawn-wider-named`, `filler-modifier-count`, `inline-band-row-anchor`, `mcp-approval-wrap-led`, `review-diff-spelling`, `saved-write-rule-applied`, `status-panel-sandbox`, `status-panel-session-id-wrap`, `status-panel-sign-in`, `status-panel-step-limit`, `status-panel-updates`, `usage-no-sign-in`.
