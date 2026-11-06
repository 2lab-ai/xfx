## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-9034e3b [debug] · Run /help for commands`, sha256 `ea0e6b380f0f0144fe3fa1297a59d0f7f2393a092eaaf76b62bd24e64d08fe45`
- size 40x120, `--jobs 2`, 2 scenarios -- **PARTIAL**: `--only p5-mcp-empty p5-mcp-verbs`, wall 2m51s, load 10.0 at the start and 10.0 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `b9a9c666b28987241e6434becde6f74821db28808a696cb207905ed476ddde6f`: 13 normalizations; 0 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/setup`, `/st`, `/vers`.

Scenarios: 2 -- ran 2, failed 0, timeout 0. **pass = 21 / 52 = 40.4%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 52 | 52 | 0 | 21 | 0 | 0 | 0 | 31 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 2 | 52 | 52 | 0 | 21 | 0 | 0 | 0 | 31 | 0 | 0 | 40.4% |

Causes of the unresolved snaps:

31 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (31 snaps):
- `p5-mcp-empty/menu` fx row 5: `  MCP servers  none yet`
- `p5-mcp-empty/menu-down` fx row 5: `  MCP servers  none yet`
- `p5-mcp-empty/menu-right` fx row 5: `  MCP servers  none yet`
- `p5-mcp-verbs/startup` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-verbs/list` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-fixture` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-remote` fx row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`
- `p5-mcp-verbs/show-proj` fx row 1: `<banner>`
- `p5-mcp-verbs/login-stdio` fx row 1: `<banner>`
- `p5-mcp-verbs/login-remote` fx row 1: `  remote   ~/.xfx/mcp.json  http       connecting`
- `p5-mcp-verbs/auth-remote` fx row 1: `  remote   ~/.xfx/mcp.json  http       connecting`
- `p5-mcp-verbs/logout-remote` xfx row 10: `* mcp: remote`
- `p5-mcp-verbs/add-new-syntax` xfx row 10: `* mcp: remote`
- `p5-mcp-verbs/add-old-syntax` xfx row 7: `* mcp: remote`
- `p5-mcp-verbs/add-old-http` xfx row 1: `* mcp: remote`
- `p5-mcp-verbs/add-new-http` xfx row 1: `    transport  http`
- `p5-mcp-verbs/add-badname` xfx row 1: `    status     waiting for approval`
- `p5-mcp-verbs/approve` fx row 1: `    error      it refused or never answered xfx's initialize request`
- `p5-mcp-verbs/trust-approve` fx row 1: `    error      it refused or never answered xfx's initialize request`
- `p5-mcp-verbs/list-after` fx row 31: `* mcp: NAME     SOURCE          TRANSPORT  STATUS`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 28/28 -- startup row 3: `┃` / `Project MCP server 'proj' is defined in .mcp.json.` (aligned: 4/4 rows unmatched)
- `p5-mcp-empty` 3/24 -- menu row 5: `  MCP servers  none yet` / `MCP 0  [Servers]  Tools  Resources  Prompts` (aligned: 4/4 rows unmatched)
