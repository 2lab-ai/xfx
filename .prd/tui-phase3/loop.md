# TUI Phase 3 — convergence loop

Status: in-progress
SSOT: ./ssot.md
Repo: 2lab-ai/xfx
Branch: plan/tui-phase3
Base: edcc2f5515870f50ac3b9ffc71201a52a0d1f746
Channel: this conversation
Started: 2026-09-10

Coordinates and receipts are observations, not authority. Revalidate before changing the associated code. This driver owns Phase 3 only.

## Plan — requirements round

- Compare two independent spec extractions; verify conflicts directly against the primary docs.
- Recover exact upstream behavior for keys, question triggering, amendment drafts and retained frames, distinguishing the port pin from the later research pin.
- Map existing implementation seams and produce independently testable vertical slices. Do not start product edits before the behavioral contract and dependency order are settled.
- Add layout convergence to the Phase 3 QA universe; the existing paragraph omits it.
- Run conditional inherited measurements rather than using their old deferral as proof they need no work.

## Gap matrix

| ID | Source | Universe Item | Observable Acceptance | AS-IS Evidence | Status | Gap | Dispatch | Verification Evidence | Last Measured |
|---|---|---|---|---|---|---|---|---|---|
| P3-EDIT | ladder 18 | Undo/redo and kill | User edits, undoes, redoes and yanks with caps and paste payload preserved | Implemented, locally verified and externally approved; shipment belongs to P3-SHIP | met | None in approved editing contract | local qualification | Controller full gate; release scenario 22 text/caret/payload proofs; refused-yank RED | 2026-09-10 |
| P3-QUESTION | ladder 19 | Ordinals and Other | Real tool request displays; chosen/freeform answer reaches next model request | ladder target; question tool deferred in parity | to work | Trigger, UI, result path | upstream + planning | None | 2026-09-10 |
| P3-APPROVAL | ladder 20 | Readiness and amendment | Early affirmative refused; same-request committed controls unlock; amended decision reaches runtime | parity says readiness/amendment absent | to work | Readiness identity and draft semantics | upstream + planning | None | 2026-09-10 |
| P3-COMMIT | ladder 21 | Self-check and recovery | Injected partial write/mismatch fails commit, then bounded recovery restores intended grid | ladder target | to work | Decoder/checker, retention and recovery | upstream + planning | None | 2026-09-10 |
| P3-LAYOUT | ladder 22 | Convergence | Coupled content/footer layout reaches consistent bounded geometry across sizes | spec says one-pass approximation | to work | Solver plus missing QA scenario | planning | None | 2026-09-10 |
| P3-THEME | ladder 23 | Live theme | Theme notification changes transcript and queued text attributes without losing content | parity says no mid-session changes | to work | Monitor and re-tint path | upstream + planning | None | 2026-09-10 |
| P3-KEYS | successor | CSI-u | Protocol-speaking input produces supported actions; tmux branch remains safe | full matrix explicitly deferred | to work | Promotion measurement and affected decoder support | planning | None | 2026-09-10 |
| P3-ACTIVITY | successor | Activity colour | Named role appears in activity cells in both themes | successor says no semantic role assigned | to work | Role definition and attribute assertion | planning | None | 2026-09-10 |
| P3-DIAGNOSTIC | successor | Give-up diagnostic | Refusing-screen fault leaves a bounded diagnostic through independent sink | successor says no sink | to work | Sink ownership and containment | planning | None | 2026-09-10 |
| P3-WRAP | successor | Wrap budget | Benchmark both named dimensions; cache only if thresholds crossed | no recorded threshold crossing | to work | Fresh measurement | planning | None | 2026-09-10 |
| P3-QA | QA acceptance | External PTY QA | All old/new release scenarios produce positive discriminators, grids and termios evidence | Baseline cargo pass, no Phase3 scenarios | partial | New scenarios and native qualification | pending | Baseline only | 2026-09-10 |
| P3-DOCS | goal | Implemented truth | Spec/parity match shipped surfaces | Phase3 correctly marked target | partial | Final current-tense reconciliation | pending | Current spec read | 2026-09-10 |
| P3-SHIP | goal | Preview delivery | Reviewed main, preview/tap/install and real-profile response receipt; carrier closed | public issue20 open, main at base | to work | Entire Phase3 delivery chain | pending | Public issue API | 2026-09-10 |

Status vocabulary: met; partial (treated as to work); to work; gated; out of scope (requires recorded justification and explicit user approval).
Accounting: universe 13 + appended 0 = met 1 + partial 2 + to work 10 + gated 0 + out of scope 0.
Evidence types: screenshot, route, API request/response, command output, log excerpt, commit, PR, artifact path. Bare reports are not verification.

## Baseline

