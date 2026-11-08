## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-0b4807b [debug] · Run /help for commands`, sha256 `4391d4e6ee17adfd0d52dd9678044cd5c57e280b14a4a5f4383eaa47c3e5c5b1`
- size 40x120, `--jobs 2`, 317 scenarios, wall 171m57s, load 11.3 at the start and 34.1 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `35a5883249b8ae9fe36fdd1e19d63a15b9b7b17dc5531c300b563d8311e47d71`: 14 normalizations; 17 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 317 -- ran 307, failed 1, timeout 9. **pass = 1026 / 1704 = 60.2%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 1704 | 1672 | 37 | 989 | 17 | 26 | 0 | 603 | 2 | 30 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | 1 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| allowlist | 5 | 25 | 25 | 0 | 23 | 0 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| answered | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| approvals | 3 | 18 | 18 | 0 | 1 | 0 | 0 | 0 | 17 | 0 | 0 | 5.6% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| compact | 7 | 25 | 25 | 2 | 23 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| copy | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| credits | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| filepicker | 3 | 20 | 20 | 0 | 8 | 0 | 0 | 0 | 12 | 0 | 0 | 40.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| h | 15 | 38 | 36 | 0 | 15 | 0 | 1 | 0 | 20 | 0 | 2 | 39.5% |
| help | 1 | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| interrupt | 12 | 61 | 61 | 0 | 28 | 0 | 0 | 0 | 33 | 0 | 0 | 45.9% |
| links | 2 | 9 | 9 | 2 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 88.9% |
| model | 14 | 69 | 69 | 0 | 63 | 0 | 0 | 0 | 6 | 0 | 0 | 91.3% |
| notice | 3 | 8 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 50.0% |
| notices | 1 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 4 | 0.0% |
| p | 137 | 786 | 781 | 16 | 416 | 17 | 24 | 0 | 308 | 2 | 3 | 55.0% |
| paste | 3 | 17 | 17 | 1 | 14 | 0 | 0 | 0 | 2 | 0 | 0 | 88.2% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| popup | 1 | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% |
| prose | 1 | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| provider | 6 | 41 | 41 | 0 | 9 | 0 | 0 | 0 | 32 | 0 | 0 | 22.0% |
| question | 3 | 26 | 26 | 0 | 22 | 0 | 0 | 0 | 4 | 0 | 0 | 84.6% |
| quit | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| recall | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| rename | 2 | 8 | 8 | 1 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 87.5% |
| reset | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 8 | 20 | 0 | 0 | 0 | 21 | 0 | 0 | 57.1% |
| rules | 4 | 19 | 19 | 1 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 42.1% |
| search | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0.0% |
| settings | 12 | 72 | 60 | 0 | 25 | 0 | 0 | 0 | 35 | 0 | 12 | 34.7% |
| skills | 4 | 44 | 44 | 0 | 23 | 0 | 0 | 0 | 21 | 0 | 0 | 52.3% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |
| sound | 2 | 13 | 13 | 0 | 4 | 0 | 0 | 0 | 9 | 0 | 0 | 30.8% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| status | 1 | 8 | 8 | 0 | 1 | 0 | 1 | 0 | 6 | 0 | 0 | 12.5% |
| statusline | 2 | 11 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 3 | 72.7% |
| stream | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| table | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 6 | 15 | 15 | 0 | 12 | 0 | 0 | 0 | 3 | 0 | 0 | 80.0% |
| toolgroup | 3 | 17 | 17 | 0 | 6 | 0 | 0 | 0 | 11 | 0 | 0 | 35.3% |
| torn | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| undo | 3 | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 4 | 30 | 30 | 0 | 30 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 2 | 13 | 13 | 0 | 11 | 0 | 0 | 0 | 2 | 0 | 0 | 84.6% |

Causes of the unresolved snaps:

