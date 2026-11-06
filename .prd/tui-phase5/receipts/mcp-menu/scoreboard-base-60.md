# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-6df717a [debug] · Run /help for commands`, sha256 `b9e952df6da047b655337e8e281875f499f94df6f8caa78d2b7c57b41bec1390`
- size 40x60, `--jobs 1`, 1 scenarios -- **PARTIAL**: `--only p5-mcp-menu-new`, wall 2m23s, load 2.2 at the start and 4.0 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `b9a9c666b28987241e6434becde6f74821db28808a696cb207905ed476ddde6f`: 13 normalizations; 0 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 1 -- ran 1, failed 0, timeout 0.

**pass = 0 / 34 = 0.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 1 | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/clear` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/status` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/version` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/quit` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/new` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/reset` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/resume` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/rename` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
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
| `/statusline` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/sound` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/permissions` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/allowlist` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/workspace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/shell` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/image` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/images` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/paste` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/mcp` | 1 | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% |
| `/skills` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/alias` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/feedback` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `/trace` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: ↑↓ navigate` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: tab category` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: enter open` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `help: esc close` | **none** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0.0% |
| `slash: ↑↓ navigate` | 1 | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% |
| `slash: enter use` | 1 | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% |
| `slash: esc close` | 1 | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 34 | 0 | 0 | 0.0% | startup row 3: `┃` / `Project MCP server 'proj' is defined in .mcp.json.` (aligned: 6/6 rows unmatched) |

## Causes of the unresolved snaps

34 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (34 snaps):
- `p5-mcp-menu-new/startup` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/startup-settled` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/list` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/list-filter` fx row 3: `┃ re`
- `p5-mcp-menu-new/list-unfilter` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-fixture` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-fixture-tool` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-fixture-nomatch` fx row 3: `┃ zzz`
- `p5-mcp-menu-new/back-list` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/list-remote` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-remote` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/remove-inplace` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/remove-no` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/remove-cancel` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/left-back` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/list-proj` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-proj` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/approve-confirm` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/approve-esc` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`
- `p5-mcp-menu-new/server-proj-reject` xfx row 3: `Project MCP server 'proj' is defined in .mcp.json.`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
