# fx parity sweep

- fx: pin `v0.0.16` (fx 0.0.16), sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`, checked before and after the run
- xfx: `x𝒇x v0.1.0-7589ec4 [debug] · Run /help for commands`, sha256 `69094eff4242d0192f61ea7348cb578b06d564ce8f50ac3d8cd0616fef21f807`
- size 40x120, `--jobs 2`, 332 scenarios, wall 138m28s, load 4.6 at the start and 25.1 at the peak (1-minute average, sampled every 5 s)
- ledger sha256 `a864bd1564dabffd182f3fb199bffee165c5fc169a54b01461ed3d32cd29a42c`: 14 normalizations; 15 divergences approved for this pin

## Coverage

fx's inventory (v0.0.16): 45 surfaces -- 38 commands, 7 key hints. Covered by a scenario: 45. **Uncovered: 0**.
Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): `/al`, `/balance`, `/co`, `/cost`, `/de`, `/e`, `/ef`, `/eff`, `/et`, `/ex`, `/exit`, `/f`, `/fo`, `/img`, `/mo`, `/nosuch`, `/notacommand`, `/q`, `/re`, `/se`, `/setup`, `/st`, `/vers`.

## Pass

Scenarios: 332 -- ran 331, failed 1, timeout 0.

**pass = 1102 / 1716 = 64.2%**

| total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout |
|---|---|---|---|---|---|---|---|---|---|
| 1716 | 1714 | 39 | 1063 | 13 | 26 | 0 | 573 | 2 | 0 |

The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a scenario that failed or timed out counts every snap it would have taken. pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two sides aligned as sequences, is approved; the positional diff stays in the raw output.

## By group

Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). A scenario counts under every surface it exercises, so the surface rows overlap.

| group | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | 1 | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% |
| allowlist | 5 | 25 | 25 | 0 | 23 | 0 | 0 | 0 | 2 | 0 | 0 | 92.0% |
| answered | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| approvals | 3 | 18 | 18 | 0 | 1 | 0 | 0 | 0 | 17 | 0 | 0 | 5.6% |
| card | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| cmd | 1 | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% |
| compact | 7 | 25 | 25 | 2 | 23 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| composer | 2 | 22 | 22 | 0 | 19 | 0 | 0 | 0 | 3 | 0 | 0 | 86.4% |
| copy | 1 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| credits | 5 | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| fast | 6 | 22 | 22 | 0 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| filepicker | 3 | 20 | 20 | 0 | 8 | 0 | 0 | 0 | 12 | 0 | 0 | 40.0% |
| fulltranscript | 2 | 19 | 19 | 0 | 7 | 0 | 0 | 0 | 12 | 0 | 0 | 36.8% |
| h | 15 | 38 | 38 | 0 | 16 | 0 | 1 | 0 | 21 | 0 | 0 | 42.1% |
| help | 1 | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% |
| image | 9 | 52 | 52 | 3 | 47 | 0 | 0 | 0 | 2 | 0 | 0 | 96.2% |
| interrupt | 12 | 61 | 61 | 0 | 28 | 0 | 0 | 0 | 33 | 0 | 0 | 45.9% |
| links | 2 | 9 | 9 | 2 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 88.9% |
| model | 14 | 69 | 69 | 0 | 62 | 0 | 0 | 0 | 7 | 0 | 0 | 89.9% |
| notice | 3 | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| notices | 1 | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| p | 152 | 798 | 796 | 17 | 473 | 13 | 24 | 0 | 269 | 2 | 0 | 61.4% |
| paste | 3 | 17 | 17 | 1 | 14 | 0 | 0 | 0 | 2 | 0 | 0 | 88.2% |
| permissions | 3 | 20 | 20 | 0 | 19 | 0 | 0 | 0 | 1 | 0 | 0 | 95.0% |
| popup | 1 | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% |
| prose | 1 | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| provider | 6 | 41 | 41 | 0 | 9 | 0 | 0 | 0 | 32 | 0 | 0 | 22.0% |
| question | 3 | 26 | 26 | 0 | 22 | 0 | 0 | 0 | 4 | 0 | 0 | 84.6% |
| quit | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| read | 1 | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% |
| recall | 1 | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% |
| rename | 2 | 8 | 8 | 1 | 6 | 0 | 0 | 0 | 1 | 0 | 0 | 87.5% |
| reset | 2 | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| resume | 6 | 49 | 49 | 8 | 20 | 0 | 0 | 0 | 21 | 0 | 0 | 57.1% |
| rules | 4 | 19 | 19 | 1 | 7 | 0 | 0 | 0 | 11 | 0 | 0 | 42.1% |
| search | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| settings | 12 | 72 | 72 | 1 | 29 | 0 | 0 | 0 | 42 | 0 | 0 | 41.7% |
| skills | 4 | 44 | 44 | 0 | 23 | 0 | 0 | 0 | 21 | 0 | 0 | 52.3% |
| slash | 2 | 15 | 15 | 0 | 9 | 0 | 0 | 0 | 6 | 0 | 0 | 60.0% |
| sound | 2 | 13 | 13 | 0 | 4 | 0 | 0 | 0 | 9 | 0 | 0 | 30.8% |
| startup | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| status | 1 | 8 | 8 | 0 | 1 | 0 | 1 | 0 | 6 | 0 | 0 | 12.5% |
| statusline | 2 | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| stream | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| streaming | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| table | 1 | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| timeout | 1 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% |
| titles | 6 | 15 | 15 | 0 | 12 | 0 | 0 | 0 | 3 | 0 | 0 | 80.0% |
| toolgroup | 3 | 17 | 17 | 0 | 6 | 0 | 0 | 0 | 11 | 0 | 0 | 35.3% |
| torn | 1 | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| trace | 2 | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| undo | 3 | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| usage | 4 | 30 | 30 | 0 | 30 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| workspace | 2 | 13 | 13 | 0 | 11 | 0 | 0 | 0 | 2 | 0 | 0 | 84.6% |

## By surface

| surface | scenarios | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `/help` | 6 | 70 | 70 | 0 | 40 | 0 | 0 | 0 | 30 | 0 | 0 | 57.1% |
| `/clear` | 3 | 45 | 45 | 0 | 17 | 0 | 0 | 0 | 28 | 0 | 0 | 37.8% |
| `/status` | 13 | 103 | 101 | 3 | 57 | 3 | 26 | 0 | 12 | 2 | 0 | 58.3% |
| `/version` | 8 | 79 | 79 | 0 | 30 | 0 | 1 | 0 | 48 | 0 | 0 | 38.0% |
| `/quit` | 36 | 244 | 244 | 20 | 127 | 0 | 3 | 0 | 94 | 0 | 0 | 60.2% |
| `/new` | 14 | 146 | 146 | 8 | 79 | 0 | 2 | 0 | 57 | 0 | 0 | 59.6% |
| `/reset` | 2 | 43 | 43 | 0 | 15 | 0 | 0 | 0 | 28 | 0 | 0 | 34.9% |
| `/resume` | 19 | 162 | 162 | 8 | 79 | 0 | 3 | 0 | 72 | 0 | 0 | 53.7% |
| `/rename` | 7 | 76 | 76 | 5 | 34 | 0 | 1 | 0 | 36 | 0 | 0 | 51.3% |
| `/fork` | 5 | 42 | 42 | 1 | 38 | 0 | 3 | 0 | 0 | 0 | 0 | 92.9% |
| `/undo` | 5 | 51 | 51 | 1 | 15 | 0 | 0 | 0 | 35 | 0 | 0 | 31.4% |
| `/copy` | 4 | 52 | 52 | 0 | 14 | 0 | 0 | 0 | 38 | 0 | 0 | 26.9% |
| `/compact` | 8 | 69 | 69 | 1 | 33 | 0 | 0 | 0 | 35 | 0 | 0 | 49.3% |
| `/login` | 3 | 48 | 48 | 0 | 5 | 0 | 0 | 0 | 43 | 0 | 0 | 10.4% |
| `/logout` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/provider` | 7 | 74 | 74 | 0 | 14 | 0 | 0 | 0 | 60 | 0 | 0 | 18.9% |
| `/stats` | 5 | 75 | 75 | 0 | 28 | 0 | 1 | 0 | 46 | 0 | 0 | 37.3% |
| `/usage` | 15 | 112 | 112 | 2 | 98 | 4 | 0 | 0 | 8 | 0 | 0 | 89.3% |
| `/credits` | 7 | 63 | 63 | 0 | 33 | 0 | 0 | 0 | 30 | 0 | 0 | 52.4% |
| `/model` | 19 | 93 | 93 | 0 | 70 | 0 | 0 | 0 | 23 | 0 | 0 | 75.3% |
| `/effort` | 8 | 51 | 51 | 0 | 51 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/fast` | 6 | 64 | 64 | 0 | 36 | 0 | 0 | 0 | 28 | 0 | 0 | 56.2% |
| `/ultrafast` | 5 | 50 | 50 | 0 | 22 | 0 | 0 | 0 | 28 | 0 | 0 | 44.0% |
| `/settings` | 14 | 109 | 109 | 1 | 37 | 0 | 0 | 0 | 71 | 0 | 0 | 34.9% |
| `/statusline` | 4 | 64 | 64 | 0 | 34 | 0 | 0 | 0 | 30 | 0 | 0 | 53.1% |
| `/sound` | 2 | 13 | 13 | 0 | 4 | 0 | 0 | 0 | 9 | 0 | 0 | 30.8% |
| `/permissions` | 11 | 92 | 92 | 3 | 62 | 1 | 6 | 0 | 20 | 0 | 0 | 70.7% |
| `/allowlist` | 7 | 73 | 73 | 2 | 37 | 0 | 1 | 0 | 33 | 0 | 0 | 53.4% |
| `/workspace` | 3 | 46 | 46 | 0 | 16 | 0 | 0 | 0 | 30 | 0 | 0 | 34.8% |
| `/shell` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/image` | 7 | 63 | 63 | 4 | 29 | 0 | 0 | 0 | 30 | 0 | 0 | 52.4% |
| `/images` | 2 | 50 | 50 | 3 | 19 | 0 | 0 | 0 | 28 | 0 | 0 | 44.0% |
| `/paste` | 1 | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |
| `/mcp` | 32 | 162 | 162 | 0 | 114 | 1 | 0 | 0 | 47 | 0 | 0 | 70.4% |
| `/skills` | 1 | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% |
| `/alias` | 3 | 59 | 59 | 0 | 19 | 0 | 0 | 0 | 40 | 0 | 0 | 32.2% |
| `/feedback` | 1 | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% |
| `/trace` | 7 | 55 | 55 | 0 | 18 | 0 | 0 | 0 | 37 | 0 | 0 | 32.7% |
| `help: ↑↓ navigate` | 2 | 28 | 28 | 0 | 13 | 0 | 0 | 0 | 15 | 0 | 0 | 46.4% |
| `help: tab category` | 3 | 47 | 47 | 0 | 28 | 0 | 0 | 0 | 19 | 0 | 0 | 59.6% |
| `help: enter open` | 6 | 70 | 70 | 0 | 40 | 0 | 0 | 0 | 30 | 0 | 0 | 57.1% |
| `help: esc close` | 5 | 67 | 67 | 0 | 40 | 0 | 0 | 0 | 27 | 0 | 0 | 59.7% |
| `slash: ↑↓ navigate` | 53 | 559 | 559 | 8 | 396 | 1 | 14 | 0 | 140 | 0 | 0 | 72.3% |
| `slash: enter use` | 204 | 1201 | 1199 | 27 | 774 | 9 | 26 | 0 | 363 | 2 | 0 | 66.7% |
| `slash: esc close` | 97 | 820 | 818 | 11 | 510 | 8 | 24 | 0 | 265 | 2 | 0 | 63.5% |

