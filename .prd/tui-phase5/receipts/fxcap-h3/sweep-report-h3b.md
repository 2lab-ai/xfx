## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `220f8acd771d`, base `b8f6994cd18a` (both xfx builds against the same fx)
- 15 scenarios, 117 snap pairs: 50 pass at the head, 14 new, **0 regression(s)**
- wall: head 270 s, retries 280 s, base 428 s, total 979 s

### Scenarios

| scenario | why | pairs | pass at head | new | not passing at head |
|---|---|---|---|---|---|
| startup | core | 1 | 1 | 0 |  |
| composer-wrap | core | 11 | 11 | 0 |  |
| streaming | core | 2 | 2 | 0 |  |
| statusline-turn | core | 3 | 3 | 0 |  |
| question | core | 9 | 9 | 0 |  |
| allowlist-remember | core | 7 | 3 | 0 | listed (unresolved-defect, base unresolved-defect), refusals (unresolved-defect, base unresolved-defect), revoke-done (unresolved-defect, base unresolved-defect), revoke-prompt (unresolved-defect, base unresolved-defect) |
| model-column | core | 8 | 8 | 0 |  |
| settings-scrollback | core | 5 | 5 | 0 |  |
| h1-resize | core | 3 | 1 | 0 | large (unresolved-defect, base unresolved-defect), small (unresolved-defect, base unresolved-defect) |
| quit | core | 4 | 4 | 0 |  |
| p5-usage-budget | changed, uses p5-usage-cache-budget.json, uses p5-usage-report.json, receipt | 14 | 0 | 2 | back (unresolved-defect, base unresolved-defect), closed (unresolved-defect, base unresolved-defect), detail (unresolved-defect, base unresolved-defect), detail-down (unresolved-defect, base unresolved-defect), detail-tab (unresolved-defect, base unresolved-defect), down (unresolved-defect, base unresolved-defect), left-7d (unresolved-defect, base unresolved-defect), open (unresolved-defect, base unresolved-defect), r (unresolved-defect, base unresolved-defect), r-updating (unresolved-defect, new), right-month (unresolved-defect, base unresolved-defect), tab-7d (unresolved-defect, base unresolved-defect), tab-today (unresolved-defect, base unresolved-defect), updating (unresolved-defect, new) |
| p5-usage-many | uses p5-usage-cache-many.json, changed, uses p5-usage-report.json, receipt | 5 | 1 | 2 | bottom (unresolved-defect, base unresolved-defect), last (unresolved-defect, new), open (unresolved-defect, base unresolved-defect), top (unresolved-defect, base unresolved-defect) |
| p5-usage-cached | uses p5-usage-cache.json, changed, uses p5-usage-report-401.json, uses p5-usage-report-429.json, uses p5-usage-report-500.json, uses p5-usage-report-bad.json, uses p5-usage-report.json, receipt | 21 | 0 | 6 | back (unresolved-defect, base unresolved-defect), btab-today (unresolved-defect, base unresolved-defect), cj (unresolved-defect, base unresolved-defect), ck (unresolved-defect, base unresolved-defect), closed (unresolved-defect, base unresolved-defect), detail (unresolved-defect, base unresolved-defect), detail-down (unresolved-defect, base unresolved-defect), detail-tab (unresolved-defect, base unresolved-defect), down (unresolved-defect, base unresolved-defect), left-7d (unresolved-defect, base unresolved-defect), open (unresolved-defect, base unresolved-defect), r (unresolved-defect, base unresolved-defect), r-401 (unresolved-defect, new), r-429 (unresolved-defect, new), r-500 (unresolved-defect, new), r-bad (unresolved-defect, new), r-updating (unresolved-defect, new), right-month (unresolved-defect, base unresolved-defect), tab-7d (unresolved-defect, base unresolved-defect), tab-today (unresolved-defect, base unresolved-defect), updating (unresolved-defect, new) |
| p5-usage-nokey | changed, uses p5-usage-report.json, receipt | 4 | 0 | 1 | closed (unresolved-defect, base unresolved-defect), open (unresolved-defect, base unresolved-defect), r (unresolved-defect, new), start (unresolved-defect, base unresolved-defect) |
| p5-usage-turns | uses p5-usage-report-turns.json, changed, receipt | 20 | 2 | 3 | arg (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), btab (unresolved-defect, base unresolved-defect), cost (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), cost-typed (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), credits (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), down (unresolved-defect, base unresolved-defect), enter (unresolved-defect, base unresolved-defect), esc (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), fetching (unresolved-defect, new), left (unresolved-defect, base unresolved-defect), open (unresolved-defect, base unresolved-defect), r (unresolved-defect, base unresolved-defect), r-7d (unresolved-defect, new), r-today (unresolved-defect, new), right (unresolved-defect, base unresolved-defect), stats (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), tab (unresolved-defect, base unresolved-defect), typed (unresolved-defect, base unresolved-defect) |

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
