## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0 [debug] · Run /help for commands`, sha256 `7ea12815f5cba784c9045a530c7a511551721ede417ae22e230137fe1e023059`
- size 40x120, `--jobs 2`, 2 scenarios -- **PARTIAL**: `--only p5-mcp-empty p5-mcp-verbs`, wall 2m50s, load 12.1 at the start and 12.1 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `b9a9c666b28987241e6434becde6f74821db28808a696cb207905ed476ddde6f`: 13 normalizations; 0 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/setup`, `/st`, `/vers`.

Scenarios: 2 -- ran 2, failed 0, timeout 0. **pass = 1 / 52 = 1.9%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 52 | 52 | 0 | 1 | 0 | 0 | 0 | 51 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 2 | 52 | 52 | 0 | 1 | 0 | 0 | 0 | 51 | 0 | 0 | 1.9% |

Causes of the unresolved snaps:

51 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| menu-shift | a+b | 1 | 1 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |

Rows no cause covers (50 snaps):
- `p5-mcp-empty/menu` fx row 5: `  MCP servers  none yet`
- `p5-mcp-empty/menu-down` fx row 5: `  MCP servers  none yet`
- `p5-mcp-empty/menu-right` fx row 5: `  MCP servers  none yet`
- `p5-mcp-empty/help` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/list` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/path` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/reload` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/auth-nope` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/trust-bare` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/resource-nope` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/prompt-bare` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/show-nope` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/login-nope` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/logout-nope` fx row 3: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/remove-nope` fx row 1: `* mcp: usage: /mcp COMMAND`
- `p5-mcp-empty/approve-bare` fx row 1: `    remove NAME                 remove a server and its login`
- `p5-mcp-empty/reject-nope` fx row 1: `    remove NAME                 remove a server and its login`
- `p5-mcp-empty/add-bare` xfx row 1: `<banner>`
- `p5-mcp-empty/add-help` xfx row 1: `<banner>`
- `p5-mcp-empty/list-connect` xfx row 1: `<banner>`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mcp-verbs` 28/28 -- startup row 3: `┃` / `Project MCP server 'proj' is defined in .mcp.json.` (aligned: 4/4 rows unmatched)
- `p5-mcp-empty` 23/24 -- complete row 7: `  /mcp   manage MCP servers and their logins     …` / `  /mcp   manage local and remote MCP servers, res…` (aligned: 1/1 rows unmatched)
