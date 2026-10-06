# Phase 3 — current execution map

Status: in-progress (ship gate). Refreshed 2026-10-06 by the controller from directly read logs.

Branch `plan/tui-phase3`, local commits `edcc2f5..` (P3-KEYS at `72cdea8`, docs/receipts after it). Mission and the
fixed thirteen-row universe: [`ssot.md`](ssot.md). Chronological ledger: [`loop.md`](loop.md) (R188–R191 cover this
session). Primary receipts: [`receipts/`](receipts/). This file is the map, not the history.

## The one-paragraph state

All implementation work is committed on the branch; nothing is pushed or merged yet, issue 20 is open. The previous
stop boundary — the independent real-terminal recovery QA blocked on a PENDIN bit — is **closed**: the corrected QA
passed with exact comparison of every termios word on tmux (direct launch) and on herdr (interactive shell, the
failing configuration, PENDIN included), and the tmux-interactive 1→0 was reproduced by a non-xfx reference program
writing the same escape bytes while xfx's own restore call re-sets the bit in-process. P3-KEYS turned from
conditional to implemented after a herdr receipt showed Ctrl-C/D/U, Escape and Alt-Enter arriving in kitty u-form,
which the decoder dropped. What remains is the external review of the new decoder plus the ship decision, then the
delivery chain.

## Work table

| ID | State | Evidence (current tree unless marked) |
|---|---|---|
| P3-EDIT | met | `a03d08f`; scenario 22; reviewed per unit |
| P3-QUESTION | met | scenarios 23/23b; reviewed per unit |
| P3-APPROVAL | met | scenarios 24/25; reviewed per unit |
| P3-COMMIT | met within the stated bound | preflight + counted delivery + primary-band same-call recovery + bounded prefix retention; every other `Partial` contained + diagnostic + session end; independent once/fatal/refusal QA exact on tmux and herdr (`receipts/2026-10-06-wu4-*`) |
| P3-LAYOUT | met | scenario 26 |
| P3-THEME | met | scenario 27 on release binaries; offscreen native scrollback never retinted |
| P3-KEYS | met (pending review) | `72cdea8`: upstream `kittyUnicodeKeyAction` (c1db919) for `u` and `27;m;k~`; herdr receipt replayed in unit + PTY tests; RED→GREEN; herdr live u-form Ctrl-U/Ctrl-D/Esc in independent QA |
| P3-ACTIVITY | met | role 252/238, row attributes asserted |
| P3-DIAGNOSTIC | met | `diagnostic.rs`; native falsifiers in the full gate; real-terminal files observed (`exhausted`/errno5, `partial`/null, 0600) |
| P3-WRAP | met | CI cost gate; fresh 300×200×1000 replay 4.9 ms vs 32 ms |
| P3-QA | met | tracked 33 scenarios / 797 checks; independent tmux + herdr QA (WU-4) |
| P3-DOCS | met (pending review) | `03`/`06`/parity current-tense, bounds kept; no-stubs ok, parity 24/0 |
| P3-SHIP | to work | review → PR → CI → merge → preview → brew install → real-profile TUI receipt → close #20 |

## Fresh gate receipt (tree = `063fc66` + P3-KEYS, byte-identical to `72cdea8`)

fmt; clippy default and fault-injection; default all-targets 2272/0/4 ignored; fault TUI 92/0; fault lib 1601/0/4;
no-stubs, no-secrets, identity, preview-contract; both release builds; preflight cost gate; CLI smoke 46/0; TUI smoke
33 scenarios + oracle, 797 checks / 0 failures. Logs: session scratch `xfx-p3/gate-keys-1/`. The docs-only commits after
`72cdea8` were re-checked with no-stubs and the parity test.

## Boundaries that still hold

- Recovery is the primary band's `Partial` only; parser resync on a real terminal is not claimed.
- No kernel-source investigation of PENDIN by any route; the API safeguard result recorded in R179 stands.
- Preview channel only. No `v*` tag, no stable release, no production deploy without the user.
- The user's llmux daemon and default tmux server are not restarted or modified; fixtures use fake credentials and loopback.
