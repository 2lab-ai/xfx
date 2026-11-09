---
title: 0.0.16 delta -- PASTE-GAP round 8 receipt (the grant width, per kind)
type: receipt
tags: [xfx, tui, phase5, input, paste, security]
---

# PASTE-GAP round 8

Round 7 closed at `ca0d97c8`. gpt6 found no MUST; opus approved. Both had
one note, on the same receipt line: round 7's README said "every fixture is
undisclosed at 62 columns and whole at 63". `plane_grants_from` measures
the one request its caller passes, an Edit. A Write's second label is a
cell longer.

**What is true, measured** (`grant-widths.txt`; a temporary test run once
at `ca0d97c8`, then removed):
- **Edit:** 56 + 7 cells, so grantable from **63**.
- **Write:** 57 + 7 cells, so grantable from **64**.

Neither kind is grantable below 63. So the round 7 test's choice of widths
holds: at 20 and 30 columns a `1` can only be refused as never whole.

**The fix:** `../round7/README.md`'s "The widths" paragraph now states both
widths. It cites the cells (`approval_view.rs:268-269`, `:301`, `:803-804`)
and this run, and names the old sentence as corrected here. The bullet that
says "63, measured by the test" now names the Edit it measures. The test
comment's "63 columns today" stays: that test measures an Edit.

Receipt files only; no source line changed. Checks: the path grep and
`check-no-secrets`, as the brief scales them.
