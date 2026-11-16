## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-9b37d77 [debug] · Run /help for commands`, sha256 `aebd414e0d4e0766a686d9c24923ce74b8e949fd5a3ee4fc5806e1604717791b`
- size 40x120, `--jobs 8`, 3 scenarios -- **PARTIAL**: `--only p5-mode-case-*`, wall 0m40s, load 12.8 at the start and 12.8 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `93ae7ee589233cfe7cb9632ae49727d8bb97cb3670e8b4dcc4485ba04fb3a4ab`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 3 -- ran 3, failed 0, timeout 0. **pass = 5 / 7 = 71.4%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 3 | 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 | 71.4% |

Causes of the unresolved snaps:

2 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (2 snaps):
- `p5-mode-case-full/typed` fx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-mode-case-full/full` fx row 3: `! permissions: mode set to full access`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mode-case-full` 2/2 -- typed row 4: `─────────────────────────────────────────────────…` / (blank) (aligned: 7/7 rows unmatched)
