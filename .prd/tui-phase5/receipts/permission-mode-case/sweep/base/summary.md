## Scoreboard (v0.0.16)

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-9c64ddd [debug] · Run /help for commands`, sha256 `6e30565a71f9eab9efee5fc2e08124a9b660bf844a1fc5a4138e709b46d26b30`
- size 40x120, `--jobs 8`, 3 scenarios -- **PARTIAL**: `--only p5-mode-case-*`, wall 0m37s, load 9.0 at the start and 9.0 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `93ae7ee589233cfe7cb9632ae49727d8bb97cb3670e8b4dcc4485ba04fb3a4ab`: 14 normalizations; 19 divergences approved for this pin

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

Scenarios: 3 -- ran 2, failed 1, timeout 0. **pass = 0 / 7 = 0.0%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 7 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 1 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output. Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p | 3 | 7 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 1 | 0 | 0.0% |

Causes of the unresolved snaps:

6 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|

Rows no cause covers (6 snaps):
- `p5-mode-case-full/typed` fx row 4: `───────────────────────────────────────────────────────────────────────────────…`
- `p5-mode-case-full/full` fx row 3: `! permissions: mode set to full access`
- `p5-mode-case-key/start` xfx row 3: `* config: 1 configuration issue (ctrl+o to view)`
- `p5-mode-case-key/full-access-upper` xfx row 3: `* config: 1 configuration issue (ctrl+o to view)`
- `p5-mode-case-key/auto-cap` xfx row 3: `* config: 1 configuration issue (ctrl+o to view)`
- `p5-mode-case-key/full-access-space` xfx row 3: `* config: 1 configuration issue (ctrl+o to view)`

Most unresolved snaps (scenario, unresolved of total, its first unresolved row):

- `p5-mode-case-key` 4/4 -- start row 3: `┃` / `* config: 1 configuration issue (ctrl+o to view)` (aligned: 3/3 rows unmatched)
- `p5-mode-case-full` 2/2 -- typed row 4: `─────────────────────────────────────────────────…` / (blank) (aligned: 7/7 rows unmatched)
- `p5-mode-case-env` failed: xfx: exit 1