## By scenario

| scenario | status | total | ran | identical | normalized-identical | approved-divergence | approved-divergence-cascade | replaced-in-next-pin | unresolved-defect | failed | timeout | pass | first unresolved row (fx / xfx) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alias | ran | 11 | 11 | 0 | 9 | 0 | 0 | 0 | 2 | 0 | 0 | 81.8% | help-query-command row 11: `Commands 4  [All]  General  Session  Account  Mod…` / `Commands 2  [All]  General  Session  Account  Mod…` (aligned: 3/3 rows unmatched) |
| allowlist | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist-remember | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| allowlist2 | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | after-reset row 1: (blank) / `<banner>` (aligned: 1/1 rows unmatched) |
| allowlist3 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| answered | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| approvals | ran | 10 | 10 | 0 | 0 | 0 | 0 | 0 | 10 | 0 | 0 | 0.0% | edit-ask row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-amend-limit | ran | 5 | 5 | 0 | 0 | 0 | 0 | 0 | 5 | 0 | 0 | 0.0% | asked row 19: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 3/3 rows unmatched) |
| approvals-big | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | write-big row 1: `     52 + generated line 52` / `     54 + generated line 54` (aligned: 4/4 rows unmatched) |
| card-wrap | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| cmd-wrap | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | cmd-a row 1: `<banner>` / (blank) (aligned: 12/12 rows unmatched) |
| compact | ran | 10 | 10 | 1 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-cancel | ran | 6 | 6 | 0 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-fail | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-inturn | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-overflow | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-text | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| compact-toolarge | ran | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| composer-keys | ran | 11 | 11 | 0 | 8 | 0 | 0 | 0 | 3 | 0 | 0 | 72.7% | help row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| composer-wrap | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| copy | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| credits-404 | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| credits-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| credits-empty | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| credits-ok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| credits-partial | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-astra | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-opus | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-override | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unavailable | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unconfirmed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| fast-unsup | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| filepicker | ran | 10 | 10 | 0 | 3 | 0 | 0 | 0 | 7 | 0 | 0 | 30.0% | at row 11: `  src/deep/sched.rs` / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| filepicker-keys | ran | 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 | 71.4% | at-pgdn row 11: `  src/deep/sched.rs` / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| filepicker-narrow | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | at-alpha row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| fulltranscript | ran | 15 | 15 | 0 | 6 | 0 | 0 | 0 | 9 | 0 | 0 | 40.0% | cmd-ask row 16: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| fulltranscript-notice | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full row 1: (blank) / `<banner>` (aligned: 4/4 rows unmatched) |
| h1-guards | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-mcphttp | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | hanging row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-mcphttp-fail | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | failed row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-mcphttp-methodnotfound | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | ready row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS` / `* mcp: NAME    SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| h1-models2 | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | after row 5: `Models 2  [All]` / `Models 1  [All]` (aligned: 2/2 rows unmatched) |
| h1-refuse | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | refused row 5: `⚠ Connection lost · waiting for connection · 2s ·…` / `✗ system: the connection to the Gateway failed: e…` (aligned: 2/2 rows unmatched) |
| h1-refuse-pause | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | refused row 5: `⚠ Connection lost · waiting for connection · 2s ·…` / `✗ system: the connection to the Gateway failed: e…` (aligned: 2/2 rows unmatched) |
| h1-rehomefile | ran | 2 | 2 | 0 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 50.0% |  |
| h1-reset | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | after row 7: `  [Response interrupted. Restarting.]` / `✗ system: the connection to the Gateway failed: e…` (aligned: 5/5 rows unmatched) |
| h1-resize | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | small row 1: `<banner>` / (blank) (aligned: 6/6 rows unmatched) |
| h1-signal | ran | 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 | 71.4% | ctrlz row 3: (blank) / `┃` (aligned: 4/4 rows unmatched) |
| h1-signal-hup | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | hup row 40: `Pane is dead (signal hup, Sun Nov  8 09:54:20 202…` / `Pane is dead (signal hup, Sun Nov  8 09:54:36 202…` (aligned: 1/1 rows unmatched) |
| h1-usage | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| h1-usage-codex | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | codex row 4: `usage: codex (ChatGPT), samdoe                   …` / `usage: API key (AI_GATEWAY_API_KEY)              …` (aligned: 26/26 rows unmatched) |
| h1-usage-grok | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | grok row 4: `usage: grok (xAI)                                …` / `usage: API key (AI_GATEWAY_API_KEY)              …` (aligned: 10/10 rows unmatched) |
| help | ran | 19 | 19 | 0 | 9 | 0 | 0 | 0 | 10 | 0 | 0 | 47.4% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| image | ran | 17 | 17 | 3 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-composer | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-inline | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-nonnative | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | sent row 7: `  It is a small red square.` / `✗ system: request failed: SubscriptionNativeImage…` (aligned: 3/3 rows unmatched) |
| image-paste | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-readfile | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-undo | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-unknown | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| image-withheld | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt | ran | 12 | 12 | 0 | 3 | 0 | 0 | 0 | 9 | 0 | 0 | 25.0% | cc-0.3 row 5: `▪ Cancelled · What can xfx do differently?` / `■ Cancelled · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-cmd | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | cmd-cc row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | help-open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| interrupt-next-cmd | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 8: `▪ Cancelled sleep 6 · What can xfx do differently?` / `■ Cancelled sleep 6 · What can xfx do differently?` (aligned: 1/1 rows unmatched) |
| interrupt-next-question | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | next row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| interrupt-question | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | q-cc row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| interrupt-recovered | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-recovery | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Notice` / `! recovery: ⚠ Provider unavailable · HTTP 500 · s…` (aligned: 10/10 rows unmatched) |
| interrupt-retry | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | r1.05 row 5: `⚠ Provider unavailable · HTTP 500 · server_error:…` / `  Recovered answer.` (aligned: 3/3 rows unmatched) |
| interrupt-steer | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt-steer-retract | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| interrupt2 | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | mid row 1: `  line 5 of the streamed answer` / `  line 4 of the streamed answer` (aligned: 1/1 rows unmatched) |
| links | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | full row 1: `  skills: loading` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 2/2 rows unmatched) |
| links-image | ran | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model | ran | 12 | 12 | 0 | 11 | 0 | 0 | 0 | 1 | 0 | 0 | 91.7% | typed row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| model-anchor | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | alt-enter row 6: `  xai/grok-4.7` / `  no matching models` (aligned: 1/1 rows unmatched) |
| model-busy | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-busy-known | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-column-idle | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | col-down row 5: `         openai/gpt-6-astra` / `         unable to load models` (aligned: 4/4 rows unmatched) |
| model-column-reask | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-effort | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-floor | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | inside-floor row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Could not reach AI Gateway; retry /model.` (aligned: 1/1 rows unmatched) |
| model-parked | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | menu-loading row 5: `~ Thinking (<s>)` / `~ Generating (<s>) (↑4 ↓3)` (aligned: 1/1 rows unmatched) |
| model-reask | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-router | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| model-speed | ran | 12 | 12 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notice-midstream | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notice-wire | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notice-wrap | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| notices | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-call-row-long | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | end row 7: `● 1 tool call · 1 command` / `● 1 tool call · 1 command · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-call-row-safe | ran | 2 | 2 | 0 | 0 | 1 | 0 | 0 | 1 | 0 | 0 | 0.0% | full row 1: `  tokens: 1200 in · 40 out` / `  finish: tool_calls` (aligned: 8/8 rows unmatched) |
| p5-cli-ledger-update-help | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | h00-update-help.full row 2: `xfx upgrade` / `error: unrecognized subcommand 'update'` (aligned: 9/9 rows unmatched) |
| p5-effort-fork-effort-backtab | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-column | ran | 17 | 17 | 0 | 17 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-ctrlc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-down | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-loading | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-menu | ran | 18 | 18 | 0 | 18 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-nolevels | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-effort-short | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork | ran | 16 | 16 | 1 | 14 | 0 | 1 | 0 | 0 | 0 | 0 | 93.8% |  |
| p5-effort-fork-fork-continue | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-fork-edge | ran | 11 | 11 | 0 | 10 | 0 | 1 | 0 | 0 | 0 | 0 | 90.9% |  |
| p5-effort-fork-fork-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-effort-fork-oldread-src | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | picker-enter-fork row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 4/4 rows unmatched) |
| p5-effort-fork-oldread-v1 | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-oldread-v2 | ran | 7 | 7 | 0 | 2 | 0 | 0 | 0 | 5 | 0 | 0 | 28.6% | picker-all row 5: `Sessions 2  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 3/3 rows unmatched) |
| p5-effort-fork-slash-rank | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | e row 5: `Commands 23                                      …` / `Commands 21                                      …` (aligned: 1/1 rows unmatched) |
| p5-inline-code-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-inline-code-table-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-approval | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | cmd-ask row 5: `● 1 tool call · 1 command` / `✗ system: the model asked to run the 'shell' tool…` (aligned: 15/15 rows unmatched) |
| p5-input-at | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | at-st row 5: `  start.sh` / `  spikes/s-01-trace.txt` (aligned: 10/10 rows unmatched) |
| p5-input-at-dir | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | dir-query row 11: (blank) / `─────────────────────────────────────────────────…` (aligned: 4/4 rows unmatched) |
| p5-input-ctrlbs | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-mcp | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-input-model | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | q-cs45 row 5: `Models 1  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` / `Models 0  [All]  Anthropic  OpenAI  xAI  Z.AI  Ot…` (aligned: 2/2 rows unmatched) |
| p5-input-model-fail | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | fail-open row 7: `Could not reach AI Gateway; xfx retries automatic…` / `Could not reach AI Gateway; retry /model.` (aligned: 1/1 rows unmatched) |
| p5-input-paste | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | paste-enter row 3: `! input: Input that arrived right after the paste…` / `✗ input: Paste was not applied because extra inpu…` (aligned: 2/2 rows unmatched) |
| p5-input-question | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | q2-cancelled row 5: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| p5-input-smoke | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-compactfail | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-compactok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-login | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | l00-picker row 3: `┃ /login` / `✗ command: Unknown command. Try /help.` (aligned: 7/7 rows unmatched) |
| p5-lifecycle-login2 | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | m00-vercel-methods row 3: `┃ /login vercel` / `✗ command: Unknown command. Try /help.` (aligned: 7/7 rows unmatched) |
| p5-lifecycle-osc | ran | 9 | 9 | 0 | 0 | 0 | 0 | 0 | 9 | 0 | 0 | 0.0% | s00-start row 5: `ask · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-relhome | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-image | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-made | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-relhome-resume | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | r00-resume.full row 2: (blank) / `xfx: HomeNotSet` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-relhome-unsaved | ran | 9 | 9 | 0 | 8 | 0 | 1 | 0 | 0 | 0 | 0 | 88.9% |  |
| p5-lifecycle-retry | ran | 7 | 7 | 0 | 1 | 0 | 0 | 0 | 6 | 0 | 0 | 14.3% | r01-2.1s row 5: `⚠ Provider unavailable · HTTP 502 · Synthetic bad…` / `✗ system: the Gateway returned HTTP 502: {"error"…` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-retryok | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-suspend | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-lifecycle-update | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | u00-update row 2: (blank) / `Usage: xfx [OPTIONS] [COMMAND]` (aligned: 2/2 rows unmatched) |
| p5-lifecycle-v1bad | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | b02-launch-refused.full row 1: (blank) / `xfx: there is no session 'cb1PK0RGxCmY'` (aligned: 1/1 rows unmatched) |
| p5-lifecycle-v1resume | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | v00b-picker-all row 5: `Sessions 1  Current workspace  [All workspaces]` / `Sessions 0  Current workspace  [All workspaces]` (aligned: 2/2 rows unmatched) |
| p5-mcp-config | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | list row 11: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 3/3 rows unmatched) |
| p5-mcp-config-ignored | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | startup row 5: `auto · model-a                                   …` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-mcp-config-vars | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME      SOURCE          TRANSPORT  STATUS` / `* mcp: NAME      SOURCE           TRANSPORT  STAT…` (aligned: 1/1 rows unmatched) |
| p5-mcp-direct | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-hostile | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-malformed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-modern | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-notimpl | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-old2024 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssefail | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssehold | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-sseidless | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-http-ssestrid | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-asks | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-banner | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-depth | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | list row 3: `* mcp: NAME   SOURCE          TRANSPORT  STATUS` / `* mcp: NAME   SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-discover-stdio-escaped | ran | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 0.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-discover-stdio-hostile | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-malformed | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-modern | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-modern-tc | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-old2024 | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-discover-stdio-silent | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-dismiss | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-empty | ran | 24 | 24 | 0 | 24 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-errdata | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-fail | ran | 6 | 6 | 0 | 4 | 0 | 0 | 0 | 2 | 0 | 0 | 66.7% | show-bad row 3: `┃` / `* mcp: bad` (aligned: 7/7 rows unmatched) |
| p5-mcp-failed | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-known | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-live | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | cancel row 19: `▪ Cancelled mcp_web_echo · What can xfx do differ…` / `■ Cancelled mcp_web_echo · What can xfx do differ…` (aligned: 1/1 rows unmatched) |
| p5-mcp-menu-new | ran | 34 | 34 | 0 | 33 | 1 | 0 | 0 | 0 | 0 | 0 | 97.1% |  |
| p5-mcp-menu-old | ran | 16 | 16 | 0 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-notes | ran | 4 | 4 | 0 | 2 | 0 | 0 | 0 | 2 | 0 | 0 | 50.0% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 1/1 rows unmatched) |
| p5-mcp-rpcerror | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-unavailable | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-verbs | ran | 28 | 28 | 0 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 3.6% | list row 3: `* mcp: NAME     SOURCE          TRANSPORT  STATUS` / `* mcp: NAME     SOURCE           TRANSPORT  STATUS` (aligned: 2/2 rows unmatched) |
| p5-mcp-wire-new | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-mcp-wire-old | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-panels-badsettings | failed | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0.0% |  |
| p5-panels-help | ran | 17 | 17 | 0 | 10 | 0 | 0 | 0 | 7 | 0 | 0 | 58.8% | open row 5: `Commands 38  [All]  General  Session  Account  Mo…` / `Commands 34  [All]  General  Session  Account  Mo…` (aligned: 4/4 rows unmatched) |
| p5-panels-keys | ran | 10 | 10 | 0 | 2 | 0 | 5 | 0 | 3 | 0 | 0 | 20.0% | approval-arrives row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-noauth | ran | 2 | 2 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-panels-permissions | ran | 20 | 20 | 2 | 14 | 0 | 1 | 0 | 3 | 0 | 0 | 80.0% | approval row 29: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-permissions-saved | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | approval row 15: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| p5-panels-slash | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| p5-panels-status | ran | 11 | 11 | 0 | 5 | 0 | 6 | 0 | 0 | 0 | 0 | 45.5% |  |
| p5-panels-status-badsettings | ran | 3 | 3 | 0 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-panels-status-first | ran | 3 | 3 | 0 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 33.3% |  |
| p5-panels-status-short | ran | 6 | 6 | 0 | 1 | 0 | 5 | 0 | 0 | 0 | 0 | 16.7% |  |
| p5-panels-sweep-a | ran | 33 | 33 | 0 | 5 | 0 | 0 | 0 | 28 | 0 | 0 | 15.2% | stats row 5: `* stats: ansi_bytes=3506, redraws=0, debounced_re…` / `* stats: ansi_bytes=2655, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-panels-sweep-b | ran | 11 | 11 | 0 | 4 | 0 | 0 | 0 | 7 | 0 | 0 | 36.4% | stats row 17: `* stats: ansi_bytes=5100, redraws=0, debounced_re…` / `* stats: ansi_bytes=4127, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| p5-permissions-early | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-permissions-early-probe-ask-after | ran | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-session-early-prefs | ran | 5 | 5 | 2 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-ctrlo | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-ctrlo-tc | ran | 6 | 6 | 0 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 50.0% | full-dark row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-toolgroup-live | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-live-tc | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-light-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolgroup-retint-tc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-toolstop-retint | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-light-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | light row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-toolstop-retint-tc | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | dark row 8: `▪ Cancelled sleep 6 && echo "x" # note · What can…` / `■ Cancelled sleep 6 && echo "x" # note · What can…` (aligned: 1/1 rows unmatched) |
| p5-transcript-anchor | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-anchormid | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-cancel | ran | 7 | 7 | 0 | 0 | 0 | 0 | 0 | 7 | 0 | 0 | 0.0% | mid row 9: `full access · model-a` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| p5-transcript-diffask | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | ask row 21: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future edits to this file fo…` (aligned: 5/5 rows unmatched; under approved row 12, review-diff-spelling) |
| p5-transcript-diffbad | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | edited row 7: `● 2 tool calls · 1 read · 1 edit` / `● 2 tool calls · 1 read · 1 edit · 1 failed` (aligned: 2/2 rows unmatched) |
| p5-transcript-diffctl | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | edited row 7: `● 1 tool call · 1 edit` / `● 2 tool calls · 1 read · 1 edit` (aligned: 3/3 rows unmatched) |
| p5-transcript-diffodd | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwide | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwide2 | ran | 3 | 3 | 0 | 2 | 1 | 0 | 0 | 0 | 0 | 0 | 66.7% |  |
| p5-transcript-diffwrap | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-diffwrap2 | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-md | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-md-light | ran | 5 | 5 | 2 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 60.0% | full row 1: `  XXXXXXXXXXXXXXXXXXXXXXX KST · Response` / (blank) (aligned: 1/1 rows unmatched) |
| p5-transcript-question | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | asked row 9: `    1) Postgres with the             Relational, …` / `    1) Postgres with the write-ahead Relational, …` (aligned: 3/3 rows unmatched) |
| p5-transcript-records | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | full-records row 2: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| p5-transcript-resize | ran | 14 | 14 | 0 | 1 | 0 | 0 | 0 | 13 | 0 | 0 | 7.1% | full-running row 1: `├ Read d.txt` / (blank) (aligned: 2/2 rows unmatched) |
| p5-transcript-safe | ran | 4 | 4 | 0 | 0 | 2 | 0 | 0 | 2 | 0 | 0 | 0.0% | full row 1: (blank) / `  +4 more)` (aligned: 6/6 rows unmatched) |
| p5-transcript-safetab | ran | 2 | 2 | 0 | 1 | 0 | 0 | 0 | 1 | 0 | 0 | 50.0% | full row 1: `  skills: loading` / `* session: provider: gateway · model: fake/model-…` (aligned: 3/3 rows unmatched) |
| p5-transcript-submit | ran | 5 | 5 | 3 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-submit-multi | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tallfirst | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tools | ran | 8 | 8 | 0 | 1 | 0 | 0 | 0 | 7 | 0 | 0 | 12.5% | full-running row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 6/6 rows unmatched) |
| p5-transcript-tz | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-transcript-tzsys | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-budget | ran | 14 | 14 | 0 | 14 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-cached | ran | 21 | 21 | 0 | 21 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-codex | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | start row 5: `auto · gpt-5.6-sol` / `auto · glm-5.2` (aligned: 1/1 rows unmatched) |
| p5-usage-many | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-nokey | ran | 4 | 4 | 0 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0.0% |  |
| p5-usage-noledger | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-oidc | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| p5-usage-turns | ran | 20 | 20 | 0 | 18 | 0 | 0 | 0 | 2 | 0 | 0 | 90.0% | stats row 27: `* stats: ansi_bytes=15631, redraws=0, debounced_r…` / `* stats: ansi_bytes=9034, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| paste-block-undo | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-card | ran | 7 | 7 | 1 | 6 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| paste-suffix | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | paste-enter row 3: `! input: Input that arrived right after the paste…` / `✗ input: Paste was not applied because extra inpu…` (aligned: 2/2 rows unmatched) |
| permissions | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-btab | ran | 7 | 7 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| permissions-midturn | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | second-asks row 16: `    2  Apply + allow workspace file access for th…` / `    2  Apply + allow future writes to this file f…` (aligned: 3/3 rows unmatched) |
| popup-full | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | popup row 31: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| prose-wrap | ran | 2 | 2 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| provider | ran | 15 | 15 | 0 | 5 | 0 | 0 | 0 | 10 | 0 | 0 | 33.3% | open row 5: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | open row 14: `            vercel · current` / `            gateway · current` (aligned: 3/3 rows unmatched) |
| provider-carry-back | ran | 6 | 6 | 0 | 0 | 0 | 0 | 0 | 6 | 0 | 0 | 0.0% | start row 5: `auto · gpt-5.6-sol` / `auto · model-a` (aligned: 1/1 rows unmatched) |
| provider-resume | ran | 6 | 6 | 0 | 2 | 0 | 0 | 0 | 4 | 0 | 0 | 33.3% | switched row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-resume-missing | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | turn2 row 12: `* provider: Preparing Codex subscription.` / `* provider: Preparing llmux.` (aligned: 3/3 rows unmatched) |
| provider-switch | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | open row 5: `            vercel · current` / `            gateway` (aligned: 4/4 rows unmatched) |
| question | ran | 9 | 9 | 0 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| question-field | ran | 10 | 10 | 0 | 8 | 0 | 0 | 0 | 2 | 0 | 0 | 80.0% | q2-cc row 16: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| question-other | ran | 7 | 7 | 0 | 5 | 0 | 0 | 0 | 2 | 0 | 0 | 71.4% | q2-esc row 14: `▪ Cancelled` / `■ Cancelled` (aligned: 1/1 rows unmatched) |
| quit | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| quit-pristine | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| read-failures | ran | 6 | 6 | 1 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | live-ctrl-o-up1 row 5: `  system prompt: ready · 6249 bytes` / `  tools: 12 advertised (list_files, glob_files, g…` (aligned: 4/4 rows unmatched) |
| recall-edge | ran | 3 | 3 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 0 | 0.0% | start row 5: `* stats: ansi_bytes=3525, redraws=0, debounced_re…` / `* stats: ansi_bytes=2675, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| rename | ran | 5 | 5 | 0 | 4 | 0 | 0 | 0 | 1 | 0 | 0 | 80.0% | relaunched row 1: `xfx: no saved sessions for this workspace.` / `* session resumed: My Title` (aligned: 5/5 rows unmatched) |
| rename-midturn | ran | 3 | 3 | 1 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| reset | ran | 10 | 10 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| reset-wire | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume | ran | 13 | 13 | 4 | 9 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-empty | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-launch | ran | 4 | 4 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| resume-paging | ran | 8 | 8 | 0 | 2 | 0 | 0 | 0 | 6 | 0 | 0 | 25.0% | page1 row 24: `  question one          work · now · 1 turn` / `  question one          work ·  1m · 1 turn` (aligned: 1/1 rows unmatched) |
| resume-tools | ran | 18 | 18 | 0 | 3 | 0 | 0 | 0 | 15 | 0 | 0 | 16.7% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| resume-wire | ran | 2 | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-hidden | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-hidden-all | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| rules-read | ran | 7 | 7 | 1 | 3 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | ask row 18: (blank) / `  2 = allow every future read_file of 'secret.txt…` (aligned: 2/2 rows unmatched) |
| rules-read-auto | ran | 9 | 9 | 0 | 1 | 0 | 0 | 0 | 8 | 0 | 0 | 11.1% | grep-ask row 18: (blank) / `  2 = allow every future grep_files below '.' for…` (aligned: 2/2 rows unmatched) |
| search-roots | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings | ran | 7 | 7 | 0 | 4 | 0 | 0 | 0 | 3 | 0 | 0 | 57.1% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-cats | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | off row 20: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse | ran | 8 | 8 | 0 | 6 | 0 | 0 | 0 | 2 | 0 | 0 | 75.0% | collapse-on row 28: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-collapse-open | ran | 3 | 3 | 1 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | running row 1: `<banner>` / `┃ look` (aligned: 2/2 rows unmatched) |
| settings-effort | ran | 8 | 8 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 0 | 0.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-history | ran | 9 | 9 | 0 | 6 | 0 | 0 | 0 | 3 | 0 | 0 | 66.7% | marked row 24: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model | ran | 3 | 3 | 0 | 1 | 0 | 0 | 0 | 2 | 0 | 0 | 33.3% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-model-list | ran | 5 | 5 | 0 | 1 | 0 | 0 | 0 | 4 | 0 | 0 | 20.0% | list row 17: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-perm | ran | 4 | 4 | 0 | 1 | 0 | 0 | 0 | 3 | 0 | 0 | 25.0% | marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings-scrollback | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| settings-speed | ran | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 4 | 0 | 0 | 0.0% | fast-marked row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| settings2 | ran | 12 | 12 | 0 | 5 | 0 | 0 | 0 | 7 | 0 | 0 | 41.7% | tab3 row 7: `  Sound level    off  on  max` / `  Sound level    off  on` (aligned: 1/1 rows unmatched) |
| skills | ran | 18 | 18 | 0 | 2 | 0 | 0 | 0 | 16 | 0 | 0 | 11.1% | menu row 5: `Skills 2  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 2  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-ghost | ran | 15 | 15 | 0 | 14 | 0 | 0 | 0 | 1 | 0 | 0 | 93.3% | dollar-menu row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-paste | ran | 3 | 3 | 0 | 2 | 0 | 0 | 0 | 1 | 0 | 0 | 66.7% | paste row 5: `Skills 1  [All]  xfx  Workspace  Claude  Codex  A…` / `Skills 1  [All]  xfx  Workspace` (aligned: 1/1 rows unmatched) |
| skills-slash | ran | 8 | 8 | 0 | 5 | 0 | 0 | 0 | 3 | 0 | 0 | 62.5% | slash-all row 5: `Results 40 · type to filter                      …` / `Results 36 · type to filter                      …` (aligned: 1/1 rows unmatched) |
| slash | ran | 9 | 9 | 0 | 4 | 0 | 0 | 0 | 5 | 0 | 0 | 44.4% | slash row 5: `Commands 38 · type to filter                     …` / `Commands 34 · type to filter                     …` (aligned: 1/1 rows unmatched) |
| slash-filter | ran | 6 | 6 | 0 | 5 | 0 | 0 | 0 | 1 | 0 | 0 | 83.3% | filter-mo row 7: `  /model   choose what model and reasoning effort…` / `  /model   show the model, or '/model <id>' to us…` (aligned: 1/1 rows unmatched) |
| sound | ran | 12 | 12 | 0 | 3 | 0 | 0 | 0 | 9 | 0 | 0 | 25.0% | max row 13: `* sound: max` / `✗ usage: /sound [on\|off]` (aligned: 1/1 rows unmatched) |
| sound-default | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| startup | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| status-version-stats | ran | 8 | 8 | 0 | 1 | 0 | 1 | 0 | 6 | 0 | 0 | 12.5% | stats row 3: `* stats: ansi_bytes=5769, redraws=0, debounced_re…` / `* stats: ansi_bytes=4549, redraws=0, debounced_re…` (aligned: 1/1 rows unmatched) |
| statusline | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| statusline-turn | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| stream-wire | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| streaming | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| table-wrap | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| timeout | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | live row 6: `└ Timed out echo started; sleep 5` / `└ Timed out echo started; sleep 130` (aligned: 2/2 rows unmatched) |
| titles | ran | 3 | 3 | 0 | 3 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-idle | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | answered row 11: `auto · model-a · fix the renderer loop, it redraw` / `auto · model-a · Fix the renderer loop` (aligned: 1/1 rows unmatched) |
| titles-off | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-rename | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| titles-settings | ran | 4 | 4 | 0 | 3 | 0 | 0 | 0 | 1 | 0 | 0 | 75.0% | bare row 18: `  Sound level              off  on  max` / `  Sound level              off  on` (aligned: 1/1 rows unmatched) |
| titles-steer | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| toolgroup | ran | 13 | 13 | 0 | 2 | 0 | 0 | 0 | 11 | 0 | 0 | 15.4% | cmd-ask row 19: `  $ # shell.run profile=user shell=/bin/zsh` / `  $ # terminal.exec cwd=.` (aligned: 3/3 rows unmatched) |
| toolgroup-noop | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| toolgroup-wire | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| torn-exit | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| trace | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| trace-nowhere | ran | 1 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| undo | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| undo-empty | ran | 2 | 2 | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| undo-resume | ran | 1 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage | ran | 13 | 13 | 0 | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage-billed | ran | 8 | 8 | 0 | 8 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage-deferred | ran | 4 | 4 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| usage-session | ran | 5 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| workspace | ran | 11 | 11 | 0 | 11 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% |  |
| workspace-index | ran | 2 | 2 | 0 | 0 | 0 | 0 | 0 | 2 | 0 | 0 | 0.0% | at-plan row 13: `  <tmp>/fxsw.zsa1hls…` / `  <tmp>/fxsw.zsa1hls…` (aligned: 5/5 rows unmatched) |