603 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 66 | 43 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| tool-schema | d | 45 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| approval-grant-scope | a | 42 | 20 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| menu-count | a+b | 40 | 10 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| sound-max | d | 39 | 38 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| truecolor-grey-notation | b | 34 | 25 | `Under COLORTERM=truecolor xfx spells its own greys -- the card's and the composer's rail,…` | `TC-NOTATION builtin greys spelt 38;5 at every depth` |
| stats-bytes | d | 34 | 27 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1583 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| session-record | d | 31 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37458 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| menu-shift | a+b | 29 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| C7-help-entries | b | 20 | 4 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37458; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| provider-label | d | 15 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| grant-scope-push | a | 14 | 4 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| C8-shell | b | 5 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| C7-help-search | b | 4 | 3 | `/help's search: fx matches a query against each command's help entry (helpCatalogSpecMatc…` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| K-recover | b | 4 | 4 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| toolgroup-ctrlo-record-short | d | 3 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37458 / fx src/core/execution/comman…` |
| upgrade-update-alias | d | 3 | 3 | `The scenario records what fx v0.0.16 prints for 'fx update --help', 'fx upgrade --help' a…` | `unowned (the upgrade command, deferred: docs/parity.md 'upgrade') -- xfx docs/parity.md:63 / fx src/builtins/commands.zig:257-260@c1db9191` |
| P1-carry-notice | b | 2 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| vision-route | a | 2 | 2 | `For a model without native image input fx forces a 'vision' call answered by a second mod…` | `user decision second-model image route (not yet on the program's decision list)` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| allowlist-band | d | 2 | 2 | `Once /allowlist's notices fill the screen fx's band stands three rows above the bottom an…` | `unowned (not traced to the line) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:39-60@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| help-login-args | a | 1 | 0 | `Enter on the Account tab's first row opens fx's /login picker; xfx's first row there is /…` | `user decision Codex/Grok sign-in (#88)` |
| call-row-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-call-row-safe''s tail page: fx's command entry has two a…` | `unowned (follows tool-schema) -- xfx src/tui/approval_view.rs:685; src/tui/toolgroup.rs:750 / fx src/core/execution/command_environment.zig…` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |

Rows no cause covers (272 snaps):
- `approvals/after-esc` xfx row 15: `(blank)`
- `approvals/after-esc.full` xfx row 15: `(blank)`
- `filepicker/at` fx row 11: `  src/deep/sched.rs`
- `filepicker/at-sch` fx row 11: `(blank)`
- `filepicker/at-dir` fx row 5: `      src/`
- `filepicker/at-dir-down` fx row 5: `      src/`
- `filepicker/at-dir-cj` fx row 5: `      src/`
- `filepicker/at-tab` fx row 3: `┃ see @src/lib.rs`
- `filepicker/at-claim` fx row 17: `(blank)`
- `filepicker-keys/at-pgdn` fx row 11: `  src/deep/sched.rs`
- `filepicker-keys/at-ck` fx row 5: `  Cargo.toml`
- `filepicker-narrow/at-alpha` fx row 11: `(blank)`
- `filepicker-narrow/at-alpha-down` fx row 11: `(blank)`
- `filepicker-narrow/at-deep` fx row 11: `(blank)`
- `h1-mcphttp/hanging` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/failed` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/rpcerror` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-methodnotfound/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-panels-sweep-a` 28/33 -- stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2655, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched)
- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched)
- `skills` 16/18 -- menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched)
- `resume-tools` 15/18 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `p5-transcript-resize` 13/14 -- full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched)
- `p5-mcp-menu-new` 11/34 -- list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched)
- `toolgroup` 11/13 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `approvals` 10/10 -- edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched)
- `help` 10/19 -- open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched)
- `p5-panels-slash` 10/15 -- slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched)
- `h1-usage-codex` timeout: fx: past its 74s deadline; xfx: past its 74s deadline
- `h1-usage-grok` timeout: fx: past its 74s deadline
- `notice-midstream` timeout: xfx: past its 75s deadline
- `notices` timeout: xfx: past its 75s deadline
- `p5-call-row-long` timeout: xfx: past its 75s deadline
- `p5-panels-badsettings` failed: fx: exit 1
- `p5-transcript-tallfirst` timeout: fx: past its 75s deadline; xfx: past its 75s deadline
- `search-roots` timeout: fx: past its 75s deadline
- `settings2` timeout: fx: past its 75s deadline
- `statusline-turn` timeout: xfx: past its 75s deadline
