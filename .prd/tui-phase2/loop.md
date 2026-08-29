# TUI Phase 2 — convergence loop

Status: shipped
Date: 2026-08-26 (shipped 2026-08-29)
Branch: `plan/tui-phase2`
Worktree: `.worktrees/plan-tui-phase2`
Base: main @ f1913a6be24f

> Build facts and coordinates are observations from the stamped base. Revalidate after every commit.

## Driver

Seven vertical slices execute in order. Each slice owns one product contract, its cargo tests, its smoke scenario(s), its parity update and its mutation report. One implementation agent writes; the coordinator reruns the gate and a fresh dual-persona reviewer judges the slice. A rejected slice loops with the same implementer and reviewer until unanimous release-ready.

## Work units

| WU | Phase-2 contract | QA scenarios | Status | Branch / writer |
|---|---|---|---|---|
| 1 | Oracle widening, shadow grid/cell diff/no-op skip, OSC 2 title | 13–14 | implemented; controller gates + 22/22 mutations + dual-persona review green (after fix round 1 closed two blocking defects) | this branch / one agent |
| 2 | Resize debounce/reflow/full repaint + keyboard OSC containment | 15 | implemented; controller gates + 40/40 mutations + dual-persona review green (after fix round 1 reproduced and closed three findings) | this branch / one agent |
| 3 | Slash registry/router/inline picker | 16 | implemented; controller gates + 31 killed/1 equivalent mutations + dual-persona review green (after fix round 1 closed two findings) | this branch / one agent |
| 4 | `/setup` provider switching, `/model` catalog, context meter | 18–19 | implemented; controller gates + 51/51 mutations + dual-persona review green | this branch / one agent |
| 5 | Prompt history + draft capture | 17 | implemented; controller gates + 41 killed/1 equivalent mutations + dual-persona review green | this branch / one agent |
| 6 | Paste entity atomicity/span shift/history renumber/transaction boundary + tab unit | 21 | implemented; controller gates + 58 killed/3 equivalent mutations + dual-persona review green | this branch / one agent |
| 7 | Alt-screen file-diff approval + QA/carrier closure; kitty/tmux reconciliation | 20 and all 1–21 | 7A implemented (32/32 mutations + scoped review green); 7B implemented (61/61 mutations + scoped review green, after its fix rounds); 7C implemented — carrier hardening, issue-#19 closure and the Phase-2 documents as current contracts, 19/19 mutations and the direct gate green. Every checkpoint's scoped panel, the full-branch panel and both test-only hotfix panels are unanimous approve, MUST-FIX none; no review receipt on this row is outstanding | this branch / one agent per checkpoint |

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
| Preview/live receipt | goal acceptance | WU 7 / coordinator | **observed**: merge `09fa056` + `c5cbad56` → preview run 33269934844 → tap `4e7f5b19` → brew `2026.08.29.190455.33269934844.1` → a real `XFX_TUI=1 xfx` session answering with needle `582e266d66bc2d59f9dc4b30`, exit 0 (receipts in `ssot.md` §Delivery) |

## Round log