## Causes of the unresolved snaps

573 unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone).
Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.

| cause | kind | snaps | only | what | owner |
|---|---|---|---|---|---|
| cancelled-marker | b | 66 | 50 | `v0.0.16 marks a cancelled tool call '▪' where c1db919 and xfx mark it '■'; T-U4 owns the …` | `T-U4 cancelled marker` |
| tool-schema | d | 48 | 4 | `fx's command tool is 'shell' ('action: run', per-call 'timeout_ms', run in the user's she…` | `unowned (decision needed? C8 runs commands in the login shell but keeps the header) -- xfx src/tui/approval_view.rs:685; src/permission/sla…` |
| sound-max | d | 46 | 45 | `fx's sound has a third level, 'max'; xfx's (the terminal bell) has none: Sound level is '…` | `unowned -- xfx src/sound.rs:37; src/tui/settings.rs:361 / fx src/core/config/settings_catalog.zig:280@c1db9191; src/core/app/app_commands.z…` |
| approval-grant-scope | a | 42 | 20 | `What an 'always' answer grants is a security choice: fx grants workspace file access for …` | `user decision approval grant scope (not yet on the program's decision list)` |
| menu-count | a+b | 40 | 10 | `The menus' count: fx 36, xfx 32 -- /login, /logout, /feedback awaiting decisions, /shell …` | `user decision Codex/Grok sign-in (#88) for /login and /logout; user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede61…` |
| stats-bytes | d | 34 | 27 | `'/stats' counts each product's own bytes to the terminal (fx 3499, xfx 2675 for the same …` | `unowned -- xfx src/tui/shell.rs:1583 / fx src/core/app/app_commands.zig:1014@c1db9191` |
| session-record | d | 31 | 0 | `Ctrl-O's '* session:' record: fx's has 'system prompt: ready · N bytes', 'skills: loading…` | `unowned (S1 item 2 owns the skill notices, not this record) -- xfx src/tui/shell.rs:37424 / fx src/core/app/app_bootstrap_runtime.zig:71@c1…` |
| menu-shift | a+b | 29 | 0 | `Command rows that only moved: a window without the four absent rows shows two others, and…` | `user decision Codex/Grok sign-in (#88); user decision /feedback destination; C8 feat/tui-phase4-shell-reload 76ede613` |
| transcript-page-offset | d | 28 | 15 | `Rows only one side has push the other side's rows off a Ctrl-O page by two: fx's command …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| C7-help-entries | b | 20 | 4 | `xfx's /model summary is the line shell's; C7 r2 gives every registry row fx's help entry.` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| full-transcript-shift | d | 18 | 1 | `Every Ctrl-O row under fx's taller session record and its shell calls' argument rows stan…` | `unowned (follows session-record and tool-schema) -- xfx src/tui/shell.rs:37424; src/tui/approval_view.rs:685 / fx src/core/app/app_bootstra…` |
| provider-label | d | 15 | 0 | `The picker names the Gateway 'gateway'; fx's row is 'vercel'.` | `unowned (P1's picker) -- xfx src/provider/mod.rs:116 / fx src/core/auth/provider_catalog.zig:19@c1db9191` |
| grant-scope-push | a | 14 | 4 | `The scope rows make xfx's approval two or more rows taller (a long command is repeated in…` | `user decision approval grant scope (not yet on the program's decision list)` |
| toolgroup-ctrlo-record-short | d | 6 | 0 | `TG-REDRAW's Ctrl-O pages hold the whole record, read from the top: fx's '* session:' reco…` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| session-age | c | 6 | 6 | `The picker's age column reads the run's clock; the snaps fall about a minute after the se…` | `scenario -- fix: seed the saved sessions with fixed timestamps (homefile), or take the picker snaps well clear of a minute after the first …` |
| C8-shell | b | 5 | 0 | `fx's /shell row in /help and the popup; xfx has no /shell until C8 lands.` | `C8 /shell reload feat/tui-phase4-shell-reload 76ede613` |
| subagent-tool | d | 5 | 2 | `The scenario's second step calls fx's 'subagent' tool twice; xfx has none, so its turns l…` | `unowned (the subagent tool, deferred: docs/parity.md 'subagent') -- xfx docs/parity.md:172 / fx src/core/tooling/tools.zig:1370@c1db9191` |
| diffctl-xfx-reads-first | c | 4 | 3 | `xfx's 'edit_file' refuses a file it has not read in full (docs/parity.md:158), so T-U6 ga…` | `T-U6 -- fix: fx's turns read 'ctl.txt' first as xfx's do ('p5-transcript-diffctl-xfx.json''s first step), so the two binaries are given the…` |
| C7-help-search | b | 4 | 3 | `/help's search: fx matches a query against each command's help entry (helpCatalogSpecMatc…` | `C7 r2 feat/tui-phase4-alias 8d83f961` |
| K-recover | b | 4 | 4 | `Stream-failure recovery: fx's '⚠ Provider unavailable · … retrying request in 1s' row, it…` | `K-recover fix/tui-stream-recovery f11a23d2` |
| edit-not-utf8 | d | 3 | 3 | `fx v0.0.16 edits a file that is not UTF-8 and shows its stray bytes as '\xNN' in the diff…` | `unowned (edit_file on a file that is not UTF-8, which xfx refuses: docs/parity.md 'edit_file') -- xfx src/tools/mutate.rs:437-442; src/tool…` |
| safe-page-edge | d | 3 | 0 | `'transcript-page-offset' for 'p5-transcript-safe' and 'p5-transcript-safetab', four rows …` | `unowned (follows tool-schema and session-record) -- xfx src/tui/approval_view.rs:685; src/tui/shell.rs:37424 / fx src/core/execution/comman…` |
| upgrade-update-alias | d | 3 | 3 | `The scenario records what fx v0.0.16 prints for 'fx update --help', 'fx upgrade --help' a…` | `unowned (the upgrade command, deferred: docs/parity.md 'upgrade') -- xfx docs/parity.md:63 / fx src/builtins/commands.zig:257-260@c1db9191` |
| P1-carry-notice | b | 2 | 0 | `After a switch back to the Gateway xfx says once that the llmux turn's reasoning was not …` | `P1 r3/r4 feat/tui-phase4-provider-session 8facfb6d` |
| provider-credential-column | a | 2 | 2 | `'/provider vercel' opens fx's credential column (OAuth or API key, the gated sign-in) and…` | `user decision Codex/Grok sign-in (#88)` |
| vision-route | a | 2 | 2 | `For a model without native image input fx forces a 'vision' call answered by a second mod…` | `user decision second-model image route (not yet on the program's decision list)` |
| approval-scroll | d | 2 | 0 | `A long review scrolls with the mouse wheel in fx and Ctrl-P/Ctrl-N in xfx, which asks for…` | `unowned -- xfx src/tui/approval_view.rs:106 / fx src/ui/approval_screen.zig:564@c1db9191` |
| allowlist-band | d | 2 | 2 | `Once /allowlist's notices fill the screen fx's band stands three rows above the bottom an…` | `unowned (not traced to the line) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:39-60@c1db9191` |
| activity-row-gap | d | 2 | 2 | `With the whole answer delivered and the turn held open, fx leaves a blank row between the…` | `unowned (not traced past the band's top) -- xfx src/tui/layout.rs:91 / fx src/ui/render_engine/footer_layout.zig:50@c1db9191` |
| idle-connection | c | 2 | 2 | `fx's title call goes out on the connection it warmed at launch, which the fake reset afte…` | `fakegw -- fix: the fake gateway keeps an unused connection open for fx's 30 s pool TTL (fakegw.ts idleTimeout), not a scenario file` |
| help-login-args | a | 1 | 0 | `Enter on the Account tab's first row opens fx's /login picker; xfx's first row there is /…` | `user decision Codex/Grok sign-in (#88)` |
| call-row-page-edge | d | 1 | 0 | `'transcript-page-offset' for 'p5-call-row-safe''s tail page: fx's command entry has two a…` | `unowned (follows tool-schema) -- xfx src/tui/approval_view.rs:685; src/tui/toolgroup.rs:749 / fx src/core/execution/command_environment.zig…` |
| command-size-bound | b | 1 | 1 | `xfx refuses a command whose text, once trimmed, passes 8 KiB ('terminal exec refuses a co…` | `CMD-SIZE (after train 2; raising the bound is an S change)` |
| launch-refusal-row | a | 1 | 1 | `fx takes the terminal before it opens the session '--resume' names and refuses after its …` | `user decision launch refusals before or after the terminal` |

