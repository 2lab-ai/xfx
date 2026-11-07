## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-127f050 [debug] · Run /help for commands`, sha256 `ef1e472d35cf1dea21099605ad8c3e039e5f7199354c066eca36e0a78c16cae1`
- size 40x120, `--jobs 1`, 1 scenarios -- **PARTIAL**: `--only p5-transcript-safe`, wall 1m00s, load 3.2 at the start and 3.6 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `30de9fddba01f44b4b4f66749df44a4e4c644240d780c4cca2e766263d56e8f5`: 14 normalizations; 8 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 1 -- ran 1, failed 0, timeout 0. **pass = 0 / 4 = 0.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 1 | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% |

Causes of the unresolved snaps:

4 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| ctrl-o-call-row-spelling | d | 4 | 2 | `A call's own row -- in the transcript and as Ctrl-O's heading -- shows its argument throu…` | `unowned (the call row's argument, src/tui/toolgroup.rs 'spelled', whose length T-U4 owns) -- xfx src/tui/toolgroup.rs:538 / fx src/core/too…` |
| tool-schema | d | 2 | 0 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| session-record | d | 2 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:32298 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| safe-page-edge | d | 2 | 0 | `'transcript-page-offset' for 'p5-transcript-safe', four rows deep: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:32298 / fx src/core/execution/comman…` |

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-transcript-safe` 4/4 -- end row 8: `├ Ran printf '%s\n' 'A\u{202e}B\u{200b}C\u{feff}D…` / `├ Ran printf '%s\n' 'A<U+202E>B<U+200B>C<U+FEFF>D [2JE F G' >/dev/null…` (aligned: 1/1 rows unmatched)
