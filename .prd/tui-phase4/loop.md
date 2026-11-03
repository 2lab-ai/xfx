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
| SWEEP | 🟡 | sweep tooling landed (batch 25); r2 at `1a94cdff`: 624/861; preliminary at `62146228`: 679/917 (74.0%), 0 failed/timeout, 0 snap regressions vs r2, 22 improvements; 23 snaps on rows no cause covers (h1-* recovery/MCP-HTTP/signal rows, `approvals/after-esc` row 15, `rules-read/ask` row 19) | `feat/fxcap-sweep` @ `194b97a1` | final-SHA run; SWEEP-CAUSES classifies the 23 uncaused snaps in the ledger (`--rescore`), else the PR states them as uncaused |
| P2-RECEIPT | 🟡 | final-SHA gates ✓ (batch 43 `cdad7609`), CI, SM1, sweep vs `c1db9191` (run and reported), seam review of the eleven units at the final SHA (split by flow; names the post-closure commits C5i `76776c21`, A4i `59304742`, C4l `02480285`), frames read, public-suitability scan, COMPAT ✓ (batch 25), RED/GREEN (X1 paste-suffix receipt; fx upgrade guard `d91a9968`) | — | running at the final SHA |
| SHIP | 🚪 | PR → `main` → preview + tap → real-profile run (sessions backed up; MCP servers checked for inherited variables they lose under SEC-C1), label "Phase 4 integration preview — fx c1db919 baseline, before the 0.0.16 transition" | — | after P2-RECEIPT |

### After P2 (own PRs to `main`)

