# TUI Phase 2 — convergence loop

Status: in-progress
Date: 2026-08-26
Branch: `plan/tui-phase2`
Worktree: `.worktrees/plan-tui-phase2`
Base: main @ f1913a6be24f

> Build facts and coordinates are observations from the stamped base. Revalidate after every commit.

## Driver

Seven vertical slices execute in order. Each slice owns one product contract, its cargo tests, its smoke scenario(s), its parity update and its mutation report. One implementation agent writes; the coordinator reruns the gate and a fresh dual-persona reviewer judges the slice. A rejected slice loops with the same implementer and reviewer until unanimous release-ready.

## Work units

| WU | Phase-2 contract | QA scenarios | Status | Branch / writer |
|---|---|---|---|---|
| 1 | Oracle widening, shadow grid/cell diff/no-op skip, OSC 2 title | 13–14 | planned | this branch / one agent |
| 2 | Resize debounce/reflow/full repaint + keyboard OSC containment | 15 | planned | this branch / one agent |
| 3 | Slash registry/router/inline picker | 16 | planned | this branch / one agent |
| 4 | `/setup` provider switching, `/model` catalog, context meter | 18–19 | planned | this branch / one agent |
| 5 | Prompt history + draft capture | 17 | planned | this branch / one agent |
| 6 | Paste entity atomicity/span shift/history renumber/transaction boundary + tab unit | 21 | planned | this branch / one agent |
| 7 | Alt-screen file-diff approval + QA/carrier closure; kitty/tmux reconciliation | 20 and all 1–21 | planned | this branch / one agent |

## Gate contract

