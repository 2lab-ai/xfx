## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-58b463c [debug] · Run /help for commands`, sha256 `c23043c48a604d7b8270907a1d6891f7177e04a479e73ec9dc72757b83d3313d`
- size 40x120, `--jobs 2`, 39 scenarios -- **PARTIAL**: `--only p5-mcp-* p5-input-mcp`, wall 16m21s, load 21.2 at the start and 29.4 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `0deeef3bc5ce20fc3fe73225edd08c7f8c165afaa6fa3ae16b89c09bf7ce8117`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 39 -- ran 39, failed 0, timeout 0. **pass = 99 / 177 = 55.9%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 177 | 177 | 0 | 99 | 1 | 0 | 0 | 77 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 39 | 177 | 177 | 0 | 99 | 1 | 0 | 0 | 77 | 0 | 0 | 55.9% |

Causes of the unresolved snaps:

77 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 3 | 3 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |

Rows no cause covers (74 snaps):
- `p5-mcp-config/list` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config/removed.full` fx row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/startup` fx row 5: `auto · model-a                                                                 …`
- `p5-mcp-config-ignored/menu` fx row 7: `  2 config notes: /mcp list shows them`
- `p5-mcp-config-ignored/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-ignored/end.full` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/list` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-config-vars/end.full` fx row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS`
- `p5-mcp-discover-http-hostile/show` fx row 4: `    status     retrying`
- `p5-mcp-discover-http-modern/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-modern/turn` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-notimpl/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-notimpl/turn` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-old2024/show` fx row 8: `    error      it answered with an MCP version xfx doesn't support (xfx speaks …`
- `p5-mcp-discover-http-ssefail/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-ssehold/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-http-ssestrid/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-stdio-banner/show` fx row 4: `    status     ready, 1 tool`
- `p5-mcp-discover-stdio-depth/list` fx row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-discover-stdio-escaped/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 27/28 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched)
- `p5-mcp-menu-new` 11/34 -- list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched)
- `p5-mcp-menu-old` 7/16 -- list row 7: `  remote                  connecting             …` / `  remote                  retrying               …` (aligned: 1/1 rows unmatched)
- `p5-mcp-config-ignored` 4/4 -- startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched)
- `p5-mcp-live` 3/6 -- cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched)
- `p5-mcp-config` 2/3 -- list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 4/4 rows unmatched)
- `p5-mcp-config-vars` 2/4 -- list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched)
- `p5-mcp-discover-http-modern` 2/2 -- show row 4: `    status     ready, 1 tool` / `    status     retrying` (aligned: 4/4 rows unmatched)
- `p5-mcp-discover-http-notimpl` 2/2 -- show row 4: `    status     ready, 1 tool` / `    status     retrying` (aligned: 4/4 rows unmatched)
- `p5-mcp-discover-stdio-modern` 2/2 -- show row 4: `    status     ready, 1 tool` / `    status     retrying` (aligned: 4/4 rows unmatched)
