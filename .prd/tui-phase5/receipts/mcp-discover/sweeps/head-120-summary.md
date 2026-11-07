## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-d96603f [debug] · Run /help for commands`, sha256 `1b7d4b9ac86b53b3e4fcd271259c73f67a7d68c5a8da45b675d8521d5f26e437`
- size 40x120, `--jobs 2`, 39 scenarios -- **PARTIAL**: `--only p5-mcp-* p5-input-mcp`, wall 15m52s, load 22.0 at the start and 34.3 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `0deeef3bc5ce20fc3fe73225edd08c7f8c165afaa6fa3ae16b89c09bf7ce8117`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 39 -- ran 39, failed 0, timeout 0. **pass = 131 / 177 = 74.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 177 | 177 | 0 | 131 | 1 | 0 | 0 | 45 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 39 | 177 | 177 | 0 | 131 | 1 | 0 | 0 | 45 | 0 | 0 | 74.0% |

Causes of the unresolved snaps:

45 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 3 | 3 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |

Rows no cause covers (42 snaps):
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/list` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/end.full` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-discover-stdio-depth/list` fx row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-discover-stdio-escaped/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-fail/show-bad` xfx row 3: `* mcp: bad`
- `p5-mcp-fail/end.full` xfx row 3: `* mcp: bad`
- `p5-mcp-menu-old/prompts` fx row 7: `› remote                  connecting              profile`
- `p5-mcp-notes/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-notes/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-fixture` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-remote` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-proj` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/login-stdio` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched)
- `p5-mcp-config-ignored` 4/4 -- startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched)
- `p5-mcp-live` 3/6 -- cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched)
- `p5-mcp-config` 2/3 -- list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched)
- `p5-mcp-config-vars` 2/4 -- list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched)
- `p5-mcp-fail` 2/6 -- show-bad row 3: `┃` / `* mcp: bad` (aligned: 7/7 rows unmatched)
- `p5-mcp-notes` 2/4 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched)
- `p5-mcp-discover-stdio-depth` 1/1 -- list row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched)
- `p5-mcp-discover-stdio-escaped` 1/1 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched)
- `p5-mcp-menu-old` 1/16 -- prompts row 7: `› remote                  connecting             …` / `› remote                  retrying               …` (aligned: 1/1 rows unmatched)
