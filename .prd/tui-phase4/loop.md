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

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| P1-r4 | 🟡 | a refused prompt is held and sent once a credential arrives (`awaiting_auth`); no hint lead during an approval; carries P1 r3 (`! auth:`/`✗ auth:`, any-credential lead) | `feat/tui-phase4-provider-session` | report → merge |
| R1-r2 | 🟡 | `ask_user_question` judged at admission; the amendment read-back as fx's user card; the read band's scope row; M1's MCP admission merged in | `feat/tui-phase4-rules` | report → merge |
| S1-r3 | 🟡 | skill resource identity re-check (both load modes); the `skill` tool's context notices | `feat/tui-phase4-skills` | report → merge |
| X1-r2 | 🟡 | a paste under a key-swallowing menu goes to the composer; a question's freeform paste bound | `fix/tui-paste-suffix` | report → merge |
| D4e | 🟡 | collapse off/on redraws the visible screen as fx's | `fix/tui-collapse-expand` | report → merge |
| C7 | 🟡 | `/alias` | `feat/tui-phase4-alias` | report → merge |
| C8 | 🟡 | `/shell reload` | `feat/tui-phase4-shell-reload` | report → merge |
| H1 | 🟡 | harness verbs: resize, signal, mid-run HOME file, request log, catalog swap, refused turn, usage routes, loopback HTTP MCP server | `feat/fxcap-verbs` | report → merge |
| SWEEP | 🔴 | the parity sweep (every scenario on fx and xfx, scoreboard) — program P3 | — | dispatch after H1 |
| SM1-final | 🚪 | smoke-tui against the final plan, release and fault-injection builds; rows other units invalidated | `fix/smoke-tui` | after the 🟡 rows land |
| DOCS | 🚪 | README command count, `docs/parity.md` header counts and pin, `03-tui-port.md`, `06-qa-harness.md`, `UPSTREAM.md`, the D4d receipt's stale lines | — | after the 🟡 rows land |
| SHIP | 🚪 | receipt (gates, CI, SM1, sweep vs `c1db9191`, trinity security review) → PR → `main` → preview + tap → real-profile run | — | after SM1, SWEEP, DOCS |

## Open gaps

| ID | Status | Gap | Next |
|---|---|---|---|
| G-copy-flake | 🔴 | `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` fails at load ≥ 60 (5/7, T9) | watch in the batch gates; RCA unit on recurrence |
| G-caret | 🔴 | the hardware cursor stays visible under the reversed caret (F3) | check in the sweep |
| G-decisions | 🚪 | user decisions: Codex/Grok sign-in (#88), MCP OAuth and `/mcp auth|logout`, other agents' skill roots, network skill install, the provider-switch reasoning notice, the OSC 8 scheme allowlist | asked batched at the blocking point (program P5) |
