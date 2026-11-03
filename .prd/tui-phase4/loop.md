# TUI Phase 4 — loop

Status: in-progress — closing out under the parity program ([`../07-fx-parity-program.md`](../07-fx-parity-program.md)): land the units below, freeze `c1db9191` scope, ship to `main`.
Branch: `plan/tui-phase4` (worktree `.worktrees/plan-tui-phase4`), base `14ad042`; pushed to origin as a plain branch (no PR yet).
Reference binary: fx `c1db9191` built with `zig 0.16.0 build -Doptimize=ReleaseSafe --cache-dir <fresh>`, sha256 `7b76319a871163ada64d74539ebee592e0e216041cd28465127457500a9be969`, bin dir read-only. Every fx run gets `FX_AUTO_UPGRADE=0` (capture.sh, `d91a9968`).
Done ledger: [`loop.done.md`](loop.done.md).

## Build facts

- Batch gate (one integration branch per batch, then `--ff-only` into plan): `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`, plain and `--features fault-injection`; `cargo test --no-fail-fast`, plain and fault-injection; `scripts/check-xfx-identity.sh`, `check-no-stubs.sh`, `check-no-secrets.sh`, `check-preview-contract.sh`. Run in the foreground or a tool's background mode, never a shell `&`/`nohup` job (an inherited SIG_IGN breaks `process::tests`).
- Last green: `9aa666c7` (batch 20) — lib 3490, tui 157; fault-injection lib 3497, tui 171; 0 failed; clippy 0. Toolchain rustc 1.99.1, MSRV 1.96 (`fetch_update` sites carry `#[allow(deprecated)]`).
- Captures: `scripts/fxcap/capture.sh {fx,xfx} <scenario> 40x120 [out]` with `FX_BIN=<reference>` and `TMPDIR=/private/tmp/claude-501`; comparator `receipts/m1/cells.py`.

## Open work

Program: [`../07-fx-parity-program.md`](../07-fx-parity-program.md) (trinity unanimous R5, 2026-11-03). Phase 4 ships once the P2 preconditions land and the receipt is green; everything else goes to `main` as its own PR after that.

### P2 preconditions

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| SWEEP | 🟡 | local sweep + ledger + scoreboard + closed denominator | `feat/fxcap-sweep` | report → merge |
| P2-RECEIPT | 🔴 | final-SHA gates, CI, SM1 with the behaviour list, sweep vs `c1db9191`, delta/seam/safety review, frames read, public-suitability scan, session back-compat, RED/GREEN | — | after S1-sec, SWEEP |
| SHIP | 🚪 | PR → `main` → preview + tap → real-profile run (sessions backed up), label "Phase 4 integration preview — fx c1db919 baseline, before the 0.0.16 transition" | — | after P2-RECEIPT |

### After P2 (own PRs to `main`)

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| C7-r2 | 🟡 | `help_entry` on every row, `/help` search as fx | `feat/tui-phase4-alias` @ `8d83f961` | PR after SHIP |
| D4e | 🟡 | collapse off/on re-projects the visible screen as fx | `fix/tui-collapse-expand` @ `65c4425e` | PR after SHIP |
| X1-r2 | 🟡 | paste under key-swallowing menus; question freeform bound | `fix/tui-paste-suffix` | report → PR after SHIP |
| P1-r3r4 | 🟡 | `! auth:`/`✗ auth:`; held prompt resent once a credential arrives (never without the user's confirmation, or exactly as fx) | `feat/tui-phase4-provider-session` | report → PR after SHIP |
| S1-vis | 🟡 | the `skill` tool's context notices; the `/skills` usage line still names `url` | `feat/tui-phase4-skills` | after S1-sec |
| C8 | 🟡 | `/shell reload` — new execution surface, own security review | `feat/tui-phase4-shell-reload` | report → PR after SHIP |
| K-resize | 🔴 | resize leaves a stale cut-off row (H1 `xfx/…-small.txt`) | — | dispatch |
| K-jobctl | 🔴 | Ctrl-Z ignored (fx suspends); external SIGTSTP restores before stopping (fx writes nothing) | — | dispatch |
| K-recover | 🔴 | a cut stream or refused connection fails the turn; fx retries and recovers (`[Response interrupted. Restarting.]`, `⚠ Connection lost · waiting for connection`) | — | dispatch |
| K-mcpwords | 🔴 | a connecting server shows `Reloading` (fx `Connecting`); startup failure `* mcp:` (fx `! mcp:`) | — | dispatch |
| DOCS | 🚪 | README counts, parity header counts and pin, `03-tui-port.md`, `06-qa-harness.md`, `UPSTREAM.md`, D4d receipt stale lines | — | with SHIP |

## Open gaps

| ID | Status | Gap | Next |
|---|---|---|---|
| G-copy-flake | 🔴 | `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` fails at load ≥ 60 (5/7, T9) | watch in the batch gates; RCA unit on recurrence |
| G-caret | 🔴 | the hardware cursor stays visible under the reversed caret (F3) | check in the sweep |
| G-decisions | 🚪 | user decisions: Codex/Grok sign-in (#88), MCP OAuth and `/mcp auth|logout`, other agents' skill roots, network skill install, the provider-switch reasoning notice, the OSC 8 scheme allowlist, the `ask_user_question` >4096-byte saved-rule exemption (R1; kept fail-closed) | asked batched at the blocking point (program § Decisions) |

## Round log (current)

- 2026-11-03: batches 14–22 landed (T8 r2, D2c r4–r6, W9 r5, M1, P1 r2, S1 r2, X1 r1, C7 r1, R1 r2, H1); fx self-upgrade guard d91a9968; program adopted. Public-suitability scan of plan's files and `origin/main..plan` history: no personal names, user paths or real keys (tests use `/Users/me`, `/Users/u`; the two `sk-` hits are fake test strings, shortened in 56f20fcc); harness scratch paths (`/private/tmp/claude-501/fxcap.*`) remain in some receipts, no personal data.