- Local and fetched origin/main both edcc2f5515870f50ac3b9ffc71201a52a0d1f746; clean before worktree creation.
- Default all-targets test output: 1797 passed across the test binaries, zero failures, one ignored library test.
- Fresh fault TUI: `79 passed; 0 failed`.
- Fresh fault lib: `1163 passed; 0 failed; 1 ignored`.
- fmt, both clippy modes and all four check scripts exited 0.
- Gate commands remain those in `.prd/tui-phase2/loop.md` §Gate contract; release qualification additionally builds both release variants and runs CLI/TUI smoke. Counts above are observations, not expected literals.

## Architecture rulings — 2026-09-10

- Commit self-check is a shipped runtime guarantee, not a debug-only test facility. A bounded decoder of the actual emitted subset must be exercised in the normal release path; its cost is measured rather than assumed. The sequence inventory must include title OSC, SGR, primary and alternate paints, plane restoration and document movement. Checking ordinary band diffs alone cannot satisfy the spec's every-byte shadow rule. Preflight byte validation and post-write shadow adoption are distinct steps; partial-write failure must not adopt the intended target.
- `Commit::NoChange` can preserve an already proven readiness receipt only for the same request and layout identity. It cannot grant first-time readiness merely because a repaint wrote zero bytes.
- Direct source check: `Transcript` retains only its unfinished tail and painted-row count (`src/tui/transcript.rs`, struct at base). Completed rows belong to native scrollback. Theme re-tint ownership remains an unresolved source tension; do not remove transcript acceptance before comparing the upstream behavior.
- Direct source check: `Band::commit` propagates write/flush errors before `landed`, without explicitly marking partial-write damage (`src/tui/frame.rs`, commit at base). Partial-write recovery needs a deterministic failing writer test and release-path invalidation; do not assume a successful retry based only on an unchanged shadow.
- Keep question and readiness state in focused modules rather than adding substantial new logic to the already large shell. Wire the existing UI/worker boundary; do not create an umbrella Phase-3 framework.
- Self-check proves agreement between emitted bytes, the decoder model and the intended state; it is not physical acknowledgement from the terminal. A mismatch diagnoses local model/emitter disagreement, not evidence that the terminal behaved differently. Performance pressure cannot silently turn mandatory checks into sampling. Normal UI-owned writes and async-signal emergency restores need separate ownership contracts; emergency handlers must not allocate or lock to run a grid check.
- Applier implementation is preceded by a bounded emitter inventory, including non-Band output. For each path identify sequence effects and available intended state before deciding validation/adoption wiring. Do not declare a path preflight-only merely because its target model has not yet been implemented.
- Inventory cross-check identifies `/clear` as a separate emitter (`Shell::CLEAR_SCREEN`: CUP home, ED2 and ED3), not a `ScreenFrame` write. Startup reservation also moves to the bottom and emits newlines; background and cursor queries share one write. The applier must model these effects or explicitly delimit the shadow's initialization/reinitialization around them. Preserve the existing user-invoked clear behavior; do not introduce automatic scrollback erasure as a recovery shortcut.

## Verified upstream contract excerpts

Source revision for these excerpts: `580a0c5da9386317251968c09c1cee69e763487a`. These are pinned-source facts, not a claim about latest upstream.

- `src/core/input/edit_history.zig` prepare/commit: retained delta bytes are removed length plus inserted length. A single delta over 1 MiB becomes a boundary that clears both undo and redo. A recorded edit clears redo and evicts oldest undo entries until both byte and entry caps hold. The xfx entity-payload accounting must also be bounded; do not adopt whole-draft copies as a substitute for delta history.
- `src/ui/footer/approval_readiness.zig`: settlement requires idle resize and nonzero dimensions. A screen commit is usable only for the same request ID and dimensions, with file identity and all decision controls visible, plus changed content or its notice visible now or previously observed for that screen state. A successful write by itself is insufficient.
- Frame retention source names retained transcript body reuse under stable source/layout identity and safe movement; it must not be casually redefined as retrying the previous byte buffer. Exact adaptation is pending the upstream report.
- `src/tools/agent/ask_user_question.zig` call/execute path invokes an injected batch requester, encodes returned answers as ordered question/answer JSON, or returns the cancellation sentinel. Parsed input requires 1–4 questions, 2–6 options per question, nonempty trimmed question/labels and case-insensitively unique sanitized labels. This is a real tool path, not an approval variant.
- `question_prompt.zig::selectOrdinal` directly submits a model-provided option; selecting synthetic Other only opens editing and requests a redraw. Do not implement every numeric choice as selection-plus-Enter: that would diverge from the verified ordinal behavior. Arrow/Tab selection and Enter remain a separate path.
- At research revision `ef1d0d0c6a1a87a621fc54d23f86ffec51755779`, `approval_decision.zig::apply` makes Tab enter an eligible allow/deny amendment draft, Enter submit the selected decision, and numeric keys insert draft text while amending rather than submit. `materializeResponse` carries optional nonempty feedback separately from the decision. Downstream verified at port pin `580a0c5d`: `tool_admission.zig:2039-2050` retains the existing file authorization on an allowed decision and copies feedback separately; `orchestrator.zig:6598-6618` appends denial result then feedback, and `:7431-7458` appends committed-file result then feedback. `appendPermissionFeedbackAfterToolResult` (`:1950-1970`) also emits the user-feedback event. Thus feedback is subsequent user context, not executable edits to tool arguments or permission authority. Revision labels remain explicit; this is not an unverified claim that both revisions are identical.

