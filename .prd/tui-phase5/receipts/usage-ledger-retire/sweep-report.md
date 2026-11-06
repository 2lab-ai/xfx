## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `f36ad428be93`, base `120f7b2003e7` (both xfx builds against the same fx)
- 11 scenarios, 57 snap pairs: 55 pass at the head, 4 new, **0 regression(s)**
- wall: head 171 s, retries 0 s, base 31 s, total 202 s

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
| p5-usage-noledger | uses p5-usage-deferred-turns.json, changed | 4 | 4 | 4 |  |

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
