## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-d96603f [debug] · Run /help for commands`, sha256 `1b7d4b9ac86b53b3e4fcd271259c73f67a7d68c5a8da45b675d8521d5f26e437`
- size 40x60, `--jobs 2`, 1 scenarios -- **PARTIAL**: `--only p5-mcp-menu-new`, wall 1m56s, load 20.7 at the start and 20.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `0deeef3bc5ce20fc3fe73225edd08c7f8c165afaa6fa3ae16b89c09bf7ce8117`: 14 normalizations; 15 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 1 -- ran 1, failed 0, timeout 0. **pass = 33 / 34 = 97.1%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 34 | 34 | 0 | 33 | 1 | 0 | 0 | 0 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 1 | 34 | 34 | 0 | 33 | 1 | 0 | 0 | 0 | 0 | 0 | 97.1% |

Causes of the unresolved snaps:

0 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

