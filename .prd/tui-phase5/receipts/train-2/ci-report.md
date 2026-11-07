## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `bca3e79300a1`, base `6066d67ef3b6` (both xfx builds against the same fx)
- 50 scenarios, 200 snap pairs: 111 pass at the head, 83 new, **0 regression(s)**
- wall: head 870 s, retries 0 s, base 206 s, total 1078 s

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
| p5-call-row-long | uses p5-call-row-long-turns.json, uses p5-call-row-long-xfx.json, changed, receipt | 1 | 0 | 1 | end (unresolved-defect, new) |
| p5-call-row-safe | uses p5-call-row-safe-turns.json, uses p5-call-row-safe-xfx.json, changed, receipt | 2 | 0 | 2 | end (approved-divergence, new), full (unresolved-defect, new) |
| p5-cli-ledger-update-help | changed, receipt | 3 | 0 | 3 | h00-update-help.full (unresolved-defect, new), h01-upgrade-help.full (unresolved-defect, new), h02-help-update.full (unresolved-defect, new) |
| p5-inline-code-retint-light-tc | changed, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), light (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-inline-code-retint-tc | changed, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-inline-code-table-light-tc | changed, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), light (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-inline-code-table-tc | changed, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-lifecycle-relhome-image | changed, uses p5-relhome-made/relative/homedir/README, receipt | 2 | 2 | 2 |  |
| p5-lifecycle-relhome-made | changed, uses p5-relhome-made-turns.json, uses p5-relhome-made/relative/homedir/README, receipt | 3 | 3 | 3 |  |
| p5-lifecycle-relhome-resume | changed, uses p5-relhome-made/relative/homedir/README, receipt | 1 | 0 | 1 | r00-resume.full (unresolved-defect, new) |
| p5-lifecycle-relhome-unsaved | changed, uses p5-relhome-made/relative/homedir/README, uses p5-relhome-unsaved-turns.json, receipt | 9 | 8 | 9 | u02-status (approved-divergence-cascade, new) |
| p5-toolgroup-retint | uses p5-toolgroup-retint-fx.json, uses p5-toolgroup-retint-xfx.json, changed, receipt | 3 | 1 | 3 | dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-toolgroup-retint-light | uses p5-toolgroup-retint-fx.json, changed, uses p5-toolgroup-retint-xfx.json, receipt | 3 | 1 | 3 | dark (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-toolgroup-retint-light-tc | uses p5-toolgroup-retint-fx.json, changed, uses p5-toolgroup-retint-xfx.json, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), light (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-toolgroup-retint-tc | uses p5-toolgroup-retint-fx.json, changed, uses p5-toolgroup-retint-xfx.json, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-toolstop-retint | uses p5-toolstop-retint-fx.json, uses p5-toolstop-retint-xfx.json, changed, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-toolstop-retint-light | uses p5-toolstop-retint-fx.json, changed, uses p5-toolstop-retint-xfx.json, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), light (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-toolstop-retint-light-tc | uses p5-toolstop-retint-fx.json, changed, uses p5-toolstop-retint-xfx.json, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), light (unresolved-defect, new), light-again (unresolved-defect, new) |
| p5-toolstop-retint-tc | uses p5-toolstop-retint-fx.json, changed, uses p5-toolstop-retint-xfx.json, receipt | 3 | 0 | 3 | dark (unresolved-defect, new), dark-again (unresolved-defect, new), light (unresolved-defect, new) |
| p5-transcript-diffask | uses p5-transcript-diffask-turns.json, changed, receipt | 4 | 3 | 4 | ask (unresolved-defect, new) |
| p5-transcript-diffbad | uses p5-transcript-diffbad-turns.json, changed, uses p5-transcript-diffws/bad.txt, uses p5-transcript-diffws/ctl.txt, uses p5-transcript-diffws/odd.txt, receipt | 3 | 0 | 3 | closed (unresolved-defect, new), edited (unresolved-defect, new), full (unresolved-defect, new) |
| p5-transcript-diffctl | uses p5-transcript-diffctl-xfx.json, changed, receipt | 4 | 0 | 0 | closed (unresolved-defect, base unresolved-defect), edited (unresolved-defect, base unresolved-defect), edited.full (unresolved-defect, base unresolved-defect), full (unresolved-defect, base unresolved-defect) |
| p5-transcript-diffodd | uses p5-transcript-diffodd-turns.json, changed, uses p5-transcript-diffws/bad.txt, uses p5-transcript-diffws/ctl.txt, uses p5-transcript-diffws/odd.txt, receipt | 3 | 3 | 3 |  |
| p5-transcript-diffwide | uses p5-transcript-diffwide-turns.json, changed, uses p5-transcript-diffwidews/wide.txt, receipt | 3 | 2 | 3 | full (unresolved-defect, new) |
| p5-transcript-diffwide2 | uses p5-transcript-diffwide2-turns.json, changed, uses p5-transcript-diffwide2ws/wide2.txt, receipt | 3 | 2 | 3 | full (unresolved-defect, new) |
| p5-transcript-safe | uses p5-transcript-safe-turns.json, uses p5-transcript-safe-ws/safe.txt, uses p5-transcript-safe-xfx.json, changed, receipt | 4 | 0 | 4 | end (approved-divergence, new), end.full (approved-divergence, new), full (unresolved-defect, new), full-pgup (unresolved-defect, new) |
| p5-transcript-safetab | uses p5-transcript-safetab-turns.json, uses p5-transcript-safetab-ws/tab.txt, uses p5-transcript-safetab-xfx.json, changed, receipt | 2 | 1 | 2 | full (unresolved-defect, new) |
| p5-transcript-submit-multi | changed, uses p5-transcript-submit-turns.json, receipt | 4 | 4 | 4 |  |
| p5-transcript-submit | uses p5-transcript-submit-turns.json, changed, receipt | 5 | 5 | 0 |  |
| resume-tools | receipt | 18 | 3 | 0 | again-stopped (unresolved-defect, base unresolved-defect), cmd-ask (unresolved-defect, base unresolved-defect), ctrl-o (unresolved-defect, base unresolved-defect), ctrl-o-up1 (unresolved-defect, base unresolved-defect), edit-ask (unresolved-defect, base unresolved-defect), exit-ask (unresolved-defect, base unresolved-defect), live-ctrl-o (unresolved-defect, base unresolved-defect), live-ctrl-o-up1 (unresolved-defect, base unresolved-defect), live-ctrl-o-up2 (unresolved-defect, base unresolved-defect), live-ctrl-o-up3 (unresolved-defect, base unresolved-defect), live.full (unresolved-defect, base unresolved-defect), long-later (unresolved-defect, base unresolved-defect), resumed (unresolved-defect, base unresolved-defect), resumed.full (unresolved-defect, base unresolved-defect), write-ask (unresolved-defect, base unresolved-defect) |
| p5-transcript-cancel | receipt | 7 | 0 | 0 | cmd-esc (unresolved-defect, base unresolved-defect), cmd-running (unresolved-defect, base unresolved-defect), end.full (unresolved-defect, base unresolved-defect), esc (unresolved-defect, base unresolved-defect), full (unresolved-defect, base unresolved-defect), mid (unresolved-defect, base unresolved-defect), next (unresolved-defect, base unresolved-defect) |
| p5-transcript-records | receipt | 4 | 0 | 0 | full-records (unresolved-defect, base unresolved-defect), full-records-120 (unresolved-defect, base unresolved-defect), full-records-80 (unresolved-defect, base unresolved-defect), start (unresolved-defect, base unresolved-defect) |
| p5-inline-code-retint | receipt | 3 | 3 | 0 |  |
| p5-inline-code-retint-light | receipt | 3 | 3 | 0 |  |
| p5-inline-code-table | receipt | 3 | 3 | 0 |  |
| p5-inline-code-table-light | receipt | 3 | 3 | 0 |  |
| p5-transcript-md | receipt | 5 | 3 | 0 | full (unresolved-defect, base unresolved-defect), full-pgup (unresolved-defect, base unresolved-defect) |
| p5-transcript-md-light | receipt | 5 | 3 | 0 | full (unresolved-defect, base unresolved-defect), full-pgup (unresolved-defect, base unresolved-defect) |
| p5-lifecycle-relhome | receipt | 2 | 2 | 0 |  |
| p5-transcript-tallfirst | receipt | 2 | 2 | 0 |  |

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
