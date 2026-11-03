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
| SEC-A1 | 🟡 | TTY approval prompt escapes every field | `fix/p2sec-tty-escape` | unit running |
| SEC-A2 | 🟡 | a failed `/allowlist` view keeps the table in force; an unreadable table at launch refuses every tool (fail closed) and config diagnostics are said at launch, as fx | `fix/p2sec-malformed-table` @ `1754afde` | batch 25 |
| SEC-A4 | 🟡 | default-ignorable code points spelled out in approval commands | `fix/p2sec-ignorable-cp` | unit running |
| SEC-B1 | 🟡 | the resumed log's model id passes `model_id_problem` before it is painted | `fix/p2sec-resume-model-inert` | unit running |
| SEC-C1 | 🟡 | MCP stdio env = fx 0.0.15 allowlist + the server's `env`; Docker cleanup the same; other children lose xfx's credentials | `fix/p2sec-mcp-env` | unit running |
| SEC-C2 | 🟡 | workspace MCP entries cannot reference xfx's credentials | `fix/p2sec-mcp-ws-cred` | unit running |
| SEC-C3 | 🟡 | a stdio server that stops reading cannot block request timeout or close | `fix/p2sec-mcp-stdin` | unit running |
| SEC-C4 | 🟡 | MCP trust bound to the entry's content; the prompt shows the command or URL | `fix/p2sec-mcp-approval-bind` | unit running (repro first) |
| SEC-C5 | 🟡 | writes to execution-config paths always ask | `fix/p2sec-exec-config-writes` | unit running (repro first) |
| SEC-D2 | 🟡 | pasted/typed paths and `/image` obey the read table's denies (every mode); stat before open, non-regular files refused | `fix/p2sec-paste-image-read` @ `96e8b7b1` | batch 25 |
| SWEEP | 🟡 | local sweep + ledger + scoreboard + closed denominator; first run at `1a94cdff`: pass 624/861 (72.5%), 169 unresolved | `feat/fxcap-sweep` @ `ccaa62b8` | ledger conformance + cascade column + root-cause table → merge |
| COMPAT | 🟡 | session-log back-compat vs the installed preview: nothing rewritten in place; opening an old session appends one `preferences_changed` | `test/session-compat` @ `9956e7a8` | merge with the next batch |
| P2-RECEIPT | 🔴 | final-SHA gates, CI, SM1, sweep vs `c1db9191` (run and reported), security fixes re-reviewed, frames read, public-suitability scan, COMPAT, RED/GREEN | — | after the SEC units, SWEEP, SM1 |
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
| K-jobctl | 🟡 | Ctrl-Z ignored (fx suspends); external SIGTSTP restores before stopping (fx writes nothing) | `fix/tui-job-control` | unit running |
| K-recover | 🟡 | a cut stream or refused connection recovers as fx (`decide` port; wait-for-connection in the TUI, Esc pauses; restart notice) — 3 capture pairs match but the elapsed seconds; tool call never re-run (test) | `fix/tui-stream-recovery` @ `0723d4af` | external review (re-sends a possibly-sent request in `ask`/line shell as fx; paused turn logged `Cancelled` where fx keeps a checkpoint; recovered spend marked unaccounted) → PR after SHIP; rebase over SEC-D1 (`gateway/mod.rs`) |
| K-mcpwords | 🟡 | a connecting server shows `Reloading` (fx `Connecting`); startup failure `* mcp:` (fx `! mcp:`); details `Error` sentences and `?` counts — 13/13 captures match | `fix/mcp-status-words` @ `4ec576f1` | PR after SHIP |
| SEC-SHOULD | 🔴 | the security reviewers' after-merge items (A: allowlist silence, grant-key hashing, MCP grants by server+tool; B: checker C1, refusal tests, `terminal_safe`, `fail()`, title cap; C: in-flight launches at shutdown, group signals before reap, HTTP MCP cancellation, signal-death cleanup, shared skill roots, osascript limit; D: bill lookup bound to origin, title-model doc) | — | after SHIP, one PR each |
| DOCS | 🚪 | README counts, parity header counts and pin, `03-tui-port.md`, `06-qa-harness.md`, `UPSTREAM.md`, D4d receipt stale lines | — | with SHIP |

## Open gaps

| ID | Status | Gap | Next |
|---|---|---|---|
| G-copy-flake | 🔴 | `tui::copy::tests::an_ending_session_lets_a_copy_that_answers_land` fails at load ≥ 60 (5/7, T9) | watch in the batch gates; RCA unit on recurrence |
| G-resume-prefs | 🔴 | opening a session in the TUI writes the launch's model and permission mode over the session's (COMPAT: `ask` → `auto`); check fx c1db9191's resume | triage with the sweep |
| G-caret | 🔴 | the hardware cursor stays visible under the reversed caret (F3) | check in the sweep |
| G-decisions | 🚪 | user decisions: Codex/Grok sign-in (#88), MCP OAuth and `/mcp auth|logout`, other agents' skill roots, network skill install, the provider-switch reasoning notice, the OSC 8 scheme allowlist, the `ask_user_question` >4096-byte saved-rule exemption (R1; kept fail-closed) | asked batched at the blocking point (program § Decisions) |

## Round log (current)

- 2026-11-03: batches 14–22 landed (T8 r2, D2c r4–r6, W9 r5, M1, P1 r2, S1 r2, X1 r1, C7 r1, R1 r2, H1); fx self-upgrade guard d91a9968; program adopted. Public-suitability scan of plan's files and `origin/main..plan` history: no personal names, user paths or real keys (tests use `/Users/me`, `/Users/u`; the two `sk-` hits are fake test strings, shortened in 56f20fcc); harness scratch paths (`/private/tmp/claude-501/fxcap.*`) remain in some receipts, no personal data.
- 2026-11-03: P2 security review, four groups × two engines (gpt-6-astra, opus) at `de6618de`: A REJECT / APPROVE-WITH-FIXES, B APPROVE-WITH-FIXES ×2, C REJECT / APPROVE-WITH-FIXES, D REJECT / APPROVE-WITH-FIXES. The union of MUST-FIX items = 11 units (SEC-*), dispatched one per worker; plausible-tier items reproduce before they are fixed. The reports land in `receipts/p2-sec/` with the fixes. Sweep tooling ran once (`1a94cdff`, 161 scenarios, 11 min at 10 jobs): 624/861 pass. Program: the Phase 4 sweep is run and reported, not passed (§P2 Receipt).
