## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `44bb45e621b0`, base `44bb45e621b0` (both xfx builds against the same fx)
- 11 scenarios, 65 snap pairs: 53 pass at the head, 0 new, **0 regression(s)**
- wall: head 226 s, retries 0 s, base 93 s, total 320 s

### Scenarios

| scenario | why | pairs | pass at head | new | not passing at head |
|---|---|---|---|---|---|
| p5-panels-status | core | 11 | 5 | 0 | after-turn (approved-divergence-cascade, base approved-divergence-cascade), down (approved-divergence-cascade, base approved-divergence-cascade), open (approved-divergence-cascade, base approved-divergence-cascade), open-after-midturn (approved-divergence-cascade, base approved-divergence-cascade), open-midturn (approved-divergence-cascade, base approved-divergence-cascade), typed (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-status-short | core | 6 | 1 | 0 | ctrl-k (approved-divergence-cascade, base approved-divergence-cascade), down-end (approved-divergence-cascade, base approved-divergence-cascade), down1 (approved-divergence-cascade, base approved-divergence-cascade), open (approved-divergence-cascade, base approved-divergence-cascade), up1 (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-mcp-empty | core | 24 | 24 | 0 |  |
| p5-lifecycle-relhome-image | core | 2 | 2 | 0 |  |
| model-reask | core | 4 | 4 | 0 |  |
| model-parked | core | 5 | 4 | 0 | menu-loading (unresolved-defect, base unresolved-defect) |
| p5-input-model-fail | core | 2 | 2 | 0 |  |
| model-column-idle | core | 3 | 3 | 0 |  |
| model-column-reask | core | 4 | 4 | 0 |  |
| p5-stop-in-catalog-wait | core | 3 | 3 | 0 |  |
| startup | core | 1 | 1 | 0 |  |

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
