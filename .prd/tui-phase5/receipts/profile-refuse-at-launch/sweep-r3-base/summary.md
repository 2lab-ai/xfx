## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-3943d66 [debug] · Run /help for commands`, sha256 `adb2401b268b639811391e4d7d4d2decaf616bb92aafb21f16cc280376919112`
- size 40x120, `--jobs 2`, 83 scenarios -- **PARTIAL**: `--only p5-panels* settings* startup* config* approvals-amend-limit approvals-big approvals cmd-wrap composer-keys fast-astra fast-opus fast-override fast-unavailable fast-unconfirmed fast-unsup fulltranscript h1-rehomefile model-anchor model-busy-known model-busy model-column model-effort model-empty model-router model-speed model p5-effort-fork-effort-backtab p5-effort-fork-effort-column p5-effort-fork-effort-ctrlc p5-effort-fork-effort-down p5-effort-fork-effort-loading p5-effort-fork-effort-menu p5-effort-fork-effort-short p5-input-approval p5-input-paste-amend p5-input-row-limit-menus p5-lifecycle-osc-hostile p5-lifecycle-osc-states p5-lifecycle-osc p5-panels-badsettings p5-panels-status-badsettings p5-session-early-prefs p5-stop-in-catalog-wait p5-usage-codex provider-carry-back-stop provider-carry-back provider-carry provider-resume-missing provider-resume provider-switch provider resume-tools rules-hidden-all rules-hidden rules-read-auto rules-read settings-collapse-open settings-collapse settings-effort settings-speed titles-idle titles-off titles-rename titles-steer titles toolgroup-noop toolgroup`, wall 30m15s, load 13.6 at the start and 17.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `93ae7ee589233cfe7cb9632ae49727d8bb97cb3670e8b4dcc4485ba04fb3a4ab`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 83 -- ran 83, failed 0, timeout 0. **pass = 277 / 523 = 53.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 523 | 523 | 5 | 272 | 3 | 24 | 0 | 219 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| approvals | 3 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| composer | 1 | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fulltranscript | 1 | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% |
| h | 1 | 2 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 50.0% |
| model | 9 | 51 | 51 | 0 | 50 | 0 | 0 | 0 | 1 | 0 | 0 | 98.0% |
| p | 29 | 220 | 220 | 4 | 108 | 3 | 23 | 0 | 82 | 0 | 0 | 50.9% |
| provider | 7 | 44 | 44 | 0 | 9 | 0 | 0 | 0 | 35 | 0 | 0 | 20.5% |
| resume | 1 | 18 | 18 | 0 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 38.9% |
| rules | 4 | 19 | 19 | 1 | 12 | 0 | 0 | 0 | 6 | 0 | 0 | 68.4% |
| settings | 12 | 72 | 72 | 0 | 30 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| titles | 5 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| toolgroup | 2 | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% |

Causes of the unresolved snaps:

219 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| approval-grant-scope | a | 44 | 23 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| sound-max | d | 39 | 38 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| tool-schema | d | 24 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| stats-bytes | d | 23 | 16 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1854 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| menu-count | a+b | 21 | 5 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| menu-shift | a+b | 15 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| full-transcript-shift | d | 12 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:41113; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| grant-scope-push | a | 11 | 6 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| provider-label | d | 10 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:117 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| C7-help-entries | b | 8 | 2 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| P1-carry-notice | b | 3 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| session-record | d | 3 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:41113 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| C8-shell | b | 2 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| amendment-card-after-stop | b+d | 2 | 2 | `A user-visible row. An amendment typed at an approval is a prompt card on both: fx writes…` | `AMEND-CARD-ROW the blank row xfx keeps under an approval's amendment card that ends the document -- xfx src/tui/shell.rs:10480 / fx src/cor…` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |

Rows no cause covers (75 snaps):
- `p5-input-approval/cmd-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/edit-ask` fx row 5: `● 1 tool call · 1 command`
- `p5-input-approval/after` fx row 5: `● 2 tool calls · 1 edit · 1 command`
- `p5-input-paste-amend/end` xfx row 17: `(blank)`
- `p5-lifecycle-osc/s00-start` fx row 5: `ask · model-a`
- `p5-lifecycle-osc/s01-working` fx row 9: `ask · model-a`
- `p5-lifecycle-osc/s02-done` fx row 11: `ask · model-a`
- `p5-lifecycle-osc/s04-error` fx row 17: `⚠ API request failed · HTTP 400 · Synthetic bad request`
- `p5-lifecycle-osc/s05-permission` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s06-after-permission` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s07-question` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s08-after-question` xfx row 17: `✗ system: the Gateway returned HTTP 400: {"error":{"message":"Synthetic bad req…`
- `p5-lifecycle-osc/s09-exit` xfx row 1: `<banner>`
- `p5-panels-badsettings/launch` xfx row 1: `<banner>`
- `p5-panels-slash/filter-f` xfx row 12: `(blank)`
- `p5-panels-sweep-a/shell` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/rename` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/allowlist` fx row 23: `* shell: The next command reloads your shell startup files. Remembered command …`
- `p5-panels-sweep-a/compact-empty` fx row 1: `<banner>`
- `p5-panels-sweep-a/trace` fx row 1: `* version: <build>`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-panels-sweep-a` 28/33 -- stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2682, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched)
- `resume-tools` 11/18 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `approvals` 10/10 -- edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `p5-panels-slash` 10/15 -- slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched)
- `provider` 10/15 -- open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched)
- `fulltranscript` 9/15 -- cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `p5-lifecycle-osc` 9/9 -- s00-start row 5: `ask · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched)
- `toolgroup` 9/13 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `settings-effort` 8/8 -- bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched)
- `p5-panels-help` 7/17 -- open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
