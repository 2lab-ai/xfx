## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-f3a859c [debug] · Run /help for commands`, sha256 `86ace79aa6e40f98b8a2cd0dc186f2c7e116f3b7db52192d5ab9b8a6a42cbbf1`
- size 40x120, `--jobs 2`, 19 scenarios -- **PARTIAL**: `--only p5-mcp-discover-* p5-mcp-wire-new`, wall 4m53s, load 24.7 at the start and 33.4 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `0deeef3bc5ce20fc3fe73225edd08c7f8c165afaa6fa3ae16b89c09bf7ce8117`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 19 -- ran 19, failed 0, timeout 0. **pass = 17 / 23 = 73.9%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 23 | 23 | 0 | 17 | 0 | 0 | 0 | 6 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 19 | 23 | 23 | 0 | 17 | 0 | 0 | 0 | 6 | 0 | 0 | 73.9% |

Causes of the unresolved snaps:

6 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (6 snaps):
- `p5-mcp-discover-http-ssehold/show` fx row 3: `* mcp: fixture`
- `p5-mcp-discover-http-sseidless/show` fx row 3: `* mcp: fixture`
- `p5-mcp-discover-http-ssestrid/show` fx row 3: `* mcp: fixture`
- `p5-mcp-discover-stdio-banner/show` fx row 3: `* mcp: fixture`
- `p5-mcp-discover-stdio-depth/list` fx row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS`
- `p5-mcp-discover-stdio-escaped/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-discover-http-ssehold` 1/1 -- show row 3: `* mcp: fixture` / `┃` (aligned: 9/9 rows unmatched)
- `p5-mcp-discover-http-sseidless` 1/1 -- show row 3: `* mcp: fixture` / `┃` (aligned: 9/9 rows unmatched)
- `p5-mcp-discover-http-ssestrid` 1/1 -- show row 3: `* mcp: fixture` / `┃` (aligned: 9/9 rows unmatched)
- `p5-mcp-discover-stdio-banner` 1/1 -- show row 3: `* mcp: fixture` / `┃` (aligned: 10/10 rows unmatched)
- `p5-mcp-discover-stdio-depth` 1/1 -- list row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched)
- `p5-mcp-discover-stdio-escaped` 1/1 -- list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched)
