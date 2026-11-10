## fx parity sweep, per pull request (v0.0.16)

- fx v0.0.16, sha256 `8e6cda2efddb1d116ccc291e678809b3dd0be342fdb84bbdf3c683d179a3c6a2`
- head `208a8dd4be52`, base `8f9214251710` (both xfx builds against the same fx)
- 125 scenarios, 815 snap pairs: 576 pass at the head, 18 new, **1 regression(s)**
- wall: head 2815 s, retries 518 s, base 1788 s, total 5125 s

### Regressions: the base passes these, the head does not

| scenario | snap | base | head | runs (base / head) | first differing row (fx / xfx) |
|---|---|---|---|---|---|
| p5-mcp-fail | list | normalized-identical | unresolved-defect | 1 / 1 | row 6: `› bad                     connecting              profile` / `› bad                     retrying                profile` |

### Scenarios

| scenario | why | pairs | pass at head | new | not passing at head |
|---|---|---|---|---|---|
| startup | core | 1 | 1 | 0 |  |
| composer-wrap | core, receipt | 11 | 11 | 0 |  |
| streaming | core | 2 | 2 | 0 |  |
| statusline-turn | core, receipt | 3 | 3 | 0 |  |
| question | core | 9 | 9 | 0 |  |
| allowlist-remember | core | 7 | 7 | 0 |  |
| model-column | core | 8 | 8 | 0 |  |
| settings-scrollback | core | 5 | 5 | 0 |  |
| h1-resize | core | 3 | 1 | 0 | large (unresolved-defect, base unresolved-defect), small (unresolved-defect, base unresolved-defect) |
| quit | core | 4 | 4 | 0 |  |
| model-column-hold | changed, receipt | 4 | 4 | 0 |  |
| model-column-idle | changed, receipt | 3 | 3 | 0 |  |
| model-column-reask | changed, receipt | 4 | 4 | 1 |  |
| model-floor | changed, receipt | 3 | 3 | 0 |  |
| model-parked | changed, receipt | 5 | 4 | 0 | menu-loading (unresolved-defect, base unresolved-defect) |
| model-reask | changed, receipt | 4 | 4 | 0 |  |
| p5-effort-fork-effort-down | changed, receipt | 2 | 2 | 0 |  |
| p5-input-model-fail | changed, receipt | 2 | 2 | 0 |  |
| p5-input-row-limit-menus | changed, uses p5-input-row-limit-models.json, receipt | 4 | 4 | 4 |  |
| p5-input-row-limit-short | changed, receipt | 4 | 2 | 4 | slash (unresolved-defect, new), slash-pgdn (unresolved-defect, new) |
| p5-input-row-limit | changed, receipt | 9 | 3 | 9 | at (unresolved-defect, new), at-pgdn (unresolved-defect, new), help (unresolved-defect, new), help-pgdn (unresolved-defect, new), slash (unresolved-defect, new), slash-pgdn (unresolved-defect, new) |
| p5-mcp-menu-new | changed, uses p5-mcp-menu-profile.json, receipt | 34 | 34 | 0 |  |
| p5-mcp-menu-old | changed, uses p5-mcp-menu-profile.json, receipt | 16 | 16 | 0 |  |
| p5-panels-badsettings | changed, receipt | 1 | 0 | 0 | launch (unresolved-defect, base unresolved-defect) |
| p5-panels-status-badsettings | changed, receipt | 3 | 0 | 0 | closed (approved-divergence, base approved-divergence), launch (approved-divergence, base approved-divergence), open (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-stop-in-catalog-wait | changed, receipt | 3 | 3 | 0 |  |
| status-version-stats | receipt | 8 | 1 | 0 | stats (unresolved-defect, base unresolved-defect), stats-after-turn (unresolved-defect, base unresolved-defect), stats-extra (unresolved-defect, base unresolved-defect), status (approved-divergence-cascade, base approved-divergence-cascade), status-after-turn (unresolved-defect, base unresolved-defect), status-now (unresolved-defect, base unresolved-defect), turn (unresolved-defect, base unresolved-defect) |
| alias | receipt | 11 | 9 | 0 | help-query-command (unresolved-defect, base unresolved-defect), help-query-name (unresolved-defect, base unresolved-defect) |
| read-failures | receipt | 6 | 5 | 0 | live-ctrl-o-up1 (unresolved-defect, base unresolved-defect) |
| rename | receipt | 5 | 4 | 0 | relaunched (unresolved-defect, base unresolved-defect) |
| rename-midturn | receipt | 3 | 3 | 0 |  |
| resume | receipt | 13 | 13 | 0 |  |
| resume-empty | receipt | 4 | 4 | 0 |  |
| resume-launch | receipt | 4 | 4 | 0 |  |
| resume-paging | receipt | 8 | 2 | 0 | down14 (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), down15 (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), down16-loaded (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), down17 (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), query-ve-down3 (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), tab-all (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs) |
| resume-tools | receipt | 18 | 7 | 0 | cmd-ask (unresolved-defect, base unresolved-defect), ctrl-o (unresolved-defect, base unresolved-defect), ctrl-o-up1 (unresolved-defect, base unresolved-defect), edit-ask (unresolved-defect, base unresolved-defect), exit-ask (unresolved-defect, base unresolved-defect), live-ctrl-o (unresolved-defect, base unresolved-defect), live-ctrl-o-up1 (unresolved-defect, base unresolved-defect), live-ctrl-o-up2 (unresolved-defect, base unresolved-defect), live-ctrl-o-up3 (unresolved-defect, base unresolved-defect), long-later (unresolved-defect, base unresolved-defect), write-ask (unresolved-defect, base unresolved-defect) |
| search-roots | receipt | 2 | 2 | 0 |  |
| timeout | receipt | 2 | 0 | 0 | live (unresolved-defect, base unresolved-defect), live-ctrl-o (unresolved-defect, base unresolved-defect) |
| statusline | receipt | 8 | 8 | 0 |  |
| approvals | receipt | 10 | 0 | 0 | after-cc-cmd-ask (unresolved-defect, base unresolved-defect), after-esc (unresolved-defect, base unresolved-defect), after-esc.full (unresolved-defect, base unresolved-defect), cmd2-ask (unresolved-defect, base unresolved-defect), edit-amend-typed (unresolved-defect, base unresolved-defect), edit-ask (unresolved-defect, base unresolved-defect), edit-down1 (unresolved-defect, base unresolved-defect), edit-down3-wrap (unresolved-defect, base unresolved-defect), edit-tab-amend (unresolved-defect, base unresolved-defect), write-ask (unresolved-defect, base unresolved-defect) |
| approvals-big | receipt | 3 | 1 | 0 | write-big (unresolved-defect, base unresolved-defect), write-big-walked (unresolved-defect, base unresolved-defect) |
| fulltranscript | receipt | 15 | 6 | 0 | cmd-ask (unresolved-defect, base unresolved-defect), cmd-ask-ctrl-o (unresolved-defect, base unresolved-defect), edit-ask (unresolved-defect, base unresolved-defect), full (unresolved-defect, base unresolved-defect), full-pgdn (unresolved-defect, base unresolved-defect), full-pgup (unresolved-defect, base unresolved-defect), full-up2 (unresolved-defect, base unresolved-defect), full2 (unresolved-defect, base unresolved-defect), full3 (unresolved-defect, base unresolved-defect) |
| rules-read | receipt | 7 | 4 | 0 | ask (unresolved-defect, base unresolved-defect), ask2 (unresolved-defect, base unresolved-defect), listed (unresolved-defect, base unresolved-defect) |
| skills | receipt | 18 | 2 | 0 | add-local (unresolved-defect, base unresolved-defect), bogus (unresolved-defect, base unresolved-defect), create (unresolved-defect, base unresolved-defect), create-bad (unresolved-defect, base unresolved-defect), dollar (unresolved-defect, base unresolved-defect), dollar-dem (unresolved-defect, base unresolved-defect), dollar-enter (unresolved-defect, base unresolved-defect), end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base unresolved-defect), menu (unresolved-defect, base unresolved-defect), menu-down (unresolved-defect, base unresolved-defect), path (unresolved-defect, base unresolved-defect), remove-made (unresolved-defect, base unresolved-defect), remove-workspace (unresolved-defect, base unresolved-defect), sent (unresolved-defect, base unresolved-defect), show-demo (unresolved-defect, base unresolved-defect) |
| skills-ghost | receipt | 15 | 14 | 0 | dollar-menu (unresolved-defect, base unresolved-defect) |
| skills-paste | receipt | 3 | 2 | 0 | paste (unresolved-defect, base unresolved-defect) |
| skills-slash | receipt | 8 | 5 | 0 | slash-all (unresolved-defect, base unresolved-defect), slash-all-up (unresolved-defect, base unresolved-defect), slash-de (unresolved-defect, base unresolved-defect) |
| image-paste | receipt | 7 | 7 | 0 |  |
| links | receipt | 6 | 5 | 0 | full (unresolved-defect, base unresolved-defect) |
| links-image | receipt | 3 | 3 | 0 |  |
| credits-busy | receipt | 2 | 2 | 0 |  |
| usage-billed | receipt | 8 | 8 | 0 |  |
| usage-deferred | receipt | 4 | 4 | 0 |  |
| approvals-amend-limit | receipt | 5 | 1 | 0 | asked (unresolved-defect, base unresolved-defect), refused (unresolved-defect, base unresolved-defect), refused-after-edit (unresolved-defect, base unresolved-defect), refused-again (unresolved-defect, base unresolved-defect) |
| image | receipt | 17 | 17 | 0 |  |
| image-composer | receipt | 8 | 8 | 0 |  |
| image-inline | receipt | 5 | 5 | 0 |  |
| image-nonnative | receipt | 3 | 1 | 0 | end.full (unresolved-defect, base unresolved-defect), sent (unresolved-defect, base unresolved-defect) |
| image-readfile | receipt | 2 | 2 | 0 |  |
| image-undo | receipt | 5 | 5 | 0 |  |
| image-unknown | receipt | 3 | 3 | 0 |  |
| image-withheld | receipt | 2 | 2 | 0 |  |
| paste-block-undo | receipt | 8 | 8 | 0 |  |
| paste-card | receipt | 7 | 7 | 0 |  |
| notice-wrap | receipt | 1 | 1 | 0 |  |
| trace | receipt | 4 | 4 | 0 |  |
| trace-nowhere | receipt | 1 | 1 | 0 |  |
| workspace | receipt | 11 | 11 | 0 |  |
| workspace-index | receipt | 2 | 2 | 0 |  |
| titles | receipt | 3 | 3 | 0 |  |
| titles-idle | receipt | 2 | 0 | 0 | answered (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs), trace (unresolved-defect, base unresolved-defect, 4 head runs, 4 base runs) |
| titles-off | receipt | 2 | 2 | 0 |  |
| titles-rename | receipt | 2 | 2 | 0 |  |
| titles-steer | receipt | 2 | 2 | 0 |  |
| p5-input-paste | receipt | 2 | 2 | 0 |  |
| paste-suffix | receipt | 2 | 2 | 0 |  |
| p5-panels-keys | receipt | 10 | 2 | 0 | approval-arrives (unresolved-defect, base unresolved-defect), approval-esc (approved-divergence-cascade, base approved-divergence-cascade), btab (approved-divergence-cascade, base approved-divergence-cascade), ctrl-c (approved-divergence-cascade, base approved-divergence-cascade), end (approved-divergence-cascade, base approved-divergence-cascade), open-before-approval (approved-divergence-cascade, base approved-divergence-cascade), paste (approved-divergence-cascade, base approved-divergence-cascade), tab-pgdn (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-cli-ledger-update-help | receipt | 3 | 0 | 0 | h00-update-help.full (unresolved-defect, base unresolved-defect), h01-upgrade-help.full (unresolved-defect, base unresolved-defect), h02-help-update.full (unresolved-defect, base unresolved-defect) |
| p5-effort-fork-effort-backtab | receipt | 2 | 2 | 0 |  |
| p5-effort-fork-effort-column | receipt | 17 | 17 | 0 |  |
| p5-effort-fork-effort-ctrlc | receipt | 2 | 2 | 0 |  |
| p5-effort-fork-effort-loading | receipt | 2 | 2 | 0 |  |
| p5-effort-fork-effort-menu | receipt | 18 | 18 | 0 |  |
| p5-effort-fork-effort-nolevels | receipt | 5 | 5 | 0 |  |
| p5-effort-fork-effort-short | receipt | 3 | 3 | 0 |  |
| p5-effort-fork-fork | receipt | 16 | 15 | 0 | status (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-effort-fork-fork-continue | receipt | 2 | 2 | 0 |  |
| p5-effort-fork-fork-edge | receipt | 11 | 10 | 0 | status-empty (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-effort-fork-fork-empty | receipt | 4 | 4 | 0 |  |
| p5-effort-fork-oldread-src | receipt | 3 | 0 | 0 | doctor.full (unresolved-defect, base unresolved-defect), picker-enter-fork (unresolved-defect, base unresolved-defect), resume-source (unresolved-defect, base unresolved-defect) |
| p5-effort-fork-oldread-v1 | receipt | 4 | 2 | 0 | picker-all (unresolved-defect, base unresolved-defect), resume-id (unresolved-defect, base unresolved-defect) |
| p5-effort-fork-oldread-v2 | receipt | 7 | 2 | 0 | continued (unresolved-defect, base unresolved-defect), picker-all (unresolved-defect, base unresolved-defect), quit (unresolved-defect, base unresolved-defect), resume-id (unresolved-defect, base unresolved-defect), resume-id.full (unresolved-defect, base unresolved-defect) |
| p5-effort-fork-slash-rank | receipt | 4 | 3 | 0 | e (unresolved-defect, base unresolved-defect) |
| p5-input-mcp | receipt | 9 | 9 | 0 |  |
| p5-mcp-config | receipt | 3 | 1 | 0 | list (unresolved-defect, base unresolved-defect), removed.full (unresolved-defect, base unresolved-defect) |
| p5-mcp-config-ignored | receipt | 4 | 0 | 0 | end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base unresolved-defect), menu (unresolved-defect, base unresolved-defect), startup (unresolved-defect, base unresolved-defect) |
| p5-mcp-config-vars | receipt | 4 | 2 | 0 | end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base unresolved-defect) |
| p5-mcp-direct | receipt | 2 | 2 | 0 |  |
| p5-mcp-dismiss | receipt | 2 | 2 | 0 |  |
| p5-mcp-empty | receipt | 24 | 24 | 0 |  |
| p5-mcp-errdata | receipt | 1 | 1 | 0 |  |
| p5-mcp-fail | receipt | 6 | 3 | 0 | end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base normalized-identical), show-bad (unresolved-defect, base unresolved-defect) |
| p5-mcp-failed | receipt | 4 | 4 | 0 |  |
| p5-mcp-known | receipt | 1 | 1 | 0 |  |
| p5-mcp-live | receipt | 6 | 6 | 0 |  |
| p5-mcp-notes | receipt | 4 | 2 | 0 | end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base unresolved-defect) |
| p5-mcp-rpcerror | receipt | 1 | 1 | 0 |  |
| p5-mcp-turn | receipt | 3 | 3 | 0 |  |
| p5-mcp-unavailable | receipt | 1 | 1 | 0 |  |
| p5-mcp-verbs | receipt | 28 | 1 | 0 | add-badname (unresolved-defect, base unresolved-defect), add-new-http (unresolved-defect, base unresolved-defect), add-new-syntax (unresolved-defect, base unresolved-defect), add-old-http (unresolved-defect, base unresolved-defect), add-old-syntax (unresolved-defect, base unresolved-defect), approve (unresolved-defect, base unresolved-defect), auth-remote (unresolved-defect, base unresolved-defect), complete-args (unresolved-defect, base unresolved-defect), complete-show (unresolved-defect, base unresolved-defect), end.full (unresolved-defect, base unresolved-defect), list (unresolved-defect, base unresolved-defect), list-after (unresolved-defect, base unresolved-defect), login-remote (unresolved-defect, base unresolved-defect), login-stdio (unresolved-defect, base unresolved-defect), logout-remote (unresolved-defect, base unresolved-defect), prompt-list (unresolved-defect, base unresolved-defect), reject (unresolved-defect, base unresolved-defect), remove-three (unresolved-defect, base unresolved-defect), remove-two (unresolved-defect, base unresolved-defect), remove-web (unresolved-defect, base unresolved-defect), remove-web2 (unresolved-defect, base unresolved-defect), resource-list (unresolved-defect, base unresolved-defect), show-bare (unresolved-defect, base unresolved-defect), show-fixture (unresolved-defect, base unresolved-defect), show-proj (unresolved-defect, base unresolved-defect), show-remote (unresolved-defect, base unresolved-defect), trust-approve (unresolved-defect, base unresolved-defect) |
| p5-mcp-wire-new | receipt | 1 | 1 | 0 |  |
| p5-mcp-wire-old | receipt | 1 | 1 | 0 |  |
| p5-panels-help | receipt | 17 | 10 | 0 | open (unresolved-defect, base unresolved-defect), query-status (unresolved-defect, base unresolved-defect), tab-account (unresolved-defect, base unresolved-defect), tab-model (unresolved-defect, base unresolved-defect), tab-product (unresolved-defect, base unresolved-defect), tab-workspace (unresolved-defect, base unresolved-defect), tab-wrap (unresolved-defect, base unresolved-defect) |
| p5-panels-noauth | receipt | 2 | 0 | 0 | launch (approved-divergence, base approved-divergence), open (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-permissions | receipt | 20 | 16 | 0 | after-revoke (unresolved-defect, base unresolved-defect), approval (unresolved-defect, base unresolved-defect), grants (unresolved-defect, base unresolved-defect), status-grants (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-permissions-saved | receipt | 8 | 5 | 0 | after-revoke (unresolved-defect, base unresolved-defect), approval (unresolved-defect, base unresolved-defect), saved (unresolved-defect, base unresolved-defect) |
| p5-panels-slash | receipt | 15 | 5 | 0 | down12 (unresolved-defect, base unresolved-defect), down18 (unresolved-defect, base unresolved-defect), down24 (unresolved-defect, base unresolved-defect), down30 (unresolved-defect, base unresolved-defect), down36 (unresolved-defect, base unresolved-defect), down38 (unresolved-defect, base unresolved-defect), down6 (unresolved-defect, base unresolved-defect), filter-e (unresolved-defect, base unresolved-defect), filter-f (unresolved-defect, base unresolved-defect), slash (unresolved-defect, base unresolved-defect) |
| p5-panels-status | receipt | 11 | 5 | 0 | after-turn (approved-divergence-cascade, base approved-divergence-cascade), down (approved-divergence-cascade, base approved-divergence-cascade), open (approved-divergence-cascade, base approved-divergence-cascade), open-after-midturn (approved-divergence-cascade, base approved-divergence-cascade), open-midturn (approved-divergence-cascade, base approved-divergence-cascade), typed (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-status-first | receipt | 3 | 1 | 0 | after-first (approved-divergence-cascade, base approved-divergence-cascade), open-first (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-status-short | receipt | 6 | 1 | 0 | ctrl-k (approved-divergence-cascade, base approved-divergence-cascade), down-end (approved-divergence-cascade, base approved-divergence-cascade), down1 (approved-divergence-cascade, base approved-divergence-cascade), open (approved-divergence-cascade, base approved-divergence-cascade), up1 (approved-divergence-cascade, base approved-divergence-cascade) |
| p5-panels-sweep-a | receipt | 33 | 4 | 0 | alias (unresolved-defect, base unresolved-defect), allowlist (unresolved-defect, base unresolved-defect), compact-empty (unresolved-defect, base unresolved-defect), copy (unresolved-defect, base unresolved-defect), credits (unresolved-defect, base unresolved-defect), fast (unresolved-defect, base unresolved-defect), feedback (unresolved-defect, base unresolved-defect), image-missing (unresolved-defect, base unresolved-defect), images (unresolved-defect, base unresolved-defect), login (unresolved-defect, base unresolved-defect), login-closed (unresolved-defect, base unresolved-defect), logout (unresolved-defect, base unresolved-defect), provider (unresolved-defect, base unresolved-defect), provider-closed (unresolved-defect, base unresolved-defect), rename (unresolved-defect, base unresolved-defect), resume (unresolved-defect, base unresolved-defect), resume-closed (unresolved-defect, base unresolved-defect), settings (unresolved-defect, base unresolved-defect), settings-closed (unresolved-defect, base unresolved-defect), shell (unresolved-defect, base unresolved-defect), stats (unresolved-defect, base unresolved-defect), statusline (unresolved-defect, base unresolved-defect), statusline-closed (unresolved-defect, base unresolved-defect), trace (unresolved-defect, base unresolved-defect), ultrafast (unresolved-defect, base unresolved-defect), undo (unresolved-defect, base unresolved-defect), version (unresolved-defect, base unresolved-defect), workspace (unresolved-defect, base unresolved-defect), workspace-closed (unresolved-defect, base unresolved-defect) |
| p5-panels-sweep-b | receipt | 11 | 4 | 0 | after-compact (unresolved-defect, base unresolved-defect), compact (unresolved-defect, base unresolved-defect), rename (unresolved-defect, base unresolved-defect), resume (unresolved-defect, base unresolved-defect), stats (unresolved-defect, base unresolved-defect), trace (unresolved-defect, base unresolved-defect), undo (unresolved-defect, base unresolved-defect) |
| p5-permissions-early | receipt | 11 | 11 | 0 |  |
| p5-permissions-early-probe-ask-after | receipt | 1 | 0 | 0 | after-call (approved-divergence, base approved-divergence) |
| p5-session-early-prefs | receipt | 5 | 5 | 0 |  |

Snaps the base takes and the head no longer does: `p5-panels-badsettings/open`.

A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it counts. CI green does not replace the local re-run.