- Round 0 (2026-08-26): Phase-1 shipped state verified at main f1913a6; issue #19 is open; no Phase-2 worktree existed; the seven canonical product items and nine QA scenarios were re-read from primary docs. Interface mapping and implementation-plan authoring started.
- Round 1 (2026-08-26): plan tip `8cbd45c454414135222a99dadf3fc2d4efac41c2`. Dual-engine plan review is final — Fable/strategist: APPROVE, MUST-FIX none; GPT-5.6: APPROVE, MUST-FIX none. The plan gate is closed and WU 1 may begin. Receipts re-observed directly in this same tree: `cargo test --locked --all-targets` 1471 passed / 0 failed (machine-summed across 11 test binaries), `cargo test --locked --features fault-injection --test tui` 53 passed / 0 failed, `cargo test --locked --lib --features fault-injection` 859 passed / 0 failed; the YAML parse check and the doc gates also passed.
- Round 2 (2026-08-26): WU 1 cell diff/no-op/title completed on base `7650f78285be0c27956b1005b562fdbe293df2d2`. Direct controller gate: fmt, both clippies, all four contract scripts, release/fault builds exited 0; default 1502 (+31), fault integration 55 (+2), fault lib 888 (+29). PTY smoke repeated 3/3 at `16 scenarios + the oracle, 313 checks, 0 failures`; M1–M22 all killed with 0 survivors and byte-identical restoration. First task review found two blocking defects (external-damage tails survived; stop/resume lost OSC 2); Fix Round 1 added RED reproductions and closed both. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none; comment-only follow-up review also `release-ready — MUST-FIX none`. Scenario 13 compares the painter-owned band cell/attribute/caret state exactly and terminal-owned document text in order across native scrollback; absolute document row is intentionally not asserted because Phase-1 append timing moves `band_top` at turn completion.
- Round 3 (2026-08-27): WU 2 resize/reflow/OSC containment completed on base `d09428dd69d52703562012f16b5c3a964fc1daf9`. Direct controller gate: fmt, both clippies, four contract scripts, release/fault builds exited 0; default 1558 (+56 from WU1), fault integration 64 (+9), fault lib 935 (+47). PTY smoke repeated 3/3 at `17 scenarios + the oracle, 334 checks, 0 failures`; M1–M40 all killed with 0 survivors and byte-identical restoration. First review found malformed-OSC phantom Escape, exit-during-debounce tail loss, and a column-1-only CUP detector; Fix Round 1 reproduced and closed all three. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none; comment/report cleanup review also `release-ready — MUST-FIX none`. The task file list was narrowly expanded to `term.rs` and `mod.rs` because the explicit post-launch `0x0` contract was unreachable through the launch fallback reader.
- Round 4 (2026-08-27): WU 3 shared slash registry/router/picker completed on base `e744bfca463d52325697d1d0be95282273cb9d6b`. Direct controller gate: all static/contract/build gates exited 0; default 1587 (+29 from WU2), fault integration 65 (+1), fault lib 963 (+28). PTY smoke repeated 3/3 at `18 scenarios + the oracle, 364 checks, 0 failures`; unified mutation runner: 32 definitions, 31 killed + M21 equivalent with a driven premise proof, 117-file byte-identical restoration. First review found false router-uniqueness comments and an unpinned minimum-screen+activity+menu invariant; Fix Round 1 closed both. Final dual-persona review: `release-ready` / `release-ready`, merged MUST-FIX none. `/exit` remains metadata on the single implemented `/quit` row, preserving the canonical six names.
- Round 5 (2026-08-29): WU 7 completed in three checkpoints. **7A** (bounded approval payload and owner state) is committed at `19779a7` and **7B** (alternate renderer, widened oracle, scenario 20) at `55ec1ee`, each independently reviewed, with 32/32 and 61/61 mutants. **7C** (carrier hardening and convergence) is this round, on base `55ec1ee`: the four carrier-hardening rows, the issue-#19 closure and the Phase-2 documents as current contracts. Direct gate, run serially in this tree after the sweep: fmt, both clippies, four contract scripts, both release builds and `scripts/smoke.sh` (46 checks) all exited 0; default **1770** (+5 from 7B's 1765), fault integration **77** (+1), fault lib **1138** (+4). `scripts/smoke-tui.sh` repeated 3/3 at `23 scenarios + the oracle, 463 checks, 0 failures`. Mutations: 19 definitions, every one executed, **19 killed, 0 survivors**, with the working tree byte-identical over `git ls-files -co --exclude-standard` (121 files) before and after. Two definitions were corrected rather than dropped when the first sweep answered honestly: one named a case that cannot see it (`turn_ended`'s reset is invisible to the idle column, so the kill belongs to the pre-existing case that holds it) and one was judged by the wrong suite (the line shell's notice write is observable on the pty, not in the lib units).
- Round 5 findings recorded rather than fixed: `docs/parity.md` still claimed the TUI never writes `1049h`, listed no catalog for a bare `/model`, counted six slash commands and called the context meter unpopulated — four statements 7A/7B/WU3/WU4 had made false and which neither checkpoint's file ownership covered; all four are now the shipped contract. `scripts/smoke-tui.sh`'s `--list` branch was unreachable (`argparse` reads a leading `--list` as an unknown option before the branch that answers it), so the scenario registration could not be reconciled mechanically; it is an option now and the runner and the shell list are compared before anything is driven. The `/model` id-shape validation is shared between the two front ends and the **catalog-membership** refusal is not — the line shell refuses an id its catalog does not publish and the TUI accepts it — which is stated in the parity row rather than changed, because the fix is in files this checkpoint does not own.
- Round 6 (2026-08-29): delivery. PR #21 merged the branch at main `09fa056`. Merge CI found two **test-only** harness races that the branch's own runs had not: a wait that returned on the alternate screen's opening bytes instead of on a complete frame (fixed in `075ce7b` — the suite's existing `last_frame` oracle, which reads a frame still open as no frame at all), and a resize case that measured its "and then nothing happened" baseline from before the child could know the screen had changed, counting one complete frame composed for the screen the session still believed in (fixed in `e94af06` — the baseline moved past the observation, and the claim is now provoked with a Ctrl-L that owes a frame). Both landed as PR #22 at main `c5cbad56`; neither changed a product line. Receipts: exact-head PR run **33269357061** all four native targets success; merge CI **33269934848** all four success; preview run **33269934844** success with prerelease tag `preview-2026-08-29-190455-33269934844-1-c5cbad561e62` and tap commit `4e7f5b19`; Homebrew `2026.08.29.190455.33269934844.1` with `xfx status --json` reporting preview revision `c5cbad561e62` and provider llmux on loopback; and a real-config `XFX_TUI=1 xfx` in an empty workspace that selected `/model gpt-5.6-sol` and answered with the computed needle `582e266d66bc2d59f9dc4b30` exactly once, response-only, no cancellation notice, exit 0. Review state: the full-branch panel and both hotfix scoped panels are unanimous approve, MUST-FIX none. Issue #19 closed 2026-08-29; the four re-deferrals reached issue #20 **after** the closure rather than before it, which is recorded as a repaired sequencing miss below rather than as a rule that was met.
- Round 1 executor notes (non-blocking review remarks; they do not widen any WU's scope):
  1. The WU 5 draft capture/restore test must read the `EntitySnapshot` fields, so the entity roundtrip is actually proven instead of leaving the fields `dead_code`.
  2. WU 7 checkpoint ownership must name the remaining `src/tui/shell.rs` changes at execution time rather than at plan time.
  3. The WU 6 timing receipt names reference this machine; the portable operation-counter gate remains the authoritative one.

## Round-0 rulings

- Resize PTY receipt sends `SIGWINCH` explicitly after `Pty::resize`. The harness intentionally does not give the child a controlling terminal, so `tcsetwinsize` cannot be trusted to deliver the foreground-process-group signal on every kernel. The product still receives and handles a real signal.
- Scenario 15's "content reflows" means the owned band and the unfinished transcript tail. Rows already committed to native scrollback belong to the terminal and reflow under that terminal's own policy; xfx does not repaint them or introduce a transcript viewport in Phase 2.
- A zero-by-zero winsize is "no new information" after launch, not a request to reflow to the startup fallback 24×80. Launch may still use the fallback.

## Carrier reconciliation spine

This table is the terminal checklist for issue #19, and it is closed: **every row below is either
`implemented` with the evidence that says so, or `re-deferred` to successor carrier
https://github.com/2lab-ai/xfx/issues/20 with an owner, a reason, a promotion threshold and a
falsification path.** No row says `open` and none says `implement`. Coordinates are this branch's;
the local receipts each WU recorded are in `.superpowers/sdd/2026-08-26-tui-phase2/`.

| Carrier item | Canonical owner | Terminal disposition | Evidence |
|---|---|---|---|
| cell diff / no-op frame skip | WU 1 | **implemented** | QA 13-14 registered and green; 22/22 mutants; dual-persona release-ready |
| resize / SIGWINCH / unfinished-tail rewrap | WU 2 | **implemented** | QA 15 registered and green; 40/40 mutants; dual-persona release-ready |
| slash picker | WU 3 | **implemented** | QA 16 registered and green; 31 killed + 1 earned equivalent; dual-persona release-ready |
| provider switching / model catalog / context meter | WU 4 | **implemented** | QA 18-19 registered and green on the fixture-discriminated pty; 51/51 mutants; `docs/parity.md`'s `/setup` and `/model` rows are the shipped contract |
| prompt history | WU 5 | **implemented** | QA 17 registered and green; 41 killed + 1 earned equivalent |
| alt-screen file-diff approval | WU 7A-7B / trinity fix | **implemented** | QA 20 registered and green; 32/32 mutants for 7A and 61/61 for 7B, every one executed; the surface, its bound and its one-write restore are `docs/parity.md`'s `full-screen TUI` row and `docs/architecture.md` §"One band at the bottom of the screen". The trinity fix round closed two holes in it: a short screen refused a large change on the **inline panel's** fit rule before the surface was chosen (each surface now answers its own fit question, and the plane's is `ApprovalScreen::presents_choices`), and `write_file` -- the largest change this product makes -- carried no before/after at all, so a whole-file replacement was reviewed as a digest. Scenario 20 now drives both content mutations in one session and the plane's enter/leave pair balances per question. Round 2 closed a third: the permission boundary flattened a change's line breaks into `\n` **without escaping the backslash**, so a file whose lines really end and a file that merely spells the breaks out rendered as the same payload -- the screen showed replacing a hundred-line file with one line of literal escapes as a change of nothing. The rendering is injective now and keeps the breaks, and the review is line-for-line. Round 3 closed the same defect one level down: every unnamed control collapsed to a single replacement character, so a file of `ESC` and a file of `BEL` -- and the two C1 scalars that are a `CSI` and an `OSC` -- were one payload. Each control is named by its **code point** now, at the boundary and on the screen, proven over the whole 65-scalar domain rather than the pairs somebody thought of |
| paste entity, 64-cap prefix scan, history renumber, transaction boundary | WU 6 | **implemented** | QA 21 registered and green; 58 killed + 3 earned equivalents |
| OSC 2 title | WU 1 | **implemented** | `term::MODE_SET` carries the title-stack push `CSI 22;2t` and every restore path the pop `CSI 23;2t` (`src/tui/term.rs:44,52,61`); the title itself is `OSC 2` with every control stripped from the model label; balanced-stack and stop/resume re-arm receipts are in `tests/tui.rs` and scenario 13 |
| kitty/tmux | WU 7C | **implemented** (push/pop and the tmux omission) + **re-deferred** (full CSI-u matrix) | `src/tui/term.rs:44` pushes `CSI > 1 u`, `:52`/`:61` pop `CSI < u`, and `MODE_SET_TMUX`/`RESTORE_TMUX` carry neither; pinned by `src/tui/term.rs`'s `tmux_never_gets_the_kitty_push_or_the_pop` and by `tests/tui.rs`'s `under_tmux_the_kitty_keyboard_push_is_omitted` on a real pty. The matrix's owner, threshold and falsification path are in `.prd/03-tui-port.md` item 17 and repeated below |
| tab visible/sent divergence | WU 6 | **implemented** | a pasted tab is kept, measured at four cells and painted as four spaces; QA 21 and the editor units |
| foreign OSC becomes composer text | WU 2 | **implemented** | OSC 10/11/52 and malformed/control handback receipts; the phantom-Escape fix was reviewed in WU 2's fix round |
| context meter / usage plumbing | WU 4 | **implemented** | numerator from a completed turn's `input_tokens`, denominator from the catalog's `max_context` matched by id or alias, and the segment dropped whole when either is absent (`src/tui/shell.rs`'s `context_meter`, `src/tui/hint.rs`); QA 19; `docs/parity.md`'s status-line row now states it rather than calling it unpopulated |
| activity-row colour | WU 1 | **re-deferred to issue #20** | see the re-deferral list below |
| give-up reason cannot print on refusing screen | WU 7C | **re-deferred to issue #20** | see the re-deferral list below |
| vt-grid grapheme / OSC / SGR oracle breadth | WU 1 and WU 7B | **implemented** for the emitted subset | 29 oracle falsification claims run before any scenario; the alternate plane, its saved cursor and per-frame snapshots landed in WU 7B; the emulator fails the run on any sequence it does not know, so the subset is a contract rather than a hope |
| Transcript fixed columns / wrap memoization | WU 2 / WU 1 | **implemented** (unfinished-tail rewrap) + **re-deferred** (memoization) | see the re-deferral list below |
| `xfx ask` grant recording unpinned | WU 7C | **implemented** | `tests/interactive.rs`'s `an_always_answered_at_an_ask_prompt_admits_the_same_change_on_the_next_resume`: a real pty types `a` at `xfx ask`'s own question, the log carries one `permission_grant_recorded` naming `edit_file` and the file's absolute path, the question named the id the test then resumes with, and a second `xfx ask --resume-id <id>` on a terminal that *could* ask makes the same change without asking and records no second grant |
| Ctrl-C notice wording | WU 7C | **implemented** | one shared sentence, bounded to the state that makes it true: `app::INTERRUPT_NOTICE` now says "another Ctrl-C **before it stops** exits", and each surface pins its own half beside its own state machine (`src/tui/gesture.rs`, `src/interactive.rs`) with the literal spelled out in `src/app.rs` |
| terminal-event full-channel third arm | WU 7C | **implemented** | `src/tui/worker.rs`'s `a_concluded_turn_reaches_the_ui_through_a_channel_that_had_no_room_for_it` drives the real `run_turn`: the turn concludes, its terminal event cannot enter the full channel, the drain frees a permit and the conclusion arrives well inside `DRAIN_DEADLINE` |
| parity `/model` shared-validation sentence | WU 4 / WU 7C / trinity fix | **implemented**, and the divergence it described is **closed** | 7C's row said the id-shape validation was shared and the catalog-membership refusal was not. The trinity fix round removed the second half rather than documenting it: the TUI's `/model <id>` is `ModelSelector::apply` on the runtime thread (`src/tui/worker.rs`'s `run_model`), its answer is one `UiEvent::ModelAnswered` carrying the model in force afterwards, and the band predicts nothing. Receipts: `worker`'s `a_model_a_loaded_catalog_does_not_publish_is_refused_and_the_session_keeps_its_own`, `shell`'s `the_model_the_band_shows_is_the_one_the_runtime_applied_and_never_a_prediction`, and a real pty in `tests/tui.rs` that reads the refusal off the band and then asserts the `model` field of the request the next turn sent |
| panic while the alternate plane is owned | WU 7C (7B residual §9(5)) | **implemented** | `fault::Fault::AlternatePanic` is injected after the entering frame is written, flushed and recorded and before any answer can be read, and `tests/tui.rs`'s `a_panic_while_the_other_plane_is_owned_gives_back_the_plane_and_the_terminal` drives it at a real process: `1049h` once, `1049l` once, the whole abnormal restore, the report *after* the leave, and `termios` byte-identical |

### Re-deferred to https://github.com/2lab-ai/xfx/issues/20

Four rows, each with the four things a deferral owes. This table is the source they were copied
from, and they are on the successor carrier now:
https://github.com/2lab-ai/xfx/issues/20#issuecomment-5464059512.

**The transfer happened after issue #19 closed, not before it**, which is a miss against the rule at
the foot of this file. It was repaired rather than papered over: the comment carries all four rows
with their thresholds and falsification paths, so no row lost its owner, but for the interval between
the closure and the comment the deferrals had no carrier. The rule stands as written for the next
drive; what changed is that this drive did not honour it.

| Row | Owner | Reason it is not in Phase 2 | Promotion threshold | Falsification path |
|---|---|---|---|---|
| full kitty CSI-u matrix | the input layer (`src/tui/input.rs`, `src/tui/term.rs`) | Phase 2 pushes one progressive-enhancement flag and decodes the xterm shapes; nothing in this port binds a key those shapes cannot express, and every `CSI ... u` is answered as a keystroke with no binding | a binding needs a key the xterm shapes cannot express, **or** a receipt from a terminal that speaks the protocol shows a key this session already claims to support arriving in the `u` form | one pty case: drive the affected keys against a terminal that speaks the protocol and read what the decoder made of them, rather than reasoning about what a terminal would send |
| activity-row colour | the band's painter (`src/tui/activity.rs`, `src/tui/theme.rs`) | no Phase-2 spec assigns that row a **semantic palette role**, and a colour chosen without one is a preference this port has no authority to invent; the row's contract today is its text and its blink, both asserted | `.prd/03-tui-port.md` names the role (what the colour *means*, not which colour it is) | a cell-attribute assertion, which the oracle already supports (it remembers SGR per cell, and scenario 12 already judges the palette that way): extend scenario 9 to read the activity row's own cells and require the named role's attributes rather than the default, so a build that painted the row plain fails |
| give-up reason on a refusing screen | the event loop's failure budget (`src/tui/event_loop.rs`'s `FrameFailures`) | when the screen refuses every frame for the budget, the session ends with that error -- and the only surface Phase 2 has for saying *why* is the screen that is refusing. No independent delivery channel exists, and 7C deliberately did not invent one for the sake of closing a row | a non-screen diagnostic sink is designed and owned -- a file under the profile, or a report written after the terminal is restored -- with its own contract about what it may contain | fault injection that refuses every frame, then an assertion that the reason reached that sink; today the same injection can only assert that the process exits nonzero with the terminal restored |
| transcript wrap memoization | the transcript (`src/tui/transcript.rs`, `src/tui/wrap.rs`) | the measured cost has not been shown to matter: a frame is a difference, an idle band writes nothing, and the unfinished tail is bounded at 256 rows | a benchmark exceeds 8 ms/frame at 80x24 or 32 ms/frame at 300x200 | run that benchmark on the shipped painter; below the threshold the memoization is work with no receipt, and its cache would be a second source of truth about what a row wraps to |

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

- Every re-deferred row is copied with its threshold and final evidence link to successor carrier https://github.com/2lab-ai/xfx/issues/20 before issue #19 closes. This drive did it in the wrong order — issue #19 closed 2026-08-29 and the four rows reached https://github.com/2lab-ai/xfx/issues/20#issuecomment-5464059512 afterwards — so the ordering is stated here as a rule that was repaired, not as a rule that was met.