## Upstream extraction status — 2026-09-10

The broad extraction ended with question and amendment semantics pending. It established an in-memory themed render path and unconditional retained-body validation, but those do not establish the full live theme-update side effects or the separate wire-frame self-check call path. In particular, the existing research points to `applyThemeUpdate` and terminal reset, so the claim that theme updates never rewrite scrollback is not yet accepted. Bounded follow-ups now own question trigger/answers, live theme-update-to-output, and approval amendment input/result separately. Existing downloaded evidence is reused; no whole-file reread is needed.

## Next implementation slice

T0's delivered-only alternate cache fix is committed at `98a7324`. P3-EDIT is implemented but uncommitted, with its approved plan in `docs/superpowers/plans/2026-09-10-tui-phase3-edit-history.md`; product review and a refused-yank allocator correction are in progress. The remaining write-recovery plan is design-only: CAN resynchronization and prefix-aware plane semantics are unverified, and its external review was stopped without a verdict. No implementation approval is inferred from that stop. Other Phase 3 rows remain open.

Pinned-source editing facts now settled: `escape_parser.zig:122-123` maps Super+Z to undo and Super+Shift+Z to redo; runtime tests pin `ESC[122;10u` and `ESC[27;10;122~` as redo. `kill_ring.zig:86-116` replaces the kill slot without coalescing. `input_completion_runtime.zig:339-356` calls `historyBoundary` only on a changed recall. These facts override the earlier speculative recall-as-delta default. They promote the specific supported editing encodings, not a claim that the full CSI-u matrix is implemented.

## Infrastructure boundary

`gh` fails certificate validation with `x509: OSStatus -26276`, also with an explicit system PEM bundle. TLS-verified curl public API and git fetch work. Public issue body and its single comment were read through curl. Do not disable TLS verification or print credentials. Local work can proceed; authenticated forge writes need a working trusted client before integration.

## Round log

- R1 2026-09-10: goal activated; clean baseline and remote base verified; dedicated worktree created. Independent doc cross-check identifies six unconditional ladder items and four conditional successor obligations. Primary QA paragraph confirms the layout scenario omission. Product code unchanged. Upstream semantics and architecture mapping remain in flight.

- R2 2026-09-10: T0 alternate cache adoption implemented in `src/tui/frame.rs`, still uncommitted and under external review. Two deterministic defect tests failed before the product edit; four added tests now cover undelivered build, refused repaint retry, delivered no-op and invalidated no-op cache resurrection. Controller full gate exited 0: default suite 1801 passed, fault TUI 79 passed, fault lib 1167 passed/1 ignored, both clippy modes and four contract scripts, both release builds, CLI smoke 46 checks/0 failures and TUI smoke 511 checks/0 failures. This proves existing release scenarios remain green; the specific refusal fix has deterministic writer evidence, not a new real-terminal fault scenario. P3-COMMIT remains open for partial-write recovery, release self-check and retained-body behavior. Source and test counts are observations at the dirty T0 tree, not a shipped commit receipt.

- R2 follow-up 2026-09-10: T0 committed as `98a7324` after dual-persona external release-ready verdict with no MUST-FIX. Added a fifth test for successful changed-repaint adoption; removing that adoption killed only the new test. Controller reran fmt, frame tests (66 passed), both clippies, fault PTY (79 passed) and four contract scripts after restoration. No push or deployment yet. Partial-write plan review was stopped after prolonged non-response: its gate is unmet, T1–T6 remain design-only. Delta-history plan is being corrected for verified redo/kill/recall semantics and executable tests; plan-author claims of implementation clearance are not authority.

