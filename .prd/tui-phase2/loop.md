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
| 4 | `/setup` provider switching, `/model` catalog, context meter | 18–19 | implemented; controller gates + 51/51 mutations + dual-persona review green | this branch / one agent |
| 5 | Prompt history + draft capture | 17 | planned | this branch / one agent |
| 6 | Paste entity atomicity/span shift/history renumber/transaction boundary + tab unit | 21 | planned | this branch / one agent |
| 7 | Alt-screen file-diff approval + QA/carrier closure; kitty/tmux reconciliation | 20 and all 1–21 | planned | this branch / one agent |

## Gate contract

Each implementation round runs sequentially in one target directory:

```bash
E=${XFX_EVIDENCE:?set XFX_EVIDENCE outside the worktree}
cargo fmt --check > "$E/fmt.log" 2>&1 && echo FMT-OK &&
cargo clippy --locked --all-targets -- -D warnings > "$E/clippy.log" 2>&1 && echo CLIPPY-OK &&
cargo clippy --locked --all-targets --features fault-injection -- -D warnings > "$E/clippy-fault.log" 2>&1 && echo CLIPPY-FAULT-OK &&
cargo test --locked --all-targets > "$E/default.log" 2>&1 && echo DEFAULT-OK &&
cargo test --locked --features fault-injection --test tui > "$E/fault-tui.log" 2>&1 && echo FAULT-TUI-OK &&
cargo test --locked --lib --features fault-injection > "$E/fault-lib.log" 2>&1 && echo FAULT-LIB-OK &&
./scripts/check-no-stubs.sh > "$E/no-stubs.log" 2>&1 && echo NO-STUBS-OK &&
./scripts/check-no-secrets.sh > "$E/no-secrets.log" 2>&1 && echo NO-SECRETS-OK &&
./scripts/check-xfx-identity.sh > "$E/identity.log" 2>&1 && echo IDENTITY-OK &&
./scripts/check-preview-contract.sh > "$E/preview-contract.log" 2>&1 && echo PREVIEW-CONTRACT-OK
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
- Round 1 (2026-08-26): plan tip `8cbd45c454414135222a99dadf3fc2d4efac41c2`. Dual-engine plan review is final — Fable/strategist: APPROVE, MUST-FIX none; GPT-5.6: APPROVE, MUST-FIX none. The plan gate is closed and WU 1 may begin. Receipts re-observed directly in this same tree: `cargo test --locked --all-targets` 1471 passed / 0 failed (machine-summed across 11 test binaries), `cargo test --locked --features fault-injection --test tui` 53 passed / 0 failed, `cargo test --locked --lib --features fault-injection` 859 passed / 0 failed; the YAML parse check and the doc gates also passed.
- Round 2 (2026-08-26): WU 1 cell diff/no-op/title completed on base `7650f78285be0c27956b1005b562fdbe293df2d2`. Direct controller gate: fmt, both clippies, all four contract scripts, release/fault builds exited 0; default 1502 (+31), fault integration 55 (+2), fault lib 888 (+29). PTY smoke repeated 3/3 at `16 scenarios + the oracle, 313 checks, 0 failures`; M1–M22 all killed with 0 survivors and byte-identical restoration. First task review found two blocking defects (external-damage tails survived; stop/resume lost OSC 2); Fix Round 1 added RED reproductions and closed both. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none; comment-only follow-up review also `release-ready — MUST-FIX none`. Scenario 13 compares the painter-owned band cell/attribute/caret state exactly and terminal-owned document text in order across native scrollback; absolute document row is intentionally not asserted because Phase-1 append timing moves `band_top` at turn completion.
- Round 3 (2026-08-27): WU 2 resize/reflow/OSC containment completed on base `d09428dd69d52703562012f16b5c3a964fc1daf9`. Direct controller gate: fmt, both clippies, four contract scripts, release/fault builds exited 0; default 1558 (+56 from WU1), fault integration 64 (+9), fault lib 935 (+47). PTY smoke repeated 3/3 at `17 scenarios + the oracle, 334 checks, 0 failures`; M1–M40 all killed with 0 survivors and byte-identical restoration. First review found malformed-OSC phantom Escape, exit-during-debounce tail loss, and a column-1-only CUP detector; Fix Round 1 reproduced and closed all three. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none; comment/report cleanup review also `release-ready — MUST-FIX none`. The task file list was narrowly expanded to `term.rs` and `mod.rs` because the explicit post-launch `0x0` contract was unreachable through the launch fallback reader.
- Round 4 (2026-08-27): WU 3 shared slash registry/router/picker completed on base `e744bfca463d52325697d1d0be95282273cb9d6b`. Direct controller gate: all static/contract/build gates exited 0; default 1587 (+29 from WU2), fault integration 65 (+1), fault lib 963 (+28). PTY smoke repeated 3/3 at `18 scenarios + the oracle, 364 checks, 0 failures`; unified mutation runner: 32 definitions, 31 killed + M21 equivalent with a driven premise proof, 117-file byte-identical restoration. First review found false router-uniqueness comments and an unpinned minimum-screen+activity+menu invariant; Fix Round 1 closed both. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none. `/exit` remains metadata on the single implemented `/quit` row, preserving the canonical six names.
- Round 1 executor notes (non-blocking review remarks; they do not widen any WU's scope):
  1. The WU 5 draft capture/restore test must read the `EntitySnapshot` fields, so the entity roundtrip is actually proven instead of leaving the fields `dead_code`.
  2. WU 7 checkpoint ownership must name the remaining `src/tui/shell.rs` changes at execution time rather than at plan time.
  3. The WU 6 timing receipt names reference this machine; the portable operation-counter gate remains the authoritative one.

## Round-0 rulings

- Resize PTY receipt sends `SIGWINCH` explicitly after `Pty::resize`. The harness intentionally does not give the child a controlling terminal, so `tcsetwinsize` cannot be trusted to deliver the foreground-process-group signal on every kernel. The product still receives and handles a real signal.
- Scenario 15's "content reflows" means the owned band and the unfinished transcript tail. Rows already committed to native scrollback belong to the terminal and reflow under that terminal's own policy; xfx does not repaint them or introduce a transcript viewport in Phase 2.
- A zero-by-zero winsize is "no new information" after launch, not a request to reflow to the startup fallback 24×80. Launch may still use the fallback.

## Carrier reconciliation spine

This table is the terminal checklist for issue #19. It must have no `open` row before merge.

| Carrier item | Canonical owner | Initial disposition | Evidence required |
|---|---|---|---|
| cell diff / no-op frame skip | WU 1 | implemented | QA 13–14: controller PTY 3/3, 313 checks; 22/22 mutants; dual-persona release-ready |
| resize / SIGWINCH / unfinished-tail rewrap | WU 2 | implemented | QA 15: controller PTY 3/3, 334 checks; 40/40 mutants; dual-persona release-ready |
| slash picker | WU 3 | implemented | QA 16: controller PTY 3/3, 364 checks; 31 killed + 1 equivalent; dual-persona release-ready |
| provider switching / model catalog / context meter | WU 4 | implement | QA 18–19 + fixture-discriminated real pty |
| prompt history | WU 5 | implement | QA 17 |
| alt-screen file-diff approval | WU 7A–7B | implement | QA 20 + atomic primary restore |
| paste entity, 64-cap prefix scan, history renumber, transaction boundary | WU 6 | implement | QA 21 + budget/mutation receipts |
| OSC 2 title | WU 1 | implemented | sanitized OSC 2 + stop/resume re-arm + balanced title-stack PTY receipt |
| kitty/tmux | WU 7C | re-defer full CSI-u matrix; existing push/pop/tmux branch reclassified implemented | existing constants/tests + explicit rationale |
| tab visible/sent divergence | WU 6 | implement as a visible editor unit | deterministic grid+provider assertion |
| foreign OSC becomes composer text | WU 2 | implemented decoder containment | OSC 10/11/52 + malformed/control handback receipts; phantom-Escape fix reviewed |
| context meter / usage plumbing | WU 4 | implement for catalog providers; omit meter when either fact absent | shared event + rendered meter |
| activity-row colour | WU 1 | re-defer to successor issue #20: no semantic palette role in Phase-2 spec | promote when a palette role is named and a cell-attribute test is supplied |
| give-up reason cannot print on refusing screen | WU 7C | re-defer to successor issue #20: no independent delivery surface in Phase 2 | promote when a non-screen diagnostic sink is designed and fault-injection proves it |
| vt-grid grapheme / OSC / SGR oracle breadth | WU 1 and WU 7B | emitted subset implemented in WU 1; alt-plane support remains WU 7B | 29 oracle falsification claims + WU 7B alt-plane receipt |
| Transcript fixed columns / wrap memoization | WU 2 / WU 1 | unfinished-tail rewrap implemented; memoization re-deferred to successor issue #20 | promote if WU1 benchmark exceeds 8ms/frame at 80×24 or 32ms/frame at 300×200 |
| `xfx ask` grant recording unpinned | WU 7C | implement PTY fixture | grant event + resume receipt |
| Ctrl-C notice wording | WU 7C | implement both surfaces together | TUI + line-shell assertions |
| terminal-event full-channel third arm | WU 7C | implement deterministic fault-injection case | DRAIN_DEADLINE receipt |
| parity `/model` shared-validation sentence | WU 4 | implement docs | parity diff |

## Build facts (Round 0, directly observed)

- `cargo fmt --check` → exit 0
- default clippy and `fault-injection` clippy with `-D warnings` → exit 0
- `cargo test --locked --all-targets` → 1471 passed, 0 failed
- `cargo test --locked --features fault-injection --test tui` → 53 passed, 0 failed
- `cargo test --locked --lib --features fault-injection` → 859 passed, 0 failed
- all four repository contract scripts → exit 0

This is the baseline for WU deltas, and it is three labeled counts, not two: default 1471, fault integration 53, fault lib units 859. Counts are machine-summed from the command output; later rounds state their delta from this observation rather than using it as a hard-coded gate.
- The canonical list is six names through WU 3 and seven after WU 4 adds `/setup`; `SLASH_REGISTRY` lives beside it and an agreement test mechanically prevents drift. `/exit` is a registry alias for `Quit`, not a seventh canonical command; it supplies a real alias-ranking path for QA scenario 16.
- The inline picker reuses the band's elastic panel slot but does not take the caret. Approval is a question and owns focus; completion is composer state and the composer remains caret owner.
- The Phase-2 paste "undo boundary" means the paste is recorded as one editor transaction and is pinned by a cargo-level boundary test. User-facing undo/redo bindings and the bounded undo stack remain Phase 3 item 18. QA scenario 21 keeps atomic move/delete/history renumber on the real PTY; its undo clause is reconciled to the cargo receipt in `.prd/06-qa-harness.md` in WU 6.
- Alt-screen approval preserves the existing 160-byte one-line summary for the inline panel and adds a separate inert `ApprovalDiff` payload for screen review. Each side is bounded to a literal 64 KiB and control characters are escaped before the approval channel; the full file is never transported as prompt text.
- WU 4 separates UI choice from runtime mutation. The shell emits provider/catalog work; the worker owns async catalog fetch, provider/config replacement, conversation reset and profile persistence. Catalog entries cross `UiEvent::made_inert` before rendering. The same catalog event supplies the context-window denominator; completed-turn usage supplies the numerator.
- Alt-screen ownership is a distinct owner enum, not another meaning of `Option<Panel>`. Returning to the primary screen emits `1049l` and the complete primary repaint in one `write_all`; no intermediate blank frame is allowed. Normal, panic and signal restoration all account for the owner state, while the main TUI surface still never takes the alternate screen.
- The seven WUs are sequential, not parallel: `src/tui/shell.rs` is a shared integration seam for WU 2–6. Use a fresh implementer per WU, but never more than one writer at a time. A rejected WU resumes its original implementer and reviewer.

- WU 7 is one carrier but three bounded review checkpoints: **7A** approval payload + owner state, **7B** alternate renderer/oracle/scenario 20, **7C** carrier hardening + all-scenario convergence. Each checkpoint has its own RED/GREEN, mutation report and unanimous scoped review before the next starts.

- Every re-deferred row is copied with its threshold and final evidence link to successor carrier https://github.com/2lab-ai/xfx/issues/20 before issue #19 closes.
