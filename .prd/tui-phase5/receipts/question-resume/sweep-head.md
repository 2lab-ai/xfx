## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-63e8bbe [debug] · Run /help for commands`, sha256 `8b3ce14f085420576d4865e2b0607756a0e80ae29ec70ceaf4622753514bbd7b`
- size 40x120, `--jobs 4`, 22 scenarios -- **PARTIAL**: `--only *question* *resume*`, wall 6m45s, load 36.2 at the start and 48.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `598bf37e52af79abdb375ab0f561f14eb9443ea4972dd1dfc18eddb7179812eb`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 22 -- ran 22, failed 0, timeout 0. **pass = 91 / 125 = 72.8%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 125 | 125 | 11 | 80 | 0 | 0 | 0 | 34 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| interrupt | 2 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 8 | 36 | 36 | 2 | 24 | 0 | 0 | 0 | 10 | 0 | 0 | 72.2% |
| provider | 2 | 9 | 9 | 0 | 2 | 0 | 0 | 0 | 7 | 0 | 0 | 22.2% |
| question | 3 | 26 | 26 | 0 | 26 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 8 | 24 | 0 | 0 | 0 | 17 | 0 | 0 | 65.3% |
| undo | 1 | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |

Causes of the unresolved snaps:

34 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| full-transcript-shift | d | 6 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:39046; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| tool-schema | d | 5 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| approval-grant-scope | a | 4 | 2 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| session-record | d | 3 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:39046 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| grant-scope-push | a | 2 | 1 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| transcript-page-offset | d | 2 | 0 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:39046 / fx src/core/execution/comman…` |
| question-resume-ctrlo-detail | b | 1 | 1 | `fx v0.0.16 hands each replayed 'ask_user_question' call its saved detail ('attachHistoric…` | `QUESTION-RESUME-CTRLO a resumed ask's saved detail in Ctrl-O (docs/parity.md:162), not yet a branch` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |

Rows no cause covers (13 snaps):
- `p5-lifecycle-v1resume/v00b-picker-all` fx row 5: `Sessions 1  Current workspace  [All workspaces]`
- `p5-lifecycle-v1resume/v01-resumed` fx row 1: `* session resumed: CONV_FIRST 1`
- `p5-lifecycle-v1resume/v01-resumed.full` fx row 2: `* session resumed: CONV_FIRST 1`
- `p5-lifecycle-v1resume/v02-continued` fx row 1: `* session resumed: CONV_FIRST 1`
- `p5-transcript-question/asked` fx row 9: `    1) Postgres with the             Relational, strong consistency`
- `p5-transcript-question/narrow` fx row 1: `<banner>`
- `provider-resume/switched` fx row 12: `* provider: Preparing Codex subscription.`
- `provider-resume/turn2` fx row 12: `* provider: Preparing Codex subscription.`
- `provider-resume/resumed` fx row 20: `auto · gpt-5.6-sol · read the notes and tell me what`
- `provider-resume/turn3` fx row 26: `auto · gpt-5.6-sol · read the notes and tell me what  · 1k/272k 0%`
- `provider-resume-missing/turn2` fx row 12: `* provider: Preparing Codex subscription.`
- `provider-resume-missing/resumed` fx row 18: `! auth: Codex needs a subscription login. Run /login, open Connections, then ch…`
- `provider-resume-missing/refused` fx row 18: `! auth: Codex needs a subscription login. Run /login, open Connections, then ch…`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `resume-tools` 11/18 -- cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched)
- `resume-paging` 6/8 -- page1 row 24: `  question one          work · now · 1 turn` / `  question one          work ·  1m · 1 turn` (aligned: 1/1 rows unmatched)
- `p5-lifecycle-v1resume` 4/5 -- v00b-picker-all row 5: `Sessions 1  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 2/2 rows unmatched)
- `p5-transcript-question` 4/5 -- asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched)
- `provider-resume` 4/6 -- switched row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched)
- `provider-resume-missing` 3/3 -- turn2 row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched)
- `p5-lifecycle-relhome-resume` 1/1 -- r00-resume.full row 2: (blank) / `xfx: HomeNotSet` (aligned: 1/1 rows unmatched)
- `p5-question-resume-ctrlo` 1/1 -- resumed-ctrlo row 9: `│  questions: [{"question":"Solo 1?","options":[{…` / (blank) (aligned: 6/6 rows unmatched)