| ID | Status | Scope | Branch | Next |
|---|---|---|---|---|
| C7-r2 | 🟡 | `help_entry` on every row, `/help` search as fx | `feat/tui-phase4-alias` @ `8d83f961` | PR after SHIP |
| D4e | 🟡 | collapse off/on re-projects the visible screen as fx | `fix/tui-collapse-expand` @ `65c4425e` | PR after SHIP |
| X1-r2 | 🟡 | paste under key-swallowing menus; question freeform bound | `fix/tui-paste-suffix` | report → PR after SHIP |
| P1-r3r4 | 🟡 | `! auth:`/`✗ auth:`; held prompt resent once a credential arrives (never without the user's confirmation, or exactly as fx) | `feat/tui-phase4-provider-session` | report → PR after SHIP |
| S1-vis | 🟡 | the `skill` tool's context notices; the `/skills` usage line still names `url` | `feat/tui-phase4-skills` | after S1-sec |
| C8 | 🟡 | `/shell reload` — new execution surface, own security review | `feat/tui-phase4-shell-reload` | report → PR after SHIP |
| K-resize | 🟡 | a settled resize resets and replays the kept document at the new width as fx (1 MiB log); 13 capture screens identical | `fix/tui-resize-stale-row` @ `cba68785` | external review → PR after SHIP; conflicts with D4e (`Op::Block`, collapse) — whichever lands second rebases; flip `ssot.md:127` with it |
| K-jobctl | 🟡 | Ctrl-Z suspends as fx (byte 26, restore, raise; resume resets and replays); outside SIGTSTP/SIGCONT unhandled as fx; smoke 1104/1104 | `fix/tui-job-control` @ `2d387ced` | external review → PR after SHIP, **after K-resize**: switch its redraw to K-resize's `Transcript::replay` (closes its band-only repaint after a resize/Ctrl-L) |
| K-recover | 🟡 | a cut stream or refused connection recovers as fx (`decide` port; wait-for-connection in the TUI, Esc pauses; restart notice) — 3 capture pairs match but the elapsed seconds; tool call never re-run (test) | `fix/tui-stream-recovery` @ `0723d4af` | external review (re-sends a possibly-sent request in `ask`/line shell as fx; paused turn logged `Cancelled` where fx keeps a checkpoint; recovered spend marked unaccounted) → PR after SHIP; rebase over SEC-D1 (`gateway/mod.rs`) |
| K-mcpwords | 🟡 | a connecting server shows `Reloading` (fx `Connecting`); startup failure `* mcp:` (fx `! mcp:`); details `Error` sentences and `?` counts — 13/13 captures match | `fix/mcp-status-words` @ `4ec576f1` | PR after SHIP |
| SEC-SHOULD | 🔴 | the security reviewers' after-merge items: round 1 (A: allowlist silence, grant-key hashing, MCP grants by server+tool; B: checker C1, refusal tests, `terminal_safe`, `fail()`, title cap; C: in-flight launches at shutdown, group signals before reap, HTTP MCP cancellation, signal-death cleanup, shared skill roots, osascript limit; D: bill lookup bound to origin, title-model doc) plus every SHOULD in zbrain `p2-sec/round{2..8}/STATUS.md` and the residuals recorded in `docs/parity.md` | — | |
| SWEEP-DEFECTS | 🔴 | sweep r2 class (d): Sound `max` 45 (`sound.rs:37`), approval tool-schema 17 (`approval_view.rs:685`), session-record 9 (`shell.rs:32298`), provider label 9 (`provider/mod.rs:116`), `/stats` bytes 9 (`shell.rs:1436`), approval scroll 2, allowlist band 2, activity-row gap 2 | — | one unit each after SHIP |
| PARITY-MCP-VARS | 🔴 | `${}` vs fx after SEC-C2c (invalid names and unclosed `${` now refused as fx): `:-` default only when unset, fx expands fewer fields (command/args/env/header values after parsing), no 1 MiB cap; fx `ask --resume` restores the recorded provider (xfx builds it first); typed `trust approve <name>` can approve an expansion-only entry unseen (product call) | — | after SHIP |
| DOCS | 🚪 | README counts, parity header counts and pin, `03-tui-port.md`, `06-qa-harness.md`, `UPSTREAM.md`, D4d receipt stale lines | — | with SHIP |

## Open gaps

| ID | Status | Gap | Next |
|---|---|---|---|
| G-copy-flake | 🔴 | `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` fails at load ≥ 60 (5/7, T9) | watch in the batch gates; RCA unit on recurrence |
| G-caret | 🔴 | the hardware cursor stays visible under the reversed caret (F3) | check in the sweep |
| G-decisions | 🚪 | user decisions: Codex/Grok sign-in (#88), MCP OAuth and `/mcp auth|logout`, other agents' skill roots, network skill install, the provider-switch reasoning notice, the OSC 8 scheme allowlist, the `ask_user_question` >4096-byte saved-rule exemption (R1; kept fail-closed), the approval "always" scope (xfx per-tool-per-file + `2 = …` row vs fx's workspace file access; sweep 46 snaps), the second-model image route (fx sends images to `google/gemini-2.5-flash`; xfx refuses) | asked batched at the blocking point (program § Decisions) |

## Round log (current)

- 2026-11-03: batches 14–22 landed (T8 r2, D2c r4–r6, W9 r5, M1, P1 r2, S1 r2, X1 r1, C7 r1, R1 r2, H1); fx self-upgrade guard d91a9968; program adopted. Public-suitability scan of plan's files and `origin/main..plan` history: no personal names, user paths or real keys (tests use `/Users/me`, `/Users/u`; the two `sk-` hits are fake test strings, shortened in 56f20fcc); harness scratch paths (`/private/tmp/claude-501/fxcap.*`) remain in some receipts, no personal data.
- 2026-11-03: P2 security review, four groups × two engines (gpt-6-astra, opus) at `de6618de`: A REJECT / APPROVE-WITH-FIXES, B APPROVE-WITH-FIXES ×2, C REJECT / APPROVE-WITH-FIXES, D REJECT / APPROVE-WITH-FIXES. The union of MUST-FIX items = 11 units (SEC-*), dispatched one per worker; plausible-tier items reproduce before they are fixed. The reports land in `receipts/p2-sec/` with the fixes. Sweep tooling ran once (`1a94cdff`, 161 scenarios, 11 min at 10 jobs): 624/861 pass. Program: the Phase 4 sweep is run and reported, not passed (§P2 Receipt).