Rows no cause covers (258 snaps):
- `approvals/after-esc` xfx row 15: `(blank)`
- `approvals/after-esc.full` xfx row 15: `(blank)`
- `filepicker/at` fx row 11: `  src/deep/sched.rs`
- `filepicker/at-sch` fx row 11: `(blank)`
- `filepicker/at-dir` fx row 5: `      src/`
- `filepicker/at-dir-down` fx row 5: `      src/`
- `filepicker/at-dir-cj` fx row 5: `      src/`
- `filepicker/at-tab` fx row 3: `┃ see @src/lib.rs`
- `filepicker/at-claim` fx row 17: `(blank)`
- `filepicker-keys/at-pgdn` fx row 11: `  src/deep/sched.rs`
- `filepicker-keys/at-ck` fx row 5: `  Cargo.toml`
- `filepicker-narrow/at-alpha` fx row 11: `(blank)`
- `filepicker-narrow/at-alpha-down` fx row 11: `(blank)`
- `filepicker-narrow/at-deep` fx row 11: `(blank)`
- `h1-mcphttp/hanging` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/failed` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/rpcerror` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-fail/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`
- `h1-mcphttp-methodnotfound/ready` fx row 3: `* mcp: NAME    SOURCE          TRANSPORT  STATUS`

## Ledger

| divergence | class | owner | expires | rows it covers | snaps |
|---|---|---|---|---|---|
| status-panel-sandbox | user-decision | `user decision /status Sandbox row` | `date:2026-12-04` | 55 | 29 |
| status-panel-updates | user-decision | `user decision updater (sign-in and updater epic)` | `date:2026-12-04` | 55 | 29 |
| status-panel-step-limit | unsafe-to-copy | `user decision default step limit` | `date:2026-12-04` | 109 | 29 |
| saved-write-rule-applied | unsafe-to-copy | `PERM-EARLY` | `date:2026-12-04` | 10 | 1 |
| status-panel-session-id-wrap | user-decision | `user decision session id shape` | `date:2026-12-04` | 0 | 0 |
| status-panel-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 8 | 4 |
| usage-no-sign-in | user-decision | `user decision Codex/Grok sign-in (#88)` | `date:2026-12-04` | 4 | 4 |
| ctrl-o-command-marks | unsafe-to-copy | `T-U1-SAFE` | `date:2026-12-04` | 9 | 2 |
| call-row-raw-argument | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 10 | 2 |
| call-row-source-cut | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 1 | 1 |
| call-row-arabic-letter-mark | unsafe-to-copy | `CALLROW-SAFE` | `date:2026-12-04` | 6 | 4 |
| mcp-approval-wrap-led | unsafe-to-copy | `MCP-M` | `date:2027-05-06` | 2 | 1 |
| review-diff-spelling | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 2 | 1 |
| ctrl-o-arabic-letter-mark | unsafe-to-copy | `T-U6b` | `date:2026-12-04` | 0 | 0 |
| filler-modifier-count | unsafe-to-copy | `WIDTH-DISPUTED` | `date:2026-12-04` | 4 | 1 |

Approved for this pin but covering no row here: `ctrl-o-arabic-letter-mark`, `status-panel-session-id-wrap`.