Each implementation round runs sequentially in one target directory:

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --features fault-injection -- -D warnings
cargo test --locked --all-targets
cargo test --locked --features fault-injection --test tui
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/check-xfx-identity.sh
./scripts/check-preview-contract.sh
```

Slice 7 and final merge qualification additionally run both release builds, `scripts/smoke.sh` and the expanded `scripts/smoke-tui.sh`. CI proves all native target rows. Local receipt numbers are discovered from the current tree and recorded as observations, never copied as expectations.

## Gap matrix

| Gap | Source | Owner | Terminal disposition |
|---|---|---|---|
| Items 11–17 | `.prd/03-tui-port.md` Phase 2 | WU 1–7 | implementation + scenario |
| Scenarios 13–21 | `.prd/06-qa-harness.md` | WU 1–7 | green evidence |
| Issue #19 inherited queue | issue #19 | matching WU or WU 7 | implemented or explicit re-deferral |
| Phase-2 docs/status | `docs/parity.md`, numbered `.prd` docs | every WU + WU 7 | current-tense contract |
| Preview/live receipt | goal acceptance | WU 7 / coordinator | merge → preview → brew → captured real session |

## Round log

- Round 0 (2026-08-26): Phase-1 shipped state verified at main f1913a6; issue #19 is open; no Phase-2 worktree existed; the seven canonical product items and nine QA scenarios were re-read from primary docs. Interface mapping and implementation-plan authoring started.

## Round-0 rulings

- Resize PTY receipt sends `SIGWINCH` explicitly after `Pty::resize`. The harness intentionally does not give the child a controlling terminal, so `tcsetwinsize` cannot be trusted to deliver the foreground-process-group signal on every kernel. The product still receives and handles a real signal.
- Scenario 15's "content reflows" means the owned band and the unfinished transcript tail. Rows already committed to native scrollback belong to the terminal and reflow under that terminal's own policy; xfx does not repaint them or introduce a transcript viewport in Phase 2.
- A zero-by-zero winsize is "no new information" after launch, not a request to reflow to the startup fallback 24×80. Launch may still use the fallback.

## Carrier reconciliation spine

This table is the terminal checklist for issue #19. It must have no `open` row before merge.

| Carrier item | Canonical owner | Initial disposition | Evidence required |
|---|---|---|---|
| cell diff / no-op frame skip | WU 1 | implement | QA 13–14 + byte/grid equivalence |
| resize / SIGWINCH / unfinished-tail rewrap | WU 2 | implement | QA 15 on macOS + Linux |
| slash picker / provider switching / model catalog | WU 3 | implement to QA 16, 18, 19 | fixture-discriminated real pty |
| prompt history | WU 4 | implement | QA 17 |
| alt-screen file-diff approval | WU 5 | implement | QA 20 + atomic primary restore |
| paste entity, 64-cap prefix scan, history renumber, undo boundary | WU 6 | implement | QA 21 + budget/mutation receipts |
| OSC 2 + kitty/tmux | WU 7 | implement | terminal-byte matrix |
| tab visible/sent divergence | WU 6 | decide with paste/editor entity model | deterministic grid+provider assertion |
| foreign OSC becomes composer text | WU 7 | implement decoder containment or explicitly re-defer | mixed-stream pty receipt or issue rationale |
| context meter / usage plumbing | WU 3 | implement if QA/spec source supplies denominator; otherwise explicit re-defer | shared event + rendered meter, or evidenced absence of denominator |
| activity-row colour | WU 1 or 7 | implement only with a semantic palette role; otherwise explicit re-defer | cell-attribute assertion or rationale |
| give-up reason cannot print on refusing screen | WU 7 | explicit re-defer unless a non-screen delivery surface is in scope | issue rationale + exit/log receipt |
| vt-grid grapheme / OSC / SGR oracle breadth | WU 1 and 7 | implement enough to validate emitted subset | oracle falsification receipts |
| Transcript fixed columns / wrap memoization | WU 2 / WU 1 | implement unfinished-tail rewrap; memoize only if measured budget fails | resize + performance receipt |
| `xfx ask` grant recording unpinned | WU 7 | implement pty fixture if feasible | grant event + resume receipt |
| Ctrl-C notice wording | WU 7 | implement both surfaces together | TUI + line-shell assertions |
| terminal-event full-channel third arm | WU 7 | implement deterministic fault-injection case | DRAIN_DEADLINE receipt |
| parity `/model` shared-validation sentence | WU 3 | implement docs | parity diff |

## Build facts (Round 0, directly observed)

- `cargo fmt --check` → exit 0
- default clippy and `fault-injection` clippy with `-D warnings` → exit 0
- `cargo test --locked --all-targets` → 1471 passed, 0 failed
- `cargo test --locked --features fault-injection --test tui` → 53 passed, 0 failed
- all four repository contract scripts → exit 0

This is the baseline for WU deltas. Counts are machine-summed from the command output; later rounds state their delta from this observation rather than using it as a hard-coded gate.
- The canonical six names remain `interactive::SLASH_COMMANDS`; `SLASH_REGISTRY` lives beside it and an agreement test mechanically prevents drift. `/exit` is a registry alias for `Quit`, not a seventh canonical command; it supplies a real alias-ranking path for QA scenario 16.
- The inline picker reuses the band's elastic panel slot but does not take the caret. Approval is a question and owns focus; completion is composer state and the composer remains caret owner.
- The Phase-2 paste "undo boundary" means the paste is recorded as one editor transaction and is pinned by a cargo-level boundary test. User-facing undo/redo bindings and the bounded undo stack remain Phase 3 item 18. QA scenario 21 keeps atomic move/delete/history renumber on the real PTY; its undo clause is reconciled to the cargo receipt in `.prd/06-qa-harness.md` in WU 6.
- Alt-screen approval preserves the existing 160-byte one-line summary for the inline panel and adds a separate inert `ApprovalDiff` payload for screen review. Each side is bounded to a literal 64 KiB and control characters are escaped before the approval channel; the full file is never transported as prompt text.
- WU 4 separates UI choice from runtime mutation. The shell emits provider/catalog work; the worker owns async catalog fetch, provider/config replacement, conversation reset and profile persistence. Catalog entries cross `UiEvent::made_inert` before rendering. The same catalog event supplies the context-window denominator; completed-turn usage supplies the numerator.
- Alt-screen ownership is a distinct owner enum, not another meaning of `Option<Panel>`. Returning to the primary screen emits `1049l` and the complete primary repaint in one `write_all`; no intermediate blank frame is allowed. Normal, panic and signal restoration all account for the owner state, while the main TUI surface still never takes the alternate screen.
- The seven WUs are sequential, not parallel: `src/tui/shell.rs` is a shared integration seam for WU 2–6. Use a fresh implementer per WU, but never more than one writer at a time. A rejected WU resumes its original implementer and reviewer.
