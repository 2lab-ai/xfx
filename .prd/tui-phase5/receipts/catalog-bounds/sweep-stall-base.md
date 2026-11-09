## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-ccced1f [debug] · Run /help for commands`, sha256 `e6d36b282f0ac117d6283aa5a72d79ee6a8fbd4f2beef8d5bbeeafd444ae87c0`
- size 40x120, `--jobs 1`, 1 scenarios -- **PARTIAL**: `--only p5-catalog-bounds-stall`, wall 1m34s, load 10.7 at the start and 10.7 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `3821cfc8eec3c9d710d109d3b0e47b4cea1a5fd965ea2540c9f517b08a97c0e3`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 1 -- ran 1, failed 0, timeout 0. **pass = 1 / 2 = 50.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 1 | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% |

Causes of the unresolved snaps:

1 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (1 snaps):
- `p5-catalog-bounds-stall/timed-out` fx row 7: `Could not reach AI Gateway; xfx retries automatically.`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-catalog-bounds-stall` 1/2 -- timed-out row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Unable to load models.` (aligned: 1/1 rows unmatched)