- R3 2026-09-10: P3-EDIT plan review REJECT, six must-fix findings. Verified draft-disposal funnel and slash-completion replacement in current shell: existing history offsets would become stale after submit/clear, and completion cannot be represented by its insertion half alone. Plan also wrongly routes yank through a non-recording branch, lacks a Delta constructor, contradicts retained-byte transfer accounting and tests refused edits without exercising the recording funnel. Same plan writer is correcting these and preserving existing paste-boundary tests. No P3-EDIT product changes are authorized yet. Rust disjoint-field borrowing is permitted; do not justify a risky owned pop/stash protocol merely by saying Shell owns both fields.

- R3 scoped re-review: original disposal/completion and yank-undoability fixes accepted, but revision 3 remains rejected for four concrete defects: `history` field collides with prompt history; yank bypasses `amended` and leaves recall navigation active; Delta accessors are incomplete; clearing redo does not subtract its retained bytes. Controller verified existing `history: History` and `amended -> history.leave -> edited` wiring. Same author is making only these corrections and relabeling completion's boundary as an xfx design choice, not the cited upstream recall rule. No new product edit has started.

- R3 final plan review: APPROVE with a mandatory direct-edit-route correction before shell integration. Controller verified the five direct `edited()` sites and wrote their `edited(None)` preservation into §S2; recall must not call `history.leave()`. `Editor::apply -> Option<Delta>` is explicit. P3-EDIT implementation is now dispatched to one fresh coding worker with RED-first tests, existing paste-proof migration, release PTY smoke and no stage/commit/push authority. No controller build or mutation runs while that worker owns the tree. All other product rows remain open.

- R4 2026-09-10: P3-EDIT worker handed back uncommitted implementation. Controller found the missing Redo PTY acceptance; the same worker added both pinned CSI encodings, separate undo/redo round trips and recorded-edit invalidation. Controller read the actual scenario-22 grids: both redos show `> one two` at row 23 with cursor column 10; after a new edit, redo leaves `> one !?` with cursor column 9. Worker smoke log reports 534 checks/0 failures, not yet a controller rerun. Product review rejected three items. Controller verified refused yank mutates the session ID allocator before the budget refusal (`shell.rs::yank_killed`, `entity.rs::renumber_recalled`) and dispatched a RED-first local-counter/commit-on-insertion correction. Reviewer is reconsidering two budget findings against explicit plan S3/S4: oversized paste is a history boundary; oversized kill empties the separately capped slot. Inserted Arc payloads cannot be charged as metadata alone because undo may leave redo as the payload's sole owner. No budget policy change, commit or shipment is inferred while review remains open.

- R4 verification: refused-yank RED fails on the first allocator assertion (`left: 2`, `right: 1`); fixed code renumbers against a local counter and commits it only after insertion succeeds. Scoped external review now APPROVE for spec and code quality, MUST-FIX none; original budget findings withdrawn. Controller full serial gate exited 0: default 1836 passed/0 failed/1 ignored across 11 binaries, fault TUI 80/0, fault lib 1201/0/1 ignored; fmt, both clippies, four contract scripts, both fresh release builds, CLI 46 checks/0 failures and TUI 534 checks/0 failures. Logs and full-grid captures: session scratchpad `xfx-p3-edit/controller/`. Worker disclosed a temporary shell-only stash/pop outside its assignment; controller observed empty stash list, empty staged diff and restored tested source. No commit or shipment yet. Mutation report corrected by worker: 25 definitions, 24 applied, 21 killed, 3 survivors, with M18 not applied; raw original sweep logs were not persisted. Controller does not certify those counts as its own observation. A bounded M18 measurement and independent survivor adjudication remain before recording mutation coverage.

- R4 mutation closure: M18 now has a raw failure log (`xfx-p3-edit/m18.log`): changing the uniquely anchored modifier-length bound makes `122;010u` decode as Redo instead of Ignore; existing test exits 101. Before/after restoration SHA-256 is `b03d61d7fad7a8d91850c21f5eab495369b575749272f9f6187eee412e52125f`, independently matched by the controller, whose restored input tests pass. Worker accounting is now 25 applied, 22 killed, 3 survived; original 21 kills remain worker transcript evidence rather than independently rerun mutations. Independent reviewer adjudicated M2/M13/M20 as equivalent on reachable production paths: oversized delta is still entirely evicted, delete starts are already entity-normalized, and `edited(None)` reduces to `moved()`. Controller checked the named record/delete/edited paths. No mandatory test gap remains. Plan summary now states the paste-budget qualification and explains why an oversized kill clears old slot contents. Optional panic-atomicity note remains non-blocking: a UI panic restores and unwinds rather than continuing to use the editor.

## Dispatch contract

One work unit per writer; provide requirement IDs, exact behavioral contract, verified file/line evidence, owned files, gate commands, report location and prohibitions. No concurrent mutation sweep and controller stress in the same tree. Reviewers do not edit. Regenerate evidence for changed surfaces; keep unfinished rows open.
