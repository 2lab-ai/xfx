## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `aa49111f7158`, base `510d4ee1f007` (both xfx builds against the same fx)
- 15 scenarios, 117 snap pairs: 109 pass at the head, 0 new, **0 regression(s)**
- wall: head 264 s, retries 0 s, base 358 s, total 623 s

### Scenarios

| scenario | why | pairs | pass at head | new | not passing at head |
|---|---|---|---|---|---|
| startup | core | 1 | 1 | 0 |  |
| composer-wrap | core | 11 | 11 | 0 |  |
| streaming | core | 2 | 2 | 0 |  |
| statusline-turn | core | 3 | 3 | 0 |  |
| question | core | 9 | 9 | 0 |  |
| allowlist-remember | core | 7 | 7 | 0 |  |
| model-column | core | 8 | 8 | 0 |  |
| settings-scrollback | core | 5 | 5 | 0 |  |
| h1-resize | core | 3 | 1 | 0 | large (unresolved-defect, base unresolved-defect), small (unresolved-defect, base unresolved-defect) |
| quit | core | 4 | 4 | 0 |  |
| p5-usage-budget | receipt | 14 | 14 | 0 |  |
| p5-usage-cached | receipt | 21 | 21 | 0 |  |
| p5-usage-many | receipt | 5 | 5 | 0 |  |
| p5-usage-nokey | receipt | 4 | 0 | 0 | closed (approved-divergence, base approved-divergence), open (approved-divergence, base unresolved-defect), r (approved-divergence, base unresolved-defect), start (approved-divergence, base approved-divergence) |
| p5-usage-turns | receipt | 20 | 18 | 0 | credits (unresolved-defect, base unresolved-defect, 4 base runs), stats (unresolved-defect, base unresolved-defect, 4 base runs) |

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
